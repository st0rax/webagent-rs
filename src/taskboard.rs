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
    let mut root: Value = serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
    let result = f(&mut root)?;
    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Taskboard schreiben: {e}"))?
        + "\n";
    atomic_write(taskboard, &formatted, op)?;
    Ok(result)
}

/// Ein Task offiziell claimen (fail-closed).
///
/// Erlaubt nur, wenn der Task existiert, `status == "free"` ist, kein
/// `claim_lock` trägt und jeder Vorgänger aus `depends_on` den Status `done`
/// hat. Fehlende `depends_on` sind v1-Altbestand und bleiben parallel
/// claimbar. Schlägt fehl, sobald der Task bereits geclaimt, done, gesperrt
/// oder blockiert ist. Das gesamte Lesen-Pruefen-Schreiben steht unter einem
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
        // Erst lesend pruefen: Der Dependency-Check darf den Task nicht
        // teilweise uebernehmen, wenn ein Vorgaenger noch nicht done ist.
        let tasks = root
            .get("tasks")
            .and_then(Value::as_array)
            .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())?;
        let task = tasks
            .iter()
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

        // T-978ff (Schema v2): Ein Task ist BLOCKED, bis alle depends_on
        // done sind. Fehlende depends_on sind Altbestand (v1) und erlauben
        // den historischen Parallelclaim.
        let mut predecessors = Vec::new();
        if let Some(deps) = task.get("depends_on") {
            let deps = deps
                .as_array()
                .ok_or_else(|| format!("Task {task_id}: depends_on ist keine Liste"))?;
            for dep in deps {
                let id = dep
                    .as_str()
                    .ok_or_else(|| format!("Task {task_id}: depends_on enthält keine Task-ID"))?;
                if id == task_id {
                    return Err(format!("Task {task_id} hängt von sich selbst ab"));
                }
                predecessors.push(id.to_string());
            }
        }
        for id in predecessors {
            let predecessor = tasks
                .iter()
                .find(|t| t.get("id").and_then(Value::as_str) == Some(id.as_str()))
                .ok_or_else(|| format!("Task {task_id}: unbekannter Vorgänger {id}"))?;
            let status = predecessor
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("fehlt");
            if status != "done" {
                return Err(format!(
                    "Task {task_id} ist blockiert: Vorgänger {id} ist {status}"
                ));
            }
        }
        let task = root
            .get_mut("tasks")
            .and_then(Value::as_array_mut)
            .and_then(|tasks| {
                tasks
                    .iter_mut()
                    .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
            })
            .ok_or_else(|| format!("Task {task_id} verschwand während des Claims"))?;
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

/// Verifizierter Taskabschluss (T-806): das Brain-Manifest ist ein ANTRAG,
/// kein Beweis. `receipt` wird gegen den gelaufenen Run (Run-ID, Brain,
/// Commit, alle Pflichtkriterien bestanden) geprüft; nur ein belastbarer
/// Antrag führt zu `done`. Eine Ablehnung wird im Run-Eventlog festgehalten.
#[allow(clippy::too_many_arguments)]
pub fn complete_claim_verified(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
    proof_path: &Path,
    receipt: &crate::acceptance::CompletionReceipt,
    expected: &crate::acceptance::ExpectedCompletion,
    run_store: &crate::run_store::RunStore,
) -> Result<(), String> {
    match crate::acceptance::verify_completion(receipt, expected) {
        Ok(()) => complete_claim(taskboard, task_id, owner, branch, proof_path),
        Err(reason) => {
            let _ = run_store.record_rejection(&expected.run_id, task_id, &reason);
            Err(format!("Task-Abnahme verweigert: {reason}"))
        }
    }
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
        let d =
            std::env::temp_dir().join(format!("webagent-acquire-unknown-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(&board, r#"{"tasks":[{"id":"T-1","status":"free"}]}"#);
        assert!(acquire_claim(&board, "T-404", "dev", "feature/x").is_err());
    }

    #[test]
    fn acquire_respects_depends_on() {
        let d = std::env::temp_dir().join(format!("webagent-acquire-deps-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"free"},{"id":"T-2","status":"free","depends_on":["T-1"]}]}"#,
        );
        let err = acquire_claim(&board, "T-2", "dev", "feature/T-2").unwrap_err();
        assert!(err.contains("blockiert"), "unexpected: {err}");
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][1]["status"], "free");

        let proof = d.join("proof");
        fs::write(&proof, "proof").unwrap();
        board_with(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"a","branch":"feature/T-1"},{"id":"T-2","status":"free","depends_on":["T-1"]}]}"#,
        );
        complete_claim(&board, "T-1", "a", "feature/T-1", &proof).unwrap();
        acquire_claim(&board, "T-2", "dev", "feature/T-2").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][1]["status"], "claimed");
        assert_eq!(value["tasks"][1]["owner"], "dev");
    }

    #[test]
    fn acquire_rejects_unknown_or_self_predecessor() {
        let d =
            std::env::temp_dir().join(format!("webagent-acquire-baddep-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        board_with(
            &board,
            r#"{"tasks":[{"id":"T-2","status":"free","depends_on":["T-404"]}]}"#,
        );
        let err = acquire_claim(&board, "T-2", "dev", "feature/T-2").unwrap_err();
        assert!(err.contains("unbekannter Vorgänger"), "unexpected: {err}");

        board_with(
            &board,
            r#"{"tasks":[{"id":"T-2","status":"free","depends_on":["T-2"]}]}"#,
        );
        let err = acquire_claim(&board, "T-2", "dev", "feature/T-2").unwrap_err();
        assert!(err.contains("sich selbst"), "unexpected: {err}");
    }

    #[test]
    fn sperre_ist_aus_pfad_deterministisch_und_pro_boards_getrennt() {
        let a = Path::new("docs/TASKBOARD.json");
        let b = Path::new("docs/ANDERES.json");
        assert_eq!(
            taskboard_lock_path(a),
            taskboard_lock_path(a),
            "gleiches Board -> gleiche Sperre"
        );
        assert_ne!(
            taskboard_lock_path(a),
            taskboard_lock_path(b),
            "andere Boards -> andere Sperren"
        );
        assert!(taskboard_lock_path(a)
            .extension()
            .unwrap()
            .to_string_lossy()
            .ends_with("lock"));
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

    #[test]
    fn verified_completion_markiert_claim_erst_bei_belastbarem_antrag_done() {
        use crate::acceptance::{CompletionReceipt, ExpectedCompletion, ACCEPTANCE_VERSION};
        let d = std::env::temp_dir().join(format!("webagent-v-ok-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        let proof = d.join("proof.txt");
        fs::write(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"webagent:claude","branch":"master"}]}"#,
        )
        .unwrap();
        fs::write(&proof, "beleg").unwrap();

        let store = crate::run_store::RunStore::new(d.join("runs"), d.join("logs"));
        let mut meta = store.create("claude", "T-806-Demo").unwrap();
        meta.status = "done".to_string();
        store.save(&meta).unwrap();

        let receipt = CompletionReceipt {
            version: ACCEPTANCE_VERSION,
            task_id: "T-1".into(),
            run_id: meta.run_id.clone(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            scope: "docs".into(),
            criteria: vec![
                crate::acceptance::CriterionOutcome::Passed,
                crate::acceptance::CriterionOutcome::Passed,
            ],
            artifact_hashes: vec!["sha256:beleg".into()],
        };
        let expected = ExpectedCompletion {
            task_id: "T-1".into(),
            run_id: meta.run_id.clone(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            run_status: meta.status.clone(),
        };

        complete_claim_verified(
            &board,
            "T-1",
            "webagent:claude",
            "master",
            &proof,
            &receipt,
            &expected,
            &store,
        )
        .unwrap();

        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "done");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn verified_completion_lehnt_unbestandene_kriterien_ab_und_lasst_board_claimed() {
        use crate::acceptance::{
            CompletionReceipt, CriterionOutcome, ExpectedCompletion, ACCEPTANCE_VERSION,
        };
        let d = std::env::temp_dir().join(format!("webagent-v-nok-{}", std::process::id()));
        let _ = fs::create_dir_all(&d);
        let board = d.join("TASKBOARD.json");
        let proof = d.join("proof.txt");
        fs::write(
            &board,
            r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"webagent:claude","branch":"master"}]}"#,
        )
        .unwrap();
        fs::write(&proof, "beleg").unwrap();

        let store = crate::run_store::RunStore::new(d.join("runs"), d.join("logs"));
        let mut meta = store.create("claude", "T-806-Demo").unwrap();
        meta.status = "done".to_string();
        store.save(&meta).unwrap();

        let receipt = CompletionReceipt {
            version: ACCEPTANCE_VERSION,
            task_id: "T-1".into(),
            run_id: meta.run_id.clone(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            scope: "docs".into(),
            criteria: vec![CriterionOutcome::Passed, CriterionOutcome::Unreachable],
            artifact_hashes: vec!["sha256:beleg".into()],
        };
        let expected = ExpectedCompletion {
            task_id: "T-1".into(),
            run_id: meta.run_id.clone(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            run_status: meta.status.clone(),
        };

        let err = complete_claim_verified(
            &board,
            "T-1",
            "webagent:claude",
            "master",
            &proof,
            &receipt,
            &expected,
            &store,
        )
        .expect_err("Unreachable-Kriterium darf den Abschluss verweigern");
        assert!(err.contains("verweigert"));

        let value: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(
            value["tasks"][0]["status"], "claimed",
            "Board darf nicht auf done wechseln"
        );

        let journal = d.join("runs").join(&meta.run_id).join("events.jsonl");
        let content = fs::read_to_string(&journal).unwrap();
        assert!(
            content.contains("task_completion_rejected"),
            "Ablehnung muss im Run-Eventlog stehen"
        );

        fs::remove_dir_all(&d).ok();
    }

    /// Invarianten des **echten** Boards, damit Claims nicht auseinanderlaufen.
    ///
    /// Harte Regeln gelten fuer jeden Eintrag. Vollstaendigkeits- und
    /// Schemaregeln greifen erst ab `CUTOFF`: Der Altbestand enthaelt Claims
    /// ohne `owner`/`claimed_at` und Branchnamen aus der Zeit vor dem Schema
    /// (`feat/browser-inference-provider`, `master`). Die nachtraeglich zu
    /// erfinden waere geraten, nicht belegt -- also wird Altlast eingefroren
    /// und nur neue Drift verhindert.
    #[test]
    fn taskboard_invarianten_halten() {
        const CUTOFF: &str = "2026-09-14";
        const PREFIXES: &[&str] = &["feature", "fix", "docs", "chore", "refactor", "test"];
        // T-966: Ein 1-Task-Stub hatte die vollstaendige Tafel ersetzt, ohne
        // dass eine bestehende Invariante ihn erkannt hat. Mindestumfang und
        // Spiegelabgleich machen genau diesen Bruch im normalen Test sichtbar.
        const MIN_TASKS: usize = 91;
        const V2_FIRST: u32 = 978;
        const MIRROR_BEGIN: &str = "<!-- TASKBOARD-MIRROR-BEGIN -->";
        const MIRROR_END: &str = "<!-- TASKBOARD-MIRROR-END -->";

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = root.join("docs/TASKBOARD.json");
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            value["schema_version"].as_u64(),
            Some(2),
            "Taskboard muss Schema v2 sein"
        );
        let tasks = value["tasks"]
            .as_array()
            .expect("tasks muss eine Liste sein");
        assert!(
            tasks.len() >= MIN_TASKS,
            "Taskboard unvollstaendig: {} statt mindestens {MIN_TASKS} Tasks",
            tasks.len()
        );

        let mut ids = std::collections::BTreeSet::new();
        let mut statuses = std::collections::BTreeMap::new();
        let mut claimed_branches = std::collections::BTreeMap::new();
        let mut v2_dependencies: std::collections::BTreeMap<String, Vec<String>> =
            std::collections::BTreeMap::new();

        for task in tasks {
            let id = task["id"].as_str().unwrap_or_default().to_string();
            assert!(!id.is_empty(), "Task ohne id");
            assert!(ids.insert(id.clone()), "doppelte id: {id}");

            let status = task["status"].as_str().unwrap_or_default();
            statuses.insert(id.clone(), status.to_string());
            let field = |key: &str| {
                task[key]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };

            match status {
                "free" => assert!(
                    field("owner").is_none() && field("branch").is_none(),
                    "{id}: 'free' darf keine Claim-Reste tragen"
                ),
                "deferred" => {}
                "claimed" | "done" => {
                    if status == "done" {
                        assert!(field("done_at").is_some(), "{id}: 'done' ohne done_at");
                    }
                    if status == "claimed" {
                        if let Some(branch) = field("branch") {
                            if let Some(other) = claimed_branches.insert(branch.clone(), id.clone())
                            {
                                panic!("{id} und {other} beanspruchen denselben Branch {branch}");
                            }
                        }
                    }
                    if field("claimed_at").is_some_and(|date| date.as_str() >= CUTOFF) {
                        assert!(
                            field("owner").is_some(),
                            "{id}: Claim ab {CUTOFF} ohne owner"
                        );
                        let branch = field("branch")
                            .unwrap_or_else(|| panic!("{id}: Claim ab {CUTOFF} ohne branch"));
                        let (prefix, rest) = branch
                            .split_once('/')
                            .unwrap_or_else(|| panic!("{id}: Branch '{branch}' ohne Praefix"));
                        assert!(
                            PREFIXES.contains(&prefix),
                            "{id}: unbekannter Branch-Praefix in '{branch}', erlaubt: {PREFIXES:?}"
                        );
                        assert!(
                            rest.starts_with("T-"),
                            "{id}: Branch '{branch}' nennt keine Task-ID"
                        );
                    }
                }
                other => panic!("{id}: unbekannter Status '{other}'"),
            }

            // Schema v2 gilt ab T-978. Aeltere Tasks bleiben ausdruecklich
            // grandfathered: Ihre Kanten stehen als Prosa in `dod`, nicht
            // als maschinenlesbare Felder.
            let number = id
                .strip_prefix("T-")
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0);
            if number >= V2_FIRST {
                assert_eq!(status, "free", "{id}: neue Tasks starten immer frei");
                let deps = task["depends_on"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{id}: depends_on fehlt"));
                let mut seen = std::collections::BTreeSet::new();
                let mut predecessors = Vec::new();
                for dep in deps {
                    let dep = dep
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| panic!("{id}: depends_on ohne Task-ID"));
                    assert!(seen.insert(dep.to_string()), "{id}: doppelte Kante {dep}");
                    assert_ne!(dep, id, "{id}: Selbstkante in depends_on");
                    predecessors.push(dep.to_string());
                }
                let scope = task["scope"]
                    .as_array()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| panic!("{id}: scope fehlt oder ist leer"));
                assert!(
                    scope
                        .iter()
                        .all(|s| s.as_str().is_some_and(|s| !s.is_empty())),
                    "{id}: scope enthält einen leeren Pfad"
                );
                for key in ["description", "verify"] {
                    assert!(field(key).is_some(), "{id}: {key} fehlt im v2-Schema");
                }
                let acceptance = task["acceptance"]
                    .as_array()
                    .filter(|a| !a.is_empty())
                    .unwrap_or_else(|| panic!("{id}: acceptance fehlt oder ist leer"));
                assert!(
                    acceptance
                        .iter()
                        .all(|a| a.as_str().is_some_and(|a| !a.is_empty())),
                    "{id}: acceptance enthält einen leeren Eintrag"
                );
                v2_dependencies.insert(id.clone(), predecessors);
            }
        }

        // Kanten muessen auf existierende Tasks zeigen und zyklusfrei sein;
        // sonst waere ein Bazaar-Claim dauerhaft blockiert oder heimlich
        // erlaubt.
        for (id, deps) in &v2_dependencies {
            for dep in deps {
                assert!(
                    ids.contains(dep),
                    "{id}: unbekannter Vorgänger {dep} in depends_on"
                );
            }
        }
        let mut remaining: std::collections::BTreeMap<String, usize> = v2_dependencies
            .iter()
            .map(|(id, deps)| (id.clone(), deps.len()))
            .collect();
        let mut ready: Vec<String> = remaining
            .iter()
            .filter(|(_, count)| **count == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut resolved = 0;
        while let Some(id) = ready.pop() {
            resolved += 1;
            for (other, deps) in &v2_dependencies {
                if deps.contains(&id) {
                    let count = remaining
                        .get_mut(other)
                        .unwrap_or_else(|| panic!("{other} fehlt in der Kantenliste"));
                    *count -= 1;
                    if *count == 0 {
                        ready.push(other.clone());
                    }
                }
            }
        }
        assert_eq!(
            resolved,
            v2_dependencies.len(),
            "Zyklus in depends_on: nur {resolved} von {} Tasks aufloesbar",
            v2_dependencies.len()
        );

        // `docs/TASKBOARD.md` ist ein Spiegel, keine zweite Wahrheit: Er muss
        // exakt dieselben IDs und Statuswerte enthalten wie die JSON-Datei.
        let mirror = fs::read_to_string(root.join("docs/TASKBOARD.md")).unwrap();
        let start = mirror
            .find(MIRROR_BEGIN)
            .unwrap_or_else(|| panic!("TASKBOARD.md ohne {MIRROR_BEGIN}"));
        let body = &mirror[start + MIRROR_BEGIN.len()..];
        let end = body
            .find(MIRROR_END)
            .unwrap_or_else(|| panic!("TASKBOARD.md ohne {MIRROR_END}"));
        let mut mirror_statuses = std::collections::BTreeMap::new();
        for line in body[..end].lines() {
            let line = line.trim();
            if !line.starts_with("| T-") {
                continue;
            }
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            assert!(cells.len() >= 4, "Spiegelzeile ohne Status: {line}");
            assert!(
                mirror_statuses
                    .insert(cells[1].to_string(), cells[2].to_string())
                    .is_none(),
                "doppelter Spieleintrag: {}",
                cells[1]
            );
        }
        assert_eq!(
            mirror_statuses, statuses,
            "docs/TASKBOARD.md ist nicht mit docs/TASKBOARD.json synchron"
        );
    }
}
