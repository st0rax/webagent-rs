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
use std::sync::{Mutex, OnceLock};

/// Wie viele Meldungen vorgehalten werden.
///
/// Ein langer Lauf produziert Zehntausende Zeilen; der Puffer deckelt das,
/// damit ein Nachtlauf nicht den Speicher frisst. Aeltestes faellt raus.
pub const CAPACITY: usize = 2000;

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
    /// Ortszeit `HH:MM:SS` zum Zeitpunkt der Meldung.
    pub ts: String,
    pub level: Level,
    /// Betroffenes Brain, falls die Meldung einem zuzuordnen ist.
    pub brain: Option<String>,
    pub text: String,
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

/// Haengt eine Meldung an. Der Zeitstempel wird hier gesetzt.
pub fn emit(level: Level, brain: Option<&str>, text: &str) {
    let ev = BenchEvent {
        ts: crate::timestamp(),
        level,
        brain: brain.map(|b| b.to_string()),
        text: text.to_string(),
    };
    let mut q = lock();
    if q.len() >= CAPACITY {
        q.pop_front();
    }
    q.push_back(ev);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Bus ist prozessglobal, Tests laufen parallel: alle Faelle teilen
    /// sich EINEN Test, damit sie sich nicht gegenseitig den Puffer umbauen.
    /// Ein flackernder Test waere schlimmer als kein Test.
    #[test]
    fn ringpuffer_haelt_reihenfolge_deckel_und_seit_index_ein() {
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
}
