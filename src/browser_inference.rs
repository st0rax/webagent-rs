//! Harnessfreie Browser-Inference-Grenze.
//!
//! Diese Schicht fuehrt genau einen Modellturn ueber eine angemeldete
//! Browser-Sitzung aus. Sie startet absichtlich keinen `AgentController`,
//! interpretiert kein `webagent/1` und fuehrt keine lokalen Werkzeuge aus.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const TOOL_ENVELOPE: &str = "WEBAGENT_INFERENCE/1";

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

/// Fuehrt genau einen Browser-Modellturn ohne Agent-Harness aus.
pub fn complete(request: BrowserInferenceRequest<'_>) -> Result<BrowserInferenceResponse, String> {
    complete_with_attachments(request, &[], &mut |_| {})
}

/// Wie [`complete`], jedoch mit Dateien, die vor dem Senden an den Browser-
/// Composer angehängt werden.
pub fn complete_with_attachments(
    request: BrowserInferenceRequest<'_>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<BrowserInferenceResponse, String> {
    complete_streaming_with_attachments(request, attachments, on_update)
}

/// Wie `complete`, liefert bei reinen Textturns bereits wachsende Snapshots.
/// Tool-Umschlaege bleiben bis zur finalen Validierung intern, damit niemals
/// ein halbes Maschinenprotokoll als Nutztext zum Client gelangt.
pub fn complete_streaming(
    request: BrowserInferenceRequest<'_>,
    on_update: &mut dyn FnMut(&str),
) -> Result<BrowserInferenceResponse, String> {
    complete_streaming_with_attachments(request, &[], on_update)
}

/// Streaming-Variante mit optionalen Bild-/Audio-Anhängen.
pub fn complete_streaming_with_attachments(
    request: BrowserInferenceRequest<'_>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<BrowserInferenceResponse, String> {
    if request.prompt.trim().is_empty() {
        return Err("Inference-Prompt darf nicht leer sein.".to_string());
    }
    validate_tools(request.tools, &request.tool_choice)?;
    validate_attachments(attachments)?;

    let prompt = prompt_with_tools(request.prompt, request.tools, &request.tool_choice)?;
    let forward_updates =
        request.tools.is_empty() || matches!(request.tool_choice, BrowserToolChoice::None);
    let mut relay_update = |snapshot: &str| {
        if forward_updates {
            on_update(snapshot);
        }
    };
    let text = crate::relay::relay_single_turn_with_attachments_streaming(
        request.brain,
        &prompt,
        request.headless,
        request.timeout_secs,
        request.model,
        attachments,
        &mut relay_update,
    )
    .map_err(|error| error.0)?;

    parse_response(&text, request.tools, &request.tool_choice)
}

fn validate_attachments(attachments: &[BrowserAttachment]) -> Result<(), String> {
    const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;
    if attachments.len() > 16 {
        return Err("Maximal 16 Bild-/Audio-Anhaenge pro Anfrage erlaubt.".to_string());
    }
    for attachment in attachments {
        if attachment.data.is_empty() {
            return Err(format!("Anhang '{}' ist leer.", attachment.file_name));
        }
        if attachment.data.len() > MAX_ATTACHMENT_BYTES {
            return Err(format!(
                "Anhang '{}' ist zu gross (Maximum 8 MiB).",
                attachment.file_name
            ));
        }
        let valid = match attachment.kind {
            BrowserAttachmentKind::Image => attachment.mime_type.starts_with("image/"),
            BrowserAttachmentKind::Audio => attachment.mime_type.starts_with("audio/"),
        };
        if !valid {
            return Err(format!(
                "MIME-Typ '{}' passt nicht zum Anhang '{}'.",
                attachment.mime_type, attachment.file_name
            ));
        }
    }
    Ok(())
}

fn validate_tools(tools: &[BrowserTool], choice: &BrowserToolChoice) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for tool in tools {
        if tool.name.trim().is_empty() {
            return Err("Toolname darf nicht leer sein.".to_string());
        }
        if !names.insert(tool.name.as_str()) {
            return Err(format!("Tool '{}' ist doppelt definiert.", tool.name));
        }
    }
    if matches!(choice, BrowserToolChoice::Required) && tools.is_empty() {
        return Err("tool_choice=required benoetigt mindestens ein Tool.".to_string());
    }
    if let BrowserToolChoice::Function(name) = choice {
        if !names.contains(name.as_str()) {
            return Err(format!(
                "tool_choice verweist auf unbekanntes Tool '{name}'."
            ));
        }
    }
    Ok(())
}

fn prompt_with_tools(
    prompt: &str,
    tools: &[BrowserTool],
    choice: &BrowserToolChoice,
) -> Result<String, String> {
    if tools.is_empty() || matches!(choice, BrowserToolChoice::None) {
        return Ok(prompt.to_string());
    }

    let tools_json = serde_json::to_string(tools)
        .map_err(|error| format!("Tooldefinitionen nicht serialisierbar: {error}"))?;
    let choice_text = match choice {
        BrowserToolChoice::Auto => "Waehle selbst, ob ein Tool erforderlich ist. Wenn die Nutzeranfrage ein Tool ausdruecklich verlangt oder die Antwort von lokalen, aktuellen oder dir nicht vorliegenden Daten abhaengt, musst du das passende Tool aufrufen; behaupte in diesem Fall nicht, du haettest es ohne Tool benutzt.".to_string(),
        BrowserToolChoice::Required => "Du musst mindestens ein Tool aufrufen.".to_string(),
        BrowserToolChoice::Function(name) => format!("Du musst das Tool '{name}' aufrufen."),
        BrowserToolChoice::None => unreachable!("oben behandelt"),
    };

    Ok(format!(
        "{prompt}\n\n[Externe Client-Tools]\n{tools_json}\n\nDiese Tools gehoeren dem aufrufenden API-Client und nicht der Browseroberflaeche. Sie sind verfuegbar: Du rufst sie auf, indem du den unten definierten Maschinenumschlag ausgibst. Versuche nicht, sie als eingebaute Browser-Tools zu bedienen, und behaupte niemals, sie seien in dieser Umgebung nicht verfuegbar. {choice_text} Wenn du ein Tool aufrufst, gib ausschliesslich diesen Maschinenumschlag aus:\n{TOOL_ENVELOPE}\n{{\"tool_calls\":[{{\"id\":\"call_eindeutig\",\"name\":\"tool_name\",\"arguments\":{{}}}}]}}\nKein Markdown und kein weiterer Text. Wenn kein Tool erforderlich ist, antworte normal ohne Maschinenumschlag."
    ))
}

#[derive(Deserialize)]
struct ToolEnvelope {
    tool_calls: Vec<ToolEnvelopeCall>,
}

#[derive(Deserialize)]
struct ToolEnvelopeCall {
    id: String,
    name: String,
    arguments: Value,
}

fn parse_response(
    raw: &str,
    tools: &[BrowserTool],
    choice: &BrowserToolChoice,
) -> Result<BrowserInferenceResponse, String> {
    let trimmed = raw.trim();
    let Some(payload) = trimmed.strip_prefix(TOOL_ENVELOPE) else {
        if matches!(
            choice,
            BrowserToolChoice::Required | BrowserToolChoice::Function(_)
        ) {
            return Err(
                "Das Browsermodell lieferte trotz verpflichtendem Tool-Call nur Text.".to_string(),
            );
        }
        return Ok(BrowserInferenceResponse {
            text: Some(trimmed.to_string()),
            tool_calls: Vec::new(),
        });
    };

    let envelope: ToolEnvelope = serde_json::from_str(payload.trim())
        .map_err(|error| format!("Ungueltiger Browser-Tool-Call-Umschlag: {error}"))?;
    if envelope.tool_calls.is_empty() {
        return Err("Browser-Tool-Call-Umschlag enthaelt keine Tool Calls.".to_string());
    }

    let allowed: BTreeSet<&str> = tools.iter().map(|tool| tool.name.as_str()).collect();
    let mut ids = BTreeSet::new();
    let mut calls = Vec::with_capacity(envelope.tool_calls.len());
    for call in envelope.tool_calls {
        if call.id.trim().is_empty() || !ids.insert(call.id.clone()) {
            return Err("Tool-Call-IDs muessen nichtleer und eindeutig sein.".to_string());
        }
        if !allowed.contains(call.name.as_str()) {
            return Err(format!(
                "Browsermodell rief unbekanntes Tool '{}' auf.",
                call.name
            ));
        }
        if let BrowserToolChoice::Function(required) = choice {
            if &call.name != required {
                return Err(format!(
                    "Browsermodell rief '{}' statt des verlangten Tools '{}' auf.",
                    call.name, required
                ));
            }
        }
        calls.push(BrowserToolCall {
            id: call.id,
            name: call.name,
            arguments: call.arguments,
        });
    }

    Ok(BrowserInferenceResponse {
        text: None,
        tool_calls: calls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_tool() -> BrowserTool {
        BrowserTool {
            name: "read_file".to_string(),
            description: Some("Datei lesen".to_string()),
            parameters: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        }
    }

    #[test]
    fn empty_prompt_fails_before_browser_start() {
        let error = complete(BrowserInferenceRequest {
            brain: "nonexistent_brain_xyz",
            prompt: "  ",
            tools: &[],
            tool_choice: BrowserToolChoice::Auto,
            headless: true,
            timeout_secs: None,
            model: None,
        })
        .unwrap_err();

        assert!(error.contains("darf nicht leer sein"));
    }

    #[test]
    fn strict_tool_envelope_is_normalized() {
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"README.md"}}]}"#
        );
        let response = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap();

        assert_eq!(response.finish_reason(), "tool_calls");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments["path"], "README.md");
    }

    #[test]
    fn unknown_tool_call_fails_closed() {
        let raw = concat!(
            "WEBAGENT_INFERENCE/1\n",
            r#"{"tool_calls":[{"id":"call_1","name":"shell","arguments":{}}]}"#
        );
        let error = parse_response(raw, &[read_tool()], &BrowserToolChoice::Auto).unwrap_err();
        assert!(error.contains("unbekanntes Tool"));
    }

    #[test]
    fn required_tool_rejects_plain_text() {
        let error = parse_response(
            "Ich wuerde die Datei lesen.",
            &[read_tool()],
            &BrowserToolChoice::Required,
        )
        .unwrap_err();
        assert!(error.contains("verpflichtendem Tool-Call"));
    }

    #[test]
    fn auto_choice_requires_tools_for_explicit_or_unknown_data_requests() {
        let prompt = prompt_with_tools(
            "Lies die Datei nonce.txt.",
            &[read_tool()],
            &BrowserToolChoice::Auto,
        )
        .unwrap();

        assert!(prompt.contains("ein Tool ausdruecklich verlangt"));
        assert!(prompt.contains("dir nicht vorliegenden Daten"));
        assert!(prompt.contains("gehoeren dem aufrufenden API-Client"));
        assert!(prompt.contains("niemals, sie seien in dieser Umgebung nicht verfuegbar"));
        assert!(prompt.contains("WEBAGENT_INFERENCE/1"));
    }
}
