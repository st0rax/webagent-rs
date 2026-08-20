//! Versionierter lokaler Plan-/Zielvertrag fuer WebAgent.
//!
//! Dieser Kern verwaltet genau ein aktives Ziel und einen aktiven Plan. Er
//! delegiert keine Aktionen und akzeptiert einen Zielabschluss nur mit Evidenz
//! und einem expliziten unabhängigen PASS-Urteil.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Completed,
    Abandoned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoalRecord {
    pub schema_version: u32,
    pub id: String,
    pub objective: String,
    pub acceptance: Vec<String>,
    pub scope: Vec<String>,
    pub status: GoalStatus,
    pub created_at: String,
    pub evidence: Vec<String>,
    pub reviewer: Option<String>,
    pub verdict: Option<String>,
    pub closed_at: Option<String>,
    pub close_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanItem {
    pub id: u32,
    pub description: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRecord {
    pub schema_version: u32,
    pub goal_id: String,
    pub title: String,
    pub created_at: String,
    pub items: Vec<PlanItem>,
}

fn goals_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("goals")
}

fn active_goal_path(data_dir: &Path) -> PathBuf {
    goals_dir(data_dir).join("active.json")
}

fn active_plan_path(data_dir: &Path) -> PathBuf {
    goals_dir(data_dir).join("plan.json")
}

fn history_dir(data_dir: &Path) -> PathBuf {
    goals_dir(data_dir).join("history")
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Zielzustand kann nicht gelesen werden: {error}"))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("Zielzustand ist kein gültiges JSON: {error}"))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Zielzustand hat kein Elternverzeichnis.".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Zielverzeichnis kann nicht erstellt werden: {error}"))?;
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("Zielzustand kann nicht serialisiert werden: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{text}\n")).map_err(|error| {
        format!("Temporärer Zielzustand kann nicht geschrieben werden: {error}")
    })?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("Zielzustand kann nicht atomar ersetzt werden: {error}"))
}

fn archive_goal(data_dir: &Path, goal: &GoalRecord) -> Result<(), String> {
    let safe_id = goal
        .id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    write_json(&history_dir(data_dir).join(format!("{safe_id}.json")), goal)
}

fn require_active_goal(data_dir: &Path) -> Result<GoalRecord, String> {
    read_json(&active_goal_path(data_dir))?.ok_or_else(|| {
        "Kein aktives Ziel vorhanden. Verwende zuerst `webagent goal create`.".to_string()
    })
}

pub fn create_goal(
    data_dir: &Path,
    objective: String,
    acceptance: Vec<String>,
    scope: Vec<String>,
) -> Result<GoalRecord, String> {
    if objective.trim().is_empty() {
        return Err("Ein Ziel benötigt eine nichtleere Objective-Beschreibung.".to_string());
    }
    if acceptance.is_empty() {
        return Err("Ein Ziel benötigt mindestens ein Akzeptanzkriterium.".to_string());
    }
    if read_json::<GoalRecord>(&active_goal_path(data_dir))?.is_some() {
        return Err("Es gibt bereits ein aktives Ziel. Schließe es ab oder brich es ab, bevor ein neues Ziel angelegt wird.".to_string());
    }
    let created_at = crate::now_rfc3339();
    let goal = GoalRecord {
        schema_version: SCHEMA_VERSION,
        id: format!("goal-{}", created_at.replace([':', '+'], "-")),
        objective: objective.trim().to_string(),
        acceptance,
        scope,
        status: GoalStatus::Active,
        created_at,
        evidence: Vec::new(),
        reviewer: None,
        verdict: None,
        closed_at: None,
        close_reason: None,
    };
    write_json(&active_goal_path(data_dir), &goal)?;
    Ok(goal)
}

pub fn active_goal(data_dir: &Path) -> Result<Option<GoalRecord>, String> {
    read_json(&active_goal_path(data_dir))
}

pub fn complete_goal(
    data_dir: &Path,
    evidence: Vec<String>,
    reviewer: String,
    verdict: String,
) -> Result<GoalRecord, String> {
    if evidence.is_empty() {
        return Err("Ein Zielabschluss benötigt mindestens einen Evidenzverweis.".to_string());
    }
    if reviewer.trim().is_empty() {
        return Err("Ein Zielabschluss benötigt einen unabhängigen Reviewer.".to_string());
    }
    if !verdict.trim().eq_ignore_ascii_case("PASS") {
        return Err(
            "Ein Ziel darf nur mit dem unabhängigen Urteil PASS abgeschlossen werden.".to_string(),
        );
    }
    let mut goal = require_active_goal(data_dir)?;
    goal.status = GoalStatus::Completed;
    goal.evidence = evidence;
    goal.reviewer = Some(reviewer.trim().to_string());
    goal.verdict = Some("PASS".to_string());
    goal.closed_at = Some(crate::now_rfc3339());
    archive_goal(data_dir, &goal)?;
    let _ = fs::remove_file(active_goal_path(data_dir));
    let _ = fs::remove_file(active_plan_path(data_dir));
    Ok(goal)
}

pub fn abandon_goal(data_dir: &Path, reason: String) -> Result<GoalRecord, String> {
    if reason.trim().is_empty() {
        return Err("Ein abgebrochenes Ziel benötigt einen Grund.".to_string());
    }
    let mut goal = require_active_goal(data_dir)?;
    goal.status = GoalStatus::Abandoned;
    goal.close_reason = Some(reason.trim().to_string());
    goal.closed_at = Some(crate::now_rfc3339());
    archive_goal(data_dir, &goal)?;
    let _ = fs::remove_file(active_goal_path(data_dir));
    let _ = fs::remove_file(active_plan_path(data_dir));
    Ok(goal)
}

pub fn create_plan(
    data_dir: &Path,
    title: String,
    items: Vec<String>,
) -> Result<PlanRecord, String> {
    let goal = require_active_goal(data_dir)?;
    if title.trim().is_empty() {
        return Err("Ein Plan benötigt einen nichtleeren Titel.".to_string());
    }
    let items: Vec<PlanItem> = items
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .enumerate()
        .map(|(index, description)| PlanItem {
            id: (index + 1) as u32,
            description,
            done: false,
        })
        .collect();
    if items.is_empty() {
        return Err("Ein Plan benötigt mindestens eine Arbeitsscheibe.".to_string());
    }
    let plan = PlanRecord {
        schema_version: SCHEMA_VERSION,
        goal_id: goal.id,
        title: title.trim().to_string(),
        created_at: crate::now_rfc3339(),
        items,
    };
    write_json(&active_plan_path(data_dir), &plan)?;
    Ok(plan)
}

pub fn active_plan(data_dir: &Path) -> Result<Option<PlanRecord>, String> {
    read_json(&active_plan_path(data_dir))
}

pub fn complete_plan_item(data_dir: &Path, item_id: u32) -> Result<PlanRecord, String> {
    let path = active_plan_path(data_dir);
    let mut plan = read_json::<PlanRecord>(&path)?.ok_or_else(|| {
        "Kein aktiver Plan vorhanden. Verwende zuerst `webagent plan create`.".to_string()
    })?;
    let item = plan
        .items
        .iter_mut()
        .find(|item| item.id == item_id)
        .ok_or_else(|| format!("Planschritt {item_id} existiert nicht."))?;
    item.done = true;
    write_json(&path, &plan)?;
    Ok(plan)
}

pub fn render_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|text| format!("{text}\n"))
        .map_err(|error| format!("Ausgabe kann nicht serialisiert werden: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "webagent_goal_plan_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            name
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn completed_goal_requires_evidence_and_pass() {
        let root = temp_root("complete");
        create_goal(
            &root,
            "Harness finalisieren".into(),
            vec!["Review PASS".into()],
            vec![],
        )
        .unwrap();
        assert!(complete_goal(&root, vec![], "reviewer".into(), "PASS".into()).is_err());
        assert!(complete_goal(
            &root,
            vec!["evidence.json".into()],
            "reviewer".into(),
            "FAIL".into()
        )
        .is_err());
        let goal = complete_goal(
            &root,
            vec!["evidence.json".into()],
            "reviewer".into(),
            "PASS".into(),
        )
        .unwrap();
        assert_eq!(goal.status, GoalStatus::Completed);
        assert!(active_goal(&root).unwrap().is_none());
    }

    #[test]
    fn plan_binds_to_active_goal_and_marks_items_done() {
        let root = temp_root("plan");
        assert!(create_plan(&root, "Harness".into(), vec!["Test".into()]).is_err());
        create_goal(
            &root,
            "Harness finalisieren".into(),
            vec!["Review PASS".into()],
            vec![],
        )
        .unwrap();
        let plan = create_plan(
            &root,
            "Harness".into(),
            vec!["Build".into(), "Review".into()],
        )
        .unwrap();
        assert_eq!(plan.goal_id, active_goal(&root).unwrap().unwrap().id);
        let plan = complete_plan_item(&root, 2).unwrap();
        assert!(plan.items[1].done);
        assert!(!plan.items[0].done);
    }
}
