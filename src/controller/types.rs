//! Gemeinsame Typen des Controllers (Turn-Ergebnis, Run-Optionen).

use std::sync::Arc;

/// Ergebnis eines einzelnen Brain-Turns.
#[derive(Debug, Clone)]
pub struct BrainTurn {
    pub text: String,
    pub complete: bool,
}

/// Art einer echten Controller-Observation (für Aufsicht/Spiel, nie für Belege).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationKind {
    Shell,
    File,
    Message,
}

/// Was der Controller wirklich ausgeführt und zurückgegeben hat.
#[derive(Debug, Clone)]
pub struct ObservationEvent {
    pub kind: ObservationKind,
    pub action_id: String,
    pub ok: bool,
    pub text: String,
}

/// Beobachter für echte Observations. Der Hook sieht nur, was der Controller
/// auch in den Feedback-Loop gibt — kein Selbstbericht des Brains.
pub type ObservationHook = Arc<dyn Fn(ObservationEvent) + Send + Sync>;

/// Optionen für `AgentController::run_with_options` (REPL: Browser-Session offen lassen).
#[derive(Default, Clone)]
pub struct RunOptions {
    pub skip_brain_start: bool,
    pub skip_brain_stop: bool,
    /// Fuer objektive Coding-Benchmarks: keine alten Run-Episoden/Wiki-Seiten
    /// in die neue Aufgabe mischen. Verhindert Datenleck und stale Pfade.
    pub suppress_memory_context: bool,
    /// Optionaler Beobachter für echte Observations. Default: keiner.
    pub observation_hook: Option<ObservationHook>,
}
