//! Doctor-Check pro Brain — Phase 1: Login-Zustand, Selektor, Profil-Lock, letzte Antwort, Recovery.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::run_store::RunMeta;

type RunMetaLoader<'a> = dyn Fn(&str) -> Option<RunMeta> + 'a;

/// Ergebnis des Doctor-Checks für ein einzelnes Gehirn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrainCheck {
    pub brain_id: String,
    pub selectors_ok: bool,
    #[serde(default)]
    pub selectors_path: String,
    #[serde(default)]
    pub selectors_mtime: String,
    #[serde(default)]
    pub profile_dir: String,
    pub profile_exists: bool,
    #[serde(default)]
    pub profile_lock_files: Vec<String>,
    #[serde(default)]
    pub last_done_run: String,
    #[serde(default)]
    pub last_done_run_age_hours: f64,
    #[serde(default)]
    pub login_state: String,
    #[serde(default)]
    pub recovery_hint: String,
}

impl BrainCheck {
    pub fn healthy(&self) -> bool {
        self.selectors_ok
            && self.profile_exists
            && self.profile_lock_files.is_empty()
            && self.login_state != "error"
            && self.login_state != "login_required"
    }
}

impl Default for BrainCheck {
    fn default() -> Self {
        Self {
            brain_id: String::new(),
            selectors_ok: false,
            selectors_path: String::new(),
            selectors_mtime: String::new(),
            profile_dir: String::new(),
            profile_exists: false,
            profile_lock_files: Vec::new(),
            last_done_run: String::new(),
            last_done_run_age_hours: -1.0,
            login_state: "unknown".to_string(),
            recovery_hint: String::new(),
        }
    }
}

/// Aggregierter Doctor-Bericht über alle geprüften Gehirne.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoctorReport {
    pub timestamp: String,
    #[serde(default)]
    pub brains: HashMap<String, BrainCheck>,
}

impl DoctorReport {
    pub fn healthy_brain_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .brains
            .iter()
            .filter(|(_, bc)| bc.healthy())
            .map(|(bid, _)| bid.clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn unhealthy_brain_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .brains
            .iter()
            .filter(|(_, bc)| !bc.healthy())
            .map(|(bid, _)| bid.clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn ok(&self) -> bool {
        self.unhealthy_brain_ids().is_empty()
    }
}

/// ISO-8601 mtime einer Datei, leerer String wenn nicht lesbar.
fn stat_mtime(path: &str) -> String {
    if let Ok(metadata) = fs::metadata(path) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                let secs = duration.as_secs() as i64;
                let (y, mo, d, h, mi, s) = crate::civil_utc(secs);
                return format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, mi, s);
            }
        }
    }
    String::new()
}

/// Singleton-Lock-Files eines Chromium-Profils finden.
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

/// Letzten abgeschlossenen Run für ein Brain finden.
/// Gibt (run_id, alter_in_stunden) zurück. (-1.0) wenn kein Run gefunden.
pub fn find_last_done_run(
    runs_dir: &str,
    brain_id: &str,
    list_runs_fn: Option<&dyn Fn() -> Vec<String>>,
    load_fn: Option<&RunMetaLoader<'_>>,
) -> (String, f64) {
    let all_runs = if let Some(list_fn) = list_runs_fn {
        list_fn()
    } else {
        if runs_dir.is_empty() || !Path::new(runs_dir).is_dir() {
            return (String::new(), -1.0);
        }
        let mut runs = Vec::new();
        if let Ok(entries) = fs::read_dir(runs_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        runs.push(name.to_string());
                    }
                }
            }
        }
        runs.sort_by(|a, b| b.cmp(a)); // neueste zuerst
        runs
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for run_id in all_runs {
        if let Some(load_fn) = load_fn {
            if let Some(meta) = load_fn(&run_id) {
                if meta.brain_id == brain_id && meta.status == "done" {
                    let age_hours = calculate_age_hours(&meta.created_at, now);
                    return (run_id, age_hours);
                }
            }
        } else {
            // Fallback: meta.json direkt lesen
            let meta_path = Path::new(runs_dir).join(&run_id).join("meta.json");
            if let Ok(content) = fs::read_to_string(&meta_path) {
                if let Ok(meta) = serde_json::from_str::<RunMeta>(&content) {
                    if meta.brain_id == brain_id && meta.status == "done" {
                        let age_hours = calculate_age_hours(&meta.created_at, now);
                        return (run_id, age_hours);
                    }
                }
            }
        }
    }

    (String::new(), -1.0)
}

/// Most recent run (any status) for the brain. Returns (run_id, meta_dict_or_None, age_hours).
pub fn find_recent_run_meta(
    runs_dir: &str,
    brain_id: &str,
    list_runs_fn: Option<&dyn Fn() -> Vec<String>>,
    load_fn: Option<&RunMetaLoader<'_>>,
) -> (String, Option<HashMap<String, Value>>, f64) {
    let all_runs = if let Some(list_fn) = list_runs_fn {
        list_fn()
    } else {
        if runs_dir.is_empty() || !Path::new(runs_dir).is_dir() {
            return (String::new(), None, -1.0);
        }
        let mut runs = Vec::new();
        if let Ok(entries) = fs::read_dir(runs_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        runs.push(name.to_string());
                    }
                }
            }
        }
        runs.sort_by(|a, b| b.cmp(a));
        runs
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    for run_id in all_runs {
        let meta_opt = if let Some(load_fn) = load_fn {
            load_fn(&run_id)
        } else {
            let meta_path = Path::new(runs_dir).join(&run_id).join("meta.json");
            fs::read_to_string(&meta_path)
                .ok()
                .and_then(|content| serde_json::from_str::<RunMeta>(&content).ok())
        };

        if let Some(meta) = meta_opt {
            if meta.brain_id == brain_id {
                let age_hours = calculate_age_hours(&meta.created_at, now);
                let mut map = HashMap::new();
                map.insert("run_id".to_string(), Value::String(meta.run_id.clone()));
                map.insert("brain_id".to_string(), Value::String(meta.brain_id.clone()));
                map.insert("status".to_string(), Value::String(meta.status.clone()));
                map.insert(
                    "created_at".to_string(),
                    Value::String(meta.created_at.clone()),
                );
                map.insert(
                    "extra".to_string(),
                    serde_json::to_value(&meta.extra).unwrap_or(Value::Null),
                );
                return (run_id, Some(map), age_hours);
            }
        }
    }

    (String::new(), None, -1.0)
}

/// Infer better login_state read-only from last_done + recent meta + transcript session_state lines.
pub fn infer_login_state(
    last_done_run: &str,
    last_done_run_age_hours: f64,
    runs_dir: &str,
    brain_id: &str,
    list_runs_fn: Option<&dyn Fn() -> Vec<String>>,
    load_fn: Option<&RunMetaLoader<'_>>,
) -> String {
    if !last_done_run.is_empty() && (0.0..48.0).contains(&last_done_run_age_hours) {
        return "ready".to_string();
    }

    let (run_id, meta_opt, age) = find_recent_run_meta(runs_dir, brain_id, list_runs_fn, load_fn);

    if let Some(meta) = meta_opt {
        let status = meta.get("status").and_then(|v| v.as_str()).unwrap_or("");

        if status == "login_required" {
            return "login_required".to_string();
        }
        if status == "done" && (0.0..48.0).contains(&age) {
            return "ready".to_string();
        }
        if status == "done" && age >= 48.0 {
            return "stale".to_string();
        }

        // scan recent transcript lines for session_state
        if !run_id.is_empty() {
            let trans_path = Path::new(runs_dir).join(&run_id).join("transcript.jsonl");
            if let Ok(content) = fs::read_to_string(&trans_path) {
                let lines: Vec<&str> = content.lines().collect();
                let tail: Vec<&str> = lines.iter().rev().take(30).copied().collect();

                for line in tail {
                    if line.contains("session_state=ready") {
                        return if age < 0.0 || age < 48.0 {
                            "ready".to_string()
                        } else {
                            "stale".to_string()
                        };
                    }
                    if line.contains("session_state=login_required") {
                        return "login_required".to_string();
                    }
                    if line.contains("session_state=cloudflare") {
                        return "cloudflare".to_string();
                    }
                    if line.contains("session_state=error") {
                        return "error".to_string();
                    }
                }
            }
        }

        if age > 72.0 {
            return "unknown (old)".to_string();
        }
        if (status == "brain_incomplete" || status == "interrupted" || status == "max_cycles")
            && (0.0..48.0).contains(&age)
        {
            return "likely_ready".to_string();
        }
    }

    if !last_done_run.is_empty() && last_done_run_age_hours >= 48.0 {
        return "stale".to_string();
    }
    if !last_done_run.is_empty() && last_done_run_age_hours > 0.0 {
        return "likely_ready".to_string();
    }
    "unknown".to_string()
}

/// Recovery-Empfehlung basierend auf dem Check-Ergebnis.
pub fn build_recovery_hint(check: &BrainCheck) -> String {
    let mut hints = Vec::new();

    if !check.selectors_ok {
        hints.push(format!(
            "Selektor-Datei fehlt ({}). Führe 'webagent diagnose --brain {}' aus.",
            check.selectors_path, check.brain_id
        ));
    }
    if !check.profile_exists {
        hints.push(format!(
            "Profil-Verzeichnis fehlt ({}). Führe 'webagent login --brain {}' aus.",
            check.profile_dir, check.brain_id
        ));
    }
    if !check.profile_lock_files.is_empty() {
        let locks_str = check.profile_lock_files.join(", ");
        hints.push(format!(
            "Lock-Dateien vorhanden ({}). Chromium-Prozesse schliessen und Locks manuell entfernen, \
             oder WebAgent neu starten (cleanup geschieht automatisch).",
            locks_str
        ));
    }
    if check.login_state == "login_required" {
        hints.push(format!(
            "Login-Zustand: login_required. Führe 'webagent login --brain {}' aus.",
            check.brain_id
        ));
    } else if check.login_state == "stale" || check.login_state == "unknown (old)" {
        hints.push(format!(
            "Login-Zustand stale/alt ({:.0}h). Login prüfen: 'webagent login --brain {}' oder neuen Run starten.",
            check.last_done_run_age_hours, check.brain_id
        ));
    } else if check.login_state == "unknown" {
        hints.push(format!(
            "Login-Zustand unbekannt. Fuehre 'webagent diagnose --brain {}' aus (startet Browser, prueft Session).",
            check.brain_id
        ));
    }

    if check.last_done_run_age_hours < 0.0 && !check.profile_exists {
        // kein Run ist normal wenn Profil fehlt
    } else if check.last_done_run_age_hours < 0.0 {
        hints.push(format!(
            "Kein erfolgreicher Run gefunden. Fuehre einen Test-Run durch: 'webagent run --brain {} --task \"echo hello\"'",
            check.brain_id
        ));
    } else if check.last_done_run_age_hours > 48.0
        && check.login_state != "ready"
        && check.login_state != "likely_ready"
    {
        hints.push(format!(
            "Letzter erfolgreicher Run ist {:.0}h alt ({}). Login moeglicherweise abgelaufen.",
            check.last_done_run_age_hours, check.last_done_run
        ));
    }

    hints.join("; ")
}

/// Führt alle read-only Checks für ein einzelnes Gehirn durch.
pub fn check_brain(
    brain_id: &str,
    brains_config: Option<&HashMap<String, HashMap<String, String>>>,
    runs_dir: &str,
    profile_dir: &str,
    list_runs_fn: Option<&dyn Fn() -> Vec<String>>,
    load_fn: Option<&RunMetaLoader<'_>>,
) -> BrainCheck {
    let config = if let Some(cfg) = brains_config {
        cfg
    } else {
        // Fallback: würde normalerweise crate::config::BRAINS nutzen
        // Für Tests übergeben wir explizit
        return BrainCheck {
            brain_id: brain_id.to_string(),
            recovery_hint: format!("Unbekanntes Brain: {}", brain_id),
            ..Default::default()
        };
    };

    let spec = match config.get(brain_id) {
        Some(s) => s,
        None => {
            return BrainCheck {
                brain_id: brain_id.to_string(),
                recovery_hint: format!("Unbekanntes Brain: {}", brain_id),
                ..Default::default()
            };
        }
    };

    let sel_path = spec.get("selectors").map(|s| s.as_str()).unwrap_or("");
    let p_dir = if !profile_dir.is_empty() {
        profile_dir
    } else {
        spec.get("profile_dir")
            .or_else(|| spec.get("profile"))
            .map(|s| s.as_str())
            .unwrap_or("")
    };

    let mut check = BrainCheck {
        brain_id: brain_id.to_string(),
        selectors_ok: !sel_path.is_empty() && Path::new(sel_path).is_file(),
        selectors_path: sel_path.to_string(),
        selectors_mtime: if !sel_path.is_empty() {
            stat_mtime(sel_path)
        } else {
            String::new()
        },
        profile_dir: p_dir.to_string(),
        profile_exists: !p_dir.is_empty() && Path::new(p_dir).is_dir(),
        profile_lock_files: find_lock_files(p_dir),
        ..Default::default()
    };

    let (last_run, last_age) = find_last_done_run(runs_dir, brain_id, list_runs_fn, load_fn);
    check.last_done_run = last_run.clone();
    check.last_done_run_age_hours = last_age;
    check.login_state = infer_login_state(
        &last_run,
        last_age,
        runs_dir,
        brain_id,
        list_runs_fn,
        load_fn,
    );
    check.recovery_hint = build_recovery_hint(&check);

    check
}

/// Führt Doctor-Checks für alle (oder ausgewählte) Gehirne durch.
pub fn run_doctor(
    brain_ids: Option<Vec<String>>,
    brains_config: Option<&HashMap<String, HashMap<String, String>>>,
    runs_dir: &str,
    list_runs_fn: Option<&dyn Fn() -> Vec<String>>,
    load_fn: Option<&RunMetaLoader<'_>>,
) -> DoctorReport {
    let config = brains_config.unwrap_or_else(|| {
        // Fallback: würde normalerweise crate::config::BRAINS nutzen
        panic!("brains_config required");
    });

    let brain_list = if let Some(ids) = brain_ids {
        ids
    } else {
        let mut ids: Vec<String> = config.keys().cloned().collect();
        ids.sort();
        ids
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let (y, mo, d, h, mi, s) = crate::civil_utc(now);
    let timestamp = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, mo, d, h, mi, s);

    let mut report = DoctorReport {
        timestamp,
        brains: HashMap::new(),
    };

    for bid in brain_list {
        let check = check_brain(&bid, Some(config), runs_dir, "", list_runs_fn, load_fn);
        report.brains.insert(bid, check);
    }

    report
}

/// Hilfsfunktion: Alter in Stunden berechnen.
fn calculate_age_hours(created_at: &str, now_secs: i64) -> f64 {
    // ISO-8601 parsen
    let created_str = if created_at.ends_with('Z') {
        created_at.replace('Z', "+00:00")
    } else {
        created_at.to_string()
    };

    // Einfaches Parsing: YYYY-MM-DDTHH:MM:SS.ffffff+00:00
    if let Some(dt_part) = created_str.split('+').next() {
        if let Some(date_time) = dt_part.split('T').collect::<Vec<_>>().get(0..2) {
            let date = date_time[0];
            let time = date_time[1].split('.').next().unwrap_or(date_time[1]);

            let date_parts: Vec<&str> = date.split('-').collect();
            let time_parts: Vec<&str> = time.split(':').collect();

            if date_parts.len() == 3 && time_parts.len() == 3 {
                if let (Ok(y), Ok(mo), Ok(d), Ok(h), Ok(mi), Ok(s)) = (
                    date_parts[0].parse::<i64>(),
                    date_parts[1].parse::<u32>(),
                    date_parts[2].parse::<u32>(),
                    time_parts[0].parse::<u32>(),
                    time_parts[1].parse::<u32>(),
                    time_parts[2].parse::<u32>(),
                ) {
                    // Vereinfachte Umrechnung in Unix-Sekunden
                    let created_secs = civil_to_unix(y, mo, d, h, mi, s);
                    let age_secs = now_secs - created_secs;
                    return age_secs as f64 / 3600.0;
                }
            }
        }
    }
    -1.0
}

/// Umrechnung von civil time (UTC) zu Unix-Sekunden — exakte Umkehrung von
/// `crate::civil_utc` nach Howard Hinnant (days_from_civil). `mo` ist 1-basiert.
fn civil_to_unix(y: i64, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> i64 {
    let m = mo as i64;
    // Jan/Feb zählen zum Vorjahr (Schaltjahr-Randbehandlung).
    let year = y - if m <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h as i64 * 3600 + mi as i64 * 60 + s as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_tmp() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_doctor_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    // --- find_lock_files ---

    #[test]
    fn test_empty_dir() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        assert_eq!(find_lock_files(tmp.to_str().unwrap()), Vec::<String>::new());
    }

    #[test]
    fn test_singleton_lock() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("SingletonLock"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert!(result.contains(&"SingletonLock".to_string()));
    }

    #[test]
    fn test_singleton_cookie_and_socket() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("SingletonCookie"), "").unwrap();
        fs::write(tmp.join("SingletonSocket"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert!(result.contains(&"SingletonCookie".to_string()));
        assert!(result.contains(&"SingletonSocket".to_string()));
    }

    #[test]
    fn test_chromium_random_socket() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join(".org.chromium.Chromium.abc123"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("Chromium"));
    }

    #[test]
    fn test_ignores_regular_files() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        fs::create_dir_all(tmp.join("Default")).unwrap();
        fs::write(tmp.join("Preferences"), "").unwrap();
        fs::write(tmp.join("test.json"), "").unwrap();
        assert_eq!(find_lock_files(tmp.to_str().unwrap()), Vec::<String>::new());
    }

    #[test]
    fn test_locks_in_default_subdir() {
        let tmp = unique_tmp();
        fs::create_dir_all(&tmp).unwrap();
        let default = tmp.join("Default");
        fs::create_dir_all(&default).unwrap();
        fs::write(default.join("SingletonLock"), "").unwrap();
        let result = find_lock_files(tmp.to_str().unwrap());
        assert!(result.iter().any(|l| l.contains("Default")));
    }

    #[test]
    fn test_nonexistent_dir() {
        assert_eq!(find_lock_files("/nonexistent/path"), Vec::<String>::new());
    }

    // --- find_last_done_run ---

    #[test]
    fn test_no_runs_dir() {
        let (run_id, age) = find_last_done_run("", "chatgpt", None, None);
        assert_eq!(run_id, "");
        assert_eq!(age, -1.0);
    }

    #[test]
    fn test_finds_done_run() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_120000_aabbccdd");
        fs::create_dir_all(&run_dir).unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_secs = now - 3600; // 1 Stunde her
        let (y, mo, d, h, mi, s) = crate::civil_utc(past_secs);
        let past = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
            y, mo, d, h, mi, s
        );

        let meta = RunMeta {
            run_id: "20260712_120000_aabbccdd".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "done".to_string(),
            created_at: past,
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let (run_id, age) = find_last_done_run(runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(run_id, "20260712_120000_aabbccdd");
        assert!(age >= 0.0);
    }

    #[test]
    fn test_skips_failed_run() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_120000_aabbccdd");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260712_120000_aabbccdd".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "failed".to_string(),
            created_at: "2026-07-12T12:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let (run_id, _) = find_last_done_run(runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(run_id, "");
    }

    #[test]
    fn test_skips_wrong_brain() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_120000_aabbccdd");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260712_120000_aabbccdd".to_string(),
            brain_id: "claude".to_string(),
            status: "done".to_string(),
            created_at: "2026-07-12T12:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let (run_id, _) = find_last_done_run(runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(run_id, "");
    }

    #[test]
    fn test_broken_meta_json() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_120000_aabbccdd");
        fs::create_dir_all(&run_dir).unwrap();
        fs::write(run_dir.join("meta.json"), "not json").unwrap();

        let (run_id, _) = find_last_done_run(runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(run_id, "");
    }

    // --- find_recent_run_meta and infer_login_state ---

    #[test]
    fn test_recent_meta_finds_any_status_not_just_done() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_120000_aabbccdd");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260712_120000_aabbccdd".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "login_required".to_string(),
            created_at: "2026-07-12T11:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let (rid, m, _age) =
            find_recent_run_meta(runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(rid, "20260712_120000_aabbccdd");
        assert!(m.is_some());
        assert_eq!(
            m.unwrap().get("status").and_then(|v| v.as_str()),
            Some("login_required")
        );
    }

    #[test]
    fn test_infer_ready_from_recent_done() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_090000_xxx");
        fs::create_dir_all(&run_dir).unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_secs = now - 7200; // 2 Stunden her
        let (y, mo, d, h, mi, s) = crate::civil_utc(past_secs);
        let past = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
            y, mo, d, h, mi, s
        );

        let meta = RunMeta {
            run_id: "20260712_090000_xxx".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "done".to_string(),
            created_at: past,
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let state = infer_login_state(
            "20260712_090000_xxx",
            2.0,
            runs_dir.to_str().unwrap(),
            "chatgpt",
            None,
            None,
        );
        assert_eq!(state, "ready");
    }

    #[test]
    fn test_infer_login_required_from_meta() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260712_100000_yyy");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260712_100000_yyy".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "login_required".to_string(),
            created_at: "2026-07-12T10:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let state = infer_login_state("", -1.0, runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(state, "login_required");
    }

    #[test]
    fn test_infer_from_transcript_session_state() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260714_080000_zzz");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260714_080000_zzz".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "brain_incomplete".to_string(),
            created_at: "2026-07-14T08:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
        fs::write(
            run_dir.join("transcript.jsonl"),
            r#"{"ts":"...","role":"system","content":"session_state=ready"}"#,
        )
        .unwrap();

        let state = infer_login_state("", -1.0, runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(state, "ready");
    }

    #[test]
    fn test_infer_stale_for_old_done() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260709_120000_old");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260709_120000_old".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "done".to_string(),
            created_at: "2026-07-09T12:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        let state = infer_login_state(
            "20260709_120000_old",
            72.0,
            runs_dir.to_str().unwrap(),
            "chatgpt",
            None,
            None,
        );
        assert!(state == "stale" || state == "unknown (old)");
    }

    #[test]
    fn test_infer_login_required_from_transcript() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("20260711_010000_lr");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "20260711_010000_lr".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "login_required".to_string(),
            created_at: "2026-07-11T01:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
        fs::write(
            run_dir.join("transcript.jsonl"),
            r#"{"role":"system","content":"session_state=login_required"}"#,
        )
        .unwrap();

        let state = infer_login_state("", -1.0, runs_dir.to_str().unwrap(), "chatgpt", None, None);
        assert_eq!(state, "login_required");
    }

    // --- build_recovery_hint ---

    #[test]
    fn test_missing_selectors() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: false,
            selectors_path: "/selectors/chatgpt.json".to_string(),
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("Selektor-Datei fehlt"));
        assert!(hint.contains("diagnose"));
    }

    #[test]
    fn test_missing_profile() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: false,
            profile_dir: "/data/profiles/shared".to_string(),
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("Profil-Verzeichnis fehlt"));
        assert!(hint.contains("login"));
    }

    #[test]
    fn test_lock_files_present() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            profile_lock_files: vec!["SingletonLock".to_string(), "SingletonCookie".to_string()],
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("Lock-Dateien vorhanden"));
        assert!(hint.contains("SingletonLock"));
    }

    #[test]
    fn test_old_run_warning() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            last_done_run: "20260710_120000_aabbccdd".to_string(),
            last_done_run_age_hours: 72.0,
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("72h") || hint.contains("72.0h"));
        assert!(hint.contains("abgelaufen") || hint.contains("alt"));
    }

    #[test]
    fn test_no_hints_when_healthy() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            last_done_run: "20260712_120000_aabbccdd".to_string(),
            last_done_run_age_hours: 1.0,
            login_state: "ready".to_string(),
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert_eq!(hint, "");
    }

    #[test]
    fn test_unknown_login_state() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            login_state: "unknown".to_string(),
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("unbekannt"));
    }

    #[test]
    fn test_login_required_hint() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            login_state: "login_required".to_string(),
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("login_required"));
        assert!(hint.to_lowercase().contains("login"));
    }

    #[test]
    fn test_stale_login_hint() {
        let check = BrainCheck {
            brain_id: "chatgpt".to_string(),
            selectors_ok: true,
            profile_exists: true,
            login_state: "stale".to_string(),
            last_done_run_age_hours: 60.0,
            ..Default::default()
        };
        let hint = build_recovery_hint(&check);
        assert!(hint.contains("stale") || hint.contains("alt"));
    }

    // --- check_brain ---

    #[test]
    fn test_all_ok() {
        let tmp = unique_tmp();
        let sel_file = tmp.join("selectors").join("chatgpt.json");
        fs::create_dir_all(sel_file.parent().unwrap()).unwrap();
        fs::write(&sel_file, "{}").unwrap();
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();

        let mut brains_config = HashMap::new();
        let mut chatgpt_spec = HashMap::new();
        chatgpt_spec.insert("url".to_string(), "https://chatgpt.com/".to_string());
        chatgpt_spec.insert(
            "selectors".to_string(),
            sel_file.to_str().unwrap().to_string(),
        );
        chatgpt_spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
        brains_config.insert("chatgpt".to_string(), chatgpt_spec);

        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let result = check_brain(
            "chatgpt",
            Some(&brains_config),
            runs_dir.to_str().unwrap(),
            "",
            None,
            None,
        );
        assert!(result.healthy());
        assert!(result.selectors_ok);
        assert!(result.profile_exists);
        assert!(result.profile_lock_files.is_empty());
        assert_eq!(result.login_state, "unknown");
    }

    #[test]
    fn test_missing_selector() {
        let tmp = unique_tmp();
        let mut brains_config = HashMap::new();
        let mut chatgpt_spec = HashMap::new();
        chatgpt_spec.insert("url".to_string(), "https://chatgpt.com/".to_string());
        chatgpt_spec.insert(
            "selectors".to_string(),
            tmp.join("nonexistent.json").to_str().unwrap().to_string(),
        );
        chatgpt_spec.insert("profile".to_string(), "".to_string());
        brains_config.insert("chatgpt".to_string(), chatgpt_spec);

        let result = check_brain("chatgpt", Some(&brains_config), "", "", None, None);
        assert!(!result.healthy());
        assert!(!result.selectors_ok);
        assert!(result.recovery_hint.contains("Selektor"));
    }

    #[test]
    fn test_unknown_brain() {
        let brains_config = HashMap::new();
        let result = check_brain("nonexistent", Some(&brains_config), "", "", None, None);
        assert!(!result.healthy());
        assert!(result.recovery_hint.contains("Unbekanntes Brain"));
    }

    #[test]
    fn test_with_list_runs_fn() {
        let tmp = unique_tmp();
        let sel_file = tmp.join("selectors").join("chatgpt.json");
        fs::create_dir_all(sel_file.parent().unwrap()).unwrap();
        fs::write(&sel_file, "{}").unwrap();
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();

        let mut brains_config = HashMap::new();
        let mut chatgpt_spec = HashMap::new();
        chatgpt_spec.insert(
            "selectors".to_string(),
            sel_file.to_str().unwrap().to_string(),
        );
        chatgpt_spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
        brains_config.insert("chatgpt".to_string(), chatgpt_spec);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_secs = now - 3600;
        let (y, mo, d, h, mi, s) = crate::civil_utc(past_secs);
        let past = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+00:00",
            y, mo, d, h, mi, s
        );

        let fake_meta = RunMeta {
            run_id: "run_001".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "done".to_string(),
            created_at: past,
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };

        let list_fn = || vec!["run_001".to_string()];
        let load_fn = |_rid: &str| Some(fake_meta.clone());

        let result = check_brain(
            "chatgpt",
            Some(&brains_config),
            "",
            "",
            Some(&list_fn),
            Some(&load_fn),
        );
        assert_eq!(result.last_done_run, "run_001");
        assert!(result.last_done_run_age_hours >= 0.0);
        assert!(result.login_state == "ready" || result.login_state == "likely_ready");
    }

    #[test]
    fn test_login_required_makes_unhealthy() {
        let tmp = unique_tmp();
        let sel_file = tmp.join("selectors").join("chatgpt.json");
        fs::create_dir_all(sel_file.parent().unwrap()).unwrap();
        fs::write(&sel_file, "{}").unwrap();
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();

        let mut brains_config = HashMap::new();
        let mut chatgpt_spec = HashMap::new();
        chatgpt_spec.insert(
            "selectors".to_string(),
            sel_file.to_str().unwrap().to_string(),
        );
        chatgpt_spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
        brains_config.insert("chatgpt".to_string(), chatgpt_spec);

        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();
        let run_dir = runs_dir.join("r1");
        fs::create_dir_all(&run_dir).unwrap();

        let meta = RunMeta {
            run_id: "r1".to_string(),
            brain_id: "chatgpt".to_string(),
            status: "login_required".to_string(),
            created_at: "2026-07-12T10:00:00+00:00".to_string(),
            task: "test".to_string(),
            extra: HashMap::new(),
            completed_actions: HashMap::new(),
            conversation_ref: None,
            cycles: 0,
        };
        fs::write(
            run_dir.join("meta.json"),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();
        fs::write(
            run_dir.join("transcript.jsonl"),
            r#"{"role":"system","content":"session_state=login_required"}"#,
        )
        .unwrap();

        let result = check_brain(
            "chatgpt",
            Some(&brains_config),
            runs_dir.to_str().unwrap(),
            "",
            None,
            None,
        );
        assert_eq!(result.login_state, "login_required");
        assert!(!result.healthy());
    }

    // --- run_doctor ---

    #[test]
    fn test_all_brains() {
        let tmp = unique_tmp();
        let sel_dir = tmp.join("selectors");
        fs::create_dir_all(&sel_dir).unwrap();
        for name in &["chatgpt", "claude"] {
            fs::write(sel_dir.join(format!("{}.json", name)), "{}").unwrap();
        }
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let mut brains_config = HashMap::new();
        for bid in &["chatgpt", "claude"] {
            let mut spec = HashMap::new();
            spec.insert(
                "selectors".to_string(),
                sel_dir
                    .join(format!("{}.json", bid))
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
            brains_config.insert(bid.to_string(), spec);
        }

        let report = run_doctor(
            None,
            Some(&brains_config),
            runs_dir.to_str().unwrap(),
            None,
            None,
        );
        assert!(report.ok());
        assert_eq!(report.brains.len(), 2);
        assert_eq!(report.healthy_brain_ids(), vec!["chatgpt", "claude"]);
    }

    #[test]
    fn test_filter_brains() {
        let tmp = unique_tmp();
        let sel_dir = tmp.join("selectors");
        fs::create_dir_all(&sel_dir).unwrap();
        fs::write(sel_dir.join("chatgpt.json"), "{}").unwrap();
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let mut brains_config = HashMap::new();
        for bid in &["chatgpt", "claude"] {
            let mut spec = HashMap::new();
            spec.insert(
                "selectors".to_string(),
                sel_dir
                    .join(format!("{}.json", bid))
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
            brains_config.insert(bid.to_string(), spec);
        }

        let report = run_doctor(
            Some(vec!["chatgpt".to_string()]),
            Some(&brains_config),
            runs_dir.to_str().unwrap(),
            None,
            None,
        );
        assert_eq!(report.brains.len(), 1);
        assert!(report.brains.contains_key("chatgpt"));
        assert!(!report.brains.contains_key("claude"));
    }

    #[test]
    fn test_unhealthy_detected() {
        let tmp = unique_tmp();
        let profile = tmp.join("profiles").join("shared");
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("SingletonLock"), "").unwrap();
        let runs_dir = tmp.join("runs");
        fs::create_dir_all(&runs_dir).unwrap();

        let mut brains_config = HashMap::new();
        let mut spec = HashMap::new();
        spec.insert(
            "selectors".to_string(),
            tmp.join("missing.json").to_str().unwrap().to_string(),
        );
        spec.insert("profile".to_string(), profile.to_str().unwrap().to_string());
        brains_config.insert("chatgpt".to_string(), spec);

        let report = run_doctor(
            None,
            Some(&brains_config),
            runs_dir.to_str().unwrap(),
            None,
            None,
        );
        assert!(!report.ok());
        assert!(report
            .unhealthy_brain_ids()
            .contains(&"chatgpt".to_string()));
    }
}
