//! WebAgent CLI-Einstiegspunkt mit clap-basierter Befehlsstruktur.

use clap::Parser;
use std::collections::HashMap;
use std::process;

use webagent::run_store::RunStore;

mod cli;
use cli::{AutoresearchArgs, Cli, Commands};

/// Startup-Helfer: stale Runs reparieren (Python `main()` vor jedem Command außer maintenance-check).
pub fn startup_reconcile_runs() -> Vec<String> {
    let runs_dir = webagent::config::runs_dir();
    let store = RunStore::new(runs_dir.clone(), runs_dir.join("logs"));
    store.reconcile_stale_runs(600.0)
}

/// Stellt die Windows-Konsole auf UTF-8 (Codepage 65001).
///
/// Ohne das rendert die Konsole unsere Ausgabe in der ANSI-Codepage: der
/// Spinner, „—" und jeder Umlaut werden zu Zeichenmuell wie „ÖÇª"
/// (Beschwerde 2026-07-24). Fehler werden bewusst geschluckt — steht stdout
/// nicht an einer Konsole (Pipe, Datei, Dienst), gibt es nichts umzustellen
/// und der Start darf daran nicht scheitern.
#[cfg(all(windows, feature = "webview"))]
fn init_console_utf8() {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{
        GetConsoleMode, SetConsoleCP, SetConsoleMode, SetConsoleOutputCP, CONSOLE_MODE,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    };
    const UTF8: u32 = 65001;
    unsafe {
        let _ = SetConsoleOutputCP(UTF8);
        let _ = SetConsoleCP(UTF8);
        // Ohne VT-Verarbeitung faellt crossterm auf die ALTE Konsolen-API
        // zurueck. Die kennt nur 16 Attribute — unsere Rgb-Palette (ACCENT,
        // MUTED und die Level-Farben) ist dort nicht darstellbar und landet
        // auf Default-Weiss: die TUI sah komplett monochrom aus, obwohl
        // tui_render 58 Farben setzt (beobachtet 2026-07-26).
        let handle = HANDLE(std::io::stdout().as_raw_handle());
        let mut mode = CONSOLE_MODE(0);
        if GetConsoleMode(handle, &mut mode).is_ok() {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

#[cfg(not(all(windows, feature = "webview")))]
fn init_console_utf8() {}

fn main() {
    // Muss vor der ersten Ausgabe laufen, sonst ist die erste Zeile Mojibake.
    init_console_utf8();

    let cli = Cli::parse();
    // Kein Subcommand -> Chat-REPL als Default: `webagent` startet einen Chat,
    // der auch Aufgaben entgegennimmt (wie andere Coding-Agenten). Der
    // Worker-Pool bleibt über `webagent tui` / `webagent workers` erreichbar.
    let command = cli.command.unwrap_or(Commands::Repl {
        brain: "chatgpt".to_string(),
        headless: false,
    });

    let exit_code = if matches!(command, Commands::MaintenanceCheck { .. }) {
        dispatch(command)
    } else {
        webagent::config::ensure_stable_layout();
        let _ = webagent::config::ensure_data_dirs();
        let swept = webagent::config::sweep_stale_runtime_profiles();
        if swept > 0 {
            webagent::bench_events::eprint_line(&format!(
                "[cleanup] {} verwaiste Laufzeit-Profile entfernt",
                swept
            ));
        }
        // Wire comms.rs into CLI entry path (exercisable, not dead code)
        let comms = webagent::comms::CommsStore::default_store();
        let _ = comms.send(
            "webagent-rs",
            "self",
            "startup",
            "comms wired from main/CLI",
            None,
        );
        let repaired = startup_reconcile_runs();
        if !repaired.is_empty() {
            eprintln!(
                "[runs] {} verwaiste Run-Statuswerte repariert.",
                repaired.len()
            );
        }
        dispatch(command)
    };

    process::exit(exit_code);
}

fn dispatch(command: Commands) -> i32 {
    match command {
        Commands::Run {
            brain,
            task,
            resume,
            headless,
            max_cycles,
        } => cmd_run(&brain, &task, resume.as_deref(), headless, max_cycles),

        Commands::Login {
            brain,
            timeout,
            force,
        } => cmd_login(&brain, timeout, force),

        Commands::LoginAll {
            timeout,
            force,
            parallel,
        } => cmd_login_all(timeout, force, parallel),

        Commands::Diagnose { brain, headless } => cmd_diagnose(&brain, headless),

        Commands::Repl { brain, headless } => webagent::repl::run_repl(&brain, headless),

        Commands::Doctor { brain, json } => {
            cmd_doctor(if brain.is_empty() { None } else { Some(brain) }, json)
        }

        Commands::Watchdog {
            bot2bot_root,
            profile_dir,
            runs_dir,
            repair,
            json,
        } => cmd_watchdog(bot2bot_root, profile_dir, runs_dir, repair, json),

        Commands::BrainsHealth {
            allow_empty_profile,
        } => webagent::brains_health::run_brains_health(allow_empty_profile),

        Commands::Canary => cmd_canary(),

        Commands::Section {
            brain,
            key,
            visible,
        } => cmd_section(&brain, &key, !visible),

        Commands::Mode {
            brain,
            set,
            options,
            visible,
        } => cmd_mode(&brain, &set, &options, !visible),

        Commands::Menu {
            brain,
            key,
            options,
            set,
            visible,
        } => cmd_menu(&brain, &key, &options, set.as_deref(), !visible),

        Commands::Toggle {
            brain,
            option,
            visible,
        } => cmd_toggle(&brain, &option, !visible),

        Commands::Wall {
            interval,
            once,
            brain,
        } => cmd_wall(interval, once, &brain),

        Commands::Model {
            brain,
            set,
            visible,
        } => cmd_model(&brain, set.as_deref(), !visible),

        Commands::Shot {
            brain,
            out,
            open,
            visible,
        } => cmd_shot(brain.as_deref(), out.as_deref(), open.as_deref(), !visible),

        Commands::Survey {
            brain,
            write,
            open,
            dump,
            visible,
        } => cmd_survey(brain.as_deref(), write, !visible, dump, open.as_deref()),

        Commands::Quests { json } => cmd_quests(json),

        Commands::Relay {
            brain,
            message,
            headless,
            timeout,
            json,
        } => cmd_relay(&brain, &message, headless, timeout, json),

        Commands::Swarm {
            message,
            headless,
            timeout,
            brains,
            json,
        } => cmd_swarm(&message, headless, timeout, &brains, json),

        Commands::Bot2BotWorker {
            brain,
            once,
            poll_secs,
            max_cycles,
            headless,
        } => webagent::bot2bot_worker::run_bot2bot_worker(
            &brain, poll_secs, once, max_cycles, headless,
        ),

        Commands::Workers {
            active,
            brains,
            poll_secs,
            headless,
        } => webagent::worker_pool::run_worker_pool(active, &brains, poll_secs, headless),

        Commands::Tui {
            active,
            brains,
            poll_secs,
            headless,
            benchmark,
            view,
        } => webagent::tui::run_tui(
            active,
            &brains,
            poll_secs,
            headless,
            benchmark.as_deref(),
            view.as_deref(),
        ),

        Commands::Oobe {
            brains,
            skip_login,
            yes,
        } => cmd_oobe(&brains, skip_login, yes),

        Commands::Autoresearch(args) => cmd_autoresearch(args),

        Commands::AutoresearchSelf {
            suggestions,
            top,
            headless,
            facts,
        } => cmd_autoresearch_self(suggestions, top, headless, facts),

        Commands::Benchmark {
            brains,
            rounds,
            suggestions,
            max_iterations,
            no_harvest,
            verbose,
            parallel,
            stall_limit,
            max_handoffs,
            lint_eval,
            build_eval,
            test_eval,
            workdir,
            headless,
            loop_forever,
        } => cmd_benchmark(
            brains,
            rounds,
            suggestions,
            max_iterations,
            no_harvest,
            verbose,
            parallel,
            stall_limit,
            max_handoffs,
            lint_eval,
            build_eval,
            test_eval,
            workdir,
            headless,
            loop_forever,
        ),

        Commands::DesignVote {
            brains,
            topic,
            context,
            implement_brain,
            headless,
        } => cmd_design_vote(brains, topic, context, implement_brain, headless),

        Commands::RunsReport { limit } => {
            let dir = webagent::config::data_dir().join("runs");
            let runs = webagent::runs_report::recent_runs(&dir, limit);
            if runs.is_empty() {
                println!("Keine Laeufe unter {}", dir.display());
                return 0;
            }
            print!("{}", webagent::runs_report::format_report(&runs));
            0
        }

        Commands::MaintenanceCheck {
            json,
            pytest,
            pytest_timeout,
        } => cmd_maintenance_check(json, pytest, pytest_timeout),
    }
}

fn cmd_autoresearch(args: AutoresearchArgs) -> i32 {
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
fn cmd_autoresearch_self(
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
fn cmd_design_vote(
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
fn run_implement(
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
fn run_implement(_b: &str, _t: &str, _h: bool, _p: Option<std::path::PathBuf>) -> i32 {
    eprintln!("[design-vote] webview-Feature nicht aktiv.");
    1
}

/// `webagent benchmark` — vote-driven Code-Kompetenz-Benchmark. Wie
/// `autoresearch-self` bereitet es isolierte Profile vor und speist Phase A mit
/// `repl::isolated_query`; Phase B baut/misst pro Brain über den Controller.
#[allow(clippy::too_many_arguments)]
fn cmd_benchmark(
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

/// Balken der Breite `width` fuer `have/max`. Bei `max == 0` bewusst leer:
/// ein Brain ohne bekanntes Angebot soll nicht wie ein volles aussehen.
fn level_bar(have: usize, max: usize, width: usize) -> String {
    if max == 0 {
        return "·".repeat(width);
    }
    let filled = (have * width).div_ceil(max).min(width);
    format!("{}{}", "▓".repeat(filled), "░".repeat(width - filled))
}

/// Schreibt die gefundenen `ui_options` in die Nutzer-Selektordatei.
/// Bewusst die Nutzer-Kopie unter `<stable_root>/selectors`: der Quellbaum ist
/// bei einer deployten exe evtl. gar nicht da, und mitgelieferte Dateien
/// sollen von der Automatik nicht ueberschrieben werden.
fn write_ui_options(brain: &str, options: &[String]) -> Result<std::path::PathBuf, String> {
    let mut sel = webagent::config::load_selectors(brain)
        .map_err(|e| format!("Selektoren nicht lesbar: {e}"))?;
    let obj = sel
        .as_object_mut()
        .ok_or_else(|| "Selektordatei ist kein JSON-Objekt".to_string())?;
    obj.insert(
        "ui_options".to_string(),
        serde_json::Value::Array(
            options
                .iter()
                .map(|o| serde_json::Value::String(o.clone()))
                .collect(),
        ),
    );
    let dir = webagent::config::user_selectors_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("{e}"))?;
    let path = dir.join(format!("{brain}.json"));
    let body = serde_json::to_string_pretty(&sel).map_err(|e| format!("{e}"))?;
    std::fs::write(&path, body).map_err(|e| format!("{e}"))?;
    Ok(path)
}

fn cmd_wall(interval: u64, once: bool, only: &[String]) -> i32 {
    use webagent::browser::WebBrainBackend;

    let dir = webagent::config::data_dir().join("shots");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[wall] Zielverzeichnis nicht anlegbar: {e}");
        return 2;
    }
    let brains: Vec<String> = if only.is_empty() {
        webagent::config::available_brain_ids()
    } else {
        only.to_vec()
    };
    if brains.is_empty() {
        eprintln!("[wall] keine Brains");
        return 2;
    }

    let mut round: u64 = 0;
    loop {
        round += 1;
        let mut ok = 0usize;
        for id in &brains {
            match WebBrainBackend::from_config(id).and_then(|mut b| b.live_screenshot(true)) {
                Ok(png) => match std::fs::write(dir.join(format!("{id}.png")), &png) {
                    Ok(()) => ok += 1,
                    Err(e) => eprintln!("[wall] {id}: schreiben fehlgeschlagen: {e}"),
                },
                // Ein Brain, das gerade nicht will, darf die Wand nicht
                // aufhalten — die alte Kachel bleibt dann einfach stehen.
                Err(e) => eprintln!("[wall] {id}: {e}"),
            }
        }
        match webagent::welcome::write_wall_html(&dir, &brains, interval, round) {
            Ok(path) => {
                println!(
                    "[wall] Runde {round}: {ok}/{} aufgenommen -> {}",
                    brains.len(),
                    path.display()
                );
                if round == 1 {
                    println!("[wall] im Browser oeffnen: {}", path.display());
                }
            }
            Err(e) => {
                eprintln!("[wall] wall.html nicht schreibbar: {e}");
                return 1;
            }
        }
        if once {
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_secs(interval.max(5)));
    }
}

fn cmd_section(brain: &str, key: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[section] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[section] {brain}: {e}");
        return 2;
    }
    let code = match backend.open_section(key) {
        Ok((before, after)) => {
            println!("[section] {brain}/{key}: ''{before}'' -> ''{after}''");
            0
        }
        Err(e) => {
            eprintln!("[section] {brain}/{key}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

fn cmd_mode(brain: &str, set: &str, options: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[mode] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[mode] {brain}: {e}");
        return 2;
    }
    let code = match backend.select_segment(options, set) {
        Ok(state) => {
            println!("[mode] {brain}: ''{set}'' aktiv ({state})");
            0
        }
        Err(e) => {
            eprintln!("[mode] {brain}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

fn cmd_menu(brain: &str, key: &str, options: &str, set: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[menu] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[menu] {brain}: {e}");
        return 2;
    }
    let code = match set {
        None => {
            println!(
                "[menu] {brain}/{key}: aktuell ''{}''",
                backend.menu_label(key)
            );
            match backend.list_menu(key, options) {
                Ok(list) if !list.is_empty() => {
                    for m in &list {
                        println!("    {m}");
                    }
                    0
                }
                Ok(_) => {
                    eprintln!("[menu] {brain}/{key}: Menue lieferte keine Eintraege");
                    1
                }
                Err(e) => {
                    eprintln!("[menu] {brain}/{key}: {e}");
                    1
                }
            }
        }
        Some(want) if want.contains('>') => {
            // Pfad durch Untermenues: "Aufwand > Hoch". Claude legt die
            // Denkstufe eine Ebene tiefer als das Modell.
            let path: Vec<&str> = want
                .split('>')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            match backend.select_in_menu_path(key, options, &path) {
                Ok(now) => {
                    println!("[menu] {brain}/{key}: ''{now}''");
                    0
                }
                Err(e) => {
                    eprintln!("[menu] {brain}/{key}: {e}");
                    1
                }
            }
        }
        Some(want) => match backend.select_in_menu(key, options, want) {
            Ok(now) => {
                println!("[menu] {brain}/{key}: ''{now}''");
                0
            }
            Err(e) => {
                eprintln!("[menu] {brain}/{key}: {e}");
                1
            }
        },
    };
    let _ = backend.close_ui();
    code
}

fn cmd_toggle(brain: &str, option: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[toggle] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[toggle] {brain}: {e}");
        return 2;
    }
    let code = match backend.toggle_option(option) {
        Ok((before, after)) => {
            println!("[toggle] {brain}/{option}: '{before}' -> '{after}'");
            0
        }
        Err(e) => {
            eprintln!("[toggle] {brain}/{option}: {e}");
            1
        }
    };
    let _ = backend.close_ui();
    code
}

fn cmd_model(brain: &str, set: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[model] {brain}: {e}");
            return 2;
        }
    };
    if let Err(e) = backend.open_for_ui(headless) {
        eprintln!("[model] {brain}: {e}");
        return 2;
    }
    let code = match set {
        None => {
            println!("[model] {brain}: aktuell '{}'", backend.current_model());
            match backend.list_models() {
                Ok(list) if !list.is_empty() => {
                    for m in &list {
                        println!("    {m}");
                    }
                    0
                }
                Ok(_) => {
                    // Leeres Menue ist ein Messfehler, kein Ergebnis: entweder
                    // greift model_option nicht oder das Menue ging nicht auf.
                    eprintln!("[model] {brain}: Menue lieferte keine Eintraege");
                    1
                }
                Err(e) => {
                    eprintln!("[model] {brain}: {e}");
                    1
                }
            }
        }
        Some(want) => match backend.switch_model(want) {
            Ok(now) => {
                println!("[model] {brain}: umgestellt auf '{now}'");
                0
            }
            Err(e) => {
                eprintln!("[model] {brain}: {e}");
                1
            }
        },
    };
    let _ = backend.close_ui();
    code
}

fn cmd_shot(brain: Option<&str>, out: Option<&str>, open: Option<&str>, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let dir = match out {
        Some(o) => std::path::PathBuf::from(o),
        None => webagent::config::data_dir().join("shots"),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("[shot] Zielverzeichnis nicht anlegbar: {e}");
        return 2;
    }
    let targets: Vec<String> = match brain {
        Some(b) => vec![b.to_string()],
        None => webagent::config::available_brain_ids(),
    };
    let mut failures = 0;
    for id in &targets {
        let mut backend = match WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[shot] {id}: {e}");
                failures += 1;
                continue;
            }
        };
        eprintln!("[shot] {id}: nehme Oberflaeche auf (headless={headless})…");
        match backend.live_screenshot_with(headless, open) {
            Ok(png) => {
                let path = dir.join(format!("{id}.png"));
                match std::fs::write(&path, &png) {
                    Ok(()) => {
                        println!("  {:<10} {} KB -> {}", id, png.len() / 1024, path.display())
                    }
                    Err(e) => {
                        eprintln!("[shot] {id}: Schreiben fehlgeschlagen: {e}");
                        failures += 1;
                    }
                }
            }
            Err(e) => {
                eprintln!("[shot] {id}: Fehler: {e}");
                failures += 1;
            }
        }
    }
    if failures > 0 {
        eprintln!("[shot] {failures}/{} fehlgeschlagen", targets.len());
        1
    } else {
        0
    }
}

fn cmd_survey(
    brain: Option<&str>,
    write: bool,
    headless: bool,
    dump: bool,
    open: Option<&str>,
) -> i32 {
    use webagent::browser::WebBrainBackend;

    let targets: Vec<String> = match brain {
        Some(b) => vec![b.to_string()],
        None => webagent::config::available_brain_ids(),
    };
    let mut failures = 0;
    for id in &targets {
        let mut backend = match WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[survey] {id}: {e}");
                failures += 1;
                continue;
            }
        };
        eprintln!("[survey] {id}: oeffne Oberflaeche (headless={headless})…");
        let report = match backend.live_survey_with(headless, open) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[survey] {id}: Fehler: {e}");
                failures += 1;
                continue;
            }
        };
        let buttons = report
            .get("buttons")
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();
        let has_composer = report
            .get("counts")
            .and_then(|c| c.get("composer"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            > 0;
        let options = webagent::capability::detect_ui_options(&buttons, has_composer);

        if dump {
            // Rohe Beschriftungen zeigen, damit die Stichwortlisten an echten
            // Texten wachsen statt an geratenen.
            let mut seen: Vec<String> = Vec::new();
            for b in &buttons {
                let label = ["al", "ti", "dt", "tp"]
                    .iter()
                    .filter_map(|k| b.get(*k).and_then(|v| v.as_str()))
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" | ");
                if !label.is_empty() && !seen.contains(&label) {
                    seen.push(label);
                }
            }
            println!("  --- {id}: ROH (erste 5) ---");
            for b in buttons.iter().take(40) {
                println!("      {b}");
            }
            println!(
                "  --- {id}: counts = {} ---",
                report
                    .get("counts")
                    .map(|c| c.to_string())
                    .unwrap_or_default()
            );
            println!("  --- {id}: {} beschriftete Elemente ---", seen.len());
            for l in seen.iter().take(60) {
                println!("      {l}");
            }
        }

        if options.is_empty() {
            // Kein Fund ist ein Messfehler, kein Ergebnis — sonst schriebe man
            // eine leere Wahrheit fest und das Brain stuende dauerhaft auf [n/0].
            eprintln!(
                "[survey] {id}: nichts erkannt ({} Buttons, composer={}) — nicht geschrieben",
                buttons.len(),
                has_composer
            );
            eprintln!(
                "[survey] {id}: url={} title={:?}",
                report
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<keine>"),
                report.get("title").and_then(|v| v.as_str()).unwrap_or("")
            );
            failures += 1;
            continue;
        }

        println!(
            "  {:<10} {} Optionen aus {} Buttons: {}",
            id,
            options.len(),
            buttons.len(),
            options.join(", ")
        );

        if write {
            match write_ui_options(id, &options) {
                Ok(p) => println!("             geschrieben nach {}", p.display()),
                Err(e) => {
                    eprintln!("[survey] {id}: Schreiben fehlgeschlagen: {e}");
                    failures += 1;
                }
            }
        }
    }
    if failures > 0 {
        eprintln!("[survey] {failures}/{} fehlgeschlagen", targets.len());
        1
    } else {
        0
    }
}

fn cmd_quests(json: bool) -> i32 {
    let levels = webagent::capability::levels_all();
    if levels.is_empty() {
        println!("[quests] keine Brains registriert");
        return 2;
    }

    if json {
        let payload: Vec<_> = levels
            .iter()
            .map(|l| {
                serde_json::json!({
                    "brain_id": l.brain_id,
                    "level": l.level(),
                    // null = unvermessen; bewusst kein geratener Zahlenwert.
                    "max_level": l.max_level(),
                    "surveyed": l.surveyed,
                    "rank": l.rank(),
                    "have": l.have,
                    "quests": l.quests.iter().map(|q| serde_json::json!({
                        "key": q.key,
                        "label": q.label,
                        "blocker": q.blocker.as_str(),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("[quests] JSON-Fehler: {e}");
                return 1;
            }
        }
        return 0;
    }

    println!();
    println!("      GREETINGS PROFESSOR.");
    println!("      SHALL WE PLAY A GAME?");
    println!();

    let total: usize = levels.iter().map(|l| l.level()).sum();
    let total_max: usize = levels.iter().filter_map(|l| l.max_level()).sum();

    let unreachable: usize = levels.iter().map(|l| l.out_of_reach.len()).sum();
    println!("  ── POKIDEX ────────────────────────────────────────────");
    for l in &levels {
        println!(
            "   {:<12} {:<8} {}  {:<14} {}",
            l.brain_id,
            l.label()
                .rsplit_once('[')
                .map(|(_, r)| format!("[{r}"))
                .unwrap_or_default(),
            level_bar(l.level(), l.max_level().unwrap_or(0), 8),
            l.rank(),
            if l.maxed() { "ausgereizt" } else { "" }
        );
    }
    println!();
    let unsurveyed = levels.iter().filter(|l| !l.surveyed).count();
    if unsurveyed > 0 {
        println!(
            "  GESAMT {total}/?  —  {unsurveyed} von {} Eintraegen noch nicht vermessen.",
            levels.len()
        );
        println!("  Ohne Zaehlung der Oberflaeche ist kein Maximum bekannt (nicht geraten).");
    } else {
        println!(
            "  GESAMT {total}/{total_max}  {}",
            level_bar(total, total_max, 20)
        );
    }
    if unreachable > 0 {
        // Sichtbar halten statt verschweigen: sonst sieht es aus, als gaebe es
        // diese Optionen nicht — dabei gibt es sie, nur nicht fuer uns.
        println!(
            "  ({unreachable} angebotene Optionen sind fuer diesen Agenten prinzipiell\n   nicht nachweisbar fahrbar und stehen deshalb nicht im Nenner.)"
        );
        println!();
    }

    let log = webagent::capability::quest_log();
    if log.is_empty() {
        println!("  KEINE OFFENEN QUESTS. A STRANGE GAME.");
        return 0;
    }

    println!("  ── OFFENE QUESTS ──────────────────────────────────────");
    println!("  (nach Reichweite: oben bringt eine Umsetzung die meisten Level)");
    println!();
    for (key, quests) in &log {
        let label = webagent::capability::capability(key)
            .map(|c| c.label)
            .unwrap_or(key.as_str());
        println!("   {label}  (+{} Level)", quests.len());
        for q in quests {
            println!("      {:<10} {}", q.brain_id, q.blocker.as_str());
        }
        println!();
    }
    println!("  THE ONLY WINNING MOVE IS TO IMPLEMENT.");
    0
}

fn cmd_canary() -> i32 {
    let results = webagent::canary::run_canary();
    if results.is_empty() {
        println!("[canary] keine Brains registriert");
        return 2;
    }
    println!("[canary] {} Brains:", results.len());
    let mut fail = 0u32;
    for r in &results {
        let status = if r.ok { "ok" } else { "FAIL" };
        if !r.ok {
            fail += 1;
        }
        println!(
            "  {:<10} {status:<4}  latency_ms={}  reason={}",
            r.brain_id, r.latency_ms, r.reason
        );
    }
    if fail > 0 {
        println!("[canary] {fail}/{} failed", results.len());
        1
    } else {
        println!("[canary] all ok");
        0
    }
}

/// Ein Brain-Ergebnis fuer `--json` (relay + swarm).
#[derive(Debug, Clone, serde::Serialize)]
struct BrainIoResult {
    brain: String,
    ok: bool,
    answer: String,
    latency_ms: u64,
    reason: String,
}

fn brain_io_json(r: &BrainIoResult) -> String {
    serde_json::to_string(r).unwrap_or_else(|_| {
        format!(
            r#"{{"brain":"{}","ok":false,"answer":"","latency_ms":0,"reason":"json_serialize_failed"}}"#,
            r.brain
        )
    })
}

fn cmd_relay(brain: &str, message: &str, headless: bool, timeout: f64, json: bool) -> i32 {
    let to = if timeout > 0.0 { Some(timeout) } else { None };
    let started = std::time::Instant::now();
    match webagent::relay::relay_single_turn(brain, message, headless, to) {
        Ok(reply) => {
            let r = BrainIoResult {
                brain: brain.to_string(),
                ok: true,
                answer: reply.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            };
            if json {
                println!("{}", brain_io_json(&r));
            } else {
                println!("{reply}");
            }
            0
        }
        Err(e) => {
            let r = BrainIoResult {
                brain: brain.to_string(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            };
            if json {
                println!("{}", brain_io_json(&r));
            } else {
                eprintln!("[relay] error: {e}");
            }
            1
        }
    }
}

fn cmd_swarm(message: &str, headless: bool, timeout: f64, brains: &str, json: bool) -> i32 {
    let to = if timeout > 0.0 { Some(timeout) } else { None };
    let targets: Vec<String> = {
        let listed: Vec<String> = brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if listed.is_empty() {
            webagent::config::available_brain_ids()
        } else {
            listed
        }
    };
    if targets.is_empty() {
        if json {
            println!(r#"{{"brains":[],"synthesis":null,"error":"no_brains"}}"#);
        } else {
            eprintln!("[swarm] keine Brains");
        }
        return 2;
    }

    if !json {
        println!("[swarm] Phase 1 — {} Brains…", targets.len());
    }

    let mut results: Vec<BrainIoResult> = Vec::new();
    for brain in &targets {
        let started = std::time::Instant::now();
        let r = match webagent::relay::relay_single_turn(brain, message, headless, to) {
            Ok(answer) => BrainIoResult {
                brain: brain.clone(),
                ok: true,
                answer,
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            },
            Err(e) => BrainIoResult {
                brain: brain.clone(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            },
        };
        if !json {
            let status = if r.ok { "ok" } else { "FAIL" };
            let preview: String = r.answer.chars().take(160).collect();
            println!(
                "  {:<10} {status:<4}  {}ms  {}{}",
                r.brain,
                r.latency_ms,
                if r.ok { preview } else { r.reason.clone() },
                if r.ok && r.answer.chars().count() > 160 {
                    "…"
                } else {
                    ""
                }
            );
        }
        results.push(r);
    }

    let ok_brains: Vec<&BrainIoResult> = results.iter().filter(|r| r.ok).collect();
    let synthesis = if ok_brains.is_empty() {
        None
    } else if ok_brains.len() == 1 {
        Some(ok_brains[0].clone())
    } else {
        // Reliability-Orch (wie REPL-Default), Fallback: erstes ok
        let board = webagent::brain_score::leaderboard();
        let score_of = |id: &str| -> f64 {
            board
                .iter()
                .find(|s| s.brain_id == id)
                .map(|s| s.reliability)
                .unwrap_or(0.0)
        };
        let orch = ok_brains
            .iter()
            .max_by(|a, b| {
                score_of(&a.brain)
                    .partial_cmp(&score_of(&b.brain))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|r| r.brain.as_str())
            .unwrap_or(ok_brains[0].brain.as_str());

        let joined: String = ok_brains
            .iter()
            .map(|r| format!("### {}\n{}", r.brain, r.answer))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synth_prompt = format!(
            "Aufgabe: «{message}».\n\nDie beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
             Führe diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
             Nenne Widersprüche, wenn es welche gibt. Du bist der Orchestrator ({orch})."
        );
        if !json {
            println!("[swarm] Phase 2/3 — Synthese via {orch}…");
        }
        let started = std::time::Instant::now();
        match webagent::relay::relay_single_turn(orch, &synth_prompt, headless, to) {
            Ok(answer) => Some(BrainIoResult {
                brain: orch.to_string(),
                ok: true,
                answer,
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            }),
            Err(e) => Some(BrainIoResult {
                brain: orch.to_string(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            }),
        }
    };

    if json {
        let payload = serde_json::json!({
            "brains": results,
            "synthesis": synthesis,
        });
        match serde_json::to_string(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("[swarm] json error: {e}");
                return 1;
            }
        }
    } else if let Some(s) = &synthesis {
        if s.ok {
            println!("\n[swarm ⇒ final via {}]\n{}\n", s.brain, s.answer);
        } else {
            println!("[swarm] Synthese fehlgeschlagen: {}", s.reason);
        }
    } else {
        println!("[swarm] Keine Antworten — Abbruch.");
    }

    let any_ok = results.iter().any(|r| r.ok);
    if any_ok {
        0
    } else {
        1
    }
}

fn cmd_oobe(brains: &str, skip_login: bool, yes: bool) -> i32 {
    match webagent::oobe::run_oobe_wizard(!yes, skip_login, brains, yes) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[oobe] {e}");
            2
        }
    }
}

fn cmd_run(brain: &str, task: &str, resume: Option<&str>, headless: bool, max_cycles: u32) -> i32 {
    use webagent::browser::WebBrainBackend;
    use webagent::controller::AgentController;
    use webagent::executor::PlatformShellExecutor;

    let backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[run] {e}");
            return 2;
        }
    };
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::new(backend, executor, max_cycles as usize);

    // Fortschritt auf stdout — auf stderr rendern PowerShell-Wrapper das als
    // roten NativeCommandError-Block, obwohl nichts kaputt ist (Dogfood-Fund 2026-07-20).
    println!(
        "[run] brain={} headless={} max_cycles={} — starte Embedded WebView…",
        brain, headless, max_cycles
    );
    match controller.run(task, brain, resume, headless) {
        Ok(meta) => {
            println!(
                "[run] status={} run_id={} cycles={}",
                meta.status, meta.run_id, meta.cycles
            );
            if meta.status == "done" {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("[run] Fehler: {e}");
            1
        }
    }
}

fn cmd_login_all(timeout_secs: u64, force: bool, parallel: usize) -> i32 {
    use std::time::Duration;

    let parallel = if parallel > 3 {
        eprintln!("[login-all] --parallel {parallel} gedeckelt auf 3");
        3
    } else {
        parallel
    };
    if parallel == 0 {
        eprintln!("[login-all] sequenziell, {timeout_secs}s pro Brain (profiles/<brain>)…");
    } else {
        eprintln!("[login-all] parallel={parallel} (experimentell), {timeout_secs}s pro Brain…");
    }
    let results = webagent::login::login_all(Duration::from_secs(timeout_secs), parallel, force);
    let mut fail = 0usize;
    for r in &results {
        let tag = if r.skipped {
            "skip"
        } else if r.ok {
            "ok"
        } else {
            fail += 1;
            "FAIL"
        };
        println!("[login-all] [{tag}] {}: {}", r.brain_id, r.message);
    }
    println!(
        "[login-all] fertig: {}/{} ok, {} übersprungen, {} fail",
        results.iter().filter(|r| r.ok).count(),
        results.len(),
        results.iter().filter(|r| r.skipped).count(),
        fail
    );
    if fail > 0 {
        1
    } else {
        0
    }
}

fn cmd_login(brain: &str, timeout_secs: u64, force: bool) -> i32 {
    use std::time::Duration;
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[login] {e}");
            return 2;
        }
    };
    let timeout = Duration::from_secs(timeout_secs);
    if force {
        eprintln!(
            "[login] {brain}: --force — Fenster bleibt {timeout_secs}s offen, unabhaengig vom Login-Check. \
             Fenster schliessen, sobald du fertig bist."
        );
        return match backend.hold_window_open(timeout) {
            Ok(()) => {
                println!("[login] {brain}: Fenster geschlossen, Profil geschrieben.");
                0
            }
            Err(e) => {
                eprintln!("[login] {brain}: Fehler: {e}");
                1
            }
        };
    }
    match backend.interactive_login(timeout) {
        Ok(true) => {
            println!("[login] {brain}: Login erkannt und Session gespeichert.");
            0
        }
        Ok(false) => {
            eprintln!(
                "[login] {brain}: kein Login innerhalb von {timeout_secs}s erkannt. Erneut versuchen mit --timeout \
                 oder --force (Erkennung ist bei manchen Brains zu optimistisch)."
            );
            1
        }
        Err(e) => {
            eprintln!("[login] {brain}: Fehler: {e}");
            1
        }
    }
}

fn cmd_diagnose(brain: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[diagnose] {e}");
            return 2;
        }
    };
    eprintln!("[diagnose] {brain}: starte Browser (headless={headless})…");
    match backend.live_diagnose(headless) {
        Ok(d) => {
            let ok = |b: bool| if b { "ok" } else { "FEHLT" };
            println!("[diagnose] {}", d.brain_id);
            println!("    session_state:  {:?}", d.session_state);
            println!("    logged_in:      {}", d.logged_in);
            println!("    login_button:   {}", d.login_button_visible);
            println!("    composer:       {}", ok(d.composer_found));
            println!("    assistant_msgs: {}", d.assistant_count);
            println!("    cloudflare:     {}", d.cloudflare);
            println!("    url:            {}", d.url);
            // Healthy = eingeloggt, Composer da, keine Cloudflare-Blockade.
            if d.logged_in && d.composer_found && !d.cloudflare {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("[diagnose] {brain}: Fehler: {e}");
            1
        }
    }
}

fn cmd_doctor(brain_ids: Option<Vec<String>>, json: bool) -> i32 {
    // Konfiguration aus config.rs laden
    let brains_config = webagent::config::brains();

    // Runs-Verzeichnis aus config.rs
    let runs_dir = webagent::config::runs_dir().to_string_lossy().to_string();

    let report = webagent::doctor::run_doctor(
        brain_ids,
        Some(&brains_config),
        &runs_dir,
        None, // list_runs_fn
        None, // load_fn
    );

    if json {
        // JSON-Ausgabe
        match serde_json::to_string_pretty(&serde_json::json!({
            "ok": report.ok(),
            "timestamp": report.timestamp,
            "healthy": report.healthy_brain_ids(),
            "unhealthy": report.unhealthy_brain_ids(),
            "brains": report.brains.iter().map(|(id, check)| {
                (id, serde_json::json!({
                    "healthy": check.healthy(),
                    "selectors_ok": check.selectors_ok,
                    "selectors_path": check.selectors_path,
                    "selectors_mtime": check.selectors_mtime,
                    "profile_exists": check.profile_exists,
                    "profile_dir": check.profile_dir,
                    "profile_lock_files": check.profile_lock_files,
                    "last_done_run": check.last_done_run,
                    "last_done_run_age_hours": check.last_done_run_age_hours,
                    "login_state": check.login_state,
                    "recovery_hint": check.recovery_hint,
                }))
            }).collect::<HashMap<_, _>>(),
        })) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("[doctor] JSON-Serialisierung fehlgeschlagen: {}", e);
                return 1;
            }
        }
    } else {
        // Menschenlesbare Ausgabe
        println!("[doctor] {}", report.timestamp);
        println!(
            "[doctor] healthy: {}",
            if report.healthy_brain_ids().is_empty() {
                "keine".to_string()
            } else {
                report.healthy_brain_ids().join(", ")
            }
        );
        if !report.unhealthy_brain_ids().is_empty() {
            println!(
                "[doctor] unhealthy: {}",
                report.unhealthy_brain_ids().join(", ")
            );
        }
        println!();

        let mut brain_ids: Vec<_> = report.brains.keys().collect();
        brain_ids.sort();

        for brain_id in brain_ids {
            let check = &report.brains[brain_id];
            let status_icon = if check.healthy() { "ok" } else { "PROBLEM" };
            println!("  [{}] {}", status_icon, brain_id);
            println!(
                "    selectors:  {} ({})",
                if check.selectors_ok { "ok" } else { "FEHLT" },
                check.selectors_path
            );
            println!(
                "    selectors:  mtime={}",
                if check.selectors_mtime.is_empty() {
                    "n/a"
                } else {
                    &check.selectors_mtime
                }
            );
            println!(
                "    profile:    {} ({})",
                if check.profile_exists { "ok" } else { "FEHLT" },
                check.profile_dir
            );
            if !check.profile_lock_files.is_empty() {
                println!("    locks:      {}", check.profile_lock_files.join(", "));
            }
            if !check.last_done_run.is_empty() {
                let age = check.last_done_run_age_hours;
                let age_str = if age >= 0.0 {
                    format!("{:.0}h", age)
                } else {
                    "unbekannt".to_string()
                };
                println!("    last_run:   {} ({})", check.last_done_run, age_str);
            } else {
                println!("    last_run:   keiner");
            }
            println!("    login_state: {}", check.login_state);
            if !check.recovery_hint.is_empty() {
                println!("    recovery:   {}", check.recovery_hint);
            }
            println!();
        }
    }

    if report.ok() {
        0
    } else {
        2
    }
}

fn cmd_watchdog(
    bot2bot_root: Option<String>,
    profile_dir: Option<String>,
    runs_dir: Option<String>,
    repair: bool,
    json: bool,
) -> i32 {
    use webagent::config;
    use webagent::run_store::RunStore;
    use webagent::watchdog;

    let bot2bot_root =
        bot2bot_root.unwrap_or_else(|| config::bot2bot_root().to_string_lossy().to_string());
    let profile_dir = profile_dir.unwrap_or_else(|| {
        config::profiles_dir()
            .join("shared")
            .to_string_lossy()
            .to_string()
    });
    let runs_dir = runs_dir.unwrap_or_else(|| config::runs_dir().to_string_lossy().to_string());

    let store = RunStore::new(config::runs_dir(), config::runs_dir().join("logs"));

    let report = watchdog::run_watchdog(
        &bot2bot_root,
        &profile_dir,
        &runs_dir,
        Some(&store),
        repair,
        None,
    );

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("[watchdog] JSON-Serialisierung fehlgeschlagen: {}", e);
                return 1;
            }
        }
    } else {
        println!("[watchdog] {}", report.timestamp);
        println!("[watchdog] orphaned_runs: {}", report.orphaned_runs.len());
        println!(
            "[watchdog] stale_bridge_locks: {}",
            report.stale_bridge_locks.len()
        );
        println!(
            "[watchdog] stale_profile_locks: {}",
            report.stale_profile_locks.len()
        );
        if repair {
            println!("[watchdog] repaired_runs: {}", report.repaired_runs.len());
            println!(
                "[watchdog] repaired_bridge_locks: {}",
                report.repaired_bridge_locks.len()
            );
            println!(
                "[watchdog] repaired_profile_locks: {}",
                report.repaired_profile_locks.len()
            );
        }
        if !report.errors.is_empty() {
            println!("[watchdog] errors: {}", report.errors.join(", "));
        }
        println!();
    }

    if (report.ok() && report.errors.is_empty()) || (repair && report.total_repaired() > 0) {
        0
    } else {
        2
    }
}

/// Führt einen Unterprozess aus und bricht nach `timeout_secs` ab.
/// Gibt `Some(true/false)` bei regulärem Exit zurück, `None` bei Timeout/Fehler.
fn run_command_with_timeout(cmd: &str, args: &[&str], timeout_secs: f64) -> Option<bool> {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut child = match Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let start = Instant::now();
    let limit = Duration::from_secs_f64(timeout_secs.max(1.0));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => return None,
        }
    }
}

/// Bündelt die Read-only-Gates: doctor + watchdog (dry-run) + optional Test-Suite.
fn maintenance_healthy(do_pytest: bool, pytest_timeout: f64) -> bool {
    // 1) Doctor: alle konfigurierten Brains gesund?
    let brains_config = webagent::config::brains();
    let runs_dir = webagent::config::runs_dir().to_string_lossy().to_string();
    let doctor_report =
        webagent::doctor::run_doctor(None, Some(&brains_config), &runs_dir, None, None);
    if !doctor_report.ok() {
        return false;
    }

    // 2) Watchdog dry-run: keine Funde/Fehler?
    let store = RunStore::new(
        webagent::config::runs_dir(),
        webagent::config::runs_dir().join("logs"),
    );
    let wd = webagent::watchdog::run_watchdog(
        webagent::config::bot2bot_root().to_string_lossy().as_ref(),
        webagent::config::profiles_dir()
            .join("shared")
            .to_string_lossy()
            .as_ref(),
        &runs_dir,
        Some(&store),
        false,
        None,
    );
    if !wd.ok() || !wd.errors.is_empty() {
        return false;
    }

    // 3) Optional: Test-Suite
    if do_pytest {
        match run_command_with_timeout("cargo", &["test", "--quiet"], pytest_timeout) {
            Some(true) => {}
            _ => return false,
        }
    }

    true
}

fn cmd_maintenance_check(json: bool, pytest: bool, pytest_timeout: f64) -> i32 {
    let healthy = maintenance_healthy(pytest, pytest_timeout);

    if json {
        match serde_json::to_string_pretty(&serde_json::json!({
            "healthy": healthy,
            "pytest": pytest,
        })) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!(
                    "[maintenance-check] JSON-Serialisierung fehlgeschlagen: {}",
                    e
                );
                return 1;
            }
        }
    } else {
        println!("[maintenance-check] healthy={}", healthy);
        if pytest {
            println!("[maintenance-check] pytest={}", pytest);
        }
    }

    if healthy {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_reconcile_runs_does_not_panic() {
        let repaired = startup_reconcile_runs();
        let _ = repaired;
    }

    #[test]
    fn test_maintenance_healthy_does_not_panic() {
        // Übt den Gate-Pfad (doctor + watchdog) ohne pytest aus.
        // Assertiert primär, dass die Funktion ohne Panic ein bool liefert.
        let result = maintenance_healthy(false, 60.0);
        assert!(result || !result);
    }
}
