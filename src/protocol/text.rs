use serde_json::Value;

use super::parser::{edit_envelope_regex, script_envelope_regex, strip_rendered_ui_controls, write_envelope_regex};
use super::types::PROTOCOL_VERSION;

pub fn is_possibly_truncated(response_text: &str) -> bool {
    let text = strip_rendered_ui_controls(response_text);

    if text.starts_with("WEBAGENT/1 SHELL") {
        return !script_envelope_regex().is_match(&text);
    }
    if text.starts_with("WEBAGENT/1 WRITE") {
        return !write_envelope_regex().is_match(&text);
    }
    if text.starts_with("WEBAGENT/1 EDIT") {
        return !edit_envelope_regex().is_match(&text);
    }

    if !text.starts_with('{') {
        return false;
    }

    // Unvollständiges Root-Objekt
    if !text.trim_end().ends_with('}') {
        return true;
    }

    match serde_json::from_str::<Value>(&text) {
        Ok(_) => false,
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            msg.contains("unterminated string")
                || msg.contains("expecting ',' delimiter")
                || msg.contains("expecting property name")
                || (msg.contains("expecting value")
                    && (text.len() < 32
                        || text.trim_end().ends_with('[')
                        || text.trim_end().ends_with(':')
                        || text.trim_end().ends_with(',')))
        }
    }
}

pub fn format_observation(
    action_id: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    interrupted: bool,
) -> String {
    let header = if interrupted {
        format!(
            "[Terminal-Ausgabe action_id={} - Ctrl+C unterbrochen]",
            action_id
        )
    } else {
        format!("[Terminal-Ausgabe action_id={}]", action_id)
    };

    let mut parts = vec![header];

    if !stdout.trim().is_empty() {
        parts.push(stdout.trim_end().to_string());
    }

    if !stderr.trim().is_empty() {
        parts.push(format!("[stderr]\n{}", stderr.trim_end()));
    }

    if let Some(code) = exit_code {
        parts.push(format!("[exit_code: {}]", code));
    }

    if parts.len() == 1 {
        parts.push("(keine Ausgabe)".to_string());
    }

    parts.join("\n")
}

/// Nach wie vielen aufeinanderfolgenden Parse-Fails der Run als `protocol_error`
/// endet. Fail 1..MAX-1 → Repair-Prompt; ab MAX → abort.
pub const PROTOCOL_REPAIR_MAX_FAILURES: usize = 3;

/// `true` solange noch Repair-Versuche erlaubt sind (streak 1 und 2 bei MAX=3).
pub fn should_attempt_protocol_repair(consecutive_failures: usize) -> bool {
    consecutive_failures > 0 && consecutive_failures < PROTOCOL_REPAIR_MAX_FAILURES
}

/// `true` ab dem dritten aufeinanderfolgenden Parse-Fail (kein weiterer Retry).
pub fn should_abort_protocol_repair(consecutive_failures: usize) -> bool {
    consecutive_failures >= PROTOCOL_REPAIR_MAX_FAILURES
}

/// Repair-Prompt nach ungueltigem Brain-Output. Zeigt exakt erwartetes Mini-JSON.
pub fn format_protocol_error(detail: &str) -> String {
    let example = serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "actions": [
            {
                "id": "repair-1",
                "type": "shell",
                "command": "Get-Location",
                "timeout_seconds": 30
            }
        ]
    });
    let example_s = serde_json::to_string_pretty(&example).unwrap();

    format!(
        "[Controller] Ungültige Antwort — Repair. {detail}\n\
         Antworte JETZT NUR mit genau diesem Format (gültiges {PROTOCOL_VERSION}-JSON).\n\
         Keine Prosa, kein Markdown-Dokument, kein Thought Process. Sofort mit `{{` oder ```json beginnen.\n\
         EXAKT erwartetes Muster (eine shell-Action; id darf neu sein):\n\
         {example_s}\n\
         Nach der Observation: eigene Antwort nur mit finish ODER nur mit message.\n\
         WICHTIG: in command Anführungszeichen als \\\" und Backslashes als \\\\ escapen."
    )
}

pub fn format_observations_bundle(parts: &[String]) -> String {
    parts.join("\n\n")
}

