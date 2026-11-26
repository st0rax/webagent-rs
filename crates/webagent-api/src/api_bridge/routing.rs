//! Request-Routenentscheidung und Streaming-Policy der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Dieses Modul bestimmt **nur**, welcher bestehende Handler fuer eine HTTP-
//! Anfrage gilt. Es aendert keine Endpunkte, keine Providerlogik und keine
//! HTTP-Bytes. Die Handler bleiben in `src/api_bridge.rs` unter denselben
//! Namen; T-913 verdrahtet [`classify`] in `route_request`.
//!
//! | [`BridgeRoute`] | kompatibler Handler |
//! |---|---|
//! | [`BridgeRoute::ResponsesIncremental`] | `handle_responses_incremental` |
//! | [`BridgeRoute::ChatIncremental`] | `handle_openai_incremental` |
//! | [`BridgeRoute::Health`] | Health-JSON in `route_request` |
//! | [`BridgeRoute::ModelsList`] | Models-Liste in `route_request` |
//! | [`BridgeRoute::ModelGet`] | Model-Metadaten in `route_request` |
//! | [`BridgeRoute::ResponseInputItems`] | `handle_response_input_items` |
//! | [`BridgeRoute::ResponseRetrieve`] | `handle_response_retrieve` |
//! | [`BridgeRoute::ResponseDelete`] | `handle_response_delete` |
//! | [`BridgeRoute::ChatCompletions`] | `handle_openai` |
//! | [`BridgeRoute::ImageGenerations`] | `handle_image_generation` |
//! | [`BridgeRoute::AudioTranscriptions`] | `handle_audio_transcription(..., false)` |
//! | [`BridgeRoute::AudioTranslations`] | `handle_audio_transcription(..., true)` |
//! | [`BridgeRoute::AudioSpeech`] | `handle_audio_speech` |
//! | [`BridgeRoute::Responses`] | `handle_responses` |
//! | [`BridgeRoute::Messages`] | `handle_anthropic` |
//! | [`BridgeRoute::NotFound`] | `api_error(..., 404, ...)` |
//!
//! Streaming-Policy: inkrementell nur bei `stream: true` und ohne aktive Tools
//! (`tools` leer/fehlend oder `tool_choice: "none"`).

use serde_json::Value;

/// Ergebnis der zentralen Routenentscheidung. Varianten entsprechen den
/// bestehenden Handlernamen in `src/api_bridge.rs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeRoute {
    ResponsesIncremental,
    ChatIncremental,
    Health,
    ModelsList,
    ModelGet { id: String },
    ResponseInputItems { path: String },
    ResponseRetrieve { path: String },
    ResponseDelete { path: String },
    ChatCompletions,
    ImageGenerations,
    AudioTranscriptions,
    AudioTranslations,
    AudioSpeech,
    Responses,
    Messages,
    NotFound { anthropic: bool },
}

/// Responses-SSE-Pfad: Textstream ohne aktive Tool-Runde.
pub fn is_incremental_text_request(body: &[u8]) -> bool {
    incremental_stream_without_tools(body)
}

/// Chat-Completions-SSE-Pfad: Textstream ohne aktive Tool-Runde.
pub fn is_incremental_chat_request(body: &[u8]) -> bool {
    incremental_stream_without_tools(body)
}

fn incremental_stream_without_tools(body: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    if value.get("stream").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    let no_tools = value
        .get("tools")
        .and_then(Value::as_array)
        .is_none_or(Vec::is_empty);
    let tools_disabled = value.get("tool_choice").and_then(Value::as_str) == Some("none");
    no_tools || tools_disabled
}

/// Zentrale Request-Routenentscheidung (Methode, Pfad, Streaming-Policy).
pub fn classify(method: &str, path: &str, body: &[u8]) -> BridgeRoute {
    if method == "POST" && path == "/v1/responses" && is_incremental_text_request(body) {
        return BridgeRoute::ResponsesIncremental;
    }
    if method == "POST" && path == "/v1/chat/completions" && is_incremental_chat_request(body) {
        return BridgeRoute::ChatIncremental;
    }

    match (method, path) {
        ("GET", "/health") => BridgeRoute::Health,
        ("GET", "/v1/models") => BridgeRoute::ModelsList,
        ("GET", path) if path.starts_with("/v1/models/") => BridgeRoute::ModelGet {
            id: path.trim_start_matches("/v1/models/").to_string(),
        },
        ("GET", path) if path.starts_with("/v1/responses/") && path.ends_with("/input_items") => {
            BridgeRoute::ResponseInputItems {
                path: path.to_string(),
            }
        }
        ("GET", path) if path.starts_with("/v1/responses/") => BridgeRoute::ResponseRetrieve {
            path: path.to_string(),
        },
        ("DELETE", path) if path.starts_with("/v1/responses/") => BridgeRoute::ResponseDelete {
            path: path.to_string(),
        },
        ("POST", "/v1/chat/completions") => BridgeRoute::ChatCompletions,
        ("POST", "/v1/images/generations") => BridgeRoute::ImageGenerations,
        ("POST", "/v1/audio/transcriptions") => BridgeRoute::AudioTranscriptions,
        ("POST", "/v1/audio/translations") => BridgeRoute::AudioTranslations,
        ("POST", "/v1/audio/speech") => BridgeRoute::AudioSpeech,
        ("POST", "/v1/responses") => BridgeRoute::Responses,
        ("POST", "/v1/messages") => BridgeRoute::Messages,
        _ => BridgeRoute::NotFound {
            anthropic: path == "/v1/messages",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_route_is_only_used_for_text_streams() {
        assert!(is_incremental_text_request(
            br#"{"model":"webagent/chatgpt","input":"x","stream":true}"#
        ));
        assert!(!is_incremental_text_request(
            br#"{"model":"webagent/chatgpt","input":"x","stream":false}"#
        ));
        assert!(!is_incremental_text_request(
            br#"{"model":"webagent/chatgpt","input":"x","stream":true,"tools":[{"type":"function","name":"f"}]}"#
        ));
        assert!(is_incremental_text_request(
            br#"{"model":"webagent/chatgpt","input":"x","stream":true,"tools":[{"type":"function","name":"f"}],"tool_choice":"none"}"#
        ));
        assert!(is_incremental_chat_request(
            br#"{"model":"webagent/chatgpt","messages":[],"stream":true}"#
        ));
        assert!(!is_incremental_chat_request(
            br#"{"model":"webagent/chatgpt","messages":[],"stream":true,"tools":[{"type":"function","name":"f"}]}"#
        ));
    }

    #[test]
    fn classify_matches_existing_handler_names() {
        let stream = br#"{"stream":true}"#;
        let buffered = br#"{"stream":false}"#;
        assert_eq!(
            classify("POST", "/v1/responses", stream),
            BridgeRoute::ResponsesIncremental
        );
        assert_eq!(
            classify("POST", "/v1/chat/completions", stream),
            BridgeRoute::ChatIncremental
        );
        assert_eq!(
            classify("POST", "/v1/responses", buffered),
            BridgeRoute::Responses
        );
        assert_eq!(
            classify("POST", "/v1/chat/completions", buffered),
            BridgeRoute::ChatCompletions
        );
        assert_eq!(classify("GET", "/health", b""), BridgeRoute::Health);
        assert_eq!(classify("GET", "/v1/models", b""), BridgeRoute::ModelsList);
        assert_eq!(
            classify("GET", "/v1/models/webagent/chatgpt", b""),
            BridgeRoute::ModelGet {
                id: "webagent/chatgpt".to_string()
            }
        );
        assert_eq!(
            classify("GET", "/v1/responses/resp_1/input_items", b""),
            BridgeRoute::ResponseInputItems {
                path: "/v1/responses/resp_1/input_items".to_string()
            }
        );
        assert_eq!(
            classify("GET", "/v1/responses/resp_1", b""),
            BridgeRoute::ResponseRetrieve {
                path: "/v1/responses/resp_1".to_string()
            }
        );
        assert_eq!(
            classify("DELETE", "/v1/responses/resp_1", b""),
            BridgeRoute::ResponseDelete {
                path: "/v1/responses/resp_1".to_string()
            }
        );
        assert_eq!(
            classify("POST", "/v1/images/generations", b""),
            BridgeRoute::ImageGenerations
        );
        assert_eq!(
            classify("POST", "/v1/audio/transcriptions", b""),
            BridgeRoute::AudioTranscriptions
        );
        assert_eq!(
            classify("POST", "/v1/audio/translations", b""),
            BridgeRoute::AudioTranslations
        );
        assert_eq!(
            classify("POST", "/v1/audio/speech", b""),
            BridgeRoute::AudioSpeech
        );
        assert_eq!(classify("POST", "/v1/messages", b""), BridgeRoute::Messages);
        assert_eq!(
            classify("GET", "/unknown", b""),
            BridgeRoute::NotFound { anthropic: false }
        );
        assert_eq!(
            classify("PUT", "/v1/messages", b""),
            BridgeRoute::NotFound { anthropic: true }
        );
    }
}
