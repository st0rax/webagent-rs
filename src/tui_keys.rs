//! Tastennamen -> Tastenereignis.
//!
//! Die TUI hat genau EINEN Bedienpfad: den `match` ueber `KeyEvent` in
//! [`crate::tui`]. Maussteuerung und Steuerkanal erzeugen deshalb keine eigenen
//! Befehle, sondern uebersetzen ihre Geste in genau das Tastenereignis, das ein
//! Mensch getippt haette. Ohne diese Regel entstuenden zwei Bedienpfade, die
//! auseinanderlaufen — das Muster, das uns hier schon mehrfach Zeit gekostet hat
//! (Antworttext, Selektoren, Profilverzeichnisse).
//!
//! Namen sind kleinschreibungs-unabhaengig; ein einzelnes Zeichen ist immer es
//! selbst (`"j"` -> `j`, `"+"` -> `+`), damit die Fusszeilen-Eintraege direkt als
//! Handlung taugen.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Uebersetzt einen Tastennamen. `None` = unbekannt; der Aufrufer meldet das,
/// statt still nichts zu tun.
pub fn parse_key(name: &str) -> Option<KeyEvent> {
    let raw = name.trim();
    if raw.is_empty() {
        return None;
    }
    // Modifikator abtrennen: "alt+3", "ALT+3".
    let (modifiers, rest) = match raw.split_once('+') {
        // `"+"` selbst ist eine Taste, kein Modifikator-Trenner — und
        // `"+/-"`-artige Reste ebenso. Nur ein bekannter Modifikator zaehlt.
        Some((m, r)) if m.eq_ignore_ascii_case("alt") && !r.is_empty() => (KeyModifiers::ALT, r),
        Some((m, r)) if m.eq_ignore_ascii_case("ctrl") && !r.is_empty() => {
            (KeyModifiers::CONTROL, r)
        }
        _ => (KeyModifiers::NONE, raw),
    };

    let code = match rest.to_ascii_lowercase().as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "enter" | "return" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "space" | "leertaste" => KeyCode::Char(' '),
        "backspace" => KeyCode::Backspace,
        "up" | "hoch" => KeyCode::Up,
        "down" | "runter" => KeyCode::Down,
        "left" | "links" => KeyCode::Left,
        "right" | "rechts" => KeyCode::Right,
        _ => {
            let mut chars = rest.chars();
            match (chars.next(), chars.next()) {
                // Genau ein Zeichen. Gross-/Kleinschreibung bleibt erhalten,
                // sonst waere `+` nicht von `-` zu unterscheiden.
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
    };

    Some(KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: crossterm::event::KeyEventState::NONE,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(name: &str) -> KeyCode {
        parse_key(name).expect("bekannt").code
    }

    #[test]
    fn einzelzeichen_bleiben_sie_selbst() {
        assert_eq!(code("j"), KeyCode::Char('j'));
        assert_eq!(code("w"), KeyCode::Char('w'));
        assert_eq!(code("/"), KeyCode::Char('/'));
    }

    #[test]
    fn plus_und_minus_sind_tasten_kein_modifikator() {
        // `"+"` darf nicht als Trenner missverstanden werden, sonst waere der
        // Worker-Knopf nicht bedienbar.
        assert_eq!(code("+"), KeyCode::Char('+'));
        assert_eq!(code("-"), KeyCode::Char('-'));
    }

    #[test]
    fn benannte_tasten_unabhaengig_von_schreibweise() {
        assert_eq!(code("Esc"), KeyCode::Esc);
        assert_eq!(code("ENTER"), KeyCode::Enter);
        assert_eq!(code("space"), KeyCode::Char(' '));
        assert_eq!(code("Down"), KeyCode::Down);
        assert_eq!(code("tab"), KeyCode::Tab);
    }

    #[test]
    fn alt_modifikator() {
        let k = parse_key("alt+3").expect("bekannt");
        assert_eq!(k.code, KeyCode::Char('3'));
        assert!(k.modifiers.contains(KeyModifiers::ALT));
    }

    #[test]
    fn unbekanntes_meldet_none_statt_still_nichts_zu_tun() {
        assert!(parse_key("").is_none());
        assert!(parse_key("   ").is_none());
        assert!(parse_key("j/k").is_none());
        assert!(parse_key("gibtsnicht").is_none());
    }

    #[test]
    fn kind_ist_press_denn_die_schleife_verwirft_alles_andere() {
        // Die Ereignisschleife filtert auf KeyEventKind::Press. Ein hier
        // erzeugtes Release-Ereignis wuerde spurlos verschluckt.
        assert_eq!(parse_key("w").expect("bekannt").kind, KeyEventKind::Press);
    }
}
