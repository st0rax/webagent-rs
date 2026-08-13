//! Shared-Typen des Benchmarks: Konfiguration, Erntekandidat und Endergebnis.
//!
//! Die Bewertung lebt in `bench_scoring`, die Erntepruefung in `bench_harvest`;
//! hier liegen nur die Datentypen, die alle Phasen verbinden.

use crate::code_score::CodeStats;
use std::path::PathBuf;

/// Konfiguration eines Benchmark-Laufs.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Zu bewertende Brains (leer ⇒ vom Aufrufer mit allen registrierten füllen).
    pub brains: Vec<String>,
    /// Wie oft der ganze Zyklus (Abstimmen → bauen) wiederholt wird.
    pub rounds: usize,
    /// Vorschläge je Brain in der Sammelphase (Phase A).
    pub suggestions: usize,
    /// Eval-Kommando „baut es?" (Default `cargo build --lib`).
    pub build_eval: String,
    /// Eval-Kommando „Tests grün?" (Default `cargo test --lib`).
    pub test_eval: String,
    /// Git-Repo-Root, in dem gebaut/gemessen wird.
    pub workdir: PathBuf,
    /// Headless-Browser für die Brain-Runs.
    pub headless: bool,
    /// Maximale Repair-Iterationen je Brain: schlägt Build/Test fehl, geht die
    /// Fehlerausgabe als Kontext zurück ans Brain (1 = kein Repair-Loop).
    pub max_iterations: u32,
    /// Ernte-Modus: der Code des besten bestandenen Brains wird nach der Runde
    /// wieder eingespielt und committet, statt verworfen zu werden.
    ///
    /// Ohne das ist der Benchmark ein reines Messgerät — deepseeks bestandene
    /// Läufe (2/4 PASS, 2026-07-21) landeten vollständig im `git reset --hard`.
    /// Die Messung bleibt unberührt: jedes Brain startet weiterhin auf
    /// identischer Baseline, geerntet wird erst NACH dem letzten Brain.
    pub harvest: bool,
    /// Ausklappen: die einzelnen Schritte (Shell-Kommandos, Datei-Aktionen,
    /// Brain-Antworten) zusaetzlich als eigene Zeilen ausgeben. Ohne das steckt
    /// nur der jeweils AKTUELLE Schritt in der mitlaufenden Timer-Zeile.
    pub verbose: bool,
    /// Wie viele Brains in den Lesephasen (Sammeln, Abstimmen) gleichzeitig
    /// befragt werden. Bauen bleibt sequenziell — die Brains teilen sich EINEN
    /// Git-Worktree, nebenlaeufige Edits wuerden einander ueberschreiben.
    pub parallel: usize,
    /// Nach wie vielen Iterationen OHNE Fortschritt ein Brain aufgibt und die
    /// Aufgabe weitergereicht wird. `max_iterations` ist nur noch die harte
    /// Obergrenze — wer vorankommt, darf sie ausschoepfen, wer sich im Kreis
    /// dreht, wird frueher gestoppt.
    pub stall_limit: u32,
    /// Wie oft eine Aufgabe hoechstens an ein weiteres Brain weitergereicht wird.
    pub max_handoffs: usize,
    /// Lint-Kommando fuer das Ernte-Tor (leer = kein Lint-Gate).
    ///
    /// Build und Tests sagen "es laeuft", nicht "es ist sauber". geminis
    /// geernteter Beitrag (2026-07-21) kompilierte und war gruen, brachte aber
    /// eine doppelte `use super::*;` mit — die Ernte hatte dafuer kein Auge.
    pub lint_eval: String,
    /// Menschlich gesperrte Begriffe. Treffer werden weder Gewinner noch
    /// Bauauftrag; ein Veto ist keine negative Brain-Wertung.
    pub vetoes: Vec<String>,
    /// Endlos-Schleife: nach der letzten Runde sofort wieder von vorne.
    pub loop_forever: bool,
}

/// Ein bestandener Brain-Lauf, dessen Diff für die spätere Ernte aufbewahrt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestCandidate {
    /// Brain, das diesen Code gebaut hat.
    pub brain: String,
    /// Die Aufgabe, die dieses Brain zugeteilt bekam (fuer die Commit-Message).
    pub task: String,
    /// Der komplette Diff gegen die Baseline (`git diff --cached`).
    pub patch: String,
    /// Benötigte Repair-Iterationen (weniger = souveräner gelöst).
    pub iterations: u32,
    /// Gesamtdauer des Brain-Laufs.
    pub latency_ms: u64,
}


/// Endergebnis eines Benchmark-Laufs.
#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    /// Je Runde der gevotete Sieger `(runde, text)`.
    pub winners: Vec<(usize, String)>,
    /// Code-Rangliste über alle bislang gespeicherten Events.
    pub leaderboard: Vec<CodeStats>,
    /// Slug der abgelegten Wiki-Seite, falls geschrieben.
    pub wiki_slug: Option<String>,
    /// Tatsächlich geerntete Beiträge `(brain, aufgabe)` — das ist der Teil,
    /// der als Code im Repo bleibt statt nur als Messpunkt.
    pub harvested: Vec<(String, String)>,
}
