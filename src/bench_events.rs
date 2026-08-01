//! Ereignisstrom des Benchmark-/Loop-Laufs.
//!
//! Bisher ging das Geschehen ausschliesslich als `println!` nach stdout — die
//! ratatui-TUI in [`crate::tui`] konnte davon nichts sehen, weshalb
//! `webagent tui` und die Benchmark-Ausgabe zwei getrennte Welten waren.
//! Dieser Bus ist die gemeinsame Quelle: der Benchmark meldet hierher, die
//! Konsole druckt weiter wie gehabt, und die TUI liest denselben Strom.
//!
//! Bewusst ein prozessglobaler Ringpuffer und kein Kanal: die TUI pollt im
//! Rendertakt und darf Frames verpassen, ohne dass der Benchmark blockiert
//! oder Meldungen verloren gehen.

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex, OnceLock,
};

/// Wie viele Meldungen vorgehalten werden.
///
/// Ein langer Lauf produziert Zehntausende Zeilen; der Puffer deckelt das,
/// damit ein Nachtlauf nicht den Speicher frisst. Aeltestes faellt raus.
pub const CAPACITY: usize = 2000;

/// Wie viele Zeichen ein ausklappbarer Detailblock hoechstens in den Bus
/// uebernimmt. Ein Controller-Step oder eine Brain-Antwort darf mehrere
/// Kilobyte gross sein; damit ein Nachtlauf nicht den Puffer sprengt, wird das
/// Detail gekappt (der Knoten-Text bleibt immer vollstaendig).
const DETAIL_CAP_CHARS: usize = 6000;

/// Laufende ID je Ereignis. Die TUI braucht eine stabile Adresse, um Knoten
/// auf-/zuzuklappen — die Position im Ringpuffer wandert ja mit jedem Eintrag.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_event_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Die Ratatui-Ansicht besitzt den Terminal-Renderer. Direkte stdout/stderr-
/// Zeilen würden ihren Alternate Screen zerreißen, daher werden sie während
/// dieser Ansicht zentral unterdrückt. Die strukturierten Ereignisse bleiben
/// davon unberührt und sind weiter im Dashboard sichtbar.
static CONSOLE_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_console_output(enabled: bool) {
    CONSOLE_OUTPUT_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn console_output_enabled() -> bool {
    CONSOLE_OUTPUT_ENABLED.load(Ordering::Relaxed)
}

/// Echoe die unstrukturierten `print_line`-Zeilen zusaetzlich in den
/// Ereignisbus. Der TUI-Benchmark schaltet das bei `--verbose` an: die
/// Schritt-fuer-Schritt-Zeilen des Controllers (`[shell:id] …`, `[edit:id] …`,
/// Antworttexte) landen dann sichtbar im "Ereignisstrom", statt nur an stdout
/// zu gehen, das die ratatui-Ansicht unterdrueckt.
static ECHO_TO_BUS: AtomicBool = AtomicBool::new(false);

pub fn set_echo_bus(enabled: bool) {
    ECHO_TO_BUS.store(enabled, Ordering::Relaxed);
}

/// Steht der Spiegelmodus? `true`, sobald der Benchmark mit `--verbose` laeuft.
///
/// Er steuert nicht nur, ob `print_line` in den Bus spiegelt, sondern auch, ob
/// die Logquellen (brain_score-events.jsonl, Run-Events, Controller-Ausgaben)
/// ihre Details hierher spiegeln. Im normalen Betrieb bleibt der Bus unberuehrt.
pub fn echo_bus_enabled() -> bool {
    ECHO_TO_BUS.load(Ordering::Relaxed)
}

pub fn print_line(text: &str) {
    if ECHO_TO_BUS.load(Ordering::Relaxed) {
        emit(Level::Info, None, text);
    }
    console_print(text);
}

/// Wie [`print_line`], aber mit ausklappbarem Detailblock: der Knoten-Text ist
/// die Zusammenfassung (`[shell:step-1] cargo test --lib`), `detail` der volle
/// Inhalt (Terminal-Ausgabe, Edit-Ergebnis, Antworttext). In der TUI-Baumansicht
/// wird das Detail per Enter/Rechts auf-, mit Links wieder zugeklappt.
pub fn print_detailed(text: &str, detail: Option<&str>) {
    if ECHO_TO_BUS.load(Ordering::Relaxed) {
        emit_detailed(Level::Info, None, text, detail);
    }
    console_print(text);
}

fn console_print(text: &str) {
    if console_output_enabled() {
        println!("{text}");
    }
}

/// Meldet einen fachlichen Fortschritt sowohl strukturiert an die TUI als auch
/// an die normale Konsole. Für längere Phasen gedacht, nicht für Ticker.
pub fn info_line(text: &str) {
    emit(Level::Info, None, text);
    console_print(text);
}

pub fn eprint_line(text: &str) {
    if console_output_enabled() {
        eprintln!("{text}");
    }
}

/// Schweregrad einer Meldung — die TUI faerbt danach ein.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Progress,
    Pass,
    Fail,
    Warn,
}

/// Eine Zeile Benchmark-/Loop-Geschehen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchEvent {
    /// Stabile Adresse fuer die TUI (Auf-/Zuklappen im Baum).
    pub id: u64,
    /// Ortszeit `HH:MM:SS` zum Zeitpunkt der Meldung.
    pub ts: String,
    pub level: Level,
    /// Betroffenes Brain, falls die Meldung einem zuzuordnen ist.
    pub brain: Option<String>,
    /// Zusammenfassung / Knoten-Text. Bleibt im Baum immer sichtbar.
    pub text: String,
    /// Ausklappbarer Detailblock, `None` = der Text ist bereits vollstaendig.
    /// Mehrzeilige Inhalte (Terminal-Ausgabe, Antworttext, Payload) werden von
    /// der Baumansicht eingerueckt unter dem Knoten gerendert.
    pub detail: Option<String>,
}

fn bus() -> &'static Mutex<VecDeque<BenchEvent>> {
    static BUS: OnceLock<Mutex<VecDeque<BenchEvent>>> = OnceLock::new();
    BUS.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Sperrt den Bus und uebersteht dabei einen vergifteten Mutex.
///
/// Ein Panic in irgendeinem Thread darf den Lauf nicht mitreissen — der
/// Ereignisstrom ist Anzeige, keine kritische Zustandsverwaltung.
fn lock() -> std::sync::MutexGuard<'static, VecDeque<BenchEvent>> {
    bus().lock().unwrap_or_else(|e| e.into_inner())
}

/// Haengt eine Meldung an (ohne Detailblock). Der Zeitstempel wird hier gesetzt.
pub fn emit(level: Level, brain: Option<&str>, text: &str) {
    emit_detailed(level, brain, text, None);
}

/// Haengt eine Meldung mit ausklappbarem Detailblock an.
pub fn emit_detailed(level: Level, brain: Option<&str>, text: &str, detail: Option<&str>) {
    let ev = BenchEvent {
        id: next_event_id(),
        ts: crate::timestamp(),
        level,
        brain: brain.map(|b| b.to_string()),
        text: text.to_string(),
        detail: detail.map(cap_detail),
    };
    let mut q = lock();
    if q.len() >= CAPACITY {
        q.pop_front();
    }
    q.push_back(ev);
}

/// Kappt einen Detailblock auf [`DETAIL_CAP_CHARS`] Zeichen, zeichensicher.
fn cap_detail(raw: &str) -> String {
    if raw.len() <= DETAIL_CAP_CHARS {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(DETAIL_CAP_CHARS).collect();
    out.push_str("\n… (gekürzt)");
    out
}

/// Vollstaendige Kopie des Puffers.
pub fn snapshot() -> Vec<BenchEvent> {
    lock().iter().cloned().collect()
}

/// Nur die Meldungen ab Index `n`, damit die TUI nicht jedes Frame alles
/// kopiert. Liegt `n` hinter dem Ende (Puffer wurde geleert), kommt nichts.
pub fn snapshot_since(n: usize) -> Vec<BenchEvent> {
    let q = lock();
    if n >= q.len() {
        return Vec::new();
    }
    q.iter().skip(n).cloned().collect()
}

pub fn len() -> usize {
    lock().len()
}

pub fn is_empty() -> bool {
    len() == 0
}

/// Leert den Puffer. Fuer Tests und fuer den Start eines neuen Laufs.
pub fn clear() {
    lock().clear();
}

/// Test-Serialisierung fuer alle Tests, die den Bus manipulieren.
///
/// Der Bus ist prozessglobal und die Tests laufen parallel: ein Test, der
/// `clear()`/`len()` prüft oder `set_echo_bus` schaltet, darf nicht neben
/// einem anderen laufen, der den Puffer fuellt. Deshalb greifen alle
/// Bus-Tests auf denselben Mutex zu.
#[cfg(test)]
pub fn test_bus_mutex() -> &'static Mutex<()> {
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    &TEST_LOCK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Bus ist prozessglobal, Tests laufen parallel: alle Faelle teilen
    /// sich EINEN Test, damit sie sich nicht gegenseitig den Puffer umbauen.
    /// Ein flackernder Test waere schlimmer als kein Test.
    #[test]
    fn ringpuffer_haelt_reihenfolge_deckel_und_seit_index_ein() {
        let _test_guard = test_bus_mutex().lock();
        clear();

        // --- Reihenfolge bleibt erhalten -------------------------------
        emit(Level::Info, None, "erste");
        emit(Level::Pass, Some("deepseek"), "zweite");
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].text, "erste");
        assert_eq!(snap[1].text, "zweite");
        assert_eq!(snap[1].brain.as_deref(), Some("deepseek"));
        assert_eq!(snap[1].level, Level::Pass);
        assert!(!snap[0].ts.is_empty(), "Zeitstempel fehlt");

        // --- snapshot_since liefert nur Neues --------------------------
        let vorher = len();
        emit(Level::Warn, None, "dritte");
        let neu = snapshot_since(vorher);
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].text, "dritte");
        // Index hinter dem Ende ist kein Fehler, sondern schlicht leer.
        assert!(snapshot_since(len() + 10).is_empty());

        // --- Deckel greift, Aeltestes faellt raus ----------------------
        clear();
        for i in 0..(CAPACITY + 50) {
            emit(Level::Progress, None, &format!("e{i}"));
        }
        assert_eq!(len(), CAPACITY, "Ringpuffer haelt den Deckel nicht ein");
        let snap = snapshot();
        assert_eq!(
            snap.first().unwrap().text,
            "e50",
            "Aeltestes nicht verworfen"
        );
        assert_eq!(snap.last().unwrap().text, format!("e{}", CAPACITY + 49));

        clear();
        assert!(is_empty());
    }

    #[test]
    fn echo_bus_spiegelt_print_line_nur_wenn_angeschaltet() {
        let _test_guard = test_bus_mutex().lock();
        // Regression 2026-08-01: der TUI-Benchmark soll die Schritt-Zeilen
        // (shell/edit/message) im Ereignisstrom zeigen — print_line muss dazu
        // bei `--verbose` in den Bus spiegeln, ohne Echo aber unverändert
        // nur an die Konsole gehen.
        let vorher_console = console_output_enabled();
        let vorher_echo = ECHO_TO_BUS.load(Ordering::Relaxed);
        set_console_output(false);
        clear();

        set_echo_bus(true);
        print_line("[shell:step-1] cargo test --lib");
        assert_eq!(len(), 1, "Echo an: Schritt-Zeile muss im Puffer landen");
        assert_eq!(snapshot()[0].text, "[shell:step-1] cargo test --lib");
        assert_eq!(snapshot()[0].detail, None, "print_line hat kein Detail");

        print_detailed(
            "[shell:step-2] cargo test",
            Some("[Terminal-Ausgabe action_id=step-2]\nalles gut\n[exit_code: 0]"),
        );
        assert_eq!(len(), 2, "print_detailed spiegelt in den Puffer");
        assert_eq!(snapshot()[1].text, "[shell:step-2] cargo test");
        assert_eq!(
            snapshot()[1].detail.as_deref(),
            Some("[Terminal-Ausgabe action_id=step-2]\nalles gut\n[exit_code: 0]"),
            "Detailblock muss den Baum fuettern"
        );
        assert_ne!(
            snapshot()[1].id, snapshot()[0].id,
            "Jeder Knoten braucht eine eigene, stabile ID"
        );

        set_echo_bus(false);
        print_line("[shell:step-3] ls");
        assert_eq!(len(), 2, "Echo aus: nichts im Puffer");

        // Aufraeumen, damit kein Test den anderen infiziert.
        set_echo_bus(vorher_echo);
        set_console_output(vorher_console);
        clear();
    }
}
