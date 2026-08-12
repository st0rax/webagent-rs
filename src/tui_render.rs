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
use crate::tui_bench::{bench_status, render_bench, BenchStatus};
use crate::tui_footer::render_footer;
use crate::tui_state::{App, CapState, View};
use crate::tui_widgets::{
    bar_spans, heartbeat_color, heartbeat_pip, kv_line, status_color, status_glyph,
    titled_block, titled_block_focus, wrap_text, ACCENT, BAR_WIDTH, HEARTBEAT_TIMEOUT_SEC,
    LABEL_WIDTH, MUTED,
};

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

    if app.view == View::Capabilities {
        render_capabilities(f, app, outer[1]);
        render_footer(f, app, outer[2]);
        return;
    }

    if app.view == View::Config {
        render_config(f, app, outer[1]);
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
        View::Config => {
            // Der Kopf zaehlt, wie viel vom Zustand nicht mehr Vorgabe ist.
            // Genau das ist die Frage, die man bei einem unerwarteten Verhalten
            // zuerst stellt: „was steht hier anders als ueberall sonst?"
            let rows = crate::tui_config::rows();
            let abweichend = rows
                .iter()
                .filter(|r| r.source != crate::tui_config::Source::Vorgabe)
                .count();
            vec![
                Line::from(vec![
                    Span::styled(
                        format!("◇ {abweichend} von {} abweichend", rows.len()),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("   "),
                    Span::styled(
                        if abweichend == 0 {
                            "alles auf Vorgabe".to_string()
                        } else {
                            "gelb = beim Start gesetzt, cyan = gespeichert".to_string()
                        },
                        Style::default().fg(MUTED),
                    ),
                ]),
                Line::from(Span::styled(
                    if app.cfg_status.is_empty() {
                        "t schaltet, r setzt zurueck".to_string()
                    } else {
                        app.cfg_status.clone()
                    },
                    Style::default().fg(MUTED),
                )),
            ]
        }
        View::Capabilities => {
            // Gesamtstand ueber alle Brains: erreicht / erreichbar. Der Nenner
            // ist die Summe der ANGEBOTENEN Optionen, nicht ein Wunschwert —
            // ein Maximum, das niemand erreichen kann, ist kein Massstab.
            let levels = crate::capability::levels_all();
            let have: usize = levels.iter().map(|l| l.level()).sum();
            let max: usize = levels.iter().filter_map(|l| l.max_level()).sum();
            let unsurveyed = levels.iter().filter(|l| l.max_level().is_none()).count();
            vec![
                Line::from(vec![
                    Span::styled(
                        format!("◇ {have}/{max} Faehigkeiten fahrbar"),
                        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("   "),
                    Span::styled(
                        if unsurveyed > 0 {
                            format!("{unsurveyed} Brain(s) unvermessen")
                        } else {
                            "alle vermessen".to_string()
                        },
                        Style::default().fg(if unsurveyed > 0 { Color::Yellow } else { MUTED }),
                    ),
                ]),
                Line::from(Span::styled(
                    if app.cap_status.is_empty() {
                        "t schaltet die gewaehlte Faehigkeit".to_string()
                    } else {
                        app.cap_status.clone()
                    },
                    Style::default().fg(Color::Gray),
                )),
            ]
        }
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

/// Einstellungen-Ansicht: Wert, Herkunft und Wirkung nebeneinander.
///
/// Die Herkunft steht bewusst gleichberechtigt neben dem Wert. „Geteilter
/// Browser: an" beantwortet die halbe Frage; erst „an (Umgebung)" sagt, ob
/// jemand das beim Start gesetzt hat oder ob es die Vorgabe ist. Genau diese
/// Luecke hat am 07.08.2026 einen halben Tag gekostet: `start_tui.ps1` setzte
/// `WEBAGENT_USE_SHARED_BROWSER` still, und niemand sah, aus welchem Profil der
/// Lauf klont.
fn render_config(f: &mut Frame, app: &App, area: Rect) {
    let block = titled_block("Einstellungen (t schaltet, r setzt zurueck)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = crate::tui_config::rows();
    let selected = app.cfg_selected.min(rows.len().saturating_sub(1));

    let mut lines: Vec<Line> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        let base = if i == selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        // Die Vorgabe gedaempft, alles Gesetzte hervorgehoben: was jemand
        // bewusst geaendert hat, soll ins Auge fallen.
        let value_style = match row.source {
            crate::tui_config::Source::Umgebung => base.fg(Color::Yellow),
            crate::tui_config::Source::Gespeichert => base.fg(ACCENT),
            crate::tui_config::Source::Vorgabe => base.fg(MUTED),
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {:<28}", row.label), base),
            Span::styled(format!("{:<12}", row.value), value_style),
            Span::styled(format!("({})", row.source.label()), base.fg(MUTED)),
        ]));
    }

    // Erklaerung der gewaehlten Zeile darunter, umgebrochen. Eine
    // Stellschraube, deren Wirkung man raten muss, wird nicht benutzt.
    if let Some(row) = rows.get(selected) {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  {}", row.key),
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        )));
        for chunk in wrap_text(row.help, inner.width.saturating_sub(4) as usize) {
            lines.push(Line::from(Span::styled(
                format!("  {chunk}"),
                Style::default().fg(MUTED),
            )));
        }
    }

    // Aenderungen greifen nicht rueckwirkend — das gehoert dazu, sonst wartet
    // jemand auf eine Wirkung, die im laufenden Lauf gar nicht kommen kann.
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Gilt ab dem naechsten Lauf; gespeichert in data/settings.json.",
        Style::default().fg(MUTED),
    )));
    if !app.cfg_status.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", app.cfg_status),
            Style::default().fg(ACCENT),
        )));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Faehigkeiten-Ansicht: was jedes Brain kann, und was sich davon schalten laesst.
///
/// Nicht fahrbare Faehigkeiten stehen bewusst mit drin, nur gedaempft. Ein
/// Panel, das ausschliesslich das Erreichte zeigt, sieht immer fertig aus —
/// die Luecke ist hier die eigentliche Information.
fn render_capabilities(f: &mut Frame, app: &App, area: Rect) {
    let block = titled_block("Faehigkeiten je Brain (↑↓ waehlen, t schalten)");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = crate::tui_state::capability_rows(&crate::capability::levels_all());
    if rows.is_empty() {
        f.render_widget(
            Paragraph::new("Keine Brains registriert.").style(Style::default().fg(MUTED)),
            inner,
        );
        return;
    }

    let height = inner.height as usize;
    let first = app.cap_selected.saturating_sub(height.saturating_sub(1) / 2);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(index, row)| {
            let selected = index == app.cap_selected;
            let marker = if selected { "▸ " } else { "  " };
            match row.key {
                None => Line::from(Span::styled(
                    format!("{marker}{}", row.label),
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                )),
                Some(_) => {
                    let (symbol, style) = match row.state {
                        CapState::Driveable => (
                            "✓",
                            Style::default().fg(Color::Green),
                        ),
                        CapState::Quest => ("·", Style::default().fg(Color::Yellow)),
                        CapState::OutOfReach => ("—", Style::default().fg(MUTED)),
                    };
                    let style = if selected {
                        style.add_modifier(Modifier::REVERSED)
                    } else {
                        style
                    };
                    Line::from(Span::styled(
                        format!("{marker}  {symbol} {}", row.label),
                        style,
                    ))
                }
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
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
            activity_history: std::collections::VecDeque::new(),
            view: crate::tui_state::View::Workers,
            bench_scroll: 0,
            bench_expanded: std::collections::HashSet::new(),
            bench_selected: 0,
            command_input: String::new(),
            grid_status: String::new(),
            cap_selected: 0,
            cap_status: String::new(),
            cfg_selected: 0,
            cfg_status: String::new(),
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
    fn detail_item_creates_indented_list_item() {
        // Just test the function doesn't panic - we can't inspect private fields
        let _item = detail_item("Test message");
    }
}
