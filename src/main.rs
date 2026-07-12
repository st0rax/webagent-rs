//! WebAgent CLI-Einstiegspunkt mit clap-basierter Befehlsstruktur.

use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::process;

use webagent::config::available_brain_ids;
use webagent::doctor::run_doctor;
use webagent::run_store::RunStore;
use webagent::watchdog::{run_watchdog, WatchdogReport};

#[derive(Parser)]
#[command(name = "webagent")]
#[command(about = "Gehirnunabhängiger lokaler Agent (Rust-Port)", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

fn main() {
    let cli = Cli::parse();

    let exit_code = match cli.command {
        Commands::Run {
            brain,
            task,
            resume,
            headless,
            max_cycles,
        } => cmd_run(&brain, &task, resume.as_deref(), headless, max_cycles),

        Commands::Doctor { brain, json } => cmd_doctor(
            if brain.is_empty() { None } else { Some(brain) },
            json,
        ),

        Commands::Watchdog {
            bot2bot_root,
            profile_dir,
            runs_dir,
            repair,
            json,
        } => cmd_watchdog(bot2bot_root, profile_dir, runs_dir, repair, json),

        Commands::MaintenanceCheck {
            json,
            pytest,
            pytest_timeout,
        } => cmd_maintenance_check(json, pytest, pytest_timeout),
    };

    process::exit(exit_code);
}

fn cmd_run(
    brain: &str,
    task: &str,
    resume: Option<&str>,
    headless: bool,
    max_cycles: u32,
) -> i32 {
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

    eprintln!(
        "[run] brain={} headless={} max_cycles={} — starte Chromium via CDP…",
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

fn cmd_doctor(brain_ids: Option<Vec<String>>, json: bool) -> i32 {
    // Konfiguration aus config.rs laden
    let brains_config = webagent::config::brains();
    
    // Runs-Verzeichnis aus config.rs
    let runs_dir = webagent::config::runs_dir()
        .to_string_lossy()
        .to_string();

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
            println!("[doctor] unhealthy: {}", report.unhealthy_brain_ids().join(", "));
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
                if check.selectors_mtime.is_empty() { "n/a" } else { &check.selectors_mtime }
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

    if report.ok() { 0 } else { 2 }
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

    let bot2bot_root = bot2bot_root.unwrap_or_else(|| {
        config::bot2bot_root().to_string_lossy().to_string()
    });
    let profile_dir = profile_dir.unwrap_or_else(|| {
        config::profiles_dir().join("shared").to_string_lossy().to_string()
    });
    let runs_dir = runs_dir.unwrap_or_else(|| {
        config::runs_dir().to_string_lossy().to_string()
    });

    let store = RunStore::new(
        config::runs_dir(),
        config::runs_dir().join("logs"),
    );

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
        println!("[watchdog] stale_bridge_locks: {}", report.stale_bridge_locks.len());
        println!("[watchdog] stale_profile_locks: {}", report.stale_profile_locks.len());
        if repair {
            println!("[watchdog] repaired_runs: {}", report.repaired_runs.len());
            println!("[watchdog] repaired_bridge_locks: {}", report.repaired_bridge_locks.len());
            println!("[watchdog] repaired_profile_locks: {}", report.repaired_profile_locks.len());
        }
        if !report.errors.is_empty() {
            println!("[watchdog] errors: {}", report.errors.join(", "));
        }
        println!();
    }

    if report.ok() && report.errors.is_empty() {
        0
    } else if repair && report.total_repaired() > 0 {
        0
    } else {
        2
    }
}

fn cmd_maintenance_check(json: bool, pytest: bool, pytest_timeout: f64) -> i32 {
    eprintln!("[maintenance-check] Noch nicht vollständig portiert.");
    eprintln!("[maintenance-check] json={}, pytest={}, pytest_timeout={}",
        json, pytest, pytest_timeout);
    eprintln!("[maintenance-check] Bitte verwende vorerst die Python-Version.");
    1
}
