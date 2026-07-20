//! autoresearch — Karpathys Modify→Verify→Keep/Discard-Muster auf webagent-rs.
//!
//! Äußere Schleife um den vorhandenen Controller: eine messbare Metrik
//! (`eval_cmd`, Vertrag: exit 0 + letzte stdout-Zeile ist eine Zahl) wird über
//! mehrere Iterationen autonom verbessert. Git ist das Sicherheitsnetz — jede
//! Iteration endet als Commit (behalten) oder als Full-Reset auf den
//! Start-SHA (verworfen). Läuft immer auf einem eigenen Branch
//! (`autoresearch/<timestamp>`), nie auf einem schmutzigen Working Tree.
//! Spezifikation: `docs/AUTORESEARCH_PLAN.md`.
//!
//! Die `git_*`-Helfer laufen bewusst über `std::process::Command::new("git")`
//! mit explizitem `current_dir` — **nicht** über `executor::ShellExecutor`,
//! weil das Sicherheits-Commits des Tools selbst sind, keine Brain-Aktionen.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Controller-Zyklen pro Modify-Schritt: klein gehalten, damit der Brain bei
/// "einer fokussierten Änderung" bleibt statt einer offenen Aufgabe (§10).
const MODIFY_MAX_CYCLES: usize = 5;
/// Wie viele vergangene Iterationen der Modify-Prompt zusammenfasst.
const PROMPT_HISTORY_LIMIT: usize = 3;
/// Kürzung der Brain-Zusammenfassung im Prompt/Log (Zeichen).
const SUMMARY_PREVIEW_CHARS: usize = 200;

/// Richtung der Metrik-Verbesserung.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    HigherIsBetter,
    LowerIsBetter,
}

impl Direction {
    /// Menschenlesbares Label für Prompt und Ausgabe.
    fn label(self) -> &'static str {
        match self {
            Direction::HigherIsBetter => "höher ist besser",
            Direction::LowerIsBetter => "niedriger ist besser",
        }
    }
}

impl std::str::FromStr for Direction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "higher" => Ok(Direction::HigherIsBetter),
            "lower" => Ok(Direction::LowerIsBetter),
            other => Err(format!(
                "unbekannte Richtung {other:?} (erwartet: higher|lower)"
            )),
        }
    }
}

/// Konfiguration eines Autoresearch-Laufs (siehe Spec §5).
///
/// Ergänzt gegenüber dem Spec-Vorschlag um `eval_timeout_secs` (Timeout des
/// Eval-Befehls, CLI-Flag `--eval-timeout`; Spec §10 verlangt ein Timeout).
#[derive(Debug, Clone)]
pub struct AutoResearchConfig {
    pub brain_id: String,
    pub goal: String,
    pub eval_cmd: String,
    pub direction: Direction,
    pub max_iterations: usize,
    pub no_improve_abort: usize,
    pub headless: bool,
    /// Git-Repo-Root, in dem gearbeitet wird.
    pub workdir: PathBuf,
    /// Timeout des Eval-Befehls in Sekunden (Default 300).
    pub eval_timeout_secs: u64,
}

/// Protokoll einer einzelnen Iteration (eine Zeile in `iterations.jsonl`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IterationLog {
    pub n: usize,
    pub metric_before: f64,
    /// `None`, wenn `eval_cmd` fehlschlug (Timeout, exit != 0, nicht parsebar).
    pub metric_after: Option<f64>,
    pub kept: bool,
    pub commit_sha: Option<String>,
    /// Letzte Message/finish-Text aus dem Controller-Run.
    pub brain_summary: String,
    /// `now_rfc3339()`-Zeitstempel.
    pub ts: String,
}

/// Endergebnis eines Autoresearch-Laufs.
#[derive(Debug, Clone, Serialize)]
pub struct AutoResearchReport {
    pub branch: String,
    pub final_metric: f64,
    pub iterations: Vec<IterationLog>,
    /// "max_iterations" | "no_improve_abort"
    /// (Baseline-Eval-Fehler bricht als `Err` mit "eval_failed_at_start" ab).
    pub stopped_reason: String,
}

/// Startet einen Autoresearch-Lauf mit echtem Modify-Schritt (Controller +
/// aktives Brain) und echtem Eval-Befehl. Logs unter
/// `data/autoresearch/<run_id>/` (log.md + iterations.jsonl).
pub fn run(config: AutoResearchConfig) -> Result<AutoResearchReport, String> {
    let run_id = crate::now_run_stamp();
    let log_dir = crate::config::data_dir().join("autoresearch").join(&run_id);
    println!("[autoresearch] log: {}", log_dir.display());
    let cfg = config;
    let mut modify = |prompt: &str| controller_modify(&cfg, prompt);
    let mut eval = || eval_metric(&cfg.eval_cmd, &cfg.workdir, cfg.eval_timeout_secs);
    run_core(&cfg, &log_dir, &mut modify, &mut eval)
}

/// Kernschleife mit injizierbarem Modify- und Eval-Schritt (testbar ohne
/// Browser/Brain, Muster wie `MockBrain`/`MockExecutor` in controller.rs).
///
/// - `modify` bekommt den gebauten Prompt, liefert eine Ein-Satz-Zusammenfassung
///   (Fehler zählen als "nicht verbessert", die Schleife läuft weiter).
/// - `eval` liefert den aktuellen Metrikwert (Fehler ⇒ `metric_after = None`).
fn run_core(
    config: &AutoResearchConfig,
    log_dir: &Path,
    modify: &mut dyn FnMut(&str) -> Result<String, String>,
    eval: &mut dyn FnMut() -> Result<f64, String>,
) -> Result<AutoResearchReport, String> {
    // Sicherheitsmodell §8: nie auf schmutzigem Working Tree starten — kein
    // Stash, kein Überschreiben von Nutzer-Zustand.
    if !git_status_clean(&config.workdir)? {
        return Err(format!(
            "Working Tree in {} ist nicht sauber — bitte committen oder stashen, \
             Autoresearch startet nur auf sauberem Stand.",
            config.workdir.display()
        ));
    }

    let branch = format!("autoresearch/{}", crate::now_run_stamp());
    git_create_branch(&config.workdir, &branch)?;

    let mut baseline = match eval() {
        Ok(v) => v,
        Err(e) => return Err(format!("eval_failed_at_start: {e}")),
    };
    println!("[autoresearch] branch={branch} baseline={baseline}");

    let mut history: Vec<IterationLog> = Vec::new();
    let mut no_improve_streak = 0usize;
    let mut stopped_reason = "max_iterations";

    for i in 1..=config.max_iterations {
        let start_sha = git_head_sha(&config.workdir)?;
        let prompt = build_modify_prompt(
            &config.goal,
            baseline,
            config.direction,
            &history,
            &config.workdir,
        );
        // Modify-Fehler (Brain-Fehler, Timeout) sind kein Abbruch, sondern
        // zählen über die anschließende Eval-Messung als "nicht verbessert".
        let brain_summary = match modify(&prompt) {
            Ok(s) => s,
            Err(e) => format!("modify_failed: {e}"),
        };

        let metric_after = eval().ok();
        let kept_metric =
            metric_after.filter(|&m| improves(m, baseline, config.direction));

        let entry = if let Some(new_metric) = kept_metric {
            let sha = git_commit_all(
                &config.workdir,
                &format!("autoresearch: iteration {i}, {baseline} -> {new_metric}"),
            )?;
            let entry = IterationLog {
                n: i,
                metric_before: baseline,
                metric_after,
                kept: true,
                commit_sha: Some(sha),
                brain_summary,
                ts: crate::now_rfc3339(),
            };
            baseline = new_metric;
            no_improve_streak = 0;
            entry
        } else {
            // Kompletter Revert dieser Iteration. `git add -A` davor sorgt
            // dafür, dass auch *neu angelegte* (untracked) Dateien vom
            // Full-Reset erfasst werden — reset --hard allein ließe sie liegen.
            git_add_all(&config.workdir)?;
            git_reset_hard(&config.workdir, &start_sha)?;
            no_improve_streak += 1;
            IterationLog {
                n: i,
                metric_before: baseline,
                metric_after,
                kept: false,
                commit_sha: None,
                brain_summary,
                ts: crate::now_rfc3339(),
            }
        };

        let after_label = entry
            .metric_after
            .map(|m| m.to_string())
            .unwrap_or_else(|| "eval-fehlgeschlagen".to_string());
        println!(
            "[autoresearch {i}/{}] metrik {} -> {after_label} ({})",
            config.max_iterations,
            entry.metric_before,
            if entry.kept { "behalten" } else { "verworfen" }
        );

        // Logging ist Telemetrie, kein Sicherheitsmechanismus — Fehler sichtbar
        // machen, aber den Lauf nicht deshalb abbrechen.
        if let Err(e) = append_log_md(&log_dir.join("log.md"), &entry) {
            eprintln!("[autoresearch] log.md nicht geschrieben: {e}");
        }
        if let Err(e) = append_iterations_jsonl(&log_dir.join("iterations.jsonl"), &entry) {
            eprintln!("[autoresearch] iterations.jsonl nicht geschrieben: {e}");
        }
        history.push(entry);

        // Circuit-Breaker-artiger früher Ausstieg (Form wie
        // circuit_breaker.rs' consecutive_failures); 0 wird als 1 behandelt.
        if no_improve_streak >= config.no_improve_abort.max(1) {
            stopped_reason = "no_improve_abort";
            break;
        }
    }

    Ok(AutoResearchReport {
        branch,
        final_metric: baseline,
        iterations: history,
        stopped_reason: stopped_reason.to_string(),
    })
}

/// `true`, wenn `new` gegenüber `baseline` in der gewünschten Richtung
/// **strikt** besser ist (gleich = nicht verbessert).
fn improves(new: f64, baseline: f64, direction: Direction) -> bool {
    match direction {
        Direction::HigherIsBetter => new > baseline,
        Direction::LowerIsBetter => new < baseline,
    }
}

// ---------------------------------------------------------------------------
// Modify-Schritt (echter Controller-Pfad, Phase 4)
// ---------------------------------------------------------------------------

/// Ein Modify-Schritt über den normalen Controller-Pfad — dadurch greift
/// `shell_policy::evaluate` für jeden Brain-Shell-Befehl wie sonst auch.
#[cfg(feature = "webview")]
fn controller_modify(config: &AutoResearchConfig, prompt: &str) -> Result<String, String> {
    use crate::browser::WebBrainBackend;
    use crate::controller::AgentController;
    use crate::executor::PlatformShellExecutor;

    let backend = WebBrainBackend::from_config(&config.brain_id)?;
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::with_data_dir(
        backend,
        executor,
        MODIFY_MAX_CYCLES,
        crate::config::data_dir(),
    );
    let meta = controller.run(prompt, &config.brain_id, None, config.headless)?;
    Ok(summarize_run(&meta.status, &meta.completed_actions))
}

#[cfg(not(feature = "webview"))]
fn controller_modify(_config: &AutoResearchConfig, _prompt: &str) -> Result<String, String> {
    Err("webview-Feature nicht aktiv — kein Brain-Backend verfügbar".to_string())
}

/// Ein-Satz-Zusammenfassung eines Controller-Runs: Status plus (falls
/// vorhanden) der Text der abschließenden message-Action. Shell-Observations
/// beginnen mit "[Terminal-Ausgabe" (siehe protocol::format_observation) und
/// werden ausgefiltert; "finish" ist der Marker der finish-Action.
fn summarize_run(status: &str, completed_actions: &HashMap<String, String>) -> String {
    let message = completed_actions
        .values()
        .filter(|v| *v != "finish" && !v.starts_with("[Terminal-Ausgabe"))
        .max_by_key(|v| v.chars().count());
    match message {
        Some(text) => format!(
            "status={status}; {}",
            crate::char_prefix(text.trim(), SUMMARY_PREVIEW_CHARS)
        ),
        None => format!("status={status}"),
    }
}

/// Prompt für den Modify-Schritt: Ziel, aktueller Metrikwert, Richtung und
/// eine Kurzfassung der letzten Iterationen, damit der Brain fehlgeschlagene
/// Ansätze nicht wiederholt (§10).
fn build_modify_prompt(
    goal: &str,
    current_metric: f64,
    direction: Direction,
    history: &[IterationLog],
    workdir: &Path,
) -> String {
    let mut p = format!(
        "Du bist der Modify-Schritt eines Autoresearch-Loops (Modify → Verify → Keep/Discard).\n\
         Arbeitsverzeichnis: {}\n\
         Ziel: {goal}\n\
         Aktueller Metrikwert: {current_metric} (Richtung: {})\n\n",
        workdir.display(),
        direction.label()
    );
    if history.is_empty() {
        p.push_str("Bisherige Iterationen: keine — dies ist die erste.\n");
    } else {
        p.push_str("Letzte Iterationen (verworfene Ansätze NICHT wiederholen):\n");
        let start = history.len().saturating_sub(PROMPT_HISTORY_LIMIT);
        for entry in &history[start..] {
            p.push_str(&format!(
                "- Iteration {}: {} — {}\n",
                entry.n,
                if entry.kept { "behalten" } else { "verworfen" },
                crate::char_prefix(&entry.brain_summary, SUMMARY_PREVIEW_CHARS)
            ));
        }
    }
    p.push_str(
        "\nAnweisungen:\n\
         - Mache GENAU EINE fokussierte Änderung, die die Metrik voraussichtlich verbessert.\n\
         - Wiederhole keine bereits fehlgeschlagenen (verworfenen) Ansätze.\n\
         - Nutze die edit/write-Actions für Dateiänderungen.\n\
         - Schließe mit einer message-Action ab, die in einem Satz beschreibt, was du geändert hast.\n",
    );
    p
}

// ---------------------------------------------------------------------------
// Eval (Vertrag §7: exit 0 + letzte stdout-Zeile ist eine f64-parsebare Zahl)
// ---------------------------------------------------------------------------

/// Führt `cmd` in `workdir` aus (Windows: `powershell.exe -NoProfile -Command`,
/// sonst `sh -c`) und parst die letzte nicht-leere stdout-Zeile als Metrik.
/// Timeout selbst gebaut über `try_wait`-Schleife (Muster wie
/// `run_command_with_timeout` in main.rs) — ein hängender Eval-Befehl darf die
/// Schleife nicht blockieren.
fn eval_metric(cmd: &str, workdir: &Path, timeout_secs: u64) -> Result<f64, String> {
    let (exit_code, stdout) = run_eval_with_timeout(cmd, workdir, timeout_secs)?;
    if exit_code != Some(0) {
        return Err(format!(
            "eval_cmd exit code {:?} (erwartet 0)",
            exit_code
        ));
    }
    parse_metric_output(&stdout)
}

/// Plattform-Kommando für den Eval-Befehl.
#[cfg(windows)]
fn eval_command(cmd: &str) -> std::process::Command {
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-Command", cmd]);
    c
}

#[cfg(not(windows))]
fn eval_command(cmd: &str) -> std::process::Command {
    let mut c = std::process::Command::new("sh");
    c.args(["-c", cmd]);
    c
}

/// Spawnt den Eval-Befehl, drainiert stdout in einem eigenen Thread (sonst
/// blockiert ein voller Pipe-Puffer das `try_wait`-Polling) und killt den
/// Prozess nach `timeout_secs`.
fn run_eval_with_timeout(
    cmd: &str,
    workdir: &Path,
    timeout_secs: u64,
) -> Result<(Option<i32>, String), String> {
    let mut child = eval_command(cmd)
        .current_dir(workdir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("eval_cmd konnte nicht gestartet werden: {e}"))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "eval_cmd: stdout-Pipe fehlt".to_string())?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = stdout.read_to_string(&mut buf);
        buf
    });

    let start = Instant::now();
    let limit = Duration::from_secs(timeout_secs.max(1));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(format!("eval_cmd timeout nach {}s", limit.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = reader.join();
                return Err(format!("eval_cmd wait fehlgeschlagen: {e}"));
            }
        }
    };
    let stdout = reader.join().unwrap_or_default();
    Ok((status.code(), stdout))
}

/// Letzte nicht-leere stdout-Zeile als endliche f64 (Vertrag §7). Alles andere
/// (leer, keine reine Zahl, NaN/inf) ist ein Fehler — die Schleife wertet das
/// als "nicht verbessert", kein Absturz.
fn parse_metric_output(stdout: &str) -> Result<f64, String> {
    let last_line = stdout
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .ok_or_else(|| "eval_cmd: leere stdout, keine Metrikzeile".to_string())?;
    let value: f64 = last_line.parse().map_err(|_| {
        format!("eval_cmd: letzte Zeile ist keine reine Zahl: {last_line:?}")
    })?;
    if !value.is_finite() {
        return Err(format!("eval_cmd: Metrik ist nicht endlich: {value}"));
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
// Git-Helfer (std::process::Command, NIE über ShellExecutor)
// ---------------------------------------------------------------------------

/// Führt git mit explizitem `workdir` aus; Err nur bei Spawn-Fehlern.
fn run_git(workdir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new("git")
        .args(args)
        .current_dir(workdir)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))
}

/// Wie `run_git`, verlangt aber Exit 0 und liefert getrimmtes stdout.
fn git_ok(workdir: &Path, args: &[&str]) -> Result<String, String> {
    let out = run_git(workdir, args)?;
    if !out.status.success() {
        return Err(format!(
            "git {} fehlgeschlagen: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// `true`, wenn `git status --porcelain` leer ist (kein uncommitted Zustand).
fn git_status_clean(workdir: &Path) -> Result<bool, String> {
    Ok(git_ok(workdir, &["status", "--porcelain"])?.is_empty())
}

/// SHA von HEAD.
fn git_head_sha(workdir: &Path) -> Result<String, String> {
    git_ok(workdir, &["rev-parse", "HEAD"])
}

/// Repo-Root des Verzeichnisses (`git rev-parse --show-toplevel`).
pub fn git_repo_root(start: &Path) -> Result<PathBuf, String> {
    git_ok(start, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

/// Neuen Branch anlegen und auschecken.
fn git_create_branch(workdir: &Path, name: &str) -> Result<(), String> {
    git_ok(workdir, &["checkout", "-b", name]).map(|_| ())
}

/// Alle Änderungen (inkl. neuer/gelöschter Dateien) stagen.
fn git_add_all(workdir: &Path) -> Result<(), String> {
    git_ok(workdir, &["add", "-A"]).map(|_| ())
}

/// Alle Änderungen committen, Commit-SHA zurückgeben. `--allow-empty`, weil
/// eine "verbesserte" Metrik ohne Dateiänderung (z.B. Flakiness) den Lauf
/// nicht abbrechen soll — der Commit hält die Loop-Invariante "jede behaltene
/// Iteration ist ein Commit" trotzdem ein.
fn git_commit_all(workdir: &Path, message: &str) -> Result<String, String> {
    git_add_all(workdir)?;
    git_ok(workdir, &["commit", "--allow-empty", "-m", message])?;
    git_head_sha(workdir)
}

/// Full-Reset auf `sha` (Revert einer verworfenen Iteration, §4/§8).
fn git_reset_hard(workdir: &Path, sha: &str) -> Result<(), String> {
    git_ok(workdir, &["reset", "--hard", sha]).map(|_| ())
}

// ---------------------------------------------------------------------------
// Logging (§9: log.md menschenlesbar + iterations.jsonl maschinenlesbar)
// ---------------------------------------------------------------------------

/// Hängt einen menschenlesbaren Eintrag an `log.md` an (Karpathys log.md-Muster).
fn append_log_md(path: &Path, entry: &IterationLog) -> Result<(), String> {
    let after = entry
        .metric_after
        .map(|m| m.to_string())
        .unwrap_or_else(|| "eval-fehlgeschlagen".to_string());
    let verdict = if entry.kept {
        format!(
            "behalten (commit {})",
            entry.commit_sha.as_deref().unwrap_or("?")
        )
    } else {
        "verworfen".to_string()
    };
    let block = format!(
        "## Iteration {} — {}\n- metrik: {} -> {}\n- ergebnis: {}\n- brain: {}\n\n",
        entry.n,
        entry.ts,
        entry.metric_before,
        after,
        verdict,
        crate::char_prefix(&entry.brain_summary, SUMMARY_PREVIEW_CHARS)
    );
    append_to_file(path, &block)
}

/// Hängt den Eintrag als JSON-Zeile an `iterations.jsonl` an (gleiche
/// JSON-Lines-Konvention wie memory.rs/brain_score.rs/circuit_breaker.rs).
fn append_iterations_jsonl(path: &Path, entry: &IterationLog) -> Result<(), String> {
    let line = serde_json::to_string(entry)
        .map_err(|e| format!("IterationLog nicht serialisierbar: {e}"))?;
    append_to_file(path, &format!("{line}\n"))
}

fn append_to_file(path: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(text.as_bytes())
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Eindeutiges Verzeichnis pro Testaufruf (Muster wie `unique_data_dir`
    /// in controller.rs — Tests laufen parallel).
    fn unique_dir(tag: &str) -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "test_autoresearch_{tag}_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Temp-Repo mit Identität und einem Initial-Commit (kein Live-Repo nötig).
    fn init_repo() -> PathBuf {
        let dir = unique_dir("repo");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "autoresearch@test.local"],
            vec!["config", "user.name", "Autoresearch Test"],
            vec!["config", "commit.gpgsign", "false"],
            // Sonst schreibt ein Windows-git mit autocrlf=true beim Checkout
            // CRLF zurück und die Inhalts-Asserts vergleichen Zeilenenden.
            vec!["config", "core.autocrlf", "false"],
        ] {
            let out = run_git(&dir, &args).unwrap();
            assert!(out.status.success(), "git {args:?} failed");
        }
        std::fs::write(dir.join("README.md"), "initial\n").unwrap();
        git_ok(&dir, &["add", "-A"]).unwrap();
        git_ok(&dir, &["commit", "-m", "initial"]).unwrap();
        dir
    }

    fn base_config(workdir: PathBuf) -> AutoResearchConfig {
        AutoResearchConfig {
            brain_id: "mock".to_string(),
            goal: "Metrik verbessern".to_string(),
            eval_cmd: "unused-in-core-tests".to_string(),
            direction: Direction::HigherIsBetter,
            max_iterations: 3,
            no_improve_abort: 3,
            headless: true,
            workdir,
            eval_timeout_secs: 30,
        }
    }

    /// Skriptbarer Eval-Schritt: liefert die Werte der Reihe nach.
    fn scripted_eval(
        values: Vec<Result<f64, String>>,
    ) -> impl FnMut() -> Result<f64, String> {
        let mut iter = values.into_iter();
        move || iter.next().unwrap_or_else(|| Err("eval-skript erschöpft".into()))
    }

    // ---- Direction / Parsing ----

    #[test]
    fn direction_from_str() {
        assert_eq!("higher".parse::<Direction>(), Ok(Direction::HigherIsBetter));
        assert_eq!("LOWER".parse::<Direction>(), Ok(Direction::LowerIsBetter));
        assert!("sideways".parse::<Direction>().is_err());
    }

    #[test]
    fn improves_respects_direction_strictly() {
        assert!(improves(2.0, 1.0, Direction::HigherIsBetter));
        assert!(!improves(1.0, 1.0, Direction::HigherIsBetter));
        assert!(!improves(0.5, 1.0, Direction::HigherIsBetter));
        assert!(improves(0.5, 1.0, Direction::LowerIsBetter));
        assert!(!improves(1.0, 1.0, Direction::LowerIsBetter));
    }

    #[test]
    fn parse_metric_takes_last_nonempty_line() {
        assert_eq!(parse_metric_output("42\n").unwrap(), 42.0);
        assert_eq!(parse_metric_output("bla\n3.5\n\n").unwrap(), 3.5);
        assert_eq!(parse_metric_output("-7").unwrap(), -7.0);
    }

    #[test]
    fn parse_metric_rejects_garbage() {
        assert!(parse_metric_output("").is_err());
        assert!(parse_metric_output("keine zahl").is_err());
        assert!(parse_metric_output("3.5 tests passed").is_err());
        assert!(parse_metric_output("NaN").is_err());
        assert!(parse_metric_output("inf").is_err());
    }

    // ---- eval_metric gegen echte Prozesse ----

    fn echo_cmd(value: &str) -> String {
        if cfg!(windows) {
            format!("Write-Output {value}")
        } else {
            format!("echo {value}")
        }
    }

    #[test]
    fn eval_metric_reads_number_from_process() {
        let dir = unique_dir("eval");
        let v = eval_metric(&echo_cmd("42"), &dir, 60).unwrap();
        assert_eq!(v, 42.0);
    }

    #[test]
    fn eval_metric_rejects_nonzero_exit() {
        let dir = unique_dir("eval");
        let cmd = if cfg!(windows) {
            "Write-Output 5; exit 3"
        } else {
            "echo 5; exit 3"
        };
        let err = eval_metric(cmd, &dir, 60).unwrap_err();
        assert!(err.contains("exit code"), "err={err}");
    }

    #[test]
    fn eval_metric_times_out() {
        let dir = unique_dir("eval");
        let cmd = if cfg!(windows) {
            "Start-Sleep -Seconds 60"
        } else {
            "sleep 60"
        };
        let err = eval_metric(cmd, &dir, 1).unwrap_err();
        assert!(err.contains("timeout"), "err={err}");
    }

    // ---- Git-Helfer gegen Tempdir-Repo ----

    #[test]
    fn git_status_clean_detects_dirty_tree() {
        let repo = init_repo();
        assert!(git_status_clean(&repo).unwrap());
        std::fs::write(repo.join("dirty.txt"), "x").unwrap();
        assert!(!git_status_clean(&repo).unwrap());
    }

    #[test]
    fn git_head_sha_looks_like_a_sha() {
        let repo = init_repo();
        let sha = git_head_sha(&repo).unwrap();
        assert_eq!(sha.len(), 40, "sha={sha}");
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn git_create_branch_switches_branch() {
        let repo = init_repo();
        git_create_branch(&repo, "autoresearch/test").unwrap();
        let branch = git_ok(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(branch, "autoresearch/test");
    }

    #[test]
    fn git_commit_all_commits_new_files() {
        let repo = init_repo();
        let before = git_head_sha(&repo).unwrap();
        std::fs::write(repo.join("neu.txt"), "inhalt").unwrap();
        let sha = git_commit_all(&repo, "autoresearch: test").unwrap();
        assert_ne!(sha, before);
        assert!(git_status_clean(&repo).unwrap());
    }

    #[test]
    fn git_reset_hard_with_add_reverts_new_and_changed_files() {
        let repo = init_repo();
        let start = git_head_sha(&repo).unwrap();
        // Bestehende Datei ändern UND neue (untracked) Datei anlegen.
        std::fs::write(repo.join("README.md"), "geändert\n").unwrap();
        std::fs::write(repo.join("neu.txt"), "neu").unwrap();
        git_add_all(&repo).unwrap();
        git_reset_hard(&repo, &start).unwrap();
        assert!(git_status_clean(&repo).unwrap());
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "initial\n"
        );
        assert!(!repo.join("neu.txt").exists(), "neue Datei muss weg sein");
    }

    // ---- Prompt / Zusammenfassung / Logging ----

    #[test]
    fn modify_prompt_contains_goal_metric_and_only_last_three() {
        let history: Vec<IterationLog> = (1..=5)
            .map(|n| IterationLog {
                n,
                metric_before: n as f64,
                metric_after: Some(n as f64),
                kept: n % 2 == 0,
                commit_sha: None,
                brain_summary: format!("versuch-{n}"),
                ts: crate::now_rfc3339(),
            })
            .collect();
        let p = build_modify_prompt(
            "Warnings reduzieren",
            7.5,
            Direction::LowerIsBetter,
            &history,
            Path::new("/tmp/repo"),
        );
        assert!(p.contains("Warnings reduzieren"));
        assert!(p.contains("7.5"));
        assert!(p.contains("niedriger ist besser"));
        assert!(p.contains("GENAU EINE"));
        assert!(p.contains("edit/write"));
        // Nur die letzten 3 Iterationen (3, 4, 5) — 1 und 2 nicht.
        assert!(p.contains("versuch-3") && p.contains("versuch-5"));
        assert!(!p.contains("versuch-1") && !p.contains("versuch-2"));
    }

    #[test]
    fn modify_prompt_without_history_says_first_iteration() {
        let p = build_modify_prompt(
            "Ziel",
            1.0,
            Direction::HigherIsBetter,
            &[],
            Path::new("."),
        );
        assert!(p.contains("erste"));
        assert!(p.contains("höher ist besser"));
    }

    #[test]
    fn summarize_run_prefers_message_text_over_observations() {
        let mut actions = HashMap::new();
        actions.insert("done-1".to_string(), "finish".to_string());
        actions.insert(
            "sh-1".to_string(),
            "[Terminal-Ausgabe action_id=sh-1]\nout".to_string(),
        );
        actions.insert(
            "msg-1".to_string(),
            "Habe den Timeout erhöht.".to_string(),
        );
        let s = summarize_run("done", &actions);
        assert!(s.contains("status=done"));
        assert!(s.contains("Habe den Timeout erhöht."));
        assert!(!s.contains("Terminal-Ausgabe"));

        let empty = summarize_run("max_cycles", &HashMap::new());
        assert_eq!(empty, "status=max_cycles");
    }

    #[test]
    fn append_logs_write_md_and_parseable_jsonl() {
        let dir = unique_dir("logs");
        let entry = IterationLog {
            n: 1,
            metric_before: 1.0,
            metric_after: Some(2.0),
            kept: true,
            commit_sha: Some("abc123".to_string()),
            brain_summary: "test".to_string(),
            ts: crate::now_rfc3339(),
        };
        let md = dir.join("log.md");
        let jsonl = dir.join("iterations.jsonl");
        append_log_md(&md, &entry).unwrap();
        append_iterations_jsonl(&jsonl, &entry).unwrap();
        let mut failed = entry.clone();
        failed.n = 2;
        failed.metric_after = None;
        failed.kept = false;
        failed.commit_sha = None;
        append_log_md(&md, &failed).unwrap();
        append_iterations_jsonl(&jsonl, &failed).unwrap();

        let md_text = std::fs::read_to_string(&md).unwrap();
        assert!(md_text.contains("Iteration 1"));
        assert!(md_text.contains("behalten (commit abc123)"));
        assert!(md_text.contains("eval-fehlgeschlagen"));

        let lines: Vec<IterationLog> = std::fs::read_to_string(&jsonl)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].n, 1);
        assert_eq!(lines[1].metric_after, None);
    }

    // ---- Kernschleife (gemockter Modify + gemockter Eval) ----

    #[test]
    fn core_loop_keeps_improvement_as_commit() {
        let repo = init_repo();
        let start_sha = git_head_sha(&repo).unwrap();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 1;

        let repo_for_modify = repo.clone();
        let mut modify = |_prompt: &str| {
            std::fs::write(repo_for_modify.join("fix.txt"), "besser").unwrap();
            Ok("fix.txt angelegt".to_string())
        };
        // Baseline 1.0, danach 2.0 → verbessert.
        let mut eval = scripted_eval(vec![Ok(1.0), Ok(2.0)]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        assert_eq!(report.final_metric, 2.0);
        assert_eq!(report.stopped_reason, "max_iterations");
        assert_eq!(report.iterations.len(), 1);
        let it = &report.iterations[0];
        assert!(it.kept);
        assert_eq!(it.metric_after, Some(2.0));
        let sha = it.commit_sha.clone().expect("commit sha");
        assert_ne!(sha, start_sha);
        assert_eq!(git_head_sha(&repo).unwrap(), sha);
        assert!(git_status_clean(&repo).unwrap());
        assert!(repo.join("fix.txt").exists());
        assert!(report.branch.starts_with("autoresearch/"));
    }

    #[test]
    fn core_loop_reverts_non_improving_iteration_completely() {
        let repo = init_repo();
        let start_sha = git_head_sha(&repo).unwrap();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 1;

        let repo_for_modify = repo.clone();
        let mut modify = |_prompt: &str| {
            // Neue Datei UND Änderung an bestehender Datei — beides muss weg.
            std::fs::write(repo_for_modify.join("junk.txt"), "junk").unwrap();
            std::fs::write(repo_for_modify.join("README.md"), "kaputt\n").unwrap();
            Ok("junk produziert".to_string())
        };
        let mut eval = scripted_eval(vec![Ok(5.0), Ok(5.0)]); // gleich = nicht besser

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        let it = &report.iterations[0];
        assert!(!it.kept);
        assert_eq!(it.commit_sha, None);
        assert_eq!(report.final_metric, 5.0);
        assert_eq!(git_head_sha(&repo).unwrap(), start_sha);
        assert!(!repo.join("junk.txt").exists(), "Revert muss neue Dateien entfernen");
        assert_eq!(
            std::fs::read_to_string(repo.join("README.md")).unwrap(),
            "initial\n"
        );
    }

    #[test]
    fn core_loop_eval_error_counts_as_not_improved() {
        let repo = init_repo();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 1;
        let mut modify = |_prompt: &str| Ok("noop".to_string());
        let mut eval = scripted_eval(vec![Ok(1.0), Err("kaputt".to_string())]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        let it = &report.iterations[0];
        assert!(!it.kept);
        assert_eq!(it.metric_after, None);
        assert_eq!(report.final_metric, 1.0);
    }

    #[test]
    fn core_loop_modify_error_does_not_crash_loop() {
        let repo = init_repo();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 1;
        let mut modify = |_prompt: &str| Err("brain down".to_string());
        let mut eval = scripted_eval(vec![Ok(1.0), Ok(1.0)]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        assert!(report.iterations[0].brain_summary.contains("modify_failed"));
        assert!(!report.iterations[0].kept);
    }

    #[test]
    fn core_loop_aborts_after_no_improve_streak() {
        let repo = init_repo();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 10;
        cfg.no_improve_abort = 2;
        let mut modify = |_prompt: &str| Ok("noop".to_string());
        let mut eval = scripted_eval(vec![Ok(1.0), Ok(1.0), Ok(0.5), Ok(9.9)]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        assert_eq!(report.stopped_reason, "no_improve_abort");
        assert_eq!(report.iterations.len(), 2, "nach 2 Fehlschlägen Schluss");
        assert_eq!(report.final_metric, 1.0);
    }

    #[test]
    fn core_loop_streak_resets_after_improvement() {
        let repo = init_repo();
        let mut cfg = base_config(repo.clone());
        cfg.max_iterations = 3;
        cfg.no_improve_abort = 2;
        let mut modify = |_prompt: &str| Ok("noop".to_string());
        // fail, improve, fail → Streak wird durch die Verbesserung zurückgesetzt,
        // kein no_improve_abort.
        let mut eval = scripted_eval(vec![Ok(1.0), Ok(0.5), Ok(2.0), Ok(1.5)]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        assert_eq!(report.stopped_reason, "max_iterations");
        assert_eq!(report.iterations.len(), 3);
        assert_eq!(report.final_metric, 2.0);
    }

    #[test]
    fn core_loop_lower_is_better() {
        let repo = init_repo();
        let mut cfg = base_config(repo.clone());
        cfg.direction = Direction::LowerIsBetter;
        cfg.max_iterations = 1;
        let mut modify = |_prompt: &str| Ok("weniger warnings".to_string());
        let mut eval = scripted_eval(vec![Ok(10.0), Ok(4.0)]);

        let report = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap();
        assert!(report.iterations[0].kept);
        assert_eq!(report.final_metric, 4.0);
    }

    #[test]
    fn core_loop_refuses_dirty_working_tree() {
        let repo = init_repo();
        std::fs::write(repo.join("uncommitted.txt"), "x").unwrap();
        let cfg = base_config(repo);
        let mut modify = |_prompt: &str| Ok("nie erreicht".to_string());
        let mut eval = scripted_eval(vec![Ok(1.0)]);

        let err = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap_err();
        assert!(err.contains("nicht sauber"), "err={err}");
    }

    #[test]
    fn core_loop_baseline_eval_failure_aborts_hard() {
        let repo = init_repo();
        let cfg = base_config(repo);
        let mut modify = |_prompt: &str| Ok("nie erreicht".to_string());
        let mut eval = scripted_eval(vec![Err("baseline kaputt".to_string())]);

        let err = run_core(&cfg, &unique_dir("log"), &mut modify, &mut eval).unwrap_err();
        assert!(err.starts_with("eval_failed_at_start"), "err={err}");
    }

    #[test]
    fn core_loop_writes_both_log_files() {
        let repo = init_repo();
        let log_dir = unique_dir("logdir").join("run");
        let mut cfg = base_config(repo);
        cfg.max_iterations = 1;
        let mut modify = |_prompt: &str| Ok("noop".to_string());
        let mut eval = scripted_eval(vec![Ok(1.0), Ok(2.0)]);

        run_core(&cfg, &log_dir, &mut modify, &mut eval).unwrap();
        assert!(log_dir.join("log.md").exists());
        let jsonl = std::fs::read_to_string(log_dir.join("iterations.jsonl")).unwrap();
        let parsed: IterationLog = serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
        assert_eq!(parsed.n, 1);
        assert!(parsed.kept);
    }
}
