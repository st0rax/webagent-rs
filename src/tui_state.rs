//! tui_state — App-State + State-Management für TUI
//!
//! Kern-Strukturen: App, AgentView, select_wrap, on_tick.
//! Die dateibasierte Beschaffung (`load_state`) liegt in [`crate::tui_load`].
//! Rein-Rust, keine I/O im Modul.

pub use crate::tui_load::load_state;

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
    /// Rückmeldung der Brain-Kachelansicht (`w`), z.B. „Wall: 8 Fenster gekachelt".
    ///
    /// Ohne diese Zeile bliebe ein fehlgeschlagenes Anordnen unsichtbar: die
    /// Fenster liegen off-screen, man sieht also weder Erfolg noch Misserfolg.
    pub grid_status: String,
    /// Cursor in der Faehigkeiten-Ansicht.
    pub cap_selected: usize,
    /// Rueckmeldung der zuletzt geschalteten Faehigkeit.
    ///
    /// Zeigt ausdruecklich, ob der Zustandswechsel BELEGT wurde — ein Klick
    /// ohne nachweisbare Wirkung ist in diesem Projekt kein Koennen.
    pub cap_status: String,
    /// Cursor in der Einstellungen-Ansicht.
    pub cfg_selected: usize,
    /// Rueckmeldung der zuletzt geaenderten Einstellung — inklusive der Frage,
    /// ob sie sofort oder erst beim naechsten Lauf wirkt.
    pub cfg_status: String,
}

/// Die umschaltbaren Hauptansichten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// Worker-/Subagent-Dashboard (Agenten, Tasks, Log).
    Workers,
    /// Arbeits-/Benchmark-Ansicht: der Ereignisstrom des laufenden Laufs.
    Bench,
    /// Einstellungen: was gilt, woher es kommt, und umschaltbar.
    ///
    /// Alle Stellschrauben lebten bisher nur in Umgebungsvariablen — man sah
    /// nicht, was gilt, und ein vergessenes Flag aenderte das Verhalten stumm.
    Config,
    /// Faehigkeiten je Brain — anzeigen UND schalten.
    ///
    /// Die CLI kann Reasoning, Modellwechsel, Websuche und den temporaeren
    /// Chat laengst fahren (`webagent toggle`, `model`, `menu`, `mode`). In der
    /// TUI gab es dafuer nichts, obwohl dort der Mensch sitzt.
    Capabilities,
}

impl View {
    pub fn next(self) -> View {
        match self {
            View::Workers => View::Bench,
            View::Bench => View::Capabilities,
            View::Capabilities => View::Config,
            View::Config => View::Workers,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            View::Workers => "Worker",
            View::Bench => "Benchmark",
            View::Capabilities => "Faehigkeiten",
            View::Config => "Einstellungen",
        }
    }
}

/// Zustand einer Faehigkeit in der Faehigkeiten-Ansicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapState {
    /// Fahrbar — per Taste schaltbar.
    Driveable,
    /// Angeboten, aber noch kein Code bzw. keine Selektoren.
    Quest,
    /// Fuer diesen Harness nicht nachweisbar fahrbar; zaehlt nicht im Nenner.
    OutOfReach,
}

/// Eine Zeile der Faehigkeiten-Ansicht.
#[derive(Debug, Clone, PartialEq)]
pub struct CapRow {
    pub brain: String,
    /// `None` = Kopfzeile des Brains.
    pub key: Option<String>,
    pub label: String,
    pub state: CapState,
}

impl CapRow {
    /// Nur fahrbare Zeilen lassen sich schalten.
    pub fn is_actionable(&self) -> bool {
        self.key.is_some() && self.state == CapState::Driveable
    }
}

/// Baut die Zeilenliste der Faehigkeiten-Ansicht.
///
/// Nicht fahrbare Faehigkeiten werden bewusst ANGEZEIGT statt versteckt: sie
/// sind die Landkarte dessen, was ein Brain koennte und webagent noch nicht
/// kann. Ein Panel, das nur das Erreichte zeigt, sieht immer fertig aus.
pub fn capability_rows(levels: &[crate::capability::BrainLevel]) -> Vec<CapRow> {
    let mut rows = Vec::new();
    for lvl in levels {
        let head = match lvl.max_level() {
            Some(max) => format!("{} [{}/{}]", lvl.brain_id, lvl.level(), max),
            // Unvermessen: ein Maximum zu behaupten waere geraten.
            None => format!("{} [{}/?] unvermessen", lvl.brain_id, lvl.level()),
        };
        rows.push(CapRow {
            brain: lvl.brain_id.clone(),
            key: None,
            label: head,
            state: CapState::Quest,
        });
        for key in &lvl.have {
            rows.push(CapRow {
                brain: lvl.brain_id.clone(),
                key: Some(key.clone()),
                label: key.clone(),
                state: CapState::Driveable,
            });
        }
        for quest in &lvl.quests {
            rows.push(CapRow {
                brain: lvl.brain_id.clone(),
                key: Some(quest.key.clone()),
                // Der Blocker ist der eigentliche Wert der Quest — sonst waere
                // ein verfallener Beleg ("Beleg verfallen") von "nie verifiziert"
                // nicht zu unterscheiden.
                label: format!(
                    "{} — {} ({})",
                    quest.key,
                    quest.label,
                    quest.blocker.as_str()
                ),
                state: CapState::Quest,
            });
        }
        for key in &lvl.out_of_reach {
            rows.push(CapRow {
                brain: lvl.brain_id.clone(),
                key: Some(key.clone()),
                label: format!("{key} — nicht belegbar"),
                state: CapState::OutOfReach,
            });
        }
    }
    rows
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
        "config" | "einstellungen" | "settings" => Some(View::Config),
        "capabilities" | "faehigkeiten" | "fähigkeiten" => Some(View::Capabilities),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faehigkeiten_zeigen_auch_die_luecken() {
        use crate::capability::{BrainLevel, Quest, QuestBlocker};

        let level = BrainLevel {
            brain_id: "qwen".to_string(),
            surveyed: true,
            available: vec!["chat".into(), "temporary_chat".into(), "voice_mode".into()],
            have: vec!["chat".into()],
            quests: vec![Quest {
                brain_id: "qwen".to_string(),
                key: "temporary_chat".to_string(),
                label: "Temporären Chat nutzen".to_string(),
                blocker: QuestBlocker::NeedsCode,
            }],
            out_of_reach: vec!["voice_mode".into()],
            verified: vec![("chat".to_string(), "2026-01-01T00:00:00Z".into())],
        };
        let rows = capability_rows(&[level]);

        // Kopfzeile mit Level, dann fahrbar, dann Quest, dann ausser Reichweite.
        assert_eq!(rows[0].key, None);
        assert!(rows[0].label.contains("qwen [1/3]"), "{}", rows[0].label);
        assert_eq!(rows[1].state, CapState::Driveable);
        assert_eq!(rows[2].state, CapState::Quest);
        assert_eq!(rows[3].state, CapState::OutOfReach);

        // Nur Fahrbares laesst sich schalten — eine Quest ist kein Knopf.
        assert!(rows[1].is_actionable());
        assert!(!rows[2].is_actionable(), "Quest darf nicht schaltbar sein");
        assert!(!rows[3].is_actionable());
        assert!(!rows[0].is_actionable(), "Kopfzeile ist kein Knopf");

        // Die Luecken MUESSEN sichtbar sein: ein Panel, das nur das Erreichte
        // zeigt, sieht immer fertig aus.
        assert_eq!(rows.len(), 4, "Quests und Unerreichbares gehoeren mit rein");
    }

    #[test]
    fn verfallener_beleg_ist_in_der_zeile_sichtbar() {
        use crate::capability::{BrainLevel, Quest, QuestBlocker};

        // Ein Beleg, der mal lief und verfiel, ist etwas anderes als eine nie
        // verifizierte Faehigkeit: „nochmal verifizieren", nicht „neu lernen".
        // Die TUI-Zeile muss das tragen (Phase 7 des Plans).
        let level = BrainLevel {
            brain_id: "qwen".to_string(),
            surveyed: true,
            available: vec!["chat".into()],
            have: Vec::new(),
            quests: vec![Quest {
                brain_id: "qwen".to_string(),
                key: "chat".to_string(),
                label: "Chat oeffnen".to_string(),
                blocker: QuestBlocker::ProofExpired,
            }],
            out_of_reach: Vec::new(),
            verified: Vec::new(),
        };
        let rows = capability_rows(&[level]);
        let quest_row = &rows[1];
        assert_eq!(quest_row.state, CapState::Quest);
        assert!(
            quest_row.label.contains("Beleg verfallen"),
            "{}",
            quest_row.label
        );
        assert!(!quest_row.is_actionable());
    }

    #[test]
    fn unvermessenes_brain_behauptet_kein_maximum() {
        use crate::capability::BrainLevel;
        // gemini stand am 02.08.2026 auf 0/0 ohne eine einzige katalogisierte
        // Option. „0/0" laese sich als „fertig" missverstehen.
        let level = BrainLevel {
            brain_id: "gemini".to_string(),
            surveyed: false,
            available: Vec::new(),
            have: Vec::new(),
            quests: Vec::new(),
            out_of_reach: Vec::new(),
            verified: Vec::new(),
        };
        let rows = capability_rows(&[level]);
        assert!(
            rows[0].label.contains("unvermessen"),
            "{}",
            rows[0].label
        );
    }

    #[test]
    fn ansichten_rotieren_durch_alle() {
        assert_eq!(View::Workers.next(), View::Bench);
        assert_eq!(View::Bench.next(), View::Capabilities);
        assert_eq!(View::Capabilities.next(), View::Config);
        assert_eq!(View::Config.next(), View::Workers);
    }

    /// Jede Ansicht muss auch ueber `--view` waehlbar sein.
    ///
    /// Die Liste in `cli.rs` (`value_parser`) und `parse_view` sind zwei
    /// Stellen fuer dieselbe Frage. Sie liefen bereits auseinander: parse_view
    /// kannte vier Ansichten, clap liess zwei zu — die beiden anderen waren
    /// ueber die Kommandozeile unerreichbar, ohne Fehlermeldung.
    #[test]
    fn jede_ansicht_ist_per_view_parameter_waehlbar() {
        for (name, erwartet) in [
            ("workers", View::Workers),
            ("bench", View::Bench),
            ("capabilities", View::Capabilities),
            ("config", View::Config),
        ] {
            assert_eq!(parse_view(name), Some(erwartet), "--view {name}");
        }
    }

    /// Jede Ansicht muss per `v` erreichbar sein — und der Rundlauf muss sich
    /// schliessen.
    ///
    /// Bewusst ohne feste Zahl: eine neue Ansicht, die jemand aus `next()`
    /// vergisst, faellt hier auf, statt still unerreichbar zu bleiben. Genau
    /// das ist beim Hinzufuegen von `Config` beinahe passiert.
    #[test]
    fn jede_ansicht_ist_per_v_erreichbar() {
        let alle = [View::Workers, View::Bench, View::Capabilities, View::Config];
        let mut gesehen = Vec::new();
        let mut v = View::Workers;
        for _ in 0..alle.len() {
            gesehen.push(v);
            v = v.next();
        }
        assert_eq!(v, View::Workers, "der Rundlauf schliesst sich nicht");
        for a in alle {
            assert!(gesehen.contains(&a), "{a:?} ist per v nicht erreichbar");
            assert!(!a.label().trim().is_empty(), "{a:?} ohne Beschriftung");
        }
    }

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
            grid_status: String::new(),
            cap_selected: 0,
            cap_status: String::new(),
            cfg_selected: 0,
            cfg_status: String::new(),
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
