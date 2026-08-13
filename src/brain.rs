//! brain — BrainBackend-Trait und zugehörige Typen (portiert aus base.py).

/// Session-Status eines Brain-Backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Ready,
    /// Eine Anmelde-Wand wurde GESEHEN (sichtbarer Anmelden-Knopf). Ein Beleg.
    LoginRequired,
    Cloudflare,
    /// Weder Anmelde-Wand noch Anmelde-Nachweis gefunden.
    ///
    /// Bewusst von [`SessionState::LoginRequired`] getrennt: „ich habe eine
    /// Anmelde-Wand gesehen" und „ich habe meinen Indikator nicht gefunden"
    /// sind verschiedene Aussagen mit sehr verschiedener Sicherheit. Der
    /// zweite Fall entsteht auch bei einem Website-Umbau oder einer Seite, die
    /// noch nicht fertig geladen ist.
    ///
    /// Am 07.08.2026 landeten beide im selben Topf: alle acht Brains wurden mit
    /// „Login noetig" fuer sechs Stunden gesperrt, obwohl jedes einzelne
    /// angemeldet war — beim anschliessenden Login musste Storax kein einziges
    /// Mal Zugangsdaten eingeben.
    Unbestimmt,
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
    pub first_text_ms: Option<u64>,
    pub stop_first_seen_ms: Option<u64>,
    pub stop_gone_ms: Option<u64>,
    pub completion_ms: Option<u64>,
    pub completion_reason: Option<String>,
    pub polls: Option<u32>,
}

impl Default for BrainResponse {
    fn default() -> Self {
        Self {
            text: String::new(),
            message_index: -1,
            generation_complete: true,
            backend_status: "ok".to_string(),
            raw_html: String::new(),
            first_text_ms: None,
            stop_first_seen_ms: None,
            stop_gone_ms: None,
            completion_ms: None,
            completion_reason: None,
            polls: None,
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

    // Protokoll-Nutzlast hat Vorrang vor Banner-Phrasen. Dateiinhalt und
    // Aufgaben koennen legitimerweise Texte wie `usage limit` enthalten. Im
    // Run 20260813_182931 enthielt eine vollstaendige EDIT-Action genau solchen
    // Code und wurde vor dem Parser als `brain_unavailable` verworfen. Eine
    // begonnene Tool-Antwort muss stattdessen normal geparst bzw. repariert
    // werden; echte Provider-Banner enthalten keine WEBAGENT/1-Nutzlast.
    if crate::browser::has_protocol_payload(trimmed) {
        return false;
    }
    let low = trimmed.to_lowercase();

    // Anbieter-Block: eine EINZIGE solche Phrase genuegt. „You have reached the
    // daily usage limit" steht nie in einer echten Antwort — qwen lieferte am
    // 2026-07-21 daneben noch „Oops! There was an issue connecting to …", also
    // matchte nicht JEDE Zeile, und die Alles-oder-nichts-Pruefung liess den
    // Block durch. Sechs Wiederholungen gegen ein Brain, das fuer 2h weg war.
    if PROVIDER_BLOCK_PHRASES.iter().any(|p| low.contains(p)) {
        return true;
    }

    // UI-Glitch: hier ist die Alles-oder-nichts-Pruefung bewusst streng. Eine
    // Glitch-Phrase KANN neben echtem Inhalt stehen (zai: „No response …" plus
    // ein SyntaxError — beide Glitch; aber „No response …" plus ein echtes
    // WEBAGENT/1 EDIT ist Inhalt und darf nicht verworfen werden).
    trimmed
        .lines()
        .filter(|l| !l.trim().is_empty())
        .all(is_ui_glitch_line)
}

/// Phrasen, die eine Anbieter-Blockade signalisieren (Kontingent erschöpft,
/// Dienst überlastet). Spiegelt `browser::BLOCK_PHRASES` (dort für den
/// Seiten-Scan) — Änderungen hier und dort zusammen pflegen. Bewusst mehrwortig,
/// damit ein Fachvorschlag über „Rate-Limiting" nicht anschlägt.
const PROVIDER_BLOCK_PHRASES: &[&str] = &[
    "usage limit",
    "nutzungslimit",
    "daily limit",
    "message limit",
    "limit reached",
    "limit erreicht",
    "too many requests",
    "quota exceeded",
    "you have reached",
    "issue connecting",
    // ChatGPT-Weboberfläche bei temporärer Provider-Auslastung. Der zweite
    // Buttontext („Erneut versuchen") ist kein Modellinhalt und darf den
    // externen Block nicht zu einem Protokollfehler umdeuten.
    "if this issue persists please contact us through our help center",
];

/// `true`, wenn eine Zeile ein reiner UI-Glitch ist (leere/kaputte Antwort der
/// Oberfläche, kein Inhalt des Modells).
fn is_ui_glitch_line(line: &str) -> bool {
    let low = line.trim().to_lowercase();
    const UI_GLITCHES: &[&str] = &[
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
    UI_GLITCHES.iter().any(|p| low.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable_empty_response() {
        assert!(is_retryable_empty_response(""));
        assert!(is_retryable_empty_response("   \n\t  "));
        assert!(is_retryable_empty_response(
            "No response, Please try again later."
        ));
        assert!(!is_retryable_empty_response(
            "Die Aufgabe wurde abgeschlossen."
        ));
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
        assert!(!is_retryable_empty_response(
            "Die Aufgabe wurde abgeschlossen."
        ));
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

    #[test]
    fn qwens_daily_limit_is_recognised_as_unavailable() {
        // Wortlaut aus Lauf 20260721_225309: qwen antwortete sechsmal so, der
        // Controller wiederholte jedes Mal, und der Fehlschlag zaehlte gegen
        // qwen — obwohl es schlicht fuer 2 Stunden gesperrt war.
        let raw = "Oops! There was an issue connecting to Qwen3.7-Plus.
                   You have reached the daily usage limit. Please wait 2 hours before trying again.";
        assert!(is_retryable_empty_response(raw));
        let frag = "e reached the daily usage limit. Please wait 2 hours before trying again.";
        assert!(is_retryable_empty_response(frag));
    }

    #[test]
    fn chatgpt_german_usage_limit_is_recognised_as_unavailable() {
        assert!(is_retryable_empty_response(
            "Dateien, Bilder und Datenanalyse sind nicht verfügbar, bis dein Nutzungslimit um 23:40 zurückgesetzt wird."
        ));
    }

    #[test]
    fn chatgpt_capacity_banner_is_recognised_as_unavailable() {
        let raw = "Something went wrong. If this issue persists please contact us through our help center at help.openai.com.\n\nErneut versuchen";
        assert!(is_retryable_empty_response(raw));
    }

    #[test]
    fn a_suggestion_about_rate_limiting_is_not_a_block() {
        // Abgrenzung: "Rate-Limiting einfuehren" als Verbesserungsvorschlag
        // darf NICHT als Anbieter-Block gelten, sonst filtert der Schutz echte
        // Arbeit weg. (Die Blockphrasen sind bewusst mehrwortig.)
        assert!(!is_retryable_empty_response(
            "Rate-Limiting fuer die Brain-Abfragen einfuehren, damit Anbieter-Limits nicht gerissen werden"
        ));
        assert!(!is_retryable_empty_response(
            "WEBAGENT/1 EDIT
id: e1
path: src/x.rs
---OLD---
a
---NEW---
b
---END EDIT---"
        ));
    }

    #[test]
    fn protocol_payload_containing_provider_banner_is_not_unavailable() {
        let edit = "WEBAGENT/1 EDIT
id: instrument-block-detection
path: src/browser/blocking.rs
---OLD---
const MESSAGE: &str = \"usage limit\";
---NEW---
const MESSAGE: &str = \"usage limit reached\";
---END EDIT---";
        assert!(!is_retryable_empty_response(edit));

        let json = r#"{"protocol":"webagent/1","actions":[{"id":"m","type":"message","text":"usage limit handled"}]}"#;
        assert!(!is_retryable_empty_response(json));
    }
}
