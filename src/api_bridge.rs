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
mod content;
mod inference;
mod provider_handlers;
mod response_protocol;
mod routing;
mod store;
#[cfg(test)]
mod tests;
mod transport;
mod wire;

pub(crate) use boundary::api_error;
#[cfg(test)]
use boundary::constant_time_equal;
use boundary::{api_error_code, authorize};
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

use provider_handlers::{
    handle_anthropic, handle_openai, handle_openai_incremental, handle_responses,
    handle_responses_incremental,
};
#[cfg(test)]
use routing::is_incremental_text_request;
use routing::{classify, BridgeRoute};
use transport::find_bytes;
pub(crate) use transport::read_http_request;
#[cfg(test)]
use wire::render_http_response;
pub(crate) use wire::write_http_response;
use wire::{sse_data, write_data_frame, write_sse_comment, write_sse_event, write_sse_headers};

pub(crate) use content::{
    anthropic_prompt, anthropic_tool_choice, anthropic_tools, conversation_prompt, decode_json,
    default_true, openai_prompt, openai_tool_choice, openai_tools, reject_unsupported_openai_body,
    require_clean_text_tools, responses_messages, responses_tool_choice, responses_tools,
};
#[cfg(test)]
pub(crate) use content::{
    content_to_prompt, conversation_task, decode_base64, openai_task,
    reject_unsupported_openai_fields, responses_content, responses_task, text_content,
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
pub(crate) struct ConnectionLimiter {
    active: AtomicUsize,
}

impl ConnectionLimiter {
    pub(crate) fn try_acquire(self: &Arc<Self>) -> Option<ConnectionPermit> {
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

pub(crate) struct ConnectionPermit(Arc<ConnectionLimiter>);

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
pub(crate) fn validate_bridge_config(config: &BridgeConfig) -> Result<(), String> {
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

pub(crate) fn overload_response() -> HttpResponse {
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
pub(crate) fn route_request(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    match classify(&request.method, &request.path, &request.body) {
        BridgeRoute::ResponsesIncremental => handle_responses_incremental(stream, request, config),
        BridgeRoute::ChatIncremental => handle_openai_incremental(stream, request, config),
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
        BridgeRoute::ChatCompletions => write_http_response(stream, handle_openai(request, config)),
        BridgeRoute::ImageGenerations => {
            write_http_response(stream, handle_image_generation(request, config))
        }
        BridgeRoute::AudioTranscriptions => {
            write_http_response(stream, handle_audio_transcription(request, config, false))
        }
        BridgeRoute::AudioTranslations => {
            write_http_response(stream, handle_audio_transcription(request, config, true))
        }
        BridgeRoute::AudioSpeech => {
            write_http_response(stream, handle_audio_speech(request, config))
        }
        BridgeRoute::Responses => write_http_response(stream, handle_responses(request, config)),
        BridgeRoute::Messages => write_http_response(stream, handle_anthropic(request, config)),
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

fn handle_image_generation(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let payload: ImageGenerationRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    if payload.prompt.trim().is_empty() {
        return api_error(ApiFlavor::OpenAi, 400, "prompt darf nicht leer sein.");
    }
    if payload.n.unwrap_or(1) != 1 {
        return api_error(
            ApiFlavor::OpenAi,
            400,
            "Die Browser-Bridge unterstuetzt derzeit genau ein Bild pro Request (n=1).",
        );
    }
    // Aktuelle GPT-Image-Antworten liefern `data[].b64_json` standardmaessig.
    // `url` bleibt als tolerierte Legacy-Kompatibilitaet fuer aeltere Clients.
    let response_format = payload.response_format.as_deref().unwrap_or("b64_json");
    if !matches!(response_format, "url" | "b64_json") {
        return api_error(
            ApiFlavor::OpenAi,
            400,
            "response_format muss 'url' oder 'b64_json' sein.",
        );
    }
    let requested_model = payload
        .model
        .clone()
        .unwrap_or_else(|| model_id(&config.brain));
    let brain = match resolve_model(&requested_model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let generation_prompt = match payload.size.as_deref() {
        Some(size) => format!(
            "Generate an image from this request. Required output size/aspect: {size}. Do not merely describe it.\n\n{}",
            payload.prompt.trim()
        ),
        None => format!(
            "Generate an image from this request. Do not merely describe it.\n\n{}",
            payload.prompt.trim()
        ),
    };
    let image = match run_image_generation_blocking(config, &brain, &generation_prompt) {
        Ok(image) => image,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let item = if response_format == "b64_json" {
        json!({"b64_json": image.base64, "revised_prompt": Value::Null})
    } else {
        json!({
            "url": format!("data:{};base64,{}", image.mime_type, image.base64),
            "revised_prompt": Value::Null
        })
    };
    HttpResponse::json(200, json!({"created": unix_seconds(), "data": [item]}))
}

fn handle_audio_transcription(
    request: &HttpRequest,
    config: &BridgeConfig,
    translate_to_english: bool,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let parts = match multipart_parts(request) {
        Ok(parts) => parts,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let Some(file) = parts.iter().find(|part| part.name == "file") else {
        return api_error(ApiFlavor::OpenAi, 400, "Multipart-Feld 'file' fehlt.");
    };
    if file.data.is_empty() {
        return api_error(ApiFlavor::OpenAi, 400, "Audiodatei ist leer.");
    }
    let response_format = multipart_text(&parts, "response_format").unwrap_or("json");
    if !matches!(response_format, "json" | "text" | "verbose_json") {
        return api_error(
            ApiFlavor::OpenAi,
            400,
            "response_format wird browserseitig als json, text oder verbose_json unterstuetzt.",
        );
    }
    let requested_model = multipart_text(&parts, "model").unwrap_or("webagent");
    let brain = if requested_model == "webagent" || !requested_model.starts_with("webagent/") {
        config.brain.clone()
    } else {
        match resolve_model(requested_model, &config.brain) {
            Ok(brain) => brain,
            Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
        }
    };
    let mime_type = file
        .content_type
        .as_deref()
        .filter(|mime| mime.starts_with("audio/"))
        .unwrap_or("audio/wav")
        .to_string();
    let attachment = crate::browser_inference::BrowserAttachment {
        kind: crate::browser_inference::BrowserAttachmentKind::Audio,
        file_name: file
            .file_name
            .clone()
            .unwrap_or_else(|| "audio.wav".to_string()),
        mime_type,
        data: file.data.clone(),
    };
    let prompt = if translate_to_english {
        "Translate the attached audio into English. Return only the translated text, without commentary or quotation marks."
    } else {
        "Transcribe the attached audio verbatim. Preserve the original language and the exact spoken words: do not translate them. Return only the transcript, without commentary or quotation marks."
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        prompt,
        &[attachment],
        &[],
        crate::browser_inference::BrowserToolChoice::None,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let text = answer.text.unwrap_or_default().trim().to_string();
    if text.is_empty() {
        return api_error(ApiFlavor::OpenAi, 502, "Provider lieferte kein Transkript.");
    }
    if is_audio_capability_refusal(&text) {
        return api_error(
            ApiFlavor::OpenAi,
            502,
            "Das ausgewaehlte Web-Brain unterstuetzt keine Audio-Transkription.",
        );
    }
    if response_format == "text" {
        return HttpResponse {
            status: 200,
            content_type: "text/plain; charset=utf-8",
            body: text.into_bytes(),
        };
    }
    let body = if response_format == "verbose_json" {
        json!({"task": if translate_to_english {"translate"} else {"transcribe"}, "language": Value::Null, "duration": Value::Null, "text": text, "segments": []})
    } else {
        json!({"text": text})
    };
    HttpResponse::json(200, body)
}

/// Browser-Brains antworten bei nicht unterstuetztem Audio gelegentlich mit
/// einer hoeflichen Textabsage statt mit einem leeren Ergebnis. Diese Absage
/// darf nicht als gueltiges OpenAI-Transkript an den Client durchgereicht
/// werden; die Erkennung bleibt bewusst auf eindeutige Formulierungen begrenzt.
fn is_audio_capability_refusal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "nicht zuverlässig transkribieren",
        "nicht zuverlaessig transkribieren",
        "unable to transcribe",
        "cannot transcribe",
        "can't transcribe",
        "not able to transcribe",
        "don't have native audio",
        "do not have the ability to transcribe",
        "audio files aren't something i can",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn handle_audio_speech(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    api_error(
        ApiFlavor::OpenAi,
        502,
        "Kein konfiguriertes Web-Brain liefert derzeit ein extrahierbares Text-to-Speech-Audioartefakt.",
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MultipartPart {
    name: String,
    file_name: Option<String>,
    content_type: Option<String>,
    data: Vec<u8>,
}

fn multipart_text<'a>(parts: &'a [MultipartPart], name: &str) -> Option<&'a str> {
    parts
        .iter()
        .find(|part| part.name == name)
        .and_then(|part| std::str::from_utf8(&part.data).ok())
        .map(str::trim)
}

fn multipart_parts(request: &HttpRequest) -> Result<Vec<MultipartPart>, String> {
    let content_type = request
        .headers
        .get("content-type")
        .ok_or_else(|| "Content-Type fehlt.".to_string())?;
    let boundary = content_type
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("boundary="))
        .map(|value| value.trim_matches('"'))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "multipart/form-data boundary fehlt.".to_string())?;
    if !content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("multipart/form-data"))
    {
        return Err("Content-Type muss multipart/form-data sein.".to_string());
    }
    let delimiter = format!("--{boundary}").into_bytes();
    let mut parts = Vec::new();
    let mut cursor = 0;
    while let Some(relative) = find_bytes(&request.body[cursor..], &delimiter) {
        let start = cursor + relative + delimiter.len();
        if request.body.get(start..start + 2) == Some(b"--") {
            break;
        }
        let start = start + 2;
        let Some(next_relative) = find_bytes(&request.body[start..], &delimiter) else {
            break;
        };
        let end = start + next_relative;
        let raw = request.body[start..end]
            .strip_suffix(b"\r\n")
            .unwrap_or(&request.body[start..end]);
        let header_end = find_bytes(raw, b"\r\n\r\n")
            .ok_or_else(|| "Multipart-Teil ohne Headerabschluss.".to_string())?;
        let headers = std::str::from_utf8(&raw[..header_end])
            .map_err(|_| "Multipart-Header ist nicht UTF-8/ASCII.".to_string())?;
        let disposition = headers
            .lines()
            .find(|line| {
                line.to_ascii_lowercase()
                    .starts_with("content-disposition:")
            })
            .ok_or_else(|| "Multipart-Teil ohne Content-Disposition.".to_string())?;
        let parameter = |key: &str| {
            disposition.split(';').map(str::trim).find_map(|value| {
                value
                    .strip_prefix(&format!("{key}="))
                    .map(|text| text.trim_matches('"').to_string())
            })
        };
        let name = parameter("name").ok_or_else(|| "Multipart-Teil ohne name.".to_string())?;
        let content_type = headers.lines().find_map(|line| {
            line.split_once(':').and_then(|(key, value)| {
                key.trim()
                    .eq_ignore_ascii_case("content-type")
                    .then(|| value.trim().to_string())
            })
        });
        parts.push(MultipartPart {
            name,
            file_name: parameter("filename"),
            content_type,
            data: raw[header_end + 4..].to_vec(),
        });
        cursor = end;
    }
    if parts.is_empty() {
        return Err("Multipart-Body enthaelt keine Felder.".to_string());
    }
    Ok(parts)
}

/// Clean DOM/stream snapshots before SSE `delta.content` (Thinking..., clocks,
/// Kimi CoT echo). Empty → no delta; caller may still keep-alive.
fn stream_answer_snapshot(raw: &str) -> String {
    crate::observer::chat_answer_text(raw)
}

fn run_image_generation_blocking(
    config: &BridgeConfig,
    brain: &str,
    prompt: &str,
) -> Result<crate::relay::GeneratedImage, String> {
    let brain = if brain == "auto" {
        select_auto_brain(config, prompt, &[], false, AutoPurpose::ImageGeneration)?
    } else {
        brain.to_string()
    };
    let lock = browser_run_lock(&brain);
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::relay::relay_image_generation(&brain, prompt, config.headless, config.timeout_secs)
        .map_err(|error| format!("Browser-Bildgenerierung fehlgeschlagen: {error}"))
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
struct ImageGenerationRequest {
    prompt: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    n: Option<u32>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    response_format: Option<String>,
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

fn audio_mime(format: &str) -> Result<String, String> {
    let normalized = format.trim().trim_start_matches('.').to_ascii_lowercase();
    let mime = match normalized.as_str() {
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "m4a" | "mp4" => "audio/mp4",
        "ogg" | "opus" => "audio/ogg",
        "flac" => "audio/flac",
        "webm" => "audio/webm",
        other => return Err(format!("Audioformat '{other}' wird nicht unterstuetzt.")),
    };
    Ok(mime.to_string())
}

pub fn available_brains() -> Vec<String> {
    let mut brains: Vec<String> = crate::config::brains().into_keys().collect();
    brains.sort();
    brains
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoPurpose {
    Chat,
    ImageGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoRoute {
    Default,
    AudioInput,
    ImageInput,
    ImageGeneration,
    Tools,
    Coding,
    CurrentResearch,
}

pub(crate) fn classify_auto_route(
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    has_tools: bool,
    purpose: AutoPurpose,
) -> AutoRoute {
    use crate::browser_inference::BrowserAttachmentKind;

    if purpose == AutoPurpose::ImageGeneration {
        return AutoRoute::ImageGeneration;
    }
    if attachments
        .iter()
        .any(|attachment| attachment.kind == BrowserAttachmentKind::Audio)
    {
        return AutoRoute::AudioInput;
    }
    if attachments
        .iter()
        .any(|attachment| attachment.kind == BrowserAttachmentKind::Image)
    {
        return AutoRoute::ImageInput;
    }
    if has_tools {
        return AutoRoute::Tools;
    }

    let lower = task.to_ascii_lowercase();
    if [
        "code",
        "cargo",
        "rust",
        "python",
        "typescript",
        "javascript",
        "compile",
        "debug",
        "refactor",
        "implement",
        "funktion",
        "klasse",
        "repository",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return AutoRoute::Coding;
    }
    if [
        "latest",
        "current",
        "today",
        "research",
        "sources",
        "web search",
        "aktuell",
        "heute",
        "recherch",
        "quellen",
        "internet",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return AutoRoute::CurrentResearch;
    }
    AutoRoute::Default
}

pub(crate) fn first_available_auto_brain(preferences: &[&str]) -> Option<String> {
    let available = available_brains();
    first_available_auto_brain_in(preferences, &available, |brain| {
        crate::circuit_breaker::check(brain).is_none()
    })
}

/// Kern der Auto-Auswahl mit injizierbarer Verfügbarkeit/Entsperrtheit —
/// deterministisch testbar ohne reale Brain-Installation oder Circuit-Breaker.
fn first_available_auto_brain_in(
    preferences: &[&str],
    available: &[String],
    is_unlocked: impl Fn(&str) -> bool,
) -> Option<String> {
    preferences
        .iter()
        .find(|brain| available.iter().any(|candidate| candidate == **brain) && is_unlocked(brain))
        .map(|brain| (*brain).to_string())
}

fn select_auto_brain(
    config: &BridgeConfig,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    has_tools: bool,
    purpose: AutoPurpose,
) -> Result<String, String> {
    let default = if config.brain == "auto" {
        "chatgpt"
    } else {
        config.brain.as_str()
    };
    select_auto_brain_with_default(task, attachments, has_tools, purpose, default)
}

/// Auto-Router fuer die CLI (run/repl/relay): ahnt aus der Aufgabe das passende
/// Brain, ohne auf eine laufende Bridge-Config angewiesen zu sein. Default-Fall
/// faellt auf das erste verfuegbare, nicht vom Circuit-Breaker gesperrte Brain.
pub fn select_auto_brain_for_cli(task: &str) -> Result<String, String> {
    select_auto_brain_with_default(task, &[], false, AutoPurpose::Chat, "chatgpt")
}

fn select_auto_brain_with_default(
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    has_tools: bool,
    purpose: AutoPurpose,
    default: &str,
) -> Result<String, String> {
    let route = classify_auto_route(task, attachments, has_tools, purpose);
    let (preferences, reason): (&[&str], &str) = match route {
        AutoRoute::ImageGeneration => (&["chatgpt", "gemini"], "image-generation"),
        AutoRoute::AudioInput => (&["gemini"], "audio-input"),
        AutoRoute::ImageInput => (&["gemini", "chatgpt", "claude"], "image-input"),
        AutoRoute::Tools => (&["chatgpt", "gemini", "claude"], "tool-call"),
        AutoRoute::Coding => (&["claude", "chatgpt", "gemini"], "coding"),
        AutoRoute::CurrentResearch => (&["perplexity", "gemini", "chatgpt"], "current-research"),
        AutoRoute::Default => {
            let preferences = [default, "chatgpt", "gemini", "claude", "deepseek"];
            let selected = first_available_auto_brain(&preferences)
                .ok_or_else(|| "AutoRouter findet kein verfuegbares Text-Brain.".to_string())?;
            eprintln!("[auto-router] selected={selected} reason=default");
            return Ok(selected);
        }
    };
    let selected = first_available_auto_brain(preferences)
        .ok_or_else(|| format!("AutoRouter findet kein verfuegbares Brain fuer {reason}."))?;
    eprintln!("[auto-router] selected={selected} reason={reason}");
    Ok(selected)
}

fn resolve_model(requested: &str, default_brain: &str) -> Result<String, String> {
    let brain = if requested == "webagent" {
        default_brain
    } else if requested == "auto"
        || available_brains()
            .iter()
            .any(|candidate| candidate == requested)
    {
        requested
    } else {
        requested
            .strip_prefix("webagent/")
            .or_else(|| requested.strip_prefix("wa/"))
            .ok_or_else(|| format!("Ungueltige WebAgent-Modell-ID '{requested}'."))?
    };
    if brain == "auto" {
        return Ok("auto".to_string());
    }
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

/// Eingabe-Modalitäten pro Brain über den Bridge-Endpoint.
///
/// Ein `file_attach`-Eintrag im Selektorprofil ist nur ein UI-Hinweis. Erst ein
/// bestätigter Live-Upload (docs/CURRENT_WORK.md) belegt, dass der jeweilige
/// Brain Medien tatsächlich annimmt. Basis sind die IMAGE_INPUT_OK- bzw.
/// AUDIO_INPUT_OK-Smokes: deepseek/gemini/kimi/mistral (Bild), gemini (Audio),
/// chatgpt/claude (Bild+Audio). Brain ohne bestätigten Smoke (qwen/zai/
/// perplexity) melden nur Text, damit Clients nicht blind in einen
/// unbestätigten Pfad senden.
fn advertised_input_modalities(brain: &str) -> &'static [&'static str] {
    match brain {
        "auto" => &["text", "image", "audio"],
        "gemini" => &["text", "image", "audio"],
        "chatgpt" | "claude" => &["text", "image"],
        "deepseek" | "kimi" | "mistral" => &["text", "image"],
        _ => &["text"],
    }
}

/// Ausgabe-Modalitäten pro Brain über den Bridge-Endpoint.
///
/// Text beherrscht jeder Brain. Bild-Output ist nur dort belegt, wo eine
/// funktionierende Generation existiert: chatgpt über `/v1/images/generations`
/// (relay_image_generation + estuary-Fetch). Die übrigen Brains liefern über
/// den Endpoint derzeit nur Text, bis eine Bildgeneration tatsächlich
/// verifiziert ist.
fn advertised_output_modalities(brain: &str) -> &'static [&'static str] {
    match brain {
        "auto" | "chatgpt" => &["text", "image"],
        _ => &["text"],
    }
}

/// Einheitliche, pro-Brain-Metadaten für den `/v1/models`-Katalog.
///
/// `context_window`/`max_tokens` bleiben konservativ als gemeinsame Defaults
/// (keine verifizierten pro-Brain-Kontingente im Repo); `advertised_*` liefern
/// die tatsächlich bestätigten Modalitäten.
fn model_metadata(brain: &str) -> Value {
    let mut metadata = json!({
        "id": model_id(brain),
        "object": "model",
        "created": 0,
        "owned_by": "webagent",
        "brain": brain,
        "context_window": 128000,
        "max_tokens": 16384,
        "modalities": {
            "input": advertised_input_modalities(brain),
            "output": advertised_output_modalities(brain)
        }
    });
    if brain == "auto" {
        metadata["name"] = json!("WebAgent AutoRouter");
        metadata["virtual"] = json!(true);
        metadata["routing"] = json!({
            "audio": "gemini",
            "image_generation": ["chatgpt", "gemini"],
            "image_input": ["gemini", "chatgpt", "claude"],
            "tools": ["chatgpt", "gemini", "claude"],
            "coding": ["claude", "chatgpt", "gemini"],
            "current_research": ["perplexity", "gemini", "chatgpt"],
            "fallback": "configured default brain"
        });
    }
    metadata
}

#[derive(Clone, Copy)]
pub(crate) enum ApiFlavor {
    OpenAi,
    Anthropic,
}

pub(crate) struct HttpRequest {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) query: String,
    pub(crate) headers: BTreeMap<String, String>,
    pub(crate) body: Vec<u8>,
}

pub(crate) struct HttpResponse {
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
