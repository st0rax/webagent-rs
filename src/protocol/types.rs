use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "webagent/1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    Shell,
    Message,
    Finish,
    /// Eindeutiger Anker-Ersatz in einer Bestandsdatei (path/old_string/new_string).
    Edit,
    /// Mehrere Anker-Ersetzungen als eine validierte, transaktionale Action.
    EditBatch,
    /// Neue Datei anlegen (path/content); existierende Dateien werden abgelehnt.
    Write,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditOperation {
    pub path: String,
    pub old_string: String,
    pub new_string: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edits: Vec<EditOperation>,
}

impl Action {
    /// Basis-Action ohne typspezifische Felder — Konstruktor-Helfer.
    pub(crate) fn base(id: String, action_type: ActionType) -> Self {
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
            edits: Vec::new(),
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
    pub(crate) fn invalid(error: impl Into<String>, raw_text: impl Into<String>) -> Self {
        Self {
            valid: false,
            actions: Vec::new(),
            error: error.into(),
            raw_text: raw_text.into(),
        }
    }

    pub(crate) fn valid(actions: Vec<Action>, raw_text: impl Into<String>) -> Self {
        Self {
            valid: true,
            actions,
            error: String::new(),
            raw_text: raw_text.into(),
        }
    }
}

/// Bildet eine Fehlermeldung (die deutschen Strings aus ParseResult::invalid)
/// auf einen stabilen, maschinenlesbaren Slug ab.
/// Teilstring-Match case-insensitive, erste Regel gewinnt.
#[cfg(test)]
pub fn error_code(error: &str) -> &'static str {
    let lower = error.to_lowercase();
    if lower.contains("unbekannt") {
        "unknown_field"
    } else if lower.contains("braucht") {
        "missing_field"
    } else if lower.contains("protocol muss") {
        "protocol_mismatch"
    } else if lower.contains("doppelte") {
        "duplicate_id"
    } else if lower.contains("leer") {
        "empty"
    } else if lower.contains("identisch") {
        "identical_old_new"
    } else {
        "invalid"
    }
}
