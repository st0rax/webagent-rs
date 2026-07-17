//! tui_render — ratatui Rendering für TUI
//!
//! 3-Pane Layout: Agenten-Liste (28%) | Status+Log+Tasks (72%)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Stylize},
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

/// Render-Top-Level: 3-Pane Layout.
pub fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(28),
            Constraint::Percentage(72),
        ])
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
            ListItem::new(Line::from(Span::styled(
                format!(" {} {}", app.spinner_frame(), a.brain),
                Style::default().fg(color),
            )))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Agenten"))
        .style(Style::default())
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    state.select(Some(app.selected));

    f.render_stateful_widget(&list, area, &mut state);
}

/// Status-Pane: Gewählter Agent + Heartbeat-Ampel.
fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);

    let content = if let Some(a) = agent {
        let hb_color = heartbeat_color(a.heartbeat_age_sec);
        let hb_label = format!("{:>3}s", a.heartbeat_age_sec);

        vec![
            Line::from(Span::raw("")),
            Line::from(Span::styled(format!("Brain: {}", a.brain), Style::default().bold())),
            Line::from(Span::raw(format!("Status: {:?}", a.status))),
            Line::from(Span::raw(format!("PID: {:?}", a.pid))),
            Line::from(Span::raw("")),
            Line::from(Span::styled("Heartbeat: ", Style::default())),
            Line::from(vec![
                Span::raw(""),
                Span::styled(
                    format!("Heartbeat: {} [{}]", hb_label, app.spinner_frame()),
                    Style::default().fg(hb_color),
                ),
            ]),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::raw(""),
                Span::styled(
                    format!("Gauge: {:.1}s remaining", 600.0 - a.heartbeat_age_sec as f64),
                    Style::default().fg(hb_color),
                ),
            ]),
        ]
    } else {
        vec![Line::from("Kein Agent ausgewählt")]
    };

    let p = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Status"));

    f.render_widget(p, area);
}

/// Log-Pane: Live-Log Stream.
fn render_log(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);
    let text = agent
        .and_then(|a| a.last_log_line.clone())
        .unwrap_or_else(|| "Keine Log-Daten".to_string());

    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Live Log"))
        .scroll((app.log_scroll, 0));

    f.render_widget(p, area);
}

/// Tasks-Pane: Offene/Erledigte Tasks.
fn render_tasks(f: &mut Frame, app: &App, area: Rect) {
    let agent = app.agents.get(app.selected);

    let content = if let Some(a) = agent {
        vec![
            Line::from(Span::raw("")),
            Line::from(Span::styled(
                format!(" Offene Tasks: {} ", a.tasks_pending),
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::raw("")),
            Line::from(vec![
                Span::raw(""),
                Span::styled(
                    format!("Tasks: {}/{} erledigt", a.tasks_done, a.tasks_pending + a.tasks_done),
                    Style::default().fg(Color::Green),
                ),
            ]),
            Line::from(Span::raw("")),
            Line::from(Span::raw(format!(
                " {} Inbox-Dateien",
                a.tasks_pending
            ))),
        ]
    } else {
        vec![Line::from("—")]
    };

    let p = Paragraph::new(content)
        .block(Block::default().borders(Borders::ALL).title("Tasks"));

    f.render_widget(p, area);
}

/// Footer: Keybindings.
fn render_footer(f: &mut Frame) {
    let footer = Paragraph::new(
        "↑↓ wechseln · Enter pinnen · t task · +/- target · r reflag · x abbrechen · q quit",
    )
    .style(Style::default().fg(Color::DarkGray));

    let area = f.area();
    let footer_area = Rect::new(area.x, area.bottom() - 1, area.width, 1);
    f.render_widget(footer, footer_area);
}
