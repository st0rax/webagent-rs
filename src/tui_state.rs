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
    /// Ausklapp-Inhalt: die letzten Ereignisse dieses Agenten im Klartext.
    ///
    /// Zusammengeklappt zeigt die Liste eine Zeile je Agent; das reicht für den
    /// Überblick, verschweigt aber, WAS gerade passiert. Diese Zeilen sind der
    /// Detailblick, der sich per Leertaste darunter aufspannt.
    pub detail: Vec<String>,
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
    /// Aufgeklappte Agenten (Brain-Name). Gehört zum App-State, nicht zur
    /// `AgentView`: die Views werden bei jedem Reload neu aus den Dateien
    /// gebaut, ein dort gehaltenes Flag wäre nach ein paar hundert Millisekunden
    /// wieder zu.
    pub expanded: std::collections::HashSet<String>,
    /// Scroll-Offset innerhalb des aufgeklappten Details.
    pub detail_scroll: usize,
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

    /// Klappt den Agenten unter dem Cursor auf bzw. zu.
    pub fn toggle_expanded(&mut self) {
        let Some(brain) = self.selected_brain() else {
            return;
        };
        if self.expanded.contains(&brain) {
            self.expanded.remove(&brain);
            self.detail_scroll = 0;
        } else {
            self.expanded.insert(brain);
        }
    }

    /// Klappt gezielt zu (Pfeil links) — ohne Umschalten, damit wiederholtes
    /// Drücken nicht versehentlich wieder aufklappt.
    pub fn collapse_selected(&mut self) {
        if let Some(brain) = self.selected_brain() {
            self.expanded.remove(&brain);
            self.detail_scroll = 0;
        }
    }

    /// Klappt gezielt auf (Pfeil rechts).
    pub fn expand_selected(&mut self) {
        if let Some(brain) = self.selected_brain() {
            self.expanded.insert(brain);
        }
    }

    pub fn is_expanded(&self, brain: &str) -> bool {
        self.expanded.contains(brain)
    }

    pub fn selected_brain(&self) -> Option<String> {
        self.agents.get(self.selected).map(|a| a.brain.clone())
    }
}

/// Baut die Detailzeilen eines Agenten aus seinen Roh-Ereignissen.
///
/// Neueste zuerst, auf `max` Einträge begrenzt und auf `width` umgebrochen —
/// eine 4000-Zeichen-Brain-Antwort würde die Liste sonst unbedienbar machen.
/// Leere Einträge fallen raus, damit kein Loch im Baum entsteht.
pub fn build_detail_lines(raw: &[String], max: usize, width: usize) -> Vec<String> {
    let w = width.max(8);
    raw.iter()
        .rev()
        .filter(|s| !s.trim().is_empty())
        .take(max)
        .flat_map(|entry| wrap_text(entry.trim(), w))
        .collect()
}

/// Zeilenindex des ausgewählten Agenten in der aufgeklappten Liste.
///
/// Sobald ein Agent aufgeklappt ist, stehen zwischen den Agenten-Zeilen weitere
/// Detailzeilen. Der Auswahl-Balken rechnet aber in Agenten-Indizes — ohne diese
/// Umrechnung markiert er nach dem ersten Aufklappen die falsche Zeile.
/// `detail_rows[i]` ist die Anzahl der Detailzeilen von Agent `i` (0 = zu).
pub fn selected_row(detail_rows: &[usize], selected: usize) -> usize {
    detail_rows
        .iter()
        .take(selected)
        .map(|n| n + 1)
        .sum::<usize>()
}

/// Zeichenweiser Umbruch auf `width` (nicht bytebasiert — Umlaute und
/// Box-Zeichen dürfen nicht mitten durchgeschnitten werden).
pub fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let w = width.max(1);
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    chars.chunks(w).map(|c| c.iter().collect()).collect()
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
                detail: recent_log_lines(&root, brain, DETAIL_HISTORY),
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

/// Wie viele Verlaufseintraege der Ausklapp-Blick hoechstens laedt.
const DETAIL_HISTORY: usize = 12;

/// Die letzten `n` Verlaufseintraege eines Agenten als Klartextzeilen —
/// Rohmaterial fuer den Ausklapp-Blick (ungekuerzt; der Umbruch passiert erst
/// beim Rendern, wo die Breite bekannt ist).
fn recent_log_lines(root: &Path, brain: &str, n: usize) -> Vec<String> {
    let history = root.join("agents").join(brain).join("history.jsonl");
    let Ok(text) = std::fs::read_to_string(&history) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all.iter()
        .rev()
        .take(n)
        .rev()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| {
                    let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("");
                    v.get("body")
                        .or_else(|| v.get("content"))
                        .and_then(|x| x.as_str())
                        .map(|b| if kind.is_empty() { b.to_string() } else { format!("[{kind}] {b}") })
                })
                .unwrap_or_else(|| (*l).to_string())
        })
        .collect()
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

    fn view(brain: &str, detail: Vec<String>) -> AgentView {
        AgentView {
            brain: brain.to_string(),
            status: "active".to_string(),
            pid: None,
            heartbeat_age_sec: 0,
            tasks_pending: 0,
            tasks_done: 0,
            last_log_line: None,
            last_response: None,
            detail,
        }
    }

    fn app_with(agents: Vec<AgentView>) -> App {
        App {
            agents,
            selected: 0,
            tick: 0,
            log_scroll: 0,
            input_mode: InputMode::Normal,
            target_active: 2,
            gauge_shown: 0.0,
            expanded: std::collections::HashSet::new(),
            detail_scroll: 0,
        }
    }

    #[test]
    fn selected_row_shifts_past_expanded_details() {
        // Der eigentliche Fallstrick: Agent 2 steht nach dem Aufklappen von
        // Agent 0 nicht mehr in Zeile 2, sondern in Zeile 6.
        assert_eq!(selected_row(&[0, 0, 0], 2), 2, "alles zu: Index = Zeile");
        assert_eq!(selected_row(&[4, 0, 0], 2), 6);
        assert_eq!(selected_row(&[4, 3, 0], 2), 9);
        assert_eq!(selected_row(&[4, 3, 0], 0), 0, "erster Agent bleibt Zeile 0");
    }

    #[test]
    fn toggle_expands_then_collapses_the_selected_agent() {
        let mut app = app_with(vec![view("a", vec!["x".into()]), view("b", vec![])]);
        assert!(!app.is_expanded("a"));
        app.toggle_expanded();
        assert!(app.is_expanded("a"));
        app.toggle_expanded();
        assert!(!app.is_expanded("a"));
    }

    #[test]
    fn arrow_keys_are_directional_not_toggling() {
        // Zweimal "rechts" darf nicht wieder zuklappen — sonst fuehlt sich die
        // Navigation kaputt an, wenn man den Baum durchhaelt.
        let mut app = app_with(vec![view("a", vec!["x".into()])]);
        app.expand_selected();
        app.expand_selected();
        assert!(app.is_expanded("a"));
        app.collapse_selected();
        app.collapse_selected();
        assert!(!app.is_expanded("a"));
    }

    #[test]
    fn expansion_survives_a_state_reload() {
        // AgentViews werden bei jedem Reload neu gebaut; haenge das Flag dort,
        // waere es nach ~einer Sekunde wieder zu.
        let mut app = app_with(vec![view("a", vec!["x".into()])]);
        app.toggle_expanded();
        app.agents = vec![view("a", vec!["neu".into()])];
        assert!(app.is_expanded("a"), "Aufklappzustand haengt am App-State");
    }

    #[test]
    fn wrap_text_splits_on_chars_not_bytes() {
        // Umlaute sind 2 Bytes: bytebasiertes Schneiden wuerde hier panicken
        // oder Muell erzeugen.
        assert_eq!(wrap_text("äöüäöüäöü", 3), vec!["äöü", "äöü", "äöü"]);
        assert_eq!(wrap_text("", 5), Vec::<String>::new());
        assert_eq!(wrap_text("abc", 0), vec!["a", "b", "c"], "Breite 0 darf nicht endlos schleifen");
    }

    #[test]
    fn detail_lines_wrap_at_the_given_width() {
        let lines = build_detail_lines(&["äöüäöüäöüäöüä".to_string()], 5, 8);
        assert_eq!(lines, vec!["äöüäöüäö", "üäöüä"]);
    }

    #[test]
    fn detail_lines_enforce_a_minimum_width() {
        // Eine sehr schmale Pane darf nicht zu einzeichenbreiten Zeilen fuehren.
        let lines = build_detail_lines(&["abcdefghijkl".to_string()], 5, 2);
        assert!(lines.iter().all(|l| l.chars().count() <= 8));
        assert_eq!(lines[0].chars().count(), 8);
    }

    #[test]
    fn detail_lines_show_newest_first_and_drop_empties() {
        let raw = vec!["alt".to_string(), "  ".to_string(), "neu".to_string()];
        let lines = build_detail_lines(&raw, 5, 40);
        assert_eq!(lines, vec!["neu", "alt"]);
    }

    #[test]
    fn detail_lines_respect_the_entry_cap() {
        let raw: Vec<String> = (0..20).map(|i| format!("eintrag-{i}")).collect();
        assert_eq!(build_detail_lines(&raw, 3, 40).len(), 3);
    }

    #[test]
    fn collapsing_resets_the_detail_scroll() {
        let mut app = app_with(vec![view("a", vec!["x".into()])]);
        app.expand_selected();
        app.detail_scroll = 7;
        app.collapse_selected();
        assert_eq!(app.detail_scroll, 0, "sonst startet das naechste Aufklappen mitten drin");
    }
}
