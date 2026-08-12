//! Textdarstellung der REPL: Chat-Aufbereitung, Sichtbreite, Boxen und
//! Token-/Dauer-Formatierung.
//!
//! Aus `repl::mod` extrahiert (Schritt 7) — reine Moves, keine Logikänderung.

pub(crate) fn get_facts_string() -> String {
    let readme = std::fs::read_to_string("README.md").unwrap_or_else(|_| "README nicht gefunden".to_string());
    let progress = std::fs::read_to_string("PROGRESS.md").unwrap_or_else(|_| "PROGRESS nicht gefunden".to_string());
    let mut modules = Vec::new();
    if let Ok(entries) = std::fs::read_dir("src") {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().map(|s| s == "rs").unwrap_or(false) {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string()) {
                    let size = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
                    modules.push((name, size));
                }
            }
        }
    }
    crate::self_research::build_facts(&readme, &progress, &modules, 2000)
}

/// Antworttext für die `/chat`-Anzeige aufbereiten: steckt das Brain nach einer
/// autonomen Aufgabe noch im webagent/1-Protokoll-Modus, kommt die Chat-Antwort
/// als JSON-Envelope zurück. Dann den Klartext der message-Actions zeigen statt
/// des rohen JSON; alles andere unverändert durchreichen.
pub(crate) fn display_chat_text(raw: &str) -> String {
    let parsed = crate::protocol::parse(raw);
    if parsed.valid {
        let texts: Vec<&str> = parsed
            .actions
            .iter()
            .filter(|a| {
                a.action_type == crate::protocol::ActionType::Message && !a.text.trim().is_empty()
            })
            .map(|a| a.text.trim())
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    // Fallback für Envelope-Varianten, die protocol::parse ablehnt (z.B. ohne
    // "protocol"-Feld oder als einzelnes message-Objekt).
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        let collect_msgs = |arr: &[serde_json::Value]| -> Vec<String> {
            arr.iter()
                .filter(|a| a.get("type").and_then(|t| t.as_str()) == Some("message"))
                .filter_map(|a| a.get("text").and_then(|t| t.as_str()))
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        };
        let msgs = match (&v, v.get("actions").and_then(|a| a.as_array())) {
            (_, Some(actions)) => collect_msgs(actions),
            (serde_json::Value::Object(_), None) => collect_msgs(std::slice::from_ref(&v)),
            _ => Vec::new(),
        };
        if !msgs.is_empty() {
            return msgs.join("\n");
        }
    }
    raw.trim().to_string()
}

/// Sichtbare Breite eines Strings: ANSI-SGR-Sequenzen (`\x1b[…m`) zählen nicht,
/// der Rest zeichenweise. Grundlage für die Box-Ausrichtung — ohne das wären
/// gefärbte Zeilen scheinbar länger und der rechte Rahmen verrutschte.
fn visible_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC[ … <Buchstabe> überspringen.
            if chars.peek() == Some(&'[') {
                chars.next();
                for e in chars.by_ref() {
                    if e.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        width += 1;
    }
    width
}

/// Rahmt Inhaltszeilen in eine abgerundete Box fester Innenbreite. Jede Zeile
/// wird anhand ihrer SICHTBAREN Breite rechts auf `inner` aufgefüllt.
pub(crate) fn boxed(lines: &[String], inner: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(lines.len() + 2);
    out.push(format!("╭{}╮", "─".repeat(inner + 2)));
    for line in lines {
        let pad = inner.saturating_sub(visible_width(line));
        out.push(format!("│ {}{} │", line, " ".repeat(pad)));
    }
    out.push(format!("╰{}╯", "─".repeat(inner + 2)));
    out
}

/// Zeichenzahl → grobe Token-Schätzung (~4 Zeichen/Token), kompakt formatiert.
pub(crate) fn fmt_est_tokens(chars: usize) -> String {
    let tokens = chars / 4;
    if tokens >= 1000 {
        format!("≈{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("≈{tokens}")
    }
}

/// Sekunden → "1h 02m 03s" / "4m 05s" / "12s".
pub(crate) fn fmt_duration(total_secs: u64) -> String {
    let (h, m, s) = (total_secs / 3600, (total_secs % 3600) / 60, total_secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repl::{parse_slash_command, SlashCommand};

    #[test]
    fn chat_display_unwraps_protocol_json() {
        // Volles webagent/1-Envelope -> nur der message-Text.
        let envelope = r#"{"protocol":"webagent/1","actions":[{"id":"answer-1","type":"message","text":"pong"}]}"#;
        assert_eq!(display_chat_text(envelope), "pong");
        // Envelope ohne "protocol"-Feld (parse lehnt ab) -> Fallback greift.
        let no_proto = r#"{"actions":[{"id":"a","type":"message","text":"hallo"}]}"#;
        assert_eq!(display_chat_text(no_proto), "hallo");
        // Einzelnes message-Objekt.
        let single = r#"{"id":"answer-1","type":"message","text":"solo"}"#;
        assert_eq!(display_chat_text(single), "solo");
        // Mehrere messages -> zusammengefügt; finish wird ignoriert.
        let multi = r#"{"protocol":"webagent/1","actions":[{"id":"1","type":"message","text":"a"},{"id":"2","type":"finish"},{"id":"3","type":"message","text":"b"}]}"#;
        assert_eq!(display_chat_text(multi), "a\nb");
        // Klartext bleibt unangetastet.
        assert_eq!(display_chat_text("  ganz normal  "), "ganz normal");
        // Kaputtes JSON -> Rohtext.
        assert_eq!(display_chat_text("{nicht json"), "{nicht json");
    }

    #[test]
    fn visible_width_ignores_ansi_sequences() {
        assert_eq!(visible_width("abc"), 3);
        // Farbcodes zaehlen nicht zur sichtbaren Breite.
        assert_eq!(visible_width("\u{1b}[1;36mabc\u{1b}[0m"), 3);
        assert_eq!(visible_width("\u{1b}[2m—\u{1b}[0m"), 1);
        // Reiner Reset ohne Text.
        assert_eq!(visible_width("\u{1b}[0m"), 0);
    }

    #[test]
    fn boxed_pads_by_visible_width_so_borders_align() {
        // Zwei Zeilen unterschiedlicher ROH-Laenge, aber gleicher SICHTBARER
        // Breite muessen zur gleichen Rahmenbreite fuehren.
        let lines = vec![
            "\u{1b}[1;36mhallo\u{1b}[0m".to_string(), // sichtbar 5
            "welt!".to_string(),                      // sichtbar 5
        ];
        let out = boxed(&lines, 10);
        assert_eq!(out.len(), 4, "oben + 2 Inhalt + unten");
        let widths: Vec<usize> = out.iter().map(|l| visible_width(l)).collect();
        assert!(
            widths.iter().all(|&w| w == widths[0]),
            "alle Rahmenzeilen gleich breit: {widths:?}"
        );
        assert!(out[0].starts_with('╭') && out[0].ends_with('╮'));
        assert!(out[3].starts_with('╰') && out[3].ends_with('╯'));
    }

    #[test]
    fn dispatch_facts_command() {
        if let Some(SlashCommand::Facts) = parse_slash_command("/facts") {
            let facts = get_facts_string();
            assert!(!facts.is_empty(), "Facts string should not be empty");
            assert!(
                facts.contains("README") || facts.contains("Fortschritt"),
                "Facts should contain 'README' or 'Fortschritt', got: {}",
                facts
            );
        } else {
            panic!("/facts should parse to SlashCommand::Facts");
        }
    }

    #[test]
    fn boxed_never_underflows_on_overlong_lines() {
        // Eine Zeile laenger als die Innenbreite darf nicht panicken (saturating).
        let lines = vec!["viel zu lange zeile fuer die schmale box".to_string()];
        let out = boxed(&lines, 5);
        assert_eq!(out.len(), 3);
    }
}
