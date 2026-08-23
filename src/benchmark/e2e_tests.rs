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

/// Diese Laeufe teilen sich Prozess-globalen Zustand: den Statistik-Store der
/// Rangliste, das Ablageverzeichnis der Erntekandidaten und den Ereignisbus.
/// Parallel gefahren gewinnt mal der eine, mal der andere — beobachtet als
/// sporadisch roter `e2e_lauf_in_scope_wird_geerntet` im Headless-Gesamtlauf,
/// waehrend er allein immer gruen war. Ein flackernder Abnahmetest ist
/// schlimmer als keiner, also laufen sie nacheinander.
static SERIELL: Mutex<()> = Mutex::new(());

/// Nimmt die Sperre und uebersteht eine Vergiftung durch einen vorherigen
/// Panic — sonst reisst der erste Fehlschlag alle uebrigen Tests mit.
fn seriell() -> std::sync::MutexGuard<'static, ()> {
    SERIELL.lock().unwrap_or_else(|e| e.into_inner())
}

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

/// Erkundung: protokolliert die ERSTE Zeile jeder Anweisung, damit die
/// Antwortheuristik an stabilen Anweisungstexten haengt statt an Inhalten.
#[test]
#[ignore = "Erkundungslauf, kein Abnahmekriterium"]
fn zeigt_die_prompts_der_pipeline() {
    let _seriell = seriell();
    let root = repo("prompts");
    let log: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let runner = SkriptRunner {
        ziel: "src/config.rs".into(),
        inhalt: IN_SCOPE_PATCH.into(),
    };
    let mut cfg = config(&root);
    cfg.brains = vec!["a".into(), "b".into()];
    let _ = run_benchmark_with(
        &cfg,
        |brain: &str, prompt: &str| {
            // Letzte nicht leere Zeile: dort steht bei allen Phasen die
            // eigentliche Anweisung.
            let letzte = prompt
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            log.lock()
                .unwrap()
                .push(format!("[{brain}] …{}", letzte.trim()));
            Ok(antwort(prompt))
        },
        &runner,
    );
    for zeile in log.lock().unwrap().iter() {
        println!("{zeile}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// Antwortet auf die Phasen der Pipeline, ohne ein Brain zu befragen.
///
/// Erkannt wird an Prompt-Merkmalen, nicht an Reihenfolge: die Pipeline darf
/// Phasen umstellen, ohne dass dieser Doppelgaenger stillschweigend falsche
/// Antworten liefert.
fn antwort(prompt: &str) -> String {
    let p = prompt.to_lowercase();
    // Erkannt wird am ANWEISUNGSTEXT der jeweiligen Phase, nie am Inhalt: der
    // Katalog der Abstimmphase enthaelt die eigenen Vorschlaege wieder, ein
    // Treffer auf Inhaltswoerter beantwortet dann die falsche Frage (gemessen
    // 2026-08-23: "Zieldatei" im Katalog lenkte die Stimmabgabe auf den
    // Auftragstext, die Runde endete mit null Stimmen).
    if p.contains("workpackage") {
        return WORK_PACKAGE_JSON.to_string();
    }
    if p.contains("antworte nur mit den nummern") {
        return "1".to_string();
    }
    if p.contains("fasse duplikate zusammen") {
        return format!("1. {VORSCHLAG}");
    }
    if p.contains("zuschnitt (wichtig)") {
        return format!("1. {VORSCHLAG}");
    }
    // Planung und Verfeinerung: der Auftrag selbst ist ein gueltiger Entwurf.
    AUFTRAG.to_string()
}

const WORK_PACKAGE_JSON: &str = r#"{"id":"e2e-1","objective":"wird von der Pipeline gesetzt",
"allowed_paths":["src/config.rs"],
"anchors":[{"symbol":"bestehend","path":"src/config.rs","requirement":"must_exist"}],
"acceptance":[{"command":"git --version","purpose":"Regressionen"}]}"#;

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
    let _seriell = seriell();
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
    let _seriell = seriell();
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
    let _seriell = seriell();
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

/// Zeichnet auf, WIE die Pipeline jeden Lauf startet, und laesst das erste
/// Brain absichtlich nichts tun.
struct ProtokollRunner {
    /// (brain, Start-Art, run_id bei Continuation)
    protokoll: Mutex<Vec<(String, String, String)>>,
    /// Dieses Brain aendert nie etwas und provoziert damit Stall + Handoff.
    stiller: String,
}

impl PhaseBRunner for ProtokollRunner {
    fn run(&self, request: BenchRunRequest<'_>) -> Result<BenchRunOutcome, String> {
        let (art, id) = match request.start {
            super::pipeline::BenchRunStart::Fresh => ("fresh".to_string(), String::new()),
            super::pipeline::BenchRunStart::Continuation(r) => {
                ("continuation".to_string(), r.to_string())
            }
            super::pipeline::BenchRunStart::CrossBrain(_) => {
                ("cross_brain".to_string(), String::new())
            }
        };
        self.protokoll
            .lock()
            .unwrap()
            .push((request.brain_id.to_string(), art, id));

        if request.brain_id == self.stiller {
            // Nichts schreiben: die Pipeline sieht keine Aenderung.
            return Ok((
                "done".to_string(),
                1,
                format!("run-{}", request.brain_id),
                0,
            ));
        }
        std::fs::write(request.workdir.join("src/config.rs"), IN_SCOPE_PATCH)
            .map_err(|e| format!("Schreiben scheiterte: {e}"))?;
        Ok((
            "done".to_string(),
            1,
            format!("run-{}", request.brain_id),
            1,
        ))
    }
}

/// Belegt die Weitergabe: ein Brain, das nicht vorankommt, wird gestoppt und
/// die Aufgabe geht an das naechste — begrenzt durch `max_handoffs`.
#[test]
fn e2e_stall_fuehrt_zu_cross_brain_handoff() {
    let _seriell = seriell();
    let root = repo("handoff");
    let mut cfg = config(&root);
    // Reihenfolge mit Bedacht: `assign_tasks` rotiert um die Rundennummer,
    // bei round=1 zieht damit der zweite Eintrag zuerst. Das stille Brain muss
    // VOR dem bauenden laufen — sonst hat die Aufgabe schon jemand geloest,
    // wenn der Stall eintritt, und es gibt nichts weiterzureichen.
    cfg.brains = vec!["baumeister".into(), "stiller".into()];
    cfg.max_iterations = 2;
    cfg.stall_limit = 1;
    cfg.max_handoffs = 1;

    let runner = ProtokollRunner {
        protokoll: Mutex::new(Vec::new()),
        stiller: "stiller".into(),
    };
    let report = run_benchmark_with(&cfg, |_b: &str, p: &str| Ok(antwort(p)), &runner)
        .expect("Lauf muss ein Ergebnis liefern");

    let protokoll = runner.protokoll.lock().unwrap().clone();
    assert_eq!(
        protokoll.len(),
        2,
        "genau zwei Laeufe: der Stall und die Uebernahme. Ein dritter waere der          Doppelstart des Plan-Eintrags, den die Queue abfangen muss: {protokoll:?}"
    );
    assert_eq!(
        protokoll[0],
        ("stiller".to_string(), "fresh".to_string(), String::new()),
        "das stille Brain startet frisch und faellt aus: {protokoll:?}"
    );
    assert_eq!(
        protokoll[1].0, "baumeister",
        "die Aufgabe muss beim zweiten Brain ankommen: {protokoll:?}"
    );
    assert_eq!(
        protokoll[1].1, "cross_brain",
        "die Uebernahme muss ein Cross-Brain-Handoff sein, kein frischer Start —          sonst hat das zweite Brain die Aufgabe nur ohnehin aus dem Turnier          bekommen und es ist gar keine Weitergabe belegt: {protokoll:?}"
    );
    assert_eq!(
        report.harvested.len(),
        1,
        "das uebernehmende Brain loest die Aufgabe, also wird geerntet: {report:?}"
    );
    assert!(
        worktree_sauber(&root),
        "nach Stall, Weitergabe und Ernte muss der Baum sauber sein"
    );

    println!("PROTOKOLL: {protokoll:?}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Runner, der in der ERSTEN Iteration nichts tut und erst in der zweiten baut.
struct ZoegerlicherRunner {
    protokoll: Mutex<Vec<(String, String)>>,
    laeufe: Mutex<u32>,
}

impl PhaseBRunner for ZoegerlicherRunner {
    fn run(&self, request: BenchRunRequest<'_>) -> Result<BenchRunOutcome, String> {
        let art = match request.start {
            super::pipeline::BenchRunStart::Fresh => "fresh".to_string(),
            super::pipeline::BenchRunStart::Continuation(r) => format!("continuation:{r}"),
            super::pipeline::BenchRunStart::CrossBrain(_) => "cross_brain".to_string(),
        };
        self.protokoll
            .lock()
            .unwrap()
            .push((request.brain_id.to_string(), art));

        let mut n = self.laeufe.lock().unwrap();
        *n += 1;
        // Immer dieselbe Run-ID: eine fortgesetzte Agent-Session behaelt sie.
        let run_id = "run-zoegerlich".to_string();
        if *n == 1 {
            // Nichts geschrieben: die Pipeline sieht keine Aenderung und
            // stoesst dasselbe Brain erneut an — als Fortsetzung.
            return Ok(("done".to_string(), 1, run_id, 0));
        }
        std::fs::write(request.workdir.join("src/config.rs"), IN_SCOPE_PATCH)
            .map_err(|e| format!("Schreiben scheiterte: {e}"))?;
        Ok(("done".to_string(), 2, run_id, 1))
    }
}

/// Belegt Fresh → Continuation MIT GLEICHER RUN-ID: ein Brain, das in der
/// ersten Iteration nichts aendert, wird erneut angestossen — als Fortsetzung
/// derselben Sitzung, nicht als frischer Lauf.
#[test]
fn e2e_zweite_iteration_ist_continuation_derselben_run_id() {
    let _seriell = seriell();
    let root = repo("continuation");
    let mut cfg = config(&root);
    // Zwei Iterationen erlauben, aber erst beim ZWEITEN Stillstand aufgeben —
    // sonst bricht die Pipeline nach Iteration 1 ab und es gibt keine
    // Fortsetzung zu belegen.
    cfg.max_iterations = 2;
    cfg.stall_limit = 2;

    let runner = ZoegerlicherRunner {
        protokoll: Mutex::new(Vec::new()),
        laeufe: Mutex::new(0),
    };
    let report = run_benchmark_with(&cfg, |_b: &str, p: &str| Ok(antwort(p)), &runner)
        .expect("Lauf muss ein Ergebnis liefern");

    let protokoll = runner.protokoll.lock().unwrap().clone();
    assert_eq!(
        protokoll.len(),
        2,
        "genau zwei Iterationen erwartet: {protokoll:?}"
    );
    assert_eq!(
        protokoll[0].1, "fresh",
        "die erste Iteration startet frisch"
    );
    assert_eq!(
        protokoll[1].1, "continuation:run-zoegerlich",
        "die zweite Iteration muss die Sitzung der ersten FORTSETZEN und deren \
         Run-ID tragen — ein frischer Start waere ein anderer Vertrag: {protokoll:?}"
    );
    assert_eq!(
        report.harvested.len(),
        1,
        "die zweite Iteration liefert, also wird geerntet: {report:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
