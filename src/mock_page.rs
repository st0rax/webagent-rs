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

    /// Registriert eine feste Antwort für ein exaktes JS-Expression.
    pub fn on_eval(self, expression: impl Into<String>, value: Value) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.scripts.insert(expression.into(), value);
        }
        self
    }

    pub fn navigate_delay(self, delay: Duration) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.navigate_delay = delay;
        }
        self
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
        let guard = self
            .state
            .inner
            .lock()
            .map_err(|_| PageDriverError::Protocol("Mock-Sperre verloren".into()))?;
        if let Some(v) = guard.scripts.get(expression) {
            return Ok(v.clone());
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
}
