//! tui_ansi — ANSI-Fallback-TUI (ohne Feature `tui`).
//!
//! Aus `tui.rs` herausgeloest (Refactoring 02:45): der ANSI-Zweig hat mit der
//! ratatui-TUI nur die dateibasierte Steuerung gemeinsam (`write_control`,
//! `send_task` u. a., bleiben in `tui.rs`). Rendering, Statusfarben und
//! Inbox-Vorschau hier sind rein ANSI-spezifisch und waren als
//! zusammenhängender Block der erste Kandidat fuer einen Schnitt aus der
//! TUI-Masse (tui.rs + tui_state.rs + tui_render.rs = ~4400 Zeilen).

use std::fs;
use std::io::{self};
use std::io::{BufRead, Write};
use std::path::Path;
use std::thread;

use crate::config::{available_brain_ids, bot2bot_root};
use crate::tui::{discard_stale_control, send_task, spawn_benchmark_from_tui, write_control};
use crate::worker_pool::{
    candidates_with_profile, PoolControl, PoolState, WorkerPool, STATUS_ACTIVE,
};

const CLEAR: &str = "\x1b[2J\x1b[H";
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";

fn status_color(status: &str) -> &'static str {
    match status {
        STATUS_ACTIVE => "\x1b[32m", // grün
        "available" => "\x1b[33m",   // gelb
        "unavailable" => "\x1b[31m", // rot
        "retired" => "\x1b[35m",     // magenta (dauerhaft ausgemustert)
        _ => "",
    }
}

/// Statuspunkt für die schnelle Erfassung in der ANSI-Tabelle.
fn status_glyph(status: &str) -> &'static str {
    match status {
        STATUS_ACTIVE => "●",
        "available" => "○",
        "unavailable" => "✕",
        "retired" => "◌",
        _ => "·",
    }
}

fn consider_dir(
    best: &mut Option<(std::time::SystemTime, String, String, bool)>,
    dir: &Path,
    done: bool,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("txt") {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if let Ok(modified) = m.modified() {
                let name = e.file_name().to_string_lossy().to_string();
                let body = preview_body(&p);
                let cand = (modified, name, body, done);
                *best = match best.take() {
                    None => Some(cand),
                    Some(b) => {
                        if cand.0 > b.0 {
                            Some(cand)
                        } else {
                            Some(b)
                        }
                    }
                };
            }
        }
    }
}

fn newest_msg(inbox: &Path) -> Option<(String, String, bool)> {
    let read = inbox.join("_read");
    let mut best: Option<(std::time::SystemTime, String, String, bool)> = None;
    consider_dir(&mut best, inbox, false);
    consider_dir(&mut best, &read, true);
    best.map(|(_, name, body, done)| (name, body, done))
}

/// Erste nicht-Header-Zeile einer Nachricht als Vorschau.
fn preview_body(path: &Path) -> String {
    if let Ok(s) = fs::read_to_string(path) {
        for line in s.lines() {
            let t = line.trim();
            if !t.is_empty()
                && !t.starts_with("From:")
                && !t.starts_with("To:")
                && !t.starts_with("Time:")
                && !t.starts_with("Subject:")
            {
                return t.chars().take(64).collect();
            }
        }
    }
    String::new()
}

/// Aktuelle Anzahl aktiver Worker aus `pool_state.json`.
fn current_active(state_path: &Path) -> usize {
    if let Ok(s) = fs::read_to_string(state_path) {
        if let Ok(st) = serde_json::from_str::<PoolState>(&s) {
            return st
                .entries
                .values()
                .filter(|e| e.status == STATUS_ACTIVE)
                .count();
        }
    }
    0
}

/// Rendert das Dashboard (Brains + Status + Task-Board) auf stdout.
fn render(
    root: &Path,
    state_path: &Path,
    candidates: &[String],
    target_active: usize,
    control_path: &Path,
) {
    print!("{CLEAR}");
    let active = current_active(state_path);
    let width = 74;

    // Kopf: Titel + Kennzahlen, klar abgesetzt.
    println!("{BOLD}{CYAN}  webagent · Worker-Pool{RESET}");
    println!(
        "  {DIM}aktiv{RESET} {BOLD}{active}{RESET}{DIM}/{target_active} Ziel{RESET}   \
         {DIM}Kandidaten{RESET} {BOLD}{}{RESET}   {DIM}IPC {}{RESET}",
        candidates.len(),
        control_path.display()
    );
    println!("  {DIM}{}{RESET}", "─".repeat(width));
    println!(
        "  {BOLD}{:<12}{:<10}{:<16}Aktuelle Aufgabe{RESET}",
        "Brain", "Status", "Fehler"
    );
    println!("  {DIM}{}{RESET}", "─".repeat(width));

    let state = if let Ok(s) = fs::read_to_string(state_path) {
        serde_json::from_str::<PoolState>(&s).unwrap_or_default()
    } else {
        PoolState::default()
    };

    for brain in candidates {
        let inbox = root.join("agents").join(brain).join("inbox");
        let task = newest_msg(&inbox)
            .map(|(name, body, done)| {
                let tag = if done { "✓" } else { "▸" };
                if body.is_empty() {
                    format!("{tag} {name}")
                } else {
                    format!("{tag} {body}")
                }
            })
            .unwrap_or_else(|| format!("{DIM}—{RESET}"));

        let (status, err) = match state.entries.get(brain) {
            Some(e) => (e.status.clone(), e.last_error.clone()),
            None => ("available".to_string(), String::new()),
        };
        let col = status_color(&status);
        let err_short: String = err.chars().take(14).collect();
        let task_short: String = task.chars().take(34).collect();
        // Statuspunkt + Name farbig, Rest ruhig — schnelle Zeilen-Erfassung.
        println!(
            "  {col}{} {:<10}{RESET}{col}{:<10}{RESET}{DIM}{:<16}{RESET}{}",
            status_glyph(&status),
            brain,
            status,
            err_short,
            task_short
        );
    }

    println!("  {DIM}{}{RESET}", "─".repeat(width));
    // Befehle: Tasten hervorgehoben, Beschriftung gedämpft.
    let cmd = |k: &str, label: &str| format!("{BOLD}{CYAN}{k}{RESET}{DIM} {label}{RESET}");
    println!(
        "  {}   {}   {}   {}   {}",
        cmd("+", "mehr"),
        cmd("-", "weniger"),
        cmd("r", "reflag"),
        cmd("send <brain> <text>", ""),
        cmd("q", "quit")
    );
}

/// ANSI-Fallback-TUI (ohne Feature `tui`): readline-basierte Steuerung.
pub(crate) fn run_tui_ansi(
    active: usize,
    brains: &str,
    poll_secs: u64,
    headless: bool,
    startup_benchmark: Option<&str>,
) -> i32 {
    let all = available_brain_ids();
    let selected: Vec<String> = if brains.trim().is_empty() {
        all
    } else {
        brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let candidates = candidates_with_profile(&selected);
    let root = bot2bot_root();
    let state_path = root.join("workers").join("pool_state.json");
    let control_path = root.join("workers").join("pool_control.json");
    // Stale Steuerdatei vom vorigen Lauf verwerfen (z.B. stop:true), damit ein
    // Relaunch nicht sofort wieder beendet.
    discard_stale_control(&control_path);

    if candidates.is_empty() {
        eprintln!(
            "[tui] Keine Kandidaten mit Browser-Profil gefunden (--brains={brains:?}). \
             Zuerst ein Profil einloggen (doctor/login)."
        );
        return 2;
    }

    // Supervisor im Hintergrund-Thread starten; TUI-Steuerung läuft dateibasiert.
    let mut pool = WorkerPool::new(
        candidates.clone(),
        active,
        poll_secs,
        headless,
        state_path.clone(),
        control_path.clone(),
    );
    let handle = thread::spawn(move || {
        pool.run();
    });

    // Benchmark beim Oeffnen genauso starten wie in der ratatui-Variante:
    // `run_tui` reichte das Startup-Argument bisher nur dorthin durch, der
    // ANSI-Fallback (aktiver Sonderweg, sobald stdout umgeleitet/detached
    // ist) liess den Benchmark nie laufen. Beobachtet 2026-08-02: nach der
    // Tee-Protokoll-Umstellung hing die TUI nur im Pool-Dashboard, die
    // brain_score-Events blieben aus.
    if let Some(arguments) = startup_benchmark.filter(|value| !value.trim().is_empty()) {
        let command = format!("/benchmark {arguments}");
        spawn_benchmark_from_tui(&command, &candidates);
        println!("[tui] Benchmark gestartet: {arguments}");
    }

    println!("{CLEAR}webagent TUI startet Worker-Pool … (q zum Beenden)");
    let mut target_active = active.min(candidates.len());
    loop {
        render(
            &root,
            &state_path,
            &candidates,
            target_active,
            &control_path,
        );
        print!("> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).is_err() {
            // Eingabe unterbrochen -> sauber beenden.
            write_control(
                &control_path,
                &PoolControl {
                    stop: true,
                    ..Default::default()
                },
            );
            break;
        }
        let input = line.trim();
        let mut parts = input.splitn(3, ' ');
        let cmd = parts.next().unwrap_or("");
        match cmd {
            "q" | "quit" => {
                write_control(
                    &control_path,
                    &PoolControl {
                        stop: true,
                        ..Default::default()
                    },
                );
                break;
            }
            "+" => {
                target_active = (target_active + 1).min(candidates.len());
                write_control(
                    &control_path,
                    &PoolControl {
                        target_active: Some(target_active),
                        ..Default::default()
                    },
                );
            }
            "-" => {
                target_active = target_active.saturating_sub(1);
                write_control(
                    &control_path,
                    &PoolControl {
                        target_active: Some(target_active),
                        ..Default::default()
                    },
                );
            }
            "r" => {
                write_control(
                    &control_path,
                    &PoolControl {
                        reflag_all: true,
                        ..Default::default()
                    },
                );
                println!("→ alle Kandidaten auf 'available' zurückgesetzt (nächster Tick).");
            }
            "send" => {
                let brain = parts.next().unwrap_or("").trim();
                let text = parts.next().unwrap_or("").trim();
                if brain.is_empty() || text.is_empty() {
                    println!("→ Nutzung: send <brain> <text>");
                } else if candidates.iter().any(|c| c == brain) {
                    match send_task(&root, brain, "tui", text) {
                        Ok(()) => println!("→ Aufgabe an '{brain}' geroutet (Inbox)."),
                        Err(e) => eprintln!("→ Fehler: {e}"),
                    }
                } else {
                    println!(
                        "→ '{brain}' ist kein Kandidat (Profile fehlt?). Verfügbar: {:?}",
                        candidates
                    );
                }
            }
            "h" | "help" | "" => {
                println!(
                    "Befehle:\n  + / -   Zielanzahl aktiver Worker erhöhen/verringern\n  r        alle Kandidaten auf 'available' (nach Fix/Stall)\n  send <brain> <text>  Aufgabe an einen bestimmten Worker routen\n  q / quit Beenden (Worker werden sauber gekillt)"
                );
            }
            other => {
                eprintln!("→ unbekannt: {other} (h für Hilfe)");
            }
        }
    }

    let _ = handle.join();
    // Browser-Pool sauber herunterfahren, BEVOR der Prozess endet: nur so
    // schliessen sich die WebView-Tabs geordnet und `write_back_session_to_master`
    // spielt die Sitzung ins Master-Profil zurueck. Ohne diesen Schritt bleibt
    // das Master eingefroren (Tabs offen -> teardown_runtime feuert nie), und
    // der naechste Start klont wieder den alten Login-Stand.
    // Der Benchmark-Thread laeuft detached weiter; seine naechste
    // Pool-Operation wartet kurz auf der Sperre, waehrend wir hier beenden.
    if let Ok(mut pool) = crate::browser_pool::BrowserPool::global().lock() {
        pool.shutdown();
    }
    0
}
