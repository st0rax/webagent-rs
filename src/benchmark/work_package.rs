//! Typisierter Vertrag zwischen Aufgabenplanung, ausfuehrendem Brain und Gate.
//!
//! Das Paket beschreibt Ziele und nachweisbare Randbedingungen, aber bewusst
//! keine Schrittfolge. Das Brain darf selbst entscheiden, wie es den Auftrag
//! untersucht, implementiert und repariert.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkPackage {
    pub id: String,
    pub objective: String,
    pub allowed_paths: Vec<String>,
    #[serde(default)]
    pub anchors: Vec<CodeAnchor>,
    pub acceptance: Vec<AcceptanceCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeAnchor {
    pub symbol: String,
    pub path: String,
    pub requirement: AnchorRequirement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnchorRequirement {
    MustExist,
    MustNotExist,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceCheck {
    pub command: String,
    pub purpose: String,
}

/// Parst ausschliesslich ein JSON-Objekt. Markdown-Zaun und Begleitprosa sind
/// absichtlich Fehler: Eine halb erkannte Spezifikation darf keinen teuren
/// Brain-Lauf starten.
pub fn parse_work_package(text: &str) -> Result<WorkPackage, String> {
    let trimmed = text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err("WorkPackage muss ein einzelnes JSON-Objekt ohne Begleittext sein".into());
    }
    serde_json::from_str(trimmed).map_err(|e| format!("ungueltiges WorkPackage: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_liefert_typisiertes_paket() {
        let json = r#"{
            "id":"task-1",
            "objective":"Parser haerten",
            "allowed_paths":["src/protocol/parser.rs"],
            "anchors":[{"symbol":"parse_action","path":"src/protocol/parser.rs","requirement":"must_exist"}],
            "acceptance":[{"command":"cargo test --lib protocol::parser","purpose":"Parser-Regressions"}]
        }"#;
        let package = parse_work_package(json).expect("gueltiges Paket");
        assert_eq!(package.id, "task-1");
        assert_eq!(package.anchors[0].requirement, AnchorRequirement::MustExist);
    }

    #[test]
    fn parser_verwirft_prosa_und_unbekannte_felder() {
        assert!(parse_work_package("Hier ist es: {\"id\":\"x\"}").is_err());
        let json = r#"{"id":"x","objective":"y","allowed_paths":[],"acceptance":[],"steps":[]}"#;
        assert!(parse_work_package(json).is_err());
    }
}
