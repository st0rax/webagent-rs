//! Prompt-, Content- und Tool-Normalizer der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! JSON-Feldvertraege bleiben hart (kein stilles Ignorieren). Keine SSE-Bytes
//! und kein Browser-Inferenzlauf. T-921 verdrahtet `mod content`.
//!
//! Enthaelt: reject_unsupported, Prompts, Content-Teile, Tool-Schemas,
//! tool_choice, Data-URL/Base64 fuer Attachments, text_content.

use super::{
    api_error_code, AnthropicRequest, ConversationMessage, HttpResponse, OpenAiRequest,
    OpenAiTool, PromptBundle, ResponsesRequest,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

pub(super) fn decode_json<T: DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|error| format!("Ungueltiger JSON-Body: {error}"))
}

/// Bekannte, aber nicht umsetzbare Semantikfelder: ablehnen statt still ignorieren.
pub(super) fn reject_unsupported_openai_body(body: &[u8]) -> Result<(), HttpResponse> {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    reject_unsupported_openai_fields(&value)
}

pub(super) fn reject_unsupported_openai_fields(value: &Value) -> Result<(), HttpResponse> {
    let Some(obj) = value.as_object() else {
        return Ok(());
    };
    const UNSUPPORTED: &[&str] = &[
        "seed",
        "service_tier",
        "logit_bias",
        "best_of",
        "echo",
        "suffix",
        "top_logprobs",
    ];
    for key in UNSUPPORTED {
        if obj.get(*key).is_some_and(|v| !v.is_null()) {
            return Err(unsupported_parameter(key));
        }
    }
    if let Some(n) = obj.get("n") {
        if !n.is_null() && n.as_u64() != Some(1) {
            return Err(unsupported_value(
                "n",
                "n>1 wird nicht unterstuetzt; nur n=1.",
            ));
        }
    }
    match obj.get("logprobs") {
        Some(v) if v.as_bool() == Some(true) || v.is_number() => {
            return Err(unsupported_parameter("logprobs"));
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn unsupported_parameter(param: &str) -> HttpResponse {
    api_error_code(
        400,
        &format!("Parameter '{param}' wird nicht unterstuetzt."),
        param,
        "unsupported_parameter",
    )
}

pub(super) fn unsupported_value(param: &str, message: &str) -> HttpResponse {
    api_error_code(400, message, param, "unsupported_value")
}

pub(super) fn default_true() -> bool {
    true
}

#[cfg(test)]
pub(super) fn openai_task(request: &OpenAiRequest) -> Result<String, String> {
    Ok(openai_prompt(request)?.text)
}

pub(super) fn openai_prompt(request: &OpenAiRequest) -> Result<PromptBundle, String> {
    conversation_prompt(None, &request.messages)
}

pub(super) fn anthropic_prompt(request: &AnthropicRequest) -> Result<PromptBundle, String> {
    let system = match &request.system {
        Some(content) => Some(text_content(content)?),
        None => None,
    };
    conversation_prompt(system, &request.messages)
}

#[cfg(test)]
pub(super) fn responses_task(request: &ResponsesRequest) -> Result<String, String> {
    let messages = responses_messages(&request.input)?;
    Ok(conversation_prompt(request.instructions.clone(), &messages)?.text)
}

pub(super) fn responses_messages(input: &Value) -> Result<Vec<ConversationMessage>, String> {
    let messages = match input {
        Value::String(text) => vec![ConversationMessage {
            role: "user".to_string(),
            content: Value::String(text.clone()),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
        Value::Array(items) => items
            .iter()
            .map(|item| {
                let object = item
                    .as_object()
                    .ok_or_else(|| "Responses-Input-Items muessen Objekte sein.".to_string())?;
                if object.get("type").and_then(Value::as_str) == Some("function_call_output") {
                    let id = object
                        .get("call_id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.trim().is_empty())
                        .ok_or_else(|| "function_call_output benoetigt call_id.".to_string())?;
                    let output = object.get("output").cloned().unwrap_or(Value::Null);
                    return Ok(ConversationMessage {
                        role: "tool".to_string(),
                        content: responses_function_output(&output)?,
                        tool_calls: Vec::new(),
                        tool_call_id: Some(id.to_string()),
                    });
                }
                if object.get("type").and_then(Value::as_str) == Some("function_call") {
                    let id = object
                        .get("call_id")
                        .and_then(Value::as_str)
                        .filter(|id| !id.trim().is_empty())
                        .ok_or_else(|| "function_call benoetigt call_id.".to_string())?;
                    let name = object
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|name| !name.trim().is_empty())
                        .ok_or_else(|| "function_call benoetigt name.".to_string())?;
                    let arguments = object
                        .get("arguments")
                        .cloned()
                        .unwrap_or_else(|| json!({}));
                    let arguments = if let Some(arguments) = arguments.as_str() {
                        arguments.to_string()
                    } else {
                        serde_json::to_string(&arguments).map_err(|error| {
                            format!("function_call arguments nicht serialisierbar: {error}")
                        })?
                    };
                    return Ok(ConversationMessage {
                        role: "assistant".to_string(),
                        content: Value::Null,
                        tool_calls: vec![OpenAiAssistantToolCall {
                            id: id.to_string(),
                            kind: "function".to_string(),
                            function: OpenAiAssistantFunction {
                                name: name.to_string(),
                                arguments,
                            },
                        }],
                        tool_call_id: None,
                    });
                }
                let role = object
                    .get("role")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Responses-Input-Item benoetigt role.".to_string())?;
                let content = responses_content(object.get("content").unwrap_or(&Value::Null))?;
                Ok(ConversationMessage {
                    role: role.to_string(),
                    content,
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        _ => return Err("Responses-Input muss ein String oder ein Array sein.".to_string()),
    };
    Ok(messages)
}

pub(super) fn responses_content(value: &Value) -> Result<Value, String> {
    if value.is_string() {
        return Ok(value.clone());
    }
    let Some(parts) = value.as_array() else {
        return Err("Responses-content muss String oder Content-Array sein.".to_string());
    };
    for part in parts {
        if !part.is_object() {
            return Err("Responses-Content-Parts muessen Objekte sein.".to_string());
        }
        let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
        if !matches!(
            part_type,
            "text"
                | "input_text"
                | "output_text"
                | "input_image"
                | "image_url"
                | "input_audio"
                | "audio"
        ) {
            return Err(format!(
                "Responses-Inhaltstyp '{part_type}' wird nicht unterstuetzt."
            ));
        }
        if matches!(part_type, "text" | "input_text" | "output_text")
            && part.get("text").and_then(Value::as_str).is_none()
        {
            return Err("Responses-Textblock ohne String-Feld 'text'.".to_string());
        }
    }
    Ok(value.clone())
}

pub(super) fn responses_function_output(value: &Value) -> Result<Value, String> {
    if value.is_string() {
        return Ok(value.clone());
    }
    if value.is_array() {
        return responses_content(value);
    }
    Ok(Value::String(value.to_string()))
}

pub(super) fn responses_tools(tools: &[Value]) -> Result<Vec<crate::browser_inference::BrowserTool>, String> {
    tools
        .iter()
        .map(|tool| {
            let object = tool
                .as_object()
                .ok_or_else(|| "Responses-Tool muss ein Objekt sein.".to_string())?;
            if object.get("type").and_then(Value::as_str) != Some("function") {
                return Err("Responses unterstuetzt derzeit nur function-Tools.".to_string());
            }
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| "Responses-function-Tool benoetigt name.".to_string())?;
            Ok(crate::browser_inference::BrowserTool {
                name: name.to_string(),
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                parameters: object
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

pub(super) fn responses_tool_choice(
    choice: Option<&Value>,
    tools: &[crate::browser_inference::BrowserTool],
) -> Result<crate::browser_inference::BrowserToolChoice, String> {
    let Some(choice) = choice else {
        return Ok(if tools.is_empty() {
            crate::browser_inference::BrowserToolChoice::None
        } else {
            crate::browser_inference::BrowserToolChoice::Auto
        });
    };
    if let Some(value) = choice.as_str() {
        return match value {
            "auto" => Ok(crate::browser_inference::BrowserToolChoice::Auto),
            "none" => Ok(crate::browser_inference::BrowserToolChoice::None),
            "required" => Ok(crate::browser_inference::BrowserToolChoice::Required),
            other => Err(format!("Unbekannter Responses-tool_choice '{other}'.")),
        };
    }
    let name = choice
        .get("name")
        .or_else(|| choice.get("function").and_then(|f| f.get("name")))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "Responses-tool_choice benoetigt name.".to_string())?;
    if !tools.iter().any(|tool| tool.name == name) {
        return Err(format!(
            "tool_choice verweist auf unbekanntes Tool '{name}'."
        ));
    }
    Ok(crate::browser_inference::BrowserToolChoice::Function(
        name.to_string(),
    ))
}

pub(super) fn anthropic_tools(tools: &[Value]) -> Result<Vec<crate::browser_inference::BrowserTool>, String> {
    tools
        .iter()
        .map(|tool| {
            let object = tool
                .as_object()
                .ok_or_else(|| "Anthropic-Tool muss ein Objekt sein.".to_string())?;
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| "Anthropic-Tool benoetigt name.".to_string())?;
            let input_schema = object
                .get("input_schema")
                .or_else(|| object.get("parameters"));
            Ok(crate::browser_inference::BrowserTool {
                name: name.to_string(),
                description: object
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                parameters: input_schema.cloned().unwrap_or_else(|| json!({})),
            })
        })
        .collect()
}

pub(super) fn anthropic_tool_choice(
    choice: Option<&Value>,
    tools: &[crate::browser_inference::BrowserTool],
) -> Result<crate::browser_inference::BrowserToolChoice, String> {
    use crate::browser_inference::BrowserToolChoice;
    let Some(choice) = choice else {
        return Ok(if tools.is_empty() {
            BrowserToolChoice::None
        } else {
            BrowserToolChoice::Auto
        });
    };
    let object = choice
        .as_object()
        .ok_or_else(|| "Anthropic-tool_choice muss ein Objekt sein.".to_string())?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| "Anthropic-tool_choice benoetigt type.".to_string())?;
    match kind {
        "auto" => Ok(BrowserToolChoice::Auto),
        "any" => Ok(BrowserToolChoice::Required),
        "tool" => {
            let name = object
                .get("name")
                .and_then(Value::as_str)
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| "Anthropic-tool_choice vom Typ tool benoetigt name.".to_string())?;
            if !tools.iter().any(|tool| tool.name == name) {
                return Err(format!(
                    "tool_choice verweist auf unbekanntes Tool '{name}'."
                ));
            }
            Ok(BrowserToolChoice::Function(name.to_string()))
        }
        other => Err(format!("Unbekannter Anthropic-tool_choice-Typ '{other}'.")),
    }
}

pub(super) fn require_clean_text_tools(
    tools: &[crate::browser_inference::BrowserTool],
    choice: &crate::browser_inference::BrowserToolChoice,
) -> Result<(), String> {

#[cfg(test)]
pub(super) fn conversation_task(
    system: Option<String>,
    messages: &[ConversationMessage],
) -> Result<String, String> {
    Ok(conversation_prompt(system, messages)?.text)
}

pub(super) fn conversation_prompt(
    system: Option<String>,
    messages: &[ConversationMessage],
) -> Result<PromptBundle, String> {
    if messages.is_empty() {
        return Err("messages darf nicht leer sein.".to_string());
    }
    if system.is_some_and(|value| !value.trim().is_empty()) {
        return Err(concat!(
            "System-/Instructions-Semantik ist im sauberen Browser-Textprofil noch nicht ",
            "unterstuetzt; WebAgent schreibt sie nicht als versteckte Nutzernachricht in den Chat."
        )
        .to_string());
    }

    for message in messages {
        if !message.tool_calls.is_empty() || message.tool_call_id.is_some() {
            return Err(concat!(
                "Tool-Call-Verlaeufe sind im sauberen Browser-Textprofil noch nicht ",
                "unterstuetzt; WebAgent injiziert keine Tool-Protokolle in den Chat."
            )
            .to_string());
        }
        if !matches!(message.role.as_str(), "user" | "assistant") {
            return Err(format!(
                "Rolle '{}' ist im sauberen Browser-Textprofil nicht unterstuetzt.",
                message.role
            ));
        }
    }

    let current = messages
        .last()
        .ok_or_else(|| "messages darf nicht leer sein.".to_string())?;
    if current.role != "user" {
        return Err(
            "Die letzte Nachricht muss im sauberen Browser-Textprofil die Rolle 'user' haben."
                .to_string(),
        );
    }

    let mut attachments = Vec::new();
    let current_text = content_to_prompt(&current.content, &mut attachments)?;
    if messages.len() == 1 {
        return Ok(PromptBundle {
            text: current_text,
            attachments,
        });
    }

    let mut task = String::from("Gespraechsverlauf mit [brain]:\n\n");
    for message in &messages[..messages.len() - 1] {
        // Historische Anhaenge werden nur als Textmarker erwaehnt. Sie duerfen
        // nicht bei jeder Fortsetzung erneut in die Browser-UI hochgeladen
        // werden und den aktuellen Composer blockieren.
        let mut historical_attachments = Vec::new();
        let content = content_to_prompt(&message.content, &mut historical_attachments)?;
        let label = if message.role == "assistant" {
            "brain"
        } else {
            "user"
        };
        task.push_str(&format!("[{label}]\n{content}\n\n"));
    }
    task.push_str("Aktuelle Nachricht:\n\n");
    task.push_str(&current_text);

    Ok(PromptBundle {
        text: task,
        attachments,
    })
}

/// Rendert einen Provider-Content-Block in den textuellen Browser-Prompt und
/// sammelt Bild-/Audio-Daten fuer den separaten Upload in die Weboberflaeche.
pub(super) fn content_to_prompt(
    value: &Value,
    attachments: &mut Vec<crate::browser_inference::BrowserAttachment>,
) -> Result<String, String> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_string());
    }
    let Some(parts) = value.as_array() else {
        return Err("content muss ein Text oder ein Content-Array sein.".to_string());
    };
    let mut out = String::new();
    for part in parts {
        let object = part
            .as_object()
            .ok_or_else(|| "Content-Parts muessen Objekte sein.".to_string())?;
        let part_type = object.get("type").and_then(Value::as_str).unwrap_or("");
        match part_type {
            "text" | "input_text" | "output_text" => {
                let text = object
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Textblock ohne String-Feld 'text'.".to_string())?;
                out.push_str(text);
            }
            "image_url" => {
                let url = object
                    .get("image_url")
                    .and_then(|value| {
                        value
                            .as_str()
                            .or_else(|| value.get("url").and_then(Value::as_str))
                    })
                    .ok_or_else(|| "image_url benoetigt ein String-Feld 'url'.".to_string())?;
                let (mime, data) =
                    parse_data_url(url, crate::browser_inference::BrowserAttachmentKind::Image)?;
                out.push_str(&append_attachment(
                    attachments,
                    crate::browser_inference::BrowserAttachmentKind::Image,
                    mime,
                    data,
                ));
            }
            "input_image" => {
                let url = object
                    .get("image_url")
                    .and_then(Value::as_str)
                    .or_else(|| object.get("image_url").and_then(|value| value.get("url")).and_then(Value::as_str))
                    .ok_or_else(|| {
                        if object.get("file_id").is_some() {
                            "input_image mit file_id wird von der Browser-Bridge nicht unterstuetzt; sende eine data-URL."
                                .to_string()
                        } else {
                            "input_image benoetigt image_url als data-URL.".to_string()
                        }
                    })?;
                let (mime, data) =
                    parse_data_url(url, crate::browser_inference::BrowserAttachmentKind::Image)?;
                out.push_str(&append_attachment(
                    attachments,
                    crate::browser_inference::BrowserAttachmentKind::Image,
                    mime,
                    data,
                ));
            }
            "input_audio" => {
                let audio = object
                    .get("input_audio")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        "input_audio benoetigt ein Objekt mit data und format.".to_string()
                    })?;
                let encoded = audio
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "input_audio benoetigt ein Base64-Feld 'data'.".to_string())?;
                let format = audio.get("format").and_then(Value::as_str).ok_or_else(|| {
                    "input_audio benoetigt ein Format (z.B. wav oder mp3).".to_string()
                })?;
                let mime = audio_mime(format)?;
                let data = decode_base64(encoded)?;
                out.push_str(&append_attachment(
                    attachments,
                    crate::browser_inference::BrowserAttachmentKind::Audio,
                    mime,
                    data,
                ));
            }
            "image" | "audio" => {
                let kind = if part_type == "image" {
                    crate::browser_inference::BrowserAttachmentKind::Image
                } else {
                    crate::browser_inference::BrowserAttachmentKind::Audio
                };
                let source = object
                    .get("source")
                    .and_then(Value::as_object)
                    .ok_or_else(|| format!("{part_type}-Block benoetigt source."))?;
                let source_type = source.get("type").and_then(Value::as_str).unwrap_or("");
                if source_type == "url" {
                    return Err(format!(
                        "{part_type}-URLs werden von der Browser-Bridge nicht automatisch heruntergeladen; sende Base64/data-URL."
                    ));
                }
                if source_type != "base64" {
                    return Err(format!("{part_type}-source benoetigt type 'base64'."));
                }
                let encoded = source
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{part_type}-source benoetigt Base64-Feld 'data'."))?;
                let mime = source
                    .get("media_type")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("{part_type}-source benoetigt media_type."))?
                    .to_string();
                validate_mime(&mime, kind)?;
                let data = decode_base64(encoded)?;
                out.push_str(&append_attachment(attachments, kind, mime, data));
            }
            other => {
                return Err(format!(
                    "Inhaltstyp '{other}' wird von der Browser-Bridge nicht unterstuetzt."
                ));
            }
        }
    }
    Ok(out)
}

pub(super) fn append_attachment(
    attachments: &mut Vec<crate::browser_inference::BrowserAttachment>,
    kind: crate::browser_inference::BrowserAttachmentKind,
    mime_type: String,
    data: Vec<u8>,
) -> String {
    let index = attachments.len() + 1;
    let prefix = match kind {
        crate::browser_inference::BrowserAttachmentKind::Image => "image",
        crate::browser_inference::BrowserAttachmentKind::Audio => "audio",
    };
    let extension = mime_type
        .split('/')
        .nth(1)
        .unwrap_or("bin")
        .split(';')
        .next()
        .unwrap_or("bin")
        .replace(['+', ' '], "_");
    let file_name = format!("{prefix}-{index}.{extension}");
    let byte_count = data.len();
    attachments.push(crate::browser_inference::BrowserAttachment {
        kind,
        file_name,
        mime_type: mime_type.clone(),
        data,
    });
    format!("[{prefix} attachment: {mime_type}, {byte_count} bytes]")
}

pub(super) fn parse_data_url(
    url: &str,
    expected: crate::browser_inference::BrowserAttachmentKind,
) -> Result<(String, Vec<u8>), String> {
    let Some(payload) = url.strip_prefix("data:") else {
        return Err("Remote Bild-/Audio-URLs werden nicht automatisch heruntergeladen; sende eine data-URL.".to_string());
    };
    let (metadata, encoded) = payload
        .split_once(',')
        .ok_or_else(|| "Data-URL ohne Nutzdaten.".to_string())?;
    if !metadata
        .split(';')
        .any(|flag| flag.eq_ignore_ascii_case("base64"))
    {
        return Err("Data-URL muss ;base64 verwenden.".to_string());
    }
    let mime = metadata
        .split(';')
        .next()
        .filter(|mime| !mime.is_empty())
        .ok_or_else(|| "Data-URL benoetigt einen MIME-Typ.".to_string())?
        .to_string();
    validate_mime(&mime, expected)?;
    Ok((mime, decode_base64(encoded)?))
}

pub(super) fn validate_mime(
    mime: &str,
    expected: crate::browser_inference::BrowserAttachmentKind,
) -> Result<(), String> {
    let valid = match expected {
        crate::browser_inference::BrowserAttachmentKind::Image => mime.starts_with("image/"),
        crate::browser_inference::BrowserAttachmentKind::Audio => mime.starts_with("audio/"),
    };
    if valid {
        Ok(())
    } else {
        Err(format!("MIME-Typ '{mime}' passt nicht zum Content-Block."))
    }
}

pub(super) fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
    let bytes: Vec<u8> = encoded
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err("Ungueltige Base64-Nutzdaten.".to_string());
    }
    let mut output = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.as_chunks::<4>().0 {
        let a = base64_value(chunk[0]).ok_or_else(|| "Ungueltiges Base64-Zeichen.".to_string())?;
        let b = base64_value(chunk[1]).ok_or_else(|| "Ungueltiges Base64-Zeichen.".to_string())?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            base64_value(chunk[2]).ok_or_else(|| "Ungueltiges Base64-Zeichen.".to_string())?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            base64_value(chunk[3]).ok_or_else(|| "Ungueltiges Base64-Zeichen.".to_string())?
        };
        output.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if chunk[3] != b'=' {
            output.push((c << 6) | d);
        }
        if chunk[2] == b'=' && chunk[3] != b'=' {
            return Err("Ungueltige Base64-Padding.".to_string());
        }
    }
    Ok(output)
}

pub(super) fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

pub(super) fn openai_tools(
    tools: &[OpenAiTool],
) -> Result<Vec<crate::browser_inference::BrowserTool>, String> {
    tools
        .iter()
        .map(|tool| {
            if tool.kind != "function" {
                return Err(format!(
                    "Tool-Typ '{}' wird nicht unterstuetzt; erwartet wird 'function'.",
                    tool.kind
                ));
            }
            Ok(crate::browser_inference::BrowserTool {
                name: tool.function.name.clone(),
                description: tool.function.description.clone(),
                parameters: tool.function.parameters.clone(),
            })
        })
        .collect()
}

pub(super) fn openai_tool_choice(
    choice: Option<&Value>,
    tools: &[crate::browser_inference::BrowserTool],
) -> Result<crate::browser_inference::BrowserToolChoice, String> {
    use crate::browser_inference::BrowserToolChoice;
    let Some(choice) = choice else {
        return Ok(if tools.is_empty() {
            BrowserToolChoice::None
        } else {
            BrowserToolChoice::Auto
        });
    };
    if let Some(choice) = choice.as_str() {
        return match choice {
            "auto" => Ok(BrowserToolChoice::Auto),
            "none" => Ok(BrowserToolChoice::None),
            "required" => Ok(BrowserToolChoice::Required),
            other => Err(format!("Unbekannter tool_choice '{other}'.")),
        };
    }
    let name = choice
        .get("function")
        .and_then(|function| function.get("name"))
        .and_then(Value::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| "Objekt-tool_choice benoetigt function.name.".to_string())?;
    if !tools.iter().any(|tool| tool.name == name) {
        return Err(format!(
            "tool_choice verweist auf unbekanntes Tool '{name}'."
        ));
    }
    Ok(BrowserToolChoice::Function(name.to_string()))
}

pub(super) fn text_content(value: &Value) -> Result<String, String> {
    if let Some(text) = value.as_str() {
        return Ok(text.to_string());
    }
    let Some(parts) = value.as_array() else {
        return Err("content muss ein Text oder ein Array aus Textbloecken sein.".to_string());
    };
    let mut out = String::new();
    for part in parts {
        let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
        if part_type != "text" {
            return Err(format!(
                "Inhaltstyp '{part_type}' wird von der ersten Bridge-Scheibe nicht unterstuetzt."
            ));
        }
        let text = part
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| "Textblock ohne String-Feld 'text'.".to_string())?;
        out.push_str(text);
    }
    Ok(out)
}
