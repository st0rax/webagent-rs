//! ToolRegistry der vier Managed Tools: read / bash / edit / write.
//!
//! Dieser Vertrag modelliert, welche Tools es gibt, welche Felder ein
//! Tool-Aufruf braucht bzw. darf (Validierung), welche Sicherheits-Grenzen
//! gelten (Policy) und dass jede Action-ID hoechstens einmal ausgefuehrt wird
//! (Exactly-once). Bestehende Aktionen (`protocol::ActionType`) werden nicht
//! umbenannt; `read` ist die einzige Ergaenzung als eigene Tool-Kennung.

use std::collections::HashSet;

use crate::protocol::ActionType;

/// Die vier Managed Tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ManagedTool {
    Read,
    Bash,
    Edit,
    Write,
}

impl ManagedTool {
    /// Kannonische Kennung (nicht umbennen; nutzbar als Registry-Schluessel).
    pub fn name(self) -> &'static str {
        match self {
            ManagedTool::Read => "read",
            ManagedTool::Bash => "bash",
            ManagedTool::Edit => "edit",
            ManagedTool::Write => "write",
        }
    }

    /// Regelt, welchem bestehenden Aktionstyp dieser Tool-Aufruf ueberhaupt
    /// entspricht (validierung gegen `protocol::ActionType`).
    pub fn action_type(self) -> Option<ActionType> {
        match self {
            ManagedTool::Read => None, // read hat keinen eigenen ActionType
            ManagedTool::Bash => Some(ActionType::Shell),
            ManagedTool::Edit => Some(ActionType::Edit),
            ManagedTool::Write => Some(ActionType::Write),
        }
    }
}

/// Fehler beim Registrieren/Ausfuehren eines Tool-Aufrufs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// Action-ID wurde bereits ausgefuehrt (Exactly-once verletzt).
    DuplicateAction(String),
    /// Pflichtfeld fehlt (z. B. `path` bei read/edit/write, `command` bei bash).
    MissingField(&'static str),
    /// Feld ist bei diesem Tool unzulaessig.
    DisallowedField(&'static str),
    /// Policy-Grenze verletzt (z. B. Pfad ausserhalb erlaubter Zone).
    PolicyViolation(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::DuplicateAction(id) => write!(f, "Action '{id}' wurde bereits ausgefuehrt"),
            ToolError::MissingField(field) => write!(f, "Pflichtfeld fehlt: {field}"),
            ToolError::DisallowedField(field) => write!(f, "Feld unzulaessig bei diesem Tool: {field}"),
            ToolError::PolicyViolation(msg) => write!(f, "Policy verletzt: {msg}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Zulassige Pfadzone: Datei-Tools (read/edit/write) duerfen nur relative,
/// nicht-ausbrechende Pfade nutzen (kein absolut, kein `..`).
pub const ALLOWED_PATH_PREFIXES: &[&str] = &["."]; // dokumentierend; Zone wird in check_path_prefix durchgesetzt
/// Max. Bash-Timeout in Sekunden (Policy-Grenze).
pub const MAX_BASH_TIMEOUT_SECS: f64 = 600.0;
/// Bash-Kommandos, die grundsaetzlich verweigert werden (Policy).
pub const FORBIDDEN_BASH_PREFIXES: &[&str] = &["rm -rf /", "format ", "del /s"];

/// Aufruf-Schema fuer einen Tool-Aufruf. Enthaelt nur die Felder, die
/// validiert werden; die eigentliche Ausfuehrung uebernimmt der Kern
/// (AgentController/executor), diese Registry prueft Vertrag + Exactly-once.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolInvocation {
    pub tool: ManagedTool,
    pub id: String,
    pub command: Option<String>,
    pub path: Option<String>,
    pub old_string: Option<String>,
    pub new_string: Option<String>,
    pub content: Option<String>,
    pub timeout_seconds: f64,
}

/// Die ToolRegistry an sich: haelt Exactly-once-IDs und fuehrt Validierung +
/// Policy-Checks aus.
#[derive(Debug, Default, Clone)]
pub struct ToolRegistry {
    executed: HashSet<String>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Hat eine Action-ID bereits eine Ausfuehrung gesehen?
    pub fn contains(&self, id: &str) -> bool {
        self.executed.contains(id)
    }

    /// Prueft only-validierung + Exactly-once, ohne auszufuehren.
    /// Gibt bei Erfolg `Ok(())`; bei Fehler den ersten aufgetretenen Mangel.
    pub fn validate(&self, inv: &ToolInvocation) -> Result<(), ToolError> {
        // Exactly-once
        if self.executed.contains(&inv.id) {
            return Err(ToolError::DuplicateAction(inv.id.clone()));
        }
        match inv.tool {
            ManagedTool::Bash => {
                let command = inv
                    .command
                    .as_deref()
                    .ok_or(ToolError::MissingField("command"))?;
                if inv.path.is_some() {
                    return Err(ToolError::DisallowedField("path"));
                }
                if inv.content.is_some() {
                    return Err(ToolError::DisallowedField("content"));
                }
                // Policy: Timeout
                if inv.timeout_seconds > MAX_BASH_TIMEOUT_SECS {
                    return Err(ToolError::PolicyViolation(format!(
                        "Bash-Timeout {}s ueber Limit {MAX_BASH_TIMEOUT_SECS}s",
                        inv.timeout_seconds
                    )));
                }
                // Policy: verbotene Kommandos
                for prefix in FORBIDDEN_BASH_PREFIXES {
                    if command.trim_start().starts_with(prefix) {
                        return Err(ToolError::PolicyViolation(format!(
                            "Kommando beginnt mit verbotenem Prefix '{prefix}'"
                        )));
                    }
                }
            }
            ManagedTool::Read => {
                let path = inv.path.as_deref().ok_or(ToolError::MissingField("path"))?;
                if inv.command.is_some() {
                    return Err(ToolError::DisallowedField("command"));
                }
                self.check_path_prefix(path)?;
            }
            ManagedTool::Edit => {
                let path = inv.path.as_deref().ok_or(ToolError::MissingField("path"))?;
                if inv.old_string.is_none() || inv.new_string.is_none() {
                    return Err(ToolError::MissingField("old_string/new_string"));
                }
                if inv.command.is_some() {
                    return Err(ToolError::DisallowedField("command"));
                }
                self.check_path_prefix(path)?;
            }
            ManagedTool::Write => {
                let path = inv.path.as_deref().ok_or(ToolError::MissingField("path"))?;
                if inv.content.is_none() {
                    return Err(ToolError::MissingField("content"));
                }
                if inv.command.is_some() {
                    return Err(ToolError::DisallowedField("command"));
                }
                self.check_path_prefix(path)?;
            }
        }
        Ok(())
    }

    /// Wie [`ToolRegistry::validate`], markiert die ID aber zusaetzlich als
    /// ausgefuehrt, wenn die Pruefung erfolgreich war (Exactly-once-Registrierung).
    pub fn register(&mut self, inv: &ToolInvocation) -> Result<(), ToolError> {
        self.validate(inv)?;
        self.executed.insert(inv.id.clone());
        Ok(())
    }

    fn check_path_prefix(&self, path: &str) -> Result<(), ToolError> {
        // Verboten: absolut (Wurzel/Escape) und Parent-Traversal (../).
        if path.starts_with('/')
            || path.starts_with("\\")
            || path == ".."
            || path.contains("..\\")
            || path.split('/').any(|seg| seg == "..")
            || path.split('\\').any(|seg| seg == "..")
        {
            return Err(ToolError::PolicyViolation(format!(
                "Pfad '{path}' liegt ausserhalb erlaubter Zonen (absolut/traversal)"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash(id: &str, command: &str) -> ToolInvocation {
        ToolInvocation {
            tool: ManagedTool::Bash,
            id: id.to_string(),
            command: Some(command.to_string()),
            path: None,
            old_string: None,
            new_string: None,
            content: None,
            timeout_seconds: 30.0,
        }
    }

    fn read(path: &str, id: &str) -> ToolInvocation {
        ToolInvocation {
            tool: ManagedTool::Read,
            id: id.to_string(),
            command: None,
            path: Some(path.to_string()),
            old_string: None,
            new_string: None,
            content: None,
            timeout_seconds: 30.0,
        }
    }

    fn edit(id: &str, path: &str) -> ToolInvocation {
        ToolInvocation {
            tool: ManagedTool::Edit,
            id: id.to_string(),
            command: None,
            path: Some(path.to_string()),
            old_string: Some("old".into()),
            new_string: Some("new".into()),
            content: None,
            timeout_seconds: 30.0,
        }
    }

    fn write(id: &str, path: &str) -> ToolInvocation {
        ToolInvocation {
            tool: ManagedTool::Write,
            id: id.to_string(),
            command: None,
            path: Some(path.to_string()),
            old_string: None,
            new_string: None,
            content: Some("inhalt".into()),
            timeout_seconds: 30.0,
        }
    }

    #[test]
    fn registry_vertraegt_vier_tools_getrennt_voneinander() {
        let mut r = ToolRegistry::new();
        r.register(&read("./a.txt", "r1")).unwrap();
        r.register(&bash("b1", "echo hi")).unwrap();
        r.register(&edit("e1", "./b.txt")).unwrap();
        r.register(&write("w1", "./c.txt")).unwrap();
        // Read hat keinen eigenen ActionType (kein Umbenennen bestehender Aktionen).
        assert_eq!(ManagedTool::Read.action_type(), None);
        assert_eq!(ManagedTool::Bash.action_type(), Some(ActionType::Shell));
        assert_eq!(ManagedTool::Edit.action_type(), Some(ActionType::Edit));
        assert_eq!(ManagedTool::Write.action_type(), Some(ActionType::Write));
    }

    #[test]
    fn exactly_once_leht_die_zweite_ausfuehrung_ab() {
        let mut r = ToolRegistry::new();
        r.register(&read("./a.txt", "id1")).unwrap();
        let second = read("./a.txt", "id1");
        assert_eq!(
            r.register(&second),
            Err(ToolError::DuplicateAction("id1".to_string()))
        );
    }

    #[test]
    fn validierung_bash_benoetigt_command_und_begrenzt_timeout() {
        let r = ToolRegistry::new();
        let mut inv = bash("b1", "echo hi");
        inv.command = None;
        assert_eq!(r.validate(&inv), Err(ToolError::MissingField("command")));

        let mut inv2 = bash("b2", "echo hi");
        inv2.timeout_seconds = 9999.0;
        assert!(matches!(r.validate(&inv2), Err(ToolError::PolicyViolation(_))));
    }

    #[test]
    fn policy_verbietet_gefaehrliche_bash_prefixe() {
        let r = ToolRegistry::new();
        for prefix in FORBIDDEN_BASH_PREFIXES {
            let inv = bash(&format!("x-{}", prefix.len()), prefix);
            assert!(
                matches!(r.validate(&inv), Err(ToolError::PolicyViolation(_))),
                "Prefix {prefix:?} haette abgelehnt werden muessen"
            );
        }
    }

    #[test]
    fn policy_begrenzt_dateipfade_auf_zone() {
        let r = ToolRegistry::new();
        // erlaubt: relativ zur Zone
        assert!(r.validate(&read("./ok.txt", "r1")).is_ok());
        assert!(r.validate(&read("ok.txt", "r2")).is_ok());
        // verweigert: absolute/ausbrechende Pfade
        assert!(matches!(
            r.validate(&read("/etc/passwd", "r3")),
            Err(ToolError::PolicyViolation(_))
        ));
        assert!(matches!(
            r.validate(&read("../outside.txt", "r4")),
            Err(ToolError::PolicyViolation(_))
        ));
    }

    #[test]
    fn validierung_edit_und_write_felder() {
        let r = ToolRegistry::new();
        // edit braucht old+new
        let mut inv = edit("e1", "./x.txt");
        inv.old_string = None;
        assert!(matches!(r.validate(&inv), Err(ToolError::MissingField(_))));
        // write braucht content
        let mut w2 = write("w1", "./y.txt");
        w2.content = None;
        assert!(matches!(r.validate(&w2), Err(ToolError::MissingField(_))));
        // bash darf kein path tragen, read darf kein command tragen
        let mut b = bash("b1", "ls");
        b.path = Some("./x".into());
        assert_eq!(r.validate(&b), Err(ToolError::DisallowedField("path")));
        let mut rd = read("./x", "r1");
        rd.command = Some("ls".into());
        assert_eq!(r.validate(&rd), Err(ToolError::DisallowedField("command")));
    }
}
