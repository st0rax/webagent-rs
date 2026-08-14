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
    build_task_prompt_with_context_budget(winner, root, 0, None)
}

/// Brain-spezifischer Bauauftrag. Nutzt die gemessene Eingabekapazitaet, um
/// die Zieldatei moeglichst vollstaendig statt scheibchenweise mitzugeben.
pub fn build_task_prompt_for_brain_in(
    winner: &str,
    root: &Path,
    brain_id: &str,
    handoff_context: Option<&str>,
) -> String {
    let accepted = crate::brain_limits::accepted_chars(brain_id).unwrap_or(40_000);
    // Systemprompt, Aufgabe, Baum und spaetere Observations brauchen ebenfalls
    // Platz. Die Haelfte der belegten Kapazitaet bleibt die konservative Grenze.
    // Auch ein noch nicht vermessenes Brain bekommt kleine Zieldateien komplett.
    // Die alte Formel ergab beim 40k-Fallback exakt 0 und degradierte abrupt auf
    // einen Ausschnitt, obwohl der Prompt bequem noch Platz hatte.
    let context_budget = (accepted / 2).saturating_sub(12_000).max(8_000);
    build_task_prompt_with_context_budget(winner, root, context_budget, handoff_context)
}

fn build_task_prompt_with_context_budget(
    winner: &str,
    root: &Path,
    context_budget: usize,
    handoff_context: Option<&str>,
) -> String {
    let workspace = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let mut basis = format!(
        "Implementiere folgenden Verbesserungsvorschlag im Rust-Projekt webagent-rs \
         im Workspace `{workspace}` mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). \
         Lies, aendere und teste AUSSCHLIESSLICH in diesem Workspace; verwende \
         keinen anderen Checkout und keinen hartcodierten frueheren Repo-Pfad. Ergänze \
         Tests. `cargo test --lib` muss grün bleiben. Mache genau diese eine \
         Änderung, nichts darüber hinaus. Ändere weder Cargo.toml noch Cargo.lock, \
         füge keine Dependencies hinzu und bearbeite keine Build-/CI-Skripte.\n\nVorschlag: {winner}",
        winner = winner.trim(),
        workspace = workspace,
    );
    if let Some(ctx) = handoff_context.map(str::trim).filter(|ctx| !ctx.is_empty()) {
        basis = format!("{basis}\n\nVorarbeit vom vorherigen Brain:\n{ctx}");
    }
    // Symbol-Pruefung: nennt der Vorschlag Bezeichner, die in der Zieldatei
    // gar nicht vorkommen, sondern woanders?
    //
    // Beobachtet 12.08.2026: Sieger war „src/benchmark/mod.rs: Fehlerbehandlung
    // bei `bench_collapse_all`" — die Funktion steht in src/tui_state.rs, der
    // ebenfalls genannte Typ `BenchmarkUi` existiert nirgends. deepseek suchte
    // acht Minuten in einer 1046-Zeilen-Datei nach einem Symbol, das dort nie
    // war, und endete in `max_cycles` ohne eine einzige Aenderung.
    //
    // `task_targets_missing_file` faengt nur erfundene PFADE; ein existierender
    // Pfad mit falschem Symbol lief bisher durch. Der Hinweis verwirft nichts —
    // ein Vorschlag darf eine neue Funktion fordern —, er nennt nur die Datei,
    // in der das Symbol wirklich steht.
    let hinweis = crate::target_check::pruefe(
        target_file_of(winner).unwrap_or_default().as_str(),
        winner,
        &crate::target_check::quelldateien(root),
    )
    .hinweis();
    let basis = if hinweis.is_empty() {
        basis
    } else {
        format!("{basis}\n\n{hinweis}")
    };

    let Some(rel) = target_file_of(winner) else {
        return basis;
    };
    let path = root.join(&rel);
    let mut teile = Vec::new();
    if let Some(gliederung) = file_outline(&path, 120) {
        teile.push(gliederung);
    }
    if let Some(kontext) = full_target_context(&path, context_budget)
        .or_else(|| relevant_target_context(&path, winner, 35))
    {
        teile.push(kontext);
    }
    if teile.is_empty() {
        basis
    } else {
        format!("{basis}\n\n{}", teile.join("\n\n"))
    }
}

fn full_target_context(path: &Path, max_chars: usize) -> Option<String> {
    if max_chars == 0 {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    if text.chars().count() > max_chars {
        return None;
    }
    Some(format!(
        "VOLLSTAENDIGE ZIELDATEI {} ({} Zeilen; bereits komplett gelesen, nicht erneut per Shell laden):\n```rust\n{}\n```",
        path.display(),
        text.lines().count(),
        text
    ))
}
/// Zieht die vom Plan genannte Zieldatei heraus (`Zieldatei: src/foo.rs`).
pub fn target_file_of(text: &str) -> Option<String> {
    let re = Regex::new(r"(?i)zieldatei:\s*([A-Za-z0-9_./-]+\.rs)").ok()?;
    if let Some(target) = re
        .captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        return Some(target);
    }
    // Konsensplaene formulieren oft natuerlich „implementiere ... in
    // src/brain.rs" statt das starre Label zu wiederholen. Der erste konkrete
    // src/-Rustpfad ist dann die Zieldatei; spaetere Aufruferlisten folgen erst
    // in Risiken/Tests.
    let fallback = Regex::new(r"(?i)\b(src/[A-Za-z0-9_./-]+\.rs)\b").ok()?;
    fallback
        .captures(text)
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
         - eine EXAKTE Rust-Signatur oder einen bestehenden, konkret zu aendernden Anker angeben; \
           eine neue oeffentliche API nur wenn sie wirklich erforderlich ist\n\
         - das erwartete Verhalten in 2-4 Saetzen praezise beschreiben\n\
         - 1-4 konkrete automatisierbare Testfaelle und ein ausfuehrbares Testkommando nennen\n\
         - unter `Lokale Belege:` mindestens zwei EXISTIERENDE Symbole/Funktionen aus den \
           Projektfakten nennen; keine unbelegte Registry, API oder Infrastruktur voraussetzen\n\
         - unter `Abschlussbeleg:` die erwartete Datei-Aenderung und das pruefende Kommando nennen\n\
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

/// Ein autonom uebernehmbares Arbeitspaket braucht lokale Anker und einen
/// maschinell pruefbaren Abschluss. Freie Wunschtexte werden nicht in einen
/// teuren Browser-Baulauf durchgereicht.
pub fn refinement_has_evidence(text: &str) -> bool {
    let lower = text.to_lowercase();
    target_file_of(text).is_some()
        && lower.contains("lokale belege:")
        && lower.contains("abschlussbeleg:")
        && (lower.contains("cargo test")
            || lower.contains("cargo check")
            || lower.contains("cargo clippy"))
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
    // Refiner schreiben trotz verlangter exakter Signatur gelegentlich
    // "Funktion foo_bar(...)" statt "pub fn foo_bar(...)". Der Name ist
    // trotzdem eindeutig genug fuer Redundanz- und Scope-Pruefung.
    let re = Regex::new(r"(?i)\bfunktion\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(").ok()?;
    if let Some(name) = re
        .captures(refined)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
    {
        return Some(name);
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

/// Eine existierende Zieldatei reicht nicht: nennt der Plan ein bereits
/// vorhandenes Symbol, das nachweislich in einer anderen Datei steht, wuerde
/// das Brain am falschen Ort arbeiten.
pub fn task_is_misdirected(refined: &str, root: &Path) -> bool {
    let target = target_file_of(refined).unwrap_or_default();
    crate::target_check::pruefe(&target, refined, &crate::target_check::quelldateien(root))
        .irrefuehrend()
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

/// Liefert den wahrscheinlich relevanten Quelltext bereits im Bauauftrag mit.
/// Vorhandene, im Vorschlag genannte Symbole schlagen allgemeine Einfuegepunkte;
/// fuer eine rein neue API dient der Beginn des Testmoduls als stabiler Anker.
pub fn relevant_target_context(path: &Path, task: &str, radius: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let identifiers: Vec<&str> = task
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| word.len() >= 3)
        .collect();
    let signature = |line: &&str| {
        let t = line.trim_start();
        t.starts_with("pub fn ")
            || t.starts_with("fn ")
            || t.starts_with("pub struct ")
            || t.starts_with("pub enum ")
            || t.starts_with("pub trait ")
            || t.starts_with("impl ")
    };
    let center = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| signature(line))
        .filter_map(|(idx, line)| {
            let score = identifiers
                .iter()
                .filter(|word| line.contains(**word))
                .count();
            (score > 0).then_some((score, idx))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, idx)| idx)
        .or_else(|| lines.iter().position(|line| line.trim() == "#[cfg(test)]"))
        .unwrap_or(0);
    let start = center.saturating_sub(radius);
    let end = (center + radius + 1).min(lines.len());
    Some(format!(
        "RELEVANTER ZIELKONTEXT aus {} (Zeilen {}-{}):\n```rust\n{}\n```\n\
         Nutze diesen Kontext als Ausgangspunkt. Lies bei Bedarf weitere relevante Stellen; \
         vermeide lediglich wiederholtes Lesen bereits gelieferter Bereiche.",
        path.display(),
        start + 1,
        end,
        lines[start..end].join("\n")
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

#[cfg(test)]
mod tests_symbol_hinweis {
    use super::*;

    fn welt(name: &str) -> std::path::PathBuf {
        let w = std::env::temp_dir().join(format!("wa_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&w);
        std::fs::create_dir_all(w.join("src").join("benchmark")).expect("anlegbar");
        std::fs::write(w.join("src/benchmark/mod.rs"), "pub fn run() {}\n").expect("schreibbar");
        std::fs::write(
            w.join("src/tui_state.rs"),
            "pub fn bench_collapse_all() {}\n",
        )
        .expect("schreibbar");
        w
    }

    /// Der Hinweis muss im PROMPT landen, nicht nur berechnet werden.
    #[test]
    fn prompt_nennt_die_datei_in_der_das_symbol_wirklich_steht() {
        let wurzel = welt("hinweis");
        let winner = "Zieldatei: src/benchmark/mod.rs. Fehlerbehandlung bei \
                      `bench_collapse_all` fuer leere Panel-Liste ergaenzen.";
        let prompt = build_task_prompt_in(winner, &wurzel);
        assert!(
            prompt.contains("src/tui_state.rs"),
            "richtige Datei fehlt:\n{prompt}"
        );
        assert!(prompt.contains("HINWEIS ZUR AUFGABE"));
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn prompt_liefert_relevanten_code_statt_nur_leseanweisung() {
        let wurzel = welt("zielkontext");
        std::fs::write(
            wurzel.join("src/tui_state.rs"),
            "fn weit_weg() {}\n\nfn bench_collapse_all() {\n    collapse();\n}\n\n#[cfg(test)]\nmod tests {}\n",
        )
        .unwrap();
        let winner = "Zieldatei: src/tui_state.rs. Erweitere `bench_collapse_all`.";

        let prompt = build_task_prompt_in(winner, &wurzel);

        assert!(prompt.contains("RELEVANTER ZIELKONTEXT"), "{prompt}");
        assert!(prompt.contains("fn bench_collapse_all()"), "{prompt}");
        assert!(prompt.contains("Lies bei Bedarf"), "{prompt}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn neuer_funktionsname_nutzt_testmodul_als_einfuegeanker() {
        let wurzel = welt("neue_api");
        std::fs::write(
            wurzel.join("src/tui_state.rs"),
            "fn vorhanden() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn alt() {}\n}\n",
        )
        .unwrap();
        let winner = "Zieldatei: src/tui_state.rs. Neue Funktion `ganz_neu` ergaenzen.";

        let prompt = build_task_prompt_in(winner, &wurzel);

        assert!(prompt.contains("#[cfg(test)]"), "{prompt}");
        assert!(prompt.contains("fn alt()"), "{prompt}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn vorhandenes_symbol_in_anderer_datei_verwirft_plan() {
        let wurzel = welt("fehlgeleitet");
        let plan = "Zieldatei: src/benchmark/mod.rs. Aendere `bench_collapse_all`.";
        assert!(task_is_misdirected(plan, &wurzel));
        assert!(!task_is_misdirected(
            "Zieldatei: src/tui_state.rs. Aendere `bench_collapse_all`.",
            &wurzel
        ));
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn brain_budget_liefert_kleine_zieldatei_vollstaendig() {
        let wurzel = welt("vollstaendig");
        let plan = "Zieldatei: src/tui_state.rs. Neue Funktion `ganz_neu` ergaenzen.";
        let prompt = build_task_prompt_with_context_budget(plan, &wurzel, 10_000, None);
        assert!(prompt.contains("VOLLSTAENDIGE ZIELDATEI"), "{prompt}");
        assert!(prompt.contains("pub fn bench_collapse_all()"), "{prompt}");
        assert!(prompt.contains("nicht erneut per Shell laden"), "{prompt}");
    }

    #[test]
    fn natuerliche_in_datei_formulierung_erkennt_zieldatei() {
        assert_eq!(
            target_file_of("Implementiere pub fn reset() in src/brain.rs. Aufrufer spaeter in src/repl/mod.rs."),
            Some("src/brain.rs".to_string())
        );
    }

    /// Eine saubere Aufgabe bekommt keinen Hinweis — sonst rauscht der Prompt
    /// zu und das Brain misstraut jeder Angabe.
    #[test]
    fn saubere_aufgabe_bleibt_ohne_hinweis() {
        let wurzel = welt("sauber");
        let winner = "Zieldatei: src/tui_state.rs. `bench_collapse_all` um einen \
                      Frueh-Ausstieg ergaenzen.";
        let prompt = build_task_prompt_in(winner, &wurzel);
        assert!(
            !prompt.contains("HINWEIS ZUR AUFGABE"),
            "kein Hinweis noetig:\n{prompt}"
        );
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn prompt_orientiert_auch_ohne_explizite_zieldatei() {
        let wurzel = welt("ohne_ziel");
        let winner = "Fehlerbehandlung bei `bench_collapse_all` fuer leere Panel-Liste ergaenzen.";
        let prompt = build_task_prompt_in(winner, &wurzel);
        assert!(prompt.contains("HINWEIS ZUR AUFGABE"), "{prompt}");
        assert!(prompt.contains("src/tui_state.rs"), "{prompt}");
        assert!(prompt.contains("keine Zieldatei genannt"), "{prompt}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }
}
