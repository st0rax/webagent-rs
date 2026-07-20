//! Native Datei-Aktionen für das webagent/1-Protokoll: `edit` (eindeutiger
//! Anker-Ersatz) und `write` (neue Datei). Bewusst NICHT über die Shell —
//! kein Escaping-/Encoding-Risiko, präzise Fehlermeldungen als Observation.

use std::fs;
use std::path::Path;

/// Ersetzt `old` exakt einmal durch `new` in der Datei `path`.
///
/// Fehlerfälle liefern eine präzise, brain-lesbare Meldung: Datei fehlt,
/// kein UTF-8, Anker nicht gefunden (mit Zeilenenden-Toleranz CRLF↔LF),
/// Anker mehrdeutig (Trefferzahl).
pub fn apply_edit(path: &str, old: &str, new: &str) -> Result<String, String> {
    let p = Path::new(path);
    if !p.is_file() {
        return Err(format!(
            "edit fehlgeschlagen: Datei nicht gefunden: {path}. Fuer neue Dateien die write-Action nutzen."
        ));
    }
    let content = fs::read_to_string(p)
        .map_err(|e| format!("edit fehlgeschlagen: {path} nicht als UTF-8 lesbar: {e}"))?;

    // 1. Versuch: exakter Match.
    let (old_eff, new_eff, note) = match content.matches(old).count() {
        1 => (old.to_string(), new.to_string(), ""),
        0 => {
            // Zeilenenden-Toleranz: Brain schickt oft LF, Datei hat CRLF (oder
            // umgekehrt). Anker und Ersatz konsistent umkodieren und erneut zählen.
            let (alt_old, alt_new) = if old.contains("\r\n") {
                (old.replace("\r\n", "\n"), new.replace("\r\n", "\n"))
            } else {
                (
                    old.replace('\n', "\r\n"),
                    new.replace('\n', "\r\n"),
                )
            };
            if alt_old == old {
                return Err(anchor_not_found(path));
            }
            match content.matches(&alt_old).count() {
                1 => (alt_old, alt_new, " (Zeilenenden-Toleranz CRLF/LF angewandt)"),
                0 => return Err(anchor_not_found(path)),
                n => return Err(anchor_ambiguous(path, n)),
            }
        }
        n => return Err(anchor_ambiguous(path, n)),
    };

    let idx = content.find(&old_eff).expect("Match wurde soeben gezaehlt");
    let line = content[..idx].matches('\n').count() + 1;
    let updated = content.replacen(&old_eff, &new_eff, 1);
    fs::write(p, &updated).map_err(|e| format!("edit fehlgeschlagen: {path} nicht schreibbar: {e}"))?;
    Ok(format!(
        "edit ok: {path} — Ersetzung ab Zeile {line}{note}. Datei jetzt {} Zeilen.",
        updated.lines().count()
    ))
}

fn anchor_not_found(path: &str) -> String {
    format!(
        "edit fehlgeschlagen: old_string in {path} nicht gefunden. Kopiere den Anker \
         EXAKT aus der Datei (inkl. Einrueckung); pruefe den Ist-Stand z.B. mit \
         Select-String oder Get-Content -TotalCount."
    )
}

fn anchor_ambiguous(path: &str, count: usize) -> String {
    format!(
        "edit fehlgeschlagen: old_string ist in {path} mehrdeutig ({count} Treffer). \
         Erweitere den Anker um umliegende Zeilen, bis er eindeutig ist."
    )
}

/// Schreibt eine NEUE Datei. Existiert `path` bereits, schlägt die Aktion fehl
/// (Änderungen an Bestandsdateien laufen über `edit` — kein stilles Überschreiben).
pub fn apply_write(path: &str, content: &str) -> Result<String, String> {
    let p = Path::new(path);
    if p.exists() {
        return Err(format!(
            "write fehlgeschlagen: {path} existiert bereits. Bestehende Dateien mit der edit-Action aendern."
        ));
    }
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("write fehlgeschlagen: Verzeichnis {} nicht anlegbar: {e}", parent.display()))?;
        }
    }
    fs::write(p, content)
        .map_err(|e| format!("write fehlgeschlagen: {path} nicht schreibbar: {e}"))?;
    Ok(format!(
        "write ok: {path} erstellt ({} Bytes, {} Zeilen).",
        content.len(),
        content.lines().count()
    ))
}

/// Kompakter Dateibaum des Arbeitsverzeichnisses für den Initial-Prompt
/// (Repo-Kontext, Phase 2). Bewusst begrenzt: max. Tiefe 3, max. `max_entries`
/// Zeilen, Build-/VCS-/Profil-Verzeichnisse übersprungen.
pub fn worktree_context(max_entries: usize) -> String {
    // Kill-Switch, z.B. für Worker in riesigen Arbeitsverzeichnissen.
    if std::env::var("WEBAGENT_NO_TREE").map(|v| v == "1").unwrap_or(false) {
        return String::new();
    }
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut lines: Vec<String> = Vec::new();
    collect_tree(&cwd, "", 0, 3, max_entries, &mut lines);
    if lines.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "Arbeitsverzeichnis: {}\nDateibaum (begrenzt, Tiefe<=3):\n",
        cwd.display()
    );
    let truncated = lines.len() >= max_entries;
    out.push_str(&lines.join("\n"));
    if truncated {
        out.push_str("\n… (gekuerzt)");
    }
    out
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "venv",
    ".venv",
    "__pycache__",
    "profiles",
    "runtime-workers",
    "_archive",
];

fn collect_tree(
    dir: &Path,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    out: &mut Vec<String>,
) {
    if depth >= max_depth || out.len() >= max_entries {
        return;
    }
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| (e.path().is_file(), e.file_name()));
    for entry in entries {
        if out.len() >= max_entries {
            return;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            out.push(format!("{prefix}{name}/"));
            collect_tree(&path, &format!("{prefix}  "), depth + 1, max_depth, max_entries, out);
        } else {
            out.push(format!("{prefix}{name}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("webagent_file_actions_tests");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(format!("{}_{}", std::process::id(), name));
        let _ = fs::remove_file(&p);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn edit_replaces_unique_anchor() {
        let p = temp_file("unique.txt", "alpha\nbeta\ngamma\n");
        let msg = apply_edit(p.to_str().unwrap(), "beta", "BETA").unwrap();
        assert!(msg.contains("edit ok"), "{msg}");
        assert!(msg.contains("Zeile 2"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "alpha\nBETA\ngamma\n");
    }

    #[test]
    fn edit_rejects_missing_anchor() {
        let p = temp_file("missing.txt", "alpha\n");
        let err = apply_edit(p.to_str().unwrap(), "nicht-da", "x").unwrap_err();
        assert!(err.contains("nicht gefunden"), "{err}");
    }

    #[test]
    fn edit_rejects_ambiguous_anchor() {
        let p = temp_file("ambig.txt", "dup\ndup\n");
        let err = apply_edit(p.to_str().unwrap(), "dup", "x").unwrap_err();
        assert!(err.contains("mehrdeutig (2 Treffer)"), "{err}");
    }

    #[test]
    fn edit_tolerates_crlf_file_with_lf_anchor() {
        let p = temp_file("crlf.txt", "eins\r\nzwei\r\ndrei\r\n");
        let msg = apply_edit(p.to_str().unwrap(), "zwei\ndrei", "zwei\nDREI").unwrap();
        assert!(msg.contains("Zeilenenden-Toleranz"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "eins\r\nzwei\r\nDREI\r\n");
    }

    #[test]
    fn edit_missing_file_points_to_write() {
        let err = apply_edit("Z:/gibts/nicht.txt", "a", "b").unwrap_err();
        assert!(err.contains("write-Action"), "{err}");
    }

    #[test]
    fn edit_empty_new_deletes_anchor() {
        let p = temp_file("delete.txt", "bleibt-WEG-bleibt");
        apply_edit(p.to_str().unwrap(), "-WEG-", "").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "bleibtbleibt");
    }

    #[test]
    fn edit_handles_umlauts() {
        // Dogfood-Fund (zai, Testfall 3): UTF-8-korrekte Suche mit Umlauten.
        let p = temp_file("umlaut.txt", "Grüße aus Köln\nTschüß\n");
        apply_edit(p.to_str().unwrap(), "Köln", "Düsseldorf").unwrap();
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "Grüße aus Düsseldorf\nTschüß\n"
        );
    }

    #[test]
    fn edit_at_file_start_and_end() {
        // Dogfood-Fund (zai, Testfall 8): Anker in erster/letzter Zeile.
        let p = temp_file("bounds.txt", "erste\nmitte\nletzte");
        apply_edit(p.to_str().unwrap(), "erste", "ERSTE").unwrap();
        apply_edit(p.to_str().unwrap(), "letzte", "LETZTE").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "ERSTE\nmitte\nLETZTE");
    }

    #[test]
    fn write_creates_new_file_with_parents() {
        let dir = std::env::temp_dir().join("webagent_file_actions_tests");
        let p = dir.join(format!("{}_sub/neu.txt", std::process::id()));
        let _ = fs::remove_file(&p);
        let msg = apply_write(p.to_str().unwrap(), "inhalt\n").unwrap();
        assert!(msg.contains("write ok"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "inhalt\n");
    }

    #[test]
    fn write_refuses_existing_file() {
        let p = temp_file("existiert.txt", "alt");
        let err = apply_write(p.to_str().unwrap(), "neu").unwrap_err();
        assert!(err.contains("existiert bereits"), "{err}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "alt");
    }

    #[test]
    fn worktree_context_is_bounded() {
        let ctx = worktree_context(10);
        // Im Repo-Root aufgerufen: nicht leer, nie mehr als max_entries+Kopf.
        if !ctx.is_empty() {
            assert!(ctx.lines().count() <= 13, "{ctx}");
        }
    }
}
