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
use crate::scoring::wilson_lower_bound;

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
    /// Wie viele Brains in der Runde dieses Versuchs überhaupt im Feld standen
    /// (also erreichbar und nicht gesperrt waren).
    ///
    /// Die Aussagekraft eines Messpunkts hängt daran: Aufgabe und Plan
    /// entstehen aus der Abstimmung des Feldes, und ein Ergebnis aus einem Feld
    /// von zwei ist schwächer belegt als eines aus einem Feld von acht. Ohne
    /// dieses Feld lässt sich das im Nachhinein nicht mehr rekonstruieren —
    /// gesperrte Brains hinterlassen bewusst keinen Messpunkt.
    ///
    /// `0` bedeutet „unbekannt" und steht für Altdaten aus der Zeit vor dieser
    /// Erfassung; solche Events zählen bei der Gewichtung neutral.
    #[serde(default)]
    pub field_size: usize,
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
    /// Durchschnittliche Feldgrösse der Versuche mit bekannter Feldgrösse.
    /// `0.0`, wenn zu keinem Versuch eine Feldgrösse erfasst ist.
    pub avg_field: f64,
    /// Relative Aussagekraft der Messreihe, 0..1.
    ///
    /// Zwei Faktoren, beide relativ zum bestmöglichen Fall: wie viel Evidenz
    /// vorliegt (`wilson_pass` trägt das bereits in sich) und in wie starkem
    /// Feld sie entstanden ist. Hier steht nur der zweite Teil — bewusst
    /// getrennt vom Score, damit die Zahl nicht heimlich die Bewertung
    /// verschiebt, sondern danebensteht und sagt, wie belastbar sie ist.
    ///
    /// Bezugsgrösse ist das grösste Feld, das in der Messreihe je zustande kam:
    /// so ist die Aussage relativ („diese Reihe entstand in halb so starkem
    /// Feld wie möglich") statt an einer festen Brain-Zahl zu hängen, die sich
    /// mit jedem neuen Brain ändern würde.
    pub significance: f64,
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

    // Storax-Vorgabe (2026-08-01): die Benchmark-Versuche aus
    // code_score/events.jsonl spiegeln in den TUI-Baum — Pass/Change/Compile
    // sind genau die Kennzahlen, die dort aufklappbar interessant sind. Nur im
    // Spiegelmodus (`--verbose`-Benchmark), damit Tests und Normalbetrieb den
    // Bus nicht mit Messdaten füllen.
    if crate::bench_events::echo_bus_enabled() {
        let level = if event.passed() {
            crate::bench_events::Level::Pass
        } else {
            crate::bench_events::Level::Fail
        };
        crate::bench_events::emit_detailed(
            level,
            Some(&event.brain_id),
            &format!(
                "[code:{}] {} change={} compiled={} tests={} {}ms",
                event.brain_id,
                crate::char_prefix(&event.task_id, 20),
                event.did_change,
                event.compiled,
                event.tests_passed,
                event.latency_ms
            ),
            Some(&serde_json::to_string(event).unwrap_or_default()),
        );
    }
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
            // Nur Versuche mit erfasster Feldgrösse; Altdaten (0) zählen neutral,
            // statt den Schnitt nach unten zu ziehen.
            let felder: Vec<usize> = evs
                .iter()
                .map(|e| e.field_size)
                .filter(|&f| f > 0)
                .collect();
            let avg_field = if felder.is_empty() {
                0.0
            } else {
                felder.iter().sum::<usize>() as f64 / felder.len() as f64
            };
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
                avg_field,
                // Bezugsgrösse wird erst unten gesetzt, wenn alle Brains
                // ausgewertet sind — sie ist relativ zum stärksten je
                // erreichten Feld, nicht zu einer festen Zahl.
                significance: 0.0,
            }
        })
        .collect();

    // Relative Aussagekraft: das grösste je zustande gekommene Feld ist 1,0.
    let max_field = out
        .iter()
        .map(|s| s.avg_field)
        .fold(0.0_f64, |a, b| if b > a { b } else { a });
    if max_field > 0.0 {
        for s in &mut out {
            s.significance = s.avg_field / max_field;
        }
    }
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
            // Standard 0 = „nicht erfasst", wie bei Altdaten. Tests, die die
            // Feldgroesse brauchen, setzen sie ausdruecklich.
            field_size: 0,
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
    fn aussagekraft_ist_relativ_zum_staerksten_feld() {
        // Storax' Vorgabe: gesperrte Brains werden ausgeklammert, und die
        // dadurch geringere Aussagekraft muss RELATIV erfasst werden.
        let path = unique_path();
        // deepseek misst im vollen Feld von acht ...
        for _ in 0..2 {
            record_at(
                &CodeEvent {
                    field_size: 8,
                    ..ev("deepseek", true, true, true)
                },
                &path,
            );
        }
        // ... zai nur im Notbetrieb mit zwei erreichbaren Brains.
        for _ in 0..2 {
            record_at(
                &CodeEvent {
                    field_size: 2,
                    ..ev("zai", true, true, true)
                },
                &path,
            );
        }
        let board = leaderboard_at(&path);
        let d = board.iter().find(|s| s.brain_id == "deepseek").unwrap();
        let z = board.iter().find(|s| s.brain_id == "zai").unwrap();

        assert_eq!(d.avg_field, 8.0);
        assert_eq!(z.avg_field, 2.0);
        // Das staerkste je erreichte Feld ist die Bezugsgroesse, nicht eine
        // feste Brain-Zahl: sonst verschoebe jedes neue Brain alle Altwerte.
        assert!((d.significance - 1.0).abs() < 1e-9, "{}", d.significance);
        assert!((z.significance - 0.25).abs() < 1e-9, "{}", z.significance);

        // Der Score selbst bleibt unberuehrt — die Aussagekraft steht daneben
        // und verschiebt die Bewertung nicht heimlich.
        assert_eq!(d.wilson_pass, z.wilson_pass);
    }

    #[test]
    fn altdaten_ohne_feldgroesse_zaehlen_neutral() {
        // Events aus der Zeit vor der Erfassung haben field_size 0. Sie duerfen
        // den Schnitt nicht nach unten ziehen, sonst saehe jede lange Historie
        // kuenstlich unbelastbar aus.
        let path = unique_path();
        record_at(&ev("kimi", true, true, true), &path); // field_size: 0
        record_at(
            &CodeEvent {
                field_size: 6,
                ..ev("kimi", true, true, true)
            },
            &path,
        );
        let board = leaderboard_at(&path);
        let k = board.iter().find(|s| s.brain_id == "kimi").unwrap();
        assert_eq!(k.avg_field, 6.0, "nur erfasste Felder zaehlen");
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

    #[test]
    fn record_spiegelt_den_bus_nur_im_spiegelmodus() {
        // Negativ-Pruefung: ohne Spiegelmodus legt record nichts in den Bus.
        // Positiv: im Spiegelmodus erscheint der Versuch als aufklappbarer
        // `[code:…]`-Knoten mit dem vollen JSON als Detail.
        let _guard = crate::bench_events::test_bus_mutex().lock();

        crate::bench_events::set_echo_bus(false);
        crate::bench_events::clear();
        let path = unique_path();
        record_at(&ev("kimi", true, true, true), &path);
        assert!(
            !crate::bench_events::snapshot()
                .iter()
                .any(|e| e.text.starts_with("[code:kimi]")),
            "ohne Spiegelmodus duerfen keine Code-Events in den Bus"
        );

        crate::bench_events::set_echo_bus(true);
        crate::bench_events::clear();
        record_at(&ev("kimi", false, true, false), &path);
        let events = crate::bench_events::snapshot();
        assert!(
            events.iter().any(|e| {
                e.brain.as_deref() == Some("kimi")
                    && e.text.starts_with("[code:kimi]")
                    && e.detail.as_deref().is_some_and(|d| d.contains("did_change"))
            }),
            "im Spiegelmodus muss der Code-Versuch mit vollem Detail in den Bus"
        );

        crate::bench_events::set_echo_bus(false);
        let _ = std::fs::remove_file(&path);
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
