//! Fail-closed Abschluss von Taskboard-Claims.
//!
//! Ein Run darf einen Claim nur nach explizitem Belegabschluss auf `done`
//! setzen. Die Operation schreibt atomar und verweigert fremde oder bereits
//! abgeschlossene Claims.

use serde_json::Value;
use std::fs;
use std::path::Path;

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
    if !proof_path.exists() {
        return Err(format!("Belegpfad fehlt: {}", proof_path.display()));
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
}
