//! Single-turn relay (Python `relay_single_turn`) — send+wait, kein Controller/Shell.

use std::time::Instant;

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::browser_inference::BrowserAttachment;
use crate::timeouts::resolve_timeout;

#[derive(Debug)]
pub struct RelayError(pub String);

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Eine Send+Wait-Runde gegen ein Brain; kein Controller, keine Shell-Actions.
///
/// `model` wechselt das Zielmodell in DERSELBEN Sitzung, in der auch gefragt
/// wird. Ein Wechsel in einer separaten Sitzung waere wirkungslos: jeder neue
/// Browserstart faellt aufs Standardmodell zurueck. Der Wechsel passiert
/// deshalb genau einmal, vor der Turn-Schleife — `new_chat` in der Schleife
/// startet nur eine neue Konversation, das Modell der Sitzung bleibt.
pub fn relay_single_turn(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
) -> Result<String, RelayError> {
    relay_single_turn_streaming(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        &mut |_| {},
    )
}

/// Single-Turn-Relay mit wachsenden Text-Snapshots fuer Transport-Streaming.
pub fn relay_single_turn_streaming(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    on_update: &mut dyn FnMut(&str),
) -> Result<String, RelayError> {
    relay_single_turn_with_attachments_streaming(
        brain_id,
        message,
        headless,
        timeout_override,
        model,
        &[],
        on_update,
    )
}

/// Single-Turn-Relay mit Dateien, die vor dem Senden an den Browser-Composer
/// angehaengt werden. Der leere Slice ist der identische Textpfad aus
/// [`relay_single_turn_streaming`].
pub fn relay_single_turn_with_attachments_streaming(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
    model: Option<&str>,
    attachments: &[BrowserAttachment],
    on_update: &mut dyn FnMut(&str),
) -> Result<String, RelayError> {
    // Ein Brain, das gerade wiederholt blockiert/rate-limitiert war, wird fuer eine
    // Cooldown-Zeit uebersprungen statt erneut in den vollen Timeout zu laufen.
    if let Some(remaining) = crate::circuit_breaker::check(brain_id) {
        return Err(RelayError(format!(
            "circuit_open: {brain_id} uebersprungen, noch {remaining}s Cooldown"
        )));
    }
    let started = Instant::now();
    let prompt_chars = message.chars().count();
    let mut backend = WebBrainBackend::from_config(brain_id).map_err(RelayError)?;
    let ready_timeout = resolve_timeout("ensure_ready", brain_id, "", timeout_override);
    let wait_timeout = resolve_timeout("wait_response", brain_id, message, timeout_override);

    backend.start(headless).map_err(RelayError)?;
    let state = backend
        .ensure_ready(ready_timeout)
        .unwrap_or(SessionState::Error);
    if state != SessionState::Ready {
        let _ = backend.stop();
        let reason = format!("session_state={state:?}");
        crate::circuit_breaker::record_failure(brain_id, &reason);
        crate::brain_score::record_event(
            brain_id,
            false,
            Some(&reason),
            started.elapsed().as_millis() as u64,
            prompt_chars,
        );
        return Err(RelayError(reason));
    }
    if let Some(want) = model {
        match backend.switch_model(want) {
            Ok(now) => {
                // „bereits aktiv" ist kein Wechsel: die Beschriftung trug das
                // Ziel schon vorher, also hat dieser Lauf den Antrieb nicht
                // belegt. Erst ein gemessener Wechsel ist ein Live-Beweis.
                if !now.contains("bereits aktiv") {
                    crate::capability_proof::record_route_proof(
                        brain_id,
                        "model_switch",
                        &format!("relay --model '{want}' (Beschriftung nachgeprueft)"),
                        started.elapsed().as_millis() as u64,
                    );
                }
            }
            Err(e) => {
                let reason = format!("model_switch({want}): {e}");
                crate::circuit_breaker::record_failure(brain_id, &reason);
                crate::brain_score::record_event(
                    brain_id,
                    false,
                    Some(&reason),
                    started.elapsed().as_millis() as u64,
                    prompt_chars,
                );
                return Err(RelayError(reason));
            }
        }
    }
    // Bis zu drei volle Turns (new_chat + send + wait_response). Web-UIs ohne API
    // sind unvermeidlich flakig: Submit oder Antworterkennung koennen scheitern.
    // Jeder Turn startet mit einem frischen `new_chat`, also entsteht kein
    // Doppel-Post im selben Thread — ein evtl. schon gesendeter, aber unerkannter
    // Vorgaenger bleibt in seiner eigenen (verlassenen) Konversation.
    // Rate-Limit wird NICHT wiederholt: das ist ein echtes "spaeter wieder", kein
    // transienter Fehler. Retries gehen sichtbar nach stderr, werden also nicht
    // versteckt.
    const MAX_TURNS: usize = 3;
    let mut last_err = format!("kein Versuch ausgefuehrt fuer {brain_id}");
    let mut answer: Option<String> = None;
    for turn in 0..MAX_TURNS {
        if turn > 0 {
            crate::bench_events::eprint_line(&format!(
                "[relay] {brain_id}: Wiederholung {turn}/{}  (vorher: {last_err})",
                MAX_TURNS - 1
            ));
            std::thread::sleep(std::time::Duration::from_millis(700));
        }
        if let Err(e) = backend.new_chat() {
            last_err = e;
            continue;
        }
        let baseline = match backend.send_with_attachments(message, attachments) {
            Ok(b) => b,
            Err(e) => {
                last_err = e;
                if is_deterministic_send_failure(&last_err) {
                    crate::bench_events::eprint_line(&format!(
                        "[relay] {brain_id}: kein Retry fuer deterministischen Sendefehler"
                    ));
                    break;
                }
                continue;
            }
        };
        let response = match backend.wait_response_streaming(baseline, wait_timeout, on_update) {
            Ok(r) => r,
            Err(e) => {
                last_err = e;
                continue;
            }
        };
        if response.backend_status == "rate_limit" {
            let _ = backend.stop();
            crate::circuit_breaker::record_failure(brain_id, "rate_limit");
            crate::brain_score::record_event(
                brain_id,
                false,
                Some("rate_limit"),
                started.elapsed().as_millis() as u64,
                prompt_chars,
            );
            return Err(RelayError(
                "rate_limited: Brain ist aktuell limitiert/nicht verfügbar".into(),
            ));
        }
        // Externe Blockierung (Rate-/Nachrichtenlimit, Login, Cloudflare) auf der
        // Seite erkannt. Terminal — ein Retry hilft nicht. Distinkt mit "blocked:"-
        // Praefix, damit Messungen es flaggen statt als Tool-Defekt zu werten.
        if response.backend_status == "blocked" {
            let _ = backend.stop();
            crate::circuit_breaker::record_failure(brain_id, "blocked");
            crate::brain_score::record_event(
                brain_id,
                false,
                Some("blocked"),
                started.elapsed().as_millis() as u64,
                prompt_chars,
            );
            return Err(RelayError(format!(
                "blocked: {brain_id}: {}",
                response.text.trim()
            )));
        }
        // Leerer Text = Timeout ohne erkannte Antwort. wait_response gibt das als
        // Ok mit leerem Text zurueck; ohne diese Pruefung zaehlte ein Timeout als
        // Erfolg (so entstand frueher "5/8 PASS" ohne eine echte Antwort).
        let text = response.text.trim().to_string();
        if text.is_empty() {
            last_err = format!(
                "keine Antwort erhalten (backend_status={}, generation_complete={})",
                response.backend_status, response.generation_complete
            );
            continue;
        }
        answer = Some(text);
        break;
    }
    // Shared-Pool: `stop` respektiert `persist_browser_tabs()` (Tab bleibt offen).
    let _ = backend.stop();
    let latency_ms = started.elapsed().as_millis() as u64;
    match answer {
        Some(text) => {
            crate::circuit_breaker::record_success(brain_id);
            crate::brain_score::record_event(brain_id, true, None, latency_ms, prompt_chars);
            // Send+Wait gegen den echten Browser = das Brain kann Text
            // senden und eine Antwort lesen. Das ist der Beleg fuer "chat".
            crate::capability_proof::record_route_proof(
                brain_id,
                "chat",
                "relay_single_turn ok (send+wait gegen echten Browser)",
                latency_ms,
            );
            Ok(text)
        }
        None => {
            // Fehlender Datei-Upload ist eine bekannte Fähigkeitsgrenze für
            // multimodale Requests, kein Ausfall des Text-Brains. Er darf
            // deshalb weder den Circuit Breaker öffnen noch den allgemeinen
            // Brain-Score verschlechtern: Ein späterer text-only Turn kann
            // mit demselben Brain problemlos funktionieren.
            if !is_attachment_capability_failure(&last_err) {
                crate::circuit_breaker::record_failure(brain_id, &last_err);
                crate::brain_score::record_event(
                    brain_id,
                    false,
                    Some(&last_err),
                    latency_ms,
                    prompt_chars,
                );
            }
            Err(RelayError(last_err))
        }
    }
}

/// Manche Sendefehler beschreiben einen stabilen UI-Zustand, bei dem ein
/// weiterer kompletter Browserturn nur Zeit verbraucht: sichtbare Blockade,
/// deaktivierter Sendeknopf oder fehlender Absende-Beweis. Transiente CDP- und
/// Navigationsfehler bleiben dagegen retry-faehig.
fn is_deterministic_send_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    is_attachment_capability_failure(error)
        || [
            "kein absende-beweis",
            "absendeknopf ist deaktiviert",
            "blockiert:",
            "usage limit",
            "nachrichtenlimit",
            "rate limit",
            "cloudflare",
            "login required",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
}

/// Der Brain-Score und der allgemeine Breaker beschreiben Textverfügbarkeit.
/// Ein Brain ohne Datei-Input ist für eine Anfrage mit Bild/Audio nicht
/// fehlerhaft; die Capability-Grenze muss als lokaler, nicht wiederholbarer
/// Requestfehler zurückkommen.
fn is_attachment_capability_failure(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    [
        "no_file_input",
        "no_file_input_and_paste_not_confirmed",
        "keinen nutzbaren datei-upload",
        "kein nutzbarer datei-upload",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_error_on_bad_brain_id() {
        let err = relay_single_turn("nonexistent_brain_xyz", "hi", true, None, None);
        assert!(err.is_err());
    }

    #[test]
    fn deterministic_send_failures_are_not_retried() {
        assert!(is_deterministic_send_failure(
            "Absenden fehlgeschlagen: kein Absende-Beweis nach 5 Versuchen"
        ));
        assert!(is_deterministic_send_failure(
            "blockiert: kein Absende-Beweis -- Seite zeigt: Login required"
        ));
        assert!(is_deterministic_send_failure(
            "Absendeknopf ist deaktiviert, obwohl der Text vollstaendig im Composer steht"
        ));
        assert!(!is_deterministic_send_failure("CDP connection reset"));
    }

    #[test]
    fn missing_file_input_is_a_capability_error_not_a_brain_failure() {
        let error =
            "Browseroberflaeche stellt keinen nutzbaren Datei-Upload bereit (no_file_input)";
        assert!(is_attachment_capability_failure(error));
        assert!(is_deterministic_send_failure(error));
        assert!(is_attachment_capability_failure(
            "Browseroberflaeche stellt keinen nutzbaren Datei-Upload \
             (no_file_input_and_paste_not_confirmed)"
        ));
        assert!(!is_attachment_capability_failure("CDP connection reset"));
    }
}
