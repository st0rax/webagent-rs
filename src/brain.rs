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

/// `true`, wenn eine Brain-Antwort inhaltsleer ist und ein erneuter Versuch
/// sinnvoll erscheint — leer, nur Whitespace, oder die Ausfallmeldung mancher
/// Oberflächen. (Von gemini im Benchmark gebaut, Ernte 2026-07-21.)
pub fn is_retryable_empty_response(response: &str) -> bool {
    let trimmed = response.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Besteht die Antwort AUSSCHLIESSLICH aus Ausfallmeldungen der Oberfläche,
    // hat das Brain nie geantwortet — dann ist das kein Formatfehler, sondern
    // ein Ausfall, und ein neuer Versuch ist das Richtige.
    //
    // Der Gleichheitsvergleich allein reichte nicht: zai lieferte am 2026-07-21
    // ZWEI Zeilen („No response, Please try again later." plus einen
    // SyntaxError der Weboberfläche), womit die exakte Prüfung ins Leere lief
    // und der Controller 2,5 Minuten auf eine Reparatur wartete, die nicht
    // kommen konnte.
    trimmed.lines().filter(|l| !l.trim().is_empty()).all(is_ui_failure_line)
}

/// `true`, wenn eine Zeile eine Ausfallmeldung der Weboberfläche ist (und kein
/// Inhalt des Modells).
fn is_ui_failure_line(line: &str) -> bool {
    let low = line.trim().to_lowercase();
    const UI_FAILURES: &[&str] = &[
        "no response",
        "please try again",
        "try again later",
        "is not valid json",
        "unexpected token",
        "<!doctype",
        "something went wrong",
        "an error occurred",
        "network error",
        "failed to fetch",
    ];
    UI_FAILURES.iter().any(|p| low.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_empty_response() {
        assert!(is_retryable_empty_response(""));
        assert!(is_retryable_empty_response("   \n\t  "));
        assert!(is_retryable_empty_response("No response, Please try again later."));
        assert!(!is_retryable_empty_response("Die Aufgabe wurde abgeschlossen."));
        // Eine Zeile, die NUR aus einer UI-Fehlermeldung besteht, ist jetzt
        // ebenfalls "kein Inhalt" — geminis Originalfassung verneinte das,
        // konnte damit aber den realen zai-Fall nicht fangen.
        assert!(is_retryable_empty_response("SyntaxError: Unexpected token"));
    }

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

    #[test]
    fn real_zai_ui_failure_is_recognised_as_retryable() {
        // Wortlaut aus dem Lauf 20260721_173223 (zai, status brain_incomplete):
        // zwei Zeilen, deshalb lief der reine Gleichheitsvergleich ins Leere.
        let raw = "No response, Please try again later.
                   SyntaxError: Unexpected token '<', \"<!doctypeh\"... is not valid JSON";
        assert!(is_retryable_empty_response(raw));
    }

    #[test]
    fn a_real_answer_is_never_treated_as_a_ui_failure() {
        assert!(!is_retryable_empty_response(
            "{\"protocol\":\"webagent/1\",\"actions\":[{\"id\":\"a\",\"type\":\"message\",\"text\":\"fertig\"}]}"
        ));
        assert!(!is_retryable_empty_response("Die Aufgabe wurde abgeschlossen."));
    }

    #[test]
    fn a_mixed_answer_counts_as_content() {
        // Enthaelt die Antwort NEBEN der Fehlermeldung echten Inhalt, ist sie
        // kein Ausfall — sonst verwirft der Filter brauchbare Arbeit.
        let mixed = "No response, Please try again later.
                     WEBAGENT/1 EDIT
                     path: src/brain.rs";
        assert!(!is_retryable_empty_response(mixed));
    }
}
