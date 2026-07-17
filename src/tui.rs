//! tui — Teil 2: Terminal-UI als Default für `webagent` (kein Subcommand).
//!
//! Startet den Worker-Pool-Supervisor in einem Hintergrund-Thread und zeigt ein
//! Live-Dashboard: Brains + Status, aktive Worker, und — zentral für
//! "Aufgaben sinnvoll auf die Worker aufteilen" — ein Task-Board, das pro
//! aktivem Worker zeigt, welche Aufgabe er gerade bearbeitet.
//!
//! Steuerung erfolgt dateibasiert über `pool_control.json` (target_active /
//! reflag / stop) — passend zur dateibasierten bot2bot-Philosophie, ohne neue
//! Crates (reines std + bereits vorhandenes `time`/`serde_json`). Das Routing
//! einer Aufgabe an einen bestimmten Worker (`send <brain> <text>`) legt die
//! Nachricht exakt im Format von `send.ps1` in dessen Inbox ab; der Worker
//! holt sie im nächsten Poll-Zyklus ab.
//!
//! ## Feature-"tui"
//!
//! Mit `--features tui` wird eine ratatui-basierte TUI mit crossterm-Events
//! verwendet (3-Pane-Layout, Agentenauswahl, Live-Refresh). Ohne das Feature
//! fällt die Implementierung auf die ANSI-TUI (readline-basiert) zurück.

use std::fs;
use std::io::{self};
use std::path::Path;
use std::thread;

use crate::config::{available_brain_ids, bot2bot_root};
use crate::worker_pool::{
    candidates_with_profile, PoolControl, WorkerPool,
};

#[cfg(not(feature = "tui"))]
use std::io::{BufRead, Write};

#[cfg(not(feature = "tui"))]
use crate::worker_pool::{PoolState, STATUS_ACTIVE};

// ---------------------------------------------------------------------------
// ANSI-TUI-Hilfsmittel (nur ohne ratatui benötigt)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "tui"))]
const CLEAR: &str = "\x1b[2J\x1b[H";
#[cfg(not(feature = "tui"))]
const RESET: &str = "\x1b[0m";

#[cfg(not(feature = "tui"))]
fn status_color(status: &str) -> &'static str {
    match status {
        STATUS_ACTIVE => "\x1b[32m",        // grün
        "available" => "\x1b[33m",           // gelb
        "unavailable" => "\x1b[31m",         // rot
        _ => "",
    }
}

// ---------------------------------------------------------------------------
// Gemeinsame Hilfsfunktionen (beide TUI-Varianten)
// ---------------------------------------------------------------------------

/// Datei-Zeitstempel im `send.ps1`-Format: `yyyyMMddTHHmmss` (UTC).
fn file_stamp() -> String {
    let t = time::OffsetDateTime::now_utc();
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}",
        t.year(),
        t.month() as u8,
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// ISO-8601-Zeitstempel (UTC) für das `Time:`-Feld der Nachricht.
fn iso_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Schreibt einen Steuerbefehl nach `pool_control.json`.
fn write_control(path: &Path, control: &PoolControl) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(control) {
        let _ = fs::write(path, s);
    }
}

/// Legt eine Aufgabe im Inbox-Format von `send.ps1` ab -> Worker holt sie ab.
/// Liefert `Err`, wenn der Ziel-Agent keine Inbox hat (nicht registriert).
fn send_task(root: &Path, brain: &str, from: &str, text: &str) -> std::io::Result<()> {
    let inbox = root.join("agents").join(brain).join("inbox");
    if !inbox.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Agent '{brain}' hat keine Inbox (nicht registriert)"),
        ));
    }
    let ts = file_stamp();
    let file = inbox.join(format!("{ts}_from_{from}.msg.txt"));
    let content = format!(
        "From: {from}\nTo: {brain}\nTime: {}\n\n{text}\n",
        iso_now()
    );
    fs::write(file, content)
}

// ---------------------------------------------------------------------------
// ANSI-TUI-Hilfsfunktionen (nur ohne ratatui)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "tui"))]
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

#[cfg(not(feature = "tui"))]
fn newest_msg(inbox: &Path) -> Option<(String, String, bool)> {
    let read = inbox.join("_read");
    let mut best: Option<(std::time::SystemTime, String, String, bool)> = None;
    consider_dir(&mut best, inbox, false);
    consider_dir(&mut best, &read, true);
    best.map(|(_, name, body, done)| (name, body, done))
}

/// Erste nicht-Header-Zeile einer Nachricht als Vorschau.
#[cfg(not(feature = "tui"))]
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
#[cfg(not(feature = "tui"))]
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

// ---------------------------------------------------------------------------
// ANSI-Render (nur ohne ratatui)
// ---------------------------------------------------------------------------

/// Rendert das Dashboard (Brains + Status + Task-Board) auf stdout.
#[cfg(not(feature = "tui"))]
fn render(
    root: &Path,
    state_path: &Path,
    candidates: &[String],
    target_active: usize,
    control_path: &Path,
) {
    print!("{CLEAR}");
    let active = current_active(state_path);
    println!("=== webagent Worker-Pool TUI ===");
    println!(
        "Ziel aktive Worker: {target_active}   Aktuell aktiv: {active}   Kandidaten: {}",
        candidates.len()
    );
    println!("Steuer-IPC: {}", control_path.display());
    println!();
    println!(
        "{:<10} {:<12} {:<14} Aktuelle Aufgabe",
        "Brain", "Status", "Fehler"
    );
    println!("{}", "-".repeat(78));

    let state = if let Ok(s) = fs::read_to_string(state_path) {
        serde_json::from_str::<PoolState>(&s).unwrap_or_default()
    } else {
        PoolState::default()
    };

    for brain in candidates {
        let inbox = root.join("agents").join(brain).join("inbox");
        let task = newest_msg(&inbox)
            .map(|(name, body, done)| {
                let tag = if done { "✓ " } else { "• " };
                if body.is_empty() {
                    format!("{tag}{name}")
                } else {
                    format!("{tag}{name}: \"{body}\"")
                }
            })
            .unwrap_or_else(|| "—".to_string());

        let (status, err) = match state.entries.get(brain) {
            Some(e) => (e.status.clone(), e.last_error.clone()),
            None => ("available".to_string(), String::new()),
        };
        println!(
            "{:<10} {}{:<12}{} {:<14} {}",
            brain,
            status_color(&status),
            status,
            RESET,
            err.chars().take(13).collect::<String>(),
            task.chars().take(48).collect::<String>()
        );
    }

    println!();
    println!(
        "Befehle:  + aktiver   - weniger   r alle verfuegbar   send <brain> <text>   q quit"
    );
}

// ---------------------------------------------------------------------------
// ANSI-TUI (Fallback ohne ratatui)
// ---------------------------------------------------------------------------

#[cfg(not(feature = "tui"))]
fn run_tui_ansi(active: usize, brains: &str, poll_secs: u64, headless: bool) -> i32 {
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
    let _ = fs::remove_file(&control_path);

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

    println!("{CLEAR}webagent TUI startet Worker-Pool … (q zum Beenden)");
    let mut target_active = active.min(candidates.len());
    loop {
        render(&root, &state_path, &candidates, target_active, &control_path);
        print!("> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).is_err() {
            // Eingabe unterbrochen -> sauber beenden.
            write_control(
                &control_path,
                &PoolControl { stop: true, ..Default::default() },
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
                    &PoolControl { stop: true, ..Default::default() },
                );
                break;
            }
            "+" => {
                target_active = (target_active + 1).min(candidates.len());
                write_control(
                    &control_path,
                    &PoolControl { target_active, ..Default::default() },
                );
            }
            "-" => {
                target_active = target_active.saturating_sub(1);
                write_control(
                    &control_path,
                    &PoolControl { target_active, ..Default::default() },
                );
            }
            "r" => {
                write_control(
                    &control_path,
                    &PoolControl { reflag_all: true, ..Default::default() },
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
    0
}

// ---------------------------------------------------------------------------
// ratatui-TUI (mit Feature "tui")
// ---------------------------------------------------------------------------

#[cfg(feature = "tui")]
fn run_tui_ratatui(active: usize, brains: &str, poll_secs: u64, headless: bool) -> i32 {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
    use crossterm::ExecutableCommand;
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;

    use crate::tui_render::ui;
    use crate::tui_state::{load_state, select_wrap, App, InputMode};

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
    // Stale Steuerdatei vom vorigen Lauf verwerfen.
    let _ = fs::remove_file(&control_path);

    if candidates.is_empty() {
        eprintln!(
            "[tui] Keine Kandidaten mit Browser-Profil gefunden (--brains={brains:?}). \
             Zuerst ein Profil einloggen (doctor/login)."
        );
        return 2;
    }

    // --- crossterm raw mode + alternate screen ---
    if let Err(e) = terminal::enable_raw_mode() {
        eprintln!("[tui] Konnte raw mode nicht aktivieren: {e}");
        return 1;
    }
    let mut stdout = io::stdout();
    if let Err(e) = stdout.execute(EnterAlternateScreen) {
        let _ = terminal::disable_raw_mode();
        eprintln!("[tui] Konnte alternate screen nicht aktivieren: {e}");
        return 1;
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            let _ = terminal::disable_raw_mode();
            eprintln!("[tui] Konnte Terminal nicht erstellen: {e}");
            return 1;
        }
    };

    // --- Worker-Pool im Hintergrund-Thread ---
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

    // --- App initialisieren ---
    let target_active = active.min(candidates.len());
    let mut app = App {
        agents: load_state(false),
        selected: 0,
        tick: 0,
        log_scroll: 0,
        input_mode: InputMode::Normal,
        target_active,
        gauge_shown: 0.0,
    };

    // --- Event-Loop ---
    let tick_rate = std::time::Duration::from_millis(80);
    // Refresh alle `poll_secs * 12.5` Ticks (bei 80ms Tick = ~poll_secs Sekunden).
    let refresh_ticks = (poll_secs as f64 * 12.5).ceil() as u64;
    let mut frame_count = 0u64;
    let mut task_input = String::new();

    let exit_code = loop {
        // Tastatur-Event (non-blocking, 80ms Timeout)
        if event::poll(tick_rate).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Up => {
                            app.selected =
                                select_wrap(app.selected, -1, app.agents.len());
                        }
                        KeyCode::Down => {
                            app.selected =
                                select_wrap(app.selected, 1, app.agents.len());
                        }
                        KeyCode::Char('q') => break 0,
                        KeyCode::Char('+') => {
                            app.target_active =
                                (app.target_active + 1).min(candidates.len());
                            write_control(
                                &control_path,
                                &PoolControl {
                                    target_active: app.target_active,
                                    ..Default::default()
                                },
                            );
                        }
                        KeyCode::Char('-') => {
                            app.target_active = app.target_active.saturating_sub(1);
                            write_control(
                                &control_path,
                                &PoolControl {
                                    target_active: app.target_active,
                                    ..Default::default()
                                },
                            );
                        }
                        KeyCode::Char('r') => {
                            write_control(
                                &control_path,
                                &PoolControl {
                                    reflag_all: true,
                                    ..Default::default()
                                },
                            );
                        }
                        KeyCode::Enter => {
                            app.input_mode = InputMode::TaskInput;
                            task_input.clear();
                        }
                        _ => {}
                    },
                    InputMode::TaskInput => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            task_input.clear();
                        }
                        KeyCode::Enter => {
                            if !task_input.is_empty() {
                                let mut parts = task_input.splitn(2, ' ');
                                let brain = parts.next().unwrap_or("").trim();
                                let text = parts.next().unwrap_or("").trim();
                                if !brain.is_empty()
                                    && !text.is_empty()
                                    && candidates.iter().any(|c| c == brain)
                                {
                                    let _ = send_task(&root, brain, "tui", text);
                                }
                            }
                            app.input_mode = InputMode::Normal;
                            task_input.clear();
                        }
                        KeyCode::Backspace => {
                            task_input.pop();
                        }
                        KeyCode::Char(c) => {
                            task_input.push(c);
                        }
                        _ => {}
                    },
                    InputMode::ConfirmQuit => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => break 0,
                        KeyCode::Char('n') | KeyCode::Esc => {
                            app.input_mode = InputMode::Normal
                        }
                        _ => {}
                    },
                }
            }
        }

        // Tick (Spinner + gedämpftes Gauge)
        let gauge_target = app.target_active as f32 / candidates.len().max(1) as f32;
        app.on_tick(gauge_target);
        frame_count += 1;

        // Periodischer State-Refresh
        if frame_count % refresh_ticks.max(1) == 0 || frame_count == 1 {
            app.agents = load_state(false);
            app.selected = app.selected.min(app.agents.len().saturating_sub(1));
        }

        // Rendern
        if let Err(e) = terminal.draw(|f| ui(f, &app)) {
            eprintln!("[tui] Render-Fehler: {e}");
            break 1;
        }
    };

    // --- Cleanup ---
    write_control(
        &control_path,
        &PoolControl { stop: true, ..Default::default() },
    );
    let _ = handle.join();
    let _ = terminal::disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    exit_code
}

// ---------------------------------------------------------------------------
// Öffentlicher Einstiegspunkt (dispatcht je nach Feature)
// ---------------------------------------------------------------------------

/// Einstiegspunkt der TUI (Default, wenn `webagent` ohne Subcommand läuft).
pub fn run_tui(active: usize, brains: &str, poll_secs: u64, headless: bool) -> i32 {
    #[cfg(feature = "tui")]
    {
        run_tui_ratatui(active, brains, poll_secs, headless)
    }
    #[cfg(not(feature = "tui"))]
    {
        run_tui_ansi(active, brains, poll_secs, headless)
    }
}
