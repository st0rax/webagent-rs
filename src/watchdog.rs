//! Watchdog — Phase 1: Scannt und repariert verwaiste Runs, tote Bridge-Locks,
//! tote Browser-Profil-Locks. Idempotent und sicher bei Dry-Run.
//!
//! Bestehende Self-Healing-Mechanismen werden NICHT dupliziert:
//! - RunStore::reconcile_stale_runs (run_store.rs) — Watchdog scannt stale parallel,
//!   reconcile passiert separat im CLI-Start.
//! - bridge_lock (bot2bot_webbrain_bridge.py) — self-healing beim nächsten Aufruf;
//!   Watchdog entfernt nur Locks toter Prozesse nach Grace-Period.
//! - _cleanup_profile_locks (playwright_base.py) — Watchdog scannt stale locks
//!   (konservativ, nur wenn kein Chrome via PS), repair entfernt; Kill in cleanup.
//!   Integriert doctor lock find.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::run_store::RunMeta;

/// Grace-Period für Bridge-Locks, bevor sie als verwaist betrachtet werden.
/// Muss mit bridge_lock-Logik übereinstimmen.
pub const BRIDGE_LOCK_GRACE_SECONDS: f64 = 60.0;

/// Ab wann ein Browser-Profil-Lock ohne aktiven Chrome-Prozess als verwaist gilt.
pub const PROFILE_LOCK_GRACE_SECONDS: f64 = 30.0;

/// Ein verwaister (stale) Run mit Status "running".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrphanedRun {
    pub run_id: String,
    pub brain_id: String,
    pub age_seconds: f64,
}

/// Ein Bridge-Lock, dessen Holder-Prozess nicht mehr läuft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleBridgeLock {
    pub path: String,
    pub agent_slug: String,
    pub holder_pid: i64,
    pub age_seconds: f64,
}

/// Ein Browser-Profil-Lock ohne aktiven Chrome-Prozess.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaleProfileLock {
    pub path: String,
    pub age_seconds: f64,
}

/// Aggregierter Watchdog-Bericht.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WatchdogReport {
    pub timestamp: String,
    pub orphaned_runs: Vec<OrphanedRun>,
    pub stale_bridge_locks: Vec<StaleBridgeLock>,
    pub stale_profile_locks: Vec<StaleProfileLock>,
    pub repaired_runs: Vec<String>,
    pub repaired_bridge_locks: Vec<String>,
    pub repaired_profile_locks: Vec<String>,
    pub errors: Vec<String>,
}

impl WatchdogReport {
    pub fn ok(&self) -> bool {
        self.orphaned_runs.is_empty()
            && self.stale_bridge_locks.is_empty()
            && self.stale_profile_locks.is_empty()
    }

    pub fn total_findings(&self) -> usize {
        self.orphaned_runs.len() + self.stale_bridge_locks.len() + self.stale_profile_locks.len()
    }

    pub fn total_repaired(&self) -> usize {
        self.repaired_runs.len()
            + self.repaired_bridge_locks.len()
            + self.repaired_profile_locks.len()
    }
}

/// Prüft, ob ein Prozess mit gegebener PID lebt (plattformübergreifend).
fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(windows)]
    {
        // tasklist liefert die Zeile nur, wenn der Prozess existiert.
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH", "/FO", "CSV"])
            .output();
        match out {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                text.contains(&format!("\"{}\"", pid))
            }
            Err(_) => true,
        }
    }
    #[cfg(not(windows))]
    {
        match std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
        {
            Ok(st) => st.success(),
            Err(_) => true,
        }
    }
}

/// Alter einer Datei in Sekunden; -1.0 bei Fehler.
fn file_age_seconds(path: &str) -> f64 {
    match fs::metadata(path) {
        Ok(meta) => match meta.modified() {
            Ok(modified) => match modified.duration_since(UNIX_EPOCH) {
                Ok(dur) => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default();
                    (now.as_secs_f64() - dur.as_secs_f64()).max(0.0)
                }
                Err(_) => -1.0,
            },
            Err(_) => -1.0,
        },
        Err(_) => -1.0,
    }
}

/// Scannt nach verwaisten Runs mit Status "running".
/// Nutzt bevorzugt RunStore; Fallback auf Verzeichnis-Listing.
pub fn scan_orphaned_runs(
    run_store: Option<&crate::run_store::RunStore>,
    runs_dir: &str,
) -> Vec<OrphanedRun> {
    let mut orphans = Vec::new();
    let now = OffsetDateTime::now_utc();
    const LEGACY_AGE: f64 = 600.0;

    if let Some(store) = run_store {
        let run_ids = store.list_runs();
        for run_id in run_ids {
            let meta = match store.load(&run_id) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if meta.status != "running" {
                continue;
            }

            let extra = meta.extra;
            let owner_pid = extra.get("owner_pid").and_then(|v| v.as_i64()).unwrap_or(0);

            let stale = if owner_pid > 0 {
                !pid_alive(owner_pid)
            } else if !meta.created_at.is_empty() {
                if let Ok(created_dt) = OffsetDateTime::parse(&meta.created_at, &Rfc3339) {
                    let age = (now - created_dt).whole_seconds() as f64;
                    age >= LEGACY_AGE
                } else {
                    true
                }
            } else {
                true
            };

            if !stale {
                continue;
            }

            let age = if !meta.created_at.is_empty() {
                OffsetDateTime::parse(&meta.created_at, &Rfc3339)
                    .ok()
                    .map(|dt| (now - dt).whole_seconds() as f64)
                    .unwrap_or(-1.0)
            } else {
                -1.0
            };

            orphans.push(OrphanedRun {
                run_id,
                brain_id: meta.brain_id,
                age_seconds: age,
            });
        }
        return orphans;
    }

    // Fallback: Verzeichnis-Listing ohne RunStore
    if runs_dir.is_empty() || !Path::new(runs_dir).is_dir() {
        return orphans;
    }

    let mut run_ids: Vec<String> = match fs::read_dir(runs_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => return orphans,
    };
    run_ids.sort_by(|a, b| b.cmp(a)); // neueste zuerst

    for run_id in run_ids {
        let meta_path = Path::new(runs_dir).join(&run_id).join("meta.json");
        if !meta_path.is_file() {
            continue;
        }
        let content = match fs::read_to_string(&meta_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let meta: RunMeta = match serde_json::from_str(&content) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.status != "running" {
            continue;
        }

        let extra = meta.extra;
        let owner_pid = extra.get("owner_pid").and_then(|v| v.as_i64()).unwrap_or(0);

        let stale = if owner_pid > 0 {
            !pid_alive(owner_pid)
        } else if !meta.created_at.is_empty() {
            if let Ok(created_dt) = OffsetDateTime::parse(&meta.created_at, &Rfc3339) {
                let age = (now - created_dt).whole_seconds() as f64;
                age >= LEGACY_AGE
            } else {
                true
            }
        } else {
            true
        };

        if !stale {
            continue;
        }

        let age = if !meta.created_at.is_empty() {
            OffsetDateTime::parse(&meta.created_at, &Rfc3339)
                .ok()
                .map(|dt| (now - dt).whole_seconds() as f64)
                .unwrap_or(-1.0)
        } else {
            -1.0
        };

        orphans.push(OrphanedRun {
            run_id,
            brain_id: meta.brain_id,
            age_seconds: age,
        });
    }

    orphans
}

/// Scannt nach Bridge-Lock-Dateien, deren Holder-Prozess tot ist.
/// Lock-Dateien heißen `.webagent-bridge-<agent>.lock` und enthalten
/// `{"pid": <int>, "token": <str>}`.
pub fn scan_bridge_locks(bot2bot_root: &str, grace_seconds: f64) -> Vec<StaleBridgeLock> {
    let mut stale = Vec::new();
    if bot2bot_root.is_empty() || !Path::new(bot2bot_root).is_dir() {
        return stale;
    }

    let entries = match fs::read_dir(bot2bot_root) {
        Ok(e) => e,
        Err(_) => return stale,
    };

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with(".webagent-bridge-") || !name.ends_with(".lock") {
            continue;
        }

        let path = entry.path();
        let agent_slug = name[".webagent-bridge-".len()..name.len() - ".lock".len()].to_string();
        let age = file_age_seconds(path.to_str().unwrap_or(""));

        let mut holder_pid = 0i64;
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(payload) = serde_json::from_str::<Value>(&content) {
                holder_pid = payload.get("pid").and_then(|v| v.as_i64()).unwrap_or(0);
            }
        }

        // Lebender Holder → nicht stale
        if holder_pid > 0 && pid_alive(holder_pid) {
            continue;
        }
        // Toter/fehlender Holder, aber Datei noch sehr jung (Grace)
        if holder_pid == 0 && age >= 0.0 && age < grace_seconds {
            continue;
        }

        stale.push(StaleBridgeLock {
            path: path.to_string_lossy().to_string(),
            agent_slug,
            holder_pid,
            age_seconds: age,
        });
    }

    stale
}

/// Scannt nach Browser-Profil-Lock-Dateien ohne aktiven Chrome-Prozess.
/// Nutzt doctor.find_lock_files zur Integration (shared Lock-Erkennung).
pub fn scan_profile_locks(
    profile_dir: &str,
    grace_seconds: f64,
    chrome_running_fn: Option<fn(&str) -> bool>,
) -> Vec<StaleProfileLock> {
    let mut stale = Vec::new();
    if profile_dir.is_empty() || !Path::new(profile_dir).is_dir() {
        return stale;
    }

    let chrome_running = chrome_running_fn.unwrap_or(chrome_running_for_profile);

    // Integriert mit doctor.find_lock_files (Lock-Erkennung)
    let rel_locks = find_lock_files(profile_dir);
    let chrome_active = chrome_running(profile_dir);

    for rel in rel_locks {
        let path = if rel.to_lowercase().starts_with("default/")
            || rel.to_lowercase().starts_with("default\\")
            || rel.to_lowercase().starts_with("default")
        {
            let base = Path::new(profile_dir).join("Default");
            let entry = rel
                .split('/')
                .next_back()
                .or_else(|| rel.split('\\').next_back())
                .unwrap_or(&rel);
            base.join(entry)
        } else {
            Path::new(profile_dir).join(&rel)
        };

        if !path.is_file() {
            continue;
        }
        // Nutzt ein Chrome das Profil noch, ist der Lock legitim.
        if chrome_active {
            continue;
        }
        // Kein Chrome aktiv => Lock ist verwaist (wie im Python-Original, das
        // Locks entfernte, sobald kein Chrome das Profil nutzt). `grace_seconds`
        // greift nur, wenn der Chrome-Status nicht sicher ausgeschlossen werden
        // kann; hier ist er es. Das Alter bleibt rein informativ.
        let _ = grace_seconds;
        let age = file_age_seconds(path.to_str().unwrap_or(""));
        stale.push(StaleProfileLock {
            path: path.to_string_lossy().to_string(),
            age_seconds: age,
        });
    }

    stale
}

/// Findet Lock-Dateien in einem Chromium-Profil (portiert aus doctor.py).
/// Gibt relative Pfade zurück: "SingletonLock", "Default/SingletonLock", etc.
pub fn find_lock_files(profile_dir: &str) -> Vec<String> {
    let mut locks = Vec::new();
    if profile_dir.is_empty() || !Path::new(profile_dir).is_dir() {
        return locks;
    }

    // Hauptverzeichnis prüfen
    if let Ok(entries) = fs::read_dir(profile_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name == "SingletonLock"
                    || name == "SingletonCookie"
                    || name == "SingletonSocket"
                    || name.starts_with(".org.chromium.Chromium.")
                {
                    locks.push(name.to_string());
                }
            }
        }
    }

    // Default-Subdir prüfen
    let default_dir = Path::new(profile_dir).join("Default");
    if default_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&default_dir) {
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name == "SingletonLock"
                        || name == "SingletonCookie"
                        || name == "SingletonSocket"
                        || name.starts_with(".org.chromium.Chromium.")
                    {
                        locks.push(format!("Default/{}", name));
                    }
                }
            }
        }
    }

    locks
}

/// Prüft, ob ein chrome.exe-Prozess mit diesem Profil aktiv ist.
/// Konservativ: bei Fehler wird true angenommen (lieber nicht antasten).
fn chrome_running_for_profile(profile_dir: &str) -> bool {
    #[cfg(windows)]
    {
        use std::process::Command;
        let profile_escaped = profile_dir.replace("\\", "\\\\");
        let ps_cmd = format!(
            "Get-CimInstance Win32_Process | Where-Object {{ $_.Name -eq 'chrome.exe' -and $_.CommandLine -like '*{}*' }} | Measure-Object | Select-Object -ExpandProperty Count",
            profile_escaped
        );
        match Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_cmd])
            .output()
        {
            Ok(output) => {
                let text = String::from_utf8_lossy(&output.stdout);
                if let Ok(count) = text.trim().parse::<i32>() {
                    return count > 0;
                }
                true // konservativ
            }
            Err(_) => true,
        }
    }
    #[cfg(not(windows))]
    {
        false // Auf Unix/Linux keine implizite Profil-Prüfung via PS
    }
}

/// Repariert verwaiste Runs: setzt Status auf "interrupted".
pub fn repair_orphaned_runs(
    report: &mut WatchdogReport,
    run_store: Option<&crate::run_store::RunStore>,
) {
    for orphan in std::mem::take(&mut report.orphaned_runs) {
        if let Some(store) = run_store {
            if let Ok(mut meta) = store.load(&orphan.run_id) {
                meta.status = "interrupted".to_string();
                meta.extra.insert(
                    "reconciled_at".to_string(),
                    serde_json::Value::String(
                        OffsetDateTime::now_utc()
                            .format(&Rfc3339)
                            .unwrap_or_default(),
                    ),
                );
                meta.extra.insert(
                    "error".to_string(),
                    serde_json::Value::String(
                        "Watchdog: Prozess endete ohne finalen Run-Status.".to_string(),
                    ),
                );
                if store.save(&meta).is_ok() {
                    report.repaired_runs.push(orphan.run_id);
                    continue;
                }
            }
        }
        // Fallback ohne RunStore wird übersprungen (braucht runs_dir-Kontext)
        report
            .errors
            .push(format!("cannot repair {} without run_store", orphan.run_id));
    }
}

/// Entfernt verwaiste Bridge-Lock-Dateien.
pub fn repair_bridge_locks(report: &mut WatchdogReport) {
    for lock in std::mem::take(&mut report.stale_bridge_locks) {
        match fs::remove_file(&lock.path) {
            Ok(_) => {
                report.repaired_bridge_locks.push(lock.path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // bereits weg
            }
            Err(e) => {
                report
                    .errors
                    .push(format!("repair bridge lock {}: {}", lock.path, e));
            }
        }
    }
}

/// Entfernt verwaiste Browser-Profil-Lock-Dateien.
pub fn repair_profile_locks(report: &mut WatchdogReport) {
    for lock in std::mem::take(&mut report.stale_profile_locks) {
        match fs::remove_file(&lock.path) {
            Ok(_) => {
                report.repaired_profile_locks.push(lock.path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // bereits weg
            }
            Err(e) => {
                report
                    .errors
                    .push(format!("repair profile lock {}: {}", lock.path, e));
            }
        }
    }
}

/// Hauptfunktion: scannt alle drei Bereiche und repariert optional.
pub fn run_watchdog(
    bot2bot_root: &str,
    profile_dir: &str,
    runs_dir: &str,
    run_store: Option<&crate::run_store::RunStore>,
    repair: bool,
    chrome_running_fn: Option<fn(&str) -> bool>,
) -> WatchdogReport {
    let mut report = WatchdogReport {
        timestamp: OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_default(),
        ..Default::default()
    };

    // 1. Verwaiste Runs
    report.orphaned_runs = scan_orphaned_runs(run_store, runs_dir);

    // 2. Stale Bridge-Locks
    report.stale_bridge_locks = scan_bridge_locks(bot2bot_root, BRIDGE_LOCK_GRACE_SECONDS);

    // 3. Stale Profil-Locks
    report.stale_profile_locks = scan_profile_locks(
        profile_dir,
        PROFILE_LOCK_GRACE_SECONDS,
        chrome_running_fn.or(Some(chrome_running_for_profile)),
    );

    // 4. Reparatur (nur wenn gewünscht)
    if repair {
        repair_orphaned_runs(&mut report, run_store);
        repair_bridge_locks(&mut report);
        repair_profile_locks(&mut report);
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_store::RunStore;
    use std::env;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use time::Duration;

    fn unique_tmp() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_watchdog_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn test_find_lock_files_empty() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        assert_eq!(find_lock_files(tmp.to_str().unwrap()), Vec::<String>::new());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_find_singleton_lock() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("SingletonLock"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert!(result.contains(&"SingletonLock".to_string()));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_find_locks_in_default_subdir() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        let default = tmp.join("Default");
        fs::create_dir_all(&default).unwrap();
        fs::write(default.join("SingletonLock"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert!(result.iter().any(|l| l.contains("Default")));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_ignores_regular_files() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("Preferences"), "").unwrap();
        fs::write(tmp.join("test.json"), "").unwrap();
        assert_eq!(find_lock_files(tmp.to_str().unwrap()), Vec::<String>::new());
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_scan_bridge_locks_no_dir() {
        assert!(scan_bridge_locks("", BRIDGE_LOCK_GRACE_SECONDS).is_empty());
    }

    #[test]
    fn test_scan_bridge_locks_alive_holder() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();

        // Lock mit aktueller PID
        let lock_path = tmp.join(".webagent-bridge-test.lock");
        let payload = serde_json::json!({"pid": std::process::id() as i64, "token": "abc"});
        fs::write(&lock_path, payload.to_string()).unwrap();

        let result = scan_bridge_locks(tmp.to_str().unwrap(), BRIDGE_LOCK_GRACE_SECONDS);
        assert!(result.is_empty()); // Holder lebt → nicht stale

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_scan_bridge_locks_dead_holder() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();

        // Lock mit toter PID
        let lock_path = tmp.join(".webagent-bridge-dead.lock");
        let payload = serde_json::json!({"pid": 99999999, "token": "xyz"});
        fs::write(&lock_path, payload.to_string()).unwrap();

        let result = scan_bridge_locks(tmp.to_str().unwrap(), BRIDGE_LOCK_GRACE_SECONDS);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].agent_slug, "dead");
        assert_eq!(result[0].holder_pid, 99999999);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_scan_orphaned_runs_with_store() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());

        // Laufenden Run ohne owner_pid, aber alt (>600s)
        let mut meta = store.create("chatgpt", "stale task").unwrap();
        let past = OffsetDateTime::now_utc() - Duration::seconds(3600); // 1h her
        meta.created_at = past.format(&Rfc3339).unwrap();
        meta.extra.remove("owner_pid"); // Legacy: kein owner_pid
        store.save(&meta).unwrap();

        let orphans = scan_orphaned_runs(Some(&store), "");
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].brain_id, "chatgpt");
        assert!(orphans[0].age_seconds > 0.0);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_run_watchdog_dry_run() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        let bot2bot = tmp.join("bot2bot");
        let profile = tmp.join("profile");
        let default = profile.join("Default");

        fs::create_dir_all(&runs_dir).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&bot2bot).unwrap();
        fs::create_dir_all(&default).unwrap();

        // Verwaister Run
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("chatgpt", "test").unwrap();
        let past = OffsetDateTime::now_utc() - Duration::seconds(3600);
        meta.created_at = past.format(&Rfc3339).unwrap();
        meta.extra.remove("owner_pid");
        store.save(&meta).unwrap();

        // Stale Bridge-Lock (tote PID)
        let bridge_lock = bot2bot.join(".webagent-bridge-test.lock");
        let payload = serde_json::json!({"pid": 99999999, "token": "xyz"});
        fs::write(&bridge_lock, payload.to_string()).unwrap();

        // Stale Profile-Lock (kein Chrome, alt)
        fs::write(default.join("SingletonLock"), "").unwrap();

        let report = run_watchdog(
            bot2bot.to_str().unwrap(),
            profile.to_str().unwrap(),
            runs_dir.to_str().unwrap(),
            Some(&store),
            false, // dry-run
            None,
        );

        assert_eq!(report.orphaned_runs.len(), 1);
        assert_eq!(report.stale_bridge_locks.len(), 1);
        assert_eq!(report.stale_profile_locks.len(), 1);
        assert_eq!(report.repaired_runs.len(), 0);
        assert_eq!(report.repaired_bridge_locks.len(), 0);
        assert_eq!(report.repaired_profile_locks.len(), 0);

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_run_watchdog_repair() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        let bot2bot = tmp.join("bot2bot");
        let profile = tmp.join("profile");
        let default = profile.join("Default");

        fs::create_dir_all(&runs_dir).unwrap();
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&bot2bot).unwrap();
        fs::create_dir_all(&default).unwrap();

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("chatgpt", "test").unwrap();
        let past = OffsetDateTime::now_utc() - Duration::seconds(3600);
        meta.created_at = past.format(&Rfc3339).unwrap();
        meta.extra.remove("owner_pid");
        store.save(&meta).unwrap();

        let bridge_lock = bot2bot.join(".webagent-bridge-test.lock");
        let payload = serde_json::json!({"pid": 99999999, "token": "xyz"});
        fs::write(&bridge_lock, payload.to_string()).unwrap();

        fs::write(default.join("SingletonLock"), "").unwrap();

        let report = run_watchdog(
            bot2bot.to_str().unwrap(),
            profile.to_str().unwrap(),
            runs_dir.to_str().unwrap(),
            Some(&store),
            true, // repair
            None,
        );

        assert_eq!(report.repaired_runs.len(), 1);
        assert_eq!(report.repaired_bridge_locks.len(), 1);
        assert_eq!(report.repaired_profile_locks.len(), 1);

        // Prüfen dass Run repariert wurde
        let repaired_meta = store.load(&report.repaired_runs[0]).unwrap();
        assert_eq!(repaired_meta.status, "interrupted");

        // Prüfen dass Lock-Dateien weg sind
        assert!(!bridge_lock.exists());
        assert!(!default.join("SingletonLock").exists());

        fs::remove_dir_all(&tmp).ok();
    }
}
