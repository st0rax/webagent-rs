//! observer — Zuverlässige Antworterkennung (plattformreine Teile).
//!
//! Die DOM-abhängige `ResponseObserver.wait_for_response`-Logik wird später
//! als Trait am Browser-Rand implementiert.

use regex::Regex;
use std::sync::OnceLock;

/// Regex für transiente UI-Status-Labels (Denke nach, Thinking, etc.).
fn transient_status_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:denke\s+nach|thinking(?:\s*\.\.\.)?(?:\s+skip)?|reasoning|ueberlege|überlege|思考|generating|loading|skip|thought\s*process)\s*[.….]*$"
        )
        .unwrap()
    })
}

/// Regex für reine Zeitanzeigen (z.B. "11:05").
fn clock_only_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^\d{1,2}:\d{2}$").unwrap())
}

/// Regex für Limit-/Quota-Meldungen in einer Web-UI.
fn limit_response_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:usage\s+limit|message\s+limit|rate\s+limit|nachrichtenlimit|limit\s+reached|too\s+many\s+messages|quota\s+exceeded|keine\s+kostenlosen|free\s+messages?\s+(?:used|left|remaining)|you\s+have\s+reached|daily\s+limit|conversation\s+limit|out\s+of\s+(?:free\s+)?messages|send\s+limit)"
        )
        .unwrap()
    })
}

/// Obergrenze, bis zu der ein Text überhaupt ein Statuslabel sein kann.
/// Fließtext dieser Länge gibt es praktisch nicht; alles Längere ist Inhalt.
const STATUS_LABEL_MAX_CHARS: usize = 60;

/// True, wenn der Text aus **einem einzigen, mehrfach wiederholten** Wort
/// besteht, das wie ein Gerundium aussieht ("Weighing Weighing").
///
/// Claudes Denk-Anzeige rotiert durch einen offenen Wortschatz — real
/// beobachtet: Crystallizing, Triangulating, Weighing. Eine feste Vokabelliste
/// (wie [`transient_status_regex`]) holt das nie ein; deshalb prüft diese
/// Funktion die **Form** statt der Vokabel. Das DOM liefert das Label doppelt
/// (Label + aria-Kopie), und genau diese Wiederholung ist das verlässliche
/// Signal: eine echte Antwort besteht nie aus demselben Wort zweimal.
/// Ein einzelnes Wort bleibt bewusst erlaubt, damit eine legitime
/// Einwortantwort ("Running") nicht verschluckt wird.
fn is_repeated_gerund_label(normalized: &str) -> bool {
    if normalized.chars().count() > STATUS_LABEL_MAX_CHARS {
        return false;
    }
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    if tokens.len() < 2 || tokens.len() > 4 {
        return false;
    }
    let first = tokens[0];
    if first.chars().count() < 5 || !first.chars().all(|c| c.is_alphabetic()) {
        return false;
    }
    if !first.to_ascii_lowercase().ends_with("ing") {
        return false;
    }
    tokens.iter().all(|t| t.eq_ignore_ascii_case(first))
}

/// True, wenn ein kurzer Text ein Zeichen aus der Unicode-Private-Use-Area
/// enthält. Solche Glyphen stammen aus Icon-Fonts der Oberfläche (Claudes
/// Denk-Anzeige trug U+E027) und kommen in echtem Antworttext nicht vor.
fn has_private_use_glyph(normalized: &str) -> bool {
    normalized.chars().count() <= STATUS_LABEL_MAX_CHARS
        && normalized
            .chars()
            .any(|c| matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD))
}

/// Entfernt eine doppelt vorangestellte Kopfzeile aus dem Antworttext.
///
/// Claude rendert die Zusammenfassung seines Denkvorgangs in denselben
/// Container wie die Antwort, und das DOM liefert sie doppelt:
///
/// ```text
/// Synthesized technical criteria for resilient selectors
///
/// Synthesized technical criteria for resilient selectors
/// Robuste Web-Automatisierung braucht ...
/// ```
///
/// Die unmittelbare Wiederholung ist das Erkennungsmerkmal — ein echter Text
/// beginnt nicht mit derselben Zeile zweimal. Beide Kopien fallen weg, denn
/// die Zeile gehört zum Denkvorgang, nicht zur Antwort. Bleibt danach nichts
/// übrig, wird nichts entfernt: lieber verunreinigt als leer.
pub fn strip_repeated_lead_line(text: &str) -> String {
    // Glyph-bereinigt vergleichen: die Oberflaeche haengt an die erste Kopie
    // ein Icon-Zeichen aus der Private-Use-Area (real: U+E027), an die zweite
    // nicht. Ein roher Vergleich haelt die beiden dann faelschlich fuer
    // verschieden und laesst die Kopfzeile stehen.
    let key = |l: &str| -> String {
        l.chars()
            .filter(
                |c| !matches!(*c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD),
            )
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut lines = text.lines();
    let head = match lines.by_ref().find(|l| !key(l).is_empty()) {
        Some(h) => key(h),
        None => return text.to_string(),
    };
    // Kopfzeilen sind kurz; ein wiederholter Absatz waere Inhalt.
    if head.chars().count() > 120 {
        return text.to_string();
    }
    let rest: Vec<&str> = lines.collect();
    let next_idx = match rest.iter().position(|l| !key(l).is_empty()) {
        Some(i) => i,
        None => return text.to_string(),
    };
    if key(rest[next_idx]) != head {
        return text.to_string();
    }
    let tail = rest[next_idx + 1..].join("\n");
    if tail.trim().is_empty() {
        return text.to_string();
    }
    tail.trim_start_matches('\n').to_string()
}

/// True für UI-Fortschritts-Labels, die keine echten Modellantworten sind.
pub fn is_transient_response_text(text: &str) -> bool {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();

    if normalized.is_empty() {
        return true;
    }

    if clock_only_regex().is_match(normalized) {
        return true;
    }

    if has_private_use_glyph(normalized) {
        return true;
    }

    if is_repeated_gerund_label(normalized) {
        return true;
    }

    transient_status_regex().is_match(normalized)
}

/// True wenn die Web-UI eine Usage/Rate-Limit-Banner statt einer Antwort zeigt.
pub fn is_limit_response_text(text: &str) -> bool {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();

    if normalized.is_empty() {
        return false;
    }

    limit_response_regex().is_match(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicated_thinking_headline_is_stripped_from_answer() {
        // Real beobachtet am 2026-07-27 nach dem Warte-Fix: die Antwort kam,
        // trug aber Claudes Denk-Ueberschrift doppelt vor sich her.
        let raw = "Synthesized technical criteria for resilient selectors\n\nSynthesized technical criteria for resilient selectors\nRobuste Selektoren brauchen drei Dinge.";
        assert_eq!(
            strip_repeated_lead_line(raw),
            "Robuste Selektoren brauchen drei Dinge."
        );
    }

    #[test]
    fn strips_exact_string_observed_from_relay() {
        // 1:1 aus der relay-Ausgabe vom 2026-07-27 (Leerzeile auch NACH dem Paar).
        let raw = "Synthesized drei Robustheitskriterien für Web-Selektoren\n\nSynthesized drei Robustheitskriterien für Web-Selektoren\n\nAntwortsprache: Deutsch.";
        assert_eq!(strip_repeated_lead_line(raw), "Antwortsprache: Deutsch.");
    }

    #[test]
    fn strips_pair_even_when_only_one_copy_carries_the_icon_glyph() {
        // Die Oberflaeche haengt den Icon-Glyph nur an die erste Kopie. Roh
        // verglichen sind die Zeilen dann verschieden — und die Kopfzeile blieb
        // stehen, obwohl beide Auslesepfade schon bereinigt waren.
        let raw =
            "\u{E027}Synthesized drei Kriterien\n\nSynthesized drei Kriterien\n\nAntwort hier.";
        assert_eq!(strip_repeated_lead_line(raw), "Antwort hier.");
    }

    #[test]
    fn strip_repeated_lead_line_leaves_normal_text_alone() {
        for s in [
            "Robuste Selektoren brauchen drei Dinge.",
            "Erste Zeile\nZweite Zeile\nErste Zeile", // Wiederholung nicht unmittelbar
            "",
            "Nur eine Zeile",
        ] {
            assert_eq!(
                strip_repeated_lead_line(s),
                s,
                "unveraendert erwartet: {s:?}"
            );
        }
    }

    #[test]
    fn strip_repeated_lead_line_never_empties_the_answer() {
        // Wenn nach dem Abschneiden nichts bliebe, ist die Wiederholung der
        // Inhalt — dann lieber verunreinigt als leer zurueckgeben.
        let raw = "Ja\n\nJa";
        assert_eq!(strip_repeated_lead_line(raw), raw);
    }

    #[test]
    fn rotating_thinking_labels_are_transient() {
        // Real am 2026-07-27 ueber `relay` beobachtet: der
        // Harness lieferte diese als fertige Antwort mit ok=true zurueck.
        // Eine Vokabelliste haette sie nie gefangen — das Wort rotiert.
        for s in [
            "Weighing\n\nWeighing",
            "Crystallizing\n\nCrystallizing",
            "Triangulating\n\nTriangulating",
            "Pondering Pondering",
        ] {
            assert!(
                is_transient_response_text(s),
                "Denk-Anzeige als Antwort durchgelassen: {s:?}"
            );
        }
    }

    #[test]
    fn real_answers_are_not_mistaken_for_thinking_labels() {
        for s in [
            "Testwort",
            "Running",               // Einwort-Gerundium bleibt gueltig
            "Running the tests now", // verschiedene Woerter
            "Ja",
            "Weighing the options carefully against each other",
            "42",
        ] {
            assert!(
                !is_transient_response_text(s),
                "echte Antwort faelschlich als Denk-Anzeige verworfen: {s:?}"
            );
        }
    }

    #[test]
    fn private_use_glyphs_mark_ui_chrome_not_content() {
        // Claudes Denk-Anzeige trug U+E027 aus einer Icon-Font.
        assert!(is_transient_response_text("\u{E027} Crystallizing"));
        assert!(is_transient_response_text("\u{E027}"));
        // In langem Fliesstext ist ein solches Zeichen kein Statuslabel.
        let long = format!("{} {}", "wort ".repeat(40), "\u{E027}");
        assert!(!is_transient_response_text(&long));
    }

    #[test]
    fn test_transient_progress_labels_are_not_final_responses() {
        let cases = vec![
            "",
            "   ",
            "Denke nach…",
            "Denke nach...",
            "Thinking…",
            "Thinking...",
            "Thinking... Skip",
            "11:05",
            "9:32",
            "reasoning",
            "Überlege …",
            "Skip",
            "skip",
            "Thought Process",
            "thought process",
        ];

        for text in cases {
            assert!(
                is_transient_response_text(text),
                "Expected '{}' to be transient",
                text
            );
        }
    }

    #[test]
    fn test_real_response_text_is_not_transient() {
        let cases = vec![
            r#"{"protocol":"webagent/1","actions":[]}"#,
            "Thinking\nHere is the actual answer",
            "Denke nach: Ergebnis liegt vor",
            "Eine normale Antwort",
        ];

        for text in cases {
            assert!(
                !is_transient_response_text(text),
                "Expected '{}' to NOT be transient",
                text
            );
        }
    }

    #[test]
    fn test_limit_messages_are_detected() {
        let cases = vec![
            "You have reached your usage limit for today.",
            "Message limit reached. Try again later.",
            "Nachrichtenlimit erreicht",
            "Rate limit exceeded",
        ];

        for text in cases {
            assert!(
                is_limit_response_text(text),
                "Expected '{}' to be a limit message",
                text
            );
        }
    }

    #[test]
    fn test_non_limit_responses_are_not_limits() {
        let cases = vec![
            "METHODOLOGY:C REASON:Balanced mix.",
            "The counter is 8.",
            "Thinking\nHere is the actual answer",
        ];

        for text in cases {
            assert!(
                !is_limit_response_text(text),
                "Expected '{}' to NOT be a limit message",
                text
            );
        }
    }
}
