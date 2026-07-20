//! Strukturiertes Aktionsprotokoll webagent/1.

use fancy_regex::Regex as FancyRegex;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::OnceLock;

pub const PROTOCOL_VERSION: &str = "webagent/1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    Shell,
    Message,
    Finish,
    /// Eindeutiger Anker-Ersatz in einer Bestandsdatei (path/old_string/new_string).
    Edit,
    /// Neue Datei anlegen (path/content); existierende Dateien werden abgelehnt.
    Write,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    #[serde(rename = "type")]
    pub action_type: ActionType,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub old_string: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub new_string: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub content: String,
}

impl Action {
    /// Basis-Action ohne typspezifische Felder — Konstruktor-Helfer.
    fn base(id: String, action_type: ActionType) -> Self {
        Self {
            id,
            action_type,
            command: String::new(),
            text: String::new(),
            timeout_seconds: 30.0,
            path: String::new(),
            old_string: String::new(),
            new_string: String::new(),
            content: String::new(),
        }
    }
}

fn default_timeout() -> f64 {
    30.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseResult {
    pub valid: bool,
    pub actions: Vec<Action>,
    pub error: String,
    pub raw_text: String,
}

impl ParseResult {
    fn invalid(error: impl Into<String>, raw_text: impl Into<String>) -> Self {
        Self {
            valid: false,
            actions: Vec::new(),
            error: error.into(),
            raw_text: raw_text.into(),
        }
    }

    fn valid(actions: Vec<Action>, raw_text: impl Into<String>) -> Self {
        Self {
            valid: true,
            actions,
            error: String::new(),
            raw_text: raw_text.into(),
        }
    }
}

fn json_block_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"```(?:json)?\s*(\{.*\})\s*```").unwrap())
}

fn rendered_json_label_regex() -> &'static FancyRegex {
    static RE: OnceLock<FancyRegex> = OnceLock::new();
    RE.get_or_init(|| {
        FancyRegex::new(
            r"(?i)^json\s*\r?\n(?:(?:copy|kopieren)\s*\r?\n)?(?:(?:download|herunterladen)\s*\r?\n)?(?=\s*\{)",
        )
        .unwrap()
    })
}

fn ui_control_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^(?:json|copy|kopieren|download|herunterladen|\d+)$").unwrap()
    })
}

fn leading_prose_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:denke nach|thinking|thought process|thoughtprocess|reasoning|ueberlege|überlege|思考|antworte|hier ist|sure|ok|okay|alright|verstanden|ich sehe das problem|thought|erneut versuchen)[\s.…:]*$",
        )
        .unwrap()
    })
}

fn script_envelope_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?s)\AWEBAGENT/1 SHELL\r?\nid:\s*([A-Za-z0-9][A-Za-z0-9._-]{0,127})\r?\ntimeout_seconds:\s*([0-9]+(?:\.[0-9]+)?)\r?\n---SCRIPT---\r?\n([\s\S]+?)\r?\n---END SCRIPT---\s*\z",
        )
        .unwrap()
    })
}

fn strip_leading_prose(text: &str) -> &str {
    let lines: Vec<&str> = text.lines().collect();
    let mut index = 0;
    let re = leading_prose_line_regex();
    while index < lines.len() && re.is_match(lines[index].trim()) {
        index += 1;
    }
    if index > 0 {
        lines[index..].join("\n").leak()
    } else {
        text
    }
}

fn extract_first_protocol_json(text: &str) -> Option<String> {
    // Suche nach einem Top-Level-Objekt mit "protocol": "webagent/1"
    // Kein Lookahead nötig, einfaches Pattern
    let re = Regex::new(r#"(?is)\{\s*"protocol"\s*:\s*"webagent/1"[^}]*\}"#).unwrap();

    if let Some(mat) = re.find(text) {
        let candidate = mat.as_str().trim();
        if let Ok(obj) = serde_json::from_str::<Value>(candidate) {
            if obj.get("protocol").and_then(|v| v.as_str()) == Some(PROTOCOL_VERSION) {
                return Some(candidate.to_string());
            }
        }
    }

    // Fallback: erstes { bis letztes }
    if let Some(first) = text.find('{') {
        if let Some(last) = text.rfind('}') {
            if last > first {
                let candidate = &text[first..=last];
                if let Ok(obj) = serde_json::from_str::<Value>(candidate) {
                    if let Some(obj_map) = obj.as_object() {
                        if obj_map.get("protocol").and_then(|v| v.as_str())
                            == Some(PROTOCOL_VERSION)
                        {
                            return Some(candidate.trim().to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn strip_rendered_ui_controls(text: &str) -> String {
    let normalized = text
        .replace(['\u{00a0}', '\u{202f}'], " ")
        .replace('\u{200b}', "")
        .trim()
        .to_string();

    let normalized = strip_leading_prose(&normalized);
    let lines: Vec<&str> = normalized.lines().collect();
    let mut index = 0;
    let mut saw_json_label = false;
    let re = ui_control_line_regex();

    while index < lines.len() {
        let label = lines[index].trim();
        if !re.is_match(label) {
            break;
        }
        if label.eq_ignore_ascii_case("json") {
            saw_json_label = true;
        } else if !saw_json_label {
            break;
        }
        index += 1;
    }

    if saw_json_label {
        lines[index..].join("\n").trim().to_string()
    } else {
        normalized.to_string()
    }
}

fn repair_message_windows_paths(json_text: &str) -> Option<String> {
    // Nur reparieren wenn alle Actions vom Typ "message" sind
    if !json_text.contains(r#""type""#) || !json_text.contains(r#""message""#) {
        return None;
    }
    if json_text.contains(r#""shell""#) || json_text.contains(r#""finish""#) {
        return None;
    }

    let re = Regex::new(r#"[A-Za-z]:\\[^"\r\n]*"#).unwrap();
    let repaired = re.replace_all(json_text, |caps: &regex::Captures| {
        caps[0].replace('\\', "\\\\")
    });

    if repaired != json_text {
        Some(repaired.to_string())
    } else {
        None
    }
}

// ============================================================================
// SCHEMA-REFERENZ webagent/1 (Single Source of Truth — spiegelt sich in
// docs/PROTOCOL_SCHEMA.md, das aus diesem Block abgeleitet ist).
//
// Envelope (Wurzel-Objekt):
//   protocol : String  == "webagent/1"           (Pflicht)
//   actions  : Array   nicht-leer                 (Pflicht)
//
// Jede Action ist ein Objekt. Erlaubte Felder je type — KEINE anderen Felder
// zugelassen (unbekannte Felder → invalid, damit Tippfehler wie "comand" statt
// "command" nicht als leerer Befehl durchrutschen). Gemeinsam für alle: {id, type}.
//
//   type "shell"   +{command, timeout_seconds}
//                    command: nicht-leer (getrimmt)
//                    timeout_seconds: Zahl, 0 < x <= 3600 (Default 30)
//   type "message" +{text}
//                    text: nicht-leer (getrimmt)
//   type "finish"   (nur id, type)
//   type "edit"    +{path, old_string, new_string}
//                    path: nicht-leer (getrimmt); old_string: nicht-leer,
//                    old_string != new_string
//   type "write"   +{path, content}
//                    path: nicht-leer (getrimmt); content: Pflicht (auch "" ok)
//
// Zusatzregeln (in parse(), nicht pro Action): finish und message müssen jeweils
// die EINZIGE Action der Antwort sein; Action-ids müssen eindeutig sein.
// ============================================================================

/// Erlaubte Feldnamen je Action-`type`, inklusive der gemeinsamen `id`/`type`.
/// Grundlage der Strikt-Validierung gegen unbekannte Felder.
fn allowed_fields(action_type: &ActionType) -> &'static [&'static str] {
    match action_type {
        ActionType::Shell => &["id", "type", "command", "timeout_seconds"],
        ActionType::Message => &["id", "type", "text"],
        ActionType::Finish => &["id", "type"],
        ActionType::Edit => &["id", "type", "path", "old_string", "new_string"],
        ActionType::Write => &["id", "type", "path", "content"],
    }
}

fn action_from_value(val: &Value) -> Result<Action, String> {
    let obj = val.as_object().ok_or("jede Action muss ein Objekt sein")?;

    let action_id = obj
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("jede Action braucht eine id")?
        .to_string();

    let action_type_str = obj
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("Action {} braucht type", action_id))?;

    let action_type = match action_type_str {
        "shell" => ActionType::Shell,
        "message" => ActionType::Message,
        "finish" => ActionType::Finish,
        "edit" => ActionType::Edit,
        "write" => ActionType::Write,
        _ => return Err(format!("unbekannter type: {:?}", action_type_str)),
    };

    // Strikte Schema-Prüfung: keine unbekannten Felder je Action-type.
    let allowed = allowed_fields(&action_type);
    for key in obj.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "Action {}: unbekanntes Feld {:?} für type {:?}; erlaubt sind {:?}",
                action_id, key, action_type_str, allowed
            ));
        }
    }

    let str_field = |key: &str| -> String {
        obj.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    match action_type {
        ActionType::Shell => {
            let command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if command.is_empty() {
                return Err(format!("shell action {} braucht command", action_id));
            }

            let default_timeout = Value::Number(30.into());
            let raw_timeout = obj.get("timeout_seconds").unwrap_or(&default_timeout);

            // Prüfe ob es ein bool ist (nicht erlaubt)
            if raw_timeout.is_boolean() {
                return Err(format!(
                    "shell action {}: timeout_seconds muss eine Zahl sein",
                    action_id
                ));
            }

            let timeout = raw_timeout.as_f64().ok_or_else(|| {
                format!(
                    "shell action {}: timeout_seconds muss eine Zahl sein",
                    action_id
                )
            })?;

            if !timeout.is_finite() || timeout <= 0.0 || timeout > 3600.0 {
                return Err(format!(
                    "shell action {}: timeout_seconds muss endlich und groesser als 0 und hoechstens 3600 sein",
                    action_id
                ));
            }

            // Prüfe auf verschachteltes Rohskript
            let re = script_envelope_regex();
            if let Some(caps) = re.captures(&command) {
                let nested_id = &caps[1];
                if nested_id != action_id {
                    return Err(
                        "verschachtelte Rohskript-ID stimmt nicht mit Action-ID überein"
                            .to_string(),
                    );
                }
                let nested_command = caps[3].trim().to_string();
                let nested_timeout: f64 = caps[2].parse().unwrap();
                let mut a = Action::base(action_id, ActionType::Shell);
                a.command = nested_command;
                a.timeout_seconds = nested_timeout;
                return Ok(a);
            }

            let mut a = Action::base(action_id, ActionType::Shell);
            a.command = command;
            a.timeout_seconds = timeout;
            Ok(a)
        }
        ActionType::Message => {
            let text = obj
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();

            if text.is_empty() {
                return Err(format!("message action {} braucht text", action_id));
            }

            let mut a = Action::base(action_id, ActionType::Message);
            a.text = text;
            Ok(a)
        }
        ActionType::Finish => Ok(Action::base(action_id, ActionType::Finish)),
        ActionType::Edit => {
            let path = str_field("path").trim().to_string();
            let old_string = str_field("old_string");
            let new_string = str_field("new_string");
            if path.is_empty() {
                return Err(format!("edit action {} braucht path", action_id));
            }
            if old_string.is_empty() {
                return Err(format!(
                    "edit action {} braucht old_string (exakter, eindeutiger Anker aus der Datei)",
                    action_id
                ));
            }
            if old_string == new_string {
                return Err(format!(
                    "edit action {}: old_string und new_string sind identisch",
                    action_id
                ));
            }
            let mut a = Action::base(action_id, ActionType::Edit);
            a.path = path;
            a.old_string = old_string;
            a.new_string = new_string;
            Ok(a)
        }
        ActionType::Write => {
            let path = str_field("path").trim().to_string();
            if path.is_empty() {
                return Err(format!("write action {} braucht path", action_id));
            }
            if !obj.contains_key("content") {
                return Err(format!("write action {} braucht content", action_id));
            }
            let mut a = Action::base(action_id, ActionType::Write);
            a.path = path;
            a.content = str_field("content");
            Ok(a)
        }
    }
}

pub fn parse(response_text: &str) -> ParseResult {
    let text = strip_rendered_ui_controls(response_text);

    if text.is_empty() {
        return ParseResult::invalid("Leere Antwort.", text);
    }

    // Graceful handling für Model capacity / rate limit
    let capacity_re =
        Regex::new(r"(?i)(Höchstgrenze|Kapazität|capacity|rate limit|zu viele|erneut versuchen)")
            .unwrap();
    if capacity_re.is_match(&text) {
        return ParseResult::invalid("Model capacity / rate limit.", text);
    }

    // WEBAGENT/1 SHELL Rohskript-Format
    let script_re = script_envelope_regex();
    if let Some(caps) = script_re.captures(&text) {
        let timeout: f64 = caps[2].parse().unwrap();
        if !timeout.is_finite() || timeout <= 0.0 || timeout > 3600.0 {
            return ParseResult::invalid(
                "timeout_seconds muss groesser als 0 und hoechstens 3600 sein",
                text,
            );
        }
        let mut a = Action::base(caps[1].to_string(), ActionType::Shell);
        a.command = caps[3].trim().to_string();
        a.timeout_seconds = timeout;
        return ParseResult::valid(vec![a], text);
    }

    // Entferne gerenderte JSON-Labels
    let label_re = rendered_json_label_regex();
    let text = label_re.replace(&text, "").trim().to_string();

    // Suche nach JSON-Codeblock
    let block_re = json_block_regex();
    let json_str = if let Some(caps) = block_re.captures(&text) {
        let before = &text[..caps.get(0).unwrap().start()];
        let after = &text[caps.get(0).unwrap().end()..];
        let outside = format!("{}{}", before, after).trim().to_string();

        if !outside.is_empty() {
            return ParseResult::invalid(
                "Text außerhalb des JSON-Codeblocks ist nicht erlaubt.",
                text,
            );
        }
        caps[1].trim().to_string()
    } else {
        text.clone()
    };

    // Robuste Extraktion für Brains mit "Thought Process" etc.
    let json_str = if !json_str.trim().starts_with('{') {
        extract_first_protocol_json(&text).unwrap_or(json_str)
    } else {
        json_str
    };

    // Parse JSON
    let doc = match serde_json::from_str::<Value>(&json_str) {
        Ok(v) => v,
        Err(exc) => {
            // Versuche Windows-Path-Reparatur
            if let Some(repaired) = repair_message_windows_paths(&json_str) {
                match serde_json::from_str::<Value>(&repaired) {
                    Ok(v) => v,
                    Err(_) => {
                        return ParseResult::invalid(format!("Ungültiges JSON: {}", exc), text);
                    }
                }
            } else {
                return ParseResult::invalid(format!("Ungültiges JSON: {}", exc), text);
            }
        }
    };

    let obj = match doc.as_object() {
        Some(o) => o,
        None => {
            return ParseResult::invalid("Wurzel muss ein JSON-Objekt sein.", text);
        }
    };

    if obj.get("protocol").and_then(|v| v.as_str()) != Some(PROTOCOL_VERSION) {
        return ParseResult::invalid(
            format!("protocol muss \"{}\" sein.", PROTOCOL_VERSION),
            text,
        );
    }

    let raw_actions = match obj.get("actions").and_then(|v| v.as_array()) {
        Some(arr) if !arr.is_empty() => arr,
        _ => {
            return ParseResult::invalid("actions muss eine nicht-leere Liste sein.", text);
        }
    };

    let mut actions = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();

    for item in raw_actions {
        match action_from_value(item) {
            Ok(action) => {
                if !seen_ids.insert(action.id.clone()) {
                    return ParseResult::invalid(
                        format!("doppelte Action-id: {}", action.id),
                        text,
                    );
                }
                actions.push(action);
            }
            Err(e) => return ParseResult::invalid(e, text),
        }
    }

    // Validierung: finish muss alleine sein
    let finish_count = actions
        .iter()
        .filter(|a| a.action_type == ActionType::Finish)
        .count();
    if finish_count > 0 && actions.len() != 1 {
        return ParseResult::invalid("finish muss die einzige Action der Antwort sein", text);
    }

    // Validierung: message muss alleine sein
    let message_count = actions
        .iter()
        .filter(|a| a.action_type == ActionType::Message)
        .count();
    if message_count > 0 && actions.len() != 1 {
        return ParseResult::invalid(
            "message muss nach allen Werkzeugbeobachtungen als einzige Action in einer eigenen Antwort stehen",
            text,
        );
    }

    ParseResult::valid(actions, text)
}

pub fn is_possibly_truncated(response_text: &str) -> bool {
    let text = strip_rendered_ui_controls(response_text);

    if text.starts_with("WEBAGENT/1 SHELL") {
        let re = script_envelope_regex();
        return !re.is_match(&text);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_envelope() -> Value {
        json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "s1", "type": "shell", "command": "Get-Location", "timeout_seconds": 30}
            ]
        })
    }

    #[test]
    fn test_parse_valid_envelope() {
        let result = parse(&serde_json::to_string(&valid_envelope()).unwrap());
        assert!(result.valid);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].id, "s1");
        assert_eq!(result.actions[0].command, "Get-Location");
    }

    #[test]
    fn test_parse_edit_action() {
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "C:/tmp/a.txt",
                 "old_string": "alt", "new_string": "neu"}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].action_type, ActionType::Edit);
        assert_eq!(result.actions[0].path, "C:/tmp/a.txt");
        assert_eq!(result.actions[0].old_string, "alt");
        assert_eq!(result.actions[0].new_string, "neu");
    }

    #[test]
    fn test_parse_edit_requires_fields() {
        for bad in [
            json!({"id": "e1", "type": "edit", "old_string": "a", "new_string": "b"}),
            json!({"id": "e1", "type": "edit", "path": "x", "new_string": "b"}),
            json!({"id": "e1", "type": "edit", "path": "x", "old_string": "a", "new_string": "a"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {env}");
        }
    }

    #[test]
    fn test_parse_write_action() {
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "w1", "type": "write", "path": "C:/tmp/neu.txt", "content": "zeile1\nzeile2\n"}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].action_type, ActionType::Write);
        assert_eq!(result.actions[0].content, "zeile1\nzeile2\n");
        // write ohne content wird abgelehnt (leerer content-String ist ok).
        let env = json!({"protocol": "webagent/1", "actions": [
            {"id": "w2", "type": "write", "path": "C:/tmp/neu.txt"}
        ]});
        assert!(!parse(&serde_json::to_string(&env).unwrap()).valid);
    }

    #[test]
    fn test_edit_batches_with_shell() {
        // edit/shell dürfen gemischt in einer Antwort stehen (seriell ausgeführt).
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y"},
                {"id": "s1", "type": "shell", "command": "cargo test", "timeout_seconds": 600}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 2);
    }

    #[test]
    fn test_parse_valid_markdown_block() {
        let text = format!(
            "```json\n{}\n```",
            serde_json::to_string(&valid_envelope()).unwrap()
        );
        let result = parse(&text);
        assert!(result.valid);
    }

    #[test]
    fn test_parse_raw_complex_powershell_envelope() {
        let text = r#"WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
$html = "<div style='color:red'>Hallo</div>"
Write-Output $html
---END SCRIPT---"#;
        let result = parse(text);
        assert!(result.valid);
        assert_eq!(result.actions[0].id, "report-1");
        assert!(result.actions[0].command.contains("style='color:red'"));
        assert_eq!(result.actions[0].timeout_seconds, 300.0);
    }

    #[test]
    fn test_complex_powershell_envelope_must_be_complete() {
        let partial = r#"WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
Write-Output "x""#;
        assert!(is_possibly_truncated(partial));
        assert!(!parse(partial).valid);
    }

    #[test]
    fn test_nested_raw_script_envelope_is_safely_unwrapped() {
        let envelope = r#"WEBAGENT/1 SHELL
id: nested-1
timeout_seconds: 120
---SCRIPT---
$html = @'
<h1>Quote "Test"</h1>
'@
Write-Output $html
---END SCRIPT---"#;
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "nested-1", "type": "shell", "command": envelope}]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert!(result.actions[0].command.starts_with("$html = @'"));
        assert!(!result.actions[0].command.contains("WEBAGENT/1 SHELL"));
        assert_eq!(result.actions[0].timeout_seconds, 120.0);
    }

    #[test]
    fn test_parse_rendered_json_code_block_label() {
        for label in &["JSON\n", "json\n", "Json\r\n"] {
            let text = format!(
                "{}{}",
                label,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&text);
            assert!(result.valid, "failed for label {:?}", label);
            assert_eq!(result.actions[0].id, "s1");
        }
    }

    #[test]
    fn test_parse_rendered_deepseek_code_controls() {
        for labels in &["json\nKopieren\nHerunterladen\n", "JSON\nCopy\nDownload\n"] {
            let text = format!(
                "{}{}",
                labels,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&text);
            assert!(result.valid, "failed for labels {:?}", labels);
        }
    }

    #[test]
    fn test_parse_qwen_rendered_line_numbers_and_nbsp() {
        let rendered = format!(
            "JSON\n1\n2\n3\n{}",
            serde_json::to_string(&valid_envelope())
                .unwrap()
                .replace(' ', "\u{00a0}")
        );
        let result = parse(&rendered);
        assert!(result.valid);
        assert_eq!(result.actions[0].id, "s1");
    }

    #[test]
    fn test_parse_strips_leading_prose_before_json_controls() {
        for prefix in &["Denke nach…\n", "Thinking\n", "Hier ist\n"] {
            let rendered = format!(
                "{}JSON\nCopy\n{}",
                prefix,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&rendered);
            assert!(result.valid, "failed for prefix {:?}", prefix);
            assert_eq!(result.actions[0].id, "s1");
        }
    }

    #[test]
    fn test_parse_tolerates_thought_process_and_prose_from_chatgpt_zai_deepseek() {
        for bad_prefix in &[
            "Thought Process\n\n",
            "Thought Process\njson\n",
            "Thinking...\n\n",
            "Thought Process\n\nVerstanden! Hier das JSON:\n",
        ] {
            let rendered = format!(
                "{}{}",
                bad_prefix,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&rendered);
            assert!(
                result.valid,
                "failed for prefix {:?}: {}",
                bad_prefix, result.error
            );
            assert_eq!(result.actions[0].id, "s1");
        }
    }

    #[test]
    fn test_repair_unescaped_windows_path_for_message_only() {
        let rendered = r#"{"protocol":"webagent/1","actions":[{"id":"answer","type":"message","text":"Pfad: C:\Users\storax\Desktop\webagent"}]}"#;
        let result = parse(rendered);
        assert!(result.valid);
        assert!(result.actions[0]
            .text
            .ends_with(r"C:\Users\storax\Desktop\webagent"));
    }

    #[test]
    fn test_never_repair_unescaped_windows_path_for_shell() {
        let rendered = r#"{"protocol":"webagent/1","actions":[{"id":"work","type":"shell","command":"Get-Item C:\Users\storax"}]}"#;
        assert!(!parse(rendered).valid);
    }

    #[test]
    fn test_message_must_be_separate_after_shell() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "work", "type": "shell", "command": "Get-Date"},
                {"id": "answer", "type": "message", "text": "fertig"}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("eigene"));
    }

    #[test]
    fn test_reject_text_outside_block() {
        let text = format!(
            "Hier:\n```json\n{}\n```",
            serde_json::to_string(&valid_envelope()).unwrap()
        );
        let result = parse(&text);
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_raw_fallback() {
        let result = parse("Get-Date");
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_wrong_protocol() {
        let bad = json!({"protocol": "other/1", "actions": [{"id": "x", "type": "finish"}]});
        let result = parse(&serde_json::to_string(&bad).unwrap());
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_missing_id() {
        let bad = json!({
            "protocol": "webagent/1",
            "actions": [{"type": "finish"}]
        });
        let result = parse(&serde_json::to_string(&bad).unwrap());
        assert!(!result.valid);
    }

    #[test]
    fn test_parse_finish() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "done", "type": "finish"}]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert_eq!(result.actions[0].action_type, ActionType::Finish);
    }

    #[test]
    fn test_finish_must_be_the_only_action() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "work", "type": "shell", "command": "Get-Date"},
                {"id": "done", "type": "finish"}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("einzige Action"));
    }

    #[test]
    fn test_detects_possibly_truncated_streamed_json() {
        for text in &[
            r#"{"pr"#,
            r#"{"protocol":"webagent/1","actions":[{"id":"x","type":"shell","command":"unterminated"#,
            r#"{"protocol":"webagent/1","actions":["#,
        ] {
            assert!(is_possibly_truncated(text), "failed for text {:?}", text);
        }
    }

    #[test]
    fn test_truncated_message_with_windows_path_is_not_released_early() {
        let text = r#"{"protocol":"webagent/1","actions":[{"id":"answer","type":"message","text":"Pfad C:\Users\storax"#;
        assert!(is_possibly_truncated(text));
    }

    #[test]
    fn test_does_not_mark_non_json_or_complete_json_as_truncated() {
        for text in &[
            "Denke nach…",
            "not json",
            &serde_json::to_string(&valid_envelope()).unwrap(),
        ] {
            assert!(!is_possibly_truncated(text), "failed for text {:?}", text);
        }
    }

    #[test]
    fn test_format_observation_includes_action_id() {
        let obs = format_observation("step-1", "hello", "", Some(0), false);
        assert!(obs.contains("action_id=step-1"));
        assert!(obs.contains("hello"));
    }

    #[test]
    fn test_shell_timeout_accepts_numeric_range() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "low", "type": "shell", "command": "Get-Date", "timeout_seconds": 0.1},
                {"id": "high", "type": "shell", "command": "Get-Date", "timeout_seconds": 3600}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert_eq!(
            result
                .actions
                .iter()
                .map(|a| a.timeout_seconds)
                .collect::<Vec<_>>(),
            vec![0.1, 3600.0]
        );
    }

    #[test]
    fn test_shell_timeout_rejects_invalid_values() {
        for timeout in &[json!(0), json!(-1), json!(3600.1), json!(true), json!("30")] {
            let doc = json!({
                "protocol": "webagent/1",
                "actions": [
                    {"id": "bad", "type": "shell", "command": "Get-Date", "timeout_seconds": timeout}
                ]
            });
            let result = parse(&serde_json::to_string(&doc).unwrap());
            assert!(!result.valid, "should reject timeout {:?}", timeout);
        }
    }

    #[test]
    fn test_parse_intentionally_broken_answers_are_invalid() {
        for bad in &[
            "",
            "nur Prosa ohne JSON",
            "{not json",
            r#"{"protocol":"webagent/2","actions":[{"id":"x","type":"finish"}]}"#,
            r#"{"protocol":"webagent/1","actions":[]}"#,
            r#"{"actions":[{"id":"x","type":"finish"}]}"#,
        ] {
            assert!(!parse(bad).valid, "expected invalid for {:?}", bad);
        }
    }

    #[test]
    fn test_protocol_repair_policy_two_repairs_then_abort() {
        assert!(!should_attempt_protocol_repair(0));
        assert!(should_attempt_protocol_repair(1));
        assert!(should_attempt_protocol_repair(2));
        assert!(!should_attempt_protocol_repair(3));
        assert!(!should_abort_protocol_repair(0));
        assert!(!should_abort_protocol_repair(1));
        assert!(!should_abort_protocol_repair(2));
        assert!(should_abort_protocol_repair(3));
        assert_eq!(PROTOCOL_REPAIR_MAX_FAILURES, 3);
    }

    #[test]
    fn test_reject_unknown_field_per_action_type() {
        // Je Action-type ein unbekanntes Feld → invalid.
        let cases = [
            json!({"id": "s1", "type": "shell", "command": "Get-Date", "foo": 1}),
            json!({"id": "m1", "type": "message", "text": "hi", "foo": 1}),
            json!({"id": "f1", "type": "finish", "foo": 1}),
            json!({"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y", "foo": 1}),
            json!({"id": "w1", "type": "write", "path": "a.txt", "content": "c", "foo": 1}),
        ];
        for bad in cases {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {bad}");
            assert!(
                result.error.contains("unbekanntes Feld") && result.error.contains("foo"),
                "unerwartete Fehlermeldung: {}",
                result.error
            );
        }
    }

    #[test]
    fn test_reject_typo_field_command() {
        // Klassischer Tippfehler: "comand" statt "command" darf nicht als leerer
        // Befehl durchrutschen, sondern muss als unbekanntes Feld auffliegen.
        let env = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "s1", "type": "shell", "comand": "Get-Date"}]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("comand"), "{}", result.error);
    }

    #[test]
    fn test_reject_cross_type_field() {
        // Feld existiert im Protokoll, aber nicht für diesen type (text bei shell,
        // command bei message, path bei finish).
        for bad in [
            json!({"id": "s1", "type": "shell", "command": "Get-Date", "text": "x"}),
            json!({"id": "m1", "type": "message", "text": "hi", "command": "Get-Date"}),
            json!({"id": "f1", "type": "finish", "path": "a.txt"}),
            json!({"id": "w1", "type": "write", "path": "a.txt", "content": "c", "old_string": "x"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {bad}");
            assert!(result.error.contains("unbekanntes Feld"), "{}", result.error);
        }
    }

    #[test]
    fn test_reject_whitespace_only_path_edit_write() {
        for bad in [
            json!({"id": "e1", "type": "edit", "path": "   ", "old_string": "x", "new_string": "y"}),
            json!({"id": "w1", "type": "write", "path": "  \t ", "content": "c"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "leerer/whitespace path muss abgelehnt werden: {bad}");
            assert!(result.error.contains("path"), "{}", result.error);
        }
    }

    #[test]
    fn test_allowed_fields_still_accepts_full_valid_actions() {
        // Gegenprobe: alle erlaubten Felder je type werden weiterhin akzeptiert.
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y"},
                {"id": "s1", "type": "shell", "command": "Get-Date", "timeout_seconds": 30}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 2);
    }

    #[test]
    fn test_format_protocol_error_demands_valid_webagent_json_only() {
        let msg = format_protocol_error("Ungültiges JSON: trailing comma");
        assert!(msg.contains(PROTOCOL_VERSION));
        assert!(msg.contains("NUR mit genau diesem Format") || msg.contains("Repair"));
        assert!(msg.contains("Ungültiges JSON: trailing comma"));
        assert!(msg.contains("repair-1"));
        assert!(msg.contains(r#""protocol""#));
    }
}
