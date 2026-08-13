//! Billiges, lokales Machbarkeits-Gate vor einem Brain-Baulauf.

use std::path::{Component, Path};

use regex::Regex;

use super::work_package::{AnchorRequirement, WorkPackage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeasibilityIssue {
    MissingId,
    MissingObjective,
    MissingScope,
    InvalidPath(String),
    MissingPath(String),
    AnchorOutsideScope { symbol: String, path: String },
    MissingDefinition { symbol: String, path: String },
    DefinitionAlreadyExists { symbol: String, path: String },
    MissingAcceptance,
    InvalidAcceptance(String),
    DeniedAcceptance { command: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feasibility {
    Ready,
    Rejected(Vec<FeasibilityIssue>),
}

impl Feasibility {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }
}

pub fn evaluate_work_package(package: &WorkPackage, root: &Path) -> Feasibility {
    let mut issues = Vec::new();
    if package.id.trim().is_empty() {
        issues.push(FeasibilityIssue::MissingId);
    }
    if package.objective.trim().is_empty() {
        issues.push(FeasibilityIssue::MissingObjective);
    }
    if package.allowed_paths.is_empty() {
        issues.push(FeasibilityIssue::MissingScope);
    }

    for path in &package.allowed_paths {
        if !safe_relative_path(path) {
            issues.push(FeasibilityIssue::InvalidPath(path.clone()));
        } else if !root.join(path).is_file() {
            issues.push(FeasibilityIssue::MissingPath(path.clone()));
        }
    }

    for anchor in &package.anchors {
        if !package.allowed_paths.iter().any(|p| p == &anchor.path) {
            issues.push(FeasibilityIssue::AnchorOutsideScope {
                symbol: anchor.symbol.clone(),
                path: anchor.path.clone(),
            });
            continue;
        }
        if anchor.symbol.trim().is_empty() || !safe_relative_path(&anchor.path) {
            issues.push(FeasibilityIssue::InvalidPath(anchor.path.clone()));
            continue;
        }
        let content = std::fs::read_to_string(root.join(&anchor.path)).unwrap_or_default();
        let exists = contains_rust_definition(&content, &anchor.symbol);
        match (anchor.requirement, exists) {
            (AnchorRequirement::MustExist, false) => {
                issues.push(FeasibilityIssue::MissingDefinition {
                    symbol: anchor.symbol.clone(),
                    path: anchor.path.clone(),
                });
            }
            (AnchorRequirement::MustNotExist, true) => {
                issues.push(FeasibilityIssue::DefinitionAlreadyExists {
                    symbol: anchor.symbol.clone(),
                    path: anchor.path.clone(),
                });
            }
            _ => {}
        }
    }

    if package.acceptance.is_empty() {
        issues.push(FeasibilityIssue::MissingAcceptance);
    }
    for check in &package.acceptance {
        if check.command.trim().is_empty() || check.purpose.trim().is_empty() {
            issues.push(FeasibilityIssue::InvalidAcceptance(check.command.clone()));
            continue;
        }
        if let crate::shell_policy::Decision::Deny(reason) =
            crate::shell_policy::evaluate(&check.command)
        {
            issues.push(FeasibilityIssue::DeniedAcceptance {
                command: check.command.clone(),
                reason,
            });
        }
    }

    if issues.is_empty() {
        Feasibility::Ready
    } else {
        Feasibility::Rejected(issues)
    }
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn contains_rust_definition(content: &str, symbol: &str) -> bool {
    if !symbol
        .chars()
        .enumerate()
        .all(|(i, c)| c == '_' || c.is_ascii_alphanumeric() && (i > 0 || !c.is_ascii_digit()))
    {
        return false;
    }
    let symbol = regex::escape(symbol);
    let pattern = format!(
        r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+|unsafe\s+|const\s+)*(?:fn|struct|enum|trait|type|const|static|mod)\s+{symbol}\b"
    );
    Regex::new(&pattern).is_ok_and(|re| re.is_match(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::{AcceptanceCheck, CodeAnchor};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Zeit")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("webagent-work-package-{nonce}"));
        std::fs::create_dir_all(root.join("src")).expect("Testverzeichnis");
        std::fs::write(
            root.join("src/lib.rs"),
            "// helper nur im Kommentar\npub fn existing() {}\nstruct LocalType;\n",
        )
        .expect("Testdatei");
        root
    }

    fn package(requirement: AnchorRequirement, symbol: &str) -> WorkPackage {
        WorkPackage {
            id: "t1".into(),
            objective: "Aendere das Verhalten".into(),
            allowed_paths: vec!["src/lib.rs".into()],
            anchors: vec![CodeAnchor {
                symbol: symbol.into(),
                path: "src/lib.rs".into(),
                requirement,
            }],
            acceptance: vec![AcceptanceCheck {
                command: "cargo test --lib".into(),
                purpose: "Regressionen".into(),
            }],
        }
    }

    #[test]
    fn vorhandene_definition_ist_machbar() {
        let root = root();
        assert!(
            evaluate_work_package(&package(AnchorRequirement::MustExist, "existing"), &root)
                .is_ready()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn kommentar_zaehlt_nicht_als_definition() {
        let root = root();
        let result = evaluate_work_package(&package(AnchorRequirement::MustExist, "helper"), &root);
        assert!(
            matches!(result, Feasibility::Rejected(ref x) if x.iter().any(|i| matches!(i, FeasibilityIssue::MissingDefinition { symbol, .. } if symbol == "helper")))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn neues_symbol_muss_noch_fehlen() {
        let root = root();
        assert!(
            evaluate_work_package(&package(AnchorRequirement::MustNotExist, "new_api"), &root)
                .is_ready()
        );
        let result =
            evaluate_work_package(&package(AnchorRequirement::MustNotExist, "existing"), &root);
        assert!(
            matches!(result, Feasibility::Rejected(ref x) if x.iter().any(|i| matches!(i, FeasibilityIssue::DefinitionAlreadyExists { .. })))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn scope_und_acceptance_werden_lokal_geprueft() {
        let root = root();
        let mut p = package(AnchorRequirement::MustExist, "existing");
        p.anchors[0].path = "src/other.rs".into();
        p.acceptance[0].command = "Remove-Item C:\\tmp -Recurse".into();
        let result = evaluate_work_package(&p, &root);
        assert!(matches!(result, Feasibility::Rejected(ref x)
            if x.iter().any(|i| matches!(i, FeasibilityIssue::AnchorOutsideScope { .. }))
            && x.iter().any(|i| matches!(i, FeasibilityIssue::DeniedAcceptance { .. }))));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn traversal_und_fehlende_datei_werden_verworfen() {
        let root = root();
        let mut p = package(AnchorRequirement::MustExist, "existing");
        p.allowed_paths = vec!["../secret.rs".into(), "src/missing.rs".into()];
        let result = evaluate_work_package(&p, &root);
        assert!(matches!(result, Feasibility::Rejected(ref x)
            if x.iter().any(|i| matches!(i, FeasibilityIssue::InvalidPath(_)))
            && x.iter().any(|i| matches!(i, FeasibilityIssue::MissingPath(_)))));
        let _ = std::fs::remove_dir_all(root);
    }
}
