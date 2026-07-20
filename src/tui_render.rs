//! tui_render — ratatui Rendering für TUI
//!
//! 3-Pane Layout: Agenten-Liste (28%) | Status+Log+Tasks (72%)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use crate::tui_state::App;

/// Farben für Status.
fn status_color(status: &str) -> Color {
    match status {
        "active" => Color::Green,
        "available" => Color::Yellow,
        "cooldown" => Color::Blue,
        _ => Color::Red,
    }
}

/// Heartbeat-Ampel (grün <60s, gelb <300s, rot >=300s).
fn heartbeat_color(age_sec: u64) -> Color {
    if age_sec < 60 {
        Color::Green
    } else if age_sec < 300 {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Heartbeat gilt ab hier als tot (Supervisor killt stale Worker).
const HEARTBEAT_TIMEOUT_SEC: u64 = 300;
/// Breite der Label-Spalte für ausgerichtete Schlüssel/Wert-Zeilen.
const LABEL_WIDTH: usize = 10;
/// Breite der Text-Fortschrittsbalken.
const BAR_WIDTH: usize = 16;

/// Statuspunkt für die schnelle Erfassung in der Liste.
fn status_glyph(status: &str) -> &'static str {
    match status {
        "active" => "●",
        "available" => "○",
        "cooldown" => "◐",
        _ => "✕",
    }
}

/// Ausgerichtete „Label   Wert"-Zeile mit optionaler Wert-Farbe.
fn kv_line(label: &str, value: impl Into<String>, value_style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!(" {label:<LABEL_WIDTH$}"),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.into(), value_style),
    ])
}

/// Text-Fortschrittsbalken `[████░░░░]` aus einem Anteil 0.0..=1.0.
fn text_bar(fraction: f64, width: usize) -> String {
    let f = fraction.clamp(0.0, 1.0);
    let filled = (f * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("[{}{}]", "█".repeat(filled), "░".repeat(width - filled))
}

/// Rahmen-Block mit etwas Luft um den Titel (einheitliche Optik).
fn titled_block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

/// Render-Top-Level: 3-Pane Layout.
pub fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Percentage(72)])
        .split(f.area());

    // Linke Seite: Agenten-Liste
    render_agent_list(f, app, chunks[0]);

    // Rechte Seite: 3 vertikale Panes
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(40), // Status
            Constraint::Percentage(30), // Log
            Constraint::Percentage(30), // Tasks
        ])
        .split(chunks[1]);

    render_status(f, app, right_chunks[0]);
    render_log(f, app, right_chunks[1]);
    render_tasks(f, app, right_chunks[2]);

    // Footer mit Keybindings
    render_footer(f);
}

/// Linke Pane: Agenten-Liste mit Auswahl-Highlight.
fn render_agent_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .agents
        .iter()
        .map(|a| {
            let color = status_color(&a.status);
            // Nur aktive Agenten „drehen" (Spinner); der Rest bleibt ruhig lesbar.
            let marker = if a.status == "active" {
                app.spinner_frame()
            } else {
                status_glyph(&a.status)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {marker} "), Style::default().fg(color)),
                Span::styled(a.brain.clone(), Style::default().fg(color)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(titled_block("Agenten"))
        .style(Style::default())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.selected));

    f.render_stateful_widget(&list, area, &mut state);
}

/// Status-Pane: Gewählter Agent + Heartbeat-Ampel.
fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);

    let content = if let Some(a) = agent {
        let hb_color = heartbeat_color(a.heartbeat_age_sec);
        let status_st = Style::default()
            .fg(status_color(&a.status))
            .add_modifier(Modifier::BOLD);
        let pid = a
            .pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "—".to_string());

        // Heartbeat-Frische als Balken: voll = frisch (0s), leer = Timeout (300s).
        let remaining = HEARTBEAT_TIMEOUT_SEC.saturating_sub(a.heartbeat_age_sec);
        let fraction = remaining as f64 / HEARTBEAT_TIMEOUT_SEC as f64;
        let bar = text_bar(fraction, BAR_WIDTH);

        vec![
            Line::from(Span::raw("")),
            kv_line(
                "Brain",
                a.brain.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            kv_line(
                "Status",
                format!("{} {}", status_glyph(&a.status), a.status),
                status_st,
            ),
            kv_line("PID", pid, Style::default()),
            Line::from(Span::raw("")),
            kv_line(
                "Heartbeat",
                format!("{}s {}", a.heartbeat_age_sec, app.spinner_frame()),
                Style::default().fg(hb_color),
            ),
            Line::from(vec![
                Span::styled(
                    format!(" {:<LABEL_WIDTH$}", ""),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(bar, Style::default().fg(hb_color)),
                Span::styled(format!("  {remaining}s bis Timeout"), Style::default().fg(Color::DarkGray)),
            ]),
        ]
    } else {
        vec![Line::from(Span::styled(
            " Kein Agent ausgewählt",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let p = Paragraph::new(content).block(titled_block("Status"));

    f.render_widget(p, area);
}

/// Log-Pane: Live-Log Stream.
fn render_log(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);
    let text = agent
        .and_then(|a| a.last_log_line.clone())
        .unwrap_or_else(|| "Keine Log-Daten".to_string());

    let p = Paragraph::new(text)
        .block(titled_block("Live Log"))
        .scroll((app.log_scroll, 0));

    f.render_widget(p, area);
}

/// Tasks-Pane: Offene/Erledigte Tasks.
fn render_tasks(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);

    let content = if let Some(a) = agent {
        let total = a.tasks_pending + a.tasks_done;
        let fraction = if total > 0 {
            a.tasks_done as f64 / total as f64
        } else {
            0.0
        };
        let pct = (fraction * 100.0).round() as u32;

        vec![
            Line::from(Span::raw("")),
            kv_line(
                "Erledigt",
                format!("{}/{}", a.tasks_done, total),
                Style::default().fg(Color::Green),
            ),
            Line::from(vec![
                Span::styled(
                    format!(" {:<LABEL_WIDTH$}", ""),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(text_bar(fraction, BAR_WIDTH), Style::default().fg(Color::Green)),
                Span::styled(format!("  {pct}%"), Style::default().fg(Color::DarkGray)),
            ]),
            kv_line(
                "Offen",
                a.tasks_pending.to_string(),
                Style::default().fg(if a.tasks_pending > 0 {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }),
            ),
        ]
    } else {
        vec![Line::from(Span::styled(
            " —",
            Style::default().fg(Color::DarkGray),
        ))]
    };

    let p = Paragraph::new(content).block(titled_block("Tasks"));

    f.render_widget(p, area);
}

/// Footer: Keybindings — Tasten hervorgehoben, Beschriftung gedämpft.
fn render_footer(f: &mut Frame) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::raw(" ")];
    for (k, label) in [
        ("↑↓", "wechseln"),
        ("↵", "pinnen"),
        ("t", "task"),
        ("+/-", "target"),
        ("r", "reflag"),
        ("x", "abbrechen"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(k, key));
        spans.push(Span::styled(format!(" {label}   "), dim));
    }
    let footer = Paragraph::new(Line::from(spans));

    let area = f.area();
    let footer_area = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    f.render_widget(footer, footer_area);
}
