//! Transaktionale Mehrdatei-/Mehrhunk-Edits.

use std::fs;
use std::path::{Path, PathBuf};

use crate::protocol::EditOperation;

use super::{
    anchor_ambiguous, anchor_not_found, current_workspace_root, resolve_existing,
    whitespace_tolerant_span,
};

pub fn apply_edit_batch_in(
    workspace_root: &Path,
    edits: &[EditOperation],
) -> Result<String, String> {
    if edits.is_empty() {
        return Err("edit_batch fehlgeschlagen: keine Edits angegeben".to_string());
    }

    let mut states: Vec<(PathBuf, String, String)> = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        let path = resolve_existing(workspace_root, Path::new(&edit.path))
            .map_err(|e| format!("edit_batch fehlgeschlagen bei Edit {}: {e}", index + 1))?;
        if !path.is_file() {
            return Err(format!(
                "edit_batch fehlgeschlagen bei Edit {}: {} ist keine Datei",
                index + 1,
                edit.path
            ));
        }
        let state_index = if let Some(i) = states.iter().position(|(p, _, _)| p == &path) {
            i
        } else {
            let original = fs::read_to_string(&path).map_err(|e| {
                format!(
                    "edit_batch fehlgeschlagen bei Edit {}: {} nicht als UTF-8 lesbar: {e}",
                    index + 1,
                    edit.path
                )
            })?;
            states.push((path.clone(), original.clone(), original));
            states.len() - 1
        };
        let current = &mut states[state_index].2;
        let (old, new) = batch_anchor(current, &edit.old_string, &edit.new_string, &edit.path)
            .map_err(|e| format!("edit_batch fehlgeschlagen bei Edit {}: {e}", index + 1))?;
        *current = current.replacen(&old, &new, 1);
    }

    for (written, (path, _, updated)) in states.iter().enumerate() {
        if let Err(error) = fs::write(path, updated) {
            for (rollback_path, original, _) in states.iter().take(written) {
                let _ = fs::write(rollback_path, original);
            }
            return Err(format!(
                "edit_batch fehlgeschlagen beim Schreiben von {}: {error}; vorherige Writes wurden zurückgerollt",
                path.display()
            ));
        }
    }

    Ok(format!(
        "edit_batch ok: {} Ersetzungen in {} Datei(en) angewandt.",
        edits.len(),
        states.len()
    ))
}

pub fn apply_edit_batch(edits: &[EditOperation]) -> Result<String, String> {
    let root = current_workspace_root()?;
    apply_edit_batch_in(&root, edits)
}

fn batch_anchor(
    content: &str,
    old: &str,
    new: &str,
    path: &str,
) -> Result<(String, String), String> {
    if old.is_empty() {
        return Err(format!("{path}: old_string ist leer"));
    }
    match content.matches(old).count() {
        1 => return Ok((old.to_string(), new.to_string())),
        n if n > 1 => return Err(anchor_ambiguous(path, n)),
        _ => {}
    }
    let (alternate_old, alternate_new) = if old.contains("\r\n") {
        (old.replace("\r\n", "\n"), new.replace("\r\n", "\n"))
    } else {
        (old.replace('\n', "\r\n"), new.replace('\n', "\r\n"))
    };
    match content.matches(&alternate_old).count() {
        1 => Ok((alternate_old, alternate_new)),
        n if n > 1 => Err(anchor_ambiguous(path, n)),
        _ => match whitespace_tolerant_span(content, old) {
            Ok((start, end)) => Ok((
                content[start..end].to_string(),
                new.trim_end_matches(['\r', '\n']).to_string(),
            )),
            Err(0) => Err(anchor_not_found(path, content, old)),
            Err(n) => Err(anchor_ambiguous(path, n)),
        },
    }
}
