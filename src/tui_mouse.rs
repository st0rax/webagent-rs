//! Wohin zeigt ein Mausklick?
//!
//! Die TUI war bis 07.08.2026 rein tastaturbedient. Das ist nicht nur unbequem:
//! es macht sie fuer alles unbedienbar, was keine Tastatur hat — Skripte,
//! Watchdogs, und einen Assistenten, der Terminalfenster nur anklicken darf.
//! Ein Bedienweg, der an genau einem Menschen haengt, ist ein Einzelpunkt-
//! Ausfall der Oberflaeche.
//!
//! Die Loesung baut KEINEN zweiten Bedienpfad: ein Klick wird zu genau der
//! Taste, die ein Mensch gedrueckt haette ([`Hit::Key`]), oder zu einer
//! Zeilenauswahl ([`Hit::Row`]). Beides landet danach in demselben `match`.
//!
//! Die Geometrie hier rechnet das Layout aus [`crate::tui_render::ui`] nach:
//! Kopf 4 Zeilen, Fusszeile 1 Zeile, Koerper dazwischen. Panels sind umrahmt,
//! ihr Inhalt beginnt deshalb eine Zeile/Spalte weiter innen. Die Alternative
//! waere, die gerenderten Rechtecke aus dem Frame herauszureichen — dann haette
//! die Trefferpruefung aber keinen Test ohne Terminal.

use crate::tui_state::View;

/// Kopfzeile inkl. Rahmen (`Constraint::Length(4)` in `ui`).
pub const HEADER_ROWS: u16 = 4;
/// Rahmenbreite der umrahmten Panels.
const BORDER: u16 = 1;
/// Breite der linken Spalte im Worker-Dashboard (`Percentage(34)`).
const LEFT_PERCENT: u16 = 34;

/// Was unter dem Zeiger liegt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// Ein Knopf — ausgeloest wird die zugehoerige Taste.
    Key(&'static str),
    /// Zeile `index` (0-basiert, relativ zum sichtbaren Ausschnitt) einer Liste.
    Row(usize),
}

/// Bildschirmgeometrie und Zustand, soweit die Trefferpruefung sie braucht.
#[derive(Debug, Clone, Copy)]
pub struct Screen {
    pub width: u16,
    pub height: u16,
    pub view: View,
    /// Worker-Ansicht ohne Agenten zeigt den Leerzustand statt der Listen.
    pub has_agents: bool,
}

/// Trefferpruefung fuer einen Klick auf `(col, row)`, beide 0-basiert.
///
/// `None` heisst ausdruecklich „hier passiert nichts" — der Aufrufer soll dann
/// auch nichts tun, statt auf eine Vorgabe zurueckzufallen. Ein Klick ins Leere
/// darf keine Ansicht umschalten.
pub fn hit(screen: Screen, col: u16, row: u16) -> Option<Hit> {
    if screen.height == 0 || screen.width == 0 {
        return None;
    }
    let last = screen.height - 1;

    // Fusszeile: die Knopfleiste.
    if row == last {
        return crate::tui_render::footer_zones(screen.view)
            .into_iter()
            .find(|(from, to, _)| col >= *from && col < *to)
            .map(|(_, _, action)| Hit::Key(action));
    }

    // Kopfzeile: ein Klick wechselt die Ansicht — dasselbe wie `v`. Der Kopf
    // traegt bereits das Ansichts-Symbol, ein Klick darauf ist die naechste
    // erwartbare Geste.
    if row < HEADER_ROWS {
        return Some(Hit::Key("v"));
    }

    // Koerper. Leerzustand hat keine Zeilen, auf die man zeigen koennte.
    if screen.view == View::Workers && !screen.has_agents {
        return None;
    }
    // Im Worker-Dashboard ist nur die linke Agentenliste zeilenweise waehlbar.
    if screen.view == View::Workers && col >= screen.width * LEFT_PERCENT / 100 {
        return None;
    }
    // Innerhalb des Rahmens?
    if col < BORDER || col >= screen.width.saturating_sub(BORDER) {
        return None;
    }
    let first_content_row = HEADER_ROWS + BORDER;
    if row < first_content_row || row >= last {
        return None;
    }
    Some(Hit::Row((row - first_content_row) as usize))
}

/// Zeilen, die der Koerper eines umrahmten Panels darstellt.
///
/// Die Maus braucht das, um eine geklickte Zeile in einen Listenindex zu
/// uebersetzen — mit derselben Zahl, die das Rendern verwendet.
pub fn body_rows(height: u16) -> usize {
    height
        .saturating_sub(HEADER_ROWS)
        .saturating_sub(1) // Fusszeile
        .saturating_sub(2 * BORDER) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(view: View) -> Screen {
        Screen {
            width: 100,
            height: 30,
            view,
            has_agents: true,
        }
    }

    #[test]
    fn fusszeile_trifft_den_kachel_knopf() {
        let s = screen(View::Bench);
        // Reihenfolge in der Bench-Ansicht: v, j/k, g, w, q.
        let zones = crate::tui_render::footer_zones(View::Bench);
        let (from, to, action) = zones[3];
        assert_eq!(action, "w");
        assert_eq!(hit(s, from, 29), Some(Hit::Key("w")));
        assert_eq!(hit(s, to - 1, 29), Some(Hit::Key("w")));
        // Der Nachbar ist ein anderer Knopf, nicht derselbe.
        assert_ne!(hit(s, to, 29), Some(Hit::Key("w")));
    }

    #[test]
    fn beschriftung_ist_teil_der_trefferflaeche() {
        // Wer auf das Wort zeigt, trifft — nicht nur wer den Buchstaben trifft.
        let s = screen(View::Bench);
        let (from, to, _) = crate::tui_render::footer_zones(View::Bench)[0];
        for col in from..to {
            assert_eq!(hit(s, col, 29), Some(Hit::Key("v")), "Spalte {col}");
        }
    }

    #[test]
    fn klick_hinter_die_knoepfe_tut_nichts() {
        let s = screen(View::Bench);
        assert_eq!(hit(s, 95, 29), None);
    }

    #[test]
    fn kopfzeile_wechselt_die_ansicht() {
        assert_eq!(hit(screen(View::Bench), 10, 0), Some(Hit::Key("v")));
        assert_eq!(hit(screen(View::Bench), 10, 3), Some(Hit::Key("v")));
    }

    #[test]
    fn erste_inhaltszeile_ist_index_null() {
        // Kopf 0..3, Rahmen 4, erste Inhaltszeile 5.
        assert_eq!(hit(screen(View::Bench), 10, 5), Some(Hit::Row(0)));
        assert_eq!(hit(screen(View::Bench), 10, 6), Some(Hit::Row(1)));
    }

    #[test]
    fn rahmen_und_fusszeile_sind_keine_zeilen() {
        let s = screen(View::Bench);
        assert_eq!(hit(s, 10, 4), None, "obere Rahmenzeile");
        assert_eq!(hit(s, 0, 10), None, "linker Rahmen");
        assert_eq!(hit(s, 99, 10), None, "rechter Rahmen");
    }

    #[test]
    fn worker_dashboard_nur_linke_spalte() {
        let s = screen(View::Workers);
        assert_eq!(hit(s, 10, 6), Some(Hit::Row(1)));
        assert_eq!(hit(s, 60, 6), None, "rechte Panels sind nicht zeilenweise");
    }

    #[test]
    fn leerzustand_hat_keine_zeilen() {
        let s = Screen {
            has_agents: false,
            ..screen(View::Workers)
        };
        assert_eq!(hit(s, 10, 6), None);
        // Die Knopfleiste bleibt aber bedienbar.
        assert_eq!(hit(s, 1, 29), Some(Hit::Key("v")));
    }

    #[test]
    fn winziges_fenster_stuerzt_nicht_ab() {
        let s = Screen {
            width: 1,
            height: 1,
            view: View::Bench,
            has_agents: false,
        };
        assert!(hit(s, 0, 0).is_none() || matches!(hit(s, 0, 0), Some(Hit::Key(_))));
        assert_eq!(body_rows(1), 0);
        assert_eq!(body_rows(0), 0);
    }

    #[test]
    fn jeder_knopf_loest_wirklich_eine_taste_aus() {
        // Ohne diesen Test kann ein Eintrag mit unbekannter Aktion in die
        // Leiste geraten: er waere sichtbar, anklickbar — und wirkungslos.
        for view in [View::Bench, View::Workers, View::Capabilities] {
            for b in crate::tui_render::footer_binds(view) {
                assert!(
                    crate::tui_keys::parse_key(b.action).is_some(),
                    "{:?}: Knopf {:?} hat keine ausloesbare Aktion ({:?})",
                    view,
                    b.label,
                    b.action
                );
            }
        }
    }

    #[test]
    fn knopf_zonen_ueberlappen_nicht() {
        for view in [View::Bench, View::Workers, View::Capabilities] {
            let zones = crate::tui_render::footer_zones(view);
            for pair in zones.windows(2) {
                assert!(
                    pair[0].1 <= pair[1].0,
                    "{view:?}: Zonen ueberlappen — ein Klick traefe zwei Knoepfe"
                );
            }
        }
    }

    #[test]
    fn body_rows_passt_zum_layout() {
        // 30 Zeilen: 4 Kopf + 1 Fuss + 2 Rahmen = 23 Inhaltszeilen.
        assert_eq!(body_rows(30), 23);
    }
}
