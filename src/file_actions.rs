//! Native Datei-Aktionen für das webagent/1-Protokoll: `edit` (eindeutiger
//! Anker-Ersatz) und `write` (neue Datei). Bewusst NICHT über die Shell —
//! kein Escaping-/Encoding-Risiko, präzise Fehlermeldungen als Observation.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Ermittelt die Sicherheitsgrenze fuer native Datei-Aktionen. Innerhalb eines
/// Git-Worktrees ist das der naechste Repo-Root, sonst das aktuelle
/// Arbeitsverzeichnis. Der Rueckgabepfad ist kanonisch (Symlinks aufgeloest).
pub fn current_workspace_root() -> Result<PathBuf, String> {
    let cwd = std::env::current_dir()
        .map_err(|e| format!("Arbeitsverzeichnis nicht ermittelbar: {e}"))?
        .canonicalize()
        .map_err(|e| format!("Arbeitsverzeichnis nicht kanonisierbar: {e}"))?;
    for ancestor in cwd.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor
                .canonicalize()
                .map_err(|e| format!("Repo-Root nicht kanonisierbar: {e}"));
        }
    }
    Ok(cwd)
}

fn reject_traversal(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("leerer Dateipfad ist nicht erlaubt".to_string());
    }
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(format!(
            "Pfad-Traversal mit '..' ist nicht erlaubt: {}",
            path.display()
        ));
    }
    if path.components().any(|c| matches!(c, Component::Prefix(_))) && !path.is_absolute() {
        return Err(format!(
            "laufwerksrelativer/praefixierter Pfad ist nicht erlaubt: {}",
            path.display()
        ));
    }
    #[cfg(windows)]
    for component in path.components() {
        if let Component::Normal(name) = component {
            let value = name.to_string_lossy();
            if value.contains("..") && value.trim_end_matches(['.', ' ']).is_empty() {
                return Err(format!(
                    "Windows-Pfadkomponente kann als Traversal normalisiert werden: {}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn canonical_root(root: &Path) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("Workspace-Root {} nicht kanonisierbar: {e}", root.display()))?;
    if !root.is_dir() {
        return Err(format!(
            "Workspace-Root ist kein Verzeichnis: {}",
            root.display()
        ));
    }
    Ok(root)
}

fn candidate_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn ensure_inside(root: &Path, canonical: &Path, supplied: &Path) -> Result<(), String> {
    if canonical == root || canonical.starts_with(root) {
        Ok(())
    } else {
        Err(format!(
            "Dateipfad liegt ausserhalb des Workspace-Roots {}: {} (aufgeloest: {})",
            root.display(),
            supplied.display(),
            canonical.display()
        ))
    }
}

/// Loest einen vorhandenen Zielpfad inklusive Symlinks auf und stellt sicher,
/// dass sein tatsaechliches Ziel innerhalb des Workspace-Roots liegt.
fn resolve_existing(root: &Path, path: &Path) -> Result<PathBuf, String> {
    reject_traversal(path)?;
    let root = canonical_root(root)?;
    let candidate = candidate_path(&root, path);
    if fs::symlink_metadata(&candidate).is_err() {
        return Err(format!(
            "Datei nicht gefunden: {}. Fuer neue Dateien die write-Action nutzen.",
            path.display()
        ));
    }
    let canonical = candidate.canonicalize().map_err(|e| {
        format!(
            "Dateipfad {} nicht kanonisierbar oder nicht vorhanden: {e}",
            path.display()
        )
    })?;
    ensure_inside(&root, &canonical, path)?;
    if canonical == root {
        return Err("Workspace-Root selbst ist keine bearbeitbare Datei".to_string());
    }
    Ok(canonical)
}

/// Validiert einen noch nicht vorhandenen Zielpfad ueber seinen naechsten
/// existierenden Elternpfad. Dadurch werden auch Symlink-Escapes in bereits
/// vorhandenen Elternverzeichnissen erkannt.
fn resolve_new(root: &Path, path: &Path) -> Result<PathBuf, String> {
    reject_traversal(path)?;
    let root = canonical_root(root)?;
    let candidate = candidate_path(&root, path);

    if fs::symlink_metadata(&candidate).is_ok() {
        return Err(format!(
            "write fehlgeschlagen: {} existiert bereits. Bestehende Dateien mit der edit-Action aendern.",
            path.display()
        ));
    }

    let mut existing = candidate.as_path();
    while fs::symlink_metadata(existing).is_err() {
        existing = existing.parent().ok_or_else(|| {
            format!(
                "kein existierender Elternpfad fuer {} gefunden",
                path.display()
            )
        })?;
    }
    let canonical_parent = existing
        .canonicalize()
        .map_err(|e| format!("Elternpfad {} nicht kanonisierbar: {e}", existing.display()))?;
    ensure_inside(&root, &canonical_parent, path)?;
    if candidate == root {
        return Err("Workspace-Root selbst ist keine neue Datei".to_string());
    }
    Ok(candidate)
}

/// Ersetzt `old` exakt einmal durch `new` in der Datei `path`.
///
/// Fehlerfälle liefern eine präzise, brain-lesbare Meldung: Datei fehlt,
/// kein UTF-8, Anker nicht gefunden (mit Zeilenenden-Toleranz CRLF↔LF),
/// Anker mehrdeutig (Trefferzahl).
pub fn apply_edit(path: &str, old: &str, new: &str) -> Result<String, String> {
    let root = current_workspace_root()?;
    apply_edit_in(&root, path, old, new)
}

/// Variante mit explizitem Workspace-Root, unter anderem fuer isolierte Tests
/// und Aufrufer, die ihren Worktree nicht ueber den Prozess-CWD festlegen.
pub fn apply_edit_in(
    workspace_root: &Path,
    path: &str,
    old: &str,
    new: &str,
) -> Result<String, String> {
    let p = resolve_existing(workspace_root, Path::new(path))
        .map_err(|e| format!("edit fehlgeschlagen: {e}"))?;
    if !p.is_file() {
        return Err(format!(
            "edit fehlgeschlagen: Datei nicht gefunden: {path}. Fuer neue Dateien die write-Action nutzen."
        ));
    }
    let content = fs::read_to_string(&p)
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
                (old.replace('\n', "\r\n"), new.replace('\n', "\r\n"))
            };
            let crlf_hit = if alt_old == old {
                0
            } else {
                content.matches(&alt_old).count()
            };
            match crlf_hit {
                1 => (
                    alt_old,
                    alt_new,
                    " (Zeilenenden-Toleranz CRLF/LF angewandt)",
                ),
                n if n > 1 => return Err(anchor_ambiguous(path, n)),
                // 0: letzter Fallback — whitespace-tolerantes Zeilen-Matching.
                // LLMs reproduzieren exakte Einrückung oft nicht; kimi hex-dumpte
                // am 2026-07-22 die Datei, um den Anker zu treffen. Wir matchen
                // Zeile für Zeile mit getrimmtem Whitespace, aber NUR wenn
                // eindeutig, und ersetzen den ECHTEN Datei-Span.
                _ => match whitespace_tolerant_span(&content, old) {
                    Ok((start, end)) => {
                        let line = content[..start].matches('\n').count() + 1;
                        let updated = format!(
                            "{}{}{}",
                            &content[..start],
                            new.trim_end_matches(['\r', '\n']),
                            &content[end..]
                        );
                        fs::write(&p, &updated).map_err(|e| {
                            format!("edit fehlgeschlagen: {path} nicht schreibbar: {e}")
                        })?;
                        return Ok(format!(
                            "edit ok: {path} — Ersetzung ab Zeile {line} (Whitespace-Toleranz angewandt). Datei jetzt {} Zeilen.",
                            updated.lines().count()
                        ));
                    }
                    Err(0) => return Err(anchor_not_found(path, &content, old)),
                    Err(n) => return Err(anchor_ambiguous(path, n)),
                },
            }
        }
        n => return Err(anchor_ambiguous(path, n)),
    };

    let idx = content.find(&old_eff).expect("Match wurde soeben gezaehlt");
    let line = content[..idx].matches('\n').count() + 1;
    let updated = content.replacen(&old_eff, &new_eff, 1);
    fs::write(&p, &updated)
        .map_err(|e| format!("edit fehlgeschlagen: {path} nicht schreibbar: {e}"))?;
    Ok(format!(
        "edit ok: {path} — Ersetzung ab Zeile {line}{note}. Datei jetzt {} Zeilen.",
        updated.lines().count()
    ))
}

/// Findet den Byte-Bereich `(start, end)` in `content`, an dem `old` Zeile für
/// Zeile matcht, wenn man führenden/nachlaufenden Whitespace je Zeile ignoriert.
///
/// Nur bei EINDEUTIGKEIT erfolgreich: `Err(0)` = kein Treffer, `Err(n>1)` =
/// mehrdeutig. `end` zeigt hinter das Inhaltsende der letzten Ankerzeile (ohne
/// deren Zeilenumbruch), damit die Ersetzung die Zeilenstruktur nicht zerstört.
fn whitespace_tolerant_span(content: &str, old: &str) -> Result<(usize, usize), usize> {
    let old_lines: Vec<&str> = old.lines().map(str::trim).collect();
    if old_lines.is_empty() {
        return Err(0);
    }
    // Zeilen des Inhalts mit Byte-Grenzen: (start, content_end ohne \r\n, trimmed).
    let mut spans: Vec<(usize, usize, &str)> = Vec::new();
    let mut pos = 0usize;
    for raw in content.split_inclusive('\n') {
        let start = pos;
        pos += raw.len();
        let trimmed_end = raw.trim_end_matches(['\r', '\n']);
        spans.push((start, start + trimmed_end.len(), trimmed_end.trim()));
    }
    let k = old_lines.len();
    if k > spans.len() {
        return Err(0);
    }
    let mut hits: Vec<(usize, usize)> = Vec::new();
    for i in 0..=spans.len() - k {
        if (0..k).all(|j| spans[i + j].2 == old_lines[j]) {
            hits.push((spans[i].0, spans[i + k - 1].1));
        }
    }
    match hits.len() {
        1 => Ok(hits[0]),
        n => Err(n),
    }
}

/// Anker nicht gefunden — MIT dem tatsaechlichen Dateiinhalt an der
/// wahrscheinlichsten Stelle.
///
/// # Warum der Ist-Stand mit hinein muss
///
/// Die alte Meldung riet, den Stand „z.B. mit Select-String" zu pruefen. Das
/// kostet einen kompletten weiteren Turn — und ein Turn kostet hier real 30 bis
/// 135 Sekunden (gemessen am 06.08.2026 ueber die Zeitstempel eines Laufs, der
/// nach 900s in den wall_timeout lief; 889 dieser 900 Sekunden waren
/// Brain-Antwortzeiten). Im selben Lauf scheiterten ZWEI Edits nacheinander an
/// ihren Ankern.
///
/// Wer dem Brain zeigt, was an der erwarteten Stelle wirklich steht, spart den
/// Leseturn und macht den zweiten Versuch treffsicher. Das ist kein Komfort,
/// das sind zwei Minuten Budget pro Fehlschlag.
fn anchor_not_found(path: &str, content: &str, old: &str) -> String {
    let hinweis = match nearest_context(content, old) {
        Some((line, block)) => format!(
            "\nSo steht es dort wirklich (ab Zeile {line}):\n{block}"
        ),
        None => String::new(),
    };
    format!(
        "edit fehlgeschlagen: old_string in {path} nicht gefunden. Kopiere den Anker \
         EXAKT aus der Datei (inkl. Einrueckung).{hinweis}"
    )
}

/// Sucht die Stelle, die dem Anker am naechsten kommt, und liefert
/// `(Startzeile, Ausschnitt mit Zeilennummern)`.
///
/// Gesucht wird ueber die erste nicht-leere Zeile des Ankers, mit getrimmtem
/// Whitespace — das ist genau die Zeile, die ein Brain am ehesten richtig
/// erinnert, waehrend Einrueckung und Folgezeilen abweichen.
fn nearest_context(content: &str, old: &str) -> Option<(usize, String)> {
    const UMGEBUNG: usize = 3;
    let anker = old.lines().map(str::trim).find(|l| !l.is_empty())?;
    if anker.len() < 4 {
        // Zu kurz, um damit sinnvoll zu suchen — sonst zeigen wir eine
        // beliebige Stelle und fuehren in die Irre.
        return None;
    }
    let zeilen: Vec<&str> = content.lines().collect();
    let treffer = zeilen
        .iter()
        .position(|l| l.trim() == anker)
        .or_else(|| zeilen.iter().position(|l| l.trim().contains(anker)))
        .or_else(|| {
            // Letzter Versuch: die Zeile, die den laengsten gemeinsamen Anfang hat.
            zeilen
                .iter()
                .enumerate()
                .map(|(i, l)| (i, gemeinsamer_anfang(l.trim(), anker)))
                .filter(|(_, n)| *n >= 8)
                .max_by_key(|(_, n)| *n)
                .map(|(i, _)| i)
        })?;
    let von = treffer.saturating_sub(UMGEBUNG);
    let bis = (treffer + UMGEBUNG + 1).min(zeilen.len());
    let block = zeilen[von..bis]
        .iter()
        .enumerate()
        .map(|(k, l)| format!("{:>5} | {l}", von + k + 1))
        .collect::<Vec<_>>()
        .join("\n");
    Some((von + 1, block))
}

fn gemeinsamer_anfang(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
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
    let root = current_workspace_root()?;
    apply_write_in(&root, path, content)
}

/// Variante mit explizitem Workspace-Root. Neue Elternverzeichnisse werden erst
/// nach erfolgreicher Escape-/Symlink-Pruefung angelegt.
pub fn apply_write_in(workspace_root: &Path, path: &str, content: &str) -> Result<String, String> {
    let p = resolve_new(workspace_root, Path::new(path))
        .map_err(|e| format!("write fehlgeschlagen: {e}"))?;
    let safe_target = if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "write fehlgeschlagen: Verzeichnis {} nicht anlegbar: {e}",
                    parent.display()
                )
            })?;
            // Nach dem Anlegen erneut kanonisieren: ein schon vorhandener
            // Symlink-Elternpfad darf auch in diesem Fenster nicht entkommen.
            let root =
                canonical_root(workspace_root).map_err(|e| format!("write fehlgeschlagen: {e}"))?;
            let canonical_parent = parent.canonicalize().map_err(|e| {
                format!(
                    "write fehlgeschlagen: Elternpfad {} nicht kanonisierbar: {e}",
                    parent.display()
                )
            })?;
            ensure_inside(&root, &canonical_parent, Path::new(path))
                .map_err(|e| format!("write fehlgeschlagen: {e}"))?;
            let file_name = p
                .file_name()
                .ok_or_else(|| format!("write fehlgeschlagen: ungueltiger Dateiname: {path}"))?;
            canonical_parent.join(file_name)
        } else {
            p.clone()
        }
    } else {
        p.clone()
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&safe_target)
        .map_err(|e| format!("write fehlgeschlagen: {path} nicht schreibbar: {e}"))?;
    file.write_all(content.as_bytes())
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
    let cwd = match std::env::current_dir() {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    worktree_context_in(&cwd, max_entries)
}

/// Variante von [`worktree_context`] fuer Controller mit explizit gebundenem
/// Workspace (z.B. Benchmark-Worktrees).
pub fn worktree_context_in(workspace_root: &Path, max_entries: usize) -> String {
    // Kill-Switch, z.B. für Worker in riesigen Arbeitsverzeichnissen.
    if std::env::var("WEBAGENT_NO_TREE")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    collect_tree(workspace_root, "", 0, 3, max_entries, &mut lines);
    if lines.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "Arbeitsverzeichnis: {}\nDateibaum (begrenzt, Tiefe<=3):\n",
        workspace_root.display()
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
            collect_tree(
                &path,
                &format!("{prefix}  "),
                depth + 1,
                max_depth,
                max_entries,
                out,
            );
        } else {
            out.push(format!("{prefix}{name}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Ein gescheiterter Anker muss den IST-Stand mitliefern.
    ///
    /// Gemessen am 06.08.2026: ein Brain-Turn kostet 30 bis 135 Sekunden. Die
    /// alte Meldung riet, den Stand selbst nachzulesen — das ist ein ganzer
    /// Turn. Im selben Lauf scheiterten zwei Edits nacheinander an ihren
    /// Ankern, danach lief er in den 900s-wall_timeout.
    #[test]
    fn gescheiterter_anker_zeigt_was_wirklich_dasteht() {
        let inhalt = "fn eins() {}\n\nfn contains_chaining(cmd: &str) -> bool {\n    let mut state = State::Normal;\n    true\n}\n";
        // Das Brain erinnert die Signatur falsch (anderer Parametername).
        let meldung = anchor_not_found(
            "src/shell_policy.rs",
            inhalt,
            "fn contains_chaining(command: &str) -> bool {",
        );
        assert!(meldung.contains("nicht gefunden"));
        assert!(
            meldung.contains("So steht es dort wirklich"),
            "Ist-Stand fehlt: {meldung}"
        );
        assert!(
            meldung.contains("fn contains_chaining(cmd: &str)"),
            "die echte Zeile muss drinstehen: {meldung}"
        );
        assert!(
            meldung.contains("    3 |"),
            "Zeilennummern muessen mit: {meldung}"
        );
    }

    #[test]
    fn zu_kurzer_anker_zeigt_lieber_nichts() {
        // Bei einem Zweizeichen-Anker waere jede Fundstelle Zufall — dann in
        // die Irre zu fuehren ist schlechter als zu schweigen.
        let meldung = anchor_not_found("a.rs", "fn eins() {}\nfn zwei() {}\n", "{}");
        assert!(!meldung.contains("So steht es dort wirklich"), "{meldung}");
    }

    #[test]
    fn kontext_findet_auch_bei_abweichender_einrueckung() {
        let inhalt = "mod a {\n        let wert = berechne(x);\n}\n";
        let (zeile, block) = nearest_context(inhalt, "let wert = berechne(x);").unwrap();
        assert_eq!(zeile, 1);
        assert!(block.contains("let wert = berechne(x);"), "{block}");
    }

    fn test_root() -> PathBuf {
        let dir = std::env::temp_dir().join("webagent_file_actions_tests");
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn temp_file(name: &str, content: &str) -> PathBuf {
        let dir = test_root();
        let p = dir.join(format!("{}_{}", std::process::id(), name));
        let _ = fs::remove_file(&p);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn edit_replaces_unique_anchor() {
        let p = temp_file("unique.txt", "alpha\nbeta\ngamma\n");
        let msg = apply_edit_in(&test_root(), p.to_str().unwrap(), "beta", "BETA").unwrap();
        assert!(msg.contains("edit ok"), "{msg}");
        assert!(msg.contains("Zeile 2"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "alpha\nBETA\ngamma\n");
    }

    #[test]
    fn edit_rejects_missing_anchor() {
        let p = temp_file("missing.txt", "alpha\n");
        let err = apply_edit_in(&test_root(), p.to_str().unwrap(), "nicht-da", "x").unwrap_err();
        assert!(err.contains("nicht gefunden"), "{err}");
    }

    #[test]
    fn edit_rejects_ambiguous_anchor() {
        let p = temp_file("ambig.txt", "dup\ndup\n");
        let err = apply_edit_in(&test_root(), p.to_str().unwrap(), "dup", "x").unwrap_err();
        assert!(err.contains("mehrdeutig (2 Treffer)"), "{err}");
    }

    #[test]
    fn edit_tolerates_crlf_file_with_lf_anchor() {
        let p = temp_file("crlf.txt", "eins\r\nzwei\r\ndrei\r\n");
        let msg = apply_edit_in(
            &test_root(),
            p.to_str().unwrap(),
            "zwei\ndrei",
            "zwei\nDREI",
        )
        .unwrap();
        assert!(msg.contains("Zeilenenden-Toleranz"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "eins\r\nzwei\r\nDREI\r\n");
    }

    #[test]
    fn edit_missing_file_points_to_write() {
        let err = apply_edit_in(&test_root(), "Z:/gibts/nicht.txt", "a", "b").unwrap_err();
        assert!(err.contains("write-Action"), "{err}");
    }

    #[test]
    fn edit_empty_new_deletes_anchor() {
        let p = temp_file("delete.txt", "bleibt-WEG-bleibt");
        apply_edit_in(&test_root(), p.to_str().unwrap(), "-WEG-", "").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "bleibtbleibt");
    }

    #[test]
    fn edit_handles_umlauts() {
        // Dogfood-Fund (zai, Testfall 3): UTF-8-korrekte Suche mit Umlauten.
        let p = temp_file("umlaut.txt", "Grüße aus Köln\nTschüß\n");
        apply_edit_in(&test_root(), p.to_str().unwrap(), "Köln", "Düsseldorf").unwrap();
        assert_eq!(
            fs::read_to_string(&p).unwrap(),
            "Grüße aus Düsseldorf\nTschüß\n"
        );
    }

    #[test]
    fn edit_at_file_start_and_end() {
        // Dogfood-Fund (zai, Testfall 8): Anker in erster/letzter Zeile.
        let p = temp_file("bounds.txt", "erste\nmitte\nletzte");
        apply_edit_in(&test_root(), p.to_str().unwrap(), "erste", "ERSTE").unwrap();
        apply_edit_in(&test_root(), p.to_str().unwrap(), "letzte", "LETZTE").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "ERSTE\nmitte\nLETZTE");
    }

    #[test]
    fn write_creates_new_file_with_parents() {
        let dir = test_root();
        let p = dir.join(format!("{}_sub/neu.txt", std::process::id()));
        let _ = fs::remove_file(&p);
        let msg = apply_write_in(&test_root(), p.to_str().unwrap(), "inhalt\n").unwrap();
        assert!(msg.contains("write ok"), "{msg}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "inhalt\n");
    }

    #[test]
    fn write_refuses_existing_file() {
        let p = temp_file("existiert.txt", "alt");
        let err = apply_write_in(&test_root(), p.to_str().unwrap(), "neu").unwrap_err();
        assert!(err.contains("existiert bereits"), "{err}");
        assert_eq!(fs::read_to_string(&p).unwrap(), "alt");
    }

    #[test]
    fn file_actions_reject_parent_traversal() {
        let root = test_root().join(format!("{}_traversal_root", std::process::id()));
        fs::create_dir_all(root.join("sub")).unwrap();
        let existing = root.join("target.txt");
        fs::write(&existing, "alt").unwrap();

        let edit = apply_edit_in(&root, "sub/../target.txt", "alt", "neu").unwrap_err();
        assert!(edit.contains("Pfad-Traversal"), "{edit}");
        let write = apply_write_in(&root, "sub/../neu.txt", "neu").unwrap_err();
        assert!(write.contains("Pfad-Traversal"), "{write}");
        assert_eq!(fs::read_to_string(existing).unwrap(), "alt");
    }

    #[test]
    fn file_actions_reject_absolute_path_outside_root() {
        let base = test_root().join(format!("{}_absolute_escape", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside.txt");
        fs::create_dir_all(&root).unwrap();
        fs::write(&outside, "alt").unwrap();

        let edit = apply_edit_in(&root, outside.to_str().unwrap(), "alt", "neu").unwrap_err();
        assert!(edit.contains("ausserhalb"), "{edit}");
        let new_outside = base.join("new-outside.txt");
        let write = apply_write_in(&root, new_outside.to_str().unwrap(), "neu").unwrap_err();
        assert!(write.contains("ausserhalb"), "{write}");
        assert_eq!(fs::read_to_string(outside).unwrap(), "alt");
        assert!(!new_outside.exists());
    }

    #[test]
    fn file_actions_allow_absolute_path_inside_root() {
        let root = test_root().join(format!("{}_absolute_inside", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let existing = root.join("existing.txt");
        fs::write(&existing, "alt").unwrap();
        apply_edit_in(&root, existing.to_str().unwrap(), "alt", "neu").unwrap();

        let new_file = root.join("sub").join("new.txt");
        apply_write_in(&root, new_file.to_str().unwrap(), "inhalt").unwrap();
        assert_eq!(fs::read_to_string(existing).unwrap(), "neu");
        assert_eq!(fs::read_to_string(new_file).unwrap(), "inhalt");
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn file_actions_reject_symlink_escape() {
        let base = test_root().join(format!("{}_symlink_escape", std::process::id()));
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let outside_file = outside.join("existing.txt");
        fs::write(&outside_file, "alt").unwrap();
        let link = root.join("escape");

        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside, &link);
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_dir(&outside, &link);
        // Windows kann Symlink-Erstellung ohne Developer Mode/Privileg ablehnen.
        if linked.is_err() {
            return;
        }

        let escaped_existing = link.join("existing.txt");
        let edit =
            apply_edit_in(&root, escaped_existing.to_str().unwrap(), "alt", "neu").unwrap_err();
        assert!(edit.contains("ausserhalb"), "{edit}");

        let escaped_new = link.join("new.txt");
        let write = apply_write_in(&root, escaped_new.to_str().unwrap(), "neu").unwrap_err();
        assert!(write.contains("ausserhalb"), "{write}");
        assert_eq!(fs::read_to_string(outside_file).unwrap(), "alt");
        assert!(!outside.join("new.txt").exists());
    }

    #[test]
    fn worktree_context_is_bounded() {
        let ctx = worktree_context(10);
        // Im Repo-Root aufgerufen: nicht leer, nie mehr als max_entries+Kopf.
        if !ctx.is_empty() {
            assert!(ctx.lines().count() <= 13, "{ctx}");
        }
    }

    #[test]
    fn whitespace_tolerant_match_rescues_wrong_indentation() {
        // kimis reales Problem (2026-07-22): richtiger Code, falsche Einrueckung.
        let dir = std::env::temp_dir().join(format!("wsedit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t1.rs");
        std::fs::write(
            &f,
            "fn a() {
        let x = 1;
        return x;
}
",
        )
        .unwrap();
        let r = apply_edit_in(
            &dir,
            f.to_str().unwrap(),
            "    let x = 1;
    return x;",
            "    let x = 42;
    return x;",
        );
        assert!(r.is_ok(), "sollte per Whitespace-Toleranz greifen: {r:?}");
        assert!(r.unwrap().contains("Whitespace-Toleranz"));
        let after = std::fs::read_to_string(&f).unwrap();
        assert!(
            after.contains("let x = 42;"),
            "Ersetzung angewandt: {after:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn whitespace_tolerant_match_refuses_when_ambiguous() {
        let dir = std::env::temp_dir().join(format!("wsedit2_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t2.rs");
        std::fs::write(
            &f,
            "  let x = 1;
    let x = 1;
",
        )
        .unwrap();
        let r = apply_edit_in(&dir, f.to_str().unwrap(), "let x = 1;", "let x = 2;");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("mehrdeutig"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn whitespace_tolerant_match_still_fails_when_truly_absent() {
        let dir = std::env::temp_dir().join(format!("wsedit3_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("t3.rs");
        std::fs::write(
            &f,
            "fn a() {}
",
        )
        .unwrap();
        let r = apply_edit_in(&dir, f.to_str().unwrap(), "let y = nonexistent;", "x");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("nicht gefunden"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn span_finder_is_unit_checkable() {
        let content = "aaa
    ziel eins
    ziel zwei
bbb
";
        let (start, end) = whitespace_tolerant_span(
            content,
            "ziel eins
ziel zwei",
        )
        .unwrap();
        assert_eq!(
            &content[start..end],
            "    ziel eins
    ziel zwei"
        );
        assert_eq!(whitespace_tolerant_span(content, "gibts nicht"), Err(0));
    }
}
