//! Fail-closed verifizierter Abschluss von Taskboard-Claims (Scheibe 6).
//!
//! Ein Run darf einen Claim nur nach verifiziertem Belegabschluss auf `done`
//! setzen. Der reine Datei-Existenz-Check ist ersetzt: Der Abschluss läuft
//! unter einer prozessübergreifenden Sperre (`run_ledger::LedgerLock`),
//! liest das Board unter der Sperre erneut, vergleicht Claim/Owner/Branch,
//! Dependencies und die eingefrorenen Abnahmeanforderungen und validiert den
//! Beleg controllerseitig (Datei, nicht leer, Geltungsbereich
//! `docs/proofs/<task_id>/`, Frische gegen `claimed_at`, SHA-256 des Inhalts).
//! Beim Abschluss werden Artefakt-Hash, Commit und Run-ID im Board gespeichert
//! — das Brain-Manifest ist Antrag, kein eigener Beweis.
//!
//! Alte/leere/fremde/manipulierte Belege und parallele Boardänderungen dürfen
//! nie `done` ergeben. Replay (zweiter Abschluss mit identischem Beleg und
//! identischem Hash) ist idempotent und schreibt nichts erneut.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// User-Agent-Festcode entschärft: Der Owner kommt aus CLI/Env (siehe ops.rs),
/// nur dieser Fallback bleibt als letzte Verteidigung gegen das komplett
/// leere Umfeld.
pub const DEFAULT_OWNER: &str = "local/opencode";
/// Env-Variable für den expliziten Agenten-Owner.
pub const AGENT_ID_ENV: &str = "WEBAGENT_AGENT_ID";

/// Eingefrorene Abnahmeanforderungen eines Claims — zur Abschlusszeit
/// unverändert abgeglichen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirements {
    pub verification: String,
    pub dod: String,
    pub depends_on: Vec<String>,
}

/// Ein frisch geclaimter Task inklusive eingefrorener Anforderungen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedTask {
    pub task_id: String,
    pub owner: String,
    pub branch: String,
    pub claimed_at: String,
    pub requirements: Requirements,
}

/// Ergebnis eines verifizierten Abschlusses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionOutcome {
    pub task_id: String,
    pub proof_path: String,
    pub proof_sha256: String,
    pub proof_commit: String,
    pub run_id: String,
    pub done_at: String,
    /// `true`, wenn der zweite Abschluss mit identischem Beleg nicht erneut
    /// geschrieben, sondern als bereits-fehlgeschlagen erkannt wurde.
    pub replayed: bool,
}

fn tasks_mut(root: &mut Value) -> Result<&mut Vec<Value>, String> {
    root.get_mut("tasks")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())
}

fn find_task<'a>(tasks: &'a mut Vec<Value>, task_id: &str) -> Result<&'a mut Value, String> {
    tasks
        .iter_mut()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| format!("Unbekannte Task-ID: {task_id}"))
}

fn find_task_ref<'a>(tasks: &'a [Value], task_id: &str) -> Result<&'a Value, String> {
    tasks
        .iter()
        .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
        .ok_or_else(|| format!("Unbekannte Task-ID: {task_id}"))
}

/// Liest `requirements_snapshot` (eingefroren beim Claim) oder verwendet die
/// aktuellen Felder, wenn die Aufgabe noch nie verifiziert abgeschlossen wurde.
fn frozen_requirements(task: &Value, task_id: &str) -> Result<Requirements, String> {
    let snapshot = task.get("requirements_snapshot");
    match snapshot {
        Some(v) => {
            let verification = v
                .get("verification")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let dod = v
                .get("dod")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let depends_on = v
                .get("depends_on")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            Ok(Requirements {
                verification,
                dod,
                depends_on,
            })
        }
        None => Err(format!(
            "Task {task_id} wurde nicht mit eingefrorenen Abnahmeanforderungen \
             geclaimt (requirements_snapshot fehlt)"
        )),
    }
}

/// Liefert die aktuellen Abnahmeanforderungen einer Aufgabe aus dem Board.
fn current_requirements(task: &Value) -> Requirements {
    Requirements {
        verification: task
            .get("verification")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        dod: task
            .get("dod")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        depends_on: task
            .get("depends_on")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Liest den Status aller dependierenden Tasks im Board.
fn dependency_states(
    root: &Value,
    depends: &[String],
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut states = std::collections::BTreeMap::new();
    for dep in depends {
        let entry = root["tasks"]
            .as_array()
            .ok_or_else(|| "Taskboard hat keine tasks-Liste".to_string())?
            .iter()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(dep.as_str()))
            .ok_or_else(|| format!("Dependency {dep} existiert nicht im Board"))?;
        let state = entry
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unbekannt")
            .to_string();
        states.insert(dep.clone(), state);
    }
    Ok(states)
}

/// Beleg-Scope: Board liegt in `docs/TASKBOARD.json`; der Geschwisterordner
/// `docs/proofs/<task_id>/` ist der Geltungsbereich jedes Belegs.
fn board_path_scope(board: &Path) -> PathBuf {
    // Board liegt in docs/TASKBOARD.json → Beleg-Scope ist der Geschwisterordner
    // docs/proofs/<task_id>/.
    board
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("proofs")
}

/// Board-Lock als LedgerLock; das Lock-Verzeichnis muss existieren, sonst
/// schlaegt die atomare `create_dir`-Reservation mit Pfad-fehlt fehl.
fn acquire_board_lock(board: &Path) -> Result<crate::run_ledger::LedgerLock, String> {
    let lock_dir = board.with_extension("taskboard.lock");
    fs::create_dir_all(&lock_dir).map_err(|e| format!("Lock-Verzeichnis: {e}"))?;
    crate::run_ledger::LedgerLock::acquire(&lock_dir, Duration::from_secs(30))
}

/// Read-Only-Sicht auf das Board unter der Prozesssperre.
fn read_board_locked<T>(
    board: &Path,
    timeout: Duration,
    f: impl FnOnce(&Value) -> Result<T, String>,
) -> Result<T, String> {
    let lock_dir = board.with_extension("taskboard.lock");
    fs::create_dir_all(&lock_dir).map_err(|e| format!("Lock-Verzeichnis: {e}"))?;
    let lock = crate::run_ledger::LedgerLock::acquire(&lock_dir, timeout)?;
    let raw = fs::read_to_string(board).map_err(|e| format!("Taskboard lesen: {e}"))?;
    let root: Value =
        serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
    let result = f(&root);
    drop(lock);
    result
}

/// Claimt einen Task mit eingefrorenen Abnahmeanforderungen.
///
/// Erlaubt nur, wenn der Task existiert, `status == "free"` ist und kein
/// `claim_lock` trägt. Schlägt fehl, sobald der Task bereits geclaimt, done
/// oder gesperrt ist. Liefert den Claim inklusive eingefrorenem
/// `requirements_snapshot`, gegen den der Abschluss die Anforderungen prüft.
/// Schreiben ist atomar (eindeutige Tempdatei + fsync + Rename) unter einer
/// prozessübergreifenden Sperre.
pub fn acquire_claim(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
) -> Result<ClaimedTask, String> {
    if task_id.trim().is_empty() || owner.trim().is_empty() || branch.trim().is_empty() {
        return Err("Task-ID, owner und branch sind Pflicht".into());
    }
    let lock = acquire_board_lock(taskboard)?;
    let result = (|| {
        let raw =
            fs::read_to_string(taskboard).map_err(|e| format!("Taskboard lesen: {e}"))?;
        let mut root: Value =
            serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
        let task = {
            let tasks = tasks_mut(&mut root)?;
            find_task(tasks, task_id)?
        };
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
        // Abnahmeanforderungen vor dem Run einfrieren, damit spätere
        // Boardänderungen den Abschluss nicht aufweichen.
        let requirements = current_requirements(task);
        let claimed_at = crate::now_rfc3339();
        task["status"] = Value::String("claimed".into());
        task["owner"] = Value::String(owner.into());
        task["branch"] = Value::String(branch.into());
        task["claimed_at"] = Value::String(claimed_at.clone());
        task["requirements_snapshot"] = json!({
            "verification": requirements.verification,
            "dod": requirements.dod,
            "depends_on": requirements.depends_on,
        });
        let formatted = serde_json::to_string_pretty(&root)
            .map_err(|e| format!("Taskboard schreiben: {e}"))?
            + "\n";
        crate::run_ledger::atomic_write(taskboard, formatted.as_bytes())?;
        Ok(ClaimedTask {
            task_id: task_id.to_string(),
            owner: owner.to_string(),
            branch: branch.to_string(),
            claimed_at,
            requirements,
        })
    })();
    drop(lock);
    result
}

/// Verifizierter Abschluss eines Claims.
///
/// - Board wird unter der Prozesssperre erneut gelesen
/// - Claim muss `claimed` sein und Owner/Branch übereinstimmen
/// - Alle eingefrorenen Dependencies müssen `done` sein
/// - Die Abnahmeanforderungen müssen unverändert (nicht manipuliert) sein
/// - Der Beleg läuft durch den controllerseitigen Check: Datei, nicht leer,
///   Geltungsbereich `docs/proofs/<task_id>/`, Frische ≥ `claimed_at`,
///   SHA-256 des Inhalts wird errechnet und mit Run-ID + Commit gespeichert
/// - Replay mit identischem Beleg und identischem Hash ist idempotent (kein
///   weiterer Schreibzugriff, `replayed == true`); ein abweichender Hash nach
///   `done` wird verweigert.
pub fn complete_claim_verified(
    taskboard: &Path,
    task_id: &str,
    owner: &str,
    branch: &str,
    run_id: &str,
    proof_path: &Path,
) -> Result<CompletionOutcome, String> {
    if task_id.trim().is_empty() || owner.trim().is_empty() || branch.trim().is_empty() {
        return Err("Task-ID, owner und branch sind Pflicht".into());
    }
    if run_id.trim().is_empty() || proof_path.as_os_str().is_empty() {
        return Err("run_id und proof_path sind Pflicht".into());
    }
    let timeout = Duration::from_secs(30);
    let lock = acquire_board_lock(taskboard)?;
    let result = (|| {
        // 1) Beleg controllerseitig prüfen (bevor das Board angefasst wird).
        let meta = fs::metadata(proof_path).map_err(|e| {
            format!(
                "Konkrete Belegdatei fehlt (Verzeichnisse sind unzulässig): {}: {e}",
                proof_path.display()
            )
        })?;
        if !meta.is_file() {
            return Err(format!(
                "Beleg ist ein Verzeichnis, keine Datei: {}",
                proof_path.display()
            ));
        }
        if meta.len() == 0 {
            return Err(format!("Belegdatei ist leer: {}", proof_path.display()));
        }
        // Geltungsbereich: Beleg muss unter docs/proofs/<task_id>/ liegen.
        let scope = board_path_scope(taskboard).join(task_id);
        if !proof_path.starts_with(&scope) {
            return Err(format!(
                "Beleg liegt ausserhalb des Geltungsbereichs {}: {}",
                scope.display(),
                proof_path.display()
            ));
        }
        // Artefakt-Hash controllerseitig über den unangetasteten Inhalt.
        let bytes = fs::read(proof_path)
            .map_err(|e| format!("Beleg lesen: {}: {e}", proof_path.display()))?;
        let proof_sha256 = hex_sha256(&bytes);

        // 2) Board unter der Sperre erneut lesen.
        let raw =
            fs::read_to_string(taskboard).map_err(|e| format!("Taskboard lesen: {e}"))?;
        let root: Value =
            serde_json::from_str(&raw).map_err(|e| format!("Taskboard-JSON: {e}"))?;
        let tasks = root["tasks"]
            .as_array()
            .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())?;
        let task = find_task_ref(tasks, task_id)?;
        // Replay-Erkennung: bereits `done`?
        let prev_proof = task
            .get("proof_path")
            .and_then(Value::as_str)
            .map(str::to_string);
        let prev_hash = task
            .get("proof_sha256")
            .and_then(Value::as_str)
            .map(str::to_string);
        if task.get("status").and_then(Value::as_str) == Some("done") {
            if prev_proof.as_deref() == Some(&proof_path.to_string_lossy().replace('\\', "/"))
                && prev_hash.as_deref() == Some(&proof_sha256)
            {
                // Idempotenter Replay: gleicher Beleg, gleicher Hash — nichts
                // erneut schreiben.
                let done_at = task
                    .get("done_at")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                return Ok(CompletionOutcome {
                    task_id: task_id.to_string(),
                    proof_path: proof_path.to_string_lossy().replace('\\', "/"),
                    proof_sha256,
                    proof_commit: task
                        .get("proof_commit")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    run_id: task
                        .get("run_id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    done_at,
                    replayed: true,
                });
            }
            return Err(format!(
                "Task {task_id} ist bereits done mit anderem Beleg oder anderem \
                 Hash — manueller Eingriff in den Beleg nötig"
            ));
        }
        if task.get("status").and_then(Value::as_str) != Some("claimed") {
            return Err("Nur ein claimed-Task darf abgeschlossen werden".into());
        }
        if task.get("owner").and_then(Value::as_str) != Some(owner)
            || task.get("branch").and_then(Value::as_str) != Some(branch)
        {
            return Err("Owner oder Branch stimmt nicht mit dem Claim überein".into());
        }

        // 3) Eingefrorene Anforderungen gegen die aktuellen vergleichen —
        //    nachträgliche Manipulation darf den Abschluss nicht aufweichen.
        let frozen = frozen_requirements(task, task_id)?;
        let current = current_requirements(task);
        if frozen != current {
            return Err(format!(
                "Abnahmeanforderungen von Task {task_id} haben sich seit dem Claim \
                 geändert (verification/dod/depends_on) — Abschluss verweigert"
            ));
        }

        // 4) Dependencies: alle eingefrorenen dürfen nicht offen sein.
        let states = dependency_states(&root, &frozen.depends_on)?;
        for (dep, state) in &states {
            if state != "done" {
                return Err(format!(
                    "Dependency {dep} von Task {task_id} ist {state}, nicht done — \
                     Abschluss verweigert"
                ));
            }
        }

        // 5) Frische: der Beleg darf nicht älter sein als der Claim.
        let claimed_at = task
            .get("claimed_at")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_unix);
        if let Some(claimed_unix) = claimed_at {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64);
            if let Some(mtime_unix) = mtime {
                if mtime_unix < claimed_unix {
                    return Err(format!(
                        "Beleg {} ist älter als der Claim (claimed_at zu spät oder \
                         Beleg manipuliert) — alter Beleg ergibt nie done",
                        proof_path.display()
                    ));
                }
            }
        }

        // 6) Controllerseitig erfasster Commit (git HEAD) und Run-ID binden.
        let proof_commit = git_head_commit();
        let done_at = crate::now_rfc3339();
        let _ = task;
        let _ = tasks;
        let mut root = root;
        let task = {
            let tasks = tasks_mut(&mut root)?;
            find_task(tasks, task_id)?
        };
        task["status"] = Value::String("done".into());
        task["done_at"] = Value::String(done_at.clone());
        task["proof_path"] =
            Value::String(proof_path.to_string_lossy().replace('\\', "/"));
        task["proof_sha256"] = Value::String(proof_sha256.clone());
        task["proof_commit"] = Value::String(proof_commit.clone());
        task["run_id"] = Value::String(run_id.to_string());
        let formatted = serde_json::to_string_pretty(&root)
            .map_err(|e| format!("Taskboard schreiben: {e}"))?
            + "\n";
        crate::run_ledger::atomic_write(taskboard, formatted.as_bytes())?;

        Ok(CompletionOutcome {
            task_id: task_id.to_string(),
            proof_path: proof_path.to_string_lossy().replace('\\', "/"),
            proof_sha256,
            proof_commit,
            run_id: run_id.to_string(),
            done_at,
            replayed: false,
        })
    })();
    drop(lock);
    result
}

/// SHA-256-Hex über eine Bytefolge.
pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn parse_rfc3339_unix(s: &str) -> Option<i64> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

/// Aktueller Git-HEAD-Commit (controllerseitig; leer, wenn nicht verfügbar).
pub fn git_head_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| o.status.success().then_some(o.stdout))
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Liest den aktuellen Status eines Tasks aus dem Board (read-only, unter Lock).
pub fn task_status(board: &Path, task_id: &str) -> Result<String, String> {
    read_board_locked(board, Duration::from_secs(10), |root| {
        let tasks = root
            .get("tasks")
            .and_then(Value::as_array)
            .ok_or_else(|| "Taskboard enthält kein tasks-Array".to_string())?;
        let task = tasks
            .iter()
            .find(|t| t.get("id").and_then(Value::as_str) == Some(task_id))
            .ok_or_else(|| format!("Unbekannte Task-ID: {task_id}"))?;
        Ok(task
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unbekannt")
            .to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::SystemTime;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let d = std::env::temp_dir().join(format!("webagent-taskboard-{tag}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    /// Board mit einem Task: `status`, `owner`, `branch`, `claimed_at` sowie
    /// die Abnahmeanforderungen (verification/dod/depends_on).
    fn board_with(dir: &Path, task_id: &str, json: &str) -> PathBuf {
        let board = dir.join("TASKBOARD.json");
        fs::write(&board, json).unwrap();
        // Sicherstellen, dass der Beleg-Scope-Ordner existiert.
        fs::create_dir_all(dir.join("proofs").join(task_id)).unwrap();
        board
    }

    fn base_task(task_id: &str, status: &str) -> String {
        format!(
            r#"{{"id":"{task_id}","status":"{status}","owner":"dev","branch":"feature/x","verification":"Plan Z.40","dod":"Gates gruen","depends_on":["DEP-1"],"claimed_at":"2026-01-01T00:00:00Z"}}"#
        )
    }

    fn deps_done() -> String {
        r#"{"id":"DEP-1","status":"done"}"#.to_string()
    }

    /// Schreibt einen frischen Beleg in den Scope des Tasks.
    fn fresh_proof(dir: &Path, task_id: &str) -> PathBuf {
        let p = dir.join("proofs").join(task_id).join("RESULT.md");
        fs::write(&p, format!("# {task_id}\n\n**Archiv:** Beleg.\n")).unwrap();
        p
    }

    fn claim(board: &Path, task_id: &str) -> ClaimedTask {
        acquire_claim(board, task_id, "dev", "feature/x").unwrap()
    }

    #[test]
    fn acquire_friert_abnahmeanforderungen_ein() {
        let d = temp_dir("freeze");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let claimed = claim(&board, "T-1");
        assert_eq!(claimed.requirements.verification, "Plan Z.40");
        assert_eq!(claimed.requirements.dod, "Gates gruen");
        assert_eq!(claimed.requirements.depends_on, vec!["DEP-1".to_string()]);
        assert_eq!(claimed.owner, "dev");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        let task = &value["tasks"][0];
        assert_eq!(task["status"], "claimed");
        assert_eq!(task["requirements_snapshot"]["dod"], "Gates gruen");
    }

    #[test]
    fn abschluss_erfordert_verifizierten_scope_und_hash() {
        let d = temp_dir("scope");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        // Anforderungen nachträglich "einfrieren" wie der Claim es täte.
        let claim = claim(&board, "T-1"); // jetzt mit requirements_snapshot
        let proof = fresh_proof(&d, "T-1");
        let outcome =
            complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof)
                .unwrap();
        assert_eq!(outcome.task_id, "T-1");
        assert_eq!(outcome.replayed, false);
        assert_eq!(outcome.proof_sha256.len(), 64);
        assert_eq!(outcome.run_id, "run-1");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        let task = &value["tasks"][0];
        assert_eq!(task["status"], "done");
        assert_eq!(task["proof_sha256"], outcome.proof_sha256);
        assert!(task["done_at"].as_str().is_some_and(|s| !s.is_empty()));
        drop(claim);
        drop(proof);
    }

    #[test]
    fn leerer_oder_externer_beleg_ergibt_nie_done() {
        let d = temp_dir("leer");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");

        // Leere Belegdatei im Scope.
        let empty = d.join("proofs").join("T-1").join("empty.md");
        fs::write(&empty, "").unwrap();
        assert!(complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &empty).is_err());

        // Beleg ausserhalb des Geltungsbereichs.
        let outside = d.join("anderswo.md");
        fs::write(&outside, "x").unwrap();
        assert!(complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &outside).is_err());

        // Verzeichnis statt Datei.
        let dir_proof = d.join("proofs").join("T-1").join("unterordner");
        fs::create_dir_all(&dir_proof).unwrap();
        assert!(complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &dir_proof).is_err());

        let value: Value =
            serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "claimed");
    }

    #[test]
    fn offene_dependency_verhindert_done() {
        let d = temp_dir("dep");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                r#"{"id":"DEP-1","status":"claimed"}"#
            ),
        );
        let _claim = claim(&board, "T-1");
        let proof = fresh_proof(&d, "T-1");
        let err = complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof).unwrap_err();
        assert!(err.contains("nicht done"), "Fehlertext: {err}");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "claimed");
    }

    #[test]
    fn geaenderte_anforderungen_sind_manipulation() {
        let d = temp_dir("manip");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");
        // Nachclaim eines Requirements ändern (Board extern verändert).
        let raw = fs::read_to_string(&board).unwrap();
        let mut root: Value = serde_json::from_str(&raw).unwrap();
        root["tasks"][0]["dod"] = json!("Gates gruen EASY-MODE");
        fs::write(&board, serde_json::to_string_pretty(&root).unwrap() + "\n").unwrap();
        let proof = fresh_proof(&d, "T-1");
        let err =
            complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof)
                .unwrap_err();
        assert!(err.contains("geändert"), "Fehlertext: {err}");
        let value: Value =
            serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        assert_eq!(value["tasks"][0]["status"], "claimed");
    }

    #[test]
    fn fremder_owner_und_branch_werden_abgelehnt() {
        let d = temp_dir("fremd");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");
        let proof = fresh_proof(&d, "T-1");
        assert!(complete_claim_verified(&board, "T-1", "x", "feature/x", "run-1", &proof).is_err());
        assert!(complete_claim_verified(&board, "T-1", "dev", "fremd", "run-1", &proof).is_err());
    }

    #[test]
    fn alter_beleg_vor_claim_ergibt_nie_done() {
        let d = temp_dir("alt");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");
        // Beleg mit mtime in der Vergangenheit anlegen (vor dem Claim).
        let p = fresh_proof(&d, "T-1");
        let past = SystemTime::now() - std::time::Duration::from_secs(3600);
        // datei mtime im Nachhinein setzen ist auf Windows unzuverlässig —
        // stattdessen prüfen wir über claimed_at der Zukunft.
        fs::write(&p, "frischer Inhalt").unwrap();
        let raw = fs::read_to_string(&board).unwrap();
        let mut root: Value = serde_json::from_str(&raw).unwrap();
        root["tasks"][0]["claimed_at"] = json!("2099-12-31T00:00:00Z");
        fs::write(&board, serde_json::to_string_pretty(&root).unwrap() + "\n").unwrap();
        let err =
            complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &p).is_err();
        assert!(err, "Claim in der (eingetragenen) Zukunft muss Abschluss verweigern");
    }

    #[test]
    fn replay_ist_idempotent_und_fremder_hash_nach_done_blockiert() {
        let d = temp_dir("replay");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");
        let proof = fresh_proof(&d, "T-1");
        let first =
            complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof).unwrap();
        assert_eq!(first.replayed, false);
        // Identischer Replay: idempotent, schreibt nichts erneut.
        let replay =
            complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof).unwrap();
        assert_eq!(replay.replayed, true);
        assert_eq!(replay.proof_sha256, first.proof_sha256);
        // Manipulierter Beleg nach done: anderer Hash → blockiert.
        fs::write(&proof, "# T-1\n\nAnderer Inhalt.\n").unwrap();
        assert!(complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof).is_err());
    }

    #[test]
    fn parallele_boardaenderung_nach_claim_aendert_owner_und_blockiert() {
        let d = temp_dir("parallel");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "claimed"),
                deps_done()
            ),
        );
        // Anforderungen und Snapshot von Hand setzen (wie ein realer Claim),
        // dann den Owner extern ändern (simulierter paralleler Dev).
        let raw = fs::read_to_string(&board).unwrap();
        let mut root: Value = serde_json::from_str(&raw).unwrap();
        root["tasks"][0]["requirements_snapshot"] = json!({
            "verification": "Plan Z.40",
            "dod": "Gates gruen",
            "depends_on": ["DEP-1"],
        });
        fs::write(&board, serde_json::to_string_pretty(&root).unwrap() + "\n").unwrap();
        let proof = fresh_proof(&d, "T-1");
        // Externer paralleler Prozess stiehlt den Claim.
        let mut root2: Value = serde_json::from_str(&fs::read_to_string(&board).unwrap()).unwrap();
        root2["tasks"][0]["owner"] = json!("anderer");
        fs::write(&board, serde_json::to_string_pretty(&root2).unwrap() + "\n").unwrap();
        assert!(complete_claim_verified(&board, "T-1", "dev", "feature/x", "run-1", &proof).is_err());
    }

    #[test]
    fn acquire_bleibt_fail_closed() {
        let d = temp_dir("failclosed");
        let board = board_with(
            &d,
            "T-1",
            &r#"{"tasks":[{"id":"T-1","status":"claimed","owner":"a","branch":"b"}]}"#,
        );
        assert!(acquire_claim(&board, "T-1", "dev", "feature/x").is_err());
        let board2 = board_with(
            &d,
            "T-2",
            &r#"{"tasks":[{"id":"T-2","status":"done"}]}"#,
        );
        assert!(acquire_claim(&board2, "T-2", "dev", "feature/x").is_err());
        let board3 = board_with(
            &d,
            "T-3",
            &r#"{"tasks":[{"id":"T-3","status":"free","claim_lock":true}]}"#,
        );
        assert!(acquire_claim(&board3, "T-3", "dev", "feature/x").is_err());
        let board4 = board_with(
            &d,
            "T-4",
            &r#"{"tasks":[{"id":"T-4","status":"free"}]}"#,
        );
        assert!(acquire_claim(&board4, "T-9", "dev", "feature/x").is_err());
    }

    #[test]
    fn sha256_hex_ist_stabil() {
        assert_eq!(
            hex_sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(hex_sha256(b"").len(), 64);
    }

    #[test]
    fn task_status_liest_unter_lock() {
        let d = temp_dir("status");
        let board = board_with(
            &d,
            "T-1",
            &format!(
                r#"{{"tasks":[{},{}]}}"#,
                base_task("T-1", "free"),
                deps_done()
            ),
        );
        let _claim = claim(&board, "T-1");
        assert_eq!(task_status(&board, "T-1").unwrap(), "claimed");
    }
}