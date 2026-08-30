//! Harnessfreie Browser-Inference-Grenze.
//!
//! Diese Schicht fuehrt genau einen Modellturn ueber eine angemeldete
//! Browser-Sitzung aus. Sie startet absichtlich keinen `AgentController`,
//! interpretiert kein `webagent/1` und fuehrt keine lokalen Werkzeuge aus.

use crate::relay::relay_single_turn;
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
    if request.prompt.trim().is_empty() {
        return Err("Inference-Prompt darf nicht leer sein.".to_string());
    }
    validate_tools(request.tools, &request.tool_choice)?;

    let prompt = prompt_with_tools(request.prompt, request.tools, &request.tool_choice)?;
    let text = relay_single_turn(
        request.brain,
        &prompt,
        request.headless,
        request.timeout_secs,
        request.model,
    )
    .map_err(|error| error.0)?;

    parse_response(&text, request.tools, &request.tool_choice)
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
        BrowserToolChoice::Auto => "Waehle selbst, ob ein Tool erforderlich ist.".to_string(),
        BrowserToolChoice::Required => "Du musst mindestens ein Tool aufrufen.".to_string(),
        BrowserToolChoice::Function(name) => format!("Du musst das Tool '{name}' aufrufen."),
        BrowserToolChoice::None => unreachable!("oben behandelt"),
    };

    Ok(format!(
        "{prompt}\n\n[Verfuegbare Tools]\n{tools_json}\n\n{choice_text} Wenn du ein Tool aufrufst, gib ausschliesslich diesen Maschinenumschlag aus:\n{TOOL_ENVELOPE}\n{{\"tool_calls\":[{{\"id\":\"call_eindeutig\",\"name\":\"tool_name\",\"arguments\":{{}}}}]}}\nKein Markdown und kein weiterer Text. Wenn kein Tool erforderlich ist, antworte normal ohne Maschinenumschlag."
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
}
