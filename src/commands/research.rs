//! Befehle rund um Selbstverbesserung und Messung: autoresearch, Swarm-Vote,
//! Design-Vote, Benchmark.

use crate::cli::AutoresearchArgs;

pub fn cmd_autoresearch(args: AutoresearchArgs) -> i32 {
    use webagent::autoresearch::{self, AutoResearchConfig, Direction};

    let direction: Direction = match args.direction.parse() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[autoresearch] {e}");
            return 2;
        }
    };
    let workdir = match args.workdir {
        Some(p) => std::path::PathBuf::from(p),
        None => match autoresearch::resolve_project_root() {
            Ok(root) => root,
            Err(e) => {
                eprintln!("[autoresearch] {e}");
                return 2;
            }
        },
    };

    let config = AutoResearchConfig {
        brain_id: args.brain,
        goal: args.goal,
        eval_cmd: args.eval,
        direction,
        max_iterations: args.max_iterations,
        no_improve_abort: args.no_improve_abort,
        headless: args.headless,
        workdir,
        eval_timeout_secs: args.eval_timeout,
    };

    match autoresearch::run(config) {
        Ok(report) => {
            let kept = report.iterations.iter().filter(|i| i.kept).count();
            println!(
                "[autoresearch] fertig: branch={} stop={} iterationen={} behalten={} final_metric={}",
                report.branch,
                report.stopped_reason,
                report.iterations.len(),
                kept,
                report.final_metric
            );
            println!(
                "[autoresearch] Ergebnis liegt auf dem Branch {} — Merge bleibt manuell.",
                report.branch
            );
            0
        }
        Err(e) => {
            eprintln!("[autoresearch] Fehler: {e}");
            1
        }
    }
}

/// `webagent autoresearch-self` — dieselbe Kernfunktion wie die REPL, nur ohne
/// gehaltene Session: isolierte Profile vorbereiten, Fakten sammeln (oder aus
/// `--facts` laden), vier Phasen fahren, Ergebnis als Wiki-Seite ablegen.
pub fn cmd_autoresearch_self(
    suggestions: usize,
    top: usize,
    headless: bool,
    facts_file: Option<String>,
) -> i32 {
    let targets = webagent::config::available_brain_ids();
    if targets.is_empty() {
        eprintln!("[self-research] keine Brains registriert.");
        return 2;
    }

    let run_id = webagent::now_run_stamp();
    let profiles: Vec<(String, std::path::PathBuf)> = targets
        .iter()
        .map(|tb| {
            (
                tb.clone(),
                webagent::config::prepare_swarm_profile(&run_id, tb),
            )
        })
        .collect();
    let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
        profiles
            .iter()
            .find(|(b, _)| b == brain)
            .map(|(_, p)| p.clone())
    };

    // Fakten: --facts-Datei überschreibt, sonst aus dem Repo-Root sammeln.
    let facts = match facts_file {
        Some(path) => std::fs::read_to_string(&path).unwrap_or_else(|e| {
            eprintln!("[self-research] --facts {path} nicht lesbar ({e}) — nutze leere Fakten.");
            String::new()
        }),
        None => {
            let root = webagent::autoresearch::resolve_project_root()
                .unwrap_or_else(|_| std::env::temp_dir());
            webagent::self_research::gather_facts(&root, 1200)
        }
    };

    let report = webagent::self_research::run_self_research(
        &targets,
        &facts,
        suggestions,
        top,
        4,
        |b, p| webagent::repl::isolated_query(b, p, headless, profile_of(b)),
    );

    let _ = webagent::config::cleanup_swarm_profiles(&run_id);

    if !report.catalog.is_empty() {
        let wiki = webagent::wiki_memory::WikiMemory::new(
            webagent::config::data_dir().join("memory").join("wiki"),
        );
        let title = format!("self-research-{run_id}");
        let body = webagent::self_research::format_report(&report);
        match wiki.write_page(&title, &body) {
            Ok(slug) => println!("[self-research] Ergebnis abgelegt als [[{slug}]]."),
            Err(e) => eprintln!("[self-research] Wiki-Ablage fehlgeschlagen: {e}"),
        }
    }

    if report.ranked.is_empty() {
        1
    } else {
        0
    }
}

/// `webagent design-vote` — Swarm entwirft ein TUI-Design, scheidet im
/// kick-vote aus, der Gewinner wird umgesetzt. Isolierte Profile wie beim
/// Benchmark; die Ausscheidungslogik steckt in `design_vote`/`knockout`.
pub fn cmd_design_vote(
    brains: String,
    topic: String,
    context: String,
    implement_brain: String,
    headless: bool,
) -> i32 {
    let targets: Vec<String> = if brains.trim().is_empty() {
        webagent::config::available_brain_ids()
    } else {
        brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    if targets.len() < 2 {
        eprintln!(
            "[design-vote] mindestens 2 Brains noetig (haben {}).",
            targets.len()
        );
        return 2;
    }

    let run_id = webagent::now_run_stamp();
    let profiles: Vec<(String, std::path::PathBuf)> = targets
        .iter()
        .map(|tb| {
            (
                tb.clone(),
                webagent::config::prepare_swarm_profile(&run_id, tb),
            )
        })
        .collect();
    let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
        profiles
            .iter()
            .find(|(b, _)| b == brain)
            .map(|(_, p)| p.clone())
    };

    let config = webagent::design_vote::DesignVoteConfig {
        brains: targets,
        topic: topic.clone(),
        context,
        mode: webagent::design_vote::VoteMode::Design,
    };

    let report = webagent::design_vote::run_design_vote(
        &config,
        &|msg| println!("[design-vote] {msg}"),
        |b, p| webagent::repl::isolated_query(b, p, headless, profile_of(b)),
    );

    let _ = webagent::config::cleanup_swarm_profiles(&run_id);

    println!("\n[design-vote] === Ergebnis ===");
    println!(
        "[design-vote] {} Entwuerfe gesammelt.",
        report.proposals.len()
    );
    let Some((author, design)) = report.winning_design() else {
        eprintln!("[design-vote] Kein Gewinner (zu wenige verwertbare Entwuerfe).");
        return 1;
    };
    println!("[design-vote] Gewinner: Entwurf von {author}\n");
    println!("{design}\n");

    let Some(approved) = report.approved_text() else {
        eprintln!("[design-vote] Kein einstimmig ratifizierter Endentwurf — keine automatische Umsetzung.");
        return 1;
    };

    if implement_brain.trim().is_empty() {
        println!("[design-vote] Kein --implement-brain gesetzt — nur abgestimmt.");
        return 0;
    }

    // Gewinner umsetzen — über denselben Controller-Pfad wie der Benchmark.
    println!("[design-vote] {implement_brain} setzt den Gewinner um …");
    let task = webagent::design_vote::build_implement_prompt(approved);
    let impl_profile = webagent::config::prepare_swarm_profile(&run_id, &implement_brain);
    let code = run_implement(&implement_brain, &task, headless, Some(impl_profile));
    let _ = webagent::config::cleanup_swarm_profiles(&run_id);
    code
}

/// Lässt EIN Brain eine Aufgabe über den Controller umsetzen (edit/write + Git).
#[cfg(feature = "webview")]
pub fn run_implement(
    brain_id: &str,
    task: &str,
    headless: bool,
    profile: Option<std::path::PathBuf>,
) -> i32 {
    use webagent::browser::WebBrainBackend;
    use webagent::controller::AgentController;
    use webagent::executor::PlatformShellExecutor;

    let backend = match WebBrainBackend::from_config(brain_id) {
        Ok(mut b) => {
            if let Some(p) = profile {
                b = b.with_profile_override(p);
            }
            b
        }
        Err(e) => {
            eprintln!("[design-vote] Backend {brain_id}: {e}");
            return 1;
        }
    };
    let mut controller = AgentController::with_data_dir(
        backend,
        PlatformShellExecutor::new(),
        30,
        webagent::config::data_dir(),
    );
    match controller.run(task, brain_id, None, headless) {
        Ok(meta) => {
            println!(
                "[design-vote] Umsetzung beendet: status={} cycles={}",
                meta.status, meta.cycles
            );
            if meta.status == "done" {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("[design-vote] Umsetzung fehlgeschlagen: {e}");
            1
        }
    }
}

#[cfg(not(feature = "webview"))]
pub fn run_implement(_b: &str, _t: &str, _h: bool, _p: Option<std::path::PathBuf>) -> i32 {
    eprintln!("[design-vote] webview-Feature nicht aktiv.");
    1
}

/// `webagent benchmark` — vote-driven Code-Kompetenz-Benchmark. Wie
/// `autoresearch-self` bereitet es isolierte Profile vor und speist Phase A mit
/// `repl::isolated_query`; Phase B baut/misst pro Brain über den Controller.
#[allow(clippy::too_many_arguments)]
pub fn cmd_benchmark(
    brains: Option<String>,
    rounds: usize,
    suggestions: usize,
    max_iterations: u32,
    no_harvest: bool,
    verbose: bool,
    parallel: usize,
    stall_limit: u32,
    max_handoffs: usize,
    lint_eval: String,
    build_eval: String,
    test_eval: String,
    workdir: Option<String>,
    headless: bool,
    loop_forever: bool,
) -> i32 {
    let targets: Vec<String> = match brains {
        Some(csv) => csv
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        None => webagent::config::available_brain_ids(),
    };
    if targets.is_empty() {
        eprintln!("[benchmark] keine Brains registriert.");
        return 2;
    }

    let workdir = match workdir {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            webagent::autoresearch::resolve_project_root().unwrap_or_else(|_| std::env::temp_dir())
        }
    };

    let run_id = webagent::now_run_stamp();
    let profiles: Vec<(String, std::path::PathBuf)> = targets
        .iter()
        .map(|tb| {
            (
                tb.clone(),
                webagent::config::prepare_swarm_profile(&run_id, tb),
            )
        })
        .collect();
    let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
        profiles
            .iter()
            .find(|(b, _)| b == brain)
            .map(|(_, p)| p.clone())
    };

    let config = webagent::benchmark::BenchmarkConfig {
        brains: targets,
        rounds,
        suggestions,
        build_eval,
        test_eval,
        workdir,
        headless,
        max_iterations,
        harvest: !no_harvest,
        verbose,
        parallel,
        stall_limit,
        max_handoffs,
        lint_eval,
        vetoes: Vec::new(),
        loop_forever,
    };

    let result = webagent::benchmark::run_benchmark(&config, |b, p| {
        webagent::repl::isolated_query(b, p, headless, profile_of(b))
    });

    let _ = webagent::config::cleanup_swarm_profiles(&run_id);

    match result {
        Ok(report) => {
            if report.leaderboard.is_empty() {
                1
            } else {
                0
            }
        }
        Err(e) => {
            eprintln!("[benchmark] Fehler: {e}");
            1
        }
    }
}
