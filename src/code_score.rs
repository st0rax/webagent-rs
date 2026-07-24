//! code_score — objektiver Code-Kompetenz-Index je Brain, gespiegelt an
//! `brain_score.rs` (JSON-Lines-Log + Wilson-Lower-Bound), aber auf einem
//! **objektiven** Outcome statt auf Selbst-Report: der Compiler und die
//! Test-Suite sind der Schiedsrichter (siehe docs/BENCHMARK_PLAN.md).
//!
//! Ein Ereignis entsteht pro Brain-Benchmark-Versuch (ein gevoteter Sieger,
//! sequenziell von jedem Brain gebaut):
//! - `did_change` — hat das Brain überhaupt etwas am Working Tree geändert?
//! - `compiled` — baut das Ergebnis (`cargo build --lib`)?
//! - `tests_passed` — bleibt die Suite grün (`cargo test --lib`)?
//!
//! `pass = did_change && compiled && tests_passed`. Der Score ist der
//! Wilson-Lower-Bound der Pass-Quote — anders als brain_score OHNE rollierendes
//! Fenster: der Benchmark aggregiert bewusst über ALLE Ereignisse (§2 der Spec:
//! „Der Score aggregiert über alle Events"), weil hier jeder Datenpunkt ein
//! teurer, gleich gewichteter Messlauf ist, kein billiges Nutzungssignal.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::config::data_dir;

/// 95%-Konfidenz-Z-Wert für den Wilson-Score (identisch zu brain_score).
const Z: f64 = 1.96;

lazy_static! {
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

/// Ein objektiv gemessener Benchmark-Versuch (eine Zeile in `events.jsonl`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeEvent {
    pub brain_id: String,
    /// Kennung der gebauten Aufgabe (Hash des gevoteten Siegers) — gleicht
    /// Versuche derselben Aufgabe über Brains hinweg ab.
    pub task_id: String,
    pub did_change: bool,
    pub compiled: bool,
    pub tests_passed: bool,
    pub cycles: u32,
    /// Wie viele Repair-Iterationen nötig waren (1 = auf Anhieb grün).
    /// `default` haelt Events aus der Zeit vor dem Repair-Loop lesbar.
    #[serde(default)]
    pub iterations: u32,
    pub latency_ms: u64,
    /// Brain, das diese Aufgabe aufgegeben hat, falls sie weitergereicht wurde.
    ///
    /// Eine weitergereichte Aufgabe ist nachweislich schwerer: mindestens ein
    /// Brain ist daran hängengeblieben. Ohne dieses Feld sieht ein PASS nach
    /// Weitergabe in den Daten aus wie ein PASS im ersten Anlauf — der
    /// Unterschied ist aber genau das aussagekräftigste Signal, das der
    /// Benchmark erzeugt.
    #[serde(default)]
    pub handoff_from: Option<String>,
    /// `true`, wenn dieser Versuch mangels Fortschritt abgebrochen wurde
    /// (Stillstand), statt regulär an Build oder Tests zu scheitern.
    #[serde(default)]
    pub stalled: bool,
    /// `now_rfc3339()`-Zeitstempel.
    pub ts: String,
}

impl CodeEvent {
    /// Ein Versuch zählt nur dann als Erfolg, wenn das Brain etwas geändert hat,
    /// das Ergebnis baut UND die Tests grün bleiben — kein Selbst-Report zählt.
    pub fn passed(&self) -> bool {
        self.did_change && self.compiled && self.tests_passed
    }
}

/// Aggregierte Code-Statistik eines Brains über alle seine Ereignisse.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeStats {
    pub brain_id: String,
    pub attempts: usize,
    /// Anteil der Versuche mit einer Änderung am Working Tree.
    pub change_rate: f64,
    /// Anteil der Versuche, die (geändert und) gebaut haben.
    pub compile_rate: f64,
    /// Anteil der Versuche, die vollständig bestanden (`passed()`).
    pub pass_rate: f64,
    /// Wilson-Lower-Bound der Pass-Quote — der eigentliche Score (mehr Evidenz
    /// bei gleicher Quote ⇒ höherer, belastbarerer Wert).
    pub wilson_pass: f64,
    /// Versuche an Aufgaben, die ein anderes Brain vorher aufgegeben hat.
    pub hard_attempts: usize,
    /// Davon bestanden — „Rettungen".
    ///
    /// Das aussagekräftigste Einzelsignal des Benchmarks: die Aufgabe ist
    /// nachweislich schwer (jemand ist daran hängengeblieben), und das Brain
    /// startet trotzdem auf derselben frischen Baseline wie der Vorgänger. Ein
    /// Vorteil aus dessen Vorarbeit besteht nicht — der Tree wurde zurückgesetzt.
    pub rescues: usize,
    /// Wie oft dieses Brain selbst mangels Fortschritt aufgegeben hat.
    pub abandoned: usize,
}

fn events_path() -> PathBuf {
    data_dir().join("code_score").join("events.jsonl")
}

/// Ein Ereignis anhängen (append-only, volle Historie bleibt erhalten).
pub fn record(event: &CodeEvent) {
    record_at(event, &events_path());
}

fn record_at(event: &CodeEvent, path: &Path) {
    let _guard = WRITE_LOCK.lock();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

fn load_events(path: &Path) -> Vec<CodeEvent> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

/// Wilson-Score-Lower-Bound für `successes` von `n` Versuchen. `n == 0` liefert
/// 0.5 (völlige Unsicherheit) — ein Brain ohne Daten ist nicht „schlecht",
/// sondern unbekannt (gleiche Konvention wie brain_score).
fn wilson_lower_bound(successes: usize, n: usize) -> f64 {
    if n == 0 {
        return 0.5;
    }
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = Z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    ((center - margin) / denom).clamp(0.0, 1.0)
}

/// Reine Aggregation eines Ereignis-Slices zu einer nach `wilson_pass`
/// absteigend sortierten Rangliste (Tiebreak: brain_id alphabetisch, für
/// deterministische Reihenfolge). Kein I/O — direkt unit-getestet.
fn aggregate(events: &[CodeEvent]) -> Vec<CodeStats> {
    let mut per_brain: BTreeMap<String, Vec<&CodeEvent>> = BTreeMap::new();
    for e in events {
        per_brain.entry(e.brain_id.clone()).or_default().push(e);
    }
    let mut out: Vec<CodeStats> = per_brain
        .into_iter()
        .map(|(brain_id, evs)| {
            let attempts = evs.len();
            let changes = evs.iter().filter(|e| e.did_change).count();
            let compiles = evs.iter().filter(|e| e.compiled).count();
            let passes = evs.iter().filter(|e| e.passed()).count();
            let rate = |c: usize| {
                if attempts == 0 {
                    0.0
                } else {
                    c as f64 / attempts as f64
                }
            };
            let hard_attempts = evs.iter().filter(|e| e.handoff_from.is_some()).count();
            let rescues = evs
                .iter()
                .filter(|e| e.handoff_from.is_some() && e.passed())
                .count();
            let abandoned = evs.iter().filter(|e| e.stalled).count();
            CodeStats {
                brain_id,
                attempts,
                change_rate: rate(changes),
                compile_rate: rate(compiles),
                pass_rate: rate(passes),
                wilson_pass: wilson_lower_bound(passes, attempts),
                hard_attempts,
                rescues,
                abandoned,
            }
        })
        .collect();
    // Rangfolge weiterhin nach `wilson_pass`; Rettungen entscheiden nur bei
    // Gleichstand.
    //
    // Sie werden BEWUSST nicht in die Wilson-Zahl eingerechnet: deren Aussage
    // („untere Vertrauensgrenze der Erfolgsquote") gilt nur für ungewichtete
    // Erfolge aus Versuchen. Zählte eine Rettung doppelt, wäre das Ergebnis
    // keine Vertrauensgrenze mehr, sondern eine Punktzahl, die bloß so aussieht
    // wie eine — und die Rangliste würde ihre Vergleichbarkeit verlieren.
    // Schwierigkeit steht deshalb als eigene Spalte daneben.
    out.sort_by(|a, b| {
        b.wilson_pass
            .partial_cmp(&a.wilson_pass)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.rescues.cmp(&a.rescues))
            .then(a.abandoned.cmp(&b.abandoned))
            .then(a.brain_id.cmp(&b.brain_id))
    });
    out
}

/// Code-Rangliste über alle Brains mit mindestens einem Ereignis, absteigend
/// nach `wilson_pass`.
pub fn leaderboard() -> Vec<CodeStats> {
    leaderboard_at(&events_path())
}

fn leaderboard_at(path: &Path) -> Vec<CodeStats> {
    aggregate(&load_events(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_code_score_test_{nanos}_{n}.jsonl"))
    }

    fn ev(brain: &str, did_change: bool, compiled: bool, tests_passed: bool) -> CodeEvent {
        CodeEvent {
            brain_id: brain.to_string(),
            task_id: "task-abc".to_string(),
            did_change,
            compiled,
            tests_passed,
            cycles: 3,
            iterations: 1,
            latency_ms: 1234,
            handoff_from: None,
            stalled: false,
            ts: crate::now_rfc3339(),
        }
    }

    #[test]
    fn passed_requires_all_three() {
        assert!(ev("k", true, true, true).passed());
        assert!(!ev("k", false, true, true).passed());
        assert!(!ev("k", true, false, true).passed());
        assert!(!ev("k", true, true, false).passed());
    }

    #[test]
    fn wilson_no_data_is_uncertain_not_zero() {
        assert_eq!(wilson_lower_bound(0, 0), 0.5);
    }

    #[test]
    fn wilson_prefers_more_evidence_at_same_ratio() {
        let few = wilson_lower_bound(9, 10);
        let many = wilson_lower_bound(90, 100);
        assert!(many > few, "many={many} sollte > few={few} sein");
    }

    #[test]
    fn aggregate_computes_rates_and_pass() {
        // kimi: 2 Versuche, beide voll bestanden.
        // qwen: 2 Versuche, einer nur gebaut (Test rot), einer keine Änderung.
        let events = vec![
            ev("kimi", true, true, true),
            ev("kimi", true, true, true),
            ev("qwen", true, true, false),
            ev("qwen", false, false, false),
        ];
        let board = aggregate(&events);
        assert_eq!(board.len(), 2);
        // kimi vorne (höhere Pass-Quote → höherer Wilson).
        assert_eq!(board[0].brain_id, "kimi");
        assert_eq!(board[0].attempts, 2);
        assert_eq!(board[0].pass_rate, 1.0);
        assert_eq!(board[0].change_rate, 1.0);
        assert_eq!(board[0].compile_rate, 1.0);

        let qwen = &board[1];
        assert_eq!(qwen.attempts, 2);
        assert_eq!(qwen.change_rate, 0.5);
        assert_eq!(qwen.compile_rate, 0.5);
        assert_eq!(qwen.pass_rate, 0.0);
        assert!(board[0].wilson_pass > qwen.wilson_pass);
    }

    #[test]
    fn aggregate_empty_is_empty() {
        assert!(aggregate(&[]).is_empty());
    }

    #[test]
    fn record_and_leaderboard_roundtrip_through_jsonl() {
        let path = unique_path();
        for _ in 0..3 {
            record_at(&ev("kimi", true, true, true), &path);
        }
        for _ in 0..3 {
            record_at(&ev("qwen", true, false, false), &path);
        }
        let board = leaderboard_at(&path);
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].brain_id, "kimi");
        assert_eq!(board[0].pass_rate, 1.0);
        assert_eq!(board[1].brain_id, "qwen");
        assert_eq!(board[1].pass_rate, 0.0);
        assert!(board[0].wilson_pass > board[1].wilson_pass);
    }

    #[test]
    fn leaderboard_of_missing_file_is_empty() {
        let path = unique_path();
        assert!(leaderboard_at(&path).is_empty());
    }

    #[test]
    fn record_persists_parseable_json_line() {
        let path = unique_path();
        let event = ev("deepseek", true, true, false);
        record_at(&event, &path);
        let loaded = load_events(&path);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], event);
    }

    /// Wie `ev`, aber als weitergereichte Aufgabe (ein anderes Brain gab auf).
    fn ev_handoff(brain: &str, from: &str, passed: bool) -> CodeEvent {
        CodeEvent {
            handoff_from: Some(from.to_string()),
            ..ev(brain, true, true, passed)
        }
    }

    #[test]
    fn rescues_are_counted_separately_from_plain_passes() {
        let path = unique_path();
        record_at(&ev("deepseek", true, true, true), &path);
        record_at(&ev_handoff("deepseek", "zai", true), &path);
        record_at(&ev_handoff("deepseek", "zai", false), &path);
        let board = leaderboard_at(&path);
        let d = board.iter().find(|s| s.brain_id == "deepseek").unwrap();
        assert_eq!(d.attempts, 3);
        assert_eq!(d.hard_attempts, 2, "zwei weitergereichte Aufgaben");
        assert_eq!(d.rescues, 1, "davon eine geloest");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rescues_do_not_inflate_the_wilson_score() {
        // Bewusste Entscheidung: eine Rettung zaehlt in der Wilson-Zahl wie
        // jeder andere Erfolg. Wuerde sie doppelt zaehlen, waere das Ergebnis
        // keine Vertrauensgrenze mehr, sondern nur noch eine Punktzahl, die so
        // aussieht wie eine.
        let a = unique_path();
        record_at(&ev("x", true, true, true), &a);
        record_at(&ev("x", true, true, false), &a);
        let b = unique_path();
        record_at(&ev_handoff("y", "z", true), &b);
        record_at(&ev("y", true, true, false), &b);
        let sa = leaderboard_at(&a)
            .into_iter()
            .find(|s| s.brain_id == "x")
            .unwrap();
        let sb = leaderboard_at(&b)
            .into_iter()
            .find(|s| s.brain_id == "y")
            .unwrap();
        assert!((sa.wilson_pass - sb.wilson_pass).abs() < 1e-9);
        assert_eq!(sa.rescues, 0);
        assert_eq!(sb.rescues, 1);
        let _ = std::fs::remove_file(&a);
        let _ = std::fs::remove_file(&b);
    }

    #[test]
    fn rescues_break_ties_in_the_ranking() {
        // Gleiche Quote, gleiche Evidenz: dann entscheidet, wer an einer
        // nachweislich schweren Aufgabe bestanden hat.
        let path = unique_path();
        record_at(&ev("aaa", true, true, true), &path);
        record_at(&ev("aaa", true, true, false), &path);
        record_at(&ev_handoff("bbb", "aaa", true), &path);
        record_at(&ev("bbb", true, true, false), &path);
        let board = leaderboard_at(&path);
        assert_eq!(
            board[0].brain_id, "bbb",
            "Retter steht bei Gleichstand vorn"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn abandoned_counts_stalled_attempts() {
        let path = unique_path();
        record_at(
            &CodeEvent {
                stalled: true,
                ..ev("lahm", true, false, false)
            },
            &path,
        );
        record_at(&ev("lahm", true, false, false), &path);
        let s = leaderboard_at(&path)
            .into_iter()
            .find(|s| s.brain_id == "lahm")
            .unwrap();
        assert_eq!(s.abandoned, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn old_events_without_the_new_fields_still_load() {
        // Bestandsdaten stammen aus der Zeit vor der Weitergabe — sie duerfen
        // die Rangliste nicht sprengen.
        let path = unique_path();
        std::fs::write(
            &path,
            "{\"brain_id\":\"alt\",\"task_id\":\"t\",\"did_change\":true,\"compiled\":true,             \"tests_passed\":true,\"cycles\":2,\"latency_ms\":10,\"ts\":\"2026-07-21T00:00:00Z\"}
",
        )
        .unwrap();
        let s = leaderboard_at(&path)
            .into_iter()
            .find(|s| s.brain_id == "alt")
            .unwrap();
        assert_eq!(s.attempts, 1);
        assert_eq!(s.hard_attempts, 0);
        assert_eq!(s.rescues, 0);
        assert!(!s.wilson_pass.is_nan());
        let _ = std::fs::remove_file(&path);
    }
}
