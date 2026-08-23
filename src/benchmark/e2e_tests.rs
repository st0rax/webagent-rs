//! Providerfreie Systemabnahme der Benchmark-Pipeline.
//!
//! Belegt den Weg Auftrag → Bau → Gates → Ernte in EINEM Lauf, mit echtem Git
//! und echten Eval-Kommandos. Nur die Brain-Ausfuehrung ist ersetzt: Phase A
//! ueber `query`, Phase B ueber [`PhaseBRunner`]. Kein Browser, kein Provider.

use super::pipeline::{run_benchmark_with, BenchRunOutcome, BenchRunRequest, PhaseBRunner};
use super::types::BenchmarkConfig;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git startet");
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Mini-Repo mit zwei Quelldateien und sauberer Baseline.
fn repo(stamp: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("webagent-e2e-{stamp}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(
        root.join("src/config.rs"),
        "pub fn bestehend() -> u8 {\n    1\n}\n",
    )
    .expect("config.rs");
    std::fs::write(
        root.join("src/fremd.rs"),
        "pub fn woanders() -> u8 {\n    1\n}\n",
    )
    .expect("fremd.rs");
    git(&root, &["init", "--quiet"]);
    git(&root, &["config", "user.email", "e2e@example.invalid"]);
    git(&root, &["config", "user.name", "E2E"]);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "--quiet", "-m", "baseline"]);
    root
}

/// Phase-B-Doppelgaenger: schreibt eine Datei, statt ein Brain zu fahren.
struct SkriptRunner {
    ziel: String,
    inhalt: String,
}

impl PhaseBRunner for SkriptRunner {
    fn run(&self, request: BenchRunRequest<'_>) -> Result<BenchRunOutcome, String> {
        std::fs::write(request.workdir.join(&self.ziel), &self.inhalt)
            .map_err(|e| format!("Schreiben scheiterte: {e}"))?;
        Ok(("done".to_string(), 1, "e2e-run".to_string(), 1))
    }
}

const TEST_EVAL: &str =
    "echo \"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\"";

fn config(root: &Path) -> BenchmarkConfig {
    BenchmarkConfig {
        brains: vec!["testbrain".into()],
        rounds: 1,
        suggestions: 1,
        // Echte Kommandos, aber ohne Cargo: das Mini-Repo hat keine Crate.
        // Das Testgate braucht eine parsebare Testzahl, also die uebliche
        // cargo-Ergebniszeile. Doppelte Quotes halten die Semikolons in
        // PowerShell wie in `sh` zusammen.
        build_eval: "git --version".into(),
        test_eval: TEST_EVAL.into(),
        workdir: root.to_path_buf(),
        headless: true,
        max_iterations: 1,
        harvest: true,
        verbose: false,
        parallel: 1,
        stall_limit: 1,
        max_handoffs: 0,
        lint_eval: String::new(),
        vetoes: vec![],
        loop_forever: false,
        work_package: None,
    }
}

/// Erkundung: haelt jeden Prompt fest, den die Pipeline stellt.
#[test]
#[ignore = "Erkundungslauf, kein Abnahmekriterium"]
fn zeigt_die_prompts_der_pipeline() {
    let root = repo("prompts");
    let log: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let runner = SkriptRunner {
        ziel: "src/config.rs".into(),
        inhalt: "pub fn bestehend() -> u8 {\n    2\n}\n".into(),
    };
    let _ = run_benchmark_with(
        &config(&root),
        |brain: &str, prompt: &str| {
            log.lock().unwrap().push(format!(
                "\n========== an {brain} ==========\n{}",
                prompt.chars().take(700).collect::<String>()
            ));
            Err("Erkundung: keine Antwort".to_string())
        },
        &runner,
    );
    let gesammelt = log.lock().unwrap().join("\n");
    std::fs::write(std::env::temp_dir().join("e2e-prompts.txt"), &gesammelt).ok();
    println!("{gesammelt}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Antwortet auf die Phasen der Pipeline, ohne ein Brain zu befragen.
///
/// Erkannt wird an Prompt-Merkmalen, nicht an Reihenfolge: die Pipeline darf
/// Phasen umstellen, ohne dass dieser Doppelgaenger stillschweigend falsche
/// Antworten liefert.
fn antwort(prompt: &str) -> String {
    let p = prompt.to_lowercase();
    if p.contains("workpackage") {
        return r#"{"id":"e2e-1","objective":"wird von der Pipeline gesetzt",
"allowed_paths":["src/config.rs"],
"anchors":[{"symbol":"bestehend","path":"src/config.rs","requirement":"must_exist"}],
"acceptance":[{"command":"git --version","purpose":"Regressionen"}]}"#
            .to_string();
    }
    if p.contains("lokale belege") || p.contains("abschlussbeleg") || p.contains("zieldatei") {
        return AUFTRAG.to_string();
    }
    // Sammelphase VOR der Stimmphase pruefen: der Sammel-Prompt bittet um eine
    // "nummerierte Liste" und enthaelt damit selbst das Wort "nummer".
    if p.contains("nenne genau") || p.contains("ein vorschlag pro zeile") {
        return format!("1. {VORSCHLAG}");
    }
    if p.contains("rangliste") || p.contains("stimme") || p.contains("nummern") {
        return "1".to_string();
    }
    // Planphase und alles Uebrige: der Auftrag selbst ist ein gueltiger Plan.
    AUFTRAG.to_string()
}

// Aendert NUR den Rumpf einer bestehenden Funktion in einer Datei, die der
// Auftrag nicht zusagt. Bewusst OHNE neue oeffentliche Funktion: sonst
// verwirft ihn schon `validate_task_scope` anhand des Aufgaben-Freitexts und
// der Nachweis gehoerte nicht mehr dem Datei-Scope (gemessen 2026-08-23 —
// der erste Anlauf scheiterte genau daran).
const OUT_OF_SCOPE_PATCH: &str = "pub fn woanders() -> u8 {
    2
}
";

const IN_SCOPE_PATCH: &str = "pub fn bestehend() -> u8 {
    verdoppelt(1)
}

pub fn verdoppelt(x: u8) -> u8 {
    x.saturating_mul(2)
}
";

const VORSCHLAG: &str =
    "Datei src/config.rs: fuege pub fn verdoppelt(x: u8) -> u8 hinzu, die den Eingabewert \
     verdoppelt und bei Ueberlauf saettigt.";

const AUFTRAG: &str = "Zieldatei: src/config.rs\n\
     Lokale Belege: bestehend existiert in src/config.rs.\n\
     Aufgabe: fuege pub fn verdoppelt(x: u8) -> u8 hinzu, die saettigend verdoppelt.\n\
     Abschlussbeleg: geaenderte Datei plus cargo test --lib.";
// `refinement_has_evidence` verlangt im Abschlussbeleg ein cargo-Kommando.
// Ausgefuehrt wird im Mini-Repo trotzdem `git --version` (siehe `config`):
// der Text beschreibt die Absicht, die Config bestimmt das reale Gate.

/// HEAD-Commit und Sauberkeit des Arbeitsbaums.
fn head(root: &Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .expect("git rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn worktree_sauber(root: &Path) -> bool {
    let out = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .expect("git status");
    out.stdout.is_empty()
}

/// Voller Lauf mit einem Patch INNERHALB des zugesagten Scopes: er wird
/// geerntet und liegt danach als Commit im Repo.
#[test]
fn e2e_lauf_in_scope_wird_geerntet() {
    let root = repo("in-scope");
    let vorher = head(&root);
    let runner = SkriptRunner {
        ziel: "src/config.rs".into(),
        inhalt: IN_SCOPE_PATCH.into(),
    };

    let report = run_benchmark_with(&config(&root), |_b: &str, p: &str| Ok(antwort(p)), &runner)
        .expect("Lauf muss ein Ergebnis liefern");

    assert_eq!(
        report.harvested.len(),
        1,
        "der bestandene Patch muss geerntet werden, Report: {report:?}"
    );
    assert_ne!(head(&root), vorher, "die Ernte muss einen Commit erzeugen");
    assert!(
        worktree_sauber(&root),
        "nach der Ernte muss der Arbeitsbaum sauber sein"
    );
    let datei = std::fs::read_to_string(root.join("src/config.rs")).expect("config.rs");
    assert!(
        datei.contains("verdoppelt"),
        "die geerntete Aenderung muss im Baum stehen: {datei}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Die geforderte Abnahme: ein technisch EINWANDFREIER Patch — er baut, die
/// Tests sind gruen — der aber eine Datei ausserhalb `allowed_paths` umbaut,
/// wird fail-closed verworfen. HEAD bleibt stehen, der Baum bleibt sauber.
#[test]
fn e2e_patch_ausserhalb_scope_wird_fail_closed_verworfen() {
    let root = repo("out-of-scope");
    let vorher = head(&root);
    // Der Auftrag sagt src/config.rs zu; gebaut wird src/fremd.rs.
    let runner = SkriptRunner {
        ziel: "src/fremd.rs".into(),
        inhalt: OUT_OF_SCOPE_PATCH.into(),
    };

    let report = run_benchmark_with(&config(&root), |_b: &str, p: &str| Ok(antwort(p)), &runner)
        .expect("Lauf muss ein Ergebnis liefern");

    assert!(
        report.harvested.is_empty(),
        "ein Patch ausserhalb des zugesagten Scopes darf NICHT geerntet werden: {report:?}"
    );
    assert_eq!(
        head(&root),
        vorher,
        "HEAD muss unveraendert bleiben, wenn nichts geerntet wurde"
    );
    assert!(
        worktree_sauber(&root),
        "der Arbeitsbaum muss nach der Verwerfung sauber sein"
    );
    let fremd = std::fs::read_to_string(root.join("src/fremd.rs")).expect("fremd.rs");
    assert!(
        !fremd.contains("    2"),
        "die verworfene Aenderung darf nicht im Baum bleiben: {fremd}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Kontrolltest — er gibt dem Test darueber erst seine Aussagekraft.
///
/// Derselbe Patch, dieselbe Aufgabe, nur OHNE typisierten Auftrag: die
/// Antwort auf die WorkPackage-Frage ist unbrauchbar, also gibt es keinen
/// Scope. Wird er jetzt geerntet, war die Verwerfung oben tatsaechlich die
/// Leistung des Scopes und nicht die irgendeiner anderen Pruefung.
///
/// Schlaegt DIESER Test fehl, ist der Nachweis oben wertlos — nicht dieser
/// hier kaputt.
#[test]
fn ohne_typisierten_auftrag_passiert_derselbe_patch() {
    let root = repo("kontrolle");
    let vorher = head(&root);
    let runner = SkriptRunner {
        ziel: "src/fremd.rs".into(),
        inhalt: OUT_OF_SCOPE_PATCH.into(),
    };

    let report = run_benchmark_with(
        &config(&root),
        |_b: &str, p: &str| {
            if p.to_lowercase().contains("workpackage") {
                // Kein JSON: `parse_work_package` verwirft, der Lauf geht
                // ohne Scope weiter.
                return Ok("Dazu kann ich nichts sagen.".to_string());
            }
            Ok(antwort(p))
        },
        &runner,
    )
    .expect("Lauf muss ein Ergebnis liefern");

    assert_eq!(
        report.harvested.len(),
        1,
        "ohne Scope muss derselbe Patch durchgehen — sonst belegt der \
         Scope-Test nichts: {report:?}"
    );
    assert_ne!(head(&root), vorher, "ohne Scope wird geerntet");

    let _ = std::fs::remove_dir_all(&root);
}
