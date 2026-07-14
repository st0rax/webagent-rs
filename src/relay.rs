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
    backend.new_chat().map_err(RelayError)?;
    let baseline = backend.send(message).map_err(RelayError)?;
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
    Ok(response.text.trim().to_string())
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
