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
    /// Fokussiertes Panel (Tab wechselt) — Gewinner-Design des Swarm-Votes
    /// 2026-07-22 (qwen: „Wechseln des UI-Fokus zwischen den drei Panels per
    /// Tab").
    pub focus: Panel,
    /// Log-Filter (f schaltet um) — „Filtern des Logs mit f".
    pub log_filter: LogFilter,
    /// Ringpuffer des Pool-Pulses: je Tick die Zahl frisch pulsierender Worker
    /// (Heartbeat < 10s). Speist die Live-Sparkline in der Kopfleiste — gibt der
    /// TUI einen lebendigen, atmenden Verlauf statt statischer Zahlen.
    pub activity_history: std::collections::VecDeque<u64>,
    /// Welche Hauptansicht gerade gezeigt wird (`<>` bzw. `v` schaltet um).
    ///
    /// Bis 2026-07-24 lag das schoene ratatui-Dashboard in `webagent tui`,
    /// waehrend die Benchmark-Ausgabe getrennter Plaintext war — zwei
    /// Oberflaechen fuer dieselbe Arbeit. Jetzt ist es EINE TUI mit zwei
    /// Ansichten auf denselben Zustand.
    pub view: View,
    /// Scroll-Offset der Benchmark-Ansicht (0 = am unteren Rand mitlaufen).
    pub bench_scroll: usize,
    /// Aufgeklappte Knoten der Benchmark-Baumansicht (stabile Event-IDs).
    pub bench_expanded: std::collections::HashSet<u64>,
    /// Cursor-Position in der gefalteten Baum-Zeilenliste (0 = unterste Zeile).
    pub bench_selected: usize,
    /// Eingabepuffer für die Kommandozeile (/).
    pub command_input: String,
    /// Rückmeldung der Brain-Wall (`w`), z.B. „Wall: 8 Fenster gekachelt".
    ///
    /// Ohne diese Zeile bliebe ein fehlgeschlagenes Anordnen unsichtbar: die
    /// Fenster liegen off-screen, man sieht also weder Erfolg noch Misserfolg.
    pub wall_status: String,
}

/// Die umschaltbaren Hauptansichten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Worker-/Subagent-Dashboard (Agenten, Tasks, Log).
    Workers,
    /// Arbeits-/Benchmark-Ansicht: der Ereignisstrom des laufenden Laufs.
    Bench,
}

impl View {
    pub fn next(self) -> View {
        match self {
            View::Workers => View::Bench,
            View::Bench => View::Workers,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Workers => "Worker",
            View::Bench => "Benchmark",
        }
    }
}

/// Eine sichtbare Zeile der Benchmark-Baumansicht.
///
/// Der flache Ereignisstrom wird gefaltet: Jedes Ereignis ist ein Knoten
/// (`depth = 0`), seine aufgeklappten Detailzeilen folgen eingerueckt
/// (`depth = 1`, `is_node = false`). So hat der Renderer eine schlichte
/// Zeilenliste zum Zeichnen, und die Navigation rechnet in Zeilen statt in
/// Baum-Indizes.
#[derive(Debug, Clone)]
pub struct BenchLine {
    /// ID des zugehoerigen Ereignisses (auch Detailzeilen tragen sie, damit
    /// Auf-/Zuklappen und Cursor zusammenpassen).
    pub id: u64,
    /// 0 = Knoten, 1 = Detailzeile.
    pub depth: usize,
    /// Ist das ein Knoten (Kopfzeile des Ereignisses) oder eine Detailzeile?
    pub is_node: bool,
    /// Hat der Knoten ueberhaupt ein Detail (zeigt der Renderer ▸/▾ an)?
    pub has_children: bool,
    pub ts: String,
    pub level: crate::bench_events::Level,
    pub brain: Option<String>,
    pub text: String,
}

/// Faltet den Ereignisstrom zu einer Baum-Zeilenliste.
///
/// Knoten, deren ID in `expanded` steht, bekommen ihre Detailzeilen als
/// eingerueckte Kinder. Ohne Aufklappen ist die Liste 1:1 der Strom.
pub fn fold_bench_events(
    events: &[crate::bench_events::BenchEvent],
    expanded: &std::collections::HashSet<u64>,
) -> Vec<BenchLine> {
    let mut out = Vec::with_capacity(events.len());
    for ev in events {
        let has_children = ev.detail.is_some();
        out.push(BenchLine {
            id: ev.id,
            depth: 0,
            is_node: true,
            has_children,
            ts: ev.ts.clone(),
            level: ev.level,
            brain: ev.brain.clone(),
            text: ev.text.clone(),
        });
        if expanded.contains(&ev.id) {
            if let Some(detail) = &ev.detail {
                for line in detail.lines() {
                    out.push(BenchLine {
                        id: ev.id,
                        depth: 1,
                        is_node: false,
                        has_children: false,
                        ts: String::new(),
                        level: ev.level,
                        brain: None,
                        text: line.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// Uebersetzt den `--view`-Wert in eine Ansicht. `None` = unbekannter Wert,
/// der Aufrufer behaelt dann seine Vorauswahl.
pub fn parse_view(raw: &str) -> Option<View> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "workers" | "worker" | "pool" => Some(View::Workers),
        "bench" | "benchmark" => Some(View::Bench),
        _ => None,
    }
}

/// Wie viele Pulswerte die Sparkline vorhält (~Fensterbreite in Ticks).
pub const ACTIVITY_HISTORY_LEN: usize = 60;

/// Die drei fokussierbaren Hauptpanels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Agents,
    Tasks,
    Log,
}

impl Panel {
    /// Nächstes Panel im Tab-Zyklus (Agents → Tasks → Log → Agents).
    pub fn next(self) -> Panel {
        match self {
            Panel::Agents => Panel::Tasks,
            Panel::Tasks => Panel::Log,
            Panel::Log => Panel::Agents,
        }
    }
}

/// Log-Filterstufen, per `f` durchgeschaltet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    /// Alle Zeilen.
    All,
    /// Nur Warnungen und Fehler.
    Warnings,
    /// Nur Fehler.
    Errors,
}

impl LogFilter {
    /// Nächste Stufe im Zyklus (All → Warnings → Errors → All).
    pub fn next(self) -> LogFilter {
        match self {
            LogFilter::All => LogFilter::Warnings,
            LogFilter::Warnings => LogFilter::Errors,
            LogFilter::Errors => LogFilter::All,
        }
    }

    /// Kurzlabel für die Panel-Überschrift.
    pub fn label(self) -> &'static str {
        match self {
            LogFilter::All => "alle",
            LogFilter::Warnings => "warn+",
            LogFilter::Errors => "fehler",
        }
    }

    /// `true`, wenn die Zeile bei dieser Stufe sichtbar ist. Severity wird
    /// heuristisch am Text erkannt (die Logs sind Freitext, keine strukturierten
    /// Level).
    pub fn keeps(self, line: &str) -> bool {
        match self {
            LogFilter::All => true,
            LogFilter::Warnings => line_is_warning(line) || line_is_error(line),
            LogFilter::Errors => line_is_error(line),
        }
    }
}

fn line_is_error(line: &str) -> bool {
    let l = line.to_lowercase();
    ["error", "fehler", "panic", "fail", "fatal", "✕", "crash"]
        .iter()
        .any(|k| l.contains(k))
}

fn line_is_warning(line: &str) -> bool {
    let l = line.to_lowercase();
    ["warn", "warnung", "cooldown", "retry", "stall", "timeout"]
        .iter()
        .any(|k| l.contains(k))
}

/// Input-Modi der TUI.
#[derive(Debug, PartialEq)]
pub enum InputMode {
    /// Normal-Modus, Keys werden interpretiert.
    Normal,
    /// Task-Eingabe (Enter-Taste gedrückt).
    TaskInput,
    /// Kommando-Eingabe (/ getippt).
    CommandInput,
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
        // Pool-Puls aufzeichnen: frisch pulsierende Worker (Heartbeat < 10s).
        let beats = self
            .agents
            .iter()
            .filter(|a| a.heartbeat_age_sec < 10)
            .count() as u64;
        self.activity_history.push_back(beats);
        while self.activity_history.len() > ACTIVITY_HISTORY_LEN {
            self.activity_history.pop_front();
        }
    }

    /// Aktueller Sparkline-Datensatz (älteste zuerst) für die Kopfleiste.
    pub fn activity_samples(&self) -> Vec<u64> {
        self.activity_history.iter().copied().collect()
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

    /// Benchmark-Baum: Cursor verschieben, auf die Zeilenliste geklemmt.
    /// `total_lines` ist die gefaltete Zeilenanzahl; 0 = Liste leer.
    pub fn bench_move(&mut self, total_lines: usize, delta: i64) {
        if total_lines == 0 {
            self.bench_selected = 0;
            return;
        }
        let next = self.bench_selected as i64 + delta;
        self.bench_selected = next.clamp(0, (total_lines - 1) as i64) as usize;
    }

    /// Benchmark-Baum: Cursor ans untere Ende (folgt dem frischen Strom).
    pub fn bench_bottom(&mut self, total_lines: usize) {
        self.bench_selected = total_lines.saturating_sub(1);
    }

    /// Benchmark-Baum: Knoten der angeklickten Zeile auf-/zuklappen.
    pub fn bench_toggle(&mut self, id: u64) {
        if self.bench_expanded.contains(&id) {
            self.bench_expanded.remove(&id);
        } else {
            self.bench_expanded.insert(id);
        }
    }

    /// Benchmark-Baum: gezielt aufklappen (Pfeil rechts, ohne Umschalten).
    pub fn bench_expand(&mut self, id: u64) {
        self.bench_expanded.insert(id);
    }

    /// Benchmark-Baum: gezielt zuklappen (Pfeil links).
    pub fn bench_collapse(&mut self, id: u64) {
        self.bench_expanded.remove(&id);
    }

    pub fn is_bench_expanded(&self, id: u64) -> bool {
        self.bench_expanded.contains(&id)
    }

    /// Benchmark-Baum: alle Knoten mit Detail aufklappen (`e`).
    pub fn bench_expand_all(&mut self) {
        for ev in crate::bench_events::snapshot() {
            if ev.detail.is_some() {
                self.bench_expanded.insert(ev.id);
            }
        }
    }

    /// Benchmark-Baum: alle Knoten zuklappen (`c`).
    pub fn bench_collapse_all(&mut self) {
        self.bench_expanded.clear();
    }

    /// Benchmark-Baum: ist mindestens ein Knoten mit Detail noch zugeklappt?
    pub fn bench_any_collapsed(&self) -> bool {
        crate::bench_events::snapshot()
            .iter()
            .any(|ev| ev.detail.is_some() && !self.bench_expanded.contains(&ev.id))
    }

    /// Benchmark-Baum: eine Taste togglet zwischen allem auf/allem zu (`e`).
    pub fn bench_toggle_all(&mut self) {
        if self.bench_any_collapsed() {
            self.bench_expand_all();
        } else {
            self.bench_collapse_all();
        }
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

    let mut agents: Vec<AgentView> = pool
        .entries
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
        .collect();
    // Feste alphabetische Reihenfolge — sonst springt der Fokus bei jedem
    // Reload, weil die HashMap-Iteration nicht-deterministisch ist.
    overlay_bench_activity(&mut agents);
    agents.sort_by(|a, b| a.brain.cmp(&b.brain));
    agents
}

/// Sekunden seit Mitternacht aus einem `HH:MM:SS`-Stempel.
fn seconds_of_day(hhmmss: &str) -> Option<u64> {
    let mut it = hhmmss.split(':');
    let h: u64 = it.next()?.parse().ok()?;
    let m: u64 = it.next()?.parse().ok()?;
    let s: u64 = it.next()?.parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// Alter eines `HH:MM:SS`-Stempels in Sekunden gegen die Ortszeit.
/// Die Ereignisse tragen kein Datum — daher der Tagesumbruch.
fn age_of_stamp(hhmmss: &str) -> Option<u64> {
    let then = seconds_of_day(hhmmss)?;
    let now = seconds_of_day(&crate::timestamp())?;
    Some(if now >= then {
        now - then
    } else {
        now + 86_400 - then
    })
}

/// Blendet die Aktivitaet des laufenden Benchmarks ueber die Pool-Daten.
///
/// Der Benchmark fasst den bot2bot-Worker-Pool NIE an (`benchmark.rs` kennt
/// `worker_pool` nicht einmal). Ohne diesen Abgleich zeigt das Dashboard
/// waehrend eines Laufs nur Altbestand — Zeilen in `cooldown`, Heartbeats
/// stundenalt — obwohl die Benchmark-Ansicht daneben mitloggt (Beschwerde
/// 2026-07-26). Uebernommen wird nur ehrlich Ableitbares; `pid` bleibt
/// unbekannt, weil der Ereignisstrom keine Prozesse kennt.
fn overlay_bench_activity(agents: &mut Vec<AgentView>) {
    use std::collections::HashMap;
    let events = crate::bench_events::snapshot();
    if events.is_empty() {
        return;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut last: HashMap<String, (String, u64)> = HashMap::new();
    let mut passes: HashMap<String, usize> = HashMap::new();
    let mut detail: HashMap<String, Vec<String>> = HashMap::new();

    for ev in &events {
        let Some(brain) = ev.brain.as_deref() else {
            continue;
        };
        if !seen.iter().any(|b| b == brain) {
            seen.push(brain.to_string());
        }
        if let Some(age) = age_of_stamp(&ev.ts) {
            last.insert(brain.to_string(), (ev.text.clone(), age));
        }
        if ev.level == crate::bench_events::Level::Pass {
            *passes.entry(brain.to_string()).or_default() += 1;
        }
        let d = detail.entry(brain.to_string()).or_default();
        d.push(format!("{} {}", ev.ts, ev.text));
        if d.len() > DETAIL_HISTORY {
            d.remove(0);
        }
    }

    let apply = |a: &mut AgentView| {
        if let Some((text, age)) = last.get(&a.brain) {
            a.heartbeat_age_sec = *age;
            a.last_log_line = Some(text.clone());
            // Frisch gemeldet = arbeitet gerade. Der Pool-Status stammt aus
            // einem anderen Subsystem und waere hier irrefuehrend.
            a.status = if *age < 60 {
                "active".to_string()
            } else if *age < 600 {
                "available".to_string()
            } else {
                "cooldown".to_string()
            };
        }
        if let Some(n) = passes.get(&a.brain) {
            a.tasks_done = *n;
        }
        if let Some(d) = detail.get(&a.brain) {
            a.detail = d.clone();
        }
    };

    for a in agents.iter_mut() {
        apply(a);
    }
    // Brains ohne Pool-Eintrag ergaenzen — sonst fehlt genau das Brain, das
    // gerade arbeitet.
    for brain in seen {
        if agents.iter().any(|a| a.brain == brain) {
            continue;
        }
        let mut a = AgentView {
            brain,
            status: "available".to_string(),
            pid: None,
            heartbeat_age_sec: u64::MAX,
            tasks_pending: 0,
            tasks_done: 0,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        };
        apply(&mut a);
        agents.push(a);
    }
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
                        .map(|b| {
                            if kind.is_empty() {
                                b.to_string()
                            } else {
                                format!("[{kind}] {b}")
                            }
                        })
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
    fn view_parameter_waehlt_die_startansicht() {
        assert_eq!(parse_view("workers"), Some(View::Workers));
        assert_eq!(parse_view("Worker"), Some(View::Workers));
        assert_eq!(parse_view("bench"), Some(View::Bench));
        assert_eq!(parse_view(" BENCHMARK "), Some(View::Bench));
        // Unbekanntes bleibt None, damit der Aufrufer seine Wahl behaelt.
        assert_eq!(parse_view("quatsch"), None);
    }

    #[test]
    fn benchmark_aktivitaet_erreicht_das_worker_dashboard() {
        crate::bench_events::clear();
        let mut agents = vec![AgentView {
            brain: "deepseek".to_string(),
            status: "cooldown".to_string(),
            pid: None,
            heartbeat_age_sec: u64::MAX,
            tasks_pending: 0,
            tasks_done: 0,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        }];
        crate::bench_events::emit(
            crate::bench_events::Level::Pass,
            Some("deepseek"),
            "deepseek: Tests gruen",
        );
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some("zai"),
            "zai: Iteration 1",
        );

        overlay_bench_activity(&mut agents);

        let ds = agents.iter().find(|a| a.brain == "deepseek").unwrap();
        assert_eq!(ds.status, "active");
        assert!(ds.heartbeat_age_sec < 60, "Heartbeat blieb alt");
        assert_eq!(ds.last_log_line.as_deref(), Some("deepseek: Tests gruen"));
        assert_eq!(ds.tasks_done, 1);
        assert!(!ds.detail.is_empty());
        assert!(
            agents.iter().any(|a| a.brain == "zai"),
            "arbeitendes Brain ohne Pool-Eintrag fehlt"
        );

        // Gegenprobe: ohne Ereignisse bleibt der Pool-Zustand unangetastet.
        crate::bench_events::clear();
        let mut u = vec![AgentView {
            brain: "kimi".to_string(),
            status: "cooldown".to_string(),
            pid: None,
            heartbeat_age_sec: 999,
            tasks_pending: 3,
            tasks_done: 7,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        }];
        overlay_bench_activity(&mut u);
        assert_eq!(u[0].status, "cooldown");
        assert_eq!(u[0].tasks_done, 7);
        assert_eq!(u.len(), 1);
        crate::bench_events::clear();
    }

    #[test]
    fn stempel_alter_ueberlebt_den_tageswechsel() {
        assert_eq!(seconds_of_day("01:02:03"), Some(3723));
        assert!(seconds_of_day("kaputt").is_none());
        assert!(age_of_stamp(&crate::timestamp()).unwrap() < 5);
    }

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
            focus: Panel::Agents,
            log_filter: LogFilter::All,
            activity_history: std::collections::VecDeque::new(),
            view: View::Workers,
            bench_scroll: 0,
            bench_expanded: std::collections::HashSet::new(),
            bench_selected: 0,
            command_input: String::new(),
            wall_status: String::new(),
        }
    }

    #[test]
    fn selected_row_shifts_past_expanded_details() {
        // Der eigentliche Fallstrick: Agent 2 steht nach dem Aufklappen von
        // Agent 0 nicht mehr in Zeile 2, sondern in Zeile 6.
        assert_eq!(selected_row(&[0, 0, 0], 2), 2, "alles zu: Index = Zeile");
        assert_eq!(selected_row(&[4, 0, 0], 2), 6);
        assert_eq!(selected_row(&[4, 3, 0], 2), 9);
        assert_eq!(
            selected_row(&[4, 3, 0], 0),
            0,
            "erster Agent bleibt Zeile 0"
        );
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
        assert_eq!(
            wrap_text("abc", 0),
            vec!["a", "b", "c"],
            "Breite 0 darf nicht endlos schleifen"
        );
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
        assert_eq!(
            app.detail_scroll, 0,
            "sonst startet das naechste Aufklappen mitten drin"
        );
    }

    #[test]
    fn on_tick_records_pool_pulse_and_caps_history() {
        // Der Puls-Ringpuffer speist die Live-Sparkline: je Tick die Zahl frisch
        // pulsierender Worker (Heartbeat < 10s), gedeckelt auf ACTIVITY_HISTORY_LEN.
        let mut app = app_with(vec![
            view("a", vec![]), // heartbeat 0s -> pulsiert
            view("b", vec![]), // heartbeat 0s -> pulsiert
        ]);
        app.on_tick(1.0);
        assert_eq!(
            app.activity_samples().last().copied(),
            Some(2),
            "2 frische Worker"
        );
        for _ in 0..(ACTIVITY_HISTORY_LEN + 20) {
            app.on_tick(1.0);
        }
        assert_eq!(
            app.activity_history.len(),
            ACTIVITY_HISTORY_LEN,
            "Ringpuffer gedeckelt"
        );
    }

    #[test]
    fn stale_workers_do_not_count_as_pulse() {
        let mut app = app_with(vec![AgentView {
            heartbeat_age_sec: 999,
            ..view("z", vec![])
        }]);
        app.on_tick(1.0);
        assert_eq!(
            app.activity_samples().last().copied(),
            Some(0),
            "toter Worker pulsiert nicht"
        );
    }

    #[test]
    fn fold_bench_events_klappt_details_als_kinder_ein() {
        use crate::bench_events::{BenchEvent, Level};
        let kopf = BenchEvent {
            id: 1,
            ts: "10:00:00".into(),
            level: Level::Info,
            brain: None,
            text: "[shell:step-1] cargo test".into(),
            detail: Some(
                "[Terminal-Ausgabe action_id=step-1]\nalles gut\n[exit_code: 0]".into(),
            ),
        };
        let meldung = BenchEvent {
            id: 2,
            ts: "10:00:01".into(),
            level: Level::Pass,
            brain: Some("kimi".into()),
            text: "[brain:kimi] ok 100ms 500Z".into(),
            detail: None,
        };
        let events = vec![kopf, meldung];
        let mut expanded = std::collections::HashSet::new();

        let flach = fold_bench_events(&events, &expanded);
        assert_eq!(flach.len(), 2, "alles zu: nur die Knoten");
        assert_eq!(flach[0].depth, 0);
        assert!(flach[0].has_children);
        assert!(!flach[1].has_children);
        assert!(flach[1].is_node);

        expanded.insert(1);
        let baum = fold_bench_events(&events, &expanded);
        assert_eq!(
            baum.len(),
            5,
            "aufgeklappt: 1 Knoten + 3 Detailzeilen + 1 Knoten"
        );
        assert_eq!(baum[0].id, 1);
        assert_eq!(baum[0].depth, 0);
        assert_eq!(baum[1].text, "[Terminal-Ausgabe action_id=step-1]");
        assert_eq!(baum[1].depth, 1);
        assert!(!baum[1].is_node, "Detailzeile ist kein Knoten");
        assert_eq!(baum[2].text, "alles gut");
        assert_eq!(baum[3].text, "[exit_code: 0]");
        assert_eq!(baum[4].id, 2, "naechster Knoten folgt unveraendert");
        assert_eq!(baum[4].depth, 0);
    }

    #[test]
    fn bench_navigation_klemmt_und_klappt_ueber_ids() {
        let mut app = app_with(vec![]);
        assert_eq!(app.bench_selected, 0);
        app.bench_move(0, 5);
        assert_eq!(app.bench_selected, 0, "leere Liste klemmt auf 0");
        app.bench_move(10, -1);
        assert_eq!(app.bench_selected, 0, "nie unter 0");
        app.bench_move(10, 5);
        assert_eq!(app.bench_selected, 5);
        app.bench_move(10, 100);
        assert_eq!(app.bench_selected, 9, "nie ueber den Rand");
        app.bench_bottom(10);
        assert_eq!(app.bench_selected, 9, "g springt ans Ende");
        app.bench_toggle(42);
        assert!(app.is_bench_expanded(42));
        app.bench_toggle(42);
        assert!(!app.is_bench_expanded(42), "Toggle schliesst wieder");
        app.bench_expand(7);
        app.bench_collapse(7);
        assert!(!app.is_bench_expanded(7));
    }

    #[test]
    fn eine_taste_togglet_den_ganzen_baum_auf_und_zu() {
        use crate::bench_events::Level;
        let _guard = crate::bench_events::test_bus_mutex().lock();
        crate::bench_events::clear();
        crate::bench_events::emit_detailed(Level::Info, None, "knoten-a", Some("detail a"));
        crate::bench_events::emit_detailed(Level::Info, None, "knoten-b", Some("detail b"));
        crate::bench_events::emit(Level::Info, None, "flach");
        let events = crate::bench_events::snapshot();
        assert_eq!(events.len(), 3);

        let mut app = app_with(vec![]);
        app.bench_toggle_all();
        assert_eq!(
            app.bench_expanded.len(),
            2,
            "erster Toggle klappt alle Knoten mit Detail auf"
        );
        assert!(app.is_bench_expanded(events[0].id));
        assert!(app.is_bench_expanded(events[1].id));
        assert!(!app.is_bench_expanded(events[2].id), "flach hat kein Detail");

        app.bench_toggle_all();
        assert!(
            app.bench_expanded.is_empty(),
            "zweiter Toggle klappt den ganzen Baum zu"
        );

        app.bench_toggle_all();
        assert_eq!(
            app.bench_expanded.len(),
            2,
            "dritter Toggle klappt wieder alles auf"
        );
    }
}
