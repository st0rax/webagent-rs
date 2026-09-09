//! Scriptbarer Page-Driver für Unit-Tests (kein echter Browser).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::page_driver::{PageDriver, PageDriverError, Result};

type ScriptMap = HashMap<String, Value>;

/// Geteilte, scriptbare Zustandsbasis für einen oder mehrere Mock-Tabs.
#[derive(Debug, Clone, Default)]
pub struct MockPageState {
    inner: Arc<Mutex<MockStateInner>>,
}

#[derive(Debug, Default)]
struct MockStateInner {
    url: String,
    scripts: ScriptMap,
    navigate_delay: Duration,
    /// PNG-Ausgabe von `capture_png`; `None` = kein Bild (Trailing-Verhalten).
    png: Option<Vec<u8>>,
    /// Antwortfolgen: jeder Aufruf nimmt den naechsten Wert, der letzte bleibt
    /// stehen. Ohne das kann ein Test keinen Zustandswechsel nachstellen —
    /// „vorher" und „nachher" waeren zwangslaeufig derselbe Wert, und jede
    /// Vorher/Nachher-Pruefung sähe im Test aus wie ein Fehlschlag.
    sequences: HashMap<String, Vec<Value>>,
    /// Count + coordinates of `move_pointer` calls (wake_renderer tests).
    pointer_moves: Vec<(f64, f64)>,
    /// Fallback when no exact `on_eval` / sequence matches (attach probes).
    default_eval: Option<Value>,
    /// Recorded `set_file_input_files` payloads (name list per call).
    file_uploads: Vec<Vec<String>>,
    /// Whether `set_file_input_files` succeeds (`true`) or returns NotAvailable.
    file_upload_ok: bool,
}

impl MockPageState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_url(self, url: impl Into<String>) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.url = url.into();
        }
        self
    }

    pub fn with_png(self, bytes: Vec<u8>) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.png = Some(bytes);
        }
        self
    }

    /// Registriert eine feste Antwort für ein exaktes JS-Expression.
    pub fn on_eval(self, expression: impl Into<String>, value: Value) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.scripts.insert(expression.into(), value);
        }
        self
    }

    /// Registriert eine Antwortfolge für ein exaktes JS-Expression: der erste
    /// Aufruf liefert `values[0]`, der zweite `values[1]` usw.; ist die Folge
    /// aufgebraucht, bleibt der letzte Wert stehen.
    pub fn on_eval_seq(self, expression: impl Into<String>, values: Vec<Value>) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.sequences.insert(expression.into(), values);
        }
        self
    }

    pub fn navigate_delay(self, delay: Duration) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.navigate_delay = delay;
        }
        self
    }

    /// Unmatched `evaluate` expressions return this value (attach/upload tests).
    pub fn with_default_eval(self, value: Value) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.default_eval = Some(value);
        }
        self
    }

    /// Configure whether `set_file_input_files` succeeds for this mock.
    pub fn with_file_upload_ok(self, ok: bool) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.file_upload_ok = ok;
        }
        self
    }

    /// How often `set_file_input_files` was invoked (trusted CDP upload path).
    pub fn set_file_input_files_calls(&self) -> usize {
        self.inner.lock().map(|g| g.file_uploads.len()).unwrap_or(0)
    }

    /// File names from each `set_file_input_files` call.
    pub fn set_file_input_files_names(&self) -> Vec<Vec<String>> {
        self.inner
            .lock()
            .map(|g| g.file_uploads.clone())
            .unwrap_or_default()
    }

    /// How many times `move_pointer` was called on drivers sharing this state.
    pub fn move_pointer_calls(&self) -> usize {
        self.inner
            .lock()
            .map(|g| g.pointer_moves.len())
            .unwrap_or(0)
    }

    /// Coordinates recorded by `move_pointer` (wake_renderer alternates 1/1 and 2/2).
    pub fn move_pointer_coords(&self) -> Vec<(f64, f64)> {
        self.inner
            .lock()
            .map(|g| g.pointer_moves.clone())
            .unwrap_or_default()
    }
}

/// Mock-Implementierung von [`PageDriver`] — Antworten per `MockPageState::on_eval`.
pub struct MockPageDriver {
    state: MockPageState,
}

impl MockPageDriver {
    pub fn new(state: MockPageState) -> Self {
        Self { state }
    }
}

impl PageDriver for MockPageDriver {
    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let mut guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        if let Some(seq) = guard.sequences.get_mut(expression) {
            if seq.len() > 1 {
                return Ok(seq.remove(0));
            }
            if let Some(v) = seq.first() {
                return Ok(v.clone());
            }
        }
        if let Some(v) = guard.scripts.get(expression) {
            return Ok(v.clone());
        }
        if let Some(v) = guard.default_eval.clone() {
            return Ok(v);
        }
        Err(PageDriverError::Protocol(format!(
            "kein Mock-Skript für: {expression}"
        )))
    }

    fn navigate(&mut self, url: &str, _timeout: Duration) -> Result<()> {
        let mut guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        if !guard.navigate_delay.is_zero() {
            std::thread::sleep(guard.navigate_delay);
        }
        guard.url = url.to_string();
        Ok(())
    }

    fn current_url(&mut self) -> Result<String> {
        let guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        Ok(guard.url.clone())
    }

    fn press_key(&mut self, _key: &str, _code: &str, _virtual_key: i64, _text: &str) -> Result<()> {
        Ok(())
    }

    fn insert_text(&mut self, _text: &str) -> Result<()> {
        Ok(())
    }

    fn click_at(&mut self, _x: f64, _y: f64) -> Result<()> {
        Ok(())
    }

    fn click_at_trusted(&mut self, _x: f64, _y: f64) -> Result<()> {
        Ok(())
    }

    fn move_pointer(&mut self, x: f64, y: f64) -> Result<()> {
        let mut guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        guard.pointer_moves.push((x, y));
        Ok(())
    }

    fn set_file_input_files(&mut self, files: &[(String, Vec<u8>)]) -> Result<()> {
        let mut guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        let names: Vec<String> = files.iter().map(|(n, _)| n.clone()).collect();
        guard.file_uploads.push(names);
        if guard.file_upload_ok {
            Ok(())
        } else {
            Err(PageDriverError::NotAvailable(
                "Mock: Datei-Upload nicht konfiguriert".into(),
            ))
        }
    }

    fn capture_png(&mut self) -> Result<Vec<u8>> {
        let guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        guard
            .png
            .clone()
            .ok_or_else(|| PageDriverError::Protocol("kein PNG im Mock hinterlegt".into()))
    }
}

/// Benannte Fixtures fuer reale beobachtete Ablaeufe (T-801, Scheibe 1).
///
/// Die rohen Antworttexte sind echte Funde aus dem Betrieb; jeder Builder
/// konfiguriert einen [`MockPageState`], der diese Ausgabe liefert. Klassifikat
/// und Storina pruefen sie ueber [`crate::contract::classify_surface`].
pub mod surface_fixtures {
    #[cfg(test)]
    use crate::contract::SurfaceKind;

    use super::MockPageState;
    use serde_json::json;

    /// Zai-HTML aus Lauf 20260721_173223: `No response…` + `Unexpected token '<'`
    /// samt HTML-artigem Fragment. Muss [`SurfaceKind::UiDiagnosis`] bleiben.
    pub fn zai_html() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("No response, Please try again later.\nSyntaxError: Unexpected token '<', \"<!doctypeh\"... is not valid JSON"),
        )
    }

    /// qwen-Tageslimit aus Lauf 20260721_225309 (sechs Wiederholungen).
    pub fn qwen_daily_limit() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("Oops! There was an issue connecting to Qwen3.7-Plus.\nYou have reached the daily usage limit. Please wait 2 hours before trying again."),
        )
    }

    /// ChatGPT-deutsches Nutzungslimit (Reset um 23:40).
    pub fn chatgpt_nutzungslimit() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("Dateien, Bilder und Datenanalyse sind nicht verfügbar, bis dein Nutzungslimit um 23:40 zurückgesetzt wird."),
        )
    }

    /// ChatGPT-Kapazitaetsbanner mit "Erneut versuchen".
    pub fn chatgpt_capacity_banner() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("Something went wrong. If this issue persists please contact us through our help center at help.openai.com.\n\nErneut versuchen"),
        )
    }

    /// Anmelde-Wand.
    pub fn login_wall() -> MockPageState {
        MockPageState::new().on_eval("fixture:assistant_text", json!("Please sign in to continue"))
    }

    /// Cloudflare-Challenge-Seite.
    pub fn cloudflare_challenge() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("Attention Required! Cloudflare"),
        )
    }

    /// Echte inhaltliche Antwort.
    pub fn real_content() -> MockPageState {
        MockPageState::new().on_eval(
            "fixture:assistant_text",
            json!("Die Aufgabe wurde abgeschlossen. Dies ist eine echte Zusammenfassung."),
        )
    }

    /// Erwartete Klassifikation zu Ordnung pruefen.
    #[cfg(test)]
    pub(crate) fn expected_kind_state() -> Vec<(&'static str, fn() -> MockPageState, SurfaceKind)> {
        vec![
            ("zai_html", zai_html, SurfaceKind::UiDiagnosis),
            ("qwen_daily_limit", qwen_daily_limit, SurfaceKind::Limit),
            ("chatgpt_nutzungslimit", chatgpt_nutzungslimit, SurfaceKind::Limit),
            ("chatgpt_capacity_banner", chatgpt_capacity_banner, SurfaceKind::Transient),
            ("login_wall", login_wall, SurfaceKind::Login),
            ("cloudflare_challenge", cloudflare_challenge, SurfaceKind::Challenge),
            ("real_content", real_content, SurfaceKind::Content),
        ]
    }

    /// Bewertet den Text hinter `fixture:assistant_text`.
    #[cfg(test)]
    pub(crate) fn classify_fixture(state: &MockPageState) -> SurfaceKind {
        use crate::contract::classify_surface;
        use crate::page_driver::PageDriver as _;
        let mut driver = crate::mock_page::MockPageDriver::new(state.clone());
        let text = driver
            .eval_string("fixture:assistant_text")
            .expect("Fixture liefert Text");
        classify_surface(&text, "", "ok").kind
    }
}

#[cfg(test)]
mod fixture_tests {
    use crate::contract::SurfaceKind;

    use super::surface_fixtures;

    #[test]
    fn real_observed_flows_classify_stably() {
        for (name, builder, expected) in surface_fixtures::expected_kind_state() {
            let state = builder();
            let kind = surface_fixtures::classify_fixture(&state);
            assert_eq!(
                kind, expected,
                "Fixture {name}: erwartet {expected:?}, bekommen {kind:?}"
            );
        }
    }

    #[test]
    fn zai_html_stays_raw_diagnosis() {
        use crate::page_driver::PageDriver as _;
        let state = surface_fixtures::zai_html();
        let mut driver = super::MockPageDriver::new(state.clone());
        let text = driver
            .eval_string("fixture:assistant_text")
            .expect("Zai-Fixture liefert Text");
        let outcome = crate::contract::classify_surface(&text, "", "brain_incomplete");
        assert_eq!(outcome.kind, SurfaceKind::UiDiagnosis, "Zai-HTML bleibt Rohbeleg");
        assert!(outcome.is_raw_diagnosis());
        assert!(!outcome.is_content());
        // Rohbeleg bleibt unveraendert erhalten.
        assert!(outcome.raw_text.contains("Unexpected token"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn mock_eval_and_navigate() {
        let state = MockPageState::new()
            .with_url("about:blank")
            .on_eval("1+1", json!(2));
        let mut driver = MockPageDriver::new(state);
        assert_eq!(driver.evaluate("1+1").unwrap(), json!(2));
        driver
            .navigate("https://example.com", Duration::ZERO)
            .unwrap();
        assert_eq!(driver.current_url().unwrap(), "https://example.com");
    }

    #[test]
    fn mock_missing_script_errors() {
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(driver.evaluate("missing()").is_err());
    }

    #[test]
    fn mock_navigate_honors_delay() {
        let state = MockPageState::new().navigate_delay(Duration::from_millis(30));
        let mut driver = MockPageDriver::new(state);
        let start = Instant::now();
        driver.navigate("https://a.test", Duration::ZERO).unwrap();
        assert!(start.elapsed() >= Duration::from_millis(25));
    }

    #[test]
    fn mock_records_set_file_input_files_when_enabled() {
        let state = MockPageState::new().with_file_upload_ok(true);
        let mut driver = MockPageDriver::new(state.clone());
        driver
            .set_file_input_files(&[("a.png".into(), vec![1, 2, 3])])
            .unwrap();
        assert_eq!(state.set_file_input_files_calls(), 1);
        assert_eq!(
            state.set_file_input_files_names(),
            vec![vec!["a.png".to_string()]]
        );
    }

    #[test]
    fn mock_default_eval_covers_unscripted_probes() {
        let state = MockPageState::new().with_default_eval(json!(0));
        let mut driver = MockPageDriver::new(state);
        assert_eq!(driver.evaluate("anything()").unwrap(), json!(0));
    }
}
