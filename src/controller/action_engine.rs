//! Native Datei-Action-Ausführung, getrennt vom Session-Lifecycle.

use std::path::Path;

use crate::protocol::{Action, ActionType};

pub(super) struct FileActionResult {
    pub kind: &'static str,
    pub target: String,
    pub result: Result<String, String>,
}

pub(super) fn execute_file_action(
    workspace_root: Option<&Path>,
    action: &Action,
) -> FileActionResult {
    let (kind, target, result) = match action.action_type {
        ActionType::Edit => (
            "edit",
            action.path.clone(),
            match workspace_root {
                Some(root) => crate::file_actions::apply_edit_in(
                    root,
                    &action.path,
                    &action.old_string,
                    &action.new_string,
                ),
                None => crate::file_actions::apply_edit(
                    &action.path,
                    &action.old_string,
                    &action.new_string,
                ),
            },
        ),
        ActionType::EditBatch => (
            "edit_batch",
            format!("{} Edits", action.edits.len()),
            match workspace_root {
                Some(root) => crate::file_actions::apply_edit_batch_in(root, &action.edits),
                None => crate::file_actions::apply_edit_batch(&action.edits),
            },
        ),
        ActionType::Write => (
            "write",
            action.path.clone(),
            match workspace_root {
                Some(root) => {
                    crate::file_actions::apply_write_in(root, &action.path, &action.content)
                }
                None => crate::file_actions::apply_write(&action.path, &action.content),
            },
        ),
        _ => unreachable!("execute_file_action nur für Datei-Actions aufrufen"),
    };
    FileActionResult {
        kind,
        target,
        result,
    }
}
