//! Lokale Provider-Bridge fuer Pi-kompatible OpenAI- und Anthropic-Anfragen.
//!
//! Der Dienst bindet ausschliesslich an Loopback, verlangt einen Bearer- oder
//! Anthropic-kompatiblen x-api-key-Token und akzeptiert Text-, Bild- und
//! Audio-Content sowie OpenAI-Function-Tools. Der synchrone HTTP-Kern
//! serialisiert Browserruns. Jeder Provideraufruf fuehrt genau einen
//! harnessfreien Browser-Inference-Turn aus; `AgentController`, `webagent/1`
//! und lokale Werkzeuge bleiben ausserhalb dieser Schicht.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[cfg(test)]
use std::{fs, path::PathBuf};

mod boundary;
mod catalog;
mod content;
mod inference;
mod media;
mod provider_handlers;
mod response_protocol;
mod routing;
mod store;
#[cfg(test)]
mod tests;
mod transport;
mod wire;

pub use boundary::api_error;
#[cfg(test)]
use boundary::constant_time_equal;
use boundary::{api_error_code, authorize, browser_inference_error, model_not_found};
use store::{
    append_response_message, handle_response_delete, handle_response_input_items,
    handle_response_retrieve, responses_context, store_response, tenant_id,
};
#[cfg(test)]
use store::{
    delete_response, forget_cached_response_stores, response_input_items, retrieve_response,
    tenant_store_path,
};

#[cfg(test)]
use inference::annotate_auto_routed_inference_error;
use inference::{browser_run_lock, run_task_blocking, run_task_streaming};
use media::{audio_mime, handle_audio_speech, handle_audio_transcription, handle_image_generation};
#[cfg(test)]
use media::{is_audio_capability_refusal, multipart_parts, multipart_text};

use provider_handlers::{
    handle_anthropic, handle_openai, handle_openai_incremental, handle_responses,
    handle_responses_incremental,
};
#[cfg(test)]
use routing::is_incremental_text_request;
use routing::{classify, BridgeRoute};

// Public re-exports for external API compatibility
pub use catalog::{
    advertised_input_modalities, advertised_output_modalities, available_brains,
    classify_auto_route, first_available_auto_brain, first_available_auto_brain_in, model_id,
    model_metadata, resolve_model, select_auto_brain, select_auto_brain_for_cli,
    select_auto_brain_with_default, AutoPurpose, AutoRoute,
};

use transport::find_bytes;
pub use transport::read_http_request;
#[cfg(test)]
pub(crate) use transport::TestPresence;
pub(crate) use transport::{AlwaysConnected, ClientPresence, TcpClientPresence};
#[cfg(test)]
use wire::render_http_response;
pub use wire::write_http_response;
use wire::{write_data_frame, write_sse_comment, write_sse_event, write_sse_headers};

pub(crate) use content::{
    anthropic_prompt, anthropic_tool_choice, anthropic_tools, conversation_prompt, decode_json,
    default_true, effective_tool_choice, openai_prompt, openai_tool_choice, openai_tools,
    reject_unsupported_openai_body, require_clean_text_tools, responses_messages,
    responses_tool_choice, responses_tools,
};
#[cfg(test)]
pub(crate) use content::{
    content_to_prompt, conversation_task, decode_base64, openai_task,
    reject_unsupported_openai_fields, responses_content, responses_task, text_content,
};
#[cfg(test)]
pub(crate) use response_protocol::responses_sse;
pub(crate) use response_protocol::{
    anthropic_response, anthropic_sse, openai_message, openai_sse, response_object,
    response_object_from_answer, response_with_state, responses_sse_with_object,
};

/// Request envelope limit. Multimodal data is base64-encoded in JSON, so the
/// transport needs more room than the historical text-only 1 MiB cap. Each
/// decoded attachment is still capped independently in `browser_inference`.
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_CONNECTIONS: usize = 8;
const MAX_STORED_RESPONSES: usize = 256;
const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const LOCAL_STATE_FORMAT: &str = "openai-local-state-v1";

#[derive(Clone, Serialize, Deserialize)]
struct StoredResponse {
    response: Value,
    messages: Vec<ConversationMessage>,
}

#[derive(Debug)]
pub(crate) struct PromptBundle {
    text: String,
    attachments: Vec<crate::browser_inference::BrowserAttachment>,
    /// T-961: ein alternativer Prompt fuer genau einen Frisch-Chat-Retry, wenn
    /// das Modell auf einem Werkzeug-Fortsetzungsturn eine semantische
    /// Verfuegbarkeitsverweigerung als ganz normale Textantwort liefert.
    /// Enthaelt den urspruenglichen Auftrag unverändert plus die bereits
    /// gesammelten Werkzeugergebnisse, ohne Korrektur- oder Meta-Wording.
    retry_task: Option<String>,
}

#[derive(Default)]
struct ResponseStore {
    entries: BTreeMap<String, StoredResponse>,
    order: VecDeque<String>,
}

#[derive(Default)]
struct StoreHub {
    tenants: BTreeMap<String, ResponseStore>,
}

#[derive(Serialize, Deserialize)]
struct OnDiskStore {
    format: String,
    entries: BTreeMap<String, StoredResponse>,
    order: Vec<String>,
}

#[derive(Default)]
pub struct ConnectionLimiter {
    active: AtomicUsize,
}

impl ConnectionLimiter {
    pub fn try_acquire(self: &Arc<Self>) -> Option<ConnectionPermit> {
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

pub struct ConnectionPermit(Arc<ConnectionLimiter>);

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::Release);
    }
}

/// Laufzeitkonfiguration des lokalen Dienstes.
///
/// `api_key` wird ausschliesslich aus einer Umgebungsvariable geladen und darf
/// weder geloggt noch in Statusantworten ausgegeben werden.
#[derive(Clone, Debug)]
pub struct BridgeConfig {
    pub bind: SocketAddr,
    pub brain: String,
    pub timeout_secs: Option<f64>,
    pub headless: bool,
    pub api_key: String,
    /// Test double: if set, browser inference is skipped and this text is returned.
    /// Production (`webagent api serve`) leaves this `None`.
    pub fake_reply: Option<String>,
    /// T-961: escort `tool_choice: auto` (and an omitted choice with tools) to
    /// `required` on the server side. Opt-in and off by default: forcing a tool
    /// call removes the model's ability to finish a tool loop with a plain text
    /// answer, so only an operator who accepts that semantics may enable it.
    pub auto_tool_choice_required: bool,
}

/// Startet den lokalen Dienst und blockiert, bis der Prozess beendet wird.
///
/// Der Dienst verarbeitet pro Brain genau einen Browserturn zur Zeit. Das ist
/// beabsichtigt: Ein Browserprofil darf nicht gleichzeitig von mehreren
/// Inference-Anfragen gesteuert werden; unterschiedliche Brains blockieren
/// sich dagegen nicht gegenseitig.
pub fn serve(config: BridgeConfig) -> Result<(), String> {
    validate_bridge_config(&config)?;

    if !config.bind.ip().is_loopback() {
        return Err("API-Bridge darf nur an eine Loopback-Adresse binden.".to_string());
    }

    let listener = TcpListener::bind(config.bind)
        .map_err(|error| format!("API-Bridge nicht bindbar: {error}"))?;
    let bound = listener.local_addr().unwrap_or(config.bind);
    println!("[api] Bridge aktiv auf http://{bound}");

    accept_loop(listener, config)
}

/// Prueft die Bridge-Rolle vor dem Binden/Servieren (geteilt mit dem
/// gemeinsamen Web-UI-Listener bei gesetzter API-Rolle).
pub fn validate_bridge_config(config: &BridgeConfig) -> Result<(), String> {
    if config.timeout_secs.is_some_and(|timeout| timeout <= 0.0) {
        return Err("--timeout-secs muss groesser als 0 sein.".to_string());
    }

    resolve_model(&model_id(&config.brain), &config.brain)?;
    Ok(())
}

fn accept_loop(listener: TcpListener, config: BridgeConfig) -> Result<(), String> {
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

pub fn overload_response() -> HttpResponse {
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
    route_request(stream, &request, config)
}

/// Verarbeitet einen bereits gelesenen HTTP-Request gegen die Bridge-Routen.
///
/// Wird von `handle_connection` (Bridge-Eigenbetrieb) und vom gemeinsamen
/// Web-UI-Listener (`web_ui::serve` bei gesetzter API-Rolle) aufgerufen:
/// ein Port, gleiche Routing-Logik.
pub fn route_request(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    // T-946: ein zweiter Handle auf demselben Socket beobachtet waehrend des
    // Browserturns, ob der Aufrufer noch da ist. Kein Clone moeglich -> der
    // Turn laeuft wie bisher (lieber ausfuehren als falsch abbrechen).
    static ALWAYS: AlwaysConnected = AlwaysConnected;
    let owned = TcpClientPresence::new(stream);
    let presence: &dyn ClientPresence = owned.as_ref().map_or(&ALWAYS, |p| p);
    match classify(&request.method, &request.path, &request.body) {
        BridgeRoute::ResponsesIncremental => {
            handle_responses_incremental(stream, request, config, presence)
        }
        BridgeRoute::ChatIncremental => {
            handle_openai_incremental(stream, request, config, presence)
        }
        BridgeRoute::Health => write_http_response(
            stream,
            HttpResponse::json(
                200,
                json!({
                    "status": "ok",
                    "service": "webagent-provider-bridge"
                }),
            ),
        ),
        BridgeRoute::ModelsList => {
            let response =
                if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                    response
                } else {
                    let models: Vec<Value> = std::iter::once("auto".to_string())
                        .chain(available_brains())
                        .map(|brain| model_metadata(&brain))
                        .collect();
                    HttpResponse::json(
                        200,
                        json!({
                            "object": "list",
                            "data": models
                        }),
                    )
                };
            write_http_response(stream, response)
        }
        BridgeRoute::ModelGet { id } => {
            let response =
                if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                    response
                } else {
                    match resolve_model(&id, &config.brain) {
                        Ok(brain) => {
                            let mut metadata = model_metadata(&brain);
                            metadata["created"] = json!(unix_seconds());
                            HttpResponse::json(200, metadata)
                        }
                        Err(error) => api_error(ApiFlavor::OpenAi, 404, &error),
                    }
                };
            write_http_response(stream, response)
        }
        BridgeRoute::ResponseInputItems { path } => {
            write_http_response(stream, handle_response_input_items(request, config, &path))
        }
        BridgeRoute::ResponseRetrieve { path } => {
            write_http_response(stream, handle_response_retrieve(request, config, &path))
        }
        BridgeRoute::ResponseDelete { path } => {
            write_http_response(stream, handle_response_delete(request, config, &path))
        }
        BridgeRoute::ChatCompletions => {
            write_http_response(stream, handle_openai(request, config, presence))
        }
        BridgeRoute::ImageGenerations => {
            write_http_response(stream, handle_image_generation(request, config))
        }
        BridgeRoute::AudioTranscriptions => write_http_response(
            stream,
            handle_audio_transcription(request, config, false, presence),
        ),
        BridgeRoute::AudioTranslations => write_http_response(
            stream,
            handle_audio_transcription(request, config, true, presence),
        ),
        BridgeRoute::AudioSpeech => {
            write_http_response(stream, handle_audio_speech(request, config))
        }
        BridgeRoute::Responses => {
            write_http_response(stream, handle_responses(request, config, presence))
        }
        BridgeRoute::Messages => {
            write_http_response(stream, handle_anthropic(request, config, presence))
        }
        BridgeRoute::NotFound { anthropic } => {
            let flavor = if anthropic {
                ApiFlavor::Anthropic
            } else {
                ApiFlavor::OpenAi
            };
            write_http_response(stream, api_error(flavor, 404, "Endpoint nicht gefunden."))
        }
    }
}

/// Clean DOM/stream snapshots before SSE `delta.content` (Thinking..., clocks,
/// Kimi CoT echo). Empty → no delta; caller may still keep-alive.
fn stream_answer_snapshot(raw: &str) -> String {
    webagent_core::observer::chat_answer_text(raw)
}

/// Einmalige, prozessweite Kern-Registrierung lebender Lauefe. Responses-Runs
/// legen ihre Ausfuehrung hier ab (statt eigene Session-Logik in der Bridge zu
/// halten), damit eine zweite Sicht (Web-UI/REPL) denselben Strom sieht.
fn session_service() -> &'static crate::session::SessionService {
    static SESSION: OnceLock<crate::session::SessionService> = OnceLock::new();
    SESSION.get_or_init(crate::session::SessionService::new)
}

#[derive(Deserialize)]
pub(crate) struct OpenAiRequest {
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
pub(crate) struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<ConversationMessage>,
    #[serde(default)]
    system: Option<Value>,
    #[serde(default)]
    stream: Option<bool>,
    #[serde(default)]
    tools: Vec<Value>,
    #[serde(default)]
    tool_choice: Option<Value>,
}

#[derive(Deserialize)]
pub(crate) struct ResponsesRequest {
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
    #[serde(default)]
    previous_response_id: Option<String>,
    #[serde(default = "default_true")]
    store: bool,
}

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct ConversationMessage {
    role: String,
    #[serde(default)]
    content: Value,
    #[serde(default)]
    tool_calls: Vec<OpenAiAssistantToolCall>,
    #[serde(default)]
    tool_call_id: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiTool {
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiFunction,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiFunction {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parameters: Value,
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct OpenAiAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiAssistantFunction,
}

#[derive(Clone, Deserialize, serde::Serialize)]
pub(crate) struct OpenAiAssistantFunction {
    name: String,
    arguments: String,
}

#[derive(Clone, Copy)]
pub enum ApiFlavor {
    OpenAi,
    Anthropic,
}

pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub query: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

pub struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    /// Zusaetzlicher `Retry-After`-Header in Sekunden. Nur gesetzt, wenn dem
    /// Client eine konkrete Wartezeit genannt werden kann (offener
    /// Circuit-Breaker), sonst `None` und der Header entfaellt.
    retry_after_secs: Option<i64>,
}

impl HttpResponse {
    fn json(status: u16, value: Value) -> Self {
        let body = serde_json::to_vec(&value)
            .unwrap_or_else(|_| b"{\"error\":\"serialization\"}".to_vec());
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body,
            retry_after_secs: None,
        }
    }

    fn sse(body: String) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream; charset=utf-8",
            body: body.into_bytes(),
            retry_after_secs: None,
        }
    }

    /// Antwort mit `Retry-After: <seconds>` versehen. Ein Wert unter 1 wird auf
    /// 1 angehoben, weil `Retry-After: 0` beim Client wie ein sofortiger
    /// Wiederholungsauftrag wirkt und den Breaker ad absurdum fuehrt.
    fn with_retry_after(mut self, seconds: i64) -> Self {
        self.retry_after_secs = Some(seconds.max(1));
        self
    }
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
    match prefix {
        "resp" => format!("resp_{nanos:x}"),
        "chatcmpl" => format!("chatcmpl-{nanos:x}"),
        _ => format!("{prefix}_{nanos:x}"),
    }
}
