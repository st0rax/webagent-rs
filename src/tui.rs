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
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use crate::config::{available_brain_ids, bot2bot_root};
use crate::worker_pool::{atomic_write, candidates_with_profile, PoolControl, WorkerPool};

use std::io::{BufRead, Write};

use crate::worker_pool::{PoolState, STATUS_ACTIVE};

// ---------------------------------------------------------------------------
// ANSI-TUI-Hilfsmittel (nur ohne ratatui benötigt)
// ---------------------------------------------------------------------------

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
    let result = serde_json::to_vec_pretty(control)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
        .and_then(|json| atomic_write(path, &json));
    if let Err(e) = result {
        crate::bench_events::emit(
            crate::bench_events::Level::Fail,
            None,
            &format!("Pool-Steuerung konnte nicht gespeichert werden: {e}"),
        );
        crate::bench_events::eprint_line(&format!(
            "[tui] Pool-Steuerung konnte nicht gespeichert werden: {e}"
        ));
    }
}

fn discard_stale_control(path: &Path) {
    match PoolControl::take(path) {
        Ok(Some(_)) | Ok(None) => {}
        Err(e) => crate::bench_events::eprint_line(&format!(
            "[tui] Stale Control-Datei konnte nicht verworfen werden: {e}"
        )),
    }
}

/// Legt eine Aufgabe im Inbox-Format von `send.ps1` ab -> Worker holt sie ab.
/// Liefert `Err`, wenn der Ziel-Agent keine Inbox hat (nicht registriert).
fn send_task(root: &Path, brain: &str, from: &str, text: &str) -> std::io::Result<()> {
    static NEXT_MESSAGE: AtomicU64 = AtomicU64::new(0);

    let inbox = root.join("agents").join(brain).join("inbox");
    if !inbox.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Agent '{brain}' hat keine Inbox (nicht registriert)"),
        ));
    }
    let ts = file_stamp();
    let now_ns = time::OffsetDateTime::now_utc().unix_timestamp_nanos();
    let seq = NEXT_MESSAGE.fetch_add(1, Ordering::Relaxed);
    let file = inbox.join(format!(
        "{ts}_{now_ns}_{}_{}_from_{from}.msg.txt",
        std::process::id(),
        seq
    ));
    let content = format!("From: {from}\nTo: {brain}\nTime: {}\n\n{text}\n", iso_now());
    atomic_write(&file, content.as_bytes())
}

// ---------------------------------------------------------------------------
// ANSI-TUI-Hilfsfunktionen (nur ohne ratatui)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ANSI-Render (nur ohne ratatui)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ANSI-TUI (Fallback ohne ratatui)
// ---------------------------------------------------------------------------

fn run_tui_ansi(
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
    0
}

// ---------------------------------------------------------------------------
// ratatui-TUI (mit Feature "tui")
// ---------------------------------------------------------------------------

#[cfg(feature = "tui")]
fn run_tui_ratatui(
    active: usize,
    brains: &str,
    poll_secs: u64,
    headless: bool,
    startup_benchmark: Option<&str>,
    startup_view: Option<&str>,
) -> i32 {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
    use crossterm::ExecutableCommand;
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;

    use crate::tui_render::ui;
    use crate::tui_state::{
        load_state, parse_view, select_wrap, App, InputMode, LogFilter, Panel, View,
    };

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
    discard_stale_control(&control_path);

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
    // VT-Verarbeitung ERNEUT setzen: `enable_raw_mode` und
    // `EnterAlternateScreen` schreiben den Konsolenmodus neu und koennen das
    // in `main()` gesetzte ENABLE_VIRTUAL_TERMINAL_PROCESSING dabei wieder
    // loeschen. Ohne das Flag faellt crossterm auf die alte Konsolen-API
    // zurueck, die unsere Rgb-Palette nicht darstellen kann — die TUI wirkt
    // dann monochrom, obwohl tui_render 58 Farben setzt (beobachtet
    // 2026-07-26: Helligkeitsunterschiede kamen durch, Farbtoene nicht).
    enable_vt_processing();

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(t) => t,
        Err(e) => {
            let _ = terminal::disable_raw_mode();
            eprintln!("[tui] Konnte Terminal nicht erstellen: {e}");
            return 1;
        }
    };
    crate::bench_events::set_console_output(false);
    // Panics aus Hintergrund-Threads sichtbar machen. Ohne das beendet ein
    // Panic nur seinen Thread und verschwindet — am 02.08.2026 starb so die
    // Benchmark-Schleife, waehrend die TUI drei Stunden weiterlief.
    crate::bench_events::install_panic_hook();

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
        expanded: std::collections::HashSet::new(),
        detail_scroll: 0,
        focus: Panel::Agents,
        log_filter: LogFilter::All,
        activity_history: std::collections::VecDeque::new(),
        view: View::Workers,
        bench_scroll: 0,
        bench_expanded: std::collections::HashSet::new(),
        bench_selected: 0,
        command_input: String::new(),
        grid_status: String::new(),
            cap_selected: 0,
            cap_status: String::new(),
    };
    if let Some(arguments) = startup_benchmark.filter(|value| !value.trim().is_empty()) {
        let command = format!("/benchmark {arguments}");
        spawn_benchmark_from_tui(&command, &candidates);
        app.view = View::Bench;
    }
    // Explizite Wahl schlaegt die Kontext-Heuristik: so laesst sich jede
    // Ansicht ohne Tastendruck oeffnen (automatisierte Abnahme).
    if let Some(v) = startup_view {
        app.view = parse_view(v).unwrap_or(app.view);
    }

    /// Gefaltete Baum-Zeilenliste der Benchmark-Ansicht — dieselbe, die der
    /// Renderer zeichnet, damit Navigation und Darstellung konsistent bleiben.
    fn bench_lines(app: &App) -> Vec<crate::tui_state::BenchLine> {
        let events = crate::bench_events::snapshot();
        crate::tui_state::fold_bench_events(&events, &app.bench_expanded)
    }

    /// ID des Knotens unter dem Cursor (oder `None`, wenn die Liste leer ist).
    fn bench_selected_id(app: &App, lines: &[crate::tui_state::BenchLine]) -> Option<u64> {
        lines.get(app.bench_selected).map(|l| l.id)
    }

    /// Cursor in der Benchmark-Baumansicht einen Schritt bewegen.
    fn bench_move(app: &mut App, delta: i64) {
        let n = bench_lines(app).len();
        app.bench_move(n, delta);
    }

    /// Auf-/Zuklappen des Knotens unter dem Cursor in der Baumansicht.
    fn bench_toggle_at(app: &mut App, open: bool) {
        let lines = bench_lines(app);
        if let Some(id) = bench_selected_id(app, &lines) {
            if open {
                app.bench_expand(id);
            } else {
                app.bench_collapse(id);
            }
        }
    }

    // --- Event-Loop ---
    let tick_rate = std::time::Duration::from_millis(80);
    // Refresh alle `poll_secs * 12.5` Ticks (bei 80ms Tick = ~poll_secs Sekunden).
    let refresh_ticks = (poll_secs as f64 * 12.5).ceil() as u64;
    let mut frame_count = 0u64;
    let mut task_input = String::new();
    // Brain-Kachelansicht: die Tab-Fenster stehen im Normalfall off-screen. `w` holt sie
    // in ein Raster auf den Bildschirm. Bewusst hier und nicht als CLI-Befehl:
    // die Fenster gehoeren DIESEM Prozess, ein zweiter `webagent`-Aufruf saehe
    // sie gar nicht.
    let mut grid_on = false;

    let exit_code = loop {
        // Tastatur-Event (non-blocking, 80ms Timeout)
        if event::poll(tick_rate).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match app.input_mode {
                    InputMode::Normal => match key.code {
                        // Alt+Nummer: Fokus ausdruecklich auf eine Kachel.
                        // Enter gehoert IMMER dem Terminal — deshalb ist die
                        // Fokusuebernahme an eine eigene, bewusste Geste
                        // gebunden und passiert nie von selbst.
                        KeyCode::Char(c)
                            if key.modifiers.contains(KeyModifiers::ALT)
                                && crate::brain_grid::brain_index_for_digit(c).is_some() =>
                        {
                            let index = crate::brain_grid::brain_index_for_digit(c)
                                .unwrap_or(0);
                            app.grid_status = focus_brain_tile(index);
                        }
                        // Esc springt aus einer Kachel zurueck ins Terminal.
                        KeyCode::Esc => {
                            app.grid_status = release_brain_focus();
                        }
                        KeyCode::Up => {
                            if app.view == View::Bench {
                                bench_move(&mut app, -1);
                            } else {
                                app.selected = select_wrap(app.selected, -1, app.agents.len());
                            }
                        }
                        KeyCode::Down => {
                            if app.view == View::Bench {
                                bench_move(&mut app, 1);
                            } else {
                                app.selected = select_wrap(app.selected, 1, app.agents.len());
                            }
                        }
                        KeyCode::Char('q') => break 0,
                        // Ansicht umschalten: Worker-Dashboard <-> Benchmark.
                        // `v` wie „view", plus `<`/`>` als Griff aufs Eck-Symbol
                        // in der Kopfzeile.
                        KeyCode::Char('v') | KeyCode::Char('<') | KeyCode::Char('>') => {
                            app.view = app.view.next();
                        }
                        // `w` schaltet die Brain-Kachelansicht um: alle offenen
                        // Brain-Fenster als Kachelraster auf den Bildschirm,
                        // nochmal `w` parkt sie wieder off-screen.
                        KeyCode::Char('w') => {
                            grid_on = !grid_on;
                            app.grid_status = toggle_brain_grid(grid_on);
                        }
                        // Faehigkeit schalten. Der Browser-Anteil laeuft im
                        // Hintergrund-Thread: ein Toggle dauert Sekunden, und
                        // eine TUI, die dabei einfriert, sieht aus wie ein
                        // Absturz — heute wissen wir, wie teuer diese
                        // Verwechslung ist.
                        KeyCode::Char('t') if app.view == View::Capabilities => {
                            let rows = crate::tui_state::capability_rows(
                                &crate::capability::levels_all(),
                            );
                            match rows.get(app.cap_selected) {
                                Some(row) if row.is_actionable() => {
                                    let brain = row.brain.clone();
                                    let key = row.key.clone().unwrap_or_default();
                                    app.cap_status =
                                        format!("{brain}/{key}: wird geschaltet …");
                                    std::thread::spawn(move || {
                                        let line = drive_capability(&brain, &key);
                                        crate::bench_events::emit(
                                            crate::bench_events::Level::Info,
                                            Some(&brain),
                                            &line,
                                        );
                                    });
                                }
                                Some(row) => {
                                    app.cap_status = format!(
                                        "{}: nicht fahrbar — steht als Quest, nicht als Knopf",
                                        row.label
                                    );
                                }
                                None => {}
                            }
                        }
                        // In der Benchmark-Ansicht springt `g` ans untere Ende
                        // des frischen Ereignisstroms.
                        KeyCode::Char('g') if app.view == View::Bench => {
                            let n = bench_lines(&app).len();
                            app.bench_bottom(n);
                        }
                        // `e` togglet den Bench-Baum: klappt alles mit Detail
                        // auf, ein zweites Mal klappt alles wieder zu (schneller
                        // Überblick ueber einen langen Lauf).
                        KeyCode::Char('e') if app.view == View::Bench => app.bench_toggle_all(),
                        // Gewinner-Design (qwen, 2026-07-22): Tab wechselt den
                        // Panel-Fokus, f schaltet den Log-Filter durch.
                        KeyCode::Tab => app.focus = app.focus.next(),
                        KeyCode::Char('f') => app.log_filter = app.log_filter.next(),
                        // Ausklappen: Leertaste schaltet um, Pfeile sind
                        // gerichtet (mehrfach rechts klappt nicht wieder zu).
                        // In der Baumansicht gilt das den Ereignis-Knoten,
                        // sonst den Agenten.
                        KeyCode::Char(' ') => {
                            if app.view == View::Bench {
                                let lines = bench_lines(&app);
                                if let Some(id) = bench_selected_id(&app, &lines) {
                                    app.bench_toggle(id);
                                }
                            } else {
                                app.toggle_expanded();
                            }
                        }
                        KeyCode::Right => {
                            if app.view == View::Bench {
                                bench_toggle_at(&mut app, true);
                            } else {
                                app.expand_selected();
                            }
                        }
                        KeyCode::Left => {
                            if app.view == View::Bench {
                                bench_toggle_at(&mut app, false);
                            } else {
                                app.collapse_selected();
                            }
                        }
                        KeyCode::Char('j') if app.view == View::Capabilities => {
                            app.cap_selected = app.cap_selected.saturating_add(1);
                        }
                        KeyCode::Char('k') if app.view == View::Capabilities => {
                            app.cap_selected = app.cap_selected.saturating_sub(1);
                        }
                        KeyCode::Char('j') => {
                            if app.view == View::Bench {
                                bench_move(&mut app, 1);
                            } else {
                                app.detail_scroll = app.detail_scroll.saturating_add(1);
                            }
                        }
                        KeyCode::Char('k') => {
                            if app.view == View::Bench {
                                bench_move(&mut app, -1);
                            } else {
                                app.detail_scroll = app.detail_scroll.saturating_sub(1);
                            }
                        }
                        KeyCode::Char('+') => {
                            app.target_active = (app.target_active + 1).min(candidates.len());
                            write_control(
                                &control_path,
                                &PoolControl {
                                    target_active: Some(app.target_active),
                                    ..Default::default()
                                },
                            );
                        }
                        KeyCode::Char('-') => {
                            app.target_active = app.target_active.saturating_sub(1);
                            write_control(
                                &control_path,
                                &PoolControl {
                                    target_active: Some(app.target_active),
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
                        // Kommandozeile: / startet Eingabe fuer /benchmark etc.
                        KeyCode::Char('/') => {
                            app.input_mode = InputMode::CommandInput;
                            app.command_input.clear();
                            app.command_input.push('/');
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
                                    match send_task(&root, brain, "tui", text) {
                                        Ok(()) => crate::bench_events::emit(
                                            crate::bench_events::Level::Pass,
                                            Some(brain),
                                            "Aufgabe in die Inbox gestellt.",
                                        ),
                                        Err(e) => {
                                            crate::bench_events::emit(
                                                crate::bench_events::Level::Fail,
                                                Some(brain),
                                                &format!("Inbox-Fehler: {e}"),
                                            );
                                            // Die Benchmark-Ansicht zeigt den
                                            // Ereignisstrom unmittelbar sichtbar.
                                            app.view = View::Bench;
                                        }
                                    }
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
                    InputMode::CommandInput => match key.code {
                        KeyCode::Esc => {
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                        }
                        KeyCode::Enter => {
                            let cmd = app.command_input.trim().to_string();
                            if cmd.starts_with("/benchmark") {
                                spawn_benchmark_from_tui(&cmd, &candidates);
                            }
                            app.input_mode = InputMode::Normal;
                            app.command_input.clear();
                        }
                        KeyCode::Backspace => {
                            app.command_input.pop();
                        }
                        KeyCode::Char(c) => {
                            app.command_input.push(c);
                        }
                        _ => {}
                    },
                    InputMode::ConfirmQuit => match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => break 0,
                        KeyCode::Char('n') | KeyCode::Esc => app.input_mode = InputMode::Normal,
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
        if frame_count.is_multiple_of(refresh_ticks.max(1)) || frame_count == 1 {
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
        &PoolControl {
            stop: true,
            ..Default::default()
        },
    );
    let _ = handle.join();
    let _ = terminal::disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    crate::bench_events::set_console_output(true);
    exit_code
}

// ---------------------------------------------------------------------------
// Benchmark-Spawner (aus der TUI-Kommandozeile via /benchmark)
// ---------------------------------------------------------------------------

/// Parst `/benchmark --brains a,b --rounds 5 --loop` und startet den Benchmark
/// in einem Hintergrund-Thread im GLEICHEN Prozess. Browser bleiben dabei
/// standardmäßig außerhalb des sichtbaren Desktops; `--headed` ist die
/// bewusste Ausnahme für eine sichtbare Diagnose.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
fn spawn_benchmark_from_tui(cmd: &str, candidates: &[String]) {
    let mut brains: Vec<String> = candidates.to_vec();
    let mut rounds = 1usize;
    let mut suggestions = 3usize;
    let mut loop_forever = false;
    let mut headless = true;
    let mut harvest = true;
    // Storax-Vorgabe (31.07.2026): maximale Ausgabe im Ereignisstrom — die
    // Schritt-Zeilen des Controllers gehoeren sichtbar in die TUI, nicht nur
    // in die unterdrueckte Konsole. `--quiet` schaltet das wieder ab.
    let mut verbose = true;
    let mut vetoes: Vec<String> = Vec::new();

    let parts: Vec<&str> = cmd.split_whitespace().skip(1).collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--brains" => {
                i += 1;
                if i < parts.len() {
                    brains = parts[i]
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
            }
            "--rounds" => {
                i += 1;
                if i < parts.len() {
                    rounds = parts[i].parse().unwrap_or(1);
                }
            }
            "--suggestions" => {
                i += 1;
                if i < parts.len() {
                    suggestions = parts[i].parse().unwrap_or(3);
                }
            }
            // Beide Schreibweisen: die CLI heisst `--loop-forever`, die TUI hiess
            // nur `--loop`. Ein unbekanntes Flag wurde hier stillschweigend
            // verschluckt — am 30.07.2026 lief der Dauerlauf deshalb genau EINE
            // Runde und beendete sich, ohne dass irgendwo ein Hinweis stand.
            "--loop" | "--loop-forever" => loop_forever = true,
            "--headless" => headless = true,
            "--headed" => headless = false,
            "--no-harvest" => harvest = false,
            "--verbose" => verbose = true,
            "--quiet" => verbose = false,
            "--veto" => {
                i += 1;
                if i < parts.len() && !parts[i].trim().is_empty() {
                    vetoes.push(parts[i].trim().to_string());
                }
            }
            // Unbekannte Flags nicht verschlucken: ein Tippfehler oder eine
            // Schreibweise, die es nur in der CLI gibt, aendert sonst lautlos
            // nichts — genau so lief der Dauerlauf am 30.07.2026 eine Runde
            // statt endlos.
            unbekannt if unbekannt.starts_with("--") => {
                crate::bench_events::eprint_line(&format!(
                    "[tui] unbekanntes Benchmark-Flag ignoriert: {unbekannt}"
                ));
            }
            _ => {}
        }
        i += 1;
    }
    if brains.iter().all(|b| b == "all") {
        brains = candidates.to_vec();
    }

    // Nicht `current_dir()`: die TUI wird per Doppelklick vom Desktop
    // gestartet, und der ist kein Git-Repo — der Benchmark brach dort sofort
    // mit "not a git repository" ab. Derselbe Fehler war in cmd_benchmark
    // bereits behoben; dieser zweite Pfad blieb stehen und lief weiter ins
    // Leere. (Zwillingssuche: ein Fix greift nie, solange die Kopie daneben
    // unangetastet bleibt.)
    let workdir = match crate::autoresearch::resolve_project_root() {
        Ok(p) => p,
        Err(e) => {
            crate::bench_events::emit(
                crate::bench_events::Level::Fail,
                None,
                &format!("Benchmark nicht startbar: {e}"),
            );
            return;
        }
    };

    crate::bench_events::emit(
        crate::bench_events::Level::Info,
        None,
        &format!(
            "Benchmark startet: {} Brain(s), {rounds} Runde(n), loop={loop_forever}",
            brains.len()
        ),
    );

    std::thread::spawn(move || {
        let config = crate::benchmark::BenchmarkConfig {
            brains: brains.clone(),
            rounds,
            suggestions,
            build_eval: "cargo build --lib".to_string(),
            test_eval: "cargo test --lib".to_string(),
            workdir: workdir.clone(),
            headless,
            max_iterations: 20,
            harvest,
            verbose,
            // Lesephasen haben keinen gemeinsamen Worktree und dürfen alle
            // verfügbaren Brains gleichzeitig auslasten. Nur das spätere
            // Implementieren bleibt im Benchmark bewusst sequenziell.
            parallel: brains.len().max(1),
            stall_limit: 3,
            max_handoffs: 2,
            lint_eval: String::new(),
            vetoes,
            loop_forever,
        };
        let queries = crate::repl::PersistentQueryPool::new(&brains, headless);
        match crate::benchmark::run_benchmark(&config, |b, p| queries.query(b, p)) {
            Ok(_) => crate::bench_events::emit(
                crate::bench_events::Level::Pass,
                None,
                "Durchlauf abgeschlossen.",
            ),
            Err(e) => crate::bench_events::emit(
                crate::bench_events::Level::Fail,
                None,
                &format!("Fehler: {e}"),
            ),
        }
    });
}

// ---------------------------------------------------------------------------
// Öffentlicher Einstiegspunkt (dispatcht je nach Feature)
// ---------------------------------------------------------------------------

/// Schaltet die VT-Verarbeitung der Windows-Konsole ein (idempotent).
#[cfg(all(windows, feature = "webview"))]
fn enable_vt_processing() {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::{
        GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    };
    let handle = HANDLE(std::io::stdout().as_raw_handle());
    let mut mode = CONSOLE_MODE(0);
    unsafe {
        if GetConsoleMode(handle, &mut mode).is_ok() {
            let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

// Ohne `tui`-Feature wird der Rumpf von `run_tui` nicht uebersetzt; die
// Funktion bleibt aber Teil der oeffentlichen Schnittstelle. Damit ist sie
// hier ungenutzt — erwartet, kein Hinweis auf toten Code.
#[cfg(not(all(windows, feature = "webview")))]
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
fn enable_vt_processing() {}

/// Schaltet eine Faehigkeit an einem Brain und liefert die Meldung dazu.
///
/// Meldet ausdruecklich, OB der Zustandswechsel belegt wurde. Ein Klick, dessen
/// Wirkung man nicht nachweisen kann, ist in diesem Projekt kein Koennen — die
/// Oberflaeche darf das nicht verwischen, sonst zaehlt sie Absichten.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(feature = "webview")]
fn drive_capability(brain: &str, key: &str) -> String {
    use crate::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => return format!("{brain}/{key}: Backend nicht verfuegbar — {e}"),
    };
    match backend.toggle_option(key) {
        // Vorher/Nachher ausdruecklich gegenueberstellen. Ein Klick ohne
        // messbaren Unterschied ist kein Erfolg — er sieht nur so aus.
        Ok((before, after)) if before != after => {
            format!("{brain}/{key}: belegt — {before} → {after}")
        }
        Ok((before, _)) => format!(
            "{brain}/{key}: geklickt, aber KEIN Zustandswechsel messbar (blieb {before})"
        ),
        Err(e) => format!("{brain}/{key}: fehlgeschlagen — {e}"),
    }
}

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(not(feature = "webview"))]
fn drive_capability(brain: &str, key: &str) -> String {
    format!("{brain}/{key}: ohne webview-Feature nicht schaltbar")
}

/// Schaltet die Brain-Kachelansicht und liefert die Zeile, die in der TUI dazu erscheint.
///
/// Rueckmeldung statt stiller Wirkung: die Fenster liegen im Normalfall
/// off-screen, ein fehlgeschlagenes Anordnen waere sonst nicht von „es gibt
/// gerade kein offenes Brain-Fenster" zu unterscheiden.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(feature = "webview")]
fn toggle_brain_grid(on: bool) -> String {
    let pool = match crate::browser_pool::BrowserPool::global().lock() {
        Ok(pool) => pool,
        Err(_) => return "Kacheln: Browser-Pool nicht erreichbar".to_string(),
    };
    let open = pool.open_brains().len();
    if !on {
        return match pool.arrange_brain_grid(None) {
            Ok(_) => format!("Kacheln aus — {open} Fenster wieder geparkt"),
            Err(e) => format!("Kacheln aus fehlgeschlagen: {e}"),
        };
    }
    if open == 0 {
        return "Kacheln: kein Brain-Fenster offen".to_string();
    }
    let Some(area) = crate::brain_grid::default_grid_area() else {
        return "Kacheln: Bildschirmflaeche nicht ermittelbar".to_string();
    };
    match pool.arrange_brain_grid(Some(area)) {
        // Passen nicht alle, wird das benannt statt stillschweigend gekuerzt.
        // Die Tastenbelegung steht hier und nicht nur in der Legende: Alt+Nummer
        // ist der einzige Weg in eine Kachel hinein, weil die Kacheln bewusst
        // nicht anklickbar-aktivierbar sind. Ohne Hinweis wirkt das wie ein
        // kaputtes Fenster statt wie eine Entscheidung.
        Ok(tiled) if tiled < open => format!(
            "Kacheln an — {tiled} von {open} Fenstern gekachelt, {} zu klein und geparkt \
             · Alt+1…9 Fokus, Esc zurueck",
            open - tiled
        ),
        Ok(tiled) => format!(
            "Kacheln an — {tiled} Fenster gekachelt · Alt+1…9 Fokus, Esc zurueck"
        ),
        Err(e) => format!("Kacheln fehlgeschlagen: {e}"),
    }
}

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(not(feature = "webview"))]
fn toggle_brain_grid(_on: bool) -> String {
    "Kacheln: ohne webview-Feature nicht verfuegbar".to_string()
}

/// Alt+Nummer: Fokus auf eine Kachel holen.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(feature = "webview")]
fn focus_brain_tile(index: usize) -> String {
    let pool = match crate::browser_pool::BrowserPool::global().lock() {
        Ok(pool) => pool,
        Err(_) => return "Fokus: Browser-Pool nicht erreichbar".to_string(),
    };
    match pool.focus_brain_tile(index) {
        Ok(name) => format!("Fokus auf {name} — Esc zurueck ins Terminal"),
        Err(e) => format!("Fokus fehlgeschlagen: {e}"),
    }
}

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(not(feature = "webview"))]
fn focus_brain_tile(_index: usize) -> String {
    "Fokus: ohne webview-Feature nicht verfuegbar".to_string()
}

/// Esc: Fokus zurueck ans Terminalfenster, Kacheln wieder nicht aktivierbar.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(feature = "webview")]
fn release_brain_focus() -> String {
    let pool = match crate::browser_pool::BrowserPool::global().lock() {
        Ok(pool) => pool,
        Err(_) => return "Fokus: Browser-Pool nicht erreichbar".to_string(),
    };
    match pool.release_brain_focus() {
        Ok(0) => "Fokus: kein Brain-Fenster offen".to_string(),
        Ok(n) => format!("Fokus zurueck im Terminal — {n} Kacheln wieder passiv"),
        Err(e) => format!("Fokusrueckgabe fehlgeschlagen: {e}"),
    }
}

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(not(feature = "webview"))]
fn release_brain_focus() -> String {
    "Fokus: ohne webview-Feature nicht verfuegbar".to_string()
}

/// Einstiegspunkt der TUI (Default, wenn `webagent` ohne Subcommand läuft).
#[cfg_attr(not(feature = "tui"), allow(unused_variables))]
pub fn run_tui(
    active: usize,
    brains: &str,
    poll_secs: u64,
    headless: bool,
    startup_benchmark: Option<&str>,
    startup_view: Option<&str>,
) -> i32 {
    #[cfg(feature = "tui")]
    {
        // ratatui braucht ein echtes Terminal (raw mode + alternate screen).
        // Bei umgeleiteter/detached Ausgabe schlaegt enable_raw_mode fehl — dann
        // die ANSI-Variante fahren statt mit Fehler abzubrechen.
        use std::io::IsTerminal;
        if std::io::stdout().is_terminal() {
            return run_tui_ratatui(
                active,
                brains,
                poll_secs,
                headless,
                startup_benchmark,
                startup_view,
            );
        }
        eprintln!("[tui] Kein interaktives Terminal (umgeleitet/detached) — ANSI-Fallback.");
    }
    run_tui_ansi(active, brains, poll_secs, headless, startup_benchmark)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_root() -> PathBuf {
        static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
        let seq = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "webagent_tui_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            seq
        ))
    }

    #[test]
    fn rapid_tasks_get_distinct_complete_inbox_files() {
        let root = temp_root();
        let inbox = root.join("agents").join("claude").join("inbox");
        fs::create_dir_all(&inbox).unwrap();

        send_task(&root, "claude", "tui", "first").unwrap();
        send_task(&root, "claude", "tui", "second").unwrap();

        let mut messages: Vec<_> = fs::read_dir(&inbox)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".msg.txt"))
            .collect();
        messages.sort_by_key(|entry| entry.file_name());
        assert_eq!(messages.len(), 2);
        let contents: Vec<_> = messages
            .iter()
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        assert!(contents.iter().any(|content| content.contains("first")));
        assert!(contents.iter().any(|content| content.contains("second")));
        assert!(
            fs::read_dir(&inbox)
                .unwrap()
                .flatten()
                .all(|entry| !entry.file_name().to_string_lossy().contains(".tmp-")),
            "temporary inbox file leaked"
        );
    }
}
