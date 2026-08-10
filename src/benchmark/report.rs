//! Ausgabe-Formatierung des Benchmarks: Markdown-Body fuer die Wiki-Ablage,
//! Konsolen-Rangliste und das maschinenlesbare Ergebnisformat.

use crate::code_score::CodeStats;

use super::bench_say;

/// Markdown-Body für die Wiki-Ablage (`code-benchmark-<stamp>`): welche Sieger je
/// Runde gebaut werden sollten + die aktuelle Code-Rangliste.
pub fn format_benchmark_report(winners: &[(usize, String)], board: &[CodeStats]) -> String {
    let mut out = String::from(
        "Vote-driven Code-Benchmark: pro Runde stimmt der Schwarm über den nächsten \
         Verbesserungsschritt ab; jedes Brain baut den Sieger sequenziell, gemessen \
         wird objektiv (Compiler + Tests, kein Selbst-Report).\n\n",
    );
    out.push_str("## Gevotete Sieger je Runde\n");
    if winners.is_empty() {
        out.push_str("(keine — keine Stimmen gesammelt)\n");
    } else {
        for (round, winner) in winners {
            out.push_str(&format!("{round}. {winner}\n"));
        }
    }
    out.push_str("\n## Code-Rangliste\n");
    out.push_str("| brain | attempts | change% | compile% | pass% | wilson_pass |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    if board.is_empty() {
        out.push_str("| (keine Daten) | 0 | – | – | – | – |\n");
    } else {
        for s in board {
            out.push_str(&format!(
                "| {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.3} |\n",
                s.brain_id,
                s.attempts,
                s.change_rate * 100.0,
                s.compile_rate * 100.0,
                s.pass_rate * 100.0,
                s.wilson_pass
            ));
        }
    }
    out
}

/// Druckt die Code-Rangliste auf stdout und Ereignisstrom (Live-Ausgabe am Ende, Spec §4).
pub(crate) fn print_leaderboard(board: &[CodeStats]) {
    bench_say!(crate::bench_events::Level::Info, None, "Code-Rangliste:");
    bench_say!(
        crate::bench_events::Level::Info,
        None,
        "  brain            attempts  change%  compile%  pass%   wilson_pass  schwer  rettung  aufgegeben  feld  aussagekraft"
    );
    for s in board {
        bench_say!(
            crate::bench_events::Level::Info,
            Some(&s.brain_id),
            "  {:<15}  {:>8}  {:>6.0}%  {:>7.0}%  {:>5.0}%   {:>11.3}  {:>6}  {:>7}  {:>10}  {:>4.1}  {:>11.0}%",
            s.brain_id,
            s.attempts,
            s.change_rate * 100.0,
            s.compile_rate * 100.0,
            s.pass_rate * 100.0,
            s.wilson_pass,
            s.hard_attempts,
            s.rescues,
            s.abandoned,
            s.avg_field,
            s.significance * 100.0
        );
    }
    let rescues: usize = board.iter().map(|s| s.rescues).sum();
    if rescues > 0 {
        bench_say!(
            crate::bench_events::Level::Info,
            None,
            "  ({rescues} Rettung(en): bestanden an Aufgaben, die ein anderes Brain aufgegeben hatte)"
        );
    }
}

/// Erzeugt ein reproduzierbares, maschinenlesbares Ergebnisformat fuer Benchmark-Szenarien.
pub fn format_benchmark_result(name: &str, value: u64, unit: &str) -> String {
    format!("{}={}{}", name, value, unit)
}

