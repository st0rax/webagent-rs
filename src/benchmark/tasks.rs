//! Aufgaben-Bau (Phase A -> konkreter Bauauftrag): Siegerauswahl, Task-IDs,
//! Refine-Prompts, Brain-Zuweisung, Datei-Gliederung und Reparaturfokus.
//!
//! Reine Helfer ohne Netzwerk und ohne Brains -- der gesamte Block ist
//! unit-getestet.

use regex::Regex;
use std::hash::{Hash, Hasher};
use std::path::Path;

use crate::self_research::SelfResearchReport;

/// Stabile, kurze Task-Kennung aus dem Sieger-Text (Hash) — gleiche Aufgabe ⇒
/// gleiche `task_id` über Brains und Läufe hinweg.
pub fn task_id(winner: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    winner.trim().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Baut den Aufgaben-Prompt aus dem gevoteten Sieger (Spec §2, Phase A).
pub fn build_task_prompt(winner: &str) -> String {
    build_task_prompt_in(winner, &crate::config::root_dir())
}

/// Wie [`build_task_prompt`], aber mit explizitem Projektwurzel-Pfad, damit die
/// Gliederung der Zieldatei ohne Prozess-CWD testbar ist.
///
/// Nennt der Plan eine Zieldatei, haengt ihre Gliederung an. Ohne sie liest sich
/// ein Brain scheibchenweise durch die Datei und verbraucht dabei sein
/// Zyklenbudget — siehe [`file_outline`].
pub fn build_task_prompt_in(winner: &str, root: &Path) -> String {
    let basis = format!(
        "Implementiere folgenden Verbesserungsvorschlag im Rust-Projekt webagent-rs \
         (aktuelles Verzeichnis) mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Ergänze \
         Tests. `cargo test --lib` muss grün bleiben. Mache genau diese eine \
         Änderung, nichts darüber hinaus. Ändere weder Cargo.toml noch Cargo.lock, \
         füge keine Dependencies hinzu und bearbeite keine Build-/CI-Skripte.\n\nVorschlag: {winner}",
        winner = winner.trim()
    );
    match target_file_of(winner).and_then(|rel| file_outline(&root.join(&rel), 120)) {
        Some(gliederung) => format!("{basis}\n\n{gliederung}"),
        None => basis,
    }
}

/// Zieht die vom Plan genannte Zieldatei heraus (`Zieldatei: src/foo.rs`).
pub fn target_file_of(text: &str) -> Option<String> {
    let re = Regex::new(r"(?i)zieldatei:\s*([A-Za-z0-9_./-]+\.rs)").ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Zählt die bestandenen Tests aus einer `cargo test`-Ausgabe
/// (`test result: ok. 387 passed; 0 failed; …`).
///
/// Anti-Schummel-Signal: ein Brain kann sonst eine VERWAISTE Datei anlegen
/// (nicht im Modulbaum) — dann baut und testet alles grün, obwohl nichts
/// integriert wurde. Real beobachtet 2026-07-21: claude und zai "bestanden"
/// mit einer Datei unter dem erfundenen Pfad src/executor/…
pub fn parse_test_count(output: &str) -> Option<u32> {
    let mut total: Option<u32> = None;
    for part in output.split_whitespace().collect::<Vec<_>>().windows(2) {
        if part[1].starts_with("passed") {
            if let Ok(n) = part[0].parse::<u32>() {
                total = Some(total.unwrap_or(0) + n);
            }
        }
    }
    total
}

/// Prompt, der einen VAGEN Abstimmungssieger in eine KONKRETE, bounded
/// Coding-Aufgabe übersetzt (Phase A.5).
///
/// Ohne diesen Schritt bekamen alle Brains den rohen Architekturwunsch und
/// explorierten ergebnislos (22/22 `did_change=false`, 2026-07-21). `files`
/// erzwingt zusätzlich eine EXISTIERENDE Zieldatei — eine erfundene wie
/// `src/executor/powershell.rs` führte zu verwaisten Dateien und damit zu
/// falschen PASS-Wertungen.
pub fn build_refine_prompt(winner: &str, facts: &str, files: &[String]) -> String {
    format!(
        "Du planst eine Coding-Aufgabe fuer das Rust-Projekt webagent-rs.\n\n\
         Projektfakten:\n{facts}\n\n\
         Zu konkretisierender Verbesserungsvorschlag:\n{winner}\n\n\
         Uebersetze ihn in EINE kleine, in sich geschlossene Aufgabe, die ein \
         Agent in wenigen Schritten umsetzen kann. Anforderungen an deine Antwort:\n\
         - genau EINE Zieldatei benennen, und sie MUSS aus dieser Liste stammen \
           (erfinde KEINE Pfade, lege KEINE neuen Dateien/Module an):\n           {files}\n\
         - genau EINE neue oeffentliche Funktion mit EXAKTER Rust-Signatur angeben\n\
         - das erwartete Verhalten in 2-4 Saetzen praezise beschreiben\n\
         - mindestens 4 konkrete Testfaelle auflisten\n\
         - nur std und bereits vorhandene Dependencies verwenden\n\
         - KEINE Architektur-Umbauten, keine neuen Module, kein Refactoring\n\n\
         Antworte AUSSCHLIESSLICH mit der Aufgabenbeschreibung als Fliesstext \
         (kein JSON, keine Einleitung, kein Nachwort).",
        facts = crate::char_prefix(facts, 900),
        winner = winner.trim(),
        files = if files.is_empty() {
            "src/protocol.rs, src/shell_policy.rs, src/file_actions.rs".to_string()
        } else {
            files.join(", ")
        }
    )
}

/// Nimmt die verfeinerte Aufgabe, wenn sie brauchbar aussieht, sonst `None`.
/// Zu kurze oder leere Antworten fallen auf den Rohsieger zurück.
pub fn usable_refinement(text: &str) -> Option<String> {
    let t = text.trim();
    if t.chars().count() < 80 {
        return None;
    }
    Some(t.to_string())
}

/// Extrahiert den vorgeschlagenen Funktionsnamen aus einer verfeinerten Aufgabe
/// (erstes `pub fn NAME` bzw. `fn NAME`), um Neuheit prüfen zu können.
pub fn proposed_fn_name(refined: &str) -> Option<String> {
    for marker in ["pub fn ", "fn "] {
        if let Some(idx) = refined.find(marker) {
            let rest = &refined[idx + marker.len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.len() >= 3 {
                return Some(name);
            }
        }
    }
    None
}

/// `true`, wenn die Aufgabe etwas verlangt, das es SCHON GIBT — dann ist die
/// Runde wertlos: das Brain meldet korrekt "ist bereits implementiert", ändert
/// nichts und würde faelschlich als Fehlschlag gewertet (Storax-Beobachtung
/// 2026-07-21: "einer der Kandidaten sagt immer wieder, alles sei schon sauber
/// implementiert").
pub fn task_is_redundant(refined: &str, existing_api: &[String]) -> bool {
    match proposed_fn_name(refined) {
        Some(name) => existing_api.iter().any(|e| e == &name),
        None => false,
    }
}

/// `true`, wenn die Aufgabe eine `Zieldatei:` nennt, die NICHT in der erlaubten
/// Modulliste steht. Einen solchen Plan auszufuehren liesse das Brain auf einem
/// nicht existierenden Pfad suchen, endlos drehen und nichts aendern
/// (real beobachtet 2026-08-01: isolated_query liegt in src/repl/mod.rs, die
/// alte flache Modulliste kannte den Pfad gar nicht). Leere Liste = nichts
/// pruefbar → nicht verwerfen.
pub fn task_targets_missing_file(refined: &str, src_files: &[String]) -> bool {
    if src_files.is_empty() {
        return false;
    }
    let Some(target) = target_file_of(refined) else {
        return false;
    };
    let rel = target.strip_prefix("src/").unwrap_or(&target);
    !src_files
        .iter()
        .any(|f| f.strip_prefix("src/") == Some(rel))
}

/// Platz-1-Vorschlag eines Self-Research-Reports (die Benchmark-Aufgabe), oder
/// `None`, wenn niemand abgestimmt hat.
pub fn winner_from_report(report: &SelfResearchReport) -> Option<String> {
    report
        .ranked
        .first()
        .map(|r| r.text.trim().to_string())
        .filter(|t| !t.is_empty())
}

/// Alle gevoteten Vorschläge in Rangfolge (Platz 1 zuerst), leere verworfen.
pub fn ranked_from_report(report: &SelfResearchReport) -> Vec<String> {
    report
        .ranked
        .iter()
        .map(|r| r.text.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Verteilt die gevoteten Vorschläge auf die Brains — Fertigungsstraße statt
/// Turnier: jedes Brain bekommt eine EIGENE Aufgabe, deshalb kann die Arbeit
/// aller bestandenen Brains geerntet werden statt nur die des besten.
///
/// Bauen alle dasselbe, kollidieren die Patches im selben Code und sieben von
/// acht Beiträgen sind zwangsläufig Ausschuss — die Messung war brauchbar, die
/// Produktion nicht.
/// Rotiert die Zuteilung: Brain `(i + round) % k` baut Rang `i % k`. Ueber
/// mehrere Runden sieht damit jedes Brain jeden Rang, sodass keines dauerhaft
/// die leichteren oder schwereren Aufgaben zieht und der Score fair bleibt.
/// Gleichzeitig wandert der ERSTE Bauplatz (der erste Implementierer eines
/// Turniers) pro Runde zum naechsten Brain — sonst bearbeitet bei nur einem
/// Sieger-Rang immer dasselbe erste Brain den Sieger zuerst. Beobachtet
/// 2026-08-02: chatgpt bekam als erstes Brain der Liste jede Runde den
/// Implementierer-Job, obwohl claude den Sieger nie sah.
/// Gibt es weniger Vorschläge als Brains, teilen sich mehrere Brains einen Rang
/// (dann gewinnt beim Ernten der beste — wie im Turnier).
pub fn assign_tasks(brains: &[String], ranked: &[String], round: usize) -> Vec<(String, String)> {
    if ranked.is_empty() {
        return Vec::new();
    }
    let n = brains.len();
    (0..n)
        .map(|i| {
            let brain = &brains[(i + round) % n];
            let idx = i % ranked.len();
            (brain.clone(), ranked[idx].clone())
        })
        .collect()
}


/// Kompakte Gliederung einer Rust-Datei: Signaturen mit Zeilennummern.
///
/// Warum das in den Aufgabentext gehoert, statt die Brains die Datei lesen zu
/// lassen: `src/protocol.rs` hat 62.276 Zeichen und 1567 Zeilen, aber nur 34
/// Signaturen. Wer dort eine Funktion ergaenzen soll, braucht die Fundstelle
/// und genug Umgebung fuer einen eindeutigen Anker — nicht die ganze Datei.
///
/// Ohne Gliederung liest sich ein Brain scheibchenweise durch: zai verbrauchte
/// am 30.07.2026 alle 15 Zyklen einer Runde mit `Get-Content` und
/// `Select-String` auf derselben Datei und kam nie zum Editieren; neun Laeufe
/// derselben Runde erzeugten NULL Datei-Aktionen.
///
/// Rund 2.000 Zeichen statt 62.000 — und danach kann gezielt gelesen werden.
pub fn file_outline(path: &Path, max_entries: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut zeilen = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let t = line.trim_start();
        let ist_signatur = t.starts_with("pub fn ")
            || t.starts_with("fn ")
            || t.starts_with("pub struct ")
            || t.starts_with("struct ")
            || t.starts_with("pub enum ")
            || t.starts_with("enum ")
            || t.starts_with("pub trait ")
            || t.starts_with("impl ")
            || t.starts_with("pub const ")
            || t.starts_with("const ")
            || t.starts_with("#[cfg(test)]");
        if !ist_signatur {
            continue;
        }
        // Nur der Kopf, nicht der Rumpf: alles ab `{` faellt weg.
        let kopf = t.split('{').next().unwrap_or(t).trim_end();
        zeilen.push(format!("{:>5}  {}", i + 1, crate::char_prefix(kopf, 110)));
        if zeilen.len() >= max_entries {
            zeilen.push("      … (gekuerzt)".to_string());
            break;
        }
    }
    if zeilen.is_empty() {
        return None;
    }
    Some(format!(
        "GLIEDERUNG von {} ({} Zeilen). Lies gezielt mit \
         `Get-Content <datei> | Select-Object -Skip N -First M` statt die ganze Datei:\n{}",
        path.display(),
        text.lines().count(),
        zeilen.join("\n")
    ))
}


/// Verdichtet echte, interne Gate-Befunde zu einem begrenzten Reparaturfokus
/// für die Folgerunde. Provider-/Login-Blockaden gelangen nicht hierher.
pub fn repair_focus_from_failures(failures: &[String]) -> Option<String> {
    let mut unique = Vec::<String>::new();
    for failure in failures {
        let compact = crate::char_prefix(failure.trim(), 700).to_string();
        if !compact.is_empty() && !unique.iter().any(|seen| seen == &compact) {
            unique.push(compact);
        }
        if unique.len() == 3 {
            break;
        }
    }
    (!unique.is_empty()).then(|| {
        format!(
            "REPARATURPRIORITÄT aus der vorherigen Benchmark-Runde:\n{}\n\n\
             Wähle und plane ausschließlich eine kleine Änderung, die einen dieser \n\
             reproduzierten Gate-Befunde behebt. Keine neue Nebenfunktion.",
            unique
                .iter()
                .enumerate()
                .map(|(i, item)| format!("{}. {item}", i + 1))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })
}

