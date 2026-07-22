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

/// Kompakte Heartbeat-Anzeige: Puls-Symbol + Alter. Frisch grün, alt rot
/// (Farbe kommt aus [`heartbeat_color`]). Zeigt auf einen Blick, ob ein Worker
/// noch lebt.
fn heartbeat_pip(age_sec: u64) -> String {
    if age_sec == u64::MAX {
        "· —".to_string()
    } else if age_sec < 60 {
        format!("♥ {age_sec}s")
    } else if age_sec < 3600 {
        format!("♡ {}m", age_sec / 60)
    } else {
        "♡ alt".to_string()
    }
}
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
    titled_block_focus(title, false)
}

/// Rahmen-Block, dessen Rand+Titel bei `focused` hervorgehoben werden — so ist
/// das per Tab fokussierte Panel sichtbar (Gewinner-Design 2026-07-22).
fn titled_block_focus(title: &str, focused: bool) -> Block<'static> {
    let (border, marker) = if focused {
        (Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD), "▸ ")
    } else {
        (Style::default().fg(Color::DarkGray), "")
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(Span::styled(
            format!(" {marker}{title} "),
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

/// Render-Top-Level: 3-Pane Layout.
pub fn ui(f: &mut Frame, app: &App) {
    // Vertikal: KPI-Kopfleiste (3) · Körper (Rest) · Footer (1).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, outer[0]);

    // Leerzustand: kein Worker-Pool aktiv -> einladender Hinweis statt toter Kästen.
    if app.agents.is_empty() {
        render_empty_state(f, outer[1]);
        render_footer(f, outer[2]);
        return;
    }

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(outer[1]);

    render_agent_list(f, app, body[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // kompakte Statuskarte
            Constraint::Min(6),    // Live-Log (wächst)
            Constraint::Length(6), // Tasks
        ])
        .split(body[1]);

    render_status(f, app, right[0]);
    render_log(f, app, right[1]);
    render_tasks(f, app, right[2]);

    render_footer(f, outer[2]);
}

/// KPI-Kopfleiste: Wortmarke + Live-Kennzahlen des Pools (aktiv/Ziel, bereit,
/// erledigte Tasks, Brain-Zahl) mit einem lebenden Puls. Das prägnant „Neue"
/// gegenüber der reinen 3-Panel-Liste.
fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let active = app.agents.iter().filter(|a| a.status == "active").count();
    let ready = app.agents.iter().filter(|a| a.status == "available").count();
    let done: usize = app.agents.iter().map(|a| a.tasks_done).sum();
    let pending: usize = app.agents.iter().map(|a| a.tasks_pending).sum();
    let total = done + pending;

    let accent = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let ok = Style::default().fg(Color::Green);
    let warn = Style::default().fg(Color::Yellow);

    // KPI-Chip: „Label Wert" kompakt.
    let chip = |icon: &str, val: String, s: Style| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!("{icon} "), s),
            Span::styled(val, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("   "),
        ]
    };

    let mut line: Vec<Span> = vec![
        Span::styled("▚▞ webagent ", accent),
        Span::styled("Worker-Pool", dim),
        Span::raw("     "),
    ];
    line.extend(chip("●", format!("{active}/{} aktiv", app.target_active), ok));
    line.extend(chip("○", format!("{ready} bereit"), warn));
    line.extend(chip("✓", format!("{done}/{total} Tasks"), ok));
    line.extend(chip("◆", format!("{} Brains", app.agents.len()), accent));
    line.push(Span::styled(app.spinner_frame().to_string(), accent));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let p = Paragraph::new(Line::from(line)).block(block);
    f.render_widget(p, area);
}

/// Einladender Leerzustand statt toter Panels, wenn kein Worker-Pool läuft.
fn render_empty_state(f: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let accent = Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("  Kein Worker-Pool aktiv.", accent)),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Drücke ", dim),
            Span::styled("+", accent),
            Span::styled(", um einen Worker zu starten — oder ", dim),
            Span::styled("q", accent),
            Span::styled(" zum Beenden.", dim),
        ]),
        Line::from(vec![
            Span::styled("  Aufgaben verteilst du mit ", dim),
            Span::styled("↵", accent),
            Span::styled(" an den gewählten Brain.", dim),
        ]),
    ];
    let p = Paragraph::new(lines).block(titled_block("Worker-Pool"));
    f.render_widget(p, area);
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
        // Reiche Zeile: Pfeil · Status-Glyph · Name · Heartbeat-Frische ·
        // Task-Balken · Live-Aktivität. Deutlich mehr als eine reine Namensliste.
        let hb = heartbeat_pip(a.heartbeat_age_sec);
        let total = a.tasks_pending + a.tasks_done;
        let frac = if total > 0 { a.tasks_done as f64 / total as f64 } else { 0.0 };
        let bar = text_bar(frac, 6);
        let activity = a
            .last_log_line
            .as_deref()
            .or_else(|| a.detail.last().map(String::as_str))
            .unwrap_or("—");
        items.push(ListItem::new(Line::from(vec![
            Span::styled(arrow.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{marker} "), Style::default().fg(color)),
            Span::styled(format!("{:<9}", crate::char_prefix(&a.brain, 9)), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {hb} "), Style::default().fg(heartbeat_color(a.heartbeat_age_sec))),
            Span::styled(bar, Style::default().fg(Color::DarkGray)),
            Span::styled(format!(" {}", crate::char_prefix(activity, 22)), Style::default().fg(Color::DarkGray)),
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
            items.push(detail_item("… ältere Einträge oben (j/k blättert)"));
        }
    }

    let list = List::new(items)
        .block(titled_block_focus("Agenten", app.focus == crate::tui_state::Panel::Agents))
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
    // Mehrzeiliges Log aus den jüngsten Ereignissen des gewählten Agenten;
    // Fallback auf die einzelne letzte Zeile.
    let raw: Vec<String> = match agent {
        Some(a) if !a.detail.is_empty() => a.detail.clone(),
        Some(a) => a.last_log_line.clone().into_iter().collect(),
        None => Vec::new(),
    };

    let filter = app.log_filter;
    let mut lines: Vec<Line> = raw
        .iter()
        .filter(|l| filter.keeps(l))
        .map(|l| Line::from(Span::styled(l.clone(), log_line_style(l))))
        .collect();
    if lines.is_empty() {
        let hint = if raw.is_empty() {
            "Keine Log-Daten".to_string()
        } else {
            format!("(keine Zeile im Filter '{}' — f schaltet um)", filter.label())
        };
        lines.push(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))));
    }

    let title = format!("Live Log [{}]", filter.label());
    let p = Paragraph::new(lines)
        .block(titled_block_focus(&title, app.focus == crate::tui_state::Panel::Log))
        .scroll((app.log_scroll, 0));

    f.render_widget(p, area);
}

/// Severity-Farbe einer Log-Zeile (rot=Fehler, gelb=Warnung, sonst gedimmt).
fn log_line_style(line: &str) -> Style {
    use crate::tui_state::LogFilter;
    if !LogFilter::Errors.keeps(line) && LogFilter::Warnings.keeps(line) {
        Style::default().fg(Color::Yellow)
    } else if LogFilter::Errors.keeps(line) {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Gray)
    }
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

    let p = Paragraph::new(content)
        .block(titled_block_focus("Tasks", app.focus == crate::tui_state::Panel::Tasks));

    f.render_widget(p, area);
}

/// Footer: Keybindings — Tasten hervorgehoben, Beschriftung gedämpft.
/// 
/// Design-spezifische Tasten: j/k für Detail-Scroll, f für Log-Filter,
/// Tab für Panel-Fokus, Space für Expand.
fn render_footer(f: &mut Frame, area: Rect) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    let mut spans = vec![Span::raw(" ")];
    for (k, label) in [
        ("↑↓", "wählen"),
        ("␣", "ausklappen"),
        ("Tab", "fokus"),
        ("f", "filter"),
        ("+/-", "worker"),
        ("↵", "task"),
        ("q", "quit"),
    ] {
        spans.push(Span::styled(k, key));
        spans.push(Span::styled(format!(" {label}  ", ), dim));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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
            focus: crate::tui_state::Panel::Agents,
            log_filter: crate::tui_state::LogFilter::All,
        }
    }

    #[test]
    fn tab_cycles_focus_through_all_three_panels() {
        use crate::tui_state::Panel;
        let mut app = test_app(vec![test_agent("a", "active", vec![])]);
        assert_eq!(app.focus, Panel::Agents);
        app.focus = app.focus.next();
        assert_eq!(app.focus, Panel::Tasks);
        app.focus = app.focus.next();
        assert_eq!(app.focus, Panel::Log);
        app.focus = app.focus.next();
        assert_eq!(app.focus, Panel::Agents, "zyklisch zurueck");
    }

    #[test]
    fn log_filter_cycles_and_keeps_by_severity() {
        use crate::tui_state::LogFilter;
        assert_eq!(LogFilter::All.next(), LogFilter::Warnings);
        assert_eq!(LogFilter::Warnings.next(), LogFilter::Errors);
        assert_eq!(LogFilter::Errors.next(), LogFilter::All);

        // Alle zeigt jede Zeile.
        assert!(LogFilter::All.keeps("ganz normale zeile"));
        // Fehler zeigt nur Fehlerzeilen.
        assert!(LogFilter::Errors.keeps("panic: index out of bounds"));
        assert!(!LogFilter::Errors.keeps("ganz normale zeile"));
        // Warn+ zeigt Warnungen UND Fehler, aber nicht Normales.
        assert!(LogFilter::Warnings.keeps("cooldown fuer 900s"));
        assert!(LogFilter::Warnings.keeps("error: kaputt"));
        assert!(!LogFilter::Warnings.keeps("alles gut"));
    }

    #[test]
    fn focused_block_uses_a_distinct_border() {
        // Der fokussierte Rahmen muss sich sichtbar vom unfokussierten
        // unterscheiden (sonst sieht man den Tab-Fokus nicht).
        let focused = titled_block_focus("X", true);
        let plain = titled_block_focus("X", false);
        assert_ne!(format!("{focused:?}"), format!("{plain:?}"));
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

    #[test]
    fn heartbeat_pip_reflects_freshness_and_death() {
        assert!(heartbeat_pip(3).contains("3s"));
        assert!(heartbeat_pip(3).starts_with('♥'), "frisch = gefuellter Puls");
        assert!(heartbeat_pip(120).contains("2m"));
        assert!(heartbeat_pip(120).starts_with('♡'), "aelter = leerer Puls");
        assert_eq!(heartbeat_pip(u64::MAX), "· —", "nie gesehen = kein Puls");
    }
}

