//! WebAgent CLI-Einstiegspunkt mit clap-basierter Befehlsstruktur.

use clap::Parser;
use std::process;

mod cli;
mod commands;
use cli::{Cli, Commands};
use commands::ops::*;
use commands::research::*;
use commands::ui::*;

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

fn run() -> i32 {
    // Muss vor der ersten Ausgabe laufen, sonst ist die erste Zeile Mojibake.
    init_console_utf8();

    let cli = Cli::parse();
    // Lokale Diagnose-/Registry-Befehle benÃ¶tigen weder persistierte
    // TUI-Einstellungen noch den Browser-Probe-Hook. Der frÃ¼he RÃ¼ckweg
    // hÃ¤lt ihre Zusage "kein Browser, kein Profil, kein Netz" ein.
    if matches!(
        &cli.command,
        Some(Commands::MaintenanceCheck { .. } | Commands::Cloud { .. })
    ) {
        return dispatch(cli.command.expect("geprÃ¼fter lokaler Befehl fehlt"));
    }

    // Gespeicherte Einstellungen werden vor dem ersten regulÃ¤ren Ablauf
    // in die Umgebung gelegt; eine beim Start gesetzte Variable bleibt
    // unverÃ¤ndert. Ohne diesen Schritt wÃ¤re die Einstellungen-Ansicht
    // eine Attrappe.
    webagent::tui_config::apply_persisted();
    webagent::bin_hooks::set_probe_fn(cmd_probe);
    // Kein Subcommand -> Web-UI (T-201). REPL bleibt `webagent repl`,
    // Pool/Wand bleibt `webagent tui`.
    let command = cli.command.unwrap_or_else(|| Commands::Ui {
        bind: "127.0.0.1".to_string(),
        port: webagent::web_ui::DEFAULT_PORT,
        no_open: false,
    });

    let exit_code = if matches!(
        command,
        Commands::MaintenanceCheck { .. } | Commands::Cloud { .. }
    ) {
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

    exit_code
}

fn dispatch(command: Commands) -> i32 {
    match command {
        Commands::Run {
            brain,
            task,
            resume,
            headless,
            max_cycles,
            no_memory,
        } => cmd_run(
            &brain,
            &task,
            resume.as_deref(),
            headless,
            max_cycles,
            no_memory,
        ),

        Commands::Login {
            brain,
            timeout,
            force,
            auto,
        } => cmd_login(&brain, timeout, force, auto),

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

        Commands::Verify {
            brain,
            cap,
            headless,
        } => cmd_verify(
            if brain.is_empty() { None } else { Some(brain) },
            cap,
            headless,
        ),

        Commands::Count {
            brain,
            count,
            headless,
        } => cmd_count(
            if brain.is_empty() { None } else { Some(brain) },
            count,
            headless,
        ),

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

        Commands::Probe {
            url,
            brain_id,
            brain,
            write,
            verify,
            open,
            dump,
            dump_text,
            generating,
            stop_diff,
            visible,
        } => cmd_probe(
            url.as_deref(),
            brain_id.as_deref(),
            brain.as_deref(),
            write,
            verify,
            open.as_deref(),
            dump,
            dump_text,
            generating,
            stop_diff,
            !visible,
        ),

        Commands::Quests { json } => cmd_quests(json),

        Commands::MeasureLimits {
            brains,
            headless,
            force,
            start,
            ceiling,
            tolerance,
        } => cmd_measure_limits(&brains, headless, force, start, ceiling, tolerance),

        Commands::Relay {
            brain,
            message,
            message_file,
            headless,
            timeout,
            json,
            model,
        } => cmd_relay(
            &brain,
            &message,
            message_file.as_deref(),
            headless,
            timeout,
            json,
            model.as_deref(),
        ),

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

        Commands::Ui {
            bind,
            port,
            no_open,
        } => cmd_ui(bind, port, no_open),

        Commands::Tui {
            active,
            brains,
            poll_secs,
            headless,
            run_secs,
            benchmark,
            view,
            force_tui,
        } => webagent::tui::run_tui(
            active,
            &brains,
            poll_secs,
            headless,
            run_secs,
            benchmark.as_deref(),
            view.as_deref(),
            force_tui,
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

        Commands::Goal { command } => cmd_goal(command),
        Commands::Plan { command } => cmd_plan(command),
        Commands::Api { command } => cmd_api(command),
        Commands::Cloud { command } => cmd_cloud(command),
        Commands::SyncMaster => cmd_sync_master(),
    }
}

fn print_goal_plan_result(result: Result<String, String>) -> i32 {
    match result {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(error) => {
            eprintln!("Plan-/Ziel-Fehler: {error}");
            2
        }
    }
}

fn cmd_cloud(command: cli::CloudCommands) -> i32 {
    use webagent::free_cloud_chat::{
        decide_route, default_registry, search_registry, stream_deterministic_mock, ModelProfile,
    };

    fn profile(value: &str) -> Result<ModelProfile, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(ModelProfile::Auto),
            "fast" | "schnell" => Ok(ModelProfile::Fast),
            "german" | "deutsch" => Ok(ModelProfile::German),
            "reasoning" | "code" | "reasoning-code" | "reasoning_code" => {
                Ok(ModelProfile::ReasoningCode)
            }
            "creative" | "kreativ" => Ok(ModelProfile::Creative),
            "privacy" | "datensparsam" => Ok(ModelProfile::Privacy),
            "custom" => Ok(ModelProfile::Custom),
            other => Err(format!(
                "Unbekanntes Profil '{other}'. Erlaubt: auto, schnell, deutsch, reasoning-code, kreativ, datensparsam, custom."
            )),
        }
    }

    let registry = default_registry();
    let result = match command {
        cli::CloudCommands::List => Ok(serde_json::json!({
            "schema_version": webagent::free_cloud_chat::REGISTRY_SCHEMA_VERSION,
            "free_only_default": true,
            "models": registry,
        })),
        cli::CloudCommands::Search {
            profile: requested_profile,
            query,
            allow_credits,
        } => profile(&requested_profile).map(|selected_profile| {
            serde_json::json!({
                "profile": selected_profile.label(),
                "query": query,
                "free_only": !allow_credits,
                "results": search_registry(&registry, selected_profile, &query, !allow_credits),
            })
        }),
        cli::CloudCommands::Decide {
            model_id,
            allow_credits,
        } => registry
            .iter()
            .find(|model| model.model_id == model_id)
            .map(|model| {
                Ok(serde_json::json!({
                    "free_only": !allow_credits,
                    "decision": decide_route(model, !allow_credits),
                }))
            })
            .unwrap_or_else(|| Err(format!("Unbekannte Registry-ID '{model_id}'."))),
        cli::CloudCommands::MockStream { prompt } => stream_deterministic_mock(&prompt)
            .map(|events| {
                serde_json::json!({
                    "adapter": "deterministic_local_mock",
                    "network_access": false,
                    "events": events,
                })
            })
            .map_err(|error| error.to_string()),
    };

    match result {
        Ok(value) => match serde_json::to_string_pretty(&value) {
            Ok(text) => {
                println!("{text}");
                0
            }
            Err(error) => {
                eprintln!("[cloud] JSON-Ausgabe fehlgeschlagen: {error}");
                2
            }
        },
        Err(error) => {
            eprintln!("[cloud] {error}");
            2
        }
    }
}
fn cmd_ui(bind: String, port: u16, no_open: bool) -> i32 {
    let ip: std::net::IpAddr = match bind.parse::<std::net::IpAddr>() {
        Ok(ip) if ip.is_loopback() => ip,
        Ok(_) => {
            eprintln!("[ui] Sicherheitsgrenze: --bind muss eine Loopback-Adresse sein.");
            return 2;
        }
        Err(error) => {
            eprintln!("[ui] Ungueltige Bind-Adresse {bind}: {error}");
            return 2;
        }
    };
    let config = webagent::web_ui::UiConfig {
        bind: std::net::SocketAddr::new(ip, port),
        open_browser: !no_open,
    };
    match webagent::web_ui::serve(config) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("[ui] {error}");
            1
        }
    }
}

fn cmd_api(command: cli::ApiCommands) -> i32 {
    match command {
        cli::ApiCommands::Serve {
            bind,
            port,
            brain,
            timeout_secs,
            headless,
            api_key_env,
        } => {
            if timeout_secs.is_some_and(|timeout| timeout <= 0.0) {
                eprintln!("[api] --timeout-secs muss groesser als 0 sein.");
                return 2;
            }

            let ip: std::net::IpAddr = match bind.parse::<std::net::IpAddr>() {
                Ok(ip) if ip.is_loopback() => ip,
                Ok(_) => {
                    eprintln!("[api] Sicherheitsgrenze: --bind muss eine Loopback-Adresse sein.");
                    return 2;
                }
                Err(error) => {
                    eprintln!("[api] Ungueltige Bind-Adresse {bind}: {error}");
                    return 2;
                }
            };
            let api_key = match std::env::var(&api_key_env) {
                Ok(value) if !value.trim().is_empty() => value,
                _ => {
                    eprintln!("[api] Token fehlt: setze die Umgebungsvariable {api_key_env}.");
                    return 2;
                }
            };
            let config = webagent::api_bridge::BridgeConfig {
                bind: std::net::SocketAddr::new(ip, port),
                brain,
                timeout_secs,
                headless,
                api_key,
                fake_reply: None,
            };

            match webagent::api_bridge::serve(config) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("[api] {error}");
                    1
                }
            }
        }
    }
}

fn cmd_goal(command: cli::GoalCommands) -> i32 {
    let data_dir = webagent::config::data_dir();
    match command {
        cli::GoalCommands::Create {
            objective,
            acceptance,
            scope,
        } => print_goal_plan_result(
            webagent::goal_plan::create_goal(&data_dir, objective, acceptance, scope)
                .and_then(|goal| webagent::goal_plan::render_json(&goal)),
        ),
        cli::GoalCommands::Get { json: _ } => print_goal_plan_result(
            webagent::goal_plan::active_goal(&data_dir).and_then(|goal| {
                let value = goal.map_or(serde_json::Value::Null, |goal| {
                    serde_json::to_value(goal).unwrap_or(serde_json::Value::Null)
                });
                webagent::goal_plan::render_json(&value)
            }),
        ),
        cli::GoalCommands::Complete {
            evidence,
            reviewer,
            verdict,
        } => print_goal_plan_result(
            webagent::goal_plan::complete_goal(&data_dir, evidence, reviewer, verdict)
                .and_then(|goal| webagent::goal_plan::render_json(&goal)),
        ),
        cli::GoalCommands::Abandon { reason } => print_goal_plan_result(
            webagent::goal_plan::abandon_goal(&data_dir, reason)
                .and_then(|goal| webagent::goal_plan::render_json(&goal)),
        ),
    }
}

fn cmd_plan(command: cli::PlanCommands) -> i32 {
    let data_dir = webagent::config::data_dir();
    match command {
        cli::PlanCommands::Create { title, item } => print_goal_plan_result(
            webagent::goal_plan::create_plan(&data_dir, title, item)
                .and_then(|plan| webagent::goal_plan::render_json(&plan)),
        ),
        cli::PlanCommands::Get { json: _ } => print_goal_plan_result(
            webagent::goal_plan::active_plan(&data_dir).and_then(|plan| {
                let value = plan.map_or(serde_json::Value::Null, |plan| {
                    serde_json::to_value(plan).unwrap_or(serde_json::Value::Null)
                });
                webagent::goal_plan::render_json(&value)
            }),
        ),
        cli::PlanCommands::Done { id } => print_goal_plan_result(
            webagent::goal_plan::complete_plan_item(&data_dir, id)
                .and_then(|plan| webagent::goal_plan::render_json(&plan)),
        ),
    }
}

#[cfg(all(windows, feature = "webview"))]
fn main() {
    process::exit(run());
}

#[cfg(not(all(windows, feature = "webview")))]
fn main() {
    let exit_code = match std::thread::Builder::new()
        .name("webagent-main".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(run)
    {
        Ok(handle) => handle.join().unwrap_or_else(|_| {
            eprintln!("[startup] WebAgent-Hauptthread ist unerwartet abgebrochen.");
            70
        }),
        Err(error) => {
            eprintln!("[startup] GroÃŸer WebAgent-Hauptthread konnte nicht starten: {error}");
            70
        }
    };
    process::exit(exit_code);
}
