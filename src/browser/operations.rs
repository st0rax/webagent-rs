//! Freie Funktionen über [`PageDriver`] + [`Selectors`] — der Arbeitsteil der
//! Diagnose, ohne Backend-`&self`.
//!
//! Schritt 2 des Entkoppelns: `is_logged_in`, `any_visible`, `assistant_count`,
//! `is_cloudflare_blocked`, `get_conversation_ref` waren Methoden am
//! `WebBrainBackend`. Als freie Funktionen sind sie mit dem `MockPageDriver`
//! testbar — die Testlücke am Fundament schließt sich, statt nur eingestanden
//! zu werden.
//!
//! Jede Funktion bekommt das, was sie braucht, explizit: den Driver und die
//! Selektoren. Der Browser-Lebenszyklus (`start`/`stop`) bleibt beim Backend.

use crate::browser::js;
use crate::browser::selectors::Selectors;
use crate::page_driver::PageDriver;

/// Rumpf der Sichtbarkeitsprüfung: das erste Element, das eine reale Fläche
/// hat, gewinnt.
const VISIBLE_BODY: &str = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}";

/// Ist mindestens ein Selektor aus der Liste im DOM sichtbar?
pub fn any_visible(driver: &mut dyn PageDriver, sel: &Selectors, key: &str) -> bool {
    let sels = sel.list(key);
    if sels.is_empty() {
        return false;
    }
    let expr = js::js_scan(&js::js_selectors(&sels), VISIBLE_BODY, "false");
    driver
        .evaluate(&expr)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Ist der Brain angemeldet?
///
/// Ein sichtbarer Anmelden-Knopf schlaegt JEDEN positiven Indikator (siehe
/// Geschichte am 2026-07-28: geminis `login_indicator` war der Composer, den
/// auch die ausgeloggte Startseite zeigt). Ohne konfigurierten Indikator
/// Composer/New-Chat als grobe Naeherung.
pub fn is_logged_in(driver: &mut dyn PageDriver, sel: &Selectors) -> bool {
    todo!("SCHRITT2:is_logged_in")
}

/// Anzahl der Assistenten-Nachrichten (robust über die Selektorliste).
pub fn assistant_count(driver: &mut dyn PageDriver, sel: &Selectors) -> i32 {
    todo!("SCHRITT2:assistant_count")
}

/// Cloudflare-Challenge-Erkennung: `__cf_chl` in der URL, oder ein
/// „Just a moment"-Titel (DE+EN). Geteilt zwischen Implementierung und Test —
/// der Mock-Driver matcht auf die EXAKTE Zeichenkette.
const CF_CHALLENGE_EXPR: &str = r#"(function(){var u=location.href||"";if(u.indexOf("__cf_chl")>=0)return true;var t=(document.title||"").toLowerCase();return t.indexOf("just a moment")>=0||t.indexOf("nur einen moment")>=0;})()"#;

/// Ist die Seite von Cloudflare blockiert (`__cf_chl`-URL oder Challenge-Titel)?
pub fn is_cloudflare_blocked(driver: &mut dyn PageDriver) -> bool {
    driver
        .evaluate(CF_CHALLENGE_EXPR)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Aktuelle Konversations-URL als Referenz; `None` für leere/`about:blank`.
pub fn get_conversation_ref(driver: &mut dyn PageDriver) -> Option<String> {
    todo!("SCHRITT2:get_conversation_ref")
}

#[cfg(test)]
mod any_visible_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn sel_with(key: &str, selectors: &[&str]) -> Selectors {
        Selectors::from_value(json!({ key: selectors }))
    }

    #[test]
    fn true_wenn_element_sichtbar() {
        let sel = sel_with("key", &["div.x"]);
        let expr = js::js_scan(&js::js_selectors(&["div.x".to_string()]), VISIBLE_BODY, "false");
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(true)));
        assert!(any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_wenn_nicht_sichtbar() {
        let sel = sel_with("key", &["div.x"]);
        let expr = js::js_scan(&js::js_selectors(&["div.x".to_string()]), VISIBLE_BODY, "false");
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(false)));
        assert!(!any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_bei_leerem_schluessel() {
        let sel = sel_with("key", &[]);
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_wenn_eval_scheitert() {
        let sel = sel_with("key", &["div.x"]);
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!any_visible(&mut driver, &sel, "key"));
    }
}

#[cfg(test)]
mod is_cloudflare_blocked_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    #[test]
    fn true_wenn_js_true_liefert() {
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(CF_CHALLENGE_EXPR, json!(true)),
        );
        assert!(is_cloudflare_blocked(&mut driver));
    }

    #[test]
    fn false_wenn_js_false_liefert() {
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(CF_CHALLENGE_EXPR, json!(false)),
        );
        assert!(!is_cloudflare_blocked(&mut driver));
    }

    #[test]
    fn false_wenn_eval_scheitert() {
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!is_cloudflare_blocked(&mut driver));
    }
}
