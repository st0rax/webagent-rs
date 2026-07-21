//! benchmark — vote-driven, objektiver Code-Kompetenz-Benchmark.
//!
//! Anders als eine fixe Aufgabe misst der Benchmark den vollen
//! Selbst-Verbesserungs-Loop (siehe docs/BENCHMARK_PLAN.md):
//!
//! - **Phase A (Sammeln + Abstimmen):** `self_research::run_self_research` liefert
//!   eine Rangliste; der Platz-1-Vorschlag ([`winner_from_report`]) wird zur
//!   Benchmark-Aufgabe ([`build_task_prompt`]).
//! - **Phase B (Implementieren + Messen, pro Brain sequenziell):** sauberen
//!   Git-Tree prüfen, Baseline-SHA merken, das Brain über den Controller (mit
//!   Wall-Timeout + kleinem `max_cycles`) den Sieger bauen lassen, dann objektiv
//!   evaluieren (`did_change` → `cargo build --lib` → `cargo test --lib`), das
//!   [`CodeEvent`](crate::code_score::CodeEvent) speichern und den Tree hart auf
//!   die Baseline zurücksetzen (`git reset --hard` + `git clean -fd`). Jedes
//!   Brain startet identisch; der Benchmark hinterlässt KEINE Änderung.
//!
//! `--rounds N` wiederholt Phase A+B N-mal (N Abstimmungen → N Sieger). Der Score
//! aggregiert über alle Events (`code_score::leaderboard`).
//!
//! Die reinen Helfer ([`build_task_prompt`], [`task_id`], [`winner_from_report`],
//! [`outcome_label`], [`is_pass`], [`format_benchmark_report`]) sind unit-getestet;
//! der Live-Teil (echtes Brain + `cargo` + Git) wird vom Orchestrator end-to-end
//! geprüft, nicht im Unit-Test.

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::code_score::{CodeEvent, CodeStats};
use crate::self_research::SelfResearchReport;

/// Wall-Timeout je Brain-Run in Sekunden (ein Benchmark-Run darf nicht ewig
/// laufen — via `AgentController::set_wall_timeout_secs`).
const BENCH_WALL_SECS: u64 = 300;
/// Controller-Zyklen je Brain-Run: klein, damit das Brain fokussiert am Sieger
/// baut statt an einer offenen Aufgabe.
const BENCH_MAX_CYCLES: usize = 15;
/// Timeout je Eval-Kommando (`cargo build`/`cargo test`) in Sekunden.
const EVAL_TIMEOUT_SECS: u64 = 300;
/// Größe der gerankten Top-Liste in Phase A (nur Platz 1 wird zur Aufgabe).
const VOTE_TOP_K: usize = 10;
/// Zeichen-Cap der Projektfakten im Sammel-Prompt (wie autoresearch-self).
const FACTS_MAX_CHARS: usize = 1200;

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
}

/// Stabile, kurze Task-Kennung aus dem Sieger-Text (Hash) — gleiche Aufgabe ⇒
/// gleiche `task_id` über Brains und Läufe hinweg.
pub fn task_id(winner: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    winner.trim().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Baut den Aufgaben-Prompt aus dem gevoteten Sieger (Spec §2, Phase A).
pub fn build_task_prompt(winner: &str) -> String {
    format!(
        "Implementiere folgenden Verbesserungsvorschlag im Rust-Projekt webagent-rs \
         (aktuelles Verzeichnis) mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Ergänze \
         Tests. `cargo test --lib` muss grün bleiben. Mache genau diese eine \
         Änderung, nichts darüber hinaus.\n\nVorschlag: {winner}",
        winner = winner.trim()
    )
}

/// Prompt, der einen VAGEN Abstimmungssieger in eine KONKRETE, bounded
/// Coding-Aufgabe übersetzt (Phase A.5).
///
/// Ohne diesen Schritt bekamen alle Brains den rohen Architekturwunsch
/// ("Sicherheitshärtung: Sandbox/Allowlist/Secret-Handling…") und explorierten
/// 6–11 Zyklen lang, ohne eine Zeile zu ändern — 22 von 22 Versuchen endeten mit
/// `did_change=false` (Messung 2026-07-21). Mit einer konkreten Vorgabe
/// (exakte Signatur + Tests) lieferten dieselben Brains auf Anhieb.
/// Der Schritt macht den Benchmark ausserdem FAIR: alle Brains bekommen exakt
/// dieselbe präzise Aufgabe, gemessen wird Umsetzung statt Interpretation.
pub fn build_refine_prompt(winner: &str, facts: &str) -> String {
    format!(
        "Du planst eine Coding-Aufgabe fuer das Rust-Projekt webagent-rs.\n\n\
         Projektfakten:\n{facts}\n\n\
         Zu konkretisierender Verbesserungsvorschlag:\n{winner}\n\n\
         Uebersetze ihn in EINE kleine, in sich geschlossene Aufgabe, die ein \
         Agent in wenigen Schritten umsetzen kann. Anforderungen an deine Antwort:\n\
         - genau EINE Zieldatei unter src/ benennen (existierende Datei bevorzugt)\n\
         - genau EINE neue oeffentliche Funktion mit EXAKTER Rust-Signatur angeben\n\
         - das erwartete Verhalten in 2-4 Saetzen praezise beschreiben\n\
         - mindestens 4 konkrete Testfaelle auflisten\n\
         - nur std und bereits vorhandene Dependencies verwenden\n\
         - KEINE Architektur-Umbauten, keine neuen Module, kein Refactoring\n\n\
         Antworte AUSSCHLIESSLICH mit der Aufgabenbeschreibung als Fliesstext \
         (kein JSON, keine Einleitung, kein Nachwort).",
        facts = crate::char_prefix(facts, 900),
        winner = winner.trim()
    )
}

/// Nimmt die verfeinerte Aufgabe, wenn sie brauchbar aussieht, sonst `None`.
/// Zu kurze oder leere Antworten fallen auf den Rohsieger zurück.
pub fn usable_refinement(text: &str) -> Option<String> {
    let t = text.trim();
    if t.chars().count() < 80 {
        return None;
    }
    Some(t.to_string())
}

/// Extrahiert den vorgeschlagenen Funktionsnamen aus einer verfeinerten Aufgabe
/// (erstes `pub fn NAME` bzw. `fn NAME`), um Neuheit prüfen zu können.
pub fn proposed_fn_name(refined: &str) -> Option<String> {
    for marker in ["pub fn ", "fn "] {
        if let Some(idx) = refined.find(marker) {
            let rest = &refined[idx + marker.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.len() >= 3 {
                return Some(name);
            }
        }
    }
    None
}

/// `true`, wenn die Aufgabe etwas verlangt, das es SCHON GIBT — dann ist die
/// Runde wertlos: das Brain meldet korrekt "ist bereits implementiert", ändert
/// nichts und würde faelschlich als Fehlschlag gewertet (Storax-Beobachtung
/// 2026-07-21: "einer der Kandidaten sagt immer wieder, alles sei schon sauber
/// implementiert").
pub fn task_is_redundant(refined: &str, existing_api: &[String]) -> bool {
    match proposed_fn_name(refined) {
        Some(name) => existing_api.iter().any(|e| e == &name),
        None => false,
    }
}

/// Platz-1-Vorschlag eines Self-Research-Reports (die Benchmark-Aufgabe), oder
/// `None`, wenn niemand abgestimmt hat.
pub fn winner_from_report(report: &SelfResearchReport) -> Option<String> {
    report
        .ranked
        .first()
        .map(|r| r.text.trim().to_string())
        .filter(|t| !t.is_empty())
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

/// Druckt die Code-Rangliste auf stdout (Live-Ausgabe am Ende, Spec §4).
fn print_leaderboard(board: &[CodeStats]) {
    println!("[benchmark] Code-Rangliste:");
    println!("  brain            attempts  change%  compile%  pass%   wilson_pass");
    for s in board {
        println!(
            "  {:<15}  {:>8}  {:>6.0}%  {:>7.0}%  {:>5.0}%   {:>0.3}",
            s.brain_id,
            s.attempts,
            s.change_rate * 100.0,
            s.compile_rate * 100.0,
            s.pass_rate * 100.0,
            s.wilson_pass
        );
    }
}

/// `true`, wenn der Working Tree Änderungen enthält (inkl. neu angelegter,
/// untracked Dateien — `git diff --quiet` allein übersähe die von write-Actions
/// erzeugten neuen Dateien; deshalb `git status --porcelain`).
fn tree_changed(workdir: &Path) -> bool {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workdir)
        .output()
    {
        Ok(out) => !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        Err(_) => false,
    }
}

/// Führt ein Eval-Kommando aus und wertet nur den Exit-Code (0 = ok) — nutzt den
/// Kommando-Runner mit Timeout aus autoresearch.
fn run_eval_ok(cmd: &str, workdir: &Path) -> bool {
    matches!(
        crate::autoresearch::run_eval_with_timeout(cmd, workdir, EVAL_TIMEOUT_SECS),
        Ok((Some(0), _))
    )
}

/// Setzt den Working Tree hart auf `baseline` zurück und entfernt untracked
/// Dateien — jeder Brain-Run startet identisch, der Benchmark hinterlässt nichts.
fn reset_repo(workdir: &Path, baseline: &str) -> Result<(), String> {
    // `git add -A` davor, damit auch neu angelegte (untracked) Dateien vom
    // Full-Reset erfasst werden; `git clean -fd` fegt den Rest weg.
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workdir)
        .output();
    crate::autoresearch::git_reset_hard(workdir, baseline)?;
    let _ = std::process::Command::new("git")
        .args(["clean", "-fd"])
        .current_dir(workdir)
        .output();
    Ok(())
}

/// Ein Brain baut die Aufgabe über den normalen Controller-Pfad (mit Wall-Timeout
/// und kleinem `max_cycles`). Liefert `(status, cycles)`.
#[cfg(feature = "webview")]
fn bench_run(
    brain_id: &str,
    task: &str,
    headless: bool,
) -> Result<(String, u32), String> {
    use crate::browser::WebBrainBackend;
    use crate::controller::AgentController;
    use crate::executor::PlatformShellExecutor;

    let backend = WebBrainBackend::from_config(brain_id)?;
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::with_data_dir(
        backend,
        executor,
        BENCH_MAX_CYCLES,
        crate::config::data_dir(),
    );
    controller.set_wall_timeout_secs(BENCH_WALL_SECS);
    let meta = controller.run(task, brain_id, None, headless)?;
    Ok((meta.status, meta.cycles))
}

#[cfg(not(feature = "webview"))]
fn bench_run(_brain_id: &str, _task: &str, _headless: bool) -> Result<(String, u32), String> {
    Err("webview-Feature nicht aktiv — kein Brain-Backend verfügbar".to_string())
}

/// Fährt den vollen Benchmark: `query` speist Phase A (Swarm-Abstimmung, in
/// CLI/REPL `repl::isolated_query`). Der Live-Teil (Phase B) läuft über den
/// Controller; getestet wird er e2e vom Orchestrator, nicht im Unit-Test.
pub fn run_benchmark<Q>(config: &BenchmarkConfig, query: Q) -> Result<BenchmarkReport, String>
where
    Q: Fn(&str, &str) -> Result<String, String>,
{
    // Sicherheitsmodell §5: nur auf sauberem Git-Tree starten.
    if !crate::autoresearch::git_status_clean(&config.workdir)? {
        return Err(format!(
            "Working Tree in {} ist nicht sauber — bitte committen oder stashen. \
             Der Benchmark läuft nur auf sauberem Stand und resettet nach jedem Run.",
            config.workdir.display()
        ));
    }

    let rounds = config.rounds.max(1);
    let facts = crate::self_research::gather_facts(&config.workdir, FACTS_MAX_CHARS);
    let mut winners: Vec<(usize, String)> = Vec::new();

    for round in 1..=rounds {
        println!("[benchmark] runde {round}/{rounds} — abstimmen…");
        // Phase A — Sammeln + Abstimmen. `&query` implementiert Fn ⇒ pro Runde
        // wiederverwendbar, ohne die Closure zu bewegen.
        let report = crate::self_research::run_self_research(
            &config.brains,
            &facts,
            config.suggestions,
            VOTE_TOP_K,
            &query,
        );
        let Some(winner) = winner_from_report(&report) else {
            println!("[benchmark] runde {round}: kein Sieger (keine Stimmen) — überspringe.");
            continue;
        };
        println!("[benchmark] Sieger: {winner}");
        winners.push((round, winner.clone()));

        // Phase A.5 — Verfeinerung: ein Brain uebersetzt den vagen Sieger in eine
        // konkrete, bounded Aufgabe (exakte Signatur + Testfaelle). Ohne das
        // explorieren die Brains ergebnislos (siehe build_refine_prompt).
        // Faellt die Verfeinerung aus, wird der Rohsieger verwendet.
        let refiner = config.brains.first().cloned().unwrap_or_default();
        let refined = if refiner.is_empty() {
            None
        } else {
            // Bis zu 2 Versuche: verlangt die Aufgabe etwas bereits Vorhandenes,
            // ist die Runde wertlos ("ist schon implementiert" -> keine Aenderung
            // -> faelschlich FAIL). Dann gezielt nach etwas Neuem fragen.
            let existing_api = crate::self_research::collect_public_api(&config.workdir.join("src"));
            let mut chosen: Option<String> = None;
            for attempt in 1..=2 {
                println!("[benchmark] verfeinern via {refiner} (Versuch {attempt}/2)…");
                let mut prompt = build_refine_prompt(&winner, &facts);
                if attempt > 1 {
                    prompt.push_str(
                        "\n\nWICHTIG: Dein vorheriger Vorschlag verlangte eine Funktion, die es \
                         BEREITS GIBT. Schlage etwas anderes vor, das noch NICHT existiert.",
                    );
                }
                match query(&refiner, &prompt) {
                    Ok(text) => match usable_refinement(&text) {
                        Some(t) if task_is_redundant(&t, &existing_api) => {
                            println!(
                                "[benchmark] verworfen: verlangt bereits vorhandene Funktion ({:?})",
                                proposed_fn_name(&t).unwrap_or_default()
                            );
                            continue;
                        }
                        other => {
                            chosen = other;
                            break;
                        }
                    },
                    Err(e) => {
                        println!("[benchmark] Verfeinerung fehlgeschlagen ({e}).");
                        break;
                    }
                }
            }
            chosen
        };
        let effective = refined.unwrap_or_else(|| winner.clone());
        if effective != winner {
            println!(
                "[benchmark] Aufgabe: {}",
                crate::char_prefix(&effective, 160)
            );
        }

        let task = build_task_prompt(&effective);
        let tid = task_id(&winner);

        // Phase B — pro Brain sequenziell bauen + objektiv messen.
        for brain in &config.brains {
            if !crate::autoresearch::git_status_clean(&config.workdir)? {
                return Err(format!(
                    "Working Tree in {} ist vor dem Run von {brain} nicht sauber — Abbruch.",
                    config.workdir.display()
                ));
            }
            let baseline = crate::autoresearch::git_head_sha(&config.workdir)?;
            let started = Instant::now();

            let (status, cycles) = match bench_run(brain, &task, config.headless) {
                Ok(res) => res,
                Err(e) => {
                    println!("[benchmark] {brain}: run fehlgeschlagen — {e}");
                    ("failed".to_string(), 0)
                }
            };
            let _ = status;

            // Eval: did_change → build → test (jeweils nur wenn die Stufe davor
            // greift; ein unveränderter Tree baut zwar, ist aber kein Erfolg).
            let did_change = tree_changed(&config.workdir);
            let compiled = did_change && run_eval_ok(&config.build_eval, &config.workdir);
            let tests_passed = compiled && run_eval_ok(&config.test_eval, &config.workdir);
            let latency_ms = started.elapsed().as_millis() as u64;

            let event = CodeEvent {
                brain_id: brain.clone(),
                task_id: tid.clone(),
                did_change,
                compiled,
                tests_passed,
                cycles,
                latency_ms,
                ts: crate::now_rfc3339(),
            };
            crate::code_score::record(&event);

            println!(
                "[benchmark] {brain}: run… did_change={} build={} test={} -> {}",
                yes_no(did_change),
                ok_x(compiled),
                ok_x(tests_passed),
                outcome_label(did_change, compiled, tests_passed)
            );

            // Reset: jedes Brain misst denselben Sieger unabhängig.
            reset_repo(&config.workdir, &baseline)?;
        }
    }

    let board = crate::code_score::leaderboard();
    print_leaderboard(&board);

    // Ergebnis als Wiki-Seite ablegen (nachvollziehbar, WAS gebaut werden sollte).
    let wiki_slug = {
        let wiki = crate::wiki_memory::WikiMemory::new(
            crate::config::data_dir().join("memory").join("wiki"),
        );
        let title = format!("code-benchmark-{}", crate::now_run_stamp());
        let body = format_benchmark_report(&winners, &board);
        match wiki.write_page(&title, &body) {
            Ok(slug) => {
                println!("[benchmark] Ergebnis abgelegt als [[{slug}]].");
                Some(slug)
            }
            Err(e) => {
                eprintln!("[benchmark] Wiki-Ablage fehlgeschlagen: {e}");
                None
            }
        }
    };

    Ok(BenchmarkReport {
        winners,
        leaderboard: board,
        wiki_slug,
    })
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "ja"
    } else {
        "nein"
    }
}

fn ok_x(b: bool) -> &'static str {
    if b {
        "ok"
    } else {
        "x"
    }
}

#[cfg(test)]
mod refine_tests {
    use super::*;

    #[test]
    fn refine_prompt_demands_concrete_signature_and_tests() {
        let p = build_refine_prompt("Sicherheitshaertung: Sandbox fuer Shell-Actions", "FAKTEN");
        assert!(p.contains("EXAKTER Rust-Signatur"), "{p}");
        assert!(p.contains("Testfaelle"), "{p}");
        assert!(p.contains("Sicherheitshaertung"), "Sieger muss drinstehen");
        assert!(p.contains("FAKTEN"), "Projektfakten muessen drinstehen");
        // Keine Architektur-Umbauten anfordern (sonst explorieren die Brains wieder).
        assert!(p.contains("KEINE Architektur-Umbauten"), "{p}");
    }

    #[test]
    fn redundanz_erkennung_verhindert_wertlose_runden() {
        // Storax-Beobachtung: Brains melden "ist bereits implementiert" und
        // aendern nichts -> wuerde faelschlich als FAIL zaehlen.
        let api = vec![
            "error_code".to_string(),
            "format_audit_line".to_string(),
            "parse".to_string(),
        ];
        let redundant = "In src/protocol.rs: fuege pub fn error_code(error: &str) -> &'static str \
                         hinzu, die Fehlermeldungen auf Slugs abbildet. Tests: a, b, c, d.";
        assert!(task_is_redundant(redundant, &api));

        let neu = "In src/protocol.rs: fuege pub fn action_summary(actions: &[Action]) -> String \
                   hinzu, die eine Kurzfassung liefert. Tests: leer, eine, viele, gemischt.";
        assert!(!task_is_redundant(neu, &api));

        // Ohne erkennbare Signatur nicht faelschlich als redundant werten.
        assert!(!task_is_redundant("Mach irgendwas mit Sicherheit.", &api));
    }

    #[test]
    fn proposed_fn_name_extrahiert_signatur() {
        assert_eq!(
            proposed_fn_name("... pub fn foo_bar(x: u8) -> bool ..."),
            Some("foo_bar".to_string())
        );
        assert_eq!(
            proposed_fn_name("Signatur: fn helper_fn() -> ()"),
            Some("helper_fn".to_string())
        );
        assert_eq!(proposed_fn_name("kein code hier"), None);
    }

    #[test]
    fn usable_refinement_rejects_too_short_and_keeps_real_specs() {
        assert_eq!(usable_refinement("   "), None);
        assert_eq!(usable_refinement("zu kurz"), None);
        let spec = "Datei src/foo.rs: fuege pub fn bar(x: &str) -> usize hinzu, die die \
                    Zeichenzahl liefert. Tests: leer, ascii, umlaute, lang.";
        assert_eq!(usable_refinement(spec), Some(spec.trim().to_string()));
        // Whitespace wird getrimmt.
        let padded = format!("   {spec}   ");
        assert_eq!(usable_refinement(&padded), Some(spec.trim().to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::self_research::RankedSuggestion;

    fn stats(brain: &str, attempts: usize, wilson: f64) -> CodeStats {
        CodeStats {
            brain_id: brain.to_string(),
            attempts,
            change_rate: 1.0,
            compile_rate: 1.0,
            pass_rate: 1.0,
            wilson_pass: wilson,
        }
    }

    #[test]
    fn task_id_is_stable_and_distinct() {
        assert_eq!(task_id("Sandbox einführen"), task_id("Sandbox einführen"));
        // Whitespace-robust (getrimmt).
        assert_eq!(task_id("  Sandbox einführen "), task_id("Sandbox einführen"));
        assert_ne!(task_id("Sandbox einführen"), task_id("Tests ergänzen"));
    }

    #[test]
    fn build_task_prompt_contains_winner_and_contract() {
        let p = build_task_prompt("Protokoll versionieren");
        assert!(p.contains("Protokoll versionieren"));
        assert!(p.contains("WEBAGENT/1"));
        assert!(p.contains("cargo test --lib"));
        assert!(p.contains("Rohformat"));
    }

    #[test]
    fn winner_from_report_takes_top_one() {
        let report = SelfResearchReport {
            catalog: vec!["Alpha".to_string(), "Beta".to_string()],
            ranked: vec![
                RankedSuggestion {
                    index: 2,
                    text: "Beta".to_string(),
                    points: 6,
                    approvals: 2,
                },
                RankedSuggestion {
                    index: 1,
                    text: "Alpha".to_string(),
                    points: 4,
                    approvals: 2,
                },
            ],
            consolidated_by: Some("claude".to_string()),
            collected: 2,
            voters: 2,
            brains_total: 2,
        };
        assert_eq!(winner_from_report(&report), Some("Beta".to_string()));
    }

    #[test]
    fn winner_from_report_none_without_votes() {
        let report = SelfResearchReport {
            catalog: vec!["Alpha".to_string()],
            ranked: Vec::new(),
            consolidated_by: None,
            collected: 1,
            voters: 0,
            brains_total: 1,
        };
        assert_eq!(winner_from_report(&report), None);
    }

    #[test]
    fn is_pass_requires_all_three() {
        assert!(is_pass(true, true, true));
        assert!(!is_pass(false, true, true));
        assert!(!is_pass(true, false, true));
        assert!(!is_pass(true, true, false));
    }

    #[test]
    fn outcome_label_classifies_each_stage() {
        assert_eq!(outcome_label(false, false, false), "SKIP (keine Änderung)");
        assert_eq!(outcome_label(true, false, false), "FAIL (build)");
        assert_eq!(outcome_label(true, true, false), "FAIL (test)");
        assert_eq!(outcome_label(true, true, true), "PASS");
    }

    #[test]
    fn format_report_shows_winners_and_table() {
        let winners = vec![
            (1, "Sandbox einführen".to_string()),
            (2, "Tests ergänzen".to_string()),
        ];
        let board = vec![stats("kimi", 4, 0.512), stats("qwen", 4, 0.180)];
        let body = format_benchmark_report(&winners, &board);
        assert!(body.contains("1. Sandbox einführen"));
        assert!(body.contains("2. Tests ergänzen"));
        assert!(body.contains("| brain | attempts | change% | compile% | pass% | wilson_pass |"));
        assert!(body.contains("| kimi | 4 |"));
        assert!(body.contains("0.512"));
    }

    #[test]
    fn format_report_handles_empty() {
        let body = format_benchmark_report(&[], &[]);
        assert!(body.contains("(keine — keine Stimmen gesammelt)"));
        assert!(body.contains("(keine Daten)"));
    }
}
