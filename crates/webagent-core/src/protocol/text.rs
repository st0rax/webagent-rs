use serde_json::Value;

use super::parser::{
    edit_batch_envelope_regex, edit_envelope_regex, message_envelope_regex, script_envelope_regex,
    strip_rendered_ui_controls, write_envelope_regex,
};
use super::types::PROTOCOL_VERSION;
pub fn is_possibly_truncated(response_text: &str) -> bool {
    let text = strip_rendered_ui_controls(response_text);

    if text.starts_with("WEBAGENT/1 SHELL") {
        return !script_envelope_regex().is_match(&text)
            && !text.trim_end().ends_with("---END SCRIPT---");
    }
    if text.starts_with("WEBAGENT/1 WRITE") {
        return !write_envelope_regex().is_match(&text)
            && !text.trim_end().ends_with("---END CONTENT---");
    }
    // EDIT_BATCH muss vor EDIT geprüft werden, weil sein Präfix ebenfalls mit
    // `WEBAGENT/1 EDIT` beginnt.
    if text.starts_with("WEBAGENT/1 EDIT_BATCH") {
        return !edit_batch_envelope_regex().is_match(&text)
            && !text.trim_end().ends_with("---END BATCH---");
    }
    if text.starts_with("WEBAGENT/1 EDIT") {
        return !edit_envelope_regex().is_match(&text)
            && !text.trim_end().ends_with("---END EDIT---");
    }
    if text.starts_with("WEBAGENT/1 MESSAGE") {
        // `MESSAGE` ist kanonisch ohne End-Tag. Sobald der Text die
        // vorgeschriebene `id:`/`text:`-Form erfüllt, ist er abgeschlossen.
        // Provider erfinden unter Formatdruck gelegentlich trotzdem einen
        // vollständig geschlossenen Legacy-Block (`---END MESSAGE---`). Der ist
        // parser-invalid, aber nicht mehr im Stream: zügig an den normalen
        // Repair-Pfad geben statt bis zum Provider-Timeout zu warten.
        return !message_envelope_regex().is_match(&text)
            && !text.trim_end().ends_with("---END MESSAGE---");
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
    format_protocol_error_for(detail, "")
}

/// Reparaturhinweis, der die erkennbare Absicht der kaputten Antwort erhaelt.
/// Mehrzeiliger Code in JSON ist der haeufigste Realfehler; dann darf der
/// Repair-Prompt nicht ausgerechnet eine weitere Shell-JSON-Aktion vormachen.
pub fn format_protocol_error_for(detail: &str, invalid_response: &str) -> String {
    let compact = invalid_response.to_ascii_lowercase().replace(' ', "");
    if compact.contains("\"type\":\"edit\"") || compact.contains("webagent/1edit") {
        return format!(
            "[Interpreter] EDIT-Anforderung nicht lesbar: {detail}\n\
             Es wurde nichts ausgeführt. Das Format unten ist keine Ausführungsbehauptung, \
             sondern eine Anfrage an den lokalen Interpreter. Bitte sende denselben Edit erneut:\n\
             WEBAGENT/1 EDIT\n\
             id: neue-eindeutige-id\n\
             path: derselbe-pfad\n\
             ---OLD---\n\
             derselbe old_string unveraendert\n\
             ---NEW---\n\
             derselbe new_string unveraendert\n\
             ---END EDIT---"
        );
    }
    if compact.contains("\"type\":\"write\"") || compact.contains("webagent/1write") {
        return format!(
            "[Interpreter] WRITE-Anforderung nicht lesbar: {detail}\n\
             Es wurde nichts ausgeführt. Bitte sende dieselbe Anfrage erneut im \
             escape-freien Transportformat:\n\
             WEBAGENT/1 WRITE\n\
             id: neue-eindeutige-id\n\
             path: derselbe-pfad\n\
             ---CONTENT---\n\
             derselbe Dateiinhalt unveraendert\n\
             ---END CONTENT---"
        );
    }
    if compact.contains("\"type\":\"shell\"") || compact.contains("webagent/1shell") {
        return format!(
            "[Interpreter] SHELL-Anforderung nicht lesbar: {detail}\n\
             Es wurde nichts ausgeführt. Das Format fordert den lokalen Interpreter zur \
             Ausführung auf und behauptet keinen Zugriff. Bitte sende denselben Befehl im \
             escape-freien Rohformat. Der Prozess arbeitet \
             bereits im richtigen Workspace; kein cd und kein absoluter Workspace-Pfad:\n\
             WEBAGENT/1 SHELL\n\
             id: neue-eindeutige-id\n\
             timeout_seconds: 300\n\
             ---SCRIPT---\n\
             derselbe Befehl direkt\n\
             ---END SCRIPT---"
        );
    }
    format!(
        "[Interpreter] {PROTOCOL_VERSION}-Tool-Anforderung nicht lesbar: {detail}\n\
         Behaupte keine lokale Ausführung ohne eine zurückgepipedete Observation.\n\
         Eine Action ist nur eine Anfrage an den verbundenen lokalen Interpreter.\n\
         Der Interpreter führt sie aus und piped stdout, stderr und Exitcode zurück.\n\
         Bitte sende die beabsichtigte Action erneut als gültiges JSON oder Rohformat.\n\
         WICHTIG: in command Anführungszeichen als \\\" und Backslashes als \\\\ escapen."
    )
}

pub fn format_observations_bundle(parts: &[String]) -> String {
    parts.join("\n\n")
}
