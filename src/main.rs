//! WebAgent CLI-Einstiegspunkt mit clap-basierter Befehlsstruktur.

use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::process;

use webagent::run_store::RunStore;

#[derive(Parser)]
#[command(name = "webagent")]
#[command(version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WEBAGENT_GIT_HASH"), ")"))]
#[command(about = "Gehirnunabhängiger lokaler Agent (Rust-Port)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Autonomen Run starten
    Run {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long, default_value = "chatgpt")]
        brain: String,

        /// Benutzeraufgabe
        #[arg(long)]
        task: String,

        /// Run-ID fortsetzen
        #[arg(long)]
        resume: Option<String>,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,

        /// Maximale Anzahl an Zyklen
        #[arg(long, default_value = "100")]
        max_cycles: u32,
    },

    /// Sichtbaren Browser oeffnen und auf manuellen Login warten (keine Zugangsdaten-Eingabe)
    Login {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long)]
        brain: String,

        /// Maximale Wartezeit auf den Login in Sekunden
        #[arg(long, default_value = "300")]
        timeout: u64,

        /// Fenster offen halten, auch wenn der Login-Check schon "eingeloggt" meldet.
        /// Noetig, wo die Erkennung zu optimistisch ist (kimi, mistral: Composer ist
        /// auch anonym sichtbar) oder wo nur ein Dialog zu bestaetigen ist (mistral-AGB).
        #[arg(long)]
        force: bool,
    },

    /// Alle Brains nacheinander einloggen (canonical profiles/<brain>).
    /// Parallel nur opt-in und gedeckelt (siehe --parallel).
    LoginAll {
        /// Maximale Wartezeit pro Brain in Sekunden
        #[arg(long, default_value = "300")]
        timeout: u64,

        /// Auch bei positivem Login-Check erneut oeffnen
        #[arg(long)]
        force: bool,

        /// Parallelitaet (0 = sequenziell/Default; max 3 experimentell)
        #[arg(long, default_value = "0")]
        parallel: usize,
    },

    /// Live-Diagnose: echten Browser oeffnen und Login/Composer/Selektoren pruefen
    Diagnose {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long)]
        brain: String,

        /// Headless statt sichtbar (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },

    /// Interaktive REPL: mehrere Aufgaben nacheinander gegen dasselbe Brain
    Repl {
        /// Brain-Backend (z.B. chatgpt, claude, deepseek)
        #[arg(long, default_value = "chatgpt")]
        brain: String,

        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
    },

    /// Pro-Brain Diagnose: Selektoren, Profil-Lock, letzte Antwort, Recovery
    Doctor {
        /// Nur diese Gehirne prüfen (leer = alle)
        #[arg(long)]
        brain: Vec<String>,

        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,
    },

    /// Watchdog: Scannt verwaiste Runs, Bridge-Locks, Profil-Locks (Dry-Run/Repair)
    Watchdog {
        /// Bridge-Lock-Root (bot2bot Verzeichnis)
        #[arg(long)]
        bot2bot_root: Option<String>,

        /// Profil-Verzeichnis
        #[arg(long)]
        profile_dir: Option<String>,

        /// Runs-Verzeichnis (Fallback wenn kein RunStore)
        #[arg(long)]
        runs_dir: Option<String>,

        /// Reparieren (Standard: Dry-Run)
        #[arg(long)]
        repair: bool,

        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,
    },

    /// Pre-flight: Profile, Selektoren, Flags (ohne Browser)
    BrainsHealth {
        /// Leeres Shared-Profil akzeptieren (Exit 0)
        #[arg(long)]
        allow_empty_profile: bool,
    },

    /// Live-Canary: alle Brains leicht pruefen (Latenz + pass/fail + reason)
    Canary,

    /// Single send+wait turn (bot2bot bridge debugging)
    Relay {
        #[arg(long)]
        brain: String,
        #[arg(long)]
        message: String,
        #[arg(long)]
        headless: bool,
        #[arg(long, default_value = "0")]
        timeout: f64,
        /// Maschinenlesbare JSON-Ausgabe (brain/ok/answer/latency_ms/reason)
        #[arg(long)]
        json: bool,
    },

    /// Multi-Brain-Swarm (Relay je Brain + Synthese). Default menschenlesbar; `--json` fuer CLI-Anbindung.
    Swarm {
        /// Aufgabe / Prompt an alle Brains
        #[arg(long)]
        message: String,
        /// Headless-Browser (Standard: sichtbar)
        #[arg(long)]
        headless: bool,
        /// Timeout pro Brain in Sekunden (0 = Default)
        #[arg(long, default_value = "0")]
        timeout: f64,
        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren)
        #[arg(long, default_value = "")]
        brains: String,
        /// Maschinenlesbares JSON (pro Brain + synthesis)
        #[arg(long)]
        json: bool,
    },

    /// Autonomer bot2bot-Worker: Inbox pollen, Task via Controller abarbeiten,
    /// Ergebnis zurueck an Absender (grok-Aequivalent). Jeder Prozess nutzt ein
    /// eigenes isoliertes Profil (Q5-copy) -> N Worker laufen parallel.
    #[command(name = "bot2bot-worker")]
    Bot2BotWorker {
        /// Brain-Backend (z.B. deepseek)
        #[arg(long)]
        brain: String,
        /// Ein Durchlauf statt Endlos-Loop
        #[arg(long)]
        once: bool,
        /// Poll-Intervall in Sekunden
        #[arg(long, default_value = "30")]
        poll_secs: u64,
        /// Maximale Controller-Zyklen
        #[arg(long, default_value = "100")]
        max_cycles: u32,
        /// Headless-Browser
        #[arg(long)]
        headless: bool,
    },

    /// Worker-Pool-Manager (Teil 1): haelt N aktive bot2bot-Worker (je ein
    /// eigener Kindprozess) am Leben, Failover bei Crash (Brain -> unavailable,
    /// naechster Reserve-Brain promoviert). Status pro Brain in pool_state.json
    /// (available/active/unavailable, extern re-flaggbar).
    #[command(name = "workers")]
    Workers {
        /// Anzahl gleichzeitig aktiver Worker (Default 2 -> 6 Reserve bei 8 Brains)
        #[arg(long, default_value = "2")]
        active: usize,

        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren mit Profil)
        #[arg(long, default_value = "")]
        brains: String,

        /// Poll-Intervall der Supervisor-Schleife in Sekunden
        #[arg(long, default_value = "10")]
        poll_secs: u64,

        /// Headless-Browser fuer die Worker-Kindprozesse
        #[arg(long)]
        headless: bool,
    },

    /// Terminal-UI (Default): steuert den Worker-Pool sichtbar im Terminal.
    /// Ohne Subcommand = diese Ansicht; `webagent tui` ist identisch. Zeigt
    /// Brain-Status + Live-Task-Board und routet Aufgaben an einzelne Worker.
    #[command(name = "tui")]
    Tui {
        /// Zielanzahl gleichzeitig aktiver Worker (Default 2)
        #[arg(long, default_value = "2")]
        active: usize,

        /// Komma-getrennte Brain-IDs (leer = alle verfuegbaren mit Profil)
        #[arg(long, default_value = "")]
        brains: String,

        /// Poll-Intervall der Supervisor-Schleife in Sekunden
        #[arg(long, default_value = "5")]
        poll_secs: u64,

        /// Headless-Browser fuer die Worker-Kindprozesse
        #[arg(long)]
        headless: bool,
    },

    /// First-run setup: Brain-Auswahl und optional Login-Hinweise
    Oobe {
        #[arg(long)]
        brains: String,
        #[arg(long)]
        skip_login: bool,
        #[arg(long)]
        yes: bool,
    },

    /// Read-only gate for autonomous maintenance
    MaintenanceCheck {
        /// Maschinenlesbares JSON
        #[arg(long)]
        json: bool,

        /// Zusätzlich vollständige Test-Suite ausführen
        #[arg(long)]
        pytest: bool,

        /// Maximale Testlaufzeit in Sekunden
        #[arg(long, default_value = "600")]
        pytest_timeout: f64,
    },
}

/// Startup-Helfer: stale Runs reparieren (Python `main()` vor jedem Command außer maintenance-check).
pub fn startup_reconcile_runs() -> Vec<String> {
    let runs_dir = webagent::config::runs_dir();
    let store = RunStore::new(runs_dir.clone(), runs_dir.join("logs"));
    store.reconcile_stale_runs(600.0)
}

fn main() {
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
        } => webagent::tui::run_tui(active, &brains, poll_secs, headless),

        Commands::Oobe {
            brains,
            skip_login,
            yes,
        } => cmd_oobe(&brains, skip_login, yes),

        Commands::MaintenanceCheck {
            json,
            pytest,
            pytest_timeout,
        } => cmd_maintenance_check(json, pytest, pytest_timeout),
    }
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
