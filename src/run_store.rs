//! Run-Persistenz.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Terminal-Status, die nicht mehr geändert werden können.
const TERMINAL_STATUSES: &[&str] = &["done", "failed", "interrupted"];

/// Nicht-laufende Status (außer Terminal).
const NON_RUNNING_STATUSES: &[&str] = &[
    "brain_incomplete",
    "max_cycles",
    "login_required",
    "cloudflare",
    "error",
];

/// Erlaubte Status-Übergänge.
fn allowed_status_transitions() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut map = HashMap::new();
    
    let mut running_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        running_next.insert(*s);
    }
    for s in NON_RUNNING_STATUSES {
        running_next.insert(*s);
    }
    map.insert("running", running_next);
    
    let mut brain_incomplete_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        brain_incomplete_next.insert(*s);
    }
    brain_incomplete_next.insert("max_cycles");
    map.insert("brain_incomplete", brain_incomplete_next);
    
    let mut max_cycles_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        max_cycles_next.insert(*s);
    }
    max_cycles_next.insert("brain_incomplete");
    map.insert("max_cycles", max_cycles_next);
    
    let mut login_required_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        login_required_next.insert(*s);
    }
    map.insert("login_required", login_required_next);
    
    let mut cloudflare_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        cloudflare_next.insert(*s);
    }
    map.insert("cloudflare", cloudflare_next);
    
    let mut error_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        error_next.insert(*s);
    }
    map.insert("error", error_next);
    
    map
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    pub brain_id: String,
    pub task: String,
    pub created_at: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub cycles: u32,
    #[serde(default)]
    pub conversation_ref: Option<String>,
    #[serde(default)]
    pub completed_actions: HashMap<String, String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_status() -> String {
    "running".to_string()
}

impl RunMeta {
    /// Verzeichnis für diesen Run.
    pub fn dir(&self, runs_dir: &Path) -> PathBuf {
        runs_dir.join(&self.run_id)
    }
}

pub struct RunStore {
    runs_dir: PathBuf,
    logs_dir: PathBuf,
}

impl RunStore {
    pub fn new(runs_dir: PathBuf, logs_dir: PathBuf) -> Self {
        fs::create_dir_all(&runs_dir).ok();
        fs::create_dir_all(&logs_dir).ok();
        Self { runs_dir, logs_dir }
    }

    /// Erstellt einen neuen Run.
    pub fn create(&self, brain_id: &str, task: &str) -> Result<RunMeta, String> {
        // Einfache Zufalls-ID ohne uuid-Crate: Timestamp + Prozess-ID + Zufallszahl
        let random_suffix = {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let pid = std::process::id();
            format!("{:08x}", (nanos ^ pid).wrapping_mul(0x9e3779b9))
        };
        let run_id = format!("{}_{}", crate::now_run_stamp(), random_suffix);
        let meta = RunMeta {
            run_id: run_id.clone(),
            brain_id: brain_id.to_string(),
            task: task.to_string(),
            created_at: crate::now_rfc3339(),
            status: "running".to_string(),
            cycles: 0,
            conversation_ref: None,
            completed_actions: HashMap::new(),
            extra: HashMap::new(),
        };

        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;
        
        let log_dir = self.logs_dir.join(&run_id);
        fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", log_dir.display(), e))?;

        self.save_internal(&meta)?;
        self.append_event(&meta, "created", serde_json::json!({"status": meta.status}))?;
        
        Ok(meta)
    }

    /// Lädt einen Run.
    pub fn load(&self, run_id: &str) -> Result<RunMeta, String> {
        let path = self.runs_dir.join(run_id).join("meta.json");
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Fehler beim Lesen von {}: {}", path.display(), e))?;
        
        let meta: RunMeta = serde_json::from_str(&content)
            .map_err(|e| format!("Fehler beim Parsen von {}: {}", path.display(), e))?;
        
        Ok(meta)
    }

    /// Speichert einen Run mit Validierung.
    pub fn save(&self, meta: &RunMeta) -> Result<(), String> {
        let previous = self.load_existing_meta(&meta.run_id);
        
        if let Some(prev) = &previous {
            self.validate_status_transition(&prev.status, &meta.status)?;
        }
        
        self.save_internal(meta)?;
        self.append_save_events(previous.as_ref(), meta)?;
        
        Ok(())
    }

    /// Interne Speicherfunktion ohne Validierung.
    fn save_internal(&self, meta: &RunMeta) -> Result<(), String> {
        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;
        
        let path = run_dir.join("meta.json");
        let tmp_path = run_dir.join("meta.json.tmp");
        
        let json = serde_json::to_string_pretty(meta)
            .map_err(|e| format!("Fehler beim Serialisieren: {}", e))?;
        
        fs::write(&tmp_path, json)
            .map_err(|e| format!("Fehler beim Schreiben von {}: {}", tmp_path.display(), e))?;
        
        fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Fehler beim Umbenennen von {} nach {}: {}", tmp_path.display(), path.display(), e))?;
        
        Ok(())
    }

    /// Lädt existierende Meta-Daten (ohne Fehler bei Nicht-Existenz).
    fn load_existing_meta(&self, run_id: &str) -> Option<RunMeta> {
        let path = self.runs_dir.join(run_id).join("meta.json");
        if !path.exists() {
            return None;
        }
        
        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Validiert Status-Übergänge.
    fn validate_status_transition(&self, previous: &str, next: &str) -> Result<(), String> {
        if previous == next {
            return Ok(());
        }
        
        let transitions = allowed_status_transitions();
        let allowed = transitions.get(previous);
        
        match allowed {
            Some(set) if set.contains(next) => Ok(()),
            Some(_) => Err(format!(
                "Ungültiger Run-Status-Übergang: {:?} -> {:?}",
                previous, next
            )),
            None => Err(format!(
                "Ungültiger Run-Status-Übergang: {:?} -> {:?}",
                previous, next
            )),
        }
    }

    /// Schreibt Events beim Speichern.
    fn append_save_events(&self, previous: Option<&RunMeta>, meta: &RunMeta) -> Result<(), String> {
        if previous.is_none() {
            return self.append_event(meta, "created", serde_json::json!({"status": &meta.status}));
        }
        
        let prev = previous.unwrap();
        if prev.status != meta.status {
            self.append_event(
                meta,
                "status_changed",
                serde_json::json!({
                    "from": &prev.status,
                    "to": &meta.status,
                }),
            )
        } else {
            self.append_event(meta, "meta_saved", serde_json::json!({"status": &meta.status}))
        }
    }

    /// Schreibt ein Event in events.jsonl.
    fn append_event(
        &self,
        meta: &RunMeta,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;
        
        let path = run_dir.join("events.jsonl");
        let event = serde_json::json!({
            "timestamp": crate::now_rfc3339(),
            "run_id": &meta.run_id,
            "type": event_type,
            "payload": payload,
        });
        
        let line = serde_json::to_string(&event)
            .map_err(|e| format!("Fehler beim Serialisieren des Events: {}", e))?;
        
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Fehler beim Öffnen von {}: {}", path.display(), e))?;
        
        writeln!(file, "{}", line)
            .map_err(|e| format!("Fehler beim Schreiben in {}: {}", path.display(), e))?;
        
        Ok(())
    }

    /// Listet alle Runs auf (sortiert, neueste zuerst).
    pub fn list_runs(&self) -> Vec<String> {
        let mut runs = Vec::new();
        
        if let Ok(entries) = fs::read_dir(&self.runs_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        runs.push(name.to_string());
                    }
                }
            }
        }
        
        runs.sort_by(|a, b| b.cmp(a)); // Neueste zuerst
        runs
    }

    /// Markiert verwaiste `running`-Runs als `interrupted`.
    pub fn reconcile_stale_runs(&self, legacy_age_seconds: f64) -> Vec<String> {
        let mut repaired = Vec::new();
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        
        for run_id in self.list_runs() {
            let meta = match self.load(&run_id) {
                Ok(m) => m,
                Err(_) => continue,
            };
            
            if meta.status != "running" {
                continue;
            }
            
            let owner_pid = meta.extra.get("owner_pid").and_then(|v| v.as_i64());
            
            let stale = if let Some(pid) = owner_pid {
                !crate::pid_alive(pid)
            } else {
                // Legacy: kein owner_pid → Alter prüfen
                match parse_rfc3339_to_unix(&meta.created_at) {
                    Some(created_secs) => {
                        let age = (now_secs - created_secs) as f64;
                        age >= legacy_age_seconds
                    }
                    None => true, // Ungültiger Zeitstempel → als stale behandeln
                }
            };
            
            if !stale {
                continue;
            }
            
            let mut updated = meta.clone();
            updated.status = "interrupted".to_string();
            updated.extra.insert(
                "reconciled_at".to_string(),
                serde_json::Value::String(crate::now_rfc3339()),
            );
            updated.extra.insert(
                "error".to_string(),
                serde_json::Value::String("Prozess endete ohne finalen Run-Status.".to_string()),
            );
            
            if self.save(&updated).is_ok() {
                repaired.push(run_id);
            }
        }
        
        repaired
    }
}

/// Parst RFC3339-Zeitstempel zu Unix-Sekunden (UTC).
/// Vereinfachte Implementierung für ISO 8601 / RFC3339 wie `2024-01-15T10:30:45.123456+00:00`.
fn parse_rfc3339_to_unix(s: &str) -> Option<i64> {
    // Format: YYYY-MM-DDTHH:MM:SS[.ffffff](+00:00|Z)
    let re = regex::Regex::new(
        r"^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.\d+)?(?:Z|\+00:00)$"
    ).ok()?;
    
    let caps = re.captures(s)?;
    let year: i32 = caps.get(1)?.as_str().parse().ok()?;
    let month: u32 = caps.get(2)?.as_str().parse().ok()?;
    let day: u32 = caps.get(3)?.as_str().parse().ok()?;
    let hour: u32 = caps.get(4)?.as_str().parse().ok()?;
    let minute: u32 = caps.get(5)?.as_str().parse().ok()?;
    let second: u32 = caps.get(6)?.as_str().parse().ok()?;
    
    // Umrechnung zu Unix-Timestamp (Tage seit Epoch + Tageszeit)
    // Vereinfachte Variante von civil_to_days (inverse von civil_utc in lib.rs)
    let y = year as i64;
    let m = month as i64;
    let d = day as i64;
    
    let adj_year = if m <= 2 { y - 1 } else { y };
    let adj_month = if m <= 2 { m + 12 } else { m };
    
    let era = if adj_year >= 0 { adj_year } else { adj_year - 399 } / 400;
    let yoe = adj_year - era * 400;
    let doy = (153 * (adj_month - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    
    let secs = days * 86_400 + (hour as i64) * 3600 + (minute as i64) * 60 + (second as i64);
    Some(secs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_reconcile_legacy_stale_running_run() {
        let tmp = env::temp_dir().join(format!("test_run_store_{}", crate::now_run_stamp()));
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "stale").unwrap();
        
        // Setze created_at auf 1 Stunde in der Vergangenheit (3600 Sekunden)
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_secs = now_secs - 3600;
        
        // Konvertiere zu RFC3339 (vereinfacht, nur für Test)
        let (y, mo, d, h, mi, s) = crate::lib::civil_utc(past_secs);
        meta.created_at = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000000+00:00",
            y, mo, d, h, mi, s
        );
        store.save_internal(&meta).unwrap();
        
        let repaired = store.reconcile_stale_runs(10.0);
        assert_eq!(repaired, vec![meta.run_id.clone()]);
        
        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "interrupted");
        assert!(loaded.extra.contains_key("reconciled_at"));
        
        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_reconcile_keeps_live_owned_run() {
        let tmp = env::temp_dir().join(format!("test_run_store_{}", crate::now_run_stamp()));
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "live").unwrap();
        
        // Setze owner_pid auf aktuellen Prozess
        meta.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(std::process::id().into()),
        );
        store.save(&meta).unwrap();
        
        let repaired = store.reconcile_stale_runs(0.0);
        assert!(repaired.is_empty());
        
        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "running");
        
        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_status_transition_validation() {
        let tmp = env::temp_dir().join(format!("test_run_store_{}", crate::now_run_stamp()));
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "test").unwrap();
        
        // Erlaubter Übergang: running -> done
        meta.status = "done".to_string();
        assert!(store.save(&meta).is_ok());
        
        // Unerlaubter Übergang: done -> running
        meta.status = "running".to_string();
        assert!(store.save(&meta).is_err());
        
        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_create_and_load() {
        let tmp = env::temp_dir().join(format!("test_run_store_{}", crate::now_run_stamp()));
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let meta = store.create("test_brain", "test task").unwrap();
        
        assert_eq!(meta.brain_id, "test_brain");
        assert_eq!(meta.task, "test task");
        assert_eq!(meta.status, "running");
        assert_eq!(meta.cycles, 0);
        
        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.run_id, meta.run_id);
        assert_eq!(loaded.brain_id, "test_brain");
        
        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_list_runs() {
        let tmp = env::temp_dir().join(format!("test_run_store_{}", crate::now_run_stamp()));
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        
        let meta1 = store.create("brain1", "task1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let meta2 = store.create("brain2", "task2").unwrap();
        
        let runs = store.list_runs();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0], meta2.run_id); // Neueste zuerst
        assert_eq!(runs[1], meta1.run_id);
        
        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }
}
