//! Gemeinsame Typen des Controllers (Turn-Ergebnis, Run-Optionen).

/// Ergebnis eines einzelnen Brain-Turns.
#[derive(Debug, Clone)]
pub struct BrainTurn {
    pub text: String,
    pub complete: bool,
}

/// Optionen für `AgentController::run_with_options` (REPL: Browser-Session offen lassen).
#[derive(Debug, Clone, Copy, Default)]
pub struct RunOptions {
    pub skip_brain_start: bool,
    pub skip_brain_stop: bool,
    /// Fuer objektive Coding-Benchmarks: keine alten Run-Episoden/Wiki-Seiten
    /// in die neue Aufgabe mischen. Verhindert Datenleck und stale Pfade.
    pub suppress_memory_context: bool,
}
