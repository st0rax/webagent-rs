//! API-Bridge-Medienhandler (Bild/Audio/Multipart).
//!
//! # Modulgrenze
//!
//! Dieses Modul enthaelt die Handler fuer Bildgenerierung, Audio-Transkription/
//! Translation/Speech sowie das Multipart-Parsing. Es aendert keine MIME-Grenzen,
//! Base64- oder Upload-Semantik und keine Browser-Inferenz. Auth, Routing,
//! Modellauffloesung und Transport bleiben in `src/api_bridge.rs` bzw. den
//! Schwestermodulen; T-914 verdrahtet `mod media`.

use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    api_error, authorize, decode_json, find_bytes, model_id, resolve_model,
    run_image_generation_blocking, run_task_blocking, unix_seconds, ApiFlavor, BridgeConfig,
    HttpRequest, HttpResponse,
};

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

/// Handler fuer `/v1/images/generations` (OpenAI-kompatibel).
pub fn handle_image_generation(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
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

/// Handler fuer `/v1/audio/transcriptions` bzw. `/v1/audio/translations`.
pub fn handle_audio_transcription(
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
pub(crate) fn is_audio_capability_refusal(text: &str) -> bool {
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

/// Handler fuer `/v1/audio/speech` (OpenAI-kompatibel).
pub fn handle_audio_speech(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
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
pub struct MultipartPart {
    pub name: String,
    pub file_name: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

pub fn multipart_text<'a>(parts: &'a [MultipartPart], name: &str) -> Option<&'a str> {
    parts
        .iter()
        .find(|part| part.name == name)
        .and_then(|part| std::str::from_utf8(&part.data).ok())
        .map(str::trim)
}

pub fn multipart_parts(request: &HttpRequest) -> Result<Vec<MultipartPart>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn multipart_parts_preserves_binary_file_and_text_fields() {
        let boundary = "webagent-media-test-boundary";
        let mut body = format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwebagent/gemini\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(&[0, 1, 2, 255]);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/v1/audio/transcriptions".to_string(),
            query: String::new(),
            headers: BTreeMap::from([(
                "content-type".to_string(),
                format!("multipart/form-data; boundary={boundary}"),
            )]),
            body,
        };

        let parts = multipart_parts(&request).unwrap();
        assert_eq!(multipart_text(&parts, "model"), Some("webagent/gemini"));
        let file = parts.iter().find(|part| part.name == "file").unwrap();
        assert_eq!(file.file_name.as_deref(), Some("voice.wav"));
        assert_eq!(file.content_type.as_deref(), Some("audio/wav"));
        assert_eq!(file.data, [0, 1, 2, 255]);
    }

    #[test]
    fn multipart_parts_requires_boundary() {
        let request = HttpRequest {
            method: "POST".to_string(),
            path: "/v1/audio/transcriptions".to_string(),
            query: String::new(),
            headers: BTreeMap::from([(
                "content-type".to_string(),
                "multipart/form-data".to_string(),
            )]),
            body: Vec::new(),
        };
        assert!(multipart_parts(&request).unwrap_err().contains("boundary"));
    }

    #[test]
    fn detects_browser_audio_capability_refusals() {
        assert!(is_audio_capability_refusal(
            "I am not able to transcribe audio files."
        ));
        assert!(is_audio_capability_refusal(
            "Diese Datei kann ich nicht zuverlaessig transkribieren."
        ));
        assert!(!is_audio_capability_refusal(
            "Hier ist die Transkription: Guten Morgen."
        ));
    }
}
