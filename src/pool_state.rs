//! pool_state — Datei-Protokoll des Worker-Pools (Zustand + Steuerung).
//!
//! Aus `worker_pool.rs` herausgeloest (Refactoring 06:20): der
//! Persistenz-/IPC-Layer des Pools. Health/Status pro Brain liegt in
//! `pool_state.json` (`available` | `active` | `unavailable` | `cooldown` |
//! `retired` + `last_error`); die TUI publiziert Befehle atomar als
//! `pool_control.json`. Alles hier ist reine Datei-IPC — keine Prozess-Spawning,
//! kein Failover (das entscheidet `pool_failover`). Extern erreichbar weiterhin
//! unter `crate::worker_pool::…` (Re-Export in `worker_pool.rs`).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

/// Status eines Brains im Pool.
pub const STATUS_AVAILABLE: &str = "available";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_UNAVAILABLE: &str = "unavailable";

/// Status: Brain ist ALIVE, aber als BLOCK/HANG erkannt (Deadlock, eingefrorener
/// WebView, natives Modal, keine Fortschritte). Ein Reserve-Brain uebernimmt,
/// das Original geht in den Cooldown und wird nach Ablauf frisch wiederhergestellt.
pub const STATUS_COOLDOWN: &str = "cooldown";

/// Status: Brain wurde nach `MAX_FAILED_RESTORES` fehlgeschlagenen Restores
/// dauerhaft ausgemustert (Retired). Eigener Status statt `unavailable` +
/// Reason-String, damit die Auto-Recovery (`select_auto_recovery`) das Brain
/// NICHT nach Ablauf der Retry-Frist wiederbelebt — Retirement ist final und
/// wird nur durch manuelles Reflag (pool_control `reflag`/`reflag_all`)
/// aufgehoben. Serialisiert als gewoehnlicher Status-String in
/// `pool_state.json` -> rueckwaertskompatibel (alte States kennen den Wert
/// schlicht nicht; unbekannte Status werden ueberall wie "nicht available"
/// behandelt).
pub const STATUS_RETIRED: &str = "retired";

/// Cooldown-Dauer (Sekunden) fuer ein als BLOCK erkanntes Brain, bevor es durch
/// einen frischen Worker wiederhergestellt wird. Ueberschreibbar via
/// `config::block_cooldown_secs()` (Env WEBAGENT_BLOCK_COOLDOWN_S); dieser const
/// ist die kanonische Default-Untergrenze (600 s = 10 min).
pub const BLOCK_COOLDOWN_SECS: u64 = 600;

/// Wie lange ein `unavailable` Brain wartet, bevor es automatisch wieder
/// als `available` reflaggt wird (Default 120s). Ueberschreibbar via Env
/// `config::retry_unavailable_secs()` (Env WEBAGENT_RETRY_UNAVAILABLE_S).
pub const RETRY_UNAVAILABLE_AFTER_SECS: u64 = 120;

/// Health/Status-Eintrag pro Brain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub brain: String,
    #[serde(default = "default_available")]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub updated_at: String,
    /// RFC3339-Zeitpunkt, bis zu dem dieses Brain im Cooldown bleibt (BLOCK-Failover).
    #[serde(default)]
    pub cooldown_until: Option<String>,
    /// Brain, das dieses (Cooldown-)Brain im Failover ersetzt (Reserve).
    #[serde(default)]
    pub replaced_by: Option<String>,
    /// Letzter Heartbeat-Zeitpunkt in Millisekunden seit Unix-Epoch.
    #[serde(default)]
    pub last_heartbeat_ms: Option<u64>,
}

impl PoolEntry {
    fn available(brain: &str) -> Self {
        PoolEntry {
            brain: brain.to_string(),
            status: STATUS_AVAILABLE.to_string(),
            last_error: String::new(),
            updated_at: crate::now_rfc3339(),
            cooldown_until: None,
            replaced_by: None,
            last_heartbeat_ms: None,
        }
    }
}

fn default_available() -> String {
    STATUS_AVAILABLE.to_string()
}

/// Gesamter Pool-Zustand (`bot2bot_root()/workers/pool_state.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolState {
    #[serde(default)]
    pub entries: HashMap<String, PoolEntry>,
}

impl PoolState {
    /// Lädt `pool_state.json`; fehlt die Datei, wird ein leerer Default geliefert.
    pub fn load(path: &Path) -> PoolState {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(st) = serde_json::from_str::<PoolState>(&s) {
                return st;
            }
        }
        PoolState::default()
    }

    /// Lädt und stellt sicher, dass alle Kandidaten einen Eintrag haben
    /// (Default `available`).
    pub fn load_or_init(path: &Path, candidates: &[String]) -> PoolState {
        let mut st = Self::load(path);
        for b in candidates {
            st.entries
                .entry(b.clone())
                .or_insert_with(|| PoolEntry::available(b));
        }
        st
    }

    pub fn set(&mut self, brain: &str, status: &str, last_error: &str) {
        let e = self
            .entries
            .entry(brain.to_string())
            .or_insert_with(|| PoolEntry::available(brain));
        e.status = status.to_string();
        e.last_error = last_error.to_string();
        e.updated_at = crate::now_rfc3339();
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        atomic_write(path, s.as_bytes())
    }
}

/// Steuerbefehle für den laufenden Supervisor (Datei-IPC, siehe
/// `PoolControl::take`). Die TUI (Teil 2) schreibt `pool_control.json`;
/// der Supervisor liest es pro Tick und wendet target_active / reflag / stop an.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolControl {
    /// Gewünschte Anzahl aktiver Worker (überschreibt `active`).
    /// `None` = nicht ändern, `Some(0)` = alle Worker stoppen.
    #[serde(default)]
    pub target_active: Option<usize>,
    /// Alle Kandidaten auf `available` zurücksetzen (nach Fix).
    #[serde(default)]
    pub reflag_all: bool,
    /// Einzelne Brains auf `available` zurücksetzen.
    #[serde(default)]
    pub reflag: Vec<String>,
    /// Supervisor sauber beenden (Kinder killen).
    #[serde(default)]
    pub stop: bool,
}

impl PoolControl {
    /// Übernimmt `pool_control.json` per atomarem Rename und lädt den Inhalt.
    ///
    /// Ein paralleler Schreiber kann sofort die nächste Steuerdatei publizieren,
    /// ohne dass der Supervisor sie nachträglich versehentlich löscht. Ungültiges
    /// JSON bleibt als `*.invalid-*` neben der Control-Datei zur Diagnose liegen.
    pub fn take(path: &Path) -> std::io::Result<Option<PoolControl>> {
        let claimed = sibling_temp_path(path, "processing");
        match fs::rename(path, &claimed) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }

        let parsed = fs::read_to_string(&claimed).and_then(|s| {
            serde_json::from_str::<PoolControl>(&s)
                .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))
        });

        match parsed {
            Ok(control) => {
                if let Err(e) = fs::remove_file(&claimed) {
                    crate::bench_events::eprint_line(&format!(
                        "[worker_pool] konsumierte Control-Datei konnte nicht entfernt werden \
                         ({}): {e}",
                        claimed.display()
                    ));
                }
                Ok(Some(control))
            }
            Err(e) => {
                let invalid = sibling_temp_path(path, "invalid");
                let retained = match fs::rename(&claimed, &invalid) {
                    Ok(()) => invalid,
                    Err(_) => claimed,
                };
                Err(std::io::Error::new(
                    e.kind(),
                    format!(
                        "{}; ungültige Control-Datei behalten: {}",
                        e,
                        retained.display()
                    ),
                ))
            }
        }
    }
}

/// Schreibt eine Datei vollständig in eine gleichgeordnete temporäre Datei und
/// veröffentlicht sie anschließend per atomarem Replace.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp = sibling_temp_path(path, "tmp");
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&tmp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn sibling_temp_path(path: &Path, purpose: &str) -> PathBuf {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pool");
    path.with_file_name(format!(".{name}.{purpose}-{}-{seq}", std::process::id()))
}

#[cfg(not(windows))]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "test_wpool_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn pool_state_roundtrip() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let mut st = PoolState::default();
        st.set("deepseek", STATUS_ACTIVE, "");
        st.set("chatgpt", STATUS_UNAVAILABLE, "exit code 1");
        st.save(&path).unwrap();

        let loaded = PoolState::load(&path);
        assert_eq!(loaded.entries["deepseek"].status, STATUS_ACTIVE);
        assert_eq!(loaded.entries["chatgpt"].status, STATUS_UNAVAILABLE);
        assert_eq!(loaded.entries["chatgpt"].last_error, "exit code 1");
    }

    #[test]
    fn pool_state_save_atomically_replaces_existing_file() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let mut st = PoolState::default();
        st.set("first", STATUS_ACTIVE, "");
        st.save(&path).unwrap();
        st.entries.clear();
        st.set("second", STATUS_AVAILABLE, "");
        st.save(&path).unwrap();

        let loaded = PoolState::load(&path);
        assert!(!loaded.entries.contains_key("first"));
        assert_eq!(loaded.entries["second"].status, STATUS_AVAILABLE);
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files leaked: {leftovers:?}"
        );
    }

    #[test]
    fn pool_control_take_distinguishes_zero_from_no_change() {
        let dir = tmp_dir();
        let path = dir.join("pool_control.json");
        let control = PoolControl {
            target_active: Some(0),
            ..Default::default()
        };
        let json = serde_json::to_vec(&control).unwrap();
        atomic_write(&path, &json).unwrap();

        let loaded = PoolControl::take(&path).unwrap().unwrap();
        assert_eq!(loaded.target_active, Some(0));
        assert!(!path.exists());
        assert!(PoolControl::take(&path).unwrap().is_none());
    }

    #[test]
    fn invalid_pool_control_is_retained_for_diagnosis() {
        let dir = tmp_dir();
        let path = dir.join("pool_control.json");
        atomic_write(&path, b"{not json").unwrap();

        let err = PoolControl::take(&path).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        let retained: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".invalid-"))
            .collect();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read_to_string(retained[0].path()).unwrap(), "{not json");
    }

    #[test]
    fn load_or_init_seeds_candidates_as_available() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let st = PoolState::load_or_init(&path, &candidates);
        assert_eq!(st.entries.len(), 3);
        assert_eq!(st.entries["a"].status, STATUS_AVAILABLE);
    }

    #[test]
    fn block_cooldown_default_is_600() {
        assert_eq!(BLOCK_COOLDOWN_SECS, 600);
    }
}
