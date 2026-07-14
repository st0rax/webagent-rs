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
}

/// Hilfsfunktion wenn das `webview`-Feature nicht kompiliert ist.
#[cfg(not(feature = "webview"))]
pub fn webview_unavailable() -> PageDriverError {
    PageDriverError::NotAvailable(
        "WebView-Feature nicht aktiviert — mit --features webview bauen".into(),
    )
}
