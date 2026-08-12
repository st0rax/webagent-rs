//! tui_widgets — gemeinsame Render-Bausteine der ratatui-TUI.
//!
//! Aus `tui_render.rs` herausgeloest (Refactoring 02:45): Farben, Status-
//! Symbole, Balken und Rahmen-Blöcke sind Wiederverwendungs-Bausteine, die
//! jedes Panel nutzt. Indem sie hier liegen, bleibt `tui_render.rs` reines
//! Layout (Panels + Reihenfolge) und ein Panel-Split wird unabhängig von den
//! Grundformen möglich.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
};

/// Akzentfarbe der Oberfläche (ein durchgängiger Ton statt bunt gemischt).
pub(crate) const ACCENT: Color = Color::Rgb(94, 197, 214); // gedämpftes Cyan
/// Gedämpfter Text (Labels, Rahmen).
pub(crate) const MUTED: Color = Color::Rgb(110, 120, 130);

/// Farben für Status.
pub(crate) fn status_color(status: &str) -> Color {
    match status {
        "active" => Color::Green,
        "available" => Color::Yellow,
        "cooldown" => Color::Blue,
        _ => Color::Red,
    }
}

/// Heartbeat-Ampel (grün <60s, gelb <300s, rot >=300s).
pub(crate) fn heartbeat_color(age_sec: u64) -> Color {
    if age_sec < 60 {
        Color::Green
    } else if age_sec < 300 {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Heartbeat gilt ab hier als tot (Supervisor killt stale Worker).
pub(crate) const HEARTBEAT_TIMEOUT_SEC: u64 = 300;

/// Kompakte Heartbeat-Anzeige: Puls-Symbol + Alter. Frisch grün, alt rot
/// (Farbe kommt aus [`heartbeat_color`]). Zeigt auf einen Blick, ob ein Worker
/// noch lebt.
pub(crate) fn heartbeat_pip(age_sec: u64) -> String {
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
pub(crate) const LABEL_WIDTH: usize = 10;
/// Breite der Text-Fortschrittsbalken.
pub(crate) const BAR_WIDTH: usize = 16;

/// Statuspunkt für die schnelle Erfassung in der Liste.
pub(crate) fn status_glyph(status: &str) -> &'static str {
    match status {
        "active" => "●",
        "available" => "○",
        "cooldown" => "◐",
        _ => "✕",
    }
}

/// Ausgerichtete „Label   Wert"-Zeile mit optionaler Wert-Farbe.
pub(crate) fn kv_line(label: &str, value: impl Into<String>, value_style: Style) -> Line<'static> {
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
pub(crate) fn bar_spans(fraction: f64, width: usize, fg: Color) -> Vec<Span<'static>> {
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
pub(crate) fn titled_block(title: &str) -> Block<'static> {
    titled_block_focus(title, false)
}

/// Rahmen-Block, dessen Rand+Titel bei `focused` hervorgehoben werden — so ist
/// das per Tab fokussierte Panel sichtbar (Gewinner-Design 2026-07-22).
pub(crate) fn titled_block_focus(title: &str, focused: bool) -> Block<'static> {
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

/// Bricht Text auf `width` Zeichen um, an Wortgrenzen.
pub(crate) fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width < 8 {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

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
