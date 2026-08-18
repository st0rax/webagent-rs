//! Lokale Provider-Bridge fuer Pi-kompatible OpenAI- und Anthropic-Anfragen.
//!
//! Der Dienst bindet ausschliesslich an Loopback, verlangt einen Bearer- oder
//! Anthropic-kompatiblen x-api-key-Token und akzeptiert in der ersten Scheibe
//! ausschliesslich textuelle Gespraechsinhalte. Der bewusst synchrone HTTP-Kern
//! serialisiert Browserruns und vermeidet eine zweite parallele Laufzeit neben
//! dem bestehenden WebAgent-Controller.

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_REQUEST_BYTES: usize = 1_048_576;
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Laufzeitkonfiguration des lokalen Dienstes.
///
/// `api_key` wird ausschliesslich aus einer Umgebungsvariable geladen und darf
/// weder geloggt noch in Statusantworten ausgegeben werden.
#[derive(Clone)]
pub struct BridgeConfig {
    pub bind: SocketAddr,
    pub brain: String,
    pub max_cycles: u32,
    pub headless: bool,
    pub api_key: String,
}

/// Startet den lokalen Dienst und blockiert, bis der Prozess beendet wird.
///
/// Der Dienst verarbeitet genau eine Anfrage zur Zeit. Das ist beabsichtigt:
/// Ein Browserprofil darf nicht gleichzeitig von mehreren Controller-Runs
/// gesteuert werden.
pub fn serve(config: BridgeConfig) -> Result<(), String> {
    if !config.bind.ip().is_loopback() {
        return Err("API-Bridge darf nur an eine Loopback-Adresse binden.".to_string());
    }
    let listener = TcpListener::bind(config.bind)
        .map_err(|error| format!("API-Bridge kann {} nicht binden: {error}", config.bind))?;
    eprintln!(
        "[api] lokale Provider-Bridge auf http://{}/v1 (Token erforderlich)",
        config.bind
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                if let Err(error) = handle_connection(&mut stream, &config) {
                    eprintln!("[api] Anfrage verworfen: {error}");
                }
            }
            Err(error) => eprintln!("[api] Verbindungsfehler: {error}"),
        }
    }
    Ok(())
}

fn handle_connection(stream: &mut TcpStream, config: &BridgeConfig) -> Result<(), String> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Lese-Timeout nicht setzbar: {error}"))?;
    stream
        .set_write_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Schreib-Timeout nicht setzbar: {error}"))?;

    let request = match read_http_request(stream) {
        Ok(request) => request,
        Err(error) => {
            write_http_response(
                stream,
                api_error(ApiFlavor::OpenAi, 400, &format!("Ungueltige HTTP-Anfrage: {error}")),
            )?;
            return Ok(());
        }
    };

    let flavor = if request.path == "/v1/messages" {
        ApiFlavor::Anthropic
    } else {
        ApiFlavor::OpenAi
    };
    let response = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/health") => HttpResponse::json(200, json!({
            "status": "ok",
            "service": "webagent-provider-bridge"
        })),
        ("GET", "/v1/models") => {
            if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                response
            } else {
                let model = model_id(&config.brain);
                HttpResponse::json(200, json!({
                    "object": "list",
                    "data": [{"id": model, "object": "model", "owned_by": "webagent"}]
                }))
            }
        }
        ("POST", "/v1/chat/completions") => handle_openai(&request, config),
        ("POST", "/v1/messages") => handle_anthropic(&request, config),
        _ => api_error(flavor, 404, "Endpoint nicht gefunden."),
    };
    write_http_response(stream, response)
}

fn handle_openai(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let payload: OpenAiRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    if let Err(error) = validate_model(&payload.model, &config.brain) {
        return api_error(ApiFlavor::OpenAi, 400, &error);
    }
    let task = match openai_task(&payload) {
        Ok(task) => task,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let answer = match run_task_blocking(config, &task) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("chatcmpl");
    if payload.stream.unwrap_or(false) {
        return openai_sse(&id, &payload.model, &answer);
    }
    HttpResponse::json(200, json!({
        "id": id,
        "object": "chat.completion",
        "created": unix_seconds(),
        "model": payload.model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": answer},
            "finish_reason": "stop"
        }],
        "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
    }))
}

fn handle_anthropic(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::Anthropic) {
        return response;
    }
    let payload: AnthropicRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    if payload.max_tokens == 0 {
        return api_error(ApiFlavor::Anthropic, 400, "max_tokens muss groesser als 0 sein.");
    }
    if let Err(error) = validate_model(&payload.model, &config.brain) {
        return api_error(ApiFlavor::Anthropic, 400, &error);
    }
    let task = match anthropic_task(&payload) {
        Ok(task) => task,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let answer = match run_task_blocking(config, &task) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::Anthropic, 502, &error),
    };
    let id = completion_id("msg");
    if payload.stream.unwrap_or(false) {
        return anthropic_sse(&id, &payload.model, &answer);
    }
    HttpResponse::json(200, json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": payload.model,
        "content": [{"type": "text", "text": answer}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {"input_tokens": 0, "output_tokens": 0}
    }))
}

fn run_task_blocking(config: &BridgeConfig, task: &str) -> Result<String, String> {
    use crate::browser::WebBrainBackend;
    use crate::controller::AgentController;
    use crate::executor::PlatformShellExecutor;

    let backend = WebBrainBackend::from_config(&config.brain)
        .map_err(|error| format!("Brain '{}' nicht verfuegbar: {error}", config.brain))?;
    let workspace = std::env::current_dir()
        .map_err(|error| format!("API-Workspace nicht bestimmbar: {error}"))?;
    let executor = PlatformShellExecutor::new_in(&workspace);
    let mut controller = AgentController::new(backend, executor, config.max_cycles as usize);
    controller.set_fresh_chat(true);
    let meta = controller
        .run(task, &config.brain, None, config.headless)
        .map_err(|error| format!("WebAgent-Run fehlgeschlagen: {error}"))?;

    final_result_text(&meta).ok_or_else(|| {
        format!(
            "WebAgent-Run {} endete ohne kanonische Abschlussantwort (status={}).",
            meta.run_id, meta.status
        )
    })
}

/// Liest ausschliesslich strukturierte Abschlussaktionen, nie Browser-Transcript.
fn final_result_text(meta: &crate::run_store::RunMeta) -> Option<String> {
    let priorities = ["answer-", "finish-", "final-", "review-", "eval-"];
    for prefix in priorities {
        let result = meta
            .completed_actions
            .iter()
            .filter(|(id, value)| id.starts_with(prefix) && !value.trim().is_empty())
            .min_by(|(left, _), (right, _)| left.cmp(right))
            .map(|(_, value)| value.trim().to_string());
        if result.is_some() {
            return result;
        }
    }
    None
}

#[derive(Deserialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<ConversationMessage>,
    #[serde(default)]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ConversationMessage>,
    #[serde(default)]
    system: Option<Value>,
    #[serde(default)]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct ConversationMessage {
    role: String,
    #[serde(default)]
    content: Value,
}

fn decode_json<T: DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|error| format!("Ungueltiger JSON-Body: {error}"))
}

fn openai_task(request: &OpenAiRequest) -> Result<String, String> {
    conversation_task(None, &request.messages, "OpenAI Chat Completions")
}

fn anthropic_task(request: &AnthropicRequest) -> Result<String, String> {
    let system = match &request.system {
        Some(content) => Some(text_content(content)?),
        None => None,
    };
    conversation_task(system, &request.messages, "Anthropic Messages")
}

fn conversation_task(
    system: Option<String>,
    messages: &[ConversationMessage],
    source: &str,
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("messages darf nicht leer sein.".to_string());
    }
    let mut task = format!(
        "Du bearbeitest eine textuelle Anfrage, die ueber die {source}-Provider-Bridge eingegangen ist.\n\n"
    );
    if let Some(system) = system.filter(|value| !value.trim().is_empty()) {
        task.push_str("[system]\n");
        task.push_str(&system);
        task.push_str("\n\n");
    }
    for message in messages {
        match message.role.as_str() {
            "system" | "developer" | "user" | "assistant" => {}
            other => return Err(format!("Rolle '{other}' wird von der Text-Bridge nicht unterstuetzt.")),
        }
        let content = text_content(&message.content)?;
        task.push_str(&format!("[{}]\n{}\n\n", message.role, content));
    }
    task.push_str(
        "Antworte auf die letzte Nutzerfrage. Schliesse zwingend mit einer strukturierten webagent/1 MESSAGE-Aktion ab: id muss mit 'final-' beginnen, type ist 'message', und text enthaelt ausschliesslich die Antwort fuer den API-Client. Fuehre keine Tools oder Shell-Befehle aus, ausser die Anfrage erfordert sie explizit.",
    );
    Ok(task)
}

fn text_content(value: &Value) -> Result<String, String> {
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

fn validate_model(requested: &str, brain: &str) -> Result<(), String> {
    let expected = model_id(brain);
    if requested == expected || requested == "webagent" {
        Ok(())
    } else {
        Err(format!(
            "Unbekanntes Modell '{requested}'. Dieser Server bietet nur '{expected}' an."
        ))
    }
}

fn model_id(brain: &str) -> String {
    format!("webagent/{brain}")
}

#[derive(Clone, Copy)]
enum ApiFlavor {
    OpenAi,
    Anthropic,
}

fn authorize(
    headers: &BTreeMap<String, String>,
    config: &BridgeConfig,
    flavor: ApiFlavor,
) -> Result<(), HttpResponse> {
    let bearer = headers
        .get("authorization")
        .and_then(|value| value.strip_prefix("Bearer "));
    let provided = bearer.or_else(|| headers.get("x-api-key").map(String::as_str));
    if provided.is_some_and(|token| constant_time_equal(token, &config.api_key)) {
        return Ok(());
    }
    Err(api_error(flavor, 401, "Ungueltiger oder fehlender API-Token."))
}

fn constant_time_equal(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.as_bytes().iter().zip(right.as_bytes()) {
        difference |= a ^ b;
    }
    difference == 0
}

fn api_error(flavor: ApiFlavor, status: u16, message: &str) -> HttpResponse {
    let body = match flavor {
        ApiFlavor::OpenAi => json!({
            "error": {
                "message": message,
                "type": "invalid_request_error",
                "param": null,
                "code": null
            }
        }),
        ApiFlavor::Anthropic => json!({
            "type": "error",
            "error": {"type": "invalid_request_error", "message": message}
        }),
    };
    HttpResponse::json(status, body)
}

fn openai_sse(id: &str, model: &str, answer: &str) -> HttpResponse {
    let first = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": {"role": "assistant", "content": answer}, "finish_reason": null}]
    });
    let last = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
    });
    HttpResponse::sse(format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n"))
}

fn anthropic_sse(id: &str, model: &str, answer: &str) -> HttpResponse {
    let started = json!({
        "type": "message_start",
        "message": {
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": null,
            "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
        }
    });
    let block_start = json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}});
    let delta = json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": answer}});
    let block_stop = json!({"type": "content_block_stop", "index": 0});
    let message_delta = json!({"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 0}});
    let message_stop = json!({"type": "message_stop"});
    HttpResponse::sse(format!(
        "event: message_start\ndata: {started}\n\n\
         event: content_block_start\ndata: {block_start}\n\n\
         event: content_block_delta\ndata: {delta}\n\n\
         event: content_block_stop\ndata: {block_stop}\n\n\
         event: message_delta\ndata: {message_delta}\n\n\
         event: message_stop\ndata: {message_stop}\n\n"
    ))
}

struct HttpRequest {
    method: String,
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, value: Value) -> Self {
        let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{\"error\":\"serialization\"}".to_vec());
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
        }
    }

    fn sse(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream; charset=utf-8",
            body: body.into_bytes(),
        }
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end;
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("HTTP-Header nicht lesbar: {error}"))?;
        if count == 0 {
            return Err("Verbindung vor vollstaendigem HTTP-Header geschlossen.".to_string());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("HTTP-Header ist zu gross.".to_string());
        }
        if let Some(index) = find_bytes(&bytes, b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }

    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| "HTTP-Header ist nicht UTF-8/ASCII.".to_string())?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP-Request-Line fehlt.".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "HTTP-Methode fehlt.".to_string())?
        .to_string();
    let path = parts
        .next()
        .ok_or_else(|| "HTTP-Pfad fehlt.".to_string())?
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err("Ungueltige HTTP-Request-Line.".to_string());
    }

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "Ungueltiger HTTP-Header.".to_string())?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return Err("Chunked HTTP-Anfragen werden nicht unterstuetzt.".to_string());
    }
    let content_length = headers
        .get("content-length")
        .map(|value| value.parse::<usize>().map_err(|_| "Ungueltige Content-Length.".to_string()))
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BYTES {
        return Err("HTTP-Body ist zu gross.".to_string());
    }
    while bytes.len() - header_end < content_length {
        let remaining = content_length - (bytes.len() - header_end);
        let read_len = remaining.min(buffer.len());
        let count = stream
            .read(&mut buffer[..read_len])
            .map_err(|error| format!("HTTP-Body nicht lesbar: {error}"))?;
        if count == 0 {
            return Err("Verbindung vor vollstaendigem HTTP-Body geschlossen.".to_string());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(HttpRequest {
        method,
        path,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn write_http_response(stream: &mut TcpStream, response: HttpResponse) -> Result<(), String> {
    let bytes = render_http_response(&response);
    stream
        .write_all(&bytes)
        .map_err(|error| format!("HTTP-Antwort nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("HTTP-Antwort nicht abschliessbar: {error}"))
}

fn render_http_response(response: &HttpResponse) -> Vec<u8> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        502 => "Bad Gateway",
        _ => "Internal Server Error",
    };
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    );
    let mut bytes = headers.into_bytes();
    bytes.extend_from_slice(&response.body);
    bytes
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn completion_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{prefix}-{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_and_block_text() {
        assert_eq!(text_content(&json!("Hallo")).unwrap(), "Hallo");
        assert_eq!(
            text_content(&json!([
                {"type": "text", "text": "Hal"},
                {"type": "text", "text": "lo"}
            ]))
            .unwrap(),
            "Hallo"
        );
    }

    #[test]
    fn rejects_non_text_content() {
        let error = text_content(&json!([{"type": "image", "source": {}}])).unwrap_err();
        assert!(error.contains("nicht unterstuetzt"));
    }

    #[test]
    fn model_validation_accepts_only_bridge_aliases() {
        assert!(validate_model("webagent", "chatgpt").is_ok());
        assert!(validate_model("webagent/chatgpt", "chatgpt").is_ok());
        assert!(validate_model("gpt-5", "chatgpt").is_err());
    }

    #[test]
    fn rejects_unsupported_roles_before_agent_execution() {
        let request = OpenAiRequest {
            model: "webagent".to_string(),
            stream: None,
            messages: vec![ConversationMessage {
                role: "tool".to_string(),
                content: json!("unsafe"),
            }],
        };
        assert!(openai_task(&request).unwrap_err().contains("Rolle 'tool'"));
    }

    #[test]
    fn timing_safe_comparison_requires_equal_content() {
        assert!(constant_time_equal("same", "same"));
        assert!(!constant_time_equal("same", "diff"));
        assert!(!constant_time_equal("short", "longer"));
    }

    #[test]
    fn response_renderer_sets_protocol_and_length() {
        let rendered = render_http_response(&HttpResponse::json(200, json!({"ok": true})));
        let text = String::from_utf8(rendered).unwrap();
        assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(text.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(text.ends_with("{\"ok\":true}"));
    }

    #[test]
    fn api_error_uses_provider_specific_shapes() {
        let openai = String::from_utf8(api_error(ApiFlavor::OpenAi, 401, "x").body).unwrap();
        let anthropic = String::from_utf8(api_error(ApiFlavor::Anthropic, 401, "x").body).unwrap();
        assert!(openai.contains("\"error\":{"));
        assert!(anthropic.contains("\"type\":\"error\""));
    }
}
