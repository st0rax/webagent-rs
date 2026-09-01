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
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Request envelope limit. Multimodal data is base64-encoded in JSON, so the
/// transport needs more room than the historical text-only 1 MiB cap. Each
/// decoded attachment is still capped independently in `browser_inference`.
const MAX_REQUEST_BYTES: usize = 16 * 1024 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_CONNECTIONS: usize = 8;
const MAX_STORED_RESPONSES: usize = 256;
const MAX_STORED_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

static BROWSER_RUN_LOCKS: OnceLock<Mutex<BTreeMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
static RESPONSE_STORE: OnceLock<Mutex<ResponseStore>> = OnceLock::new();

#[derive(Clone)]
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

    if request.method == "POST"
        && request.path == "/v1/responses"
        && is_incremental_text_request(&request.body)
    {
        return handle_responses_incremental(stream, &request, config);
    }
    if request.method == "POST"
        && request.path == "/v1/chat/completions"
        && is_incremental_chat_request(&request.body)
    {
        return handle_openai_incremental(stream, &request, config);
    }

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
            }
        }
        ("GET", path) if path.starts_with("/v1/models/") => {
            if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
                response
            } else {
                let requested = path.trim_start_matches("/v1/models/");
                match resolve_model(requested, &config.brain) {
                    Ok(brain) => {
                        let mut metadata = model_metadata(&brain);
                        metadata["created"] = json!(unix_seconds());
                        HttpResponse::json(200, metadata)
                    }
                    Err(error) => api_error(ApiFlavor::OpenAi, 404, &error),
                }
            }
        }
        ("GET", path) if path.starts_with("/v1/responses/") && path.ends_with("/input_items") => {
            handle_response_input_items(&request, config, path)
        }
        ("GET", path) if path.starts_with("/v1/responses/") => {
            handle_response_retrieve(&request, config, path)
        }
        ("DELETE", path) if path.starts_with("/v1/responses/") => {
            handle_response_delete(&request, config, path)
        }
        ("POST", "/v1/chat/completions") => handle_openai(&request, config),
        ("POST", "/v1/images/generations") => handle_image_generation(&request, config),
        ("POST", "/v1/audio/transcriptions") => handle_audio_transcription(&request, config, false),
        ("POST", "/v1/audio/translations") => handle_audio_transcription(&request, config, true),
        ("POST", "/v1/audio/speech") => handle_audio_speech(&request, config),
        ("POST", "/v1/responses") => handle_responses(&request, config),
        ("POST", "/v1/messages") => handle_anthropic(&request, config),
        _ => api_error(flavor, 404, "Endpoint nicht gefunden."),
    };
    write_http_response(stream, response)
}

fn is_incremental_text_request(body: &[u8]) -> bool {
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

fn is_incremental_chat_request(body: &[u8]) -> bool {
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
    match retrieve_response(id) {
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
    if delete_response(id) {
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
    let Some(stored) = retrieve_response(id) else {
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
    let prompt = match openai_prompt(&payload) {
        Ok(prompt) => prompt,
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
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &tools,
        tool_choice,
    ) {
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

fn handle_openai_incremental(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return write_http_response(stream, response);
    }
    let payload: OpenAiRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let prompt = match openai_prompt(&payload) {
        Ok(prompt) => prompt,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let tools = openai_tools(&payload.tools)
        .map_err(|error| format!("Inkrementeller Tool-Request unerwartet: {error}"))?;
    let choice = openai_tool_choice(payload.tool_choice.as_ref(), &tools)
        .map_err(|error| format!("Inkrementeller tool_choice unerwartet: {error}"))?;
    if !tools.is_empty() && !matches!(choice, crate::browser_inference::BrowserToolChoice::None) {
        return write_http_response(stream, handle_openai(request, config));
    }

    let id = completion_id("chatcmpl");
    write_sse_headers(stream)?;
    write_data_frame(
        stream,
        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}),
    )?;

    let mut last_sent = String::new();
    let mut last_keepalive = Instant::now();
    let mut stream_error: Option<String> = None;
    let answer = {
        let mut on_update = |snapshot: &str| {
            if stream_error.is_some() {
                return;
            }
            if snapshot == last_sent {
                if last_keepalive.elapsed() >= Duration::from_secs(5) {
                    if let Err(error) = write_sse_comment(stream, "keep-alive") {
                        stream_error = Some(error);
                    } else {
                        last_keepalive = Instant::now();
                    }
                }
                return;
            }
            if let Some(delta) = snapshot.strip_prefix(&last_sent) {
                if !delta.is_empty() {
                    if let Err(error) = write_data_frame(
                        stream,
                        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"content":delta},"finish_reason":null}]}),
                    ) {
                        stream_error = Some(error);
                        return;
                    }
                }
                last_sent = snapshot.to_string();
                last_keepalive = Instant::now();
            }
        };
        run_task_streaming(
            config,
            &brain,
            &prompt.text,
            &prompt.attachments,
            &mut on_update,
        )
    };
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            if stream_error.is_none() {
                let _ = write_data_frame(
                    stream,
                    json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{},"finish_reason":"error"}],"error":{"message":error,"type":"server_error"}}),
                );
            }
            return Ok(());
        }
    };
    let text = answer.text.as_deref().unwrap_or_default();
    if let Some(delta) = text
        .strip_prefix(&last_sent)
        .filter(|delta| !delta.is_empty())
    {
        write_data_frame(
            stream,
            json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"content":delta},"finish_reason":null}]}),
        )?;
    }
    if let Some(error) = stream_error {
        return Err(error);
    }
    write_data_frame(
        stream,
        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{},"finish_reason":answer.finish_reason()}]}),
    )?;
    stream
        .write_all(b"data: [DONE]\n\n")
        .map_err(|error| format!("Chat-SSE-Abschluss nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("Chat-SSE-Abschluss nicht flushbar: {error}"))
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
    let prompt = match anthropic_prompt(&payload) {
        Ok(prompt) => prompt,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let tools = match anthropic_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let tool_choice = match anthropic_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &tools,
        tool_choice,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::Anthropic, 502, &error),
    };
    let id = completion_id("msg");
    let response = anthropic_response(&id, &payload.model, &answer);
    if payload.stream.unwrap_or(false) {
        return anthropic_sse(&id, &payload.model, &answer);
    }
    HttpResponse::json(200, response)
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
            "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
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
        "stop_sequence": null,
        "usage": {"input_tokens": 0, "output_tokens": 0}
    })
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
    let (mut messages, prompt) = match responses_context(&payload) {
        Ok(context) => context,
        Err((status, error)) => return api_error(ApiFlavor::OpenAi, status, &error),
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &tools,
        tool_choice,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("resp");
    append_response_message(&mut messages, &answer);
    let response = response_object_from_answer(&id, &payload.model, &answer);
    let response = response_with_state(response, payload.previous_response_id.as_deref());
    if payload.store {
        store_response(
            id.clone(),
            StoredResponse {
                response: response.clone(),
                messages,
            },
        );
    }
    if payload.stream.unwrap_or(false) {
        return responses_sse_with_object(&id, &payload.model, &answer, response);
    }
    HttpResponse::json(200, response)
}

fn handle_responses_incremental(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return write_http_response(stream, response);
    }
    let payload: ResponsesRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let tools = responses_tools(&payload.tools)
        .map_err(|error| format!("Inkrementeller Tool-Request unerwartet: {error}"))?;
    let choice = responses_tool_choice(payload.tool_choice.as_ref(), &tools)
        .map_err(|error| format!("Inkrementeller tool_choice unerwartet: {error}"))?;
    if !tools.is_empty() && !matches!(choice, crate::browser_inference::BrowserToolChoice::None) {
        return write_http_response(stream, handle_responses(request, config));
    }
    let (mut messages, prompt) = match responses_context(&payload) {
        Ok(context) => context,
        Err((status, error)) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, status, &error))
        }
    };

    let id = completion_id("resp");
    let item_id = format!("{id}_msg");
    let mut started = response_with_state(
        response_object(&id, &payload.model, ""),
        payload.previous_response_id.as_deref(),
    );
    started["status"] = json!("in_progress");
    started["output"] = json!([]);
    started["output_text"] = json!("");
    write_sse_headers(stream)?;
    write_sse_event(
        stream,
        "response.created",
        json!({"type":"response.created","response":started}),
    )?;
    write_sse_event(
        stream,
        "response.in_progress",
        json!({"type":"response.in_progress","response":started}),
    )?;
    write_sse_event(
        stream,
        "response.output_item.added",
        json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]}}),
    )?;
    write_sse_event(
        stream,
        "response.content_part.added",
        json!({"type":"response.content_part.added","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
    )?;

    let mut last_sent = String::new();
    let mut last_keepalive = Instant::now();
    let mut stream_error: Option<String> = None;
    let answer = {
        let mut on_update = |snapshot: &str| {
            if stream_error.is_some() {
                return;
            }
            if snapshot == last_sent {
                if last_keepalive.elapsed() >= Duration::from_secs(5) {
                    if let Err(error) = write_sse_comment(stream, "keep-alive") {
                        stream_error = Some(error);
                    } else {
                        last_keepalive = Instant::now();
                    }
                }
                return;
            }
            if let Some(delta) = snapshot.strip_prefix(&last_sent) {
                if !delta.is_empty() {
                    if let Err(error) = write_sse_event(
                        stream,
                        "response.output_text.delta",
                        json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":delta}),
                    ) {
                        stream_error = Some(error);
                        return;
                    }
                }
                last_sent = snapshot.to_string();
                last_keepalive = Instant::now();
            }
        };
        run_task_streaming(
            config,
            &brain,
            &prompt.text,
            &prompt.attachments,
            &mut on_update,
        )
    };

    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            let mut failed = started;
            failed["status"] = json!("failed");
            failed["error"] = json!({"code":"server_error","message":error});
            if stream_error.is_none() {
                write_sse_event(
                    stream,
                    "response.failed",
                    json!({"type":"response.failed","response":failed}),
                )?;
            }
            return Ok(());
        }
    };
    let text = answer.text.as_deref().unwrap_or_default();
    if let Some(delta) = text
        .strip_prefix(&last_sent)
        .filter(|delta| !delta.is_empty())
    {
        write_sse_event(
            stream,
            "response.output_text.delta",
            json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":delta}),
        )?;
    }
    if let Some(error) = stream_error {
        return Err(error);
    }

    append_response_message(&mut messages, &answer);
    let completed = response_with_state(
        response_object_from_answer(&id, &payload.model, &answer),
        payload.previous_response_id.as_deref(),
    );
    if payload.store {
        store_response(
            id.clone(),
            StoredResponse {
                response: completed.clone(),
                messages,
            },
        );
    }
    write_sse_event(
        stream,
        "response.output_text.done",
        json!({"type":"response.output_text.done","item_id":item_id,"output_index":0,"content_index":0,"text":text}),
    )?;
    write_sse_event(
        stream,
        "response.content_part.done",
        json!({"type":"response.content_part.done","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}}),
    )?;
    write_sse_event(
        stream,
        "response.output_item.done",
        json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]}),
    )?;
    write_sse_event(
        stream,
        "response.completed",
        json!({"type":"response.completed","response":completed}),
    )
}

fn write_sse_headers(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
        )
        .map_err(|error| format!("SSE-Header nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Header nicht flushbar: {error}"))
}

fn write_sse_event(stream: &mut TcpStream, event: &str, data: Value) -> Result<(), String> {
    let frame = format!("event: {event}\ndata: {data}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Event nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Event nicht flushbar: {error}"))
}

fn write_data_frame(stream: &mut TcpStream, data: Value) -> Result<(), String> {
    let frame = format!("data: {data}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Datenframe nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Datenframe nicht flushbar: {error}"))
}

fn write_sse_comment(stream: &mut TcpStream, comment: &str) -> Result<(), String> {
    let frame = format!(": {comment}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Keepalive nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Keepalive nicht flushbar: {error}"))
}

fn responses_context(
    payload: &ResponsesRequest,
) -> Result<(Vec<ConversationMessage>, PromptBundle), (u16, String)> {
    let mut messages = match payload.previous_response_id.as_deref() {
        Some(id) => retrieve_response(id).map_or_else(
            || Err((404, format!("Previous response '{id}' nicht gefunden."))),
            |stored| Ok(stored.messages),
        )?,
        None => Vec::new(),
    };
    messages.extend(responses_messages(&payload.input).map_err(|error| (400, error))?);
    let prompt = conversation_prompt(payload.instructions.clone(), &messages, "OpenAI Responses")
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

fn run_task_blocking(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    tools: &[crate::browser_inference::BrowserTool],
    tool_choice: crate::browser_inference::BrowserToolChoice,
) -> Result<crate::browser_inference::BrowserInferenceResponse, String> {
    let brain = if brain == "auto" {
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
    let lock = BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone();
    let _browser_run = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    crate::browser_inference::complete_with_attachments(
        crate::browser_inference::BrowserInferenceRequest {
            brain: &brain,
            prompt: task,
            tools,
            tool_choice,
            headless: config.headless,
            timeout_secs: config.timeout_secs,
            model: None,
        },
        attachments,
        &mut |_| {},
    )
    .map_err(|error| format!("Browser-Inference fehlgeschlagen: {error}"))
}

fn run_task_streaming(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<crate::browser_inference::BrowserInferenceResponse, String> {
    let brain = if brain == "auto" {
        select_auto_brain(config, task, attachments, false, AutoPurpose::Chat)?
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

    crate::browser_inference::complete_streaming_with_attachments(
        crate::browser_inference::BrowserInferenceRequest {
            brain: &brain,
            prompt: task,
            tools: &[],
            tool_choice: crate::browser_inference::BrowserToolChoice::None,
            headless: config.headless,
            timeout_secs: config.timeout_secs,
            model: None,
        },
        attachments,
        on_update,
    )
    .map_err(|error| format!("Browser-Inference fehlgeschlagen: {error}"))
}

fn response_store() -> &'static Mutex<ResponseStore> {
    RESPONSE_STORE.get_or_init(|| Mutex::new(ResponseStore::default()))
}

fn retrieve_response(id: &str) -> Option<StoredResponse> {
    response_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entries
        .get(id)
        .cloned()
}

fn store_response(id: String, response: StoredResponse) {
    let mut store = response_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
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
                let removed_bytes =
                    serde_json::to_vec(&removed.messages).map_or(usize::MAX, |bytes| bytes.len());
                stored_bytes = stored_bytes.saturating_sub(removed_bytes);
            }
        } else {
            break;
        }
    }
}

fn delete_response(id: &str) -> bool {
    let mut store = response_store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let removed = store.entries.remove(id).is_some();
    if removed {
        store.order.retain(|entry| entry != id);
    }
    removed
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

fn default_true() -> bool {
    true
}

#[cfg(test)]
fn openai_task(request: &OpenAiRequest) -> Result<String, String> {
    Ok(openai_prompt(request)?.text)
}

fn openai_prompt(request: &OpenAiRequest) -> Result<PromptBundle, String> {
    conversation_prompt(None, &request.messages, "OpenAI Chat Completions")
}

fn anthropic_prompt(request: &AnthropicRequest) -> Result<PromptBundle, String> {
    let system = match &request.system {
        Some(content) => Some(text_content(content)?),
        None => None,
    };
    conversation_prompt(system, &request.messages, "Anthropic Messages")
}

#[cfg(test)]
fn responses_task(request: &ResponsesRequest) -> Result<String, String> {
    let messages = responses_messages(&request.input)?;
    Ok(conversation_prompt(request.instructions.clone(), &messages, "OpenAI Responses")?.text)
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

#[cfg(test)]
fn conversation_task(
    system: Option<String>,
    messages: &[ConversationMessage],
    source: &str,
) -> Result<String, String> {
    Ok(conversation_prompt(system, messages, source)?.text)
}

fn conversation_prompt(
    system: Option<String>,
    messages: &[ConversationMessage],
    _source: &str,
) -> Result<PromptBundle, String> {
    if messages.is_empty() {
        return Err("messages darf nicht leer sein.".to_string());
    }
    // Der Browser kann keine echte providerseitige system-Rolle setzen. Die
    // Einleitung muss deshalb wie eine neutrale Transcript-Anweisung wirken:
    // Begriffe wie "Provider-Bridge" oder WEBAGENT/1 laden das Modell sonst
    // dazu ein, ueber Transport und Identitaet zu diskutieren, statt die
    // eigentliche Nutzerfrage zu beantworten.
    let mut task = String::from(concat!(
        "Behandle den folgenden Inhalt als Gespraechsverlauf eines API-Clients. ",
        "Beantworte die letzte Nutzeranfrage direkt. Gib ausschliesslich die Antwort fuer den API-Client zurueck und erwaehne weder Browser, Transport, Provider noch interne Protokolle.\n\n",
    ));
    if let Some(system) = system.filter(|value| !value.trim().is_empty()) {
        task.push_str("[system]\n");
        task.push_str(&system);
        task.push_str("\n\n");
    }
    let mut attachments = Vec::new();
    for message in messages {
        match message.role.as_str() {
            "system" | "developer" | "user" => {
                if !message.tool_calls.is_empty() || message.tool_call_id.is_some() {
                    return Err(format!(
                        "Rolle '{}' darf keine Tool-Call-Felder enthalten.",
                        message.role
                    ));
                }
                let content = content_to_prompt(&message.content, &mut attachments)?;
                task.push_str(&format!("[{}]\n{}\n\n", message.role, content));
            }
            "assistant" => {
                if message.tool_call_id.is_some() {
                    return Err("Assistant-Nachricht darf keine tool_call_id tragen.".to_string());
                }
                if !message.content.is_null() {
                    let content = content_to_prompt(&message.content, &mut attachments)?;
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
                let content = content_to_prompt(&message.content, &mut attachments)?;
                task.push_str(&format!("[tool id={id}]\n{content}\n\n"));
            }
            other => {
                return Err(format!(
                    "Rolle '{other}' wird von der Text-Bridge nicht unterstuetzt."
                ))
            }
        }
    }
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
    for chunk in bytes.chunks_exact(4) {
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

fn available_brains() -> Vec<String> {
    let mut brains: Vec<String> = crate::config::brains().into_keys().collect();
    brains.sort();
    brains
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoPurpose {
    Chat,
    ImageGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoRoute {
    Default,
    AudioInput,
    ImageInput,
    ImageGeneration,
    Tools,
    Coding,
    CurrentResearch,
}

fn classify_auto_route(
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

fn first_available_auto_brain(preferences: &[&str]) -> Option<String> {
    let available = available_brains();
    preferences
        .iter()
        .find(|brain| {
            available.iter().any(|candidate| candidate == **brain)
                && crate::circuit_breaker::check(brain).is_none()
        })
        .map(|brain| (*brain).to_string())
}

fn select_auto_brain(
    config: &BridgeConfig,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    has_tools: bool,
    purpose: AutoPurpose,
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
            let default = if config.brain == "auto" {
                "chatgpt"
            } else {
                config.brain.as_str()
            };
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
    } else {
        requested
            .strip_prefix("webagent/")
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
    let mut body = format!(
        "event: response.created\ndata: {}\n\n",
        json!({"type":"response.created","response":created})
    );
    body.push_str(&format!(
        "event: response.in_progress\ndata: {}\n\n",
        json!({"type":"response.in_progress","response":created})
    ));
    if answer.tool_calls.is_empty() {
        let item_id = format!("{id}_msg");
        body.push_str(&format!(
            "event: response.output_item.added\ndata: {}\n\n",
            json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]}})
        ));
        body.push_str(&format!(
            "event: response.content_part.added\ndata: {}\n\n",
            json!({"type":"response.content_part.added","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}})
        ));
        body.push_str(&format!(
            "event: response.output_text.delta\ndata: {}\n\n",
            json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":text})
        ));
        body.push_str(&format!(
            "event: response.output_text.done\ndata: {}\n\n",
            json!({"type":"response.output_text.done","item_id":item_id,"output_index":0,"content_index":0,"text":text})
        ));
        body.push_str(&format!(
            "event: response.content_part.done\ndata: {}\n\n",
            json!({"type":"response.content_part.done","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}})
        ));
        body.push_str(&format!(
            "event: response.output_item.done\ndata: {}\n\n",
            json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]})
        ));
    } else {
        for (index, call) in answer.tool_calls.iter().enumerate() {
            let item = json!({"id":call.id,"type":"function_call","status":"in_progress","call_id":call.id,"name":call.name,"arguments":""});
            body.push_str(&format!(
                "event: response.output_item.added\ndata: {}\n\n",
                json!({"type":"response.output_item.added","output_index":index,"item":item})
            ));
            let arguments =
                serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
            body.push_str(&format!(
                "event: response.function_call_arguments.delta\ndata: {}\n\n",
                json!({"type":"response.function_call_arguments.delta","item_id":call.id,"output_index":index,"delta":arguments})
            ));
            body.push_str(&format!(
                "event: response.function_call_arguments.done\ndata: {}\n\n",
                json!({"type":"response.function_call_arguments.done","item_id":call.id,"output_index":index,"arguments":arguments})
            ));
            body.push_str(&format!(
                "event: response.output_item.done\ndata: {}\n\n",
                json!({"type":"response.output_item.done","output_index":index,"item":completed["output"][index]})
            ));
        }
    }
    body.push_str(&format!(
        "event: response.completed\ndata: {}\n\n",
        json!({"type":"response.completed","response":completed})
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
            "stop_sequence": null,
            "usage": {"input_tokens": 0, "output_tokens": 0}
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
    let message_delta = json!({"type": "message_delta", "delta": {"stop_reason": if answer.tool_calls.is_empty() { "end_turn" } else { "tool_use" }, "stop_sequence": null}, "usage": {"output_tokens": 0}});
    let message_stop = json!({"type": "message_stop"});
    body.push_str(&format!(
        "event: message_delta\ndata: {message_delta}\n\n\
         event: message_stop\ndata: {message_stop}\n\n"
    ));
    HttpResponse::sse(body)
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
    fn browser_prompt_does_not_expose_transport_identity() {
        let task = conversation_task(
            None,
            &[ConversationMessage {
                role: "user".to_string(),
                content: json!("Hallo"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
            "Anthropic Messages",
        )
        .unwrap();

        assert!(task.contains("Beantworte die letzte Nutzeranfrage direkt"));
        assert!(!task.contains("Provider-Bridge"));
        assert!(!task.contains("WEBAGENT_INFERENCE/1"));
    }

    #[test]
    fn extracts_openai_image_and_audio_parts_for_browser_upload() {
        let request = OpenAiRequest {
            model: "webagent/chatgpt".to_string(),
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            messages: vec![ConversationMessage {
                role: "user".to_string(),
                content: json!([
                    {"type":"text","text":"Beschreibe die Dateien:"},
                    {"type":"image_url","image_url":{"url":"data:image/png;base64,AQI="}},
                    {"type":"input_audio","input_audio":{"data":"SGk=","format":"wav"}}
                ]),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }],
        };
        let prompt = openai_prompt(&request).unwrap();
        assert!(prompt
            .text
            .contains("[image attachment: image/png, 2 bytes]"));
        assert!(prompt
            .text
            .contains("[audio attachment: audio/wav, 2 bytes]"));
        assert_eq!(prompt.attachments.len(), 2);
        assert_eq!(prompt.attachments[0].file_name, "image-1.png");
        assert_eq!(prompt.attachments[0].data, vec![1, 2]);
        assert_eq!(prompt.attachments[1].file_name, "audio-2.wav");
        assert_eq!(prompt.attachments[1].data, b"Hi");
    }

    #[test]
    fn extracts_anthropic_base64_image_part() {
        let content = json!([{
            "type":"image",
            "source":{"type":"base64","media_type":"image/jpeg","data":"AQI="}
        }]);
        let mut attachments = Vec::new();
        let text = content_to_prompt(&content, &mut attachments).unwrap();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].mime_type, "image/jpeg");
        assert_eq!(attachments[0].data, vec![1, 2]);
        assert!(text.contains("image attachment"));
    }

    #[test]
    fn remote_media_and_invalid_base64_fail_closed() {
        let mut attachments = Vec::new();
        let error = content_to_prompt(
            &json!([{"type":"image_url","image_url":"https://example.invalid/x.png"}]),
            &mut attachments,
        )
        .unwrap_err();
        assert!(error.contains("data-URL"));
        assert!(decode_base64("not-base64!").is_err());
        assert!(decode_base64("a===").is_err());
    }

    #[test]
    fn response_input_items_keep_multimodal_content_shape() {
        let messages = vec![ConversationMessage {
            role: "user".to_string(),
            content: json!([
                {"type":"input_text","text":"Sehen"},
                {"type":"input_image","image_url":"data:image/png;base64,AQI="}
            ]),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }];
        let items = response_input_items(&messages);
        assert_eq!(items[0]["content"][0]["type"], "input_text");
        assert_eq!(items[0]["content"][1]["type"], "input_image");
        assert_eq!(
            items[0]["content"][1]["image_url"],
            "data:image/png;base64,AQI="
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
        assert_eq!(resolve_model("webagent/auto", "chatgpt").unwrap(), "auto");
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
    fn model_catalog_is_conservative_about_unverified_media_inputs() {
        assert_eq!(advertised_input_modalities("chatgpt"), ["text", "image"]);
        assert_eq!(advertised_input_modalities("claude"), ["text", "image"]);
        // Live bestätigt (docs/CURRENT_WORK.md): gemini Bild+Audio, deepseek/
        // kimi/mistral Bild. qwen/zai/perplexity haben keinen Media-Smoke.
        assert_eq!(
            advertised_input_modalities("gemini"),
            ["text", "image", "audio"]
        );
        for brain in ["deepseek", "kimi", "mistral"] {
            assert_eq!(
                advertised_input_modalities(brain),
                ["text", "image"],
                "{brain} Bild-Input ist live bestätigt"
            );
        }
        for brain in ["qwen", "zai", "perplexity"] {
            assert_eq!(
                advertised_input_modalities(brain),
                ["text"],
                "{brain} darf Medien nicht als belegt melden"
            );
        }
    }

    #[test]
    fn model_output_modalities_are_advertised_correctly() {
        assert_eq!(advertised_output_modalities("auto"), ["text", "image"]);
        assert_eq!(advertised_output_modalities("chatgpt"), ["text", "image"]);
        for brain in [
            "claude",
            "deepseek",
            "gemini",
            "kimi",
            "mistral",
            "qwen",
            "zai",
            "perplexity",
        ] {
            assert_eq!(
                advertised_output_modalities(brain),
                ["text"],
                "{brain} hat keine verifizierte Bild-Generation"
            );
        }
    }

    #[test]
    fn model_metadata_has_stable_catalog_shape() {
        let auto = model_metadata("auto");
        assert_eq!(auto["id"], "webagent/auto");
        assert_eq!(auto["virtual"], true);
        assert_eq!(auto["routing"]["audio"], "gemini");
        assert_eq!(auto["modalities"]["input"][2], "audio");
        let meta = model_metadata("chatgpt");
        assert_eq!(meta["id"], "webagent/chatgpt");
        assert_eq!(meta["context_window"], 128000);
        assert_eq!(meta["max_tokens"], 16384);
        assert_eq!(meta["modalities"]["output"][1], "image");
        let meta = model_metadata("kimi");
        assert_eq!(meta["modalities"]["input"][1], "image");
        assert_eq!(meta["modalities"]["output"][0], "text");
        assert!(meta["modalities"]["output"].as_array().unwrap().len() == 1);
    }

    #[test]
    fn auto_router_classifies_requests_deterministically() {
        use crate::browser_inference::{BrowserAttachment, BrowserAttachmentKind};

        let audio = BrowserAttachment {
            kind: BrowserAttachmentKind::Audio,
            file_name: "sample.wav".to_string(),
            mime_type: "audio/wav".to_string(),
            data: vec![1, 2, 3],
        };
        let image = BrowserAttachment {
            kind: BrowserAttachmentKind::Image,
            file_name: "sample.png".to_string(),
            mime_type: "image/png".to_string(),
            data: vec![4, 5, 6],
        };

        assert_eq!(
            classify_auto_route("anything", &[], false, AutoPurpose::ImageGeneration),
            AutoRoute::ImageGeneration
        );
        assert_eq!(
            classify_auto_route("debug Rust code", &[audio], true, AutoPurpose::Chat),
            AutoRoute::AudioInput,
            "media routing must outrank tools and text heuristics"
        );
        assert_eq!(
            classify_auto_route("debug Rust code", &[image], true, AutoPurpose::Chat),
            AutoRoute::ImageInput
        );
        assert_eq!(
            classify_auto_route("debug Rust code", &[], true, AutoPurpose::Chat),
            AutoRoute::Tools
        );
        assert_eq!(
            classify_auto_route(
                "Please debug this Rust function",
                &[],
                false,
                AutoPurpose::Chat
            ),
            AutoRoute::Coding
        );
        assert_eq!(
            classify_auto_route(
                "Suche aktuelle Quellen im Internet",
                &[],
                false,
                AutoPurpose::Chat
            ),
            AutoRoute::CurrentResearch
        );
        assert_eq!(
            classify_auto_route("Sag einfach hallo", &[], false, AutoPurpose::Chat),
            AutoRoute::Default
        );
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
        assert!(prompt.contains("Beantworte die letzte Nutzeranfrage direkt"));
        assert!(!prompt.contains("WEBAGENT_INFERENCE/1"));
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
        assert!(openai_tool_choice(
            Some(&json!({"type":"function","function":{"name":"missing"}})),
            &tools
        )
        .is_err());
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
            previous_response_id: None,
            store: true,
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
            previous_response_id: None,
            store: true,
        };
        assert!(responses_task(&message_request)
            .unwrap()
            .contains("[user]\nPing"));

        let block_request = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!([{"role":"user","content":[{"type":"input_text","text":"Teil 1"},{"type":"input_text","text":" Teil 2"}]}]),
            instructions: None,
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            previous_response_id: None,
            store: true,
        };
        assert!(responses_task(&block_request)
            .unwrap()
            .contains("[user]\nTeil 1 Teil 2"));
        assert!(responses_content(
            &json!([{"type":"input_image","image_url":"data:image/png;base64,SGk="}])
        )
        .is_ok());
        let remote_image_request = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!([{"role":"user","content":[{"type":"input_image","image_url":"https://example.invalid/x"}]}]),
            instructions: None,
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            previous_response_id: None,
            store: true,
        };
        assert!(responses_task(&remote_image_request).is_err());
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
        assert!(
            sse.find("event: response.output_item.added").unwrap()
                < sse.find("event: response.content_part.added").unwrap()
        );
        assert!(
            sse.find("event: response.content_part.added").unwrap()
                < sse.find("event: response.output_text.delta").unwrap()
        );
    }

    #[test]
    fn responses_tools_and_choice_follow_responses_shape() {
        let tools = responses_tools(&[json!({
            "type": "function",
            "name": "read_file",
            "description": "Read a file",
            "parameters": {"type": "object"}
        })])
        .unwrap();
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description.as_deref(), Some("Read a file"));
        assert_eq!(
            responses_tool_choice(Some(&json!({"type":"function","name":"read_file"})), &tools)
                .unwrap(),
            crate::browser_inference::BrowserToolChoice::Function("read_file".to_string())
        );
        assert_eq!(
            responses_tool_choice(Some(&json!("required")), &tools).unwrap(),
            crate::browser_inference::BrowserToolChoice::Required
        );
        assert!(responses_tools(&[json!({"type":"computer"})]).is_err());
        assert!(responses_tool_choice(Some(&json!("read_file")), &tools).is_err());
    }

    #[test]
    fn anthropic_tools_normalizes_input_schema() {
        let tools = anthropic_tools(&[json!({
            "name": "read_file",
            "description": "Datei lesen",
            "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
        })])
        .unwrap();
        assert_eq!(tools[0].name, "read_file");
        assert_eq!(tools[0].description.as_deref(), Some("Datei lesen"));
        assert_eq!(tools[0].parameters["type"], "object");
        assert_eq!(tools[0].parameters["properties"]["path"]["type"], "string");

        let fallback =
            anthropic_tools(&[json!({"name": "search", "parameters": {"type": "object"}})])
                .unwrap();
        assert_eq!(fallback[0].name, "search");
        assert_eq!(fallback[0].parameters["type"], "object");

        let empty_schema = anthropic_tools(&[json!({"name": "noop"})]).unwrap();
        assert_eq!(empty_schema[0].parameters, json!({}));
        assert!(anthropic_tools(&[json!({})]).is_err());
        assert!(anthropic_tools(&[json!({"name": ""})]).is_err());
        assert!(anthropic_tools(&[json!("not-an-object")]).is_err());
    }

    #[test]
    fn anthropic_tool_choice_maps_anthropic_forms() {
        let tools = anthropic_tools(&[json!({
            "name": "read_file",
            "input_schema": {"type": "object"}
        })])
        .unwrap();
        assert_eq!(
            anthropic_tool_choice(Some(&json!({"type": "auto"})), &tools).unwrap(),
            crate::browser_inference::BrowserToolChoice::Auto
        );
        assert_eq!(
            anthropic_tool_choice(Some(&json!({"type": "any"})), &tools).unwrap(),
            crate::browser_inference::BrowserToolChoice::Required
        );
        assert_eq!(
            anthropic_tool_choice(Some(&json!({"type": "tool", "name": "read_file"})), &tools)
                .unwrap(),
            crate::browser_inference::BrowserToolChoice::Function("read_file".to_string())
        );
        assert_eq!(
            anthropic_tool_choice(None, &tools).unwrap(),
            crate::browser_inference::BrowserToolChoice::Auto
        );
        assert_eq!(
            anthropic_tool_choice(None, &[]).unwrap(),
            crate::browser_inference::BrowserToolChoice::None
        );
        assert!(
            anthropic_tool_choice(Some(&json!({"type": "tool", "name": "missing"})), &tools)
                .is_err()
        );
        assert!(anthropic_tool_choice(Some(&json!("auto")), &tools).is_err());
        assert!(anthropic_tool_choice(Some(&json!({"type": "unknown"})), &tools).is_err());
        assert!(anthropic_tool_choice(Some(&json!({})), &tools).is_err());
    }

    #[test]
    fn anthropic_response_renders_tool_use_blocks() {
        let answer = crate::browser_inference::BrowserInferenceResponse {
            text: None,
            tool_calls: vec![crate::browser_inference::BrowserToolCall {
                id: "toolu_1".to_string(),
                name: "read_file".to_string(),
                arguments: json!({"path": "README.md"}),
            }],
        };
        let response = anthropic_response("msg_1", "claude-3-5-sonnet", &answer);
        assert_eq!(response["type"], "message");
        assert_eq!(response["stop_reason"], "tool_use");
        assert_eq!(response["content"][0]["type"], "tool_use");
        assert_eq!(response["content"][0]["id"], "toolu_1");
        assert_eq!(response["content"][0]["name"], "read_file");
        assert_eq!(response["content"][0]["input"]["path"], "README.md");

        let text_answer = crate::browser_inference::BrowserInferenceResponse {
            text: Some("Hallo".to_string()),
            tool_calls: Vec::new(),
        };
        let text_response = anthropic_response("msg_2", "claude-3-5-sonnet", &text_answer);
        assert_eq!(text_response["stop_reason"], "end_turn");
        assert_eq!(text_response["content"][0]["type"], "text");
        assert_eq!(text_response["content"][0]["text"], "Hallo");
    }

    #[test]
    fn responses_task_accepts_function_call_output_items() {
        let request = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!([{"type":"function_call_output","call_id":"call_7","output":{"ok":true}}]),
            instructions: None,
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            previous_response_id: None,
            store: true,
        };
        let task = responses_task(&request).unwrap();
        assert!(task.contains("[tool id=call_7]\n{\"ok\":true}"));

        let continuation = ResponsesRequest {
            model: "webagent/chatgpt".to_string(),
            input: json!([{
                "type":"function_call",
                "call_id":"call_7",
                "name":"read_file",
                "arguments":{"path":"README.md"}
            }, {
                "type":"function_call_output",
                "call_id":"call_7",
                "output":[{"type":"input_text","text":"Dateiinhalt"}]
            }]),
            instructions: None,
            stream: None,
            tools: Vec::new(),
            tool_choice: None,
            previous_response_id: None,
            store: true,
        };
        let continuation_task = responses_task(&continuation).unwrap();
        assert!(continuation_task.contains("[assistant tool_calls]"));
        assert!(continuation_task.contains("read_file"));
        assert!(continuation_task.contains("[tool id=call_7]\nDateiinhalt"));
    }

    #[test]
    fn responses_tool_call_sse_uses_output_item_events() {
        let answer = crate::browser_inference::BrowserInferenceResponse {
            text: None,
            tool_calls: vec![crate::browser_inference::BrowserToolCall {
                id: "call_8".to_string(),
                name: "read_file".to_string(),
                arguments: json!({"path":"README.md"}),
            }],
        };
        let sse = String::from_utf8(responses_sse("resp_tool", "webagent/chatgpt", &answer).body)
            .unwrap();
        assert!(sse.contains("response.output_item.added"));
        assert!(sse.contains("response.function_call_arguments.delta"));
        assert!(sse.contains("response.function_call_arguments.done"));
        assert!(sse.contains("response.output_item.done"));
        assert!(!sse.contains("response.output_text.delta"));
    }

    #[test]
    fn responses_state_is_stored_and_can_extend_context() {
        let id = "resp_state_contract".to_string();
        let mut messages = responses_messages(&json!("Mein Codewort ist Otter.")).unwrap();
        append_response_message(
            &mut messages,
            &crate::browser_inference::BrowserInferenceResponse {
                text: Some("Verstanden.".to_string()),
                tool_calls: Vec::new(),
            },
        );
        let response = response_with_state(
            response_object("resp_state_contract", "webagent/chatgpt", "Verstanden."),
            None,
        );
        store_response(
            id.clone(),
            StoredResponse {
                response: response.clone(),
                messages,
            },
        );

        let mut stored = retrieve_response(&id).unwrap();
        stored
            .messages
            .extend(responses_messages(&json!("Wie lautet es?")).unwrap());
        let task = conversation_task(None, &stored.messages, "OpenAI Responses").unwrap();
        assert!(task.contains("[user]\nMein Codewort ist Otter."));
        assert!(task.contains("[assistant]\nVerstanden."));
        assert!(task.contains("[user]\nWie lautet es?"));
        assert!(response["previous_response_id"].is_null());
    }

    #[test]
    fn responses_request_defaults_to_stored() {
        let request: ResponsesRequest = serde_json::from_value(json!({
            "model": "webagent/chatgpt",
            "input": "Hallo"
        }))
        .unwrap();
        assert!(request.store);
        assert!(request.previous_response_id.is_none());

        let chained = response_with_state(
            response_object("resp_child", "webagent/chatgpt", "OK"),
            Some("resp_parent"),
        );
        assert_eq!(chained["previous_response_id"], "resp_parent");
    }

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
    }

    #[test]
    fn response_retrieval_requires_auth_and_returns_stored_object() {
        let id = "resp_retrieve_contract";
        store_response(
            id.to_string(),
            StoredResponse {
                response: response_with_state(
                    response_object(id, "webagent/chatgpt", "Gespeichert"),
                    None,
                ),
                messages: Vec::new(),
            },
        );
        let config = BridgeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            brain: "chatgpt".to_string(),
            timeout_secs: None,
            headless: true,
            api_key: "test-secret".to_string(),
        };
        let authorized = HttpRequest {
            method: "GET".to_string(),
            path: format!("/v1/responses/{id}"),
            headers: BTreeMap::from([(
                "authorization".to_string(),
                "Bearer test-secret".to_string(),
            )]),
            body: Vec::new(),
        };
        let response = handle_response_retrieve(&authorized, &config, &authorized.path);
        assert_eq!(response.status, 200);
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["id"], id);
        assert_eq!(body["output_text"], "Gespeichert");

        let unauthorized = HttpRequest {
            headers: BTreeMap::new(),
            ..authorized
        };
        assert_eq!(
            handle_response_retrieve(&unauthorized, &config, &unauthorized.path).status,
            401
        );
    }

    #[test]
    fn response_input_items_and_delete_follow_lifecycle_contract() {
        let messages = vec![
            ConversationMessage {
                role: "user".to_string(),
                content: json!("Hallo"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            },
            ConversationMessage {
                role: "assistant".to_string(),
                content: Value::Null,
                tool_calls: vec![OpenAiAssistantToolCall {
                    id: "call_lifecycle".to_string(),
                    kind: "function".to_string(),
                    function: OpenAiAssistantFunction {
                        name: "read_file".to_string(),
                        arguments: "{\"path\":\"README.md\"}".to_string(),
                    },
                }],
                tool_call_id: None,
            },
            ConversationMessage {
                role: "tool".to_string(),
                content: json!("Inhalt"),
                tool_calls: Vec::new(),
                tool_call_id: Some("call_lifecycle".to_string()),
            },
        ];
        let items = response_input_items(&messages);
        assert_eq!(items[0]["type"], "message");
        assert_eq!(items[0]["content"][0]["type"], "input_text");
        assert_eq!(items[1]["type"], "function_call");
        assert_eq!(items[2]["type"], "function_call_output");
        let id = "resp_delete_contract";
        store_response(
            id.to_string(),
            StoredResponse {
                response: response_object(id, "webagent/chatgpt", "x"),
                messages,
            },
        );
        assert!(delete_response(id));
        assert!(retrieve_response(id).is_none());
        assert!(!delete_response(id));
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
    fn multipart_audio_request_preserves_binary_file_and_fields() {
        let boundary = "webagent-test-boundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwebagent/gemini\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\ntext\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(&[0, 1, 2, 255]);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/v1/audio/transcriptions".to_string(),
            headers: BTreeMap::from([(
                "content-type".to_string(),
                format!("multipart/form-data; boundary={boundary}"),
            )]),
            body,
        };

        let parts = multipart_parts(&request).unwrap();
        assert_eq!(multipart_text(&parts, "model"), Some("webagent/gemini"));
        assert_eq!(multipart_text(&parts, "response_format"), Some("text"));
        let file = parts.iter().find(|part| part.name == "file").unwrap();
        assert_eq!(file.file_name.as_deref(), Some("voice.wav"));
        assert_eq!(file.content_type.as_deref(), Some("audio/wav"));
        assert_eq!(file.data, [0, 1, 2, 255]);
    }

    #[test]
    fn multipart_audio_request_requires_boundary() {
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/v1/audio/transcriptions".to_string(),
            headers: BTreeMap::from([(
                "content-type".to_string(),
                "multipart/form-data".to_string(),
            )]),
            body: Vec::new(),
        };
        assert!(multipart_parts(&request).unwrap_err().contains("boundary"));
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
