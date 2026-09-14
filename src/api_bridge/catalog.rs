//! Modellkatalog und Auto-Router der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Keine neuen Modelle, keine Live-Verfuegbarkeit behaupten. HTTP-Routing
//! bleibt in `routing.rs`. Modalitaeten nur laut bestaetigter Smokes.
//! T-931 verdrahtet `mod catalog`.

use super::BridgeConfig;
use serde_json::{json, Value};

pub fn available_brains() -> Vec<String> {
    let mut brains: Vec<String> = crate::config::brains().into_keys().collect();
    brains.sort();
    brains
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPurpose {
    Chat,
    ImageGeneration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoRoute {
    Default,
    AudioInput,
    ImageInput,
    ImageGeneration,
    Tools,
    Coding,
    CurrentResearch,
}

pub fn classify_auto_route(
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

pub fn first_available_auto_brain(preferences: &[&str]) -> Option<String> {
    let available = available_brains();
    first_available_auto_brain_in(preferences, &available, |brain| {
        crate::circuit_breaker::check(brain).is_none()
    })
}

/// Kern der Auto-Auswahl mit injizierbarer Verfügbarkeit/Entsperrtheit —
/// deterministisch testbar ohne reale Brain-Installation oder Circuit-Breaker.
pub fn first_available_auto_brain_in(
    preferences: &[&str],
    available: &[String],
    is_unlocked: impl Fn(&str) -> bool,
) -> Option<String> {
    preferences
        .iter()
        .find(|brain| available.iter().any(|candidate| candidate == **brain) && is_unlocked(brain))
        .map(|brain| (*brain).to_string())
}

pub fn select_auto_brain(
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

pub fn select_auto_brain_with_default(
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

pub fn resolve_model(requested: &str, default_brain: &str) -> Result<String, String> {
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

pub fn model_id(brain: &str) -> String {
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
pub fn advertised_input_modalities(brain: &str) -> &'static [&'static str] {
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
pub fn advertised_output_modalities(brain: &str) -> &'static [&'static str] {
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
pub fn model_metadata(brain: &str) -> Value {
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
