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
    // Ein sichtbarer Anmelden-Knopf schlaegt JEDEN positiven Indikator.
    // (Geschichte: geminis login_indicator war der Composer, den auch die
    // ausgeloggte Startseite zeigt - deshalb darf der Composer NIE als
    // Login-Beweis zaehlen, und ein sichtbarer login_button widerlegt alles.)
    if any_visible(driver, sel, "login_button") {
        return false;
    }
    let indicator = sel.list("login_indicator");
    if !indicator.is_empty() {
        return any_visible(driver, sel, "login_indicator");
    }
    // Ohne konfigurierten Indikator: Composer/New-Chat als grobe Naeherung.
    any_visible(driver, sel, "composer") || any_visible(driver, sel, "new_chat_button")
}

/// Anzahl der Assistenten-Nachrichten (robust über die Selektorliste).
pub fn assistant_count(driver: &mut dyn PageDriver, sel: &Selectors) -> i32 {
    let list = sel.js("assistant_message", &["div.prose"]);
    let expr = js::js_scan(&list, "var n=QA(S[i]).length;if(n>0)return n;", "0");
    driver
        .evaluate(&expr)
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32
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
    let url = driver.current_url().ok()?;
    let url = url.trim();
    if url.is_empty() || url == "about:blank" {
        None
    } else {
        Some(url.to_string())
    }
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
mod is_logged_in_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn expr_for(selectors: &[&str]) -> String {
        let sels: Vec<String> = selectors.iter().map(|s| s.to_string()).collect();
        js::js_scan(&js::js_selectors(&sels), VISIBLE_BODY, "false")
    }

    fn sel_with(key: &str, selectors: &[&str]) -> Selectors {
        Selectors::from_value(json!({ key: selectors }))
    }

    #[test]
    fn true_wenn_login_indicator_sichtbar() {
        let sel = sel_with("login_indicator", &["div.avatar"]);
        let indicator_expr = expr_for(&["div.avatar"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&indicator_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn false_wenn_login_button_sichtbar_trotz_indikator() {
        let sel = Selectors::from_value(json!({
            "login_button": ["button.login"],
            "login_indicator": ["div.avatar"],
        }));
        let button_expr = expr_for(&["button.login"]);
        let indicator_expr = expr_for(&["div.avatar"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(&button_expr, json!(true))
                .on_eval(&indicator_expr, json!(true)),
        );
        assert!(!is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_auf_composer_ohne_indikator() {
        let sel = sel_with("composer", &["textarea.prompt"]);
        let composer_expr = expr_for(&["textarea.prompt"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&composer_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_auf_new_chat_button() {
        let sel = sel_with("new_chat_button", &["button.new-chat"]);
        let new_chat_expr = expr_for(&["button.new-chat"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&new_chat_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_false_wenn_nichts_sichtbar() {
        let sel = Selectors::from_value(json!({
            "composer": ["textarea.prompt"],
            "new_chat_button": ["button.new-chat"],
        }));
        let composer_expr = expr_for(&["textarea.prompt"]);
        let new_chat_expr = expr_for(&["button.new-chat"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(&composer_expr, json!(false))
                .on_eval(&new_chat_expr, json!(false)),
        );
        assert!(!is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn false_bei_unbekannten_oder_leeren_selektoren() {
        let sel = Selectors::from_value(json!({
            "composer": [],
            "new_chat_button": [],
        }));
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!is_logged_in(&mut driver, &sel));
    }
}

#[cfg(test)]
mod assistant_count_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn count_expr_for(sel: &Selectors) -> String {
        let list = sel.js("assistant_message", &["div.prose"]);
        js::js_scan(&list, "var n=QA(S[i]).length;if(n>0)return n;", "0")
    }

    #[test]
    fn returns_count_when_js_returns_number() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(5)));
        assert_eq!(assistant_count(&mut driver, &sel), 5);
    }

    #[test]
    fn returns_zero_when_js_returns_zero() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(0)));
        assert_eq!(assistant_count(&mut driver, &sel), 0);
    }

    #[test]
    fn returns_zero_when_eval_fails() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert_eq!(assistant_count(&mut driver, &sel), 0);
    }

    #[test]
    fn uses_fallback_list_when_key_absent() {
        let sel = Selectors::from_value(json!({}));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(3)));
        assert_eq!(assistant_count(&mut driver, &sel), 3);
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

#[cfg(test)]
mod get_conversation_ref_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};

    #[test]
    fn normale_url_ergibt_some() {
        let mut driver =
            MockPageDriver::new(MockPageState::new().with_url("https://chatgpt.com/c/abc"));
        assert_eq!(
            get_conversation_ref(&mut driver),
            Some("https://chatgpt.com/c/abc".to_string())
        );
    }

    #[test]
    fn about_blank_ergibt_none() {
        let mut driver = MockPageDriver::new(MockPageState::new().with_url("about:blank"));
        assert_eq!(get_conversation_ref(&mut driver), None);
    }

    #[test]
    fn leere_url_ergibt_none() {
        let mut driver = MockPageDriver::new(MockPageState::new().with_url(""));
        assert_eq!(get_conversation_ref(&mut driver), None);
    }

    #[test]
    fn whitespace_um_url_wird_getrimmt() {
        let mut driver =
            MockPageDriver::new(MockPageState::new().with_url("  https://chatgpt.com/c/def\t\n"));
        assert_eq!(
            get_conversation_ref(&mut driver),
            Some("https://chatgpt.com/c/def".to_string())
        );
    }
}
