//! Single-turn relay (Python `relay_single_turn`) — send+wait, kein Controller/Shell.

use std::time::Instant;

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::timeouts::resolve_timeout;

#[derive(Debug)]
pub struct RelayError(pub String);

impl std::fmt::Display for RelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Warum das letzte Absenden fehlschlug — als Typ, fuer Aufrufer die es
/// unterscheiden muessen.
///
/// Prozessweit abgelegt statt durch `RelayError` gereicht: das ist ein
/// Tupel-Struct, das an vielen Stellen konstruiert wird; ein zweites Feld
/// haette jede davon angefasst. Der eine Aufrufer, der die Unterscheidung
/// braucht (die Laengenmessung), liest sie direkt nach dem fehlgeschlagenen
/// Aufruf.
///
/// Auch das ist ein Uebergang, kein Ziel: sauber waere `RelayError` als enum.
/// Das ist der naechste Schritt derselben Umstellung.
static LAST_SEND_ERROR: std::sync::Mutex<Option<crate::send_error::SendError>> =
    std::sync::Mutex::new(None);

fn remember_send_error(err: Option<crate::send_error::SendError>) {
    if let Ok(mut slot) = LAST_SEND_ERROR.lock() {
        *slot = err;
    }
}

/// Der zuletzt gemerkte Absende-Fehler; `None`, wenn keiner vorlag.
pub fn last_send_error() -> Option<crate::send_error::SendError> {
    LAST_SEND_ERROR.lock().ok().and_then(|s| s.clone())
}

/// Eine Send+Wait-Runde gegen ein Brain; kein Controller, keine Shell-Actions.
pub fn relay_single_turn(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
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
    // Bis zu drei volle Turns (new_chat + send + wait_response). Web-UIs ohne API
    // sind unvermeidlich flakig: das Editor-Submit greift manchmal nicht (kimi),
    // manchmal wird die Antwort nicht erkannt (qwen/zai). Jeder Turn startet mit
    // einem frischen `new_chat`, also entsteht kein Doppel-Post im selben Thread —
    // ein evtl. schon gesendeter, aber unerkannter Vorgaenger bleibt in seiner
    // eigenen (verlassenen) Konversation. Rate-Limit wird NICHT wiederholt: das ist
    // ein echtes "spaeter wieder", kein transienter Fehler. Retries gehen sichtbar
    // nach stderr, werden also nicht versteckt.
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
        let baseline = match backend.send(message) {
            Ok(b) => {
                remember_send_error(None);
                b
            }
            Err(e) => {
                // Den TYP mitnehmen, bevor der naechste Versuch ihn ueberschreibt.
                remember_send_error(backend.last_send_error());
                last_err = e;
                // Eine Verweigerung der Oberflaeche faellt beim naechsten
                // Versuch genauso aus. Am 05.08.2026 liefen claude und mistral
                // deshalb in je zwei sinnlose Wiederholungen plus
                // Circuit-Breaker — mit einem Typ statt einem Text ist das
                // eine Abfrage.
                if backend
                    .last_send_error()
                    .is_some_and(|se| !se.is_retryable())
                {
                    break;
                }
                continue;
            }
        };
        let response = match backend.wait_response(baseline, wait_timeout) {
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
                "claude_rate_limited: Claude ist aktuell limitiert/nicht verfügbar".into(),
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
            Ok(text)
        }
        None => {
            crate::circuit_breaker::record_failure(brain_id, &last_err);
            crate::brain_score::record_event(
                brain_id,
                false,
                Some(&last_err),
                latency_ms,
                prompt_chars,
            );
            Err(RelayError(last_err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_error_on_bad_brain_id() {
        let err = relay_single_turn("nonexistent_brain_xyz", "hi", true, None);
        assert!(err.is_err());
    }
}
