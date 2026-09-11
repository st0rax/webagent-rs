//! Fail-closed Abschluss von Taskboard-Claims.
//!
//! Ein Run darf einen Claim nur nach explizitem Belegabschluss auf `done`
//! setzen. Die Operation schreibt atomar und verweigert fremde oder bereits
//! abgeschlossene Claims.
//!
//! Nehmen (claimen) darf ein Run einen Task nur, wenn er im Taskboard als
//! `free` steht und nicht per `claim_lock` gesperrt ist. Bereits geclaimte,
//! abgeschlossene oder gesperrte Tasks werden nie erneut übernommen — so ist
//! ein versehentlicher Doppel-Claim (z.B. durch einen parallelen Dev) nicht
//! möglich.

use serde_json::Value;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::config::data_dir;

/// Pfad zur Taskboard-Sperre. Deterministisch aus dem Taskboard-Pfad, aber
/// NICHT als Datei daneben (das wuerde den Repo-Baum verschmutzen), sondern
/// unter `data_dir()/locks`. Zwei Prozesse mit demselben Board sperren daher
/// dieselbe Datei — egal von welchem Verzeichnis aus sie laufen (T-806).
fn taskboard_lock_path(taskboard: &Path) -> PathBuf {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    taskboard.to_string_lossy().hash(&mut hasher);
    data_dir()
        .join("locks")
        .join(format!("taskboard-{:016x}.lock", hasher.finish()))
}

/// Eindeutige Temp-Datei: pid + Nanosekunden-Stempel — NIE ein gemeinsam
/// genutzter Tempname. Parallele Runs koennen sich nicht gegenseitig die
/// Temporärdatei wegrennen (T-806/T-808: "keine gemeinsam verwendeten
/// Tempnamen").
fn unique_tmp_path(taskboard: &Path, op: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    unique_tmp_path_with(taskboard, op, std::process::id(), stamp)
}

/// Testbarer Kern der eindeutigen Temp-Namen (pid + Stempel injizierbar).
fn unique_tmp_path_with(board: &Path, op: &str, pid: u32, stamp: u32) -> PathBuf {
    board.with_extension(format!("{op}.{pid}.{stamp:x}.tmp"))
}

/// Atomarer Replace mit flush/sync VOR dem Rename. Unter Windows wird der
/// Rename-Fallback am Ende best effort nachgelegt; ein Fehlschlag des Rename
/// laesst das Original unangetastet.
fn atomic_write(board: &Path, body: &str, op: &str) -> Result<(), String> {
    let tmp = unique_tmp_path(board, op);
    let mut file = fs::File::create(&tmp)
        .map_err(|e| format!("Taskboard temporär schreiben ({tmp:?}): {e}"))?;
    file.write_all(body.as_bytes())
        .and_then(|_| file.sync_all())
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Taskboard nicht dauerhaft geschrieben: {e}")
        })?;
    drop(file);
    fs::rename(&tmp, board).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Taskboard atomar ersetzen: {e}")
    })
}

/// Liest das Board unter ausschliesslicher Sperre, laesst `f` den Claim/
/// Abschluss pruefen und veraendern und schreibt bei Erfolg atomar zurueck.
/// Die Pruefung erfolgt ERST unter der Sperre (fail-closed): eine parallele
/// Aenderung eines zweiten Prozesses ist sichtbar und wird nie ueberschrieben.
fn with_board_lock<R>(
    taskboard: &Path,
    op: &str,
    f: impl FnOnce(&mut Value) -> Result<R, String>,
) -> Result<R, String> {
    let lock_path = taskboard_lock_path(taskboard);
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Taskboard-Sperrverzeichnis erstellen: {e}"))?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("Taskboard-Sperre ({op}) öffnen: {e}"))?;
    lock.lock_exclusive()
        .map_err(|e| format!("Taskboard-Sperre ({op}) belegt: {e}"))?;

    let raw = fs::read_to_string(taskboard).map_err(|e| format!("Taskboard lesen: {e}"))?;
    let mut root: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
    let result = f(&mut root)?;
    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Taskboard schreiben: {e}"))?
        + "\n";
    atomic_write(taskboard, &formatted, op)?;
    Ok(result)
}

/// Ein Task offiziell claimen (fail-closed).
///
/// Erlaubt nur, wenn der Task existiert, `status == "free"` ist und kein
/// `claim_lock` trägt. Schlägt fehl, sobald der Task bereits geclaimt, done
/// oder gesperrt ist. Das gesamte Lesen-Pruefen-Schreiben steht unter einem
/// prozessuebergreifenden Lock; parallele Doppel-Claims sind damit
/// ausgeschlossen, nicht nur erkannt.
pub fn acquire_claim(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
) -> Result<(), String> {
    if task_id.trim().is_empty() || owner.trim().is_empty() || branch.trim().is_empty() {
        return Err("Task-ID, owner und branch sind Pflicht".into());
    }
    with_board_lock(taskboard, "acquire", |root| {
        let tasks = root
            .get_mut("tasks")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())?;
        let task = tasks
            .iter_mut()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
            .ok_or_else(|| format!("Unbekannte Task-ID: {task_id}"))?;
        if task.get("claim_lock").and_then(Value::as_bool) == Some(true) {
            return Err(format!("Task {task_id} ist per claim_lock gesperrt"));
        }
        match task.get("status").and_then(Value::as_str) {
            Some("free") => {}
            Some("claimed") => {
                let who = task
                    .get("owner")
                    .and_then(Value::as_str)
                    .unwrap_or("unbekannter Owner");
                return Err(format!(
                    "Task {task_id} ist bereits von {who} geclaimt — kein Doppel-Claim"
                ));
            }
            Some("done") => {
                return Err(format!("Task {task_id} ist bereits abgeschlossen"));
            }
            other => {
                return Err(format!(
                    "Task {task_id} hat unerlaubten Status {:?}",
                    other.unwrap_or("fehlt")
                ))
            }
        }
        task["status"] = Value::String("claimed".into());
        task["owner"] = Value::String(owner.into());
        task["branch"] = Value::String(branch.into());
        task["claimed_at"] = Value::String(crate::now_rfc3339());
        Ok(())
    })
}

pub fn complete_claim(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
    proof_path: &Path,
) -> Result<(), String> {
    if task_id.trim().is_empty() || owner.trim().is_empty() || branch.trim().is_empty() {
        return Err("Task-ID, owner und branch sind Pflicht".into());
    }
    if !proof_path.is_file() {
        return Err(format!(
            "Konkrete Belegdatei fehlt (Verzeichnisse sind unzulässig): {}",
            proof_path.display()
        ));
    }
    with_board_lock(taskboard, "complete", |root| {
        let tasks = root
            .get_mut("tasks")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())?;
        let task = tasks
            .iter_mut()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
            .ok_or_else(|| format!("Unbekannte Task-ID: {task_id}"))?;
        if task.get("status").and_then(Value::as_str) != Some("claimed") {
            return Err("Nur ein claimed-Task darf abgeschlossen werden".into());
        }
        if task.get("owner").and_then(Value::as_str) != Some(owner)
            || task.get("branch").and_then(Value::as_str) != Some(branch)
        {
            return Err("Owner oder Branch stimmt nicht mit dem Claim überein".into());
        }
        let proof = proof_path.to_string_lossy().replace('\\', "/");
        task["status"] = Value::String("done".into());
        task["done_at"] = Value::String(crate::now_rfc3339());
        task["proof_path"] = Value::String(proof);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_is_fail_closed_for_wrong_owner() {
        let d = std::env::temp_dir().join(format!("webagent-taskboard-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        let proof = d.join("proof");
        fs::write(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"a","branch":"b"}]}"#,
        )
        .unwrap();
        fs::write(&proof, "proof").unwrap();
        assert!(complete_claim(&board, "T-1", "x", "b", &proof).is_err());
    }

    #[test]
    fn completion_atomically_marks_matching_claim_done() {
        let d = std::env::temp_dir().join(format!("webagent-taskboard-ok-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        let proof = d.join("proof");
        fs::write(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"a","branch":"b"}]}"#,
        )
        .unwrap();
        fs::write(&proof, "proof").unwrap();
        complete_claim(&board, "T-1", "a", "b", &proof).unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        let task = &value["tasks"][0];
        assert_eq!(task["status"], "done");
        assert_eq!(
            task["proof_path"],
            proof.to_string_lossy().replace('\\', "/")
        );
        assert!(task["done_at"].as_str().is_some_and(|s| !s.is_empty()));
    }

    fn board_with(board: &std::path::Path, json: &str) {
        fs::write(board, json).unwrap();
    }

    #[test]
    fn acquire_marks_free_task_as_claimed() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-ok-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(&board, r#"{"tasks":[{"id":"T-1","status":"free"}]}"#);
        acquire_claim(&board, "T-1", "dev", "feature/x").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        let task = &value["tasks"][0];
        assert_eq!(task["status"], "claimed");
        assert_eq!(task["owner"], "dev");
        assert_eq!(task["branch"], "feature/x");
        assert!(task["claimed_at"].as_str().is_some_and(|s| !s.is_empty()));
    }

    #[test]
    fn acquire_is_fail_closed_for_already_claimed() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-dup-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"a","branch":"b"}]}"#,
        );
        assert!(acquire_claim(&board, "T-1", "dev", "feature/x").is_err());
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["owner"], "a");
    }

    #[test]
    fn acquire_is_fail_closed_for_done() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-done-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(&board, r#"{"tasks":[{"id":"T-1","status":"done"}]}"#);
        assert!(acquire_claim(&board, "T-1", "dev", "feature/x").is_err());
    }

    #[test]
    fn acquire_is_fail_closed_for_locked() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-lock-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"free","claim_lock":true}]}"#,
        );
        assert!(acquire_claim(&board, "T-1", "dev", "feature/x").is_err());
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "free");
    }

    #[test]
    fn acquire_is_fail_closed_for_unknown_task() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-unknown-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(&board, r#"{"tasks":[{"id":"T-1","status":"free"}]}"#);
        assert!(acquire_claim(&board, "T-404", "dev", "feature/x").is_err());
    }

    #[test]
    fn sperre_ist_aus_pfad_deterministisch_und_pro_boards_getrennt() {
        let a = Path::new("docs/TASKBOARD.json");
        let b = Path::new("docs/ANDERES.json");
        assert_eq!(taskboard_lock_path(a), taskboard_lock_path(a), "gleiches Board -> gleiche Sperre");
        assert_ne!(taskboard_lock_path(a), taskboard_lock_path(b), "andere Boards -> andere Sperren");
        assert!(taskboard_lock_path(a).extension().unwrap().to_string_lossy().ends_with("lock"));
    }

    #[test]
    fn temp_namen_sind_nie_gemeinsam_genutzt() {
        let a = unique_tmp_path_with(Path::new("B.json"), "acquire", 1, 7);
        let b = unique_tmp_path_with(Path::new("B.json"), "acquire", 1, 8);
        let c = unique_tmp_path_with(Path::new("B.json"), "acquire", 2, 7);
        assert_ne!(a, b, "Stempel muss trennen");
        assert_ne!(a, c, "pid muss trennen");
        assert!(a.to_string_lossy().ends_with(".tmp"));
        assert_ne!(
            unique_tmp_path_with(Path::new("B.json"), "complete", 1, 7),
            a,
            "verschiedene Operationen duerfen nicht kollidieren"
        );
    }

    #[test]
    fn zwei_parallele_runs_claimen_fail_closed_nur_einmal() {
        let d = std::env::temp_dir().join(format!("webagent-parallel-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(&board, r#"{"tasks":[{"id":"T-1","status":"free"}]}"#);
        let b1 = board.clone();
        let b2 = board.clone();
        let t1 = std::thread::spawn(move || acquire_claim(&b1, "T-1", "dev", "feature/x").is_ok());
        let t2 = std::thread::spawn(move || acquire_claim(&b2, "T-1", "dev", "feature/x").is_ok());
        let ok1 = t1.join().unwrap();
        let ok2 = t2.join().unwrap();
        assert!(
            ok1 ^ ok2,
            "genau EIN paralller Claim darf gewinnen (ok1={ok1}, ok2={ok2})"
        );
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "claimed");
    }
}
