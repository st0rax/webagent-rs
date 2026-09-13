//! Lokale Provider-Bridge fuer Pi-kompatible OpenAI- und Anthropic-Anfragen.
//!
//! Der Dienst bindet ausschliesslich an Loopback, verlangt einen Bearer- oder
//! Anthropic-kompatiblen x-api-key-Token und akzeptiert Text-, Bild- und
//! Audio-Content sowie OpenAI-Function-Tools. Der synchrone HTTP-Kern
//! serialisiert Browserruns. Jeder Provideraufruf fuehrt genau einen
//! harnessfreien Browser-Inference-Turn aus; `AgentController`, `webagent/1`
//! und lokale Werkzeuge bleiben ausserhalb dieser Schicht.

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

mod boundary;
mod provider_handlers;
mod routing;
#[cfg(test)]
mod tests;
mod transport;
mod wire;

pub(crate) use boundary::api_error;
#[cfg(test)]
use boundary::constant_time_equal;
use boundary::{api_error_code, authorize};
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

/// Request envelope limit. Multimodal data is base64-encoded in JSON, so the
/// transport needs more room than the historical text-only 1 MiB cap. Each
/// decoded attachment is still capped independently in `browser_inference`.
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_CONNECTIONS: usize = 8;
const MAX_STORED_RESPONSES: usize = 256;
const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024 * 1024;
const LOCAL_STATE_FORMAT: &str = "openai-local-state-v1";

static BROWSER_RUN_LOCKS: OnceLock<Mutex<BTreeMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

#[derive(Clone, Serialize, Deserialize)]
struct StoredResponse {
    response: Value,
    messages: Vec<ConversationMessage>,
}

#[derive(Debug)]
struct PromptBundle {
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

fn handle_response_retrieve(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let id = path.trim_start_matches("/v1/responses/");
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    match retrieve_response(&tenant_id(&config.api_key), id) {
        Some(stored) => HttpResponse::json(200, stored.response),
        None => api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        ),
    }
}

fn handle_response_delete(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let id = path.trim_start_matches("/v1/responses/");
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    if delete_response(&tenant_id(&config.api_key), id) {
        HttpResponse::json(200, json!({"id":id,"object":"response","deleted":true}))
    } else {
        api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        )
    }
}

fn handle_response_input_items(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let suffix = "/input_items";
    let id = path
        .strip_prefix("/v1/responses/")
        .and_then(|value| value.strip_suffix(suffix))
        .unwrap_or_default();
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    let Some(stored) = retrieve_response(&tenant_id(&config.api_key), id) else {
        return api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        );
    };
    let data = response_input_items(&stored.messages);
    HttpResponse::json(
        200,
        json!({
            "object": "list",
            "data": data,
            "has_more": false,
            "first_id": data.first().and_then(|item| item.get("id")),
            "last_id": data.last().and_then(|item| item.get("id"))
        }),
    )
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

fn anthropic_response(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> Value {
    if answer.tool_calls.is_empty() {
        return json!({
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [{"type": "text", "text": answer.text.as_deref().unwrap_or_default()}],
            "stop_reason": "end_turn",
            "stop_sequence": null
        });
    }
    let content: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.arguments
            })
        })
        .collect();
    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": "tool_use",
        "stop_sequence": null
    })
}

/// Clean DOM/stream snapshots before SSE `delta.content` (Thinking..., clocks,
/// Kimi CoT echo). Empty → no delta; caller may still keep-alive.
fn stream_answer_snapshot(raw: &str) -> String {
    crate::observer::chat_answer_text(raw)
}

fn responses_context(
    payload: &ResponsesRequest,
    tenant: &str,
) -> Result<(Vec<ConversationMessage>, PromptBundle), (u16, String)> {
    let mut messages = match payload.previous_response_id.as_deref() {
        Some(id) => retrieve_response(tenant, id).map_or_else(
            || Err((404, format!("Previous response '{id}' nicht gefunden."))),
            |stored| Ok(stored.messages),
        )?,
        None => Vec::new(),
    };
    messages.extend(responses_messages(&payload.input).map_err(|error| (400, error))?);
    let prompt = conversation_prompt(payload.instructions.clone(), &messages)
        .map_err(|error| (400, error))?;
    Ok((messages, prompt))
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
    let lock = BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::relay::relay_image_generation(&brain, prompt, config.headless, config.timeout_secs)
        .map_err(|error| format!("Browser-Bildgenerierung fehlgeschlagen: {error}"))
}

fn fake_inference_response(text: &str) -> crate::browser_inference::BrowserInferenceResponse {
    crate::browser_inference::BrowserInferenceResponse {
        text: Some(text.to_string()),
        tool_calls: Vec::new(),
    }
}

fn emit_fake_stream(text: &str, on_update: &mut dyn FnMut(&str)) {
    let mut acc = String::new();
    let total = text.chars().count();
    for (index, ch) in text.chars().enumerate() {
        acc.push(ch);
        if (index + 1) % 3 == 0 || index + 1 == total {
            on_update(&acc);
        }
    }
}

fn run_task_blocking(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    tools: &[crate::browser_inference::BrowserTool],
    tool_choice: crate::browser_inference::BrowserToolChoice,
) -> Result<crate::browser_inference::BrowserInferenceResponse, String> {
    if let Some(text) = config.fake_reply.as_deref() {
        return Ok(fake_inference_response(text));
    }
    let via_auto = brain == "auto";
    let brain = if via_auto {
        select_auto_brain(
            config,
            task,
            attachments,
            !tools.is_empty(),
            AutoPurpose::Chat,
        )?
    } else {
        brain.to_string()
    };
    let timeout_secs = auto_attach_timeout_secs(via_auto, attachments, &brain, task, config);
    if via_auto {
        eprintln!(
            "[auto-router] execute brain={brain} attachments={} timeout_secs={:?}",
            attachments.len(),
            timeout_secs
        );
    }
    let lock = BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let started = Instant::now();
    crate::browser_inference::complete_with_attachments(
        crate::browser_inference::BrowserInferenceRequest {
            brain: &brain,
            prompt: task,
            tools,
            tool_choice,
            headless: config.headless,
            timeout_secs,
            model: None,
        },
        attachments,
        &mut |_| {},
    )
    .map_err(|error| {
        annotate_auto_routed_inference_error(via_auto, &brain, &error, started.elapsed())
    })
}

fn run_task_streaming(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<crate::browser_inference::BrowserInferenceResponse, String> {
    if let Some(text) = config.fake_reply.as_deref() {
        emit_fake_stream(text, on_update);
        return Ok(fake_inference_response(text));
    }
    let via_auto = brain == "auto";
    let brain = if via_auto {
        select_auto_brain(config, task, attachments, false, AutoPurpose::Chat)?
    } else {
        brain.to_string()
    };
    let timeout_secs = auto_attach_timeout_secs(via_auto, attachments, &brain, task, config);
    if via_auto {
        eprintln!(
            "[auto-router] execute brain={brain} attachments={} timeout_secs={:?}",
            attachments.len(),
            timeout_secs
        );
    }
    let lock = BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let started = Instant::now();
    crate::browser_inference::complete_streaming_with_attachments(
        crate::browser_inference::BrowserInferenceRequest {
            brain: &brain,
            prompt: task,
            tools: &[],
            tool_choice: crate::browser_inference::BrowserToolChoice::None,
            headless: config.headless,
            timeout_secs,
            model: None,
        },
        attachments,
        on_update,
    )
    .map_err(|error| {
        annotate_auto_routed_inference_error(via_auto, &brain, &error, started.elapsed())
    })
}

/// Bei `auto` + Attachments: Budget an das geroutete Brain koppeln und deckeln.
fn auto_attach_timeout_secs(
    via_auto: bool,
    attachments: &[crate::browser_inference::BrowserAttachment],
    routed_brain: &str,
    task: &str,
    config: &BridgeConfig,
) -> Option<f64> {
    if via_auto && !attachments.is_empty() {
        Some(crate::timeouts::resolve_auto_attach_budget(
            routed_brain,
            task,
            config.timeout_secs,
        ))
    } else {
        config.timeout_secs
    }
}

fn is_auto_attach_timeout_signal(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "keine antwort erhalten",
        "timeout_budget=",
        "page-timeout",
        "zeitueberschreitung",
        "timed out",
        "timeout",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn is_auto_attach_upload_signal(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "no_file_input",
        "keinen nutzbaren datei-upload",
        "kein nutzbarer datei-upload",
        "dateien uebernommen",
        "dateien übernommen",
        "0 von",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Macht AutoRouter-Fehler explizit: welches Brain, Timeout vs. Upload, via=auto→brain.
fn annotate_auto_routed_inference_error(
    via_auto: bool,
    routed_brain: &str,
    error: &str,
    elapsed: Duration,
) -> String {
    if !via_auto {
        return format!("Browser-Inference fehlgeschlagen: {error}");
    }
    let secs = elapsed.as_secs_f64();
    if is_auto_attach_timeout_signal(error) {
        return format!("auto_attach_timeout: routed={routed_brain} after {secs:.0}s ({error})");
    }
    if is_auto_attach_upload_signal(error) {
        if error.contains("via=auto") {
            return format!("Browser-Inference fehlgeschlagen: {error}");
        }
        return format!("Browser-Inference fehlgeschlagen: {error}; via=auto→{routed_brain}");
    }
    format!("Browser-Inference fehlgeschlagen: {error}; via=auto→{routed_brain}")
}

fn store_hub() -> &'static Mutex<StoreHub> {
    static HUB: OnceLock<Mutex<StoreHub>> = OnceLock::new();
    HUB.get_or_init(|| Mutex::new(StoreHub::default()))
}

fn tenant_id(api_key: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in api_key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("t{hash:016x}")
}

fn local_state_root() -> PathBuf {
    crate::config::data_dir().join(LOCAL_STATE_FORMAT)
}

fn tenant_store_path(tenant: &str) -> PathBuf {
    local_state_root().join(tenant).join("store.json")
}

fn load_tenant_from_disk(tenant: &str) -> ResponseStore {
    let path = tenant_store_path(tenant);
    let Ok(bytes) = fs::read(&path) else {
        return ResponseStore::default();
    };
    let Ok(disk) = serde_json::from_slice::<OnDiskStore>(&bytes) else {
        return ResponseStore::default();
    };
    if disk.format != LOCAL_STATE_FORMAT {
        return ResponseStore::default();
    }
    ResponseStore {
        entries: disk.entries,
        order: disk.order.into(),
    }
}

fn persist_tenant(tenant: &str, store: &ResponseStore) {
    let path = tenant_store_path(tenant);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let disk = OnDiskStore {
        format: LOCAL_STATE_FORMAT.to_string(),
        entries: store.entries.clone(),
        order: store.order.iter().cloned().collect(),
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&disk) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

fn with_tenant_store<R>(tenant: &str, f: impl FnOnce(&mut ResponseStore) -> R) -> R {
    let mut hub = store_hub()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !hub.tenants.contains_key(tenant) {
        hub.tenants
            .insert(tenant.to_string(), load_tenant_from_disk(tenant));
    }
    let store = hub.tenants.get_mut(tenant).expect("tenant just inserted");
    let result = f(store);
    persist_tenant(tenant, store);
    result
}

#[cfg(test)]
fn forget_cached_response_stores() {
    store_hub()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .tenants
        .clear();
}

/// Einmalige, prozessweite Kern-Registrierung lebender Lauefe. Responses-Runs
/// legen ihre Ausfuehrung hier ab (statt eigene Session-Logik in der Bridge zu
/// halten), damit eine zweite Sicht (Web-UI/REPL) denselben Strom sieht.
fn session_service() -> &'static crate::session::SessionService {
    static SESSION: OnceLock<crate::session::SessionService> = OnceLock::new();
    SESSION.get_or_init(crate::session::SessionService::new)
}

fn retrieve_response(tenant: &str, id: &str) -> Option<StoredResponse> {
    with_tenant_store(tenant, |store| store.entries.get(id).cloned())
}

fn store_response(tenant: &str, id: String, response: StoredResponse) {
    with_tenant_store(tenant, |store| {
        if !store.entries.contains_key(&id) {
            store.order.push_back(id.clone());
        }
        store.entries.insert(id, response);
        let mut stored_bytes: usize = store.entries.values().fold(0, |total, entry| {
            total.saturating_add(
                serde_json::to_vec(&entry.messages).map_or(usize::MAX, |bytes| bytes.len()),
            )
        });
        while store.order.len() > MAX_STORED_RESPONSES || stored_bytes > MAX_STORED_RESPONSE_BYTES {
            if let Some(expired) = store.order.pop_front() {
                if let Some(removed) = store.entries.remove(&expired) {
                    let removed_bytes = serde_json::to_vec(&removed.messages)
                        .map_or(usize::MAX, |bytes| bytes.len());
                    stored_bytes = stored_bytes.saturating_sub(removed_bytes);
                }
            } else {
                break;
            }
        }
    });
}

fn delete_response(tenant: &str, id: &str) -> bool {
    with_tenant_store(tenant, |store| {
        let removed = store.entries.remove(id).is_some();
        if removed {
            store.order.retain(|entry| entry != id);
        }
        removed
    })
}

fn response_input_items(messages: &[ConversationMessage]) -> Vec<Value> {
    let mut items = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.role == "tool" {
            items.push(json!({
                "id": format!("item_{index}"),
                "type": "function_call_output",
                "call_id": message.tool_call_id,
                "output": message.content
            }));
            continue;
        }
        if message.role == "assistant" && !message.tool_calls.is_empty() {
            for call in &message.tool_calls {
                items.push(json!({
                    "id": call.id,
                    "type": "function_call",
                    "status": "completed",
                    "call_id": call.id,
                    "name": call.function.name,
                    "arguments": call.function.arguments
                }));
            }
            continue;
        }
        let content_type = if message.role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        items.push(json!({
            "id": format!("item_{index}"),
            "type": "message",
            "status": "completed",
            "role": message.role,
            "content": response_content_parts(&message.content, content_type)
        }));
    }
    items
}

fn response_content_parts(value: &Value, default_text_type: &str) -> Vec<Value> {
    if let Some(text) = value.as_str() {
        return vec![json!({
            "type": default_text_type,
            "text": text,
            "annotations": []
        })];
    }
    let Some(parts) = value.as_array() else {
        return vec![json!({
            "type": default_text_type,
            "text": value.to_string(),
            "annotations": []
        })];
    };
    parts
        .iter()
        .map(|part| {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(part_type, "text" | "input_text" | "output_text") {
                json!({
                    "type": default_text_type,
                    "text": part.get("text").and_then(Value::as_str).unwrap_or_default(),
                    "annotations": []
                })
            } else {
                part.clone()
            }
        })
        .collect()
}

fn append_response_message(
    messages: &mut Vec<ConversationMessage>,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) {
    messages.push(ConversationMessage {
        role: "assistant".to_string(),
        content: answer
            .text
            .as_ref()
            .map_or(Value::Null, |text| Value::String(text.clone())),
        tool_calls: answer
            .tool_calls
            .iter()
            .map(|call| OpenAiAssistantToolCall {
                id: call.id.clone(),
                kind: "function".to_string(),
                function: OpenAiAssistantFunction {
                    name: call.name.clone(),
                    arguments: serde_json::to_string(&call.arguments)
                        .unwrap_or_else(|_| "{}".to_string()),
                },
            })
            .collect(),
        tool_call_id: None,
    });
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
struct AnthropicRequest {
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
    #[serde(default)]
    previous_response_id: Option<String>,
    #[serde(default = "default_true")]
    store: bool,
}

#[derive(Clone, Deserialize, Serialize)]
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

#[derive(Clone, Deserialize, serde::Serialize)]
struct OpenAiAssistantToolCall {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    function: OpenAiAssistantFunction,
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct OpenAiAssistantFunction {
    name: String,
    arguments: String,
}

fn decode_json<T: DeserializeOwned>(body: &[u8]) -> Result<T, String> {
    serde_json::from_slice(body).map_err(|error| format!("Ungueltiger JSON-Body: {error}"))
}

/// Bekannte, aber nicht umsetzbare Semantikfelder: ablehnen statt still ignorieren.
fn reject_unsupported_openai_body(body: &[u8]) -> Result<(), HttpResponse> {
    let value: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };
    reject_unsupported_openai_fields(&value)
}

fn reject_unsupported_openai_fields(value: &Value) -> Result<(), HttpResponse> {
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

fn unsupported_parameter(param: &str) -> HttpResponse {
    api_error_code(
        400,
        &format!("Parameter '{param}' wird nicht unterstuetzt."),
        param,
        "unsupported_parameter",
    )
}

fn unsupported_value(param: &str, message: &str) -> HttpResponse {
    api_error_code(400, message, param, "unsupported_value")
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
fn openai_task(request: &OpenAiRequest) -> Result<String, String> {
    Ok(openai_prompt(request)?.text)
}

fn openai_prompt(request: &OpenAiRequest) -> Result<PromptBundle, String> {
    conversation_prompt(None, &request.messages)
}

fn anthropic_prompt(request: &AnthropicRequest) -> Result<PromptBundle, String> {
    let system = match &request.system {
        Some(content) => Some(text_content(content)?),
        None => None,
    };
    conversation_prompt(system, &request.messages)
}

#[cfg(test)]
fn responses_task(request: &ResponsesRequest) -> Result<String, String> {
    let messages = responses_messages(&request.input)?;
    Ok(conversation_prompt(request.instructions.clone(), &messages)?.text)
}

fn responses_messages(input: &Value) -> Result<Vec<ConversationMessage>, String> {
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

fn responses_content(value: &Value) -> Result<Value, String> {
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

fn responses_function_output(value: &Value) -> Result<Value, String> {
    if value.is_string() {
        return Ok(value.clone());
    }
    if value.is_array() {
        return responses_content(value);
    }
    Ok(Value::String(value.to_string()))
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

fn anthropic_tools(tools: &[Value]) -> Result<Vec<crate::browser_inference::BrowserTool>, String> {
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

fn anthropic_tool_choice(
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

fn require_clean_text_tools(
    tools: &[crate::browser_inference::BrowserTool],
    choice: &crate::browser_inference::BrowserToolChoice,
) -> Result<(), String> {
    if tools.is_empty() || matches!(choice, crate::browser_inference::BrowserToolChoice::None) {
        return Ok(());
    }
    Err(concat!(
        "Aktive Client-Tools sind im sauberen Browser-Textprofil noch nicht unterstuetzt. ",
        "WebAgent injiziert weder Tool-Schemas noch Skill-/System-Protokolle in die ",
        "Browser-Unterhaltung; verwaltete WebAgent-Tools folgen separat."
    )
    .to_string())
}

#[cfg(test)]
fn conversation_task(
    system: Option<String>,
    messages: &[ConversationMessage],
) -> Result<String, String> {
    Ok(conversation_prompt(system, messages)?.text)
}

fn conversation_prompt(
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
fn content_to_prompt(
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

fn append_attachment(
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

fn parse_data_url(
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

fn validate_mime(
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

fn decode_base64(encoded: &str) -> Result<Vec<u8>, String> {
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

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
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
    if !tools.iter().any(|tool| tool.name == name) {
        return Err(format!(
            "tool_choice verweist auf unbekanntes Tool '{name}'."
        ));
    }
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
        "choices": [{"index": 0, "delta": delta, "finish_reason": null, "logprobs": null}],
        "usage": null
    });
    let last = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": answer.finish_reason(), "logprobs": null}],
        "usage": null
    });
    HttpResponse::sse(format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n"))
}

fn response_object(id: &str, model: &str, text: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "created_at": unix_seconds(),
        "status": "completed",
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "max_output_tokens": null,
        "max_tool_calls": null,
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
        "previous_response_id": null,
        "prompt_cache_key": null,
        "reasoning": null,
        "store": true,
        "temperature": null,
        "tool_choice": "none",
        "tools": [],
        "top_p": null,
        "truncation": null,
        "usage": null,
        "user": null,
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
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "max_output_tokens": null,
        "max_tool_calls": null,
        "model": model,
        "output": output,
        "output_text": "",
        "parallel_tool_calls": false,
        "previous_response_id": null,
        "prompt_cache_key": null,
        "reasoning": null,
        "store": true,
        "temperature": null,
        "tool_choice": "auto",
        "tools": [],
        "top_p": null,
        "truncation": null,
        "usage": null,
        "user": null,
        "metadata": {}
    })
}

fn response_with_state(mut response: Value, previous_response_id: Option<&str>) -> Value {
    response["previous_response_id"] = previous_response_id.map_or(Value::Null, |id| json!(id));
    response
}

#[cfg(test)]
fn responses_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let response = response_with_state(response_object_from_answer(id, model, answer), None);
    responses_sse_with_object(id, model, answer, response)
}

fn responses_sse_with_object(
    id: &str,
    _model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
    completed: Value,
) -> HttpResponse {
    let text = answer.text.as_deref().unwrap_or_default();
    let mut created = completed.clone();
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created["output_text"] = json!("");
    let mut seq = 0u64;
    let mut body = format!(
        "event: response.created\ndata: {}\n\n",
        sse_data(
            "response.created",
            json!({"type":"response.created","response":created}),
            &mut seq
        )
    );
    body.push_str(&format!(
        "event: response.in_progress\ndata: {}\n\n",
        sse_data(
            "response.in_progress",
            json!({"type":"response.in_progress","response":created}),
            &mut seq
        )
    ));
    if answer.tool_calls.is_empty() {
        let item_id = format!("{id}_msg");
        body.push_str(&format!(
            "event: response.output_item.added\ndata: {}\n\n",
            sse_data("response.output_item.added", json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.content_part.added\ndata: {}\n\n",
            sse_data("response.content_part.added", json!({"type":"response.content_part.added","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_text.delta\ndata: {}\n\n",
            sse_data("response.output_text.delta", json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":text}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_text.done\ndata: {}\n\n",
            sse_data("response.output_text.done", json!({"type":"response.output_text.done","item_id":item_id,"output_index":0,"content_index":0,"text":text}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.content_part.done\ndata: {}\n\n",
            sse_data("response.content_part.done", json!({"type":"response.content_part.done","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_item.done\ndata: {}\n\n",
            sse_data("response.output_item.done", json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]}), &mut seq)
        ));
    } else {
        for (index, call) in answer.tool_calls.iter().enumerate() {
            let item = json!({"id":call.id,"type":"function_call","status":"in_progress","call_id":call.id,"name":call.name,"arguments":""});
            body.push_str(&format!(
                "event: response.output_item.added\ndata: {}\n\n",
                sse_data(
                    "response.output_item.added",
                    json!({"type":"response.output_item.added","output_index":index,"item":item}),
                    &mut seq
                )
            ));
            let arguments =
                serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
            body.push_str(&format!(
                "event: response.function_call_arguments.delta\ndata: {}\n\n",
                sse_data("response.function_call_arguments.delta", json!({"type":"response.function_call_arguments.delta","item_id":call.id,"output_index":index,"delta":arguments}), &mut seq)
            ));
            body.push_str(&format!(
                "event: response.function_call_arguments.done\ndata: {}\n\n",
                sse_data("response.function_call_arguments.done", json!({"type":"response.function_call_arguments.done","item_id":call.id,"output_index":index,"arguments":arguments}), &mut seq)
            ));
            body.push_str(&format!(
                "event: response.output_item.done\ndata: {}\n\n",
                sse_data("response.output_item.done", json!({"type":"response.output_item.done","output_index":index,"item":completed["output"][index]}), &mut seq)
            ));
        }
    }
    body.push_str(&format!(
        "event: response.completed\ndata: {}\n\n",
        sse_data(
            "response.completed",
            json!({"type":"response.completed","response":completed}),
            &mut seq
        )
    ));
    HttpResponse::sse(body)
}

fn anthropic_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let started = json!({
        "type": "message_start",
        "message": {
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": null,
            "stop_sequence": null
        }
    });
    let mut body = format!("event: message_start\ndata: {started}\n\n");
    if answer.tool_calls.is_empty() {
        let text = answer.text.as_deref().unwrap_or_default();
        let block_start = json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}});
        let delta = json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}});
        let block_stop = json!({"type": "content_block_stop", "index": 0});
        body.push_str(&format!(
            "event: content_block_start\ndata: {block_start}\n\n\
             event: content_block_delta\ndata: {delta}\n\n\
             event: content_block_stop\ndata: {block_stop}\n\n"
        ));
    } else {
        for (index, call) in answer.tool_calls.iter().enumerate() {
            let block_start = json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": call.arguments
                }
            });
            let block_stop = json!({"type": "content_block_stop", "index": index});
            body.push_str(&format!(
                "event: content_block_start\ndata: {block_start}\n\n\
                 event: content_block_stop\ndata: {block_stop}\n\n"
            ));
        }
    }
    let message_delta = json!({"type": "message_delta", "delta": {"stop_reason": if answer.tool_calls.is_empty() { "end_turn" } else { "tool_use" }, "stop_sequence": null}});
    let message_stop = json!({"type": "message_stop"});
    body.push_str(&format!(
        "event: message_delta\ndata: {message_delta}\n\n\
         event: message_stop\ndata: {message_stop}\n\n"
    ));
    HttpResponse::sse(body)
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
