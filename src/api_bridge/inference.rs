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

use super::{select_auto_brain, transport::ClientPresence, AutoPurpose, BridgeConfig};
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

/// Stabiler Marker: der Aufrufer war weg, bevor bzw. waehrend ein Browser-Turn
/// lief. Kein Brain-Fehler und kein Client-Fehler — der Turn wird als verwaist
/// abgebrochen, die Brain-Sperre wird freigegeben (T-946).
pub(super) const CLIENT_DISCONNECTED: &str = "client_disconnected";

fn disconnected_error(brain: &str) -> String {
    format!("{CLIENT_DISCONNECTED}: Aufrufer nicht mehr verbunden; kein Browserturn fuer {brain}")
}

/// Fuehrt `run` nur dann unter der per-Brain-Sperre aus, wenn der Aufrufer noch
/// verbunden ist (T-946).
///
/// Der Check VOR dem Lock verhindert, dass eine bereits verwaiste Anfrage
/// ueberhaupt in die Warteschlange vor der Brain-Sperre gelangt. Der zweite
/// Check NACH dem Lock verhindert, dass sie nach dem Warten trotzdem noch einen
/// Browser-Turn startet. In beiden Faellen wird die Sperre nicht gehalten
/// bzw. sofort wieder freigegeben. Eine globale Serialisierung entsteht nicht:
/// der Lock-Key bleibt das Brain.
fn with_browser_lock<T>(
    brain: &str,
    presence: &dyn ClientPresence,
    run: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    if !presence.connected() {
        return Err(disconnected_error(brain));
    }
    let lock = browser_run_lock(brain);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if !presence.connected() {
        return Err(disconnected_error(brain));
    }
    run()
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

#[allow(clippy::too_many_arguments)]
pub(super) fn run_task_blocking(
    config: &BridgeConfig,
    brain: &str,
    task: &str,
    attachments: &[crate::browser_inference::BrowserAttachment],
    tools: &[crate::browser_inference::BrowserTool],
    tool_choice: crate::browser_inference::BrowserToolChoice,
    retry_prompt: Option<&str>,
    presence: &dyn ClientPresence,
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
    let started = Instant::now();
    with_browser_lock(&brain, presence, || {
        crate::browser_inference::complete_with_attachments(
            crate::browser_inference::BrowserInferenceRequest {
                brain: &brain,
                prompt: task,
                tools,
                tool_choice,
                headless: config.headless,
                timeout_secs,
                model: None,
                retry_prompt,
            },
            attachments,
            &mut |_| {},
            presence,
        )
    })
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
    presence: &dyn ClientPresence,
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
    let started = Instant::now();
    with_browser_lock(&brain, presence, || {
        crate::browser_inference::complete_streaming_with_attachments(
            crate::browser_inference::BrowserInferenceRequest {
                brain: &brain,
                prompt: task,
                tools: &[],
                tool_choice: crate::browser_inference::BrowserToolChoice::None,
                headless: config.headless,
                timeout_secs,
                model: None,
                retry_prompt: None,
            },
            attachments,
            on_update,
            presence,
        )
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_bridge::TestPresence;
    use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

    /// DoD T-946: eine Anfrage, deren Aufrufer bereits verschwunden ist,
    /// wartet nicht in der Warteschlange vor der Brain-Sperre. Der Vorab-Check
    /// schlaegt zu, bevor ueberhaupt `lock()` aufgerufen wird — die Closure
    /// laeuft nicht, die fremde Sperre bleibt unberuehrt.
    #[test]
    fn verwaisste_anfrage_wartet_nicht_auf_die_brain_sperre() {
        let brain = "mock_sperre_verwaist";
        let lock = browser_run_lock(brain);
        let _fremder_turn = lock.lock().unwrap_or_else(|p| p.into_inner());
        let presence = TestPresence::connected(false);
        let ran = AtomicBool::new(false);
        let started = Instant::now();
        let result = with_browser_lock(brain, &presence, || {
            ran.store(true, SeqCst);
            Ok(())
        });
        assert!(result.is_err(), "{result:?}");
        assert!(
            result.unwrap_err().contains(CLIENT_DISCONNECTED),
            "stabiler Grund fehlt"
        );
        assert!(!ran.load(SeqCst), "Turn darf nicht gestartet werden");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "darf nicht auf die fremde Sperre warten"
        );
    }

    /// DoD T-946: eine Folgeanfrage an dasselbe Brain wartet nicht auf einen
    /// verwaisten Turn. Turn A haelt die Sperre; sein Aufrufer verschwindet,
    /// der Turn endet frueh (wie im Wait-Poll des Relay bewiesen) und gibt die
    /// Sperre frei. B laeuft unmittelbar danach — nicht nach A's Zeitbudget.
    #[test]
    fn folgeanfrage_wartet_nicht_auf_verwaisten_turn() {
        let brain = "mock_folge_verwaist";
        let presence_a = TestPresence::connected(true);
        let presence_b = TestPresence::connected(true);
        let in_turn = Arc::new(AtomicBool::new(false));
        let in_turn_a = in_turn.clone();
        let a = presence_a.clone();
        let turn_a = std::thread::spawn(move || {
            with_browser_lock(brain, &a, || {
                in_turn_a.store(true, SeqCst);
                // Simuliert den Relay-Wait-Poll: der Turn endet, sobald der
                // Aufrufer verschwindet (im Relay ueber `cancelled` belegt).
                while a.connected() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok("A")
            })
        });
        while !in_turn.load(SeqCst) {
            std::thread::sleep(Duration::from_millis(2));
        }
        presence_a.set(false);
        let started = Instant::now();
        let follow_up = with_browser_lock(brain, &presence_b, || Ok("B"));
        assert_eq!(follow_up.unwrap(), "B");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "Folgeanfrage darf nicht auf den verwaisten Turn warten"
        );
        assert_eq!(turn_a.join().expect("turn_a"), Ok("A"));
    }
}
