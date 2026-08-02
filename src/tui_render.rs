//! tui_render — ratatui Rendering für TUI
//!
//! 3-Pane Layout: Agenten-Liste (28%) | Status+Log+Tasks (72%)

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Sparkline},
    Frame,
};

use crate::bench_events::Level;
use crate::tui_state::{App, View};

/// Akzentfarbe der Oberfläche (ein durchgängiger Ton statt bunt gemischt).
const ACCENT: Color = Color::Rgb(94, 197, 214); // gedämpftes Cyan
/// Gedämpfter Text (Labels, Rahmen).
const MUTED: Color = Color::Rgb(110, 120, 130);

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

/// Balken als ZWEI Spans: gefuellter Teil in `fg`, leerer Teil gedaempft.
///
/// Frueher wurde der komplette Balken einfarbig gerendert — dadurch sahen
/// auch die leeren `░`-Zeichen wie Fuellung aus und ein Balken bei 0% wirkte
/// vollstaendig gefuellt (beobachtet 2026-07-27 im Tasks-Panel: "Erledigt
/// 0/0" mit durchgehend gruenem Balken und "0%" daneben).
fn bar_spans(fraction: f64, width: usize, fg: Color) -> Vec<Span<'static>> {
    let f = fraction.clamp(0.0, 1.0);
    let filled = ((f * width as f64).round() as usize).min(width);
    vec![
        Span::styled("[", Style::default().fg(MUTED)),
        Span::styled("█".repeat(filled), Style::default().fg(fg)),
        Span::styled(
            "░".repeat(width - filled),
            Style::default().fg(Color::Rgb(58, 64, 72)),
        ),
        Span::styled("]", Style::default().fg(MUTED)),
    ]
}

/// Rahmen-Block mit etwas Luft um den Titel (einheitliche Optik).
fn titled_block(title: &str) -> Block<'static> {
    titled_block_focus(title, false)
}

/// Rahmen-Block, dessen Rand+Titel bei `focused` hervorgehoben werden — so ist
/// das per Tab fokussierte Panel sichtbar (Gewinner-Design 2026-07-22).
fn titled_block_focus(title: &str, focused: bool) -> Block<'static> {
    let (border, marker) = if focused {
        (
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            "▸ ",
        )
    } else {
        (Style::default().fg(MUTED), "")
    };
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(
            format!(" {marker}{title} "),
            Style::default()
                .fg(if focused { ACCENT } else { Color::Gray })
                .add_modifier(Modifier::BOLD),
        ))
}

/// Render-Top-Level: 3-Pane Layout.
pub fn ui(f: &mut Frame, app: &App) {
    // Vertikal: KPI-Kopfleiste (3) · Körper (Rest) · Footer (1).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_header(f, app, outer[0]);

    // Benchmark-Ansicht: der Ereignisstrom fuellt den Koerper. Umschaltbar per
    // `v` / `<>` — dieselbe TUI, andere Sicht auf denselben Lauf.
    if app.view == View::Bench {
        render_bench(f, app, outer[1]);
        render_footer(f, app, outer[2]);
        return;
    }

    // Leerzustand: kein Worker-Pool aktiv -> einladender Hinweis statt toter Kästen.
    if app.agents.is_empty() {
        render_empty_state(f, outer[1]);
        render_footer(f, app, outer[2]);
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

    render_footer(f, app, outer[2]);
}

/// KPI-Kopfleiste: Wortmarke + Live-Kennzahlen des Pools (aktiv/Ziel, bereit,
/// erledigte Tasks, Brain-Zahl) mit einem lebenden Puls. Das prägnant „Neue"
/// gegenüber der reinen 3-Panel-Liste.
fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let active = app.agents.iter().filter(|a| a.status == "active").count();
    let ready = app
        .agents
        .iter()
        .filter(|a| a.status == "available")
        .count();
    let done: usize = app.agents.iter().map(|a| a.tasks_done).sum();
    let pending: usize = app.agents.iter().map(|a| a.tasks_pending).sum();
    let total = done + pending;

    // Umrahmter Kopf; Titel zeigt die aktive Ansicht. Benchmark-Modus
    // tauscht die Worker-KPIs gegen Benchmark-Status aus.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" ▚▞ webagent · {} ", app.view.label()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(30),    // KPIs / Benchmark-Info
            Constraint::Length(22), // Gauge (nur Worker-View)
            Constraint::Length(18), // Sparkline
        ])
        .split(inner);

    let ok = Style::default().fg(Color::Green);
    let warn = Style::default().fg(Color::Yellow);
    let chip = |icon: &str, val: String, s: Style| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!("{icon} "), s),
            Span::styled(
                val,
                Style::default()
                    .fg(Color::Gray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
        ]
    };

    let kpi_rows: Vec<Line> = match app.view {
        View::Workers => {
            let mut kpis: Vec<Span> = Vec::new();
            kpis.extend(chip(
                "●",
                format!("{active}/{} aktiv", app.target_active),
                ok,
            ));
            kpis.extend(chip("○", format!("{ready} bereit"), warn));
            kpis.extend(chip("✓", format!("{done}/{total} Tasks"), ok));
            kpis.extend(chip(
                "◆",
                format!("{} Brains", app.agents.len()),
                Style::default().fg(ACCENT),
            ));
            let stale = app
                .agents
                .iter()
                .filter(|a| a.heartbeat_age_sec >= 300)
                .count();
            vec![
                Line::from(vec![
                    Span::styled("Ziel ", Style::default().fg(MUTED)),
                    Span::styled(
                        format!("{} aktiv", app.target_active),
                        Style::default().fg(Color::Gray),
                    ),
                    Span::raw("   "),
                    Span::styled(
                        format!("{stale} ohne frischen Puls"),
                        if stale > 0 {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default().fg(MUTED)
                        },
                    ),
                ]),
                Line::from(kpis),
            ]
        }
        View::Bench => {
            // Kennzahlen aus dem Ereignisstrom ableiten statt nur die Zahl der
            // Meldungen zu zeigen. Die Zeile "Benchmark" entfaellt — sie
            // wiederholte nur den Rahmentitel.
            let evs = crate::bench_events::snapshot();
            let n = evs.len();
            let phase = evs
                .iter()
                .rev()
                .find_map(|e| {
                    e.text.find("Phase ").map(|i| {
                        e.text[i..]
                            .chars()
                            .take_while(|c| !c.is_whitespace() || *c == ' ')
                            .take(10)
                            .collect::<String>()
                            .trim()
                            .to_string()
                    })
                })
                .unwrap_or_else(|| "—".to_string());
            let pass = evs.iter().filter(|e| e.level == Level::Pass).count();
            let fail = evs.iter().filter(|e| e.level == Level::Fail).count();
            // "Zuletzt gehoert von": die Brains der juengsten Meldungen.
            let mut aktiv: Vec<&str> = Vec::new();
            for e in evs.iter().rev().take(25) {
                if let Some(b) = e.brain.as_deref() {
                    if !aktiv.contains(&b) {
                        aktiv.push(b);
                    }
                }
            }
            let seit = evs
                .first()
                .map(|e| e.ts.clone())
                .unwrap_or_else(|| "—".to_string());
            vec![
                Line::from(vec![
                    Span::styled(format!("◇ {phase}"), Style::default().fg(ACCENT)),
                    Span::raw("   "),
                    Span::styled("seit ", Style::default().fg(MUTED)),
                    Span::styled(seit, Style::default().fg(Color::Gray)),
                    Span::raw("   "),
                    Span::styled(
                        format!("{} Brains gemeldet", aktiv.len()),
                        Style::default().fg(MUTED),
                    ),
                    Span::raw("   "),
                    // Verwertbarkeitsquote der laufenden Runde. Ohne sie sieht
                    // eine Runde, in der 5 von 6 Antworten wegen Formatfehlern
                    // weggeworfen wurden, genauso aus wie eine gesunde.
                    {
                        let tally = crate::round_tally::snapshot();
                        Span::styled(
                            tally.label(),
                            if tally.is_alarming() {
                                Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::BOLD)
                            } else {
                                Style::default().fg(MUTED)
                            },
                        )
                    },
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("⚡ {n} Meldungen"),
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("   "),
                    Span::styled(format!("✓ {pass}"), ok),
                    Span::raw("  "),
                    Span::styled(
                        format!("✕ {fail}"),
                        if fail > 0 {
                            Style::default().fg(Color::Red)
                        } else {
                            Style::default().fg(MUTED)
                        },
                    ),
                    Span::raw("   "),
                    {
                        let last = evs.last().map(|e| e.text.as_str()).unwrap_or("");
                        let status = bench_status(
                            n,
                            crate::bench_events::seconds_since_last_event(),
                            last,
                        );
                        Span::styled(
                            status.label(),
                            match status {
                                BenchStatus::Aktiv => ok,
                                BenchStatus::Bereit => warn,
                                BenchStatus::Beendet => Style::default().fg(MUTED),
                                BenchStatus::Stillstand => Style::default()
                                    .fg(Color::Red)
                                    .add_modifier(Modifier::BOLD),
                            },
                        )
                    },
                ]),
            ]
        }
    };
    f.render_widget(Paragraph::new(kpi_rows), cols[0]);

    // --- Auslastungs-Gauge (nur Worker-View) ---
    let ratio = if app.target_active > 0 {
        (active as f64 / app.target_active as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(ACCENT).bg(Color::Rgb(30, 34, 40)))
        .ratio(ratio)
        .label(Span::styled(
            format!("Auslastung {:.0}%", ratio * 100.0),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    let grows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(cols[1]);
    if app.view == View::Workers {
        f.render_widget(gauge, grows[1]);
    }

    // --- Live-Puls-Sparkline ---
    let samples = app.activity_samples();
    let srows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(cols[2]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Puls ", Style::default().fg(MUTED)),
            Span::styled(app.spinner_frame().to_string(), Style::default().fg(ACCENT)),
        ])),
        srows[0],
    );
    let spark = Sparkline::default()
        .data(&samples)
        .style(Style::default().fg(ACCENT));
    f.render_widget(spark, srows[1]);
}

/// Einladender Leerzustand statt toter Panels, wenn kein Worker-Pool läuft.
fn render_empty_state(f: &mut Frame, area: Rect) {
    let dim = Style::default().fg(Color::DarkGray);
    let accent = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
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
        let frac = if total > 0 {
            a.tasks_done as f64 / total as f64
        } else {
            0.0
        };
        let bar_mini = bar_spans(frac, 6, ACCENT);
        let activity = a
            .last_log_line
            .as_deref()
            .or_else(|| a.detail.last().map(String::as_str))
            .unwrap_or("—");
        items.push(ListItem::new(Line::from({
            let mut spans = vec![
                Span::styled(arrow.to_string(), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{marker} "), Style::default().fg(color)),
                Span::styled(
                    format!("{:<9}", crate::char_prefix(&a.brain, 9)),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {hb} "),
                    Style::default().fg(heartbeat_color(a.heartbeat_age_sec)),
                ),
            ];
            spans.extend(bar_mini);
            spans.push(Span::styled(
                format!(" {}", crate::char_prefix(activity, 22)),
                Style::default().fg(Color::DarkGray),
            ));
            spans
        })));

        if !open {
            continue;
        }
        let lines =
            crate::tui_state::build_detail_lines(&a.detail, DETAIL_MAX_ENTRIES, detail_width);
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
            items.push(detail_item(&format!(
                "… {rest} weitere Zeile(n), j/k blättert"
            )));
        } else if app.detail_scroll > 0 && !shown.is_empty() {
            // Wenn wir durch Scrollen nach oben in der History sind, zeigen wir
            // an, dass noch ältere Einträge vorhanden sind.
            items.push(detail_item("… ältere Einträge oben (j/k blättert)"));
        }
    }

    let list = List::new(items)
        .block(titled_block_focus(
            "Agenten",
            app.focus == crate::tui_state::Panel::Agents,
        ))
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
    state.select(Some(crate::tui_state::selected_row(
        &detail_rows,
        app.selected,
    )));

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
        let bar_frische = bar_spans(fraction, BAR_WIDTH, heartbeat_color(a.heartbeat_age_sec));

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
            Line::from({
                let mut spans = vec![Span::styled(
                    format!(" {:<LABEL_WIDTH$}", ""),
                    Style::default().fg(Color::DarkGray),
                )];
                spans.extend(bar_frische.clone());
                spans.push(Span::styled(
                    format!("  {remaining}s bis Timeout"),
                    Style::default().fg(Color::DarkGray),
                ));
                spans
            }),
            Line::from(vec![Span::raw("")]),
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
            format!(
                "(keine Zeile im Filter '{}' — f schaltet um)",
                filter.label()
            )
        };
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(Color::DarkGray),
        )));
    }

    let title = format!("Live Log [{}]", filter.label());
    let p = Paragraph::new(lines)
        .block(titled_block_focus(
            &title,
            app.focus == crate::tui_state::Panel::Log,
        ))
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
            Line::from({
                let mut spans = vec![Span::styled(
                    format!(" {:<LABEL_WIDTH$}", ""),
                    Style::default().fg(Color::DarkGray),
                )];
                spans.extend(bar_spans(fraction, BAR_WIDTH, Color::Green));
                spans.push(Span::styled(
                    format!("  {pct}%"),
                    Style::default().fg(Color::DarkGray),
                ));
                spans
            }),
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

    let p = Paragraph::new(content).block(titled_block_focus(
        "Tasks",
        app.focus == crate::tui_state::Panel::Tasks,
    ));

    f.render_widget(p, area);
}

/// Was die Kopfzeile ueber den Benchmark sagt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchStatus {
    /// Noch kein Ereignis.
    Bereit,
    /// Es kommen Meldungen.
    Aktiv,
    /// Lange nichts mehr gehoert, ohne dass ein Ende gemeldet wurde.
    Stillstand,
    /// Der Benchmark hat sich selbst beendet.
    Beendet,
}

impl BenchStatus {
    pub fn label(self) -> &'static str {
        match self {
            BenchStatus::Bereit => "bereit",
            BenchStatus::Aktiv => "aktiv",
            BenchStatus::Stillstand => "STILLSTAND",
            BenchStatus::Beendet => "beendet",
        }
    }
}

/// Leitet den Kopfstatus aus dem Ereignisstrom ab.
///
/// Vorher stand hier `if n > 0 { "aktiv" }` — der Status haing also allein
/// daran, ob JE eine Meldung kam, nicht daran, ob gerade etwas passiert. Am
/// 02.08.2026 beendete sich der Benchmark um 12:53:21 selbst („nach 2
/// unproduktiven Runden angehalten"), und die Kopfzeile behauptete drei
/// Stunden weiter „aktiv". Genau darum hielten Claude und opencode den Lauf
/// fuer lebendig und berichteten das auch so.
pub fn bench_status(
    event_count: usize,
    idle_seconds: Option<u64>,
    last_event_text: &str,
) -> BenchStatus {
    if event_count == 0 {
        return BenchStatus::Bereit;
    }
    // Ein gemeldetes Ende schlaegt jede Zeitrechnung: es ist kein Stillstand,
    // sondern ein Abschluss, und der Unterschied entscheidet, ob man neu
    // startet oder nach der Ursache sucht.
    let low = last_event_text.to_lowercase();
    if low.contains("angehalten")
        || low.contains("abgebrochen")
        || low.contains("benchmark beendet")
        || low.contains("run_finished")
    {
        return BenchStatus::Beendet;
    }
    match idle_seconds {
        Some(seconds) if seconds > STALL_WARN_SECONDS => BenchStatus::Stillstand,
        _ => BenchStatus::Aktiv,
    }
}

/// Ab hier gilt ein Lauf als verdaechtig still.
///
/// Bewusst grosszuegig: ein Brain-Turn dauert gemessen ~34s, ein `cargo test`
/// im Turn bis zu mehreren Minuten. Wer zu frueh Alarm schlaegt, erzieht zum
/// Wegsehen — und genau dann faellt der echte Stillstand nicht mehr auf.
const STALL_WARN_SECONDS: u64 = 600;

/// `615` → `10m 15s`. Sekunden allein liest im Minutenbereich niemand mehr.
fn human_duration(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// Footer: Keybindings — Tasten hervorgehoben, Beschriftung gedämpft.
///
/// Design-spezifische Tasten: j/k für Detail-Scroll, f für Log-Filter,
/// Tab für Panel-Fokus, Space für Expand.
fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    if app.input_mode == crate::tui_state::InputMode::CommandInput {
        let prompt = format!(" {}█", app.command_input);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                prompt,
                Style::default().fg(Color::Cyan),
            ))),
            area,
        );
        return;
    }

    let mut spans = vec![Span::raw(" ")];
    // Die Tastenleiste haengt an der Ansicht — in der Benchmark-Ansicht sind
    // Worker-Tasten (Tab/Filter/+/-) sinnlos.
    let binds: &[(&str, &str)] = match app.view {
        View::Workers => &[
            ("v", "ansicht"),
            ("↑↓", "wählen"),
            ("␣", "ausklappen"),
            ("Tab", "fokus"),
            ("f", "filter"),
            ("+/-", "worker"),
            ("↵", "task"),
            ("/", "kommando"),
            ("w", "kacheln"),
            ("q", "quit"),
        ],
        View::Bench => &[
            ("v", "ansicht"),
            ("j/k", "scroll"),
            ("g", "ans ende"),
            ("w", "kacheln"),
            ("q", "quit"),
        ],
    };
    for (k, label) in binds {
        spans.push(Span::styled(*k, key));
        spans.push(Span::styled(format!(" {label}  "), dim));
    }
    // Totmannschalter: Zeit seit dem letzten Ereignis.
    //
    // Am 02.08.2026 stand der Dauerlauf drei Stunden still und sah dabei aus
    // wie ein laufender — die TUI zeigte unveraendert den letzten Stand.
    // Ereignisse anzuzeigen genuegt nicht; das AUSBLEIBEN von Ereignissen ist
    // hier die Nachricht.
    if let Some(idle) = crate::bench_events::seconds_since_last_event() {
        let (label, style) = match idle {
            0..=STALL_WARN_SECONDS => (
                format!("│ letztes Ereignis vor {idle}s"),
                Style::default().fg(Color::DarkGray),
            ),
            _ => (
                format!("│ STILLSTAND — seit {} kein Ereignis", human_duration(idle)),
                Style::default()
                    .fg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
        };
        spans.push(Span::styled(label, style));
    }
    // Rueckmeldung der Brain-Kachelansicht rechts daneben. Ohne sie bliebe ein
    // fehlgeschlagenes Anordnen unsichtbar — die Fenster stehen off-screen,
    // man saehe also weder Erfolg noch Fehler.
    if !app.grid_status.is_empty() {
        spans.push(Span::styled(
            format!("│ {}", app.grid_status),
            Style::default().fg(ACCENT),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Farbe je Schweregrad einer Benchmark-Meldung.
fn level_color(level: Level) -> Color {
    match level {
        Level::Info => Color::Gray,
        Level::Progress => ACCENT,
        Level::Pass => Color::Green,
        Level::Fail => Color::Red,
        Level::Warn => Color::Yellow,
    }
}

/// Arbeits-/Benchmark-Ansicht: der Ereignisstrom aus [`crate::bench_events`].
///
/// Gerendert als ausklappbarer Baum: Jedes Ereignis ist ein Knoten, dessen
/// Detailblock (Terminal-Ausgabe, Edit-Ergebnis, Antworttext, Log-Payload) per
/// Space/Rechts eingerueckt unter ihm aufklappt. Navigation: Up/Down oder j/k
/// bewegen den Cursor, `g` springt ans untere Ende des frischen Stroms.
fn render_bench(f: &mut Frame, app: &App, area: Rect) {
    let block = titled_block(
        "Benchmark — Ereignisbaum (▸/▾: Space, aufklappen: →, zuklappen: ←, Ende: g)",
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let events = crate::bench_events::snapshot();
    if events.is_empty() {
        let hint = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Noch kein Benchmark-Ereignis.",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "  Starte `webagent benchmark …` — der Lauf meldet hierher.",
                Style::default().fg(MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  v / < >  zurück zum Worker-Dashboard",
                Style::default().fg(MUTED),
            )),
        ]);
        f.render_widget(hint, inner);
        return;
    }

    // Baum falten und das Fenster um den Cursor legen. `bench_scroll` ist
    // abgelegt: der Cursor ist jetzt der Anker (er bleibt im oberen Drittel,
    // damit unter ihm Platz fuer aufgeklappte Details bleibt).
    let lines = crate::tui_state::fold_bench_events(&events, &app.bench_expanded);
    let rows = inner.height as usize;
    let total = lines.len();
    if total == 0 {
        return;
    }
    let sel = app.bench_selected.min(total - 1);
    let keep = rows.saturating_div(3).max(1);
    let max_start = total.saturating_sub(rows);
    let start = sel.saturating_sub(keep).min(max_start);
    let end = (start + rows).min(total);

    let mut out: Vec<Line> = Vec::with_capacity(end - start);
    for (i, ln) in lines[start..end].iter().enumerate() {
        let abs = start + i;
        let selected = abs == sel;
        let base = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let mut spans: Vec<Span> = Vec::new();
        if ln.depth > 0 {
            spans.push(Span::styled("   │ ", base.fg(Color::DarkGray)));
        }
        if ln.is_node {
            let marker = if ln.has_children {
                if app.is_bench_expanded(ln.id) {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            };
            spans.push(Span::styled(marker, base.fg(Color::DarkGray)));
            spans.push(Span::styled(format!("{} ", ln.ts), base.fg(Color::DarkGray)));
            if let Some(b) = &ln.brain {
                spans.push(Span::styled(format!("{b:<9} "), base.fg(ACCENT)));
            }
            spans.push(Span::styled(ln.text.clone(), base.fg(level_color(ln.level))));
        } else {
            spans.push(Span::styled(ln.text.clone(), base.fg(Color::DarkGray)));
        }
        out.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(out), inner);
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

    #[test]
    fn kopfstatus_unterscheidet_beendet_von_aktiv() {
        // Der echte Fall vom 02.08.2026: der Benchmark beendete sich um
        // 12:53:21 selbst, die Kopfzeile stand drei Stunden auf "aktiv".
        let halt = "Fehler: Benchmark nach 2 unproduktiven Runden angehalten \
                    - kein Kandidat bestand Build/Test/Scope-Gates.";
        assert_eq!(
            bench_status(2000, Some(11_160), halt),
            BenchStatus::Beendet,
            "ein gemeldetes Ende ist kein Stillstand und erst recht nicht aktiv"
        );

        // Kein Ende gemeldet, aber lange nichts gehoert -> Stillstand.
        assert_eq!(
            bench_status(2000, Some(11_160), "deepseek: Browser: warte auf Antwort"),
            BenchStatus::Stillstand
        );
        // Frische Meldung -> aktiv.
        assert_eq!(
            bench_status(2000, Some(5), "deepseek: Browser: Antwort empfangen"),
            BenchStatus::Aktiv
        );
        // Noch nichts passiert -> bereit, nicht aktiv.
        assert_eq!(bench_status(0, None, ""), BenchStatus::Bereit);
    }

    #[test]
    fn stillstand_wird_lesbar_formatiert() {
        // Sekunden allein liest im Stundenbereich niemand — der Lauf stand
        // 3h 6m, und genau das muss dastehen.
        assert_eq!(human_duration(45), "45s");
        assert_eq!(human_duration(615), "10m 15s");
        assert_eq!(human_duration(11_160), "3h 6m");
    }

    #[test]
    fn stillstandsschwelle_ist_groesser_als_ein_langsamer_turn() {
        // Ein Brain-Turn mit cargo test darin dauert gemessen bis zu 14 min…
        // aber wer bei jedem langsamen Turn Alarm schlaegt, erzieht zum
        // Wegsehen. 10 min ist der Kompromiss; dieser Test haelt die
        // Begruendung fest, damit die Zahl nicht unbemerkt verrutscht.
        assert!(STALL_WARN_SECONDS >= 300, "zu nervoes");
        assert!(STALL_WARN_SECONDS <= 1800, "zu traege - 3h Stillstand fiel so durch");
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
            activity_history: std::collections::VecDeque::new(),
            view: crate::tui_state::View::Workers,
            bench_scroll: 0,
            bench_expanded: std::collections::HashSet::new(),
            bench_selected: 0,
            command_input: String::new(),
            grid_status: String::new(),
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
    fn balken_faerbt_nur_den_gefuellten_teil() {
        // Regression: frueher wurde der GANZE Balken einfarbig gerendert,
        // wodurch die leeren Zeichen wie Fuellung aussahen — ein Balken bei
        // 0% wirkte komplett gefuellt.
        let spans = bar_spans(0.0, 10, Color::Green);
        let gefuellt: String = spans[1].content.to_string();
        let leer: String = spans[2].content.to_string();
        assert!(gefuellt.is_empty(), "bei 0% darf nichts gefuellt sein");
        assert_eq!(leer.chars().count(), 10);
        assert_ne!(
            spans[1].style.fg, spans[2].style.fg,
            "gefuellt und leer muessen unterscheidbar sein"
        );
    }

    #[test]
    fn balken_haelt_breite_und_grenzen_ein() {
        for (frac, erwartet_gefuellt) in [(0.5, 5usize), (1.0, 10), (1.5, 10), (-0.5, 0)] {
            let spans = bar_spans(frac, 10, Color::Green);
            let g = spans[1].content.chars().count();
            let l = spans[2].content.chars().count();
            assert_eq!(g, erwartet_gefuellt, "frac={frac}");
            assert_eq!(g + l, 10, "Gesamtbreite verletzt bei frac={frac}");
        }
        let spans = bar_spans(0.5, 10, Color::Green);
        assert_eq!(spans[0].content.as_ref(), "[");
        assert_eq!(spans[3].content.as_ref(), "]");
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
        assert!(
            heartbeat_pip(3).starts_with('♥'),
            "frisch = gefuellter Puls"
        );
        assert!(heartbeat_pip(120).contains("2m"));
        assert!(heartbeat_pip(120).starts_with('♡'), "aelter = leerer Puls");
        assert_eq!(heartbeat_pip(u64::MAX), "· —", "nie gesehen = kein Puls");
    }
}
