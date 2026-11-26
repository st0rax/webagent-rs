//! Shared, browser-free value types for the later core crate.
//!
//! This module owns the data shapes and presence contract that are used on
//! both sides of the future core/transport/api boundary. It deliberately has
//! no `crate::` dependencies: every consumer may depend on it, and it may not
//! depend on any higher layer. Old public paths stay available through
//! re-exports in `browser_inference`, `api_bridge::transport` and `brain_score`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const TOOL_ENVELOPE: &str = "WEBAGENT_INFERENCE/1";
/// Stabiler Marker fuer Fehler, die in der MODELLAUSGABE liegen (T-953).
/// `api_bridge/provider_handlers.rs` erkennt ihn und meldet `server_error` statt
/// `invalid_request_error`: die Client-Anfrage war gueltig, das Browsermodell
/// lieferte einen unbrauchbaren Werkzeugumschlag.
pub const MODEL_OUTPUT_ERROR_MARKER: &str = "MODEL_OUTPUT_ERROR:";

/// Kennzeichnet einen Parserfehler der Modellausgabe mit dem stabilen Marker.
pub fn model_output_error(message: impl std::fmt::Display) -> String {
    format!("{MODEL_OUTPUT_ERROR_MARKER} {message}")
}

/// Stabiler Fehlername für das native DSML-Werkzeugformat (T-957).
pub const DSML_TOOL_FORMAT_ERROR: &str = "DSML_TOOL_FORMAT_UNSUPPORTED";
/// DSML-Marker aus dem realen deepseek-Beispiel (2026-09-14). Die zweite Form
/// ist die im Taskboard gespeicherte, durch eine Zeichencodierung verformte
/// Variante desselben Markers.
pub const DSML_MARKERS: [&str; 2] = ["｜｜DSML｜｜", "´¢£´¢£DSML´¢£´¢£"];
// Browser chat UIs are substantially less tolerant of a giant tool prompt
// than their HTTP APIs. Keep the serialized schema block bounded while still
// retaining every tool name and its argument shape for the model.
//
// 2026-11-26 (T-1017): live chatgpt turns with 12 client tools failed at
// `ABSENDEKNOPF_DEAKTIVIERT` with a ~196k-char assembled prompt. Further live
// probes showed the real UI limit is near 140k chars (138,561 sent; 147,459
// rejected), not the ~200k claimed by an older code comment. Keep only the
// tool names and argument shapes; descriptions are compacted and, if needed,
// dropped entirely before the schema is truncated. The external client's
// ~132k prompt plus a small schema then stays under the UI limit.
pub const MAX_TOOL_SCHEMA_BYTES: usize = 12 * 1024;
/// Only the description-free form is sent. Skipping the intermediate 256/128
/// char steps keeps a 12-tool schema near 2.4 KiB instead of ~9 KiB; with the
/// external client's ~132k prompt that is the difference between sending and
/// a silent UI rejection. Names, property names and types stay untouched.
pub const TOOL_DESCRIPTION_LIMITS: &[usize] = &[0];
/// Oberhalb dieser Groesse lehnt die Browser-Oberflaeche die Eingabe still ab
/// (deaktivierter Absendeknopf). Ein zu grosser Prompt muss deshalb **vor** dem
/// Browserlauf scheitern, statt eine Minute die Brain-Sperre zu belegen.
/// Die Grenze wurde live eingegrenzt: 138.561 Zeichen wurden gesendet,
/// 147.459 Zeichen wurden abgelehnt.
pub const MAX_BROWSER_PROMPT_CHARS: usize = 140_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BrowserTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserToolChoice {
    Auto,
    None,
    Required,
    Function(String),
}

/// Eine vom API-Client eingereichte Multimodal-Datei.
///
/// Die Datei wird nach der Browser-Navigation in ein `input[type=file]` gesetzt;
/// der textuelle Prompt enthält nur einen transparenten Attachment-Marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserAttachment {
    pub kind: BrowserAttachmentKind,
    pub file_name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserAttachmentKind {
    Image,
    Audio,
}

/// Providerneutraler Auftrag fuer genau einen Browser-Modellturn.
#[derive(Debug, Clone)]
pub struct BrowserInferenceRequest<'a> {
    pub brain: &'a str,
    pub prompt: &'a str,
    pub tools: &'a [BrowserTool],
    pub tool_choice: BrowserToolChoice,
    pub headless: bool,
    pub timeout_secs: Option<f64>,
    pub model: Option<&'a str>,
    /// T-961: nur gesetzt von der OpenAI-Bridge auf einem Werkzeug-
    /// Fortsetzungsturn. Bei einer semantischen Verfuegbarkeitsverweigerung
    /// wird genau einmal in einem frischen Chat mit diesem Prompt retryt.
    pub retry_prompt: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrowserToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Normalisierte Antwort eines einzelnen Browser-Modellturns.
#[derive(Debug, Clone, PartialEq)]
pub struct BrowserInferenceResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<BrowserToolCall>,
}

impl BrowserInferenceResponse {
    pub fn finish_reason(&self) -> &'static str {
        if self.tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        }
    }
}

/// Beobachtet, ob der HTTP-Aufrufer noch verbunden ist (T-946).
///
/// Bewusst ein Trait: die Bridge benutzt einen echten TCP-Socket, Tests einen
/// schaltbaren Stub. Damit ist der Beleg "Folgeanfrage wartet nicht auf einen
/// verwaisten Turn" ohne echten Browser und ohne Zugangsdaten pruefbar.
pub trait ClientPresence: Send + Sync {
    fn connected(&self) -> bool;
}

/// Immer verbunden — fuer `fake_reply` und Pfade ohne echten Socket.
pub struct AlwaysConnected;

impl ClientPresence for AlwaysConnected {
    fn connected(&self) -> bool {
        true
    }
}

/// T-950: Zustand der anbieterseitigen Websuche unmittelbar vor dem Senden.
/// Der Zustand wird nicht erfunden: `status` beschreibt, ob ein Umschalter
/// vorhanden und erfolgreich ausgeschaltet wurde oder ob das Brain nicht
/// isolierbar ist. `before`/`after` sind die gemessenen Zustandsstrings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebSearchObservation {
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// Zustandsstrings koennen bei manchen Oberflaechen sehr lange Klassenlisten
/// sein; im Log reicht der Beleg, nicht das ganze DOM.
fn clip_state(state: &str) -> String {
    state.trim().chars().take(160).collect()
}

impl WebSearchObservation {
    pub fn turned_off(before: &str, after: &str) -> Self {
        Self {
            status: "off_before_send".to_string(),
            before: Some(clip_state(before)),
            after: Some(clip_state(after)),
        }
    }
    pub fn already_off(state: &str) -> Self {
        Self {
            status: "already_off".to_string(),
            before: Some(clip_state(state)),
            after: None,
        }
    }
    pub fn not_isolatable(reason: &str) -> Self {
        Self {
            status: "not_isolatable".to_string(),
            before: Some(clip_state(reason)),
            after: None,
        }
    }
    pub fn toggle_failed(before: &str, after: &str) -> Self {
        Self {
            status: "toggle_failed".to_string(),
            before: Some(clip_state(before)),
            after: Some(clip_state(after)),
        }
    }
    pub fn is_off(&self) -> bool {
        matches!(self.status.as_str(), "off_before_send" | "already_off")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_connected_reports_a_live_client() {
        assert!(AlwaysConnected.connected());
    }

    #[test]
    fn web_search_observation_keeps_only_a_bounded_proof() {
        let long = "x".repeat(500);
        let obs = WebSearchObservation::turned_off(&long, "aus");
        assert!(obs.is_off());
        assert_eq!(obs.before.as_deref().map(str::len), Some(160));
        assert_eq!(obs.after.as_deref(), Some("aus"));
    }

    #[test]
    fn response_finish_reason_follows_tool_calls() {
        let text_only = BrowserInferenceResponse {
            text: Some(" fertig ".into()),
            tool_calls: Vec::new(),
        };
        assert_eq!(text_only.finish_reason(), "stop");
        let with_tool = BrowserInferenceResponse {
            text: None,
            tool_calls: vec![BrowserToolCall {
                id: "1".into(),
                name: "read_file".into(),
                arguments: serde_json::json!({}),
            }],
        };
        assert_eq!(with_tool.finish_reason(), "tool_calls");
    }
}
