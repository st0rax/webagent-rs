//! Lokale Provider-Bridge fuer Pi-kompatible OpenAI- und Anthropic-Anfragen.
//!
//! Der Dienst bindet ausschliesslich an Loopback, verlangt einen Bearer- oder
//! Anthropic-kompatiblen x-api-key-Token und akzeptiert textuelle
//! Gespraechsinhalte sowie OpenAI-Function-Tools. Der synchrone HTTP-Kern
//! serialisiert Browserruns. Jeder Provideraufruf fuehrt genau einen
//! harnessfreien Browser-Inference-Turn aus; `AgentController`, `webagent/1`
//! und lokale Werkzeuge bleiben ausserhalb dieser Schicht.

use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_REQUEST_BYTES: usize = 1_048_576;
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_CONNECTIONS: usize = 8;

static BROWSER_RUN_LOCKS: OnceLock<Mutex<BTreeMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Default)]
struct ConnectionLimiter {
    active: AtomicUsize,
}

impl ConnectionLimiter {
    fn try_acquire(self: &Arc<Self>) -> Option<ConnectionPermit> {
        let mut active = self.active.load(Ordering::Acquire);
        loop {
            if active >= MAX_CONCURRENT_CONNECTIONS {
                return None;
            }
            match self.active.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(ConnectionPermit(Arc::clone(self))),
                Err(current) => active = current,
            }
        }
    }
}

struct ConnectionPermit(Arc<ConnectionLimiter>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Release);
    }
}

/// Laufzeitkonfiguration des lokalen Dienstes.
///
/// `api_key` wird ausschliesslich aus einer Umgebungsvariable geladen und darf
/// weder geloggt noch in Statusantworten ausgegeben werden.
#[derive(Clone)]
pub struct BridgeConfig {
    pub bind: SocketAddr,
    pub brain: String,
    pub timeout_secs: Option<f64>,
    pub headless: bool,
    pub api_key: String,
}

/// Startet den lokalen Dienst und blockiert, bis der Prozess beendet wird.
///
/// Der Dienst verarbeitet pro Brain genau einen Browserturn zur Zeit. Das ist
/// beabsichtigt: Ein Browserprofil darf nicht gleichzeitig von mehreren
/// Inference-Anfragen gesteuert werden; unterschiedliche Brains blockieren
/// sich dagegen nicht gegenseitig.
pub fn serve(config: BridgeConfig) -> Result<(), String> {
    if config.timeout_secs.is_some_and(|timeout| timeout <= 0.0) {
        return Err("--timeout-secs muss groesser als 0 sein.".to_string());
    }

    if !config.bind.ip().is_loopback() {
        return Err("API-Bridge darf nur an eine Loopback-Adresse binden.".to_string());
    }

    resolve_model(&model_id(&config.brain), &config.brain)?;

    let listener = TcpListener::bind(config.bind)
        .map_err(|error| format!("API-Bridge nicht bindbar: {error}"))?;
    eprintln!("[api] Bridge aktiv auf http://{}", config.bind);

    let config = Arc::new(config);
    let limiter = Arc::new(ConnectionLimiter::default());
    for stream in listener.incoming() {
        let mut stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("[api] Verbindung nicht annehmbar: {error}");
                continue;
            }
        };

        let Some(permit) = limiter.try_acquire() else {
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            if let Err(error) = write_http_response(&mut stream, overload_response()) {
                eprintln!("[api] Ueberlastungsantwort nicht schreibbar: {error}");
            }
            continue;
        };

        let config = Arc::clone(&config);
        thread::spawn(move || {
            let _permit = permit;
            if let Err(error) = handle_connection(&mut stream, &config) {
                eprintln!("[api] Anfrage verworfen: {error}");
            }
        });
    }

    Ok(())
}

fn overload_response() -> HttpResponse {
    HttpResponse::json(
        503,
        json!({
            "error": {
                "message": "API-Bridge ist ausgelastet; bitte Anfrage wiederholen.",
                "type": "server_error",
                "code": "overloaded"
            }
        }),
    )
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
                api_error(
                    ApiFlavor::OpenAi,
                    400,
                    &format!("Ungueltige HTTP-Anfrage: {error}"),
                ),
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
        ("GET", "/health") => HttpResponse::json(
            200,
            json!({
                "status": "ok",
                "service": "webagent-provider-bridge"
            }),
        ),
        ("GET", "/v1/models") => {
            if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                response
            } else {
                let models: Vec<Value> = available_brains()
                    .into_iter()
                    .map(|brain| {
                        json!({
                            "id": model_id(&brain),
                            "object": "model",
                            "owned_by": "webagent",
                            "brain": brain,
                            "context_window": 128000,
                            "max_tokens": 16384
                        })
                    })
                    .collect();
                HttpResponse::json(
                    200,
                    json!({
                        "object": "list",
                        "data": models
                    }),
                )
            }
        }
        ("GET", path) if path.starts_with("/v1/models/") => {
            if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                response
            } else {
                let requested = path.trim_start_matches("/v1/models/");
                match resolve_model(requested, &config.brain) {
                    Ok(brain) => HttpResponse::json(
                        200,
                        json!({
                            "id": model_id(&brain),
                            "object": "model",
                            "created": unix_seconds(),
                            "owned_by": "webagent",
                            "brain": brain,
                            "context_window": 128000,
                            "max_tokens": 16384
                        }),
                    ),
                    Err(error) => api_error(ApiFlavor::OpenAi, 404, &error),
                }
            }
        }
        ("POST", "/v1/chat/completions") => handle_openai(&request, config),
        ("POST", "/v1/responses") => handle_responses(&request, config),
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
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let task = match openai_task(&payload) {
        Ok(task) => task,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tools = match openai_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tool_choice = match openai_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let answer = match run_task_blocking(config, &brain, &task, &tools, tool_choice) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("chatcmpl");
    if payload.stream.unwrap_or(false) {
        return openai_sse(&id, &payload.model, &answer);
    }
    let message = openai_message(&answer);
    HttpResponse::json(
        200,
        json!({
            "id": id,
            "object": "chat.completion",
            "created": unix_seconds(),
            "model": payload.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": answer.finish_reason()
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
        }),
    )
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
        return api_error(
            ApiFlavor::Anthropic,
            400,
            "max_tokens muss groesser als 0 sein.",
        );
    }
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let task = match anthropic_task(&payload) {
        Ok(task) => task,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        &task,
        &[],
        crate::browser_inference::BrowserToolChoice::None,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::Anthropic, 502, &error),
    };
    let Some(text) = answer.text else {
        return api_error(
            ApiFlavor::Anthropic,
            502,
            "Anthropic-Textpfad erhielt unerwartet einen Tool Call.",
        );
    };
    let id = completion_id("msg");
    if payload.stream.unwrap_or(false) {
        return anthropic_sse(&id, &payload.model, &text);
    }
    HttpResponse::json(
        200,
        json!({
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": payload.model,
            "content": [{"type": "text", "text": text}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
        }),
    )
}

fn handle_responses(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let payload: ResponsesRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tools = match responses_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tool_choice = match responses_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let task = match responses_task(&payload) {
        Ok(task) => task,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let answer = match run_task_blocking(config, &brain, &task, &tools, tool_choice) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("resp");
    if payload.stream.unwrap_or(false) {
        return responses_sse(&id, &payload.model, &answer);
    }
    HttpResponse::json(
        200,
        response_object_from_answer(&id, &payload.model, &answer),
    )
}

fn run_task_blocking(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    tools: &[crate::browser_inference::BrowserTool],
    tool_choice: crate::browser_inference::BrowserToolChoice,
) -> Result<crate::browser_inference::BrowserInferenceResponse, String> {
    let lock = BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    crate::browser_inference::complete(crate::browser_inference::BrowserInferenceRequest {
        brain,
        prompt: task,
        tools,
        tool_choice,
        headless: config.headless,
        timeout_secs: config.timeout_secs,
        model: None,
    })
    .map_err(|error| format!("Browser-Inference fehlgeschlagen: {error}"))
}

#[derive(Deserialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<ConversationMessage>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Vec<OpenAiTool>,
    #[serde(default)]
    tool_choice: Option<Value>,
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
struct ResponsesRequest {
    model: String,
    input: Value,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    tool_choice: Option<Value>,
}

#[derive(Deserialize)]
struct ConversationMessage {
    role: String,
    #[serde(default)]
    content: Value,
    #[serde(default)]
    tool_calls: Vec<OpenAiAssistantToolCall>,
    #[serde(default)]
    tool_call_id: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiFunction,
}

#[derive(Deserialize)]
struct OpenAiFunction {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Value,
}

#[derive(Deserialize, serde::Serialize)]
struct OpenAiAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiAssistantFunction,
}

#[derive(Deserialize, serde::Serialize)]
struct OpenAiAssistantFunction {
    name: String,
    arguments: String,
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

fn responses_task(request: &ResponsesRequest) -> Result<String, String> {
    let messages = match &request.input {
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
                        content: if output.is_string() {
                            output
                        } else {
                            Value::String(output.to_string())
                        },
                        tool_calls: Vec::new(),
                        tool_call_id: Some(id.to_string()),
                    });
                }
                let role = object
                    .get("role")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Responses-Input-Item benoetigt role.".to_string())?;
                let content = object.get("content").cloned().unwrap_or(Value::Null);
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
    conversation_task(request.instructions.clone(), &messages, "OpenAI Responses")
}

fn responses_tools(tools: &[Value]) -> Result<Vec<crate::browser_inference::BrowserTool>, String> {
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

fn responses_tool_choice(
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

fn conversation_task(
    system: Option<String>,
    messages: &[ConversationMessage],
    source: &str,
) -> Result<String, String> {
    if messages.is_empty() {
        return Err("messages darf nicht leer sein.".to_string());
    }
    let mut task = format!(
        "Die folgende Unterhaltung wurde ueber die {source}-Provider-Bridge uebermittelt. Antworte auf die letzte Nutzerfrage. Gib nur die eigentliche Antwort fuer den API-Client aus; verwende nicht das lokale WEBAGENT/1-Agent-Aktionsprotokoll. Ein eventuell spaeter angefuegter WEBAGENT_INFERENCE/1-Tool-Umschlag ist davon getrennt.\n\n"
    );
    if let Some(system) = system.filter(|value| !value.trim().is_empty()) {
        task.push_str("[system]\n");
        task.push_str(&system);
        task.push_str("\n\n");
    }
    for message in messages {
        match message.role.as_str() {
            "system" | "developer" | "user" => {
                if !message.tool_calls.is_empty() || message.tool_call_id.is_some() {
                    return Err(format!(
                        "Rolle '{}' darf keine Tool-Call-Felder enthalten.",
                        message.role
                    ));
                }
                let content = text_content(&message.content)?;
                task.push_str(&format!("[{}]\n{}\n\n", message.role, content));
            }
            "assistant" => {
                if message.tool_call_id.is_some() {
                    return Err("Assistant-Nachricht darf keine tool_call_id tragen.".to_string());
                }
                if !message.content.is_null() {
                    let content = text_content(&message.content)?;
                    if !content.is_empty() {
                        task.push_str(&format!("[assistant]\n{content}\n\n"));
                    }
                }
                if !message.tool_calls.is_empty() {
                    let calls = serde_json::to_string(&message.tool_calls).map_err(|error| {
                        format!("Assistant-Tool-Calls nicht serialisierbar: {error}")
                    })?;
                    task.push_str(&format!("[assistant tool_calls]\n{calls}\n\n"));
                }
            }
            "tool" => {
                if !message.tool_calls.is_empty() {
                    return Err("Tool-Nachricht darf keine weiteren tool_calls tragen.".to_string());
                }
                let id = message
                    .tool_call_id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .ok_or_else(|| "Tool-Nachricht benoetigt tool_call_id.".to_string())?;
                let content = text_content(&message.content)?;
                task.push_str(&format!("[tool id={id}]\n{content}\n\n"));
            }
            other => {
                return Err(format!(
                    "Rolle '{other}' wird von der Text-Bridge nicht unterstuetzt."
                ))
            }
        }
    }
    Ok(task)
}

fn openai_tools(
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

fn openai_tool_choice(
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
    Ok(BrowserToolChoice::Function(name.to_string()))
}

fn openai_message(answer: &crate::browser_inference::BrowserInferenceResponse) -> Value {
    if answer.tool_calls.is_empty() {
        return json!({"role": "assistant", "content": answer.text});
    }
    let tool_calls: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
                }
            })
        })
        .collect();
    json!({"role": "assistant", "content": null, "tool_calls": tool_calls})
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

fn available_brains() -> Vec<String> {
    let mut brains: Vec<String> = crate::config::brains().into_keys().collect();
    brains.sort();
    brains
}

fn resolve_model(requested: &str, default_brain: &str) -> Result<String, String> {
    let brain = if requested == "webagent" {
        default_brain
    } else {
        requested
            .strip_prefix("webagent/")
            .ok_or_else(|| format!("Ungueltige WebAgent-Modell-ID '{requested}'."))?
    };
    if available_brains()
        .iter()
        .any(|candidate| candidate == brain)
    {
        return Ok(brain.to_string());
    }
    Err(format!(
        "Unbekanntes Modell '{requested}'. Verfuegbar: {}.",
        available_brains()
            .iter()
            .map(|brain| model_id(brain))
            .collect::<Vec<_>>()
            .join(", ")
    ))
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
    Err(api_error(
        flavor,
        401,
        "Ungueltiger oder fehlender API-Token.",
    ))
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

fn openai_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let delta = if answer.tool_calls.is_empty() {
        json!({"role": "assistant", "content": answer.text})
    } else {
        let tool_calls: Vec<Value> = answer
            .tool_calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                json!({
                    "index": index,
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
                    }
                })
            })
            .collect();
        json!({"role": "assistant", "tool_calls": tool_calls})
    };
    let first = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": null}]
    });
    let last = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": answer.finish_reason()}]
    });
    HttpResponse::sse(format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n"))
}

fn response_object(id: &str, model: &str, text: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "created_at": unix_seconds(),
        "status": "completed",
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "model": model,
        "output": [{
            "id": format!("{id}_msg"),
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{"type": "output_text", "text": text, "annotations": []}]
        }],
        "output_text": text,
        "parallel_tool_calls": false,
        "tool_choice": "none",
        "tools": [],
        "usage": null,
        "metadata": {}
    })
}

fn response_object_from_answer(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> Value {
    if answer.tool_calls.is_empty() {
        return response_object(id, model, answer.text.as_deref().unwrap_or_default());
    }
    let output: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function_call",
                "status": "completed",
                "call_id": call.id,
                "name": call.name,
                "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
            })
        })
        .collect();
    json!({
        "id": id,
        "object": "response",
        "created_at": unix_seconds(),
        "status": "completed",
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "model": model,
        "output": output,
        "output_text": "",
        "parallel_tool_calls": false,
        "tool_choice": "auto",
        "tools": [],
        "usage": null,
        "metadata": {}
    })
}

fn responses_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let text = answer.text.as_deref().unwrap_or_default();
    let created = response_object_from_answer(id, model, answer);
    let mut body = format!(
        "event: response.created\ndata: {}\n\n",
        json!({"type":"response.created","response":created})
    );
    body.push_str(&format!(
        "event: response.in_progress\ndata: {}\n\n",
        json!({"type":"response.in_progress","response":created})
    ));
    body.push_str(&format!(
        "event: response.output_text.delta\ndata: {}\n\n",
        json!({"type":"response.output_text.delta","item_id":format!("{id}_msg"),"output_index":0,"content_index":0,"delta":text})
    ));
    body.push_str(&format!(
        "event: response.output_text.done\ndata: {}\n\n",
        json!({"type":"response.output_text.done","item_id":format!("{id}_msg"),"output_index":0,"content_index":0,"text":text})
    ));
    body.push_str(&format!(
        "event: response.completed\ndata: {}\n\n",
        json!({"type":"response.completed","response":response_object_from_answer(id, model, answer)})
    ));
    HttpResponse::sse(body)
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
        let body = serde_json::to_vec(&value)
            .unwrap_or_else(|_| b"{\"error\":\"serialization\"}".to_vec());
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
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "Ungueltige Content-Length.".to_string())
        })
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
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
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
        503 => "Service Unavailable",
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
    fn model_resolution_routes_each_available_brain() {
        assert_eq!(resolve_model("webagent", "chatgpt").unwrap(), "chatgpt");
        assert_eq!(
            resolve_model("webagent/claude", "chatgpt").unwrap(),
            "claude"
        );
        assert!(resolve_model("gpt-5", "chatgpt").is_err());
        assert!(resolve_model("webagent/not-configured", "chatgpt").is_err());
    }

    #[test]
    fn model_catalog_contains_all_builtin_brains() {
        let brains = available_brains();
        for expected in [
            "chatgpt", "claude", "deepseek", "gemini", "kimi", "mistral", "qwen", "zai",
        ] {
            assert!(brains.contains(&expected.to_string()), "{expected} fehlt");
        }
    }

    #[test]
    fn rejects_unsupported_roles_before_browser_execution() {
        let request = OpenAiRequest {
            model: "webagent".to_string(),
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            messages: vec![ConversationMessage {
                role: "invalid".to_string(),
                content: json!("unsafe"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
        };
        assert!(openai_task(&request)
            .unwrap_err()
            .contains("Rolle 'invalid'"));
    }

    #[test]
    fn plain_inference_prompt_does_not_request_webagent_actions() {
        let request = OpenAiRequest {
            model: "webagent".to_string(),
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            messages: vec![ConversationMessage {
                role: "user".to_string(),
                content: json!("Hallo"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
        };

        let prompt = openai_task(&request).unwrap();
        assert!(prompt.contains("[user]\nHallo"));
        assert!(prompt.contains("verwende nicht das lokale WEBAGENT/1"));
        assert!(!prompt.contains("final-"));
    }

    #[test]
    fn openai_tools_and_choice_are_normalized() {
        let request = OpenAiRequest {
            model: "webagent".to_string(),
            stream: None,
            tools: vec![OpenAiTool {
                kind: "function".to_string(),
                function: OpenAiFunction {
                    name: "read_file".to_string(),
                    description: Some("Datei lesen".to_string()),
                    parameters: json!({"type": "object"}),
                },
            }],
            tool_choice: Some(json!("required")),
            messages: vec![ConversationMessage {
                role: "user".to_string(),
                content: json!("Lies eine Datei"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
        };

        let tools = openai_tools(&request.tools).unwrap();
        let choice = openai_tool_choice(request.tool_choice.as_ref(), &tools).unwrap();
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(
            choice,
            crate::browser_inference::BrowserToolChoice::Required
        );
    }

    #[test]
    fn tool_result_is_preserved_in_browser_prompt() {
        let request = OpenAiRequest {
            model: "webagent".to_string(),
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            messages: vec![ConversationMessage {
                role: "tool".to_string(),
                content: json!("Dateiinhalt"),
                tool_calls: Vec::new(),
                tool_call_id: Some("call_1".to_string()),
            }],
        };

        let prompt = openai_task(&request).unwrap();
        assert!(prompt.contains("[tool id=call_1]\nDateiinhalt"));
    }

    #[test]
    fn browser_tool_call_maps_to_openai_response() {
        let answer = crate::browser_inference::BrowserInferenceResponse {
            text: None,
            tool_calls: vec![crate::browser_inference::BrowserToolCall {
                id: "call_1".to_string(),
                name: "read_file".to_string(),
                arguments: json!({"path": "README.md"}),
            }],
        };

        let message = openai_message(&answer);
        assert!(message["content"].is_null());
        assert_eq!(message["tool_calls"][0]["id"], "call_1");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "read_file");
        assert_eq!(answer.finish_reason(), "tool_calls");
    }

    #[test]
    fn responses_task_accepts_string_and_message_input() {
        let string_request = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!("Hallo"),
            instructions: Some("Antworte kurz.".to_string()),
            stream: Some(false),
            tools: Vec::new(),
            tool_choice: None,
        };
        let string_task = responses_task(&string_request).unwrap();
        assert!(string_task.contains("[system]\nAntworte kurz."));
        assert!(string_task.contains("[user]\nHallo"));

        let message_request = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!([{"role":"user","content":"Ping"}]),
            instructions: None,
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
        };
        assert!(responses_task(&message_request)
            .unwrap()
            .contains("[user]\nPing"));
    }

    #[test]
    fn responses_renderer_emits_completion_contract() {
        let response = response_object("resp_test", "webagent/chatgpt", "OK");
        assert_eq!(response["object"], "response");
        assert_eq!(response["status"], "completed");
        assert_eq!(response["output"][0]["type"], "message");
        assert_eq!(response["output"][0]["content"][0]["text"], "OK");

        let answer = crate::browser_inference::BrowserInferenceResponse {
            text: Some("OK".to_string()),
            tool_calls: Vec::new(),
        };
        let sse = String::from_utf8(responses_sse("resp_test", "webagent/chatgpt", &answer).body)
            .unwrap();
        assert!(sse.contains("response.created"));
        assert!(sse.contains("response.output_text.delta"));
        assert!(sse.contains("response.completed"));
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

    #[test]
    fn accepts_x_api_key_header() {
        let config = BridgeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            brain: "chatgpt".to_string(),
            timeout_secs: None,
            headless: true,
            api_key: "test-secret".to_string(),
        };
        let headers = BTreeMap::from([("x-api-key".to_string(), "test-secret".to_string())]);

        assert!(authorize(&headers, &config, ApiFlavor::OpenAi).is_ok());
    }

    #[test]
    fn rejects_non_positive_timeout_before_binding() {
        let error = serve(BridgeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            brain: "chatgpt".to_string(),
            timeout_secs: Some(0.0),
            headless: true,
            api_key: "test-secret".to_string(),
        })
        .unwrap_err();

        assert!(error.contains("--timeout-secs muss groesser als 0 sein"));
    }

    #[test]
    fn connection_limiter_rejects_excess_and_recovers_after_release() {
        let limiter = Arc::new(ConnectionLimiter::default());
        let mut permits: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
            .map(|_| limiter.try_acquire().expect("Kapazitaet verfuegbar"))
            .collect();

        assert!(limiter.try_acquire().is_none());
        drop(permits.pop());
        assert!(limiter.try_acquire().is_some());
    }
}
