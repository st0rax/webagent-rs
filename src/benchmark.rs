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
//!   Wall-Timeout + kleinem `max_cycles`) SEINE zugeteilte Aufgabe bauen lassen
//!   ([`assign_tasks`]), dann objektiv evaluieren (`did_change` →
//!   `cargo build --lib` → `cargo test --lib`), das
//!   [`CodeEvent`](crate::code_score::CodeEvent) speichern und den Tree hart auf
//!   die Baseline zurücksetzen (`git reset --hard` + `git clean -fd`). Jedes
//!   Brain startet identisch.
//! - **Phase C (Ernten):** der Diff jedes BESTANDENEN Brains wird vor dem Reset
//!   gesichert und danach wieder eingespielt, erneut gebaut/getestet und mit dem
//!   Brain als Autor committet. Der Benchmark ist damit Fertigungsstraße UND
//!   Messgerät: er misst objektiv und behält, was die Messung bestanden hat.
//!   `--no-harvest` schaltet auf reines Messen zurück.
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

/// Wählt den zu erntenden Kandidaten: wenige Iterationen schlagen viele, bei
/// Gleichstand entscheidet die kürzere Laufzeit.
///
/// „Beim ersten Versuch grün" ist das stärkere Signal als „nach neun Korrekturen
/// grün" — beide bestehen, aber nur eines davon ist verlässliche Arbeit.
pub fn pick_harvest(candidates: &[HarvestCandidate]) -> Option<&HarvestCandidate> {
    candidates
        .iter()
        .filter(|c| !c.patch.trim().is_empty())
        .min_by_key(|c| (c.iterations, c.latency_ms))
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

/// Zählt die bestandenen Tests aus einer `cargo test`-Ausgabe
/// (`test result: ok. 387 passed; 0 failed; …`).
///
/// Anti-Schummel-Signal: ein Brain kann sonst eine VERWAISTE Datei anlegen
/// (nicht im Modulbaum) — dann baut und testet alles grün, obwohl nichts
/// integriert wurde. Real beobachtet 2026-07-21: claude und zai "bestanden"
/// mit einer Datei unter dem erfundenen Pfad src/executor/…
pub fn parse_test_count(output: &str) -> Option<u32> {
    let mut total: Option<u32> = None;
    for part in output.split_whitespace().collect::<Vec<_>>().windows(2) {
        if part[1].starts_with("passed") {
            if let Ok(n) = part[0].parse::<u32>() {
                total = Some(total.unwrap_or(0) + n);
            }
        }
    }
    total
}

/// Prompt, der einen VAGEN Abstimmungssieger in eine KONKRETE, bounded
/// Coding-Aufgabe übersetzt (Phase A.5).
///
/// Ohne diesen Schritt bekamen alle Brains den rohen Architekturwunsch und
/// explorierten ergebnislos (22/22 `did_change=false`, 2026-07-21). `files`
/// erzwingt zusätzlich eine EXISTIERENDE Zieldatei — eine erfundene wie
/// `src/executor/powershell.rs` führte zu verwaisten Dateien und damit zu
/// falschen PASS-Wertungen.
pub fn build_refine_prompt(winner: &str, facts: &str, files: &[String]) -> String {
    format!(
        "Du planst eine Coding-Aufgabe fuer das Rust-Projekt webagent-rs.\n\n\
         Projektfakten:\n{facts}\n\n\
         Zu konkretisierender Verbesserungsvorschlag:\n{winner}\n\n\
         Uebersetze ihn in EINE kleine, in sich geschlossene Aufgabe, die ein \
         Agent in wenigen Schritten umsetzen kann. Anforderungen an deine Antwort:\n\
         - genau EINE Zieldatei benennen, und sie MUSS aus dieser Liste stammen \
           (erfinde KEINE Pfade, lege KEINE neuen Dateien/Module an):\n           {files}\n\
         - genau EINE neue oeffentliche Funktion mit EXAKTER Rust-Signatur angeben\n\
         - das erwartete Verhalten in 2-4 Saetzen praezise beschreiben\n\
         - mindestens 4 konkrete Testfaelle auflisten\n\
         - nur std und bereits vorhandene Dependencies verwenden\n\
         - KEINE Architektur-Umbauten, keine neuen Module, kein Refactoring\n\n\
         Antworte AUSSCHLIESSLICH mit der Aufgabenbeschreibung als Fliesstext \
         (kein JSON, keine Einleitung, kein Nachwort).",
        facts = crate::char_prefix(facts, 900),
        winner = winner.trim(),
        files = if files.is_empty() {
            "src/protocol.rs, src/shell_policy.rs, src/file_actions.rs".to_string()
        } else {
            files.join(", ")
        }
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

/// Alle gevoteten Vorschläge in Rangfolge (Platz 1 zuerst), leere verworfen.
pub fn ranked_from_report(report: &SelfResearchReport) -> Vec<String> {
    report
        .ranked
        .iter()
        .map(|r| r.text.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Verteilt die gevoteten Vorschläge auf die Brains — Fertigungsstraße statt
/// Turnier: jedes Brain bekommt eine EIGENE Aufgabe, deshalb kann die Arbeit
/// aller bestandenen Brains geerntet werden statt nur die des besten.
///
/// Bauen alle dasselbe, kollidieren die Patches im selben Code und sieben von
/// acht Beiträgen sind zwangsläufig Ausschuss — die Messung war brauchbar, die
/// Produktion nicht.
///
/// `round` rotiert die Zuteilung: Brain `i` baut Rang `(i + round) % k`. Über
/// mehrere Runden sieht damit jedes Brain jeden Rang, sodass keines dauerhaft
/// die leichteren oder schwereren Aufgaben zieht und der Score fair bleibt.
/// Gibt es weniger Vorschläge als Brains, teilen sich mehrere Brains einen Rang
/// (dann gewinnt beim Ernten der beste — wie im Turnier).
pub fn assign_tasks(brains: &[String], ranked: &[String], round: usize) -> Vec<(String, String)> {
    if ranked.is_empty() {
        return Vec::new();
    }
    brains
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let idx = (i + round) % ranked.len();
            (b.clone(), ranked[idx].clone())
        })
        .collect()
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
/// Führt ein Eval-Kommando aus: `(exit==0, Ausgabe)`. Die Ausgabe geht als
/// Kontext in den Repair-Versuch (Compiler-/Testfehler zurück ans Brain).
fn run_eval_detail(cmd: &str, workdir: &Path) -> (bool, String) {
    match crate::autoresearch::run_eval_with_timeout(cmd, workdir, EVAL_TIMEOUT_SECS) {
        Ok((code, out)) => (code == Some(0), out),
        Err(e) => (false, e),
    }
}

/// Baut den Folgeauftrag nach einem fehlgeschlagenen Build/Test: die
/// Original-Aufgabe plus die echte Fehlerausgabe als Kontext.
///
/// Ohne diesen Schritt zählte ein Brain als Fehlschlag, obwohl es oft nur einen
/// Tippfehler von grün entfernt war — echte Coding-Agenten lesen den
/// Compilerfehler und korrigieren (Storax-Vorschlag 2026-07-21).
pub fn build_repair_prompt(task: &str, stage: &str, output: &str) -> String {
    format!(
        "{task}\n\n--- KORREKTUR NÖTIG ---\n\
         Deine bisherige Änderung ist im Arbeitsverzeichnis vorhanden, aber \
         `{stage}` schlägt fehl. Lies die Fehlerausgabe, finde die Ursache und \
         korrigiere sie mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Fange NICHT \
         von vorne an — repariere das Vorhandene.\n\n\
         Fehlerausgabe (gekürzt):\n{out}",
        task = task.trim(),
        stage = stage,
        out = crate::char_prefix(output.trim(), 2500)
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

/// Sichert die Arbeit des laufenden Brains als Patch, BEVOR `reset_repo` sie
/// verwirft. `git add -A` davor, damit neue Dateien im Diff auftauchen.
fn capture_patch(workdir: &Path) -> Result<String, String> {
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workdir)
        .output();
    let out = std::process::Command::new("git")
        .args(["diff", "--cached", "--binary"])
        .current_dir(workdir)
        .output()
        .map_err(|e| format!("git diff fehlgeschlagen: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Spielt einen geernteten Patch wieder ein, verifiziert ihn erneut (Build +
/// Tests) und committet ihn mit dem Brain als Autor.
///
/// Die Wiederholung von Build und Tests ist kein Ritual: der Patch wurde auf
/// einem Tree gemessen, der seither zurückgesetzt wurde — erst der zweite grüne
/// Durchlauf belegt, dass der Code auch eigenständig trägt.
fn harvest_commit(
    cand: &HarvestCandidate,
    winner: &str,
    config: &BenchmarkConfig,
) -> Result<(), String> {
    let patch_path = crate::config::data_dir()
        .join("benchmark")
        .join(format!("harvest_{}.patch", cand.brain));
    if let Some(parent) = patch_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
    }
    std::fs::write(&patch_path, &cand.patch).map_err(|e| format!("Patch schreiben: {e}"))?;

    let apply = std::process::Command::new("git")
        .args(["apply", "--index"])
        .arg(&patch_path)
        .current_dir(&config.workdir)
        .output()
        .map_err(|e| format!("git apply: {e}"))?;
    if !apply.status.success() {
        return Err(format!(
            "Patch von {} liess sich nicht einspielen: {}",
            cand.brain,
            String::from_utf8_lossy(&apply.stderr).trim()
        ));
    }

    let (b_ok, _) = run_eval_detail(&config.build_eval, &config.workdir);
    if !b_ok {
        return Err(format!("Nachkontrolle: Build rot — {} verworfen", cand.brain));
    }
    let (t_ok, _) = run_eval_detail(&config.test_eval, &config.workdir);
    if !t_ok {
        return Err(format!("Nachkontrolle: Tests rot — {} verworfen", cand.brain));
    }

    let msg = format!(
        "feat(bench): {headline} — vom webagent gebaut ({brain})\n\n\
         Abstimmungs-Sieger der Benchmark-Runde, umgesetzt von {brain} in \
         {iters} Iteration(en). Nach dem Wiedereinspielen erneut verifiziert: \
         `{build}` und `{test}` gruen.\n\n\
         Authored-by: {brain} (webagent benchmark harvest)\n",
        headline = crate::char_prefix(winner.trim(), 72),
        brain = cand.brain,
        iters = cand.iterations,
        build = config.build_eval,
        test = config.test_eval,
    );
    let commit = std::process::Command::new("git")
        .args(["commit", "-m", &msg])
        .current_dir(&config.workdir)
        .output()
        .map_err(|e| format!("git commit: {e}"))?;
    if !commit.status.success() {
        return Err(format!(
            "Commit fehlgeschlagen: {}",
            String::from_utf8_lossy(&commit.stderr).trim()
        ));
    }
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

/// Übersetzt EINEN gevoteten Vorschlag in eine konkrete, bounded Coding-Aufgabe
/// (Phase A.5). Zwei Versuche: verlangt die Aufgabe etwas bereits Vorhandenes,
/// wäre der Bauauftrag wertlos ("ist schon implementiert" → keine Änderung →
/// fälschlich FAIL). Fällt die Verfeinerung aus, trägt der Rohvorschlag.
fn refine_one<Q>(
    winner: &str,
    facts: &str,
    refiner: &str,
    existing_api: &[String],
    src_files: &[String],
    query: &Q,
) -> String
where
    Q: Fn(&str, &str) -> Result<String, String>,
{
    if refiner.is_empty() {
        return winner.to_string();
    }
    for attempt in 1..=2 {
        let mut prompt = build_refine_prompt(winner, facts, src_files);
        if attempt > 1 {
            prompt.push_str(
                "

WICHTIG: Dein vorheriger Vorschlag verlangte eine Funktion, die es                  BEREITS GIBT. Schlage etwas anderes vor, das noch NICHT existiert.",
            );
        }
        match query(refiner, &prompt) {
            Ok(text) => match usable_refinement(&text) {
                Some(t) if task_is_redundant(&t, existing_api) => {
                    println!(
                        "[benchmark]   verworfen: verlangt bereits vorhandene Funktion ({:?})",
                        proposed_fn_name(&t).unwrap_or_default()
                    );
                    continue;
                }
                Some(t) => return t,
                None => break,
            },
            Err(e) => {
                println!("[benchmark]   Verfeinerung fehlgeschlagen ({e}).");
                break;
            }
        }
    }
    winner.to_string()
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
    let mut harvested: Vec<(String, String)> = Vec::new();

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
        let ranked = ranked_from_report(&report);
        if ranked.is_empty() {
            println!("[benchmark] runde {round}: kein Sieger (keine Stimmen) — überspringe.");
            continue;
        }
        let winner = ranked[0].clone();
        println!("[benchmark] Sieger: {winner}");
        winners.push((round, winner.clone()));

        // Fertigungsstrasse: jedes Brain baut einen EIGENEN Rang der Rangliste,
        // pro Runde rotiert. Damit kollidieren die Patches nicht und die Arbeit
        // ALLER bestandenen Brains kann geerntet werden — nicht nur die des
        // besten. Die Rotation haelt die Messung fair (jedes Brain sieht ueber
        // die Runden jeden Rang).
        let assignments = assign_tasks(&config.brains, &ranked, round);

        // Phase A.5 — jede zugeteilte Aufgabe konkretisieren. Gleiche Vorschlaege
        // nur einmal verfeinern (bei weniger Vorschlaegen als Brains).
        let refiner = config.brains.first().cloned().unwrap_or_default();
        let existing_api = crate::self_research::collect_public_api(&config.workdir.join("src"));
        let src_files: Vec<String> = crate::self_research::collect_modules(&config.workdir.join("src"))
            .into_iter()
            .map(|(name, _lines)| format!("src/{name}"))
            .collect();
        let mut refined_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut plan: Vec<(String, String)> = Vec::new();
        for (brain, raw) in &assignments {
            let eff = match refined_cache.get(raw) {
                Some(t) => t.clone(),
                None => {
                    let t = crate::StageTimer::start(format!("verfeinern fuer {brain} via {refiner}"));
                    let e = refine_one(raw, &facts, &refiner, &existing_api, &src_files, &query);
                    t.finish(crate::char_prefix(&e, 90));
                    refined_cache.insert(raw.clone(), e.clone());
                    e
                }
            };
            println!("[benchmark] {brain} -> {}", crate::char_prefix(&eff, 120));
            plan.push((brain.clone(), eff));
        }

        // Baseline-Testzahl auf sauberem Tree: ein PASS muss die Testzahl
        // ERHOEHEN, sonst zaehlt eine verwaiste (nicht eingebundene) Datei als
        // Erfolg — real beobachtet 2026-07-21.
        let baseline_tests = {
            let t = crate::StageTimer::start("Baseline-Tests".to_string());
            let (_ok, out) = run_eval_detail(&config.test_eval, &config.workdir);
            let n = parse_test_count(&out);
            t.finish(&format!("Baseline: {} Tests", n.unwrap_or(0)));
            n.unwrap_or(0)
        };

        // Phase B — pro Brain sequenziell bauen + objektiv messen.
        let mut harvest_pool: Vec<HarvestCandidate> = Vec::new();
        for (brain, effective) in &plan {
            let task = build_task_prompt(effective);
            let tid = task_id(effective);
            if !crate::autoresearch::git_status_clean(&config.workdir)? {
                return Err(format!(
                    "Working Tree in {} ist vor dem Run von {brain} nicht sauber — Abbruch.",
                    config.workdir.display()
                ));
            }
            let baseline = crate::autoresearch::git_head_sha(&config.workdir)?;
            let started = Instant::now();

            // Repair-Loop: schlägt Build/Test fehl, geht die echte Fehlerausgabe
            // als Kontext zurück ans Brain (bis zu max_iterations). Zwischen den
            // Iterationen wird NICHT resettet — das Brain repariert sein eigenes
            // Werk, wie ein echter Coding-Agent am Compilerfehler.
            let max_iter = config.max_iterations.max(1);
            let mut attempt_task = task.clone();
            let mut cycles = 0u32;
            let mut did_change = false;
            let mut compiled = false;
            let mut tests_passed = false;
            let mut iterations = 0u32;

            for iter in 1..=max_iter {
                iterations = iter;
                let t = crate::StageTimer::start(format!("{brain} Iteration {iter}/{max_iter}: Brain baut"));
                match bench_run(brain, &attempt_task, config.headless) {
                    Ok((_status, c)) => {
                        cycles += c;
                        t.finish(&format!("Brain fertig ({c} Zyklen)"));
                    }
                    Err(e) => {
                        t.finish("Brain-Run fehlgeschlagen");
                        println!("[benchmark] {brain}: run fehlgeschlagen — {e}");
                    }
                }

                did_change = tree_changed(&config.workdir);
                if !did_change {
                    // Nichts geändert -> Reparatur sinnlos, das Brain hat nicht gebaut.
                    println!("[benchmark] {brain}: Iteration {iter}/{max_iter} — keine Änderung");
                    break;
                }
                let tb = crate::StageTimer::start(format!("{brain}: {}", config.build_eval));
                let (b_ok, b_out) = run_eval_detail(&config.build_eval, &config.workdir);
                tb.finish(if b_ok { "Build ok" } else { "Build ROT" });
                compiled = b_ok;
                if !compiled {
                    println!("[benchmark] {brain}: Iteration {iter}/{max_iter} — Build rot");
                    if iter < max_iter {
                        attempt_task = build_repair_prompt(&task, &config.build_eval, &b_out);
                        continue;
                    }
                    break;
                }
                let tt = crate::StageTimer::start(format!("{brain}: {}", config.test_eval));
                let (t_ok, t_out) = run_eval_detail(&config.test_eval, &config.workdir);
                let after = parse_test_count(&t_out).unwrap_or(0);
                tt.finish(&format!("Tests {} ({after} bestanden)", if t_ok { "ok" } else { "ROT" }));
                // Gruen UND mehr Tests als vorher: nur dann ist der Code wirklich
                // eingebunden und getestet (verwaiste Datei erhoeht die Zahl nicht).
                tests_passed = t_ok && after > baseline_tests;
                if t_ok && after <= baseline_tests {
                    println!(
                        "[benchmark]   Tests gruen, aber Testzahl unveraendert ({after} <= {baseline_tests}) — nicht eingebunden"
                    );
                }
                if tests_passed {
                    println!("[benchmark] {brain}: Iteration {iter}/{max_iter} — grün");
                    break;
                }
                println!("[benchmark] {brain}: Iteration {iter}/{max_iter} — Tests rot");
                if iter < max_iter {
                    attempt_task = build_repair_prompt(&task, &config.test_eval, &t_out);
                }
            }
            let latency_ms = started.elapsed().as_millis() as u64;

            let event = CodeEvent {
                brain_id: brain.clone(),
                task_id: tid.clone(),
                did_change,
                compiled,
                tests_passed,
                cycles,
                iterations,
                latency_ms,
                ts: crate::now_rfc3339(),
            };
            crate::code_score::record(&event);

            println!(
                "[benchmark] {brain}: {iterations} Iteration(en), did_change={} build={} test={} -> {}",
                yes_no(did_change),
                ok_x(compiled),
                ok_x(tests_passed),
                outcome_label(did_change, compiled, tests_passed)
            );

            // Bestandene Arbeit sichern, BEVOR der Reset sie verwirft.
            if config.harvest && is_pass(did_change, compiled, tests_passed) {
                match capture_patch(&config.workdir) {
                    Ok(patch) if !patch.trim().is_empty() => {
                        println!(
                            "[benchmark]   {brain}: Patch gesichert ({} Zeilen) — Kandidat für die Ernte",
                            patch.lines().count()
                        );
                        harvest_pool.push(HarvestCandidate {
                            brain: brain.clone(),
                            task: effective.clone(),
                            patch,
                            iterations,
                            latency_ms,
                        });
                    }
                    Ok(_) => println!("[benchmark]   {brain}: leerer Patch — nichts zu ernten"),
                    Err(e) => println!("[benchmark]   {brain}: Patch-Sicherung fehlgeschlagen — {e}"),
                }
            }

            // Reset: jedes Brain misst denselben Sieger unabhängig.
            reset_repo(&config.workdir, &baseline)?;
        }

        // Ernte: JEDER bestandene Lauf wird eingespielt — die Brains bauten
        // verschiedene Aufgaben, also ist nichts davon Ausschuss. Reihenfolge:
        // die souveraensten zuerst (wenige Iterationen), damit bei einem
        // seltenen Konflikt der schwaechere Beitrag zurueckstecken muss.
        if config.harvest {
            if harvest_pool.is_empty() {
                println!("[benchmark] Nichts zu ernten — kein Brain hat bestanden.");
            }
            harvest_pool.sort_by_key(|c| (c.iterations, c.latency_ms));
            for cand in &harvest_pool {
                if cand.patch.trim().is_empty() {
                    continue;
                }
                let t = crate::StageTimer::start(format!(
                    "Ernte: {} einspielen + nachpruefen",
                    cand.brain
                ));
                match harvest_commit(cand, &cand.task, config) {
                    Ok(()) => {
                        t.finish("geerntet und committet");
                        println!(
                            "[benchmark] GEERNTET: {} ({} Iteration(en)) — Code bleibt im Repo.",
                            cand.brain, cand.iterations
                        );
                        harvested.push((cand.brain.clone(), cand.task.clone()));
                    }
                    Err(e) => {
                        t.finish("Ernte fehlgeschlagen");
                        println!("[benchmark] Ernte verworfen ({}): {e}", cand.brain);
                        let head = crate::autoresearch::git_head_sha(&config.workdir)?;
                        reset_repo(&config.workdir, &head)?;
                    }
                }
            }
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
        harvested,
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
        let p = build_refine_prompt("Sicherheitshaertung: Sandbox fuer Shell-Actions", "FAKTEN", &[]);
        assert!(p.contains("EXAKTER Rust-Signatur"), "{p}");
        assert!(p.contains("Testfaelle"), "{p}");
        assert!(p.contains("Sicherheitshaertung"), "Sieger muss drinstehen");
        assert!(p.contains("FAKTEN"), "Projektfakten muessen drinstehen");
        // Keine Architektur-Umbauten anfordern (sonst explorieren die Brains wieder).
        assert!(p.contains("KEINE Architektur-Umbauten"), "{p}");
    }

    #[test]
    fn test_count_gate_erkennt_verwaiste_datei() {
        // Anti-Schummel: eine nicht eingebundene Datei laesst die Testzahl gleich.
        let before = "test result: ok. 387 passed; 0 failed; 0 ignored";
        let after_orphan = "test result: ok. 387 passed; 0 failed; 0 ignored";
        let after_real = "test result: ok. 391 passed; 0 failed; 0 ignored";
        assert_eq!(parse_test_count(before), Some(387));
        assert_eq!(parse_test_count(after_orphan), Some(387));
        assert_eq!(parse_test_count(after_real), Some(391));
        // Mehrere Testbinaries werden summiert (lib + bin).
        let multi = "test result: ok. 10 passed; 0 failed\ntest result: ok. 5 passed; 0 failed";
        assert_eq!(parse_test_count(multi), Some(15));
        assert_eq!(parse_test_count("kein testoutput"), None);
    }

    #[test]
    fn refine_prompt_erzwingt_existierende_zieldatei() {
        let files = vec!["src/protocol.rs".to_string(), "src/executor.rs".to_string()];
        let p = build_refine_prompt("Sicherheit haerten", "FAKTEN", &files);
        assert!(p.contains("src/protocol.rs"), "Dateiliste fehlt");
        assert!(p.contains("erfinde KEINE Pfade"), "Verbot fehlt");
        // Ohne Liste ein sinnvoller Default statt leerer Aufzaehlung.
        let p2 = build_refine_prompt("x", "y", &[]);
        assert!(p2.contains("src/protocol.rs"));
    }

    #[test]
    fn repair_prompt_enthaelt_fehlerausgabe_und_verbietet_neuanfang() {
        let p = build_repair_prompt(
            "Baue pub fn foo() -> u8",
            "cargo build --lib",
            "error[E0433]: failed to resolve: use of undeclared crate `serde_jsonx`",
        );
        assert!(p.contains("Baue pub fn foo"), "Original-Aufgabe fehlt");
        assert!(p.contains("cargo build --lib"), "Stufe fehlt");
        assert!(p.contains("E0433"), "Fehlerausgabe fehlt");
        assert!(p.contains("NICHT von vorne"), "Neuanfang muss verboten sein");
        // Sehr lange Ausgaben werden gekuerzt (Kontextbudget).
        let long = "x".repeat(9000);
        assert!(build_repair_prompt("t", "cargo test", &long).chars().count() < 3200);
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

    fn cand(brain: &str, iters: u32, ms: u64, patch: &str) -> HarvestCandidate {
        HarvestCandidate {
            brain: brain.to_string(),
            task: "Testaufgabe".to_string(),
            patch: patch.to_string(),
            iterations: iters,
            latency_ms: ms,
        }
    }

    #[test]
    fn harvest_prefers_fewer_iterations() {
        // Beim ersten Versuch gruen schlaegt "nach neun Korrekturen gruen" —
        // beide bestehen, nur eines davon ist verlaessliche Arbeit.
        let pool = vec![
            cand("zai", 9, 1_000, "diff --git a/src/x.rs"),
            cand("deepseek", 1, 90_000, "diff --git a/src/y.rs"),
        ];
        assert_eq!(pick_harvest(&pool).unwrap().brain, "deepseek");
    }

    #[test]
    fn harvest_breaks_iteration_tie_by_latency() {
        let pool = vec![
            cand("kimi", 2, 80_000, "diff --git a/src/x.rs"),
            cand("deepseek", 2, 20_000, "diff --git a/src/y.rs"),
        ];
        assert_eq!(pick_harvest(&pool).unwrap().brain, "deepseek");
    }

    #[test]
    fn harvest_ignores_empty_patches_and_empty_pool() {
        // Ein PASS ohne Diff waere nichts zum Einspielen — darf nicht gewinnen.
        let pool = vec![cand("geist", 1, 10, "   
  ")];
        assert!(pick_harvest(&pool).is_none());
        assert!(pick_harvest(&[]).is_none());
    }

    #[test]
    fn harvest_flag_off_is_the_default_shape() {
        // Ohne --harvest bleibt der Benchmark ein reines Messgeraet.
        assert!(!bench_config_for_test().harvest);
    }

    fn bench_config_for_test() -> BenchmarkConfig {
        BenchmarkConfig {
            brains: vec!["deepseek".to_string()],
            rounds: 1,
            suggestions: 10,
            build_eval: "cargo build --lib".to_string(),
            test_eval: "cargo test --lib".to_string(),
            workdir: PathBuf::from("."),
            headless: true,
            max_iterations: 10,
            harvest: false,
        }
    }

    #[test]
    fn assign_gives_every_brain_its_own_task() {
        // Fertigungsstrasse: acht Brains, acht verschiedene Raenge — sonst
        // kollidieren die Patches und nur einer waere erntbar.
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = ["r1", "r2", "r3"].iter().map(|s| s.to_string()).collect();
        let got = assign_tasks(&brains, &ranked, 0);
        let tasks: std::collections::HashSet<&String> = got.iter().map(|(_, t)| t).collect();
        assert_eq!(got.len(), 3);
        assert_eq!(tasks.len(), 3, "jedes Brain braucht eine eigene Aufgabe");
    }

    #[test]
    fn assign_rotates_across_rounds_so_scoring_stays_fair() {
        // Ueber die Runden muss jedes Brain jeden Rang sehen, sonst zieht eines
        // dauerhaft die leichteren Aufgaben und der Score waere verzerrt.
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = ["r1", "r2", "r3"].iter().map(|s| s.to_string()).collect();
        let seen: std::collections::HashSet<String> = (0..3)
            .flat_map(|round| assign_tasks(&brains, &ranked, round))
            .filter(|(b, _)| b == "a")
            .map(|(_, t)| t)
            .collect();
        assert_eq!(seen.len(), 3, "Brain a muss ueber 3 Runden alle 3 Raenge bauen");
    }

    #[test]
    fn assign_shares_ranks_when_fewer_suggestions_than_brains() {
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = vec!["nur_einer".to_string()];
        let got = assign_tasks(&brains, &ranked, 0);
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|(_, t)| t == "nur_einer"));
    }

    #[test]
    fn assign_without_suggestions_is_empty() {
        let brains: Vec<String> = vec!["a".to_string()];
        assert!(assign_tasks(&brains, &[], 0).is_empty());
    }

    #[test]
    fn ranked_from_report_drops_empty_entries() {
        let report = SelfResearchReport {
            catalog: Vec::new(),
            ranked: vec![
                crate::self_research::RankedSuggestion {
                    index: 1,
                    text: "Fehlerhierarchie mit thiserror einfuehren".to_string(),
                    points: 10,
                    approvals: 2,
                },
                crate::self_research::RankedSuggestion {
                    index: 2,
                    text: "   ".to_string(),
                    points: 5,
                    approvals: 1,
                },
            ],
            consolidated_by: None,
            collected: 2,
            voters: 2,
            brains_total: 2,
        };
        assert_eq!(ranked_from_report(&report).len(), 1);
    }
}
