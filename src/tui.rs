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

#[cfg(feature = "tui")]
use std::io::{self};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "tui")]
use std::thread;

#[cfg(feature = "tui")]
use crate::config::{available_brain_ids, bot2bot_root};
use crate::worker_pool::{atomic_write, PoolControl};
#[cfg(feature = "tui")]
use crate::worker_pool::{candidates_with_profile, WorkerPool};

use crate::tui_ansi::run_tui_ansi;

#[cfg(feature = "tui")]
use crate::tui_state::App;
#[cfg(feature = "tui")]
use std::path::PathBuf;

#[cfg(feature = "tui")]
fn apply_session_resume(app: &mut App, id: Option<&str>) {
    let runs = crate::config::data_dir().join("runs");
    let dir = match id {
        Some(name) if !name.is_empty() => Some(runs.join(name)),
        _ => latest_run_dir(&runs),
    };
    let Some(dir) = dir else {
        app.session_status = "kein run".to_string();
        return;
    };
    let text = std::fs::read_to_string(dir.join("transcript.jsonl")).unwrap_or_default();
    app.session_turns = crate::transcript::session_turns_from_jsonl(&text);
    crate::transcript::sync_session_folds(&app.session_turns, &mut app.session_folded);
    app.session_follow_disk = false;
    app.session_transcript = Some(dir.join("transcript.jsonl"));
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    app.session_status = format!("resume {name} ({} turns)", app.session_turns.len());
}

#[cfg(feature = "tui")]
fn refresh_session_from_disk(app: &mut App) {
    if !app.session_follow_disk && app.session_transcript.is_none() {
        return;
    }
    let runs = crate::config::data_dir().join("runs");
    let path = app.session_transcript.clone().or_else(|| {
        crate::transcript::latest_session_run_dir(&runs).map(|d| d.join("transcript.jsonl"))
    });
    let Some(path) = path else {
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let turns = crate::transcript::session_turns_from_jsonl(&text);
    if turns != app.session_turns {
        app.session_turns = turns;
        crate::transcript::sync_session_folds(&app.session_turns, &mut app.session_folded);
    }
}

#[cfg(feature = "tui")]
fn latest_run_dir(runs: &std::path::Path) -> Option<PathBuf> {
    let mut dirs: Vec<_> = std::fs::read_dir(runs)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    dirs.into_iter().next().map(|e| e.path())
}

// ---------------------------------------------------------------------------
// Gemeinsame Hilfsfunktionen (beide TUI-Varianten)
// ---------------------------------------------------------------------------

/// Datei-Zeitstempel im `send.ps1`-Format: `yyyyMMddTHHmmss` (UTC).
pub(crate) fn file_stamp() -> String {
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
pub(crate) fn iso_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// Schreibt einen Steuerbefehl nach `pool_control.json`.
pub(crate) fn write_control(path: &Path, control: &PoolControl) {
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

pub(crate) fn discard_stale_control(path: &Path) {
    match PoolControl::take(path) {
        Ok(Some(_)) | Ok(None) => {}
        Err(e) => crate::bench_events::eprint_line(&format!(
            "[tui] Stale Control-Datei konnte nicht verworfen werden: {e}"
        )),
    }
}

/// Legt eine Aufgabe im Inbox-Format von `send.ps1` ab -> Worker holt sie ab.
/// Liefert `Err`, wenn der Ziel-Agent keine Inbox hat (nicht registriert).
pub(crate) fn send_task(root: &Path, brain: &str, from: &str, text: &str) -> std::io::Result<()> {
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
// ratatui-TUI (mit Feature "tui")
// ---------------------------------------------------------------------------

#[cfg(feature = "tui")]
fn run_tui_ratatui(
    active: usize,
    brains: &str,
    poll_secs: u64,
    headless: bool,
    run_secs: u64,
    startup_benchmark: Option<&str>,
    startup_view: Option<&str>,
) -> i32 {
    use crossterm::event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseButton, MouseEventKind,
    };
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
    // Maus erfassen. Eine rein tastaturbediente Oberflaeche ist fuer alles
    // unbedienbar, was keine Tastatur hat — Skripte, Watchdogs, und ein
    // Assistent, der Terminalfenster nur anklicken darf. Ein Fehlschlag ist
    // kein Abbruchgrund (die Tastatur bleibt), muss aber sichtbar sein: sonst
    // sucht der naechste den Fehler in der Trefferpruefung.
    let mouse_ready = stdout.execute(EnableMouseCapture).is_ok();

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

    // --force-tui: Terminal-Fenster wird extern via PowerShell-Helfer
    // positioniert (opencode-Kontext erlaubt kein FindWindowW).
    // Kein dock_terminal_bottom_force() noetig.

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
        bench_expanded: std::collections::HashSet::new(),
        bench_selected: 0,
        command_input: String::new(),
        // Eine stumm gescheiterte Mauserfassung sieht exakt aus wie eine
        // kaputte Trefferpruefung. Deshalb steht der Fehlschlag da, wo man
        // hinsieht, wenn ein Klick nichts tut.
        grid_status: if mouse_ready {
            String::new()
        } else {
            "Maus nicht erfasst — nur Tastatur".to_string()
        },
        cap_selected: 0,
        cap_status: String::new(),
        cfg_selected: 0,
        cfg_status: String::new(),
        session_turns: Vec::new(),
        session_status: String::new(),
        session_brain: "chatgpt".to_string(),
        session_selected: 0,
        session_folded: Vec::new(),
        session_help: false,
        session_follow_disk: true,
        session_transcript: None,
    };
    refresh_session_from_disk(&mut app);
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

    /// Klick auf die `offset`-te sichtbare Zeile des Koerpers.
    ///
    /// Rechnet den Ausschnitt mit [`crate::tui_bench::bench_window_start`]
    /// zurueck — derselben Formel, mit der gerendert wird. Eine zweite Formel
    /// hier waere ein Klick, der eine andere Zeile trifft als die, auf die man
    /// zeigt.
    ///
    /// Ein Klick auf die bereits gewaehlte Zeile klappt sie auf bzw. zu. So
    /// bleibt die Oberflaeche ohne Doppelklick-Erkennung vollstaendig mit der
    /// Maus bedienbar — Auswaehlen und Oeffnen sind zwei Klicks, nicht ein
    /// zeitkritischer.
    fn select_row_at(app: &mut App, offset: usize, rows: usize) {
        match app.view {
            View::Bench => {
                let lines = bench_lines(app);
                if lines.is_empty() {
                    return;
                }
                let sel = app.bench_selected.min(lines.len() - 1);
                let start = crate::tui_bench::bench_window_start(lines.len(), rows, sel);
                let Some(target) = start.checked_add(offset).filter(|t| *t < lines.len()) else {
                    return;
                };
                if target == sel {
                    if let Some(id) = bench_selected_id(app, &lines) {
                        app.bench_toggle(id);
                    }
                } else {
                    app.bench_selected = target;
                }
            }
            View::Capabilities => {
                app.cap_selected = offset;
            }
            View::Config => {
                if offset < crate::tui_config::SETTINGS.len() {
                    app.cfg_selected = offset;
                }
            }
            View::Workers => {
                if offset < app.agents.len() {
                    if offset == app.selected {
                        app.toggle_expanded();
                    } else {
                        app.selected = offset;
                    }
                }
            }
            View::Session => {}
        }
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
    // Brain-Kachelansicht: Worker-Fenster leben in Kindprozessen und stehen
    // oft off-screen. Beim normalen TUI-Start dockt die Wall automatisch an
    // (Terminal unten, Brains oben); `w` schaltet Arrange/Park.
    let mut wall = crate::brain_wall::WallState::start_on();
    let _wall_cleanup = crate::brain_wall::WallCleanupGuard;
    let (wall_tx, wall_rx) =
        std::sync::mpsc::channel::<Result<(crate::brain_wall::WindowSignature, String), String>>();
    let mut wall_apply_pending = false;
    let mut wall_apply_thread: Option<std::thread::JoinHandle<()>> = None;
    let mut wall_retry_after = std::time::Instant::now();
    // TUI-Fenster minimiert: Kacheln parken. Restore legt neu (needs_relayout).
    let mut wall_hidden_for_host_minimize = false;
    // Prozesssnapshot + EnumWindows sind globale OS-Scans, nicht Teil des
    // 80-ms-Render-Ticks. Rund einmal pro Sekunde reicht fuer nachwachsende
    // Worker-Fenster und haelt Tastatur/Rendering frei.
    let wall_discovery_ticks = 13u64;

    // Reguläres Zeitende: dieselbe Ausstiegsstelle wie `q`, damit der
    // Shutdown-Pfad (Pool herunterfahren, Write-back) identisch bleibt. Ein
    // eigener Beendigungsweg waere ein zweiter Pfad, der auseinanderlaufen
    // koennte — genau das vermeidet die TUI schon bei Maus und Tastatur.
    let deadline = (run_secs > 0)
        .then(|| std::time::Instant::now() + std::time::Duration::from_secs(run_secs));

    let mut exit_code = 'main: loop {
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            break 'main 0;
        }
        // Eingaben einsammeln. Tastatur und Maus laufen bewusst in DIESELBE
        // Warteschlange: ein Klick wird zu der Taste, die ein Mensch gedrueckt
        // haette, und durchlaeuft danach denselben `match`. Ein eigener
        // Maus-Zweig waere ein zweiter Bedienpfad — und zwei Bedienpfade
        // laufen erfahrungsgemaess auseinander.
        let mut pending: Vec<KeyEvent> = Vec::new();
        if event::poll(tick_rate).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => pending.push(key),
                Ok(Event::Mouse(m)) => {
                    let size = terminal.size().unwrap_or_default();
                    let screen = crate::tui_mouse::Screen {
                        width: size.width,
                        height: size.height,
                        view: app.view,
                        has_agents: !app.agents.is_empty(),
                    };
                    match m.kind {
                        // Mausrad: eine Zeile pro Raste, in jeder Ansicht.
                        MouseEventKind::ScrollDown => {
                            pending.extend(crate::tui_keys::parse_key("j"))
                        }
                        MouseEventKind::ScrollUp => pending.extend(crate::tui_keys::parse_key("k")),
                        MouseEventKind::Down(MouseButton::Left) => {
                            match crate::tui_mouse::hit(screen, m.column, m.row) {
                                Some(crate::tui_mouse::Hit::Key(action)) => {
                                    pending.extend(crate::tui_keys::parse_key(action));
                                }
                                Some(crate::tui_mouse::Hit::Row(offset)) => {
                                    let rows = crate::tui_mouse::body_rows(size.height);
                                    select_row_at(&mut app, offset, rows);
                                }
                                None => {}
                            }
                        }
                        // Rechtsklick klappt zu — das Gegenstueck zum
                        // Aufklappen per Doppel-/Linksklick auf einen Knoten.
                        MouseEventKind::Down(MouseButton::Right) => {
                            pending.extend(crate::tui_keys::parse_key("left"));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        for key in pending {
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
                        let index = crate::brain_grid::brain_index_for_digit(c).unwrap_or(0);
                        app.grid_status = crate::brain_wall::focus_tile(index);
                    }
                    // Esc springt aus einer Kachel zurueck ins Terminal.
                    KeyCode::Esc => {
                        app.grid_status = crate::brain_wall::release_focus();
                    }
                    // Pfeile tun dasselbe wie j/k. Die Fusszeile bewirbt
                    // zwar j/k, aber niemand liest eine Legende, bevor er
                    // die Pfeiltaste drueckt — eine Liste, die auf Pfeile
                    // nicht reagiert, wirkt kaputt.
                    KeyCode::Up if app.view == View::Config => {
                        app.cfg_selected = app.cfg_selected.saturating_sub(1);
                    }
                    KeyCode::Down if app.view == View::Config => {
                        let last = crate::tui_config::SETTINGS.len().saturating_sub(1);
                        app.cfg_selected = app.cfg_selected.saturating_add(1).min(last);
                    }
                    KeyCode::Up if app.view == View::Capabilities => {
                        app.cap_selected = app.cap_selected.saturating_sub(1);
                    }
                    KeyCode::Down if app.view == View::Capabilities => {
                        app.cap_selected = app.cap_selected.saturating_add(1);
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
                    KeyCode::Char('?') if app.view == View::Session => {
                        app.session_help = !app.session_help;
                    }
                    KeyCode::Char('y') if app.view == View::Session => {
                        match crate::transcript::copy_last_brain_reply(&app.session_turns) {
                            Ok(copied) => {
                                app.session_status =
                                    format!("copy {}c", copied.text.chars().count());
                            }
                            Err(e) => {
                                app.session_status = format!("copy: {e}");
                            }
                        }
                    }
                    KeyCode::Char('f') if app.view == View::Session => {
                        crate::transcript::sync_session_folds(
                            &app.session_turns,
                            &mut app.session_folded,
                        );
                        crate::transcript::toggle_session_fold(
                            &mut app.session_folded,
                            app.session_selected,
                        );
                    }
                    KeyCode::Char('q') => break 'main 0,
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
                        // Kein Arrange/Park gegeneinander laufen lassen:
                        // sonst kann ein spaet endender Auto-Thread die eben
                        // geparkten Fenster wieder auf den Bildschirm holen.
                        if let Some(thread) = wall_apply_thread.take() {
                            let _ = thread.join();
                            wall_apply_pending = false;
                            let _ = wall_rx.try_recv();
                        }
                        wall.toggle();
                        let discovered = crate::brain_wall::discover_owned();
                        let signature = crate::brain_wall::window_signature(&discovered);
                        match crate::brain_wall::apply_wall_checked(wall.on) {
                            Ok(status) => {
                                app.grid_status = status;
                                if wall.on {
                                    wall.mark_arranged(signature);
                                } else {
                                    wall.mark_parked();
                                }
                            }
                            Err(status) => {
                                app.grid_status = status;
                                wall_retry_after =
                                    std::time::Instant::now() + std::time::Duration::from_secs(1);
                            }
                        }
                    }
                    // Faehigkeit schalten. Der Browser-Anteil laeuft im
                    // Hintergrund-Thread: ein Toggle dauert Sekunden, und
                    // eine TUI, die dabei einfriert, sieht aus wie ein
                    // Absturz — heute wissen wir, wie teuer diese
                    // Verwechslung ist.
                    // Einstellung weiterschalten. Laeuft im Vordergrund:
                    // eine Datei schreiben und eine Umgebungsvariable
                    // setzen dauert Millisekunden — anders als das
                    // Schalten einer Faehigkeit, das einen Browser fahren
                    // muss.
                    KeyCode::Char('t') if app.view == View::Config => {
                        if let Some(setting) = crate::tui_config::SETTINGS.get(app.cfg_selected) {
                            app.cfg_status = match crate::tui_config::cycle(setting) {
                                Ok(next) => {
                                    format!("{} = {next} — gilt ab dem naechsten Lauf", setting.key)
                                }
                                // Ein fehlgeschlagenes Speichern MUSS
                                // dastehen: sonst zeigt die Zeile den
                                // neuen Wert und die Platte den alten.
                                Err(e) => format!("{}: nicht gespeichert — {e}", setting.key),
                            };
                        }
                    }
                    KeyCode::Char('r') if app.view == View::Config => {
                        if let Some(setting) = crate::tui_config::SETTINGS.get(app.cfg_selected) {
                            app.cfg_status = match crate::tui_config::reset(setting) {
                                Ok(()) => format!(
                                    "{} zurueckgesetzt — es gilt wieder die Vorgabe",
                                    setting.key
                                ),
                                Err(e) => format!("{}: nicht zurueckgesetzt — {e}", setting.key),
                            };
                        }
                    }
                    KeyCode::Char('j') if app.view == View::Config => {
                        let last = crate::tui_config::SETTINGS.len().saturating_sub(1);
                        app.cfg_selected = app.cfg_selected.saturating_add(1).min(last);
                    }
                    KeyCode::Char('k') if app.view == View::Config => {
                        app.cfg_selected = app.cfg_selected.saturating_sub(1);
                    }
                    KeyCode::Char('t') if app.view == View::Capabilities => {
                        let rows =
                            crate::tui_state::capability_rows(&crate::capability::levels_all());
                        match rows.get(app.cap_selected) {
                            Some(row) if row.is_actionable() => {
                                let brain = row.brain.clone();
                                let key = row.key.clone().unwrap_or_default();
                                app.cap_status = format!("{brain}/{key}: wird geschaltet …");
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
                    KeyCode::Char('f') if app.view != View::Session => {
                        app.log_filter = app.log_filter.next();
                    }
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
                    KeyCode::Char('j') if app.view == View::Session => {
                        if app.session_selected + 1 < app.session_turns.len() {
                            app.session_selected += 1;
                        }
                    }
                    KeyCode::Char('k') if app.view == View::Session => {
                        app.session_selected = app.session_selected.saturating_sub(1);
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
                        } else if let Some(parsed) = crate::repl::parse_slash_command(&cmd) {
                            use crate::repl::commands::SessionSlashEffect;
                            match crate::repl::commands::session_slash_effect(&parsed) {
                                SessionSlashEffect::Quit => break 'main 0,
                                SessionSlashEffect::Dashboard => {
                                    app.view = View::Workers;
                                }
                                SessionSlashEffect::NewSession => {
                                    app.session_turns.clear();
                                    app.session_folded.clear();
                                    app.session_selected = 0;
                                    app.session_follow_disk = false;
                                    app.session_transcript = None;
                                    app.session_status = "neue session".to_string();
                                }
                                SessionSlashEffect::Status => {
                                    app.session_status = format!(
                                        "brain={} turns={}",
                                        app.session_brain,
                                        app.session_turns.len()
                                    );
                                }
                                SessionSlashEffect::SwitchBrain(target) => {
                                    if let Some(b) = target {
                                        app.session_brain = b;
                                    }
                                    app.session_status = format!("model {}", app.session_brain);
                                }
                                SessionSlashEffect::Resume(id) => {
                                    apply_session_resume(&mut app, id.as_deref());
                                }
                                SessionSlashEffect::Evolve(args) => {
                                    run_evolve(&args, &candidates);
                                    app.view = View::Bench;
                                    app.session_status = "evolve".to_string();
                                }
                                SessionSlashEffect::Compact => {
                                    let summary = app
                                        .session_transcript
                                        .as_ref()
                                        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
                                        .or_else(|| {
                                            crate::transcript::latest_session_run_dir(
                                                &crate::config::data_dir().join("runs"),
                                            )
                                        })
                                        .map(|d| crate::transcript::compact_run_dir(&d));
                                    match summary {
                                        Some(Ok(text)) => {
                                            app.session_status =
                                                format!("compact {}c", text.chars().count());
                                            app.session_turns.push(
                                                crate::transcript::SessionTurn {
                                                    kind: crate::transcript::SessionTurnKind::Tool,
                                                    body: text,
                                                },
                                            );
                                            crate::transcript::sync_session_folds(
                                                &app.session_turns,
                                                &mut app.session_folded,
                                            );
                                        }
                                        Some(Err(e)) => {
                                            app.session_status = format!("compact: {e}");
                                        }
                                        None => {
                                            app.session_status = "compact: kein run".into();
                                        }
                                    }
                                }
                                SessionSlashEffect::Swarm { prompt, .. } => {
                                    let cards = crate::bin_hooks::session_swarm_cards(&prompt);
                                    app.session_turns.extend(cards);
                                    crate::transcript::sync_session_folds(
                                        &app.session_turns,
                                        &mut app.session_folded,
                                    );
                                    app.session_status = format!("swarm {prompt}");
                                }
                                SessionSlashEffect::Brute(url) => {
                                    match crate::repl::commands::brute_http_url(&url) {
                                        Some(u) => {
                                            app.session_status = format!("brute {u}");
                                            let code = crate::bin_hooks::run_brute_write(&u, true);
                                            if code != 0 {
                                                app.session_status = format!("brute exit {code}");
                                            }
                                        }
                                        None => {
                                            app.session_status =
                                                "brute: /brute <https://url>".to_string();
                                        }
                                    }
                                }
                                SessionSlashEffect::Unhandled => {
                                    app.session_status = format!("unbekannt: {cmd}");
                                }
                            }
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
            refresh_session_from_disk(&mut app);
        }

        // Ergebnis erst quittieren, wenn das Win32-Anordnen wirklich gelang.
        // Fehler bleiben mit Backoff retrybar; waehrend eines laufenden Apply
        // wird kein zweiter Thread gestartet.
        if let Ok(result) = wall_rx.try_recv() {
            wall_apply_pending = false;
            if let Some(thread) = wall_apply_thread.take() {
                let _ = thread.join();
            }
            match result {
                Ok((signature, status)) => {
                    wall.mark_arranged(signature);
                    crate::bench_events::emit(
                        crate::bench_events::Level::Info,
                        None,
                        &format!("Auto-Wall: {status}"),
                    );
                }
                Err(status) => {
                    wall_retry_after =
                        std::time::Instant::now() + std::time::Duration::from_secs(1);
                    crate::bench_events::emit(
                        crate::bench_events::Level::Warn,
                        None,
                        &format!("Auto-Wall: {status}; neuer Versuch folgt"),
                    );
                }
            }
        }
        if wall.on
            && !wall_apply_pending
            && frame_count.is_multiple_of(wall_discovery_ticks)
            && std::time::Instant::now() >= wall_retry_after
        {
            let host_iconic = crate::brain_grid::host_window_is_iconic();
            if host_iconic {
                if !wall_hidden_for_host_minimize {
                    if let Some(thread) = wall_apply_thread.take() {
                        let _ = thread.join();
                    }
                    let _ = crate::brain_wall::park_owned();
                    wall.mark_parked();
                    wall_hidden_for_host_minimize = true;
                    app.grid_status = "Kacheln geparkt — TUI minimiert".to_string();
                }
            } else if wall_hidden_for_host_minimize {
                wall_hidden_for_host_minimize = false;
            }
            let discovered = crate::brain_wall::discover_owned();
            let signature = crate::brain_wall::window_signature(&discovered);
            let iconic = crate::brain_wall::any_iconic(&discovered);
            if !host_iconic && wall.needs_relayout(&signature, iconic) {
                wall_apply_pending = true;
                let tx = wall_tx.clone();
                wall_apply_thread = Some(std::thread::spawn(move || {
                    let result = crate::brain_wall::apply_wall_checked(true)
                        .map(|status| (signature, status));
                    let _ = tx.send(result);
                }));
            }
        }

        // Rendern
        if let Err(e) = terminal.draw(|f| ui(f, &app)) {
            eprintln!("[tui] Render-Fehler: {e}");
            break 1;
        }
    };

    // --- Cleanup ---
    // Noch bevor die Worker beendet werden parken, damit deren Fenster fuer
    // die Discovery vorhanden sind. Der Guard wiederholt den Terminal-Restore
    // auf jedem spaeteren Return-/Unwind-Pfad idempotent.
    if let Some(thread) = wall_apply_thread.take() {
        let _ = thread.join();
    }
    let _ = crate::brain_wall::apply_wall_checked(false);
    wall.mark_parked();
    write_control(
        &control_path,
        &PoolControl {
            stop: true,
            ..Default::default()
        },
    );
    let _ = handle.join();
    // Browser-Pool sauber herunterfahren, BEVOR der Prozess endet: nur so
    // schliessen sich die WebView-Tabs geordnet und `write_back_session_to_master`
    // spielt die Sitzung ins Master-Profil zurueck. Ohne diesen Schritt bleibt
    // das Master eingefroren (Tabs offen -> teardown_runtime feuert nie), und
    // der naechste Start klont wieder den alten Login-Stand.
    // Der Benchmark-Thread laeuft detached weiter; seine naechste
    // Pool-Operation wartet kurz auf der Sperre, waehrend wir hier beenden.
    match crate::browser_pool::BrowserPool::global().lock() {
        Ok(mut pool) => {
            if let Err(error) = pool.shutdown_with_result() {
                eprintln!(
                    "[master-profile] geordneter Browserpool-Shutdown fehlgeschlagen: {error}"
                );
                exit_code = 1;
            }
        }
        Err(error) => {
            eprintln!("[master-profile] Browserpool-Sperre vergiftet: {error}");
            exit_code = 1;
        }
    }
    // Mauserfassung ZUERST zuruecknehmen: bleibt sie an, schluckt das Terminal
    // nach dem Beenden weiter jede Mausgeste — Markieren und Kopieren gingen
    // dann nicht mehr, und zwar in einer Sitzung, die mit der TUI gar nichts
    // mehr zu tun hat.
    if mouse_ready {
        let _ = io::stdout().execute(DisableMouseCapture);
    }
    let _ = terminal::disable_raw_mode();
    let _ = io::stdout().execute(LeaveAlternateScreen);
    crate::bench_events::set_console_output(true);
    exit_code
}

// ---------------------------------------------------------------------------
// Benchmark-Spawner (aus der TUI-Kommandozeile via /benchmark)
// ---------------------------------------------------------------------------

/// `/evolve` und `/benchmark` — dieselbe Pipeline wie die TUI-Kommandozeile.
pub fn run_evolve(args: &str, candidates: &[String]) {
    let line = if args.trim().is_empty() {
        "/benchmark".to_string()
    } else {
        format!("/benchmark {args}")
    };
    spawn_benchmark_from_tui(&line, candidates);
}

/// Parst `/benchmark --brains a,b --rounds 5 --loop` und startet den Benchmark
/// in einem Hintergrund-Thread im GLEICHEN Prozess. Browser bleiben dabei
/// standardmäßig außerhalb des sichtbaren Desktops; `--headed` ist die
/// bewusste Ausnahme für eine sichtbare Diagnose.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
pub(crate) fn spawn_benchmark_from_tui(cmd: &str, candidates: &[String]) {
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
            work_package: None,
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
#[cfg(all(windows, feature = "webview", feature = "tui"))]
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

#[cfg(all(feature = "tui", not(all(windows, feature = "webview"))))]
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
        Ok((before, _)) => {
            format!("{brain}/{key}: geklickt, aber KEIN Zustandswechsel messbar (blieb {before})")
        }
        Err(e) => format!("{brain}/{key}: fehlgeschlagen — {e}"),
    }
}

#[cfg_attr(not(feature = "tui"), allow(dead_code))]
#[cfg(not(feature = "webview"))]
fn drive_capability(brain: &str, key: &str) -> String {
    format!("{brain}/{key}: ohne webview-Feature nicht schaltbar")
}

/// Alt+Nummer: Fokus auf eine Kachel holen.
#[cfg(feature = "webview")]
#[allow(dead_code)]
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
#[cfg(feature = "webview")]
#[allow(dead_code)]
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
// Die Parameterliste war mit sieben Argumenten schon an der Grenze; `run_secs`
// stoesst sie darueber. Ein Options-Struct waere die saubere Antwort, betrifft
// aber drei Aufrufer und gehoert damit in eine eigene Scheibe — nicht in einen
// Commit, der einen fehlenden Shutdown-Pfad nachruestet.
#[allow(clippy::too_many_arguments)]
pub fn run_tui(
    active: usize,
    brains: &str,
    poll_secs: u64,
    headless: bool,
    run_secs: u64,
    startup_benchmark: Option<&str>,
    startup_view: Option<&str>,
    force_tui: bool,
) -> i32 {
    #[cfg(feature = "tui")]
    {
        use std::io::IsTerminal;
        if force_tui || std::io::stdout().is_terminal() {
            // --force-tui: Kacheln auf gesamtem Bildschirm (kein Terminal-Andocken)
            if force_tui {
                crate::brain_grid::set_force_tui(true);
            }
            return run_tui_ratatui(
                active,
                brains,
                poll_secs,
                headless,
                run_secs,
                startup_benchmark,
                startup_view,
            );
        }
        eprintln!("[tui] Kein interaktives Terminal (umgeleitet/detached) — ANSI-Fallback.");
    }
    run_tui_ansi(
        active,
        brains,
        poll_secs,
        headless,
        run_secs,
        startup_benchmark,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
