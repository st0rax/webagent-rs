//! Abstraktion über Seitensteuerung — früher CDP (`Runtime.evaluate`, `Input.*`),
//! jetzt Embedded WebView (wry/tao) oder Mock für Unit-Tests.

use std::time::Duration;

use serde_json::Value;

/// Fehler eines Page-Drivers (CDP-freie Variante).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageDriverError {
    Launch(String),
    Protocol(String),
    Timeout(String),
    NotAvailable(String),
}

impl std::fmt::Display for PageDriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageDriverError::Launch(m) => write!(f, "page-launch: {m}"),
            PageDriverError::Protocol(m) => write!(f, "page-protocol: {m}"),
            PageDriverError::Timeout(m) => write!(f, "page-timeout: {m}"),
            PageDriverError::NotAvailable(m) => write!(f, "page-unavailable: {m}"),
        }
    }
}
impl std::error::Error for PageDriverError {}

pub type Result<T> = std::result::Result<T, PageDriverError>;

/// Gemeinsame API — 1:1 zum früheren `CdpClient` (synchron/blockierend).
pub trait PageDriver: Send {
    /// Wertet JS in der Seite aus (`awaitPromise`, Rückgabewert als JSON-Value).
    fn evaluate(&mut self, expression: &str) -> Result<Value>;

    /// Convenience: JS auswerten und als String zurückgeben ("" bei null).
    fn eval_string(&mut self, expression: &str) -> Result<String> {
        Ok(self
            .evaluate(expression)?
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    /// Navigiert zu einer URL und wartet (best effort) auf `document.readyState`.
    fn navigate(&mut self, url: &str, timeout: Duration) -> Result<()>;

    /// Aktuelle URL der Seite.
    fn current_url(&mut self) -> Result<String>;

    /// Tastendruck ans fokussierte Element (`text` z.B. `"\r"` für Enter).
    fn press_key(&mut self, key: &str, code: &str, virtual_key: i64, text: &str) -> Result<()>;

    /// Text als echtes Tippen ins fokussierte Element.
    fn insert_text(&mut self, text: &str) -> Result<()>;

    /// Linksklick an Viewport-Koordinaten.
    fn click_at(&mut self, x: f64, y: f64) -> Result<()>;

    /// Nimmt den sichtbaren Seiteninhalt als PNG auf.
    ///
    /// Warum das hier gebraucht wird: die Fähigkeits-Vermessung über das DOM
    /// findet nur, was einen Namen trägt. Real gemessen am 2026-07-27 rendert
    /// deepseek 107 Bedienelemente als anonyme Icon-Divs — kein aria-label,
    /// kein title, kein Text. Auf einem Bild sind dieselben Knöpfe sofort
    /// erkennbar. Ein Treiber ohne Bildunterstützung meldet das ehrlich,
    /// statt ein leeres Bild zu liefern.
    fn capture_png(&mut self) -> Result<Vec<u8>> {
        Err(PageDriverError::NotAvailable(
            "Dieser PageDriver kann keine Screenshots aufnehmen".into(),
        ))
    }
}

/// Hilfsfunktion wenn das `webview`-Feature nicht kompiliert ist.
#[cfg(not(feature = "webview"))]
pub fn webview_unavailable() -> PageDriverError {
    PageDriverError::NotAvailable(
        "WebView-Feature nicht aktiviert — mit --features webview bauen".into(),
    )
}

/// Attempts to select an element using a primary selector, falling back to secondary selectors.
///
/// The function first tries the primary selector. If it fails, it tries each secondary selector
/// in order and returns the first successful match. If no exact match is found, it applies a
/// simple drift heuristic: if any selector appears as a substring in the DOM (e.g., class name
/// without prefix), it returns that selector as a drift hit. Returns `None` if no selector matches.
pub fn select_with_fallback(
    primary: &str,
    secondary: &[&str],
    dom_snapshot: &str,
) -> Option<String> {
    // Helper: check if a selector (exact match) appears in the DOM
    fn selector_matches_exact(selector: &str, html: &str) -> bool {
        html.contains(selector)
    }

    // Helper: check if a selector appears as a substring (drift heuristic)
    fn selector_matches_substring(selector: &str, html: &str) -> bool {
        // Remove leading '.' or '#' for substring matching (class/ID without prefix)
        let cleaned = if selector.starts_with('.') || selector.starts_with('#') {
            &selector[1..]
        } else {
            selector
        };
        html.contains(cleaned)
    }

    // Try primary selector (exact match)
    if selector_matches_exact(primary, dom_snapshot) {
        return Some(primary.to_string());
    }

    // Try secondary selectors (exact match) in order
    for &sel in secondary {
        if selector_matches_exact(sel, dom_snapshot) {
            return Some(sel.to_string());
        }
    }

    // Drift heuristic: if no exact match, try substring matches
    // Check primary first (if primary fails exact but has substring match)
    if selector_matches_substring(primary, dom_snapshot) {
        return Some(primary.to_string());
    }

    // Then secondary in order
    for &sel in secondary {
        if selector_matches_substring(sel, dom_snapshot) {
            return Some(sel.to_string());
        }
    }

    None
}

#[cfg(test)]
mod select_with_fallback_tests {
    use super::*;

    #[test]
    fn primary_exact_match_returns_primary() {
        let html = r#"<div id="main"><button class="submit">Click</button></div>"#;
        let result = select_with_fallback(".submit", &[".other"], html);
        assert_eq!(result, Some(".submit".to_string()));
    }

    #[test]
    fn primary_missing_first_secondary_match_returns_secondary() {
        let html = r#"<div id="main"><span class="fallback">Hello</span></div>"#;
        let result = select_with_fallback(".missing", &[".fallback", ".other"], html);
        assert_eq!(result, Some(".fallback".to_string()));
    }

    #[test]
    fn no_exact_match_substring_drift_returns_secondary() {
        let html = r#"<div id="main"><button class="btn-primary">Click</button></div>"#;
        let result = select_with_fallback(".missing", &[".btn-primary", ".other"], html);
        assert_eq!(result, Some(".btn-primary".to_string()));
    }

    #[test]
    fn no_selectors_match_returns_none() {
        let html = r#"<div id="main">Hello</div>"#;
        let result = select_with_fallback(".missing", &[".other1", ".other2"], html);
        assert_eq!(result, None);
    }

    #[test]
    fn multiple_secondary_matches_returns_first_in_order() {
        let html = r#"<div class="first"></div><div class="second"></div>"#;
        let result = select_with_fallback(".missing", &[".first", ".second"], html);
        assert_eq!(result, Some(".first".to_string()));
    }
}
