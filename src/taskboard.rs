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
use std::path::Path;

/// Ein Task offiziell claimen (fail-closed).
///
/// Erlaubt nur, wenn der Task existiert, `status == "free"` ist und kein
/// `claim_lock` trägt. Schlägt fehl, sobald der Task bereits geclaimt, done
/// oder gesperrt ist. Schreiben ist atomar wie bei [`complete_claim`].
pub fn acquire_claim(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
) -> Result<(), String> {
    if task_id.trim().is_empty() || owner.trim().is_empty() || branch.trim().is_empty() {
        return Err("Task-ID, owner und branch sind Pflicht".into());
    }
    let raw = fs::read_to_string(taskboard).map_err(|e| format!("Taskboard lesen: {e}"))?;
    let mut root: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
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
    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Taskboard schreiben: {e}"))?
        + "\n";
    let tmp = taskboard.with_extension("json.acquire.tmp");
    fs::write(&tmp, formatted).map_err(|e| format!("Taskboard temporär schreiben: {e}"))?;
    fs::rename(&tmp, taskboard).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Taskboard atomar ersetzen: {e}")
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
    let raw = fs::read_to_string(taskboard).map_err(|e| format!("Taskboard lesen: {e}"))?;
    let mut root: Value = serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
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
    let formatted = serde_json::to_string_pretty(&root)
        .map_err(|e| format!("Taskboard schreiben: {e}"))?
        + "\n";
    let tmp = taskboard.with_extension("json.complete.tmp");
    fs::write(&tmp, formatted).map_err(|e| format!("Taskboard temporär schreiben: {e}"))?;
    fs::rename(&tmp, taskboard).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("Taskboard atomar ersetzen: {e}")
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
}
