//! tui_footer — die Tastenleiste der ratatui-TUI.
//!
//! Aus `tui_render.rs` herausgeloest (Refactoring 02:45): Fusszeile als
//! einzige Quelle fuer Darstellung UND Trefferflaeche. `footer_binds` und
//! `footer_zones` sind bewusst `pub`: die Maus laeuft ueber dieselben Zonen,
//! damit ein Klick genau das trifft, was dransteht.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui_bench::{human_duration, STALL_WARN_SECONDS};
use crate::tui_state::{App, InputMode, View};
use crate::tui_widgets::ACCENT;

/// Ein Eintrag der Fusszeile: Beschriftung UND anklickbare Handlung.
///
/// `key` ist, was dransteht; `action` ist, was ein Klick ausloest. Beides
/// auseinanderzuhalten ist noetig, weil Eintraege wie `j/k` oder `+/-` zwei
/// Tasten anzeigen, ein Klick aber genau eine Handlung meinen muss.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FooterBind {
    pub key: &'static str,
    pub label: &'static str,
    /// Tastenname fuer [`crate::tui_keys::parse_key`] — die Maus laeuft ueber
    /// dieselbe Tastenlogik, damit es keinen zweiten Bedienpfad gibt.
    pub action: &'static str,
}

const fn bind(key: &'static str, label: &'static str, action: &'static str) -> FooterBind {
    FooterBind { key, label, action }
}

/// Tastenleiste der Ansicht — einzige Quelle fuer Darstellung und Trefferflaeche.
///
/// Die Leiste haengt an der Ansicht: in der Benchmark-Ansicht sind
/// Worker-Tasten (Tab/Filter/+/-) sinnlos.
pub fn footer_binds(view: View) -> &'static [FooterBind] {
    const CAPABILITIES: [FooterBind; 5] = [
        bind("v", "ansicht", "v"),
        bind("j/k", "wählen", "j"),
        bind("t", "schalten", "t"),
        bind("w", "kacheln", "w"),
        bind("q", "quit", "q"),
    ];
    const WORKERS: [FooterBind; 10] = [
        bind("v", "ansicht", "v"),
        bind("↑↓", "wählen", "down"),
        bind("␣", "ausklappen", "space"),
        bind("Tab", "fokus", "tab"),
        bind("f", "filter", "f"),
        bind("+/-", "worker", "+"),
        bind("↵", "task", "enter"),
        bind("/", "kommando", "/"),
        bind("w", "kacheln", "w"),
        bind("q", "quit", "q"),
    ];
    const BENCH: [FooterBind; 5] = [
        bind("v", "ansicht", "v"),
        bind("j/k", "scroll", "j"),
        bind("g", "ans ende", "g"),
        bind("w", "kacheln", "w"),
        bind("q", "quit", "q"),
    ];
    const CONFIG: [FooterBind; 6] = [
        bind("v", "ansicht", "v"),
        bind("j/k", "wählen", "j"),
        bind("t", "schalten", "t"),
        bind("r", "zuruecksetzen", "r"),
        bind("w", "kacheln", "w"),
        bind("q", "quit", "q"),
    ];
    const SESSION: [FooterBind; 5] = [
        bind("v", "ansicht", "v"),
        bind("/", "befehl", "/"),
        bind("j/k", "scroll", "j"),
        bind("w", "kacheln", "w"),
        bind("q", "quit", "q"),
    ];
    match view {
        View::Capabilities => &CAPABILITIES,
        View::Workers => &WORKERS,
        View::Bench => &BENCH,
        View::Config => &CONFIG,
        View::Session => &SESSION,
    }
}

/// Spaltenbereiche der Fusszeilen-Knoepfe: `(start, ende_exklusiv, action)`.
///
/// Rechnet exakt das Layout von [`render_footer`] nach: ein Leerzeichen
/// Vorlauf, dann je Eintrag `key` + `" {label}  "`. Die Trefferflaeche umfasst
/// Taste UND Beschriftung — wer auf das Wort „kacheln" zielt, trifft.
/// Gemessen in Zeichenzellen, wie ratatui rendert.
pub fn footer_zones(view: View) -> Vec<(u16, u16, &'static str)> {
    let mut zones = Vec::new();
    let mut x: u16 = 1; // fuehrendes Span::raw(" ")
    for b in footer_binds(view) {
        let width = (b.key.chars().count() + 1 + b.label.chars().count() + 2) as u16;
        zones.push((x, x + width, b.action));
        x += width;
    }
    zones
}

/// Footer: Keybindings — Tasten hervorgehoben, Beschriftung gedämpft.
///
/// Design-spezifische Tasten: j/k für Detail-Scroll, f für Log-Filter,
/// Tab für Panel-Fokus, Space für Expand.
pub(crate) fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let key = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);
    if app.input_mode == InputMode::CommandInput {
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
    for b in footer_binds(app.view) {
        spans.push(Span::styled(b.key, key));
        spans.push(Span::styled(format!(" {}  ", b.label), dim));
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
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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
