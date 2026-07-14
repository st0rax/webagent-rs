//! brain — BrainBackend-Trait und zugehörige Typen (portiert aus base.py).

/// Session-Status eines Brain-Backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    LoginRequired,
    Cloudflare,
    Error,
}

/// Antwort vom Brain-Backend nach einer Nachricht.
#[derive(Debug, Clone)]
pub struct BrainResponse {
    pub text: String,
    pub message_index: i32,
    pub generation_complete: bool,
    pub backend_status: String,
    pub raw_html: String,
}

impl Default for BrainResponse {
    fn default() -> Self {
        Self {
            text: String::new(),
            message_index: -1,
            generation_complete: true,
            backend_status: "ok".to_string(),
            raw_html: String::new(),
        }
    }
}

/// Trait für Brain-Backends (Browser-basierte LLM-Interfaces).
pub trait BrainBackend {
    /// Eindeutige ID des Backends (z.B. "chatgpt", "claude").
    fn brain_id(&self) -> &str;

    /// Startet das Backend (Browser-Session).
    fn start(&mut self, headless: bool) -> Result<(), String>;

    /// Stoppt das Backend und schließt den Browser.
    fn stop(&mut self) -> Result<(), String>;

    /// Wartet bis das Backend bereit ist (Login, Cloudflare, etc.).
    fn ensure_ready(&mut self, timeout: f64) -> Result<SessionState, String>;

    /// Gibt den aktuellen Session-Status zurück.
    fn session_state(&self) -> SessionState;

    /// Startet einen neuen Chat.
    fn new_chat(&mut self) -> Result<(), String>;

    /// Sendet eine Nachricht. Gibt assistant_count_before zurück.
    fn send(&mut self, text: &str) -> Result<i32, String>;

    /// Wartet auf die Antwort des Assistenten.
    fn wait_response(&mut self, baseline_count: i32, timeout: f64)
        -> Result<BrainResponse, String>;

    /// Prüft, ob der Benutzer eingeloggt ist.
    fn is_logged_in(&self) -> bool;

    /// Klickt auf den Login-Button (falls vorhanden).
    fn click_login(&mut self) -> Result<(), String>;

    /// Wartet darauf, dass der Benutzer sich einloggt.
    fn wait_for_login(&mut self, poll_interval: f64) -> Result<(), String>;

    /// Gibt eine Backend-neutrale Conversation-Referenz zurück (z.B. URL).
    fn get_conversation_ref(&self) -> Option<String>;

    /// Stellt eine gespeicherte Conversation wieder her.
    fn restore_conversation(&mut self, reference: &str) -> Result<bool, String>;

    /// Gibt Zugriff auf die Playwright-Page für Diagnose (optional).
    fn page(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dummy-Backend für Kompilier-Tests.
    struct DummyBrain {
        id: String,
        state: SessionState,
    }

    impl DummyBrain {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                state: SessionState::Ready,
            }
        }
    }

    impl BrainBackend for DummyBrain {
        fn brain_id(&self) -> &str {
            &self.id
        }

        fn start(&mut self, _headless: bool) -> Result<(), String> {
            self.state = SessionState::Ready;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn ensure_ready(&mut self, _timeout: f64) -> Result<SessionState, String> {
            Ok(self.state)
        }

        fn session_state(&self) -> SessionState {
            self.state
        }

        fn new_chat(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn send(&mut self, _text: &str) -> Result<i32, String> {
            Ok(0)
        }

        fn wait_response(
            &mut self,
            _baseline_count: i32,
            _timeout: f64,
        ) -> Result<BrainResponse, String> {
            Ok(BrainResponse {
                text: "Dummy response".to_string(),
                ..Default::default()
            })
        }

        fn is_logged_in(&self) -> bool {
            true
        }

        fn click_login(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn wait_for_login(&mut self, _poll_interval: f64) -> Result<(), String> {
            Ok(())
        }

        fn get_conversation_ref(&self) -> Option<String> {
            Some("dummy://conversation/123".to_string())
        }

        fn restore_conversation(&mut self, _reference: &str) -> Result<bool, String> {
            Ok(true)
        }
    }

    #[test]
    fn test_dummy_brain_compiles() {
        let mut brain = DummyBrain::new("test");
        assert_eq!(brain.brain_id(), "test");
        assert_eq!(brain.session_state(), SessionState::Ready);

        brain.start(true).unwrap();
        assert_eq!(brain.ensure_ready(10.0).unwrap(), SessionState::Ready);

        brain.new_chat().unwrap();
        let count = brain.send("Hello").unwrap();
        let response = brain.wait_response(count, 30.0).unwrap();
        assert_eq!(response.text, "Dummy response");

        assert!(brain.is_logged_in());
        assert_eq!(
            brain.get_conversation_ref(),
            Some("dummy://conversation/123".to_string())
        );

        brain.stop().unwrap();
    }

    #[test]
    fn test_session_state_enum() {
        assert_eq!(SessionState::Ready, SessionState::Ready);
        assert_ne!(SessionState::Ready, SessionState::LoginRequired);
    }

    #[test]
    fn test_brain_response_default() {
        let response = BrainResponse::default();
        assert_eq!(response.text, "");
        assert_eq!(response.message_index, -1);
        assert!(response.generation_complete);
        assert_eq!(response.backend_status, "ok");
        assert_eq!(response.raw_html, "");
    }
}
