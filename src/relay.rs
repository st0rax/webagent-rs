//! Single-turn relay (Python `relay_single_turn`) — send+wait, kein Controller/Shell.

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

/// Eine Send+Wait-Runde gegen ein Brain; kein Controller, keine Shell-Actions.
pub fn relay_single_turn(
    brain_id: &str,
    message: &str,
    headless: bool,
    timeout_override: Option<f64>,
) -> Result<String, RelayError> {
    let mut backend = WebBrainBackend::from_config(brain_id).map_err(RelayError)?;
    let ready_timeout = resolve_timeout("ensure_ready", brain_id, "", timeout_override);
    let wait_timeout = resolve_timeout("wait_response", brain_id, message, timeout_override);

    backend.start(headless).map_err(RelayError)?;
    let state = backend
        .ensure_ready(ready_timeout)
        .unwrap_or(SessionState::Error);
    if state != SessionState::Ready {
        let _ = backend.stop();
        return Err(RelayError(format!("session_state={state:?}")));
    }
    // Bis zu zwei Sende-Anläufe: schlägt `send` fehl, wurde nachweislich **nichts**
    // abgeschickt (send meldet nur bei bestätigtem Absende-Beweis Erfolg), also ist
    // ein frischer new_chat + erneutes Senden gefahrlos — kein Doppel-Post. Das hebt
    // Brains mit flakigem Editor-Submit (kimi) von ~75 % auf ~95 %. Nur der Sende-
    // Schritt wird wiederholt, NICHT wait_response: dort könnte die Nachricht bereits
    // draußen sein, und ein Retry würde doppelt posten.
    let mut last_err = String::new();
    let mut baseline = None;
    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(600));
        }
        if let Err(e) = backend.new_chat() {
            last_err = e;
            continue;
        }
        match backend.send(message) {
            Ok(b) => {
                baseline = Some(b);
                break;
            }
            Err(e) => last_err = e,
        }
    }
    let baseline = match baseline {
        Some(b) => b,
        None => {
            let _ = backend.stop();
            return Err(RelayError(last_err));
        }
    };
    let response = backend
        .wait_response(baseline, wait_timeout)
        .map_err(RelayError)?;
    // Shared-Pool: `stop` respektiert `persist_browser_tabs()` (Tab bleibt offen).
    let _ = backend.stop();
    if response.backend_status == "rate_limit" {
        return Err(RelayError(
            "claude_rate_limited: Claude ist aktuell limitiert/nicht verfügbar".into(),
        ));
    }
    // Ohne diese Pruefung meldet der Relay ein Timeout als Erfolg: wait_response
    // liefert bei "kein Ergebnis" einen leeren Text mit generation_complete=false,
    // und ein blosses Ok("") wird vom Aufrufer (und vom Smoke-Skript, das exit 0
    // als PASS wertet) als gruener Lauf gezaehlt. Genau so entstand "5/8 PASS"
    // ohne eine einzige echte Antwort.
    let text = response.text.trim().to_string();
    if text.is_empty() {
        return Err(RelayError(format!(
            "keine Antwort erhalten (backend_status={}, generation_complete={})",
            response.backend_status, response.generation_complete
        )));
    }
    Ok(text)
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
