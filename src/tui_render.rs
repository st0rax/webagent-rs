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

/// Wie viele Roh-Ereignisse ein aufgeklappter Agent höchstens zeigt.
const DETAIL_MAX_ENTRIES: usize = 8;
/// Wie viele Detailzeilen gleichzeitig sichtbar sind (Rest per j/k).
const DETAIL_MAX_ROWS: usize = 12;

/// Eine eingerückte Detailzeile im aufgeklappten Baum.
fn detail_item(text: &str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled("   │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(text.to_string(), Style::default().fg(Color::Gray)),
    ]))
}

/// Linke Pane: Agenten-Liste mit Auswahl-Highlight.
fn render_agent_list(f: &mut Frame, app: &App, area: Rect) {
    // Innenbreite abzüglich Rahmen und Einrückung des Detailblocks.
    let detail_width = (area.width as usize).saturating_sub(8);
    let mut items: Vec<ListItem> = Vec::new();
    for a in &app.agents {
        let color = status_color(&a.status);
        // Nur aktive Agenten „drehen" (Spinner); der Rest bleibt ruhig lesbar.
        let marker = if a.status == "active" {
            app.spinner_frame()
        } else {
            status_glyph(&a.status)
        };
        let open = app.is_expanded(&a.brain);
        // Klapp-Pfeil nur, wo es auch etwas zu sehen gibt.
        let arrow = if a.detail.is_empty() {
            "  "
        } else if open {
            "▾ "
        } else {
            "▸ "
        };
        items.push(ListItem::new(Line::from(vec![
            Span::styled(arrow.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{marker} "), Style::default().fg(color)),
            Span::styled(a.brain.clone(), Style::default().fg(color)),
        ])));

        if !open {
            continue;
        }
        let lines = crate::tui_state::build_detail_lines(&a.detail, DETAIL_MAX_ENTRIES, detail_width);
        if lines.is_empty() {
            items.push(detail_item("(noch nichts protokolliert)"));
            continue;
        }
        let shown: Vec<&String> = lines.iter().skip(app.detail_scroll).collect();
        for l in shown.iter().take(DETAIL_MAX_ROWS) {
            items.push(detail_item(l));
        }
        // Ehrlich anzeigen, dass unten noch etwas liegt — sonst haelt man den
        // abgeschnittenen Ausschnitt fuer das ganze Protokoll.
        let rest = shown.len().saturating_sub(DETAIL_MAX_ROWS);
        if rest > 0 {
            items.push(detail_item(&format!("… {rest} weitere Zeile(n), j/k blättert")));
        } else if app.detail_scroll > 0 && !shown.is_empty() {
            // Wenn wir durch Scrollen nach oben in der History sind, zeigen wir
            // an, dass noch ältere Einträge vorhanden sind.
            items.push(detail_item(&format!("… ältere Einträge oben (j/k blättert)")));
        }
    }

    let list = List::new(items)
        .block(titled_block("Agenten"))
        .style(Style::default())
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .add_modifier(Modifier::BOLD),
        );

    // Agenten-Index in Zeilen-Index umrechnen — aufgeklappte Details schieben
    // die Zeilen, der Auswahlbalken traefe sonst die falsche.
    let detail_rows: Vec<usize> = app
        .agents
        .iter()
        .map(|a| {
            if !app.is_expanded(&a.brain) {
                return 0;
            }
            let lines =
                crate::tui_state::build_detail_lines(&a.detail, DETAIL_MAX_ENTRIES, detail_width);
            if lines.is_empty() {
                return 1;
            }
            let shown = lines.len().saturating_sub(app.detail_scroll);
            shown.min(DETAIL_MAX_ROWS) + usize::from(shown > DETAIL_MAX_ROWS)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(crate::tui_state::selected_row(&detail_rows, app.selected)));

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
/// 
/// Design-spezifische Tasten: j/k für Detail-Scroll, f für Log-Filter,
/// Tab für Panel-Fokus, Space für Expand.
fn render_footer(f: &mut Frame) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::raw(" ")];
    for (k, label) in [
        ("↑↓/j/k", "navigieren"),
        ("␣", "expand"),
        ("→", "expand"),
        ("←", "collapse"),
        ("Tab", "fokus"),
        ("f", "filter"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui_state::{AgentView, App, InputMode};

    fn test_agent(brain: &str, status: &str, detail: Vec<String>) -> AgentView {
        AgentView {
            brain: brain.to_string(),
            status: status.to_string(),
            pid: Some(1234),
            heartbeat_age_sec: 30,
            tasks_pending: 5,
            tasks_done: 3,
            last_log_line: Some("Test log".to_string()),
            last_response: Some("Test response".to_string()),
            detail,
        }
    }

    fn test_app(agents: Vec<AgentView>) -> App {
        App {
            agents,
            selected: 0,
            tick: 0,
            log_scroll: 0,
            input_mode: InputMode::Normal,
            target_active: 2,
            gauge_shown: 0.5,
            expanded: std::collections::HashSet::new(),
            detail_scroll: 0,
        }
    }

    #[test]
    fn status_color_returns_correct_colors() {
        assert_eq!(status_color("active"), Color::Green);
        assert_eq!(status_color("available"), Color::Yellow);
        assert_eq!(status_color("cooldown"), Color::Blue);
        assert_eq!(status_color("unavailable"), Color::Red);
        assert_eq!(status_color("unknown"), Color::Red);
    }

    #[test]
    fn heartbeat_color_returns_correct_colors() {
        assert_eq!(heartbeat_color(0), Color::Green);
        assert_eq!(heartbeat_color(59), Color::Green);
        assert_eq!(heartbeat_color(60), Color::Yellow);
        assert_eq!(heartbeat_color(299), Color::Yellow);
        assert_eq!(heartbeat_color(300), Color::Red);
        assert_eq!(heartbeat_color(999), Color::Red);
    }

    #[test]
    fn status_glyph_returns_correct_symbols() {
        assert_eq!(status_glyph("active"), "●");
        assert_eq!(status_glyph("available"), "○");
        assert_eq!(status_glyph("cooldown"), "◐");
        assert_eq!(status_glyph("unavailable"), "✕");
        assert_eq!(status_glyph("unknown"), "✕");
    }

    #[test]
    fn text_bar_returns_correct_length() {
        // Test that the bar has the right length: [ + width + ]
        // Note: Unicode characters like █ are 3 bytes in UTF-8, so len() returns bytes, not chars
        let bar = text_bar(0.5, 10);
        // Check char count instead of byte count
        assert_eq!(bar.chars().count(), 12); // [ + 10 chars + ]
        // When width is 10 and fraction 0.5, we get exactly 5 filled
        assert_eq!(bar, "[█████░░░░░]");

        let bar = text_bar(0.0, 10);
        assert_eq!(bar, "[░░░░░░░░░░]");

        let bar = text_bar(1.0, 10);
        assert_eq!(bar, "[██████████]");

        let bar = text_bar(1.5, 10);
        assert_eq!(bar, "[██████████]");

        let bar = text_bar(-0.5, 10);
        assert_eq!(bar, "[░░░░░░░░░░]");
    }

    #[test]
    fn kv_line_creates_formatted_line() {
        let line = kv_line("Test", "Value", Style::default().fg(Color::Green));
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, " Test      ");
        assert_eq!(line.spans[1].content, "Value");
    }

    #[test]
    fn detail_item_creates_indented_list_item() {
        // Just test the function doesn't panic - we can't inspect private fields
        let _item = detail_item("Test message");
    }

    #[test]
    fn titled_block_creates_block_with_title() {
        // Just test the function doesn't panic
        let _block = titled_block("Test Title");
    }
}

