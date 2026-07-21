//! runs_report — Fehlerursachen vergangener Läufe klassifizieren.
//!
//! Dreimal in Folge (2026-07-21) sah eine Kennzahl wie Unfähigkeit eines Brains
//! aus und war ein Fehler des Harness:
//!
//! - zai `brain_incomplete` → die Weboberfläche warf einen Fehler, das Brain
//!   hatte nie geantwortet.
//! - claude `protocol_error` → ein syntaktisch korrektes `WEBAGENT/1 EDIT` fiel
//!   durch die `\A`-Verankerung der Regex.
//! - kimi `max_cycles` → Antworten mit dem Sprachlabel `plain` wurden nicht
//!   vom UI-Vorspann befreit.
//!
//! Jedes Mal steckte der Beleg im Transkript, und jedes Mal habe ich ihn von
//! Hand gesucht. Dieses Modul macht daraus eine Abfrage: [`classify_run`]
//! trennt „Brain konnte nicht" von „Harness hat es kaputtgemacht", damit die
//! nächste Fehlklassifikation auffällt, bevor sie in den Score wandert.

use std::path::{Path, PathBuf};

/// Woran ein Lauf gescheitert ist — oder dass er es nicht ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureClass {
    /// Sauber durchgelaufen.
    Passed,
    /// Anbieter/Oberfläche hat blockiert — zählt NICHT gegen das Brain.
    ExternalBlock,
    /// Das Brain lieferte ein erkennbares Format, das der Parser trotzdem
    /// abgelehnt hat. Das ist ein Harness-Fehler, kein Brain-Fehler.
    HarnessParseBug,
    /// Das Brain hat das Protokoll wirklich verletzt.
    ProtocolViolation,
    /// Zyklenbudget aufgebraucht.
    CycleBudget,
    /// Wall-Timeout.
    Timeout,
    /// Alles andere.
    Other,
}

impl FailureClass {
    pub fn label(&self) -> &'static str {
        match self {
            FailureClass::Passed => "bestanden",
            FailureClass::ExternalBlock => "extern blockiert",
            FailureClass::HarnessParseBug => "HARNESS-BUG",
            FailureClass::ProtocolViolation => "Protokollverstoss",
            FailureClass::CycleBudget => "Zyklenbudget",
            FailureClass::Timeout => "Timeout",
            FailureClass::Other => "sonstiges",
        }
    }

    /// `true`, wenn der Fehlschlag dem Brain anzulasten ist.
    pub fn blames_brain(&self) -> bool {
        matches!(
            self,
            FailureClass::ProtocolViolation | FailureClass::CycleBudget | FailureClass::Timeout
        )
    }
}

/// Ein Brain-Text, wie er im Transkript steht.
#[derive(Debug, Clone)]
pub struct RunFacts {
    pub brain: String,
    pub status: String,
    pub cycles: u32,
    /// Alle Antworten des Brains in Reihenfolge.
    pub brain_texts: Vec<String>,
    /// Alle `protocol_invalid`-Meldungen des Controllers.
    pub protocol_errors: usize,
}

/// `true`, wenn der Text einen Rohformat-Marker enthält — also erkennbar dem
/// Protokoll folgen WOLLTE.
pub fn has_raw_marker(text: &str) -> bool {
    ["WEBAGENT/1 EDIT", "WEBAGENT/1 WRITE", "WEBAGENT/1 SHELL"]
        .iter()
        .any(|m| text.contains(m))
}

/// `true`, wenn der Text ein webagent/1-JSON-Objekt zu enthalten scheint.
pub fn looks_like_protocol_json(text: &str) -> bool {
    let low = text.to_lowercase();
    low.contains("\"protocol\"") && low.contains("webagent/1")
}

/// Ordnet einem Lauf seine Fehlerursache zu.
///
/// Der springende Punkt ist die Reihenfolge: eine externe Blockade schlägt
/// alles (dann hat das Brain nie gearbeitet), und ein erkennbares Format bei
/// gleichzeitigem `protocol_invalid` ist ein Harness-Fehler — nicht das Brain.
pub fn classify_run(facts: &RunFacts) -> FailureClass {
    let all = facts.brain_texts.join("\n");

    if facts.brain_texts.iter().any(|t| crate::brain::is_retryable_empty_response(t))
        || crate::benchmark::is_external_block(&facts.status)
    {
        return FailureClass::ExternalBlock;
    }

    // Das Brain lieferte ein erkennbares Format UND der Controller hat es
    // abgelehnt: dann liegt es am Parser.
    if facts.protocol_errors > 0 && (has_raw_marker(&all) || looks_like_protocol_json(&all)) {
        return FailureClass::HarnessParseBug;
    }

    match facts.status.as_str() {
        "done" => FailureClass::Passed,
        "max_cycles" => FailureClass::CycleBudget,
        "wall_timeout" => FailureClass::Timeout,
        "protocol_error" => FailureClass::ProtocolViolation,
        _ if facts.protocol_errors > 0 => FailureClass::ProtocolViolation,
        _ => FailureClass::Other,
    }
}

/// Liest die Fakten eines Run-Verzeichnisses (`meta.json` + `transcript.jsonl`).
pub fn read_run(dir: &Path) -> Option<RunFacts> {
    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).ok()?).ok()?;
    let mut brain_texts = Vec::new();
    let mut protocol_errors = 0usize;
    if let Ok(t) = std::fs::read_to_string(dir.join("transcript.jsonl")) {
        for line in t.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let role = v.get("role").and_then(|x| x.as_str()).unwrap_or("");
            let content = v.get("content").and_then(|x| x.as_str()).unwrap_or("");
            match role {
                "brain" => brain_texts.push(content.to_string()),
                "system" if content.starts_with("protocol_invalid") => protocol_errors += 1,
                _ => {}
            }
        }
    }
    Some(RunFacts {
        brain: meta.get("brain_id").and_then(|x| x.as_str()).unwrap_or("?").to_string(),
        status: meta.get("status").and_then(|x| x.as_str()).unwrap_or("?").to_string(),
        cycles: meta.get("cycles").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
        brain_texts,
        protocol_errors,
    })
}

/// Sammelt die jüngsten `limit` Läufe, neueste zuerst.
pub fn recent_runs(runs_dir: &Path, limit: usize) -> Vec<(PathBuf, RunFacts)> {
    let Ok(entries) = std::fs::read_dir(runs_dir) else {
        return Vec::new();
    };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let m = e.metadata().ok()?.modified().ok()?;
            Some((m, e.path()))
        })
        .collect();
    dirs.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    dirs.into_iter()
        .take(limit)
        .filter_map(|(_, p)| read_run(&p).map(|f| (p, f)))
        .collect()
}

/// Formatiert den Bericht: eine Zeile je Lauf, danach die Summe je Ursache.
pub fn format_report(runs: &[(PathBuf, RunFacts)]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<10} {:<18} {:>6} {:>7}  {}\n",
        "brain", "status", "cycles", "p_err", "Ursache"
    ));
    let mut counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut harness = 0usize;
    for (_, f) in runs {
        let c = classify_run(f);
        *counts.entry(c.label()).or_insert(0) += 1;
        if c == FailureClass::HarnessParseBug {
            harness += 1;
        }
        out.push_str(&format!(
            "{:<10} {:<18} {:>6} {:>7}  {}\n",
            f.brain,
            crate::char_prefix(&f.status, 18),
            f.cycles,
            f.protocol_errors,
            c.label()
        ));
    }
    out.push_str("\nSumme je Ursache:\n");
    for (label, n) in &counts {
        out.push_str(&format!("  {label:<20} {n}\n"));
    }
    if harness > 0 {
        out.push_str(&format!(
            "\n{harness} Lauf/Laeufe scheiterten trotz erkennbarem Format — das ist der \
             Harness, nicht das Brain. Diese Faelle gehoeren NICHT in den Score.\n"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(status: &str, texts: &[&str], p_err: usize) -> RunFacts {
        RunFacts {
            brain: "test".to_string(),
            status: status.to_string(),
            cycles: 3,
            brain_texts: texts.iter().map(|s| s.to_string()).collect(),
            protocol_errors: p_err,
        }
    }

    #[test]
    fn claudes_rejected_edit_is_classified_as_harness_bug() {
        // Realfall: korrektes EDIT mit Denk-Vorspann, vom Parser abgelehnt.
        // Stand als "protocol_error" in der Statistik und lief dort als
        // Unfaehigkeit des Brains.
        let f = facts(
            "protocol_error",
            &["Architected adaptive brain weight function\nWEBAGENT/1 EDIT\nid: e1\npath: src/x.rs"],
            1,
        );
        assert_eq!(classify_run(&f), FailureClass::HarnessParseBug);
        assert!(!classify_run(&f).blames_brain());
    }

    #[test]
    fn zais_ui_failure_is_an_external_block() {
        let f = facts(
            "brain_incomplete",
            &["No response, Please try again later.\nSyntaxError: Unexpected token"],
            1,
        );
        assert_eq!(classify_run(&f), FailureClass::ExternalBlock);
        assert!(!classify_run(&f).blames_brain());
    }

    #[test]
    fn real_protocol_violations_still_blame_the_brain() {
        // Prosa ohne jedes erkennbare Format: hier hat das Brain wirklich
        // danebengelegen. Wuerde das als Harness-Bug durchgehen, verschwaende
        // die Klassifikation ihren Zweck.
        let f = facts("protocol_error", &["Ich wuerde vorschlagen, die Datei anzupassen."], 2);
        assert_eq!(classify_run(&f), FailureClass::ProtocolViolation);
        assert!(classify_run(&f).blames_brain());
    }

    #[test]
    fn max_cycles_without_parse_errors_is_a_budget_problem() {
        let f = facts("max_cycles", &["{\"protocol\":\"webagent/1\",\"actions\":[]}"], 0);
        assert_eq!(classify_run(&f), FailureClass::CycleBudget);
        assert!(classify_run(&f).blames_brain());
    }

    #[test]
    fn a_clean_run_passes() {
        let f = facts("done", &["{\"protocol\":\"webagent/1\",\"actions\":[]}"], 0);
        assert_eq!(classify_run(&f), FailureClass::Passed);
    }

    #[test]
    fn external_block_beats_everything_else() {
        // Auch mit Parse-Fehlern: wer nie geantwortet hat, hat nichts falsch
        // gemacht.
        let f = facts("protocol_error", &["No response, Please try again later."], 3);
        assert_eq!(classify_run(&f), FailureClass::ExternalBlock);
    }

    #[test]
    fn json_protocol_is_recognised_as_an_attempt() {
        assert!(looks_like_protocol_json(r#"{"protocol": "webagent/1", "actions": []}"#));
        assert!(!looks_like_protocol_json("nur prosa"));
        assert!(has_raw_marker("bla\nWEBAGENT/1 WRITE\nid: x"));
        assert!(!has_raw_marker("bla blubb"));
    }
}
