//! bench_scoring — wie weit eine Iteration kam und wie ein Run-Ausgang zu
//! werten ist.
//!
//! Erster Schnitt des Modul-Splits, den der Brain-Schwarm mit der Note 4
//! gefordert hat (`struktur-evaluation-2026-07`: „flaches src/ skaliert
//! nicht"). Seither hatte sich die Lage VERSCHLECHTERT: aus fuenf Dateien ueber
//! 1300 Zeilen wurden neun, und `benchmark.rs` allein wuchs auf ueber 3000.
//!
//! Herausgeschnitten ist die Bewertung: zaehlen, was Build und Testlauf ergeben
//! haben, und einordnen, was ein Run-Status bedeutet. Reine Funktionen ohne
//! I/O — genau deshalb liessen sie sich als erstes gefahrlos loesen. Ein Split
//! beginnt nicht an der groessten Stelle, sondern an der am klarsten
//! abgegrenzten.
//!
//! `benchmark.rs` re-exportiert sie, damit kein Aufrufer und kein Test
//! angefasst werden muss. Ein Umbau, der gleichzeitig verschiebt UND Aufrufer
//! aendert, ist nicht mehr nachvollziehbar, wenn er schiefgeht — und bei 3000
//! Zeilen will man genau das nicht riskieren.

use std::path::Path;

// Bleibt vorerst in `benchmark`: parst die Zusammenfassungszeile eines
// Testlaufs und gehoert thematisch hierher, wird dort aber noch von mehreren
// Stellen benutzt. Ein Schnitt, der mehrere Bewegungen mischt, ist nicht mehr
// nachvollziehbar — sie folgt im naechsten Schritt.
use crate::benchmark::parse_test_count;
/// Wie weit eine Iteration gekommen ist — die Grundlage dafür, ob sich
/// Weitermachen lohnt.
///
/// `stage` ist die grobe Stufe, `errors` die Feinauflösung darin (Compilerfehler
/// bzw. rote Tests). Zwölf Fehler auf elf zu drücken ist Fortschritt, auch wenn
/// die Stufe gleich bleibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// 0 = nichts geändert, 1 = Build rot, 2 = Tests rot, 3 = grün.
    pub stage: u8,
    /// Verbleibende Fehler auf dieser Stufe (kleiner ist besser).
    pub errors: u32,
}

/// Zählt Compilerfehler in einer `cargo build`-Ausgabe.
pub fn count_build_errors(output: &str) -> u32 {
    output
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            // `error[E0308]:` und `error:` — aber nicht die Schlusszeile
            // "error: could not compile …", die nur den Sammelabbruch meldet.
            (t.starts_with("error[") || t.starts_with("error:")) && !t.contains("could not compile")
        })
        .count() as u32
}

/// Zählt fehlgeschlagene Tests aus einer `cargo test`-Ausgabe.
pub fn count_failed_tests(output: &str) -> u32 {
    let mut total = 0u32;
    for part in output.split_whitespace().collect::<Vec<_>>().windows(2) {
        if part[1].starts_with("failed") {
            if let Ok(n) = part[0].parse::<u32>() {
                total += n;
            }
        }
    }
    total
}

/// Bewertet den Stand NACH dem Testlauf.
///
/// Der Fallstrick, den ein echter Lauf offengelegt hat (deepseek, 2026-07-21):
/// `cargo build --lib` war grün, `cargo test --lib` meldete zehnmal in Folge
/// „0 bestanden" — die Test-Binary ließ sich gar nicht erst übersetzen. Naiv
/// gezählt sind das *null* fehlgeschlagene Tests, also scheinbar der beste
/// denkbare Stand auf Stufe 2; ein Brain, das später wirklich Tests laufen
/// lässt und drei davon rot sieht, hätte damit „schlechter" abgeschnitten.
///
/// Deshalb: laufen gar keine Tests, ist das kein Stufe-2-Ergebnis. Der
/// Test-Build ist strenger als `build --lib` (er übersetzt auch die
/// `#[cfg(test)]`-Module), also bleibt es Stufe 1.
pub fn progress_after_tests(output: &str) -> Progress {
    let ran = parse_test_count(output).is_some() || count_failed_tests(output) > 0;
    if ran {
        Progress {
            stage: 2,
            errors: count_failed_tests(output),
        }
    } else {
        Progress {
            stage: 1,
            errors: count_build_errors(output),
        }
    }
}

/// `true`, wenn `now` näher an grün ist als das bisher Beste.
///
/// Ohne dieses Maß entschied allein das Schleifenlimit über den Abbruch: ein
/// Brain, das sich Iteration für Iteration von zwölf Fehlern auf zwei
/// herunterarbeitet, wurde genauso hart gestoppt wie eines, das zehnmal
/// dieselbe kaputte Zeile schreibt (Beobachtung Storax, deepseek lief
/// regelmäßig ins Limit statt zu scheitern).
pub fn is_improvement(best: Option<Progress>, now: Progress) -> bool {
    match best {
        None => now.stage > 0,
        Some(b) => now.stage > b.stage || (now.stage == b.stage && now.errors < b.errors),
    }
}
/// War eine harvest-lose Runde eine Verfuegbarkeitsstoerung statt einer
/// Sackgasse im Code?
///
/// Genau dann, wenn kein einziges Brain zum Messen kam: dann gibt es weder
/// bestandene noch gescheiterte Gates, die Runde sagt ueber den Code nichts aus
/// und darf das Abbruchbudget nicht verbrauchen. Kam mindestens ein Brain dran,
/// ist die Runde eine echte Aussage — auch wenn alle anderen gesperrt waren.
pub fn is_availability_outage(attempted: usize, failures: &[String]) -> bool {
    attempted == 0 && failures.is_empty()
}

/// `true`, wenn der Run-Status eine EXTERNE Blockade meldet (Anbieter-Limit,
/// Login, Cloudflare, Oberflaeche ohne Antwort) statt eines Fehlversuchs.
///
/// Solche Laeufe duerfen nicht in den Score: sonst faellt die Bewertung eines
/// Brains mit der Auslastung seines Anbieters statt mit seiner Faehigkeit.
pub fn is_external_block(status: &str) -> bool {
    let low = status.to_lowercase();
    [
        "brain_unavailable",
        "blocked",
        "login_required",
        "loginrequired",
        "cloudflare",
        "rate_limit",
    ]
    .iter()
    .any(|p| low.contains(p))
}

/// Ein Protokollfehler ist weder ein Provider-Limit noch ein sinnvoller
/// Reparaturfall: derselbe Browser-Run hat die Antwort bereits mehrfach
/// zurückgewiesen. Für diese Aufgabe wird das Brain sofort als Fehlversuch
/// gewertet; der nächste geplante Kandidat bekommt eine frische Chance.
pub fn is_protocol_fault(status: &str) -> bool {
    let low = status.to_lowercase();
    low.contains("protocol_error") || low.contains("protocol_invalid")
}

/// Diese Status werden nicht im selben Brain erneut versucht: der Controller
/// hat bereits terminal abgebrochen. Sie bleiben im Score sichtbar, aber der
/// nächste geplante Kandidat übernimmt auf frischer Basis.
pub fn is_nonretryable_run_fault(status: &str) -> bool {
    let low = status.to_lowercase();
    is_protocol_fault(&low)
        || low.contains("wall_timeout")
        || low.contains("false_done")
        || low.contains("max_cycles")
}

/// `true`, wenn ein Versuch objektiv besteht (geändert UND gebaut UND grün).
pub fn is_pass(did_change: bool, compiled: bool, tests_passed: bool) -> bool {
    did_change && compiled && tests_passed
}

/// Menschenlesbare Outcome-Klassifikation für die Live-Ausgabe.
pub fn outcome_label(did_change: bool, compiled: bool, tests_passed: bool) -> &'static str {
    if !did_change {
        "SKIP (keine Änderung)"
    } else if !compiled {
        "FAIL (build)"
    } else if !tests_passed {
        "FAIL (test)"
    } else {
        "PASS"
    }
}

