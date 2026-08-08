//! Die konfigurierten DOM-Selektoren eines Brains als eigener Wert.
//!
//! Früher steckten sie als nacktes `serde_json::Value` im Backend und die
//! Zugriffe `sel` / `sel_js` waren Backend-Methoden. Beides wohnt jetzt hier:
//! die Operationen aus Schritt 2 brauchen die Selektoren als Parameter
//! (`fn f(driver: &mut dyn PageDriver, sel: &Selectors, …)`) und dieser Typ ist
//! ohne Browser testbar.
//!
//! `browser/js.rs` bleibt der JS-Ausdrucksbauer (`js_selectors`, `js_scan`,
//! `JS_SEL_PRELUDE`) — der `Selectors`-Typ reicht nur durch, er baut nichts.

use serde_json::Value;

use crate::browser::js;

/// DOM-Selektoren eines Brains (geladen aus `selectors/<brain>.json`).
///
/// Der Aufbau entspricht dem der JSON-Dateien: ein Objekt von Schlüssel →
/// Selektorliste. Unbekannte Schlüssel liefern eine leere Liste.
#[derive(Debug, Clone)]
pub struct Selectors {
    inner: Value,
}

impl Selectors {
    /// Leerer Selektor-Satz (z.B. für ein noch nicht registriertes Brain).
    pub fn empty() -> Self {
        Self {
            inner: serde_json::json!({}),
        }
    }

    /// Aus dem rohen JSON-Wert der Selektoren-Datei.
    pub fn from_value(inner: Value) -> Self {
        Self { inner }
    }

    /// Selektor-Liste zu einem Schlüssel (leere Liste, wenn nicht vorhanden).
    pub fn list(&self, key: &str) -> Vec<String> {
        self.inner
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(|x| x.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// JS-Array-Literal der Selektoren zu einem Schlüssel; `fallback` greift,
    /// wenn keine konfiguriert sind.
    pub fn js(&self, key: &str, fallback: &[&str]) -> String {
        let mut sels = self.list(key);
        if sels.is_empty() {
            sels = fallback.iter().map(|s| s.to_string()).collect();
        }
        js::js_selectors(&sels)
    }
}

impl Default for Selectors {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Selectors {
        Selectors::from_value(serde_json::json!({
            "composer": ["textarea[name=\"prompt\"]", "div.prose"],
            "login_button": [],
        }))
    }

    #[test]
    fn list_liefert_alle_konfigurierten_selektoren() {
        let s = sample();
        assert_eq!(
            s.list("composer"),
            vec!["textarea[name=\"prompt\"]".to_string(), "div.prose".to_string()]
        );
    }

    #[test]
    fn list_leer_fuer_unbekannten_schluessel() {
        let s = sample();
        assert!(s.list("does_not_exist").is_empty());
    }

    #[test]
    fn js_baut_array_literal_aus_der_liste() {
        let s = sample();
        let js = s.js("composer", &[]);
        assert!(js.starts_with('[') && js.ends_with(']'));
        assert!(js.contains("\"div.prose\""));
    }

    #[test]
    fn js_benutzt_fallback_bei_leerem_schluessel() {
        let s = sample();
        let js = s.js("does_not_exist", &["div.fallback"]);
        assert!(js.contains("\"div.fallback\""), "js={js}");
    }

    #[test]
    fn js_faellt_auf_fallback_wenn_key_leer_ist() {
        let s = sample();
        let js = s.js("login_button", &["button.login"]);
        assert!(js.contains("\"button.login\""), "js={js}");
    }

    #[test]
    fn js_escaped_quotes() {
        let s = Selectors::from_value(serde_json::json!({
            "key": ["a.b", "c\"d"]
        }));
        let js = s.js("key", &[]);
        assert!(js.contains("\"a.b\""));
        assert!(js.contains("\\\"c\\\"d\\\"") || js.contains("\"c\\\"d\""));
    }

    #[test]
    fn empty_liefert_immer_leere_liste() {
        let s = Selectors::empty();
        assert!(s.list("composer").is_empty());
    }
}
