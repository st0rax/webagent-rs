//! Browser-Inference-Lauf der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Serialisiert Browser-Laeufe pro Brain (`BROWSER_RUN_LOCKS`), fuehrt
//! blocking/streaming Turns aus und annotiert AutoRouter-Fehler. Keine
//! SSE-Bytes, kein Store, kein Prompt-Normalizer, keine Medien-Handler.
//! Typen (`BridgeConfig`, `AutoPurpose`) bleiben in der Root-Datei.
//!
//! Invarianten:
//! - Ein Lock pro Brain-Key (ascii-lowercase); Giftfreigabe via into_inner.
//! - `fake_reply` liefert deterministische Antworten ohne Browser.
//! - Auto+Attachments: Timeout-Budget ueber `timeouts::resolve_auto_attach_budget`.
//! - Fehlerpfade bleiben sichtbar (kein stilles Schlucken).

use super::{select_auto_brain, AutoPurpose, BridgeConfig};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

static BROWSER_RUN_LOCKS: OnceLock<Mutex<BTreeMap<String, Arc<Mutex<()>>>>> = OnceLock::new();

/// Per-Brain Mutex fuer serialisierte Browser-Laeufe (auch Medienpfad).
pub(super) fn browser_run_lock(brain: &str) -> Arc<Mutex<()>> {
    BROWSER_RUN_LOCKS
        .get_or_init(|| Mutex::new(BTreeMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .entry(brain.to_ascii_lowercase())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
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

pub(super) fn run_task_blocking(
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
    let lock = browser_run_lock(&brain);
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

pub(super) fn run_task_streaming(
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
    let lock = browser_run_lock(&brain);
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
pub(super) fn annotate_auto_routed_inference_error(
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
