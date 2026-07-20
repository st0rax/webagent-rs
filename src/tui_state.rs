//! tui_state — App-State + State-Management für TUI
//!
//! Kern-Strukturen: App, AgentView, select_wrap, on_tick, load_state.
//! Rein-Rust, keine I/O-im-Member (load_state ist free fn).

use std::path::Path;
use std::time::SystemTime;

use crate::config::bot2bot_root;
use crate::worker_pool::PoolState;

/// Spinner-Frames für Animation (80ms Tick).
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Ein Agent im Dashboard.
#[derive(Debug, Clone)]
pub struct AgentView {
    pub brain: String,
    pub status: String, // available | active | unavailable | cooldown
    pub pid: Option<u32>,
    pub heartbeat_age_sec: u64, // Sekunden seit letztem Heartbeat
    pub tasks_pending: usize,
    pub tasks_done: usize,
    pub last_log_line: Option<String>,
    pub last_response: Option<String>,
}

/// Haupt-App-State für die TUI.
#[derive(Debug)]
pub struct App {
    pub agents: Vec<AgentView>,
    pub selected: usize,
    pub tick: u64,
    pub log_scroll: u16,
    pub input_mode: InputMode,
    pub target_active: usize,
    /// Gedämpfte Gauge-Werte (smooth animation).
    pub gauge_shown: f32,
}

/// Input-Modi der TUI.
#[derive(Debug, PartialEq)]
pub enum InputMode {
    /// Normal-Modus, Keys werden interpretiert.
    Normal,
    /// Task-Eingabe (t-Taste gedrückt).
    TaskInput,
    /// Quit bestätigen.
    ConfirmQuit,
}

/// Wrap-around Selektion (Pfeil hoch/runter in Liste).
pub fn select_wrap(current: usize, delta: i32, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let new = (current as i32 + delta) % len as i32;
    if new < 0 {
        (len as i32 + new) as usize
    } else {
        new as usize
    }
}

/// Tick-Handler: Spinner-Index + gedämpftes Gauge.
impl App {
    pub fn on_tick(&mut self, target: f32) {
        self.tick += 1;
        // Gedämpftes Gauge: shown += (target - shown) * 0.2
        self.gauge_shown += (target - self.gauge_shown) * 0.2;
        self.gauge_shown = self.gauge_shown.clamp(0.0, 1.0);
    }

    /// Spinner-Frame für aktuellen Tick.
    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[(self.tick as usize) % 10]
    }
}

/// Lädt State aus Dateien (throttled, nicht jeden Frame).
pub fn load_state(_force: bool) -> Vec<AgentView> {
    let root = bot2bot_root();
    let pool_path = root.join("workers").join("pool_state.json");
    let now = SystemTime::now();

    let pool: PoolState = fs_read_json(&pool_path).unwrap_or_default();

    // Heartbeat-Directory
    let heartbeat_dir = root.join("workers");

    pool.entries
        .iter()
        .map(|(brain, entry)| {
            let hb_path = heartbeat_dir.join(format!("heartbeat_{}.json", brain));
            let heartbeat_age = hb_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);

            // Inbox-Zählen
            let inbox = root.join("agents").join(brain).join("inbox");
            let read_dir = inbox.join("_read");
            let pending = count_msgs(&inbox);
            let done = count_msgs(&read_dir);

            // Log-Zeile (letzte aus history.jsonl)
            let log = latest_log_line(&root, brain);

            AgentView {
                brain: brain.clone(),
                status: entry.status.clone(),
                pid: None, // pid kommt aus heartbeat_dir/process_map wenn nötig
                heartbeat_age_sec: heartbeat_age,
                tasks_pending: pending,
                tasks_done: done,
                last_log_line: log.clone(),
                last_response: None, // TODO: aus history.jsonl extrahieren
            }
        })
        .collect()
}

fn fs_read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn count_msgs(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .ok()
        .map(|e| {
            e.filter_map(|f| f.ok())
                .filter(|f| f.path().extension().is_some_and(|e| e == "txt"))
                .count()
        })
        .unwrap_or(0)
}

fn latest_log_line(root: &Path, brain: &str) -> Option<String> {
    let history = root.join("agents").join(brain).join("history.jsonl");

    std::fs::read_to_string(&history)
        .ok()?
        .lines()
        .last()
        .map(|l| {
            // Versuche JSON zu parsen für "body" oder "content"
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| {
                    v.get("body")
                        .or_else(|| v.get("content"))
                        .and_then(|x| x.as_str().map(String::from))
                })
                .map(|s| s.chars().take(80).collect())
                .unwrap_or_else(|| l.chars().take(80).collect())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_wrap_forward() {
        assert_eq!(select_wrap(0, 1, 5), 1);
        assert_eq!(select_wrap(4, 1, 5), 0); // wrap
    }

    #[test]
    fn test_select_wrap_backward() {
        assert_eq!(select_wrap(0, -1, 5), 4);
        assert_eq!(select_wrap(2, -1, 5), 1);
    }

    #[test]
    fn test_select_wrap_empty() {
        assert_eq!(select_wrap(0, 1, 0), 0);
    }
}
