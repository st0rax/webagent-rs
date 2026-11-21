//! Medienpfade der API-Bridge: Bildgenerierung, Audio und Multipart-Parsing.
//!
//! Aus `src/api_bridge.rs` herausgeloest (T-914). Die Funktionen sind mit
//! Ausnahme der Sichtbarkeit unveraendert uebernommen: keine Aenderung an
//! MIME-Grenzen, Base64- oder Upload-Semantik (T-914 non-goals). Die Root-
//! Datei enthaelt danach keine Multipart-Schleife mehr (T-921).

use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    api_error, authorize, browser_inference_error, browser_run_lock, decode_json, find_bytes,
    model_id, resolve_model, run_task_blocking, select_auto_brain, unix_seconds, ApiFlavor,
    AutoPurpose, BridgeConfig, ClientPresence, HttpRequest, HttpResponse,
};

pub(super) fn handle_image_generation(
    request: &HttpRequest,
    config: &BridgeConfig,
) -> HttpResponse {
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
        Err(error) => return browser_inference_error(ApiFlavor::OpenAi, &error),
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

pub(super) fn handle_audio_transcription(
    request: &HttpRequest,
    config: &BridgeConfig,
    translate_to_english: bool,
    presence: &dyn ClientPresence,
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
        presence,
    ) {
        Ok(answer) => answer,
        Err(error) => return browser_inference_error(ApiFlavor::OpenAi, &error),
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
            retry_after_secs: None,
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
pub(super) fn is_audio_capability_refusal(text: &str) -> bool {
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

pub(super) fn handle_audio_speech(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
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
pub(super) struct MultipartPart {
    pub(super) name: String,
    pub(super) file_name: Option<String>,
    pub(super) content_type: Option<String>,
    pub(super) data: Vec<u8>,
}

pub(super) fn multipart_text<'a>(parts: &'a [MultipartPart], name: &str) -> Option<&'a str> {
    parts
        .iter()
        .find(|part| part.name == name)
        .and_then(|part| std::str::from_utf8(&part.data).ok())
        .map(str::trim)
}

pub(super) fn multipart_parts(request: &HttpRequest) -> Result<Vec<MultipartPart>, String> {
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

pub(super) fn audio_mime(format: &str) -> Result<String, String> {
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
