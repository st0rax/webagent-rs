//! tui_bench — die Arbeits-/Benchmark-Ansicht der ratatui-TUI.
//!
//! Aus `tui_render.rs` herausgeloest (Refactoring 02:45): der komplette
//! Benchmark-Block — Kopfstatus (Bereit/Aktiv/Stillstand/Beendet), der
//! ausklappbare Ereignisbaum, das Cursor-Fenster und der Schweregrad-Farb-
//! Zuordnung. So bleibt `tui_render.rs` die Dashboard-Ansicht (Worker/Status/
//! Tasks/Log/Faehigkeiten/Einstellungen) und der Lauf selbst lebt getrennt.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::bench_events::Level;
use crate::tui_state::App;
use crate::tui_widgets::{titled_block, ACCENT, MUTED};

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
pub(crate) const STALL_WARN_SECONDS: u64 = 600;

/// `615` → `10m 15s`. Sekunden allein liest im Minutenbereich niemand mehr.
pub(crate) fn human_duration(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds}s"),
        60..=3599 => format!("{}m {}s", seconds / 60, seconds % 60),
        _ => format!("{}h {}m", seconds / 3600, (seconds % 3600) / 60),
    }
}

/// Erste sichtbare Zeile des Ereignisbaums.
///
/// Der Cursor ist der Anker und bleibt im oberen Drittel, damit unter ihm Platz
/// fuer aufgeklappte Details bleibt.
///
/// Eigene Funktion, weil zwei Stellen dieselbe Antwort brauchen: das Rendern und
/// die Maus (welche Zeile liegt unter dem Klick?). Zwei Kopien dieser Formel
/// waeren ein Klick, der eine andere Zeile trifft als die, auf die man zeigt.
pub fn bench_window_start(total: usize, rows: usize, selected: usize) -> usize {
    let keep = rows.saturating_div(3).max(1);
    let max_start = total.saturating_sub(rows);
    selected.saturating_sub(keep).min(max_start)
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
pub(crate) fn render_bench(f: &mut Frame, app: &App, area: Rect) {
    let block =
        titled_block("Benchmark — Ereignisbaum (▸/▾: Space, aufklappen: →, zuklappen: ←, Ende: g)");
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
    let start = bench_window_start(total, rows, sel);
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
            spans.push(Span::styled(
                format!("{} ", ln.ts),
                base.fg(Color::DarkGray),
            ));
            if let Some(b) = &ln.brain {
                spans.push(Span::styled(format!("{b:<9} "), base.fg(ACCENT)));
            }
            spans.push(Span::styled(
                ln.text.clone(),
                base.fg(level_color(ln.level)),
            ));
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
        const _: () = assert!(STALL_WARN_SECONDS >= 300, "zu nervoes");
        const _: () = assert!(
            STALL_WARN_SECONDS <= 1800,
            "zu traege - 3h Stillstand fiel so durch"
        );
    }
}
