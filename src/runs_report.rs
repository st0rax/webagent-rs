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
//!
//! Der Status `never_started` (Reconcile-Fund: angelegt, aber nie ausgefuehrt,
//! cycles==0) ist kein Fehlschlag: er taucht als eigenes Label „nie gestartet"
//! auf und laedt dem Brain nichts auf. Siehe `run_store::stale_status`.

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
    /// Das Brain meldete fertig, obwohl JEDER Edit-Versuch fehlschlug — die
    /// Erfolgsmeldung ist nicht durch eine Datei-Aenderung gedeckt.
    FalseDone,
    /// Lauf wurde angelegt, aber nie ausgefuehrt (cycles==0, per Reconcile als
    /// verwaist markiert). Kein Fehlschlag und kein Abbruch — ein Nichtereignis.
    NeverStarted,
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
            FailureClass::FalseDone => "Falschmeldung (kein Edit)",
            FailureClass::CycleBudget => "Zyklenbudget",
            FailureClass::Timeout => "Timeout",
            FailureClass::NeverStarted => "nie gestartet",
            FailureClass::Other => "sonstiges",
        }
    }

    /// `true`, wenn der Fehlschlag dem Brain anzulasten ist.
    pub fn blames_brain(&self) -> bool {
        matches!(
            self,
            FailureClass::ProtocolViolation
                | FailureClass::CycleBudget
                | FailureClass::Timeout
                // Erfolg behaupten, ohne editiert zu haben, ist Brain-Verhalten
                // — der Harness hat die Fehler sauber zurueckgemeldet.
                | FailureClass::FalseDone
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

/// `true`, wenn der Text ein webagent/1-JSON-Objekt zu enthalten scheint —
/// also erkennbar dem Protokoll folgen WOLLTE.
///
/// Die Erwähnung allein genügt nicht. Vorher reichte `"protocol"` irgendwo im
/// Text; damit galt am 30.07.2026 claudes Weigerung als Harness-Fehler, denn
/// sie erklärt woertlich:
///
/// > Wenn ich hier {"protocol": "webagent/1", ...} ausgebe, passiert nichts
/// > weiter, als dass du reinen Text siehst
///
/// Das ist ein Satz ueber das Format, kein Versuch, es zu benutzen — und die
/// Rundenmeldung schob den Fehlschlag faelschlich dem Harness zu.
///
/// Deshalb muss das Objekt eine Zeile BEGINNEN. Ein Brain, das antwortet,
/// stellt sein JSON an den Zeilenanfang (ggf. nach Codefence oder Label); wer
/// darueber redet, tut das mitten im Satz.
pub fn looks_like_protocol_json(text: &str) -> bool {
    text.lines().any(|line| {
        let t = line.trim_start().trim_start_matches("```json").trim_start();
        if !t.starts_with('{') {
            return false;
        }
        let low = t.to_lowercase();
        low.contains("\"protocol\"") && low.contains("webagent/1")
    })
}

/// Ordnet einem Lauf seine Fehlerursache zu.
///
/// Der springende Punkt ist die Reihenfolge: eine externe Blockade schlägt
/// alles (dann hat das Brain nie gearbeitet), und ein erkennbares Format bei
/// gleichzeitigem `protocol_invalid` ist ein Harness-Fehler — nicht das Brain.
pub fn classify_run(facts: &RunFacts) -> FailureClass {
    let all = facts.brain_texts.join("\n");

    if facts
        .brain_texts
        .iter()
        .any(|t| crate::brain::is_retryable_empty_response(t))
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
        "false_done" => FailureClass::FalseDone,
        "max_cycles" => FailureClass::CycleBudget,
        "wall_timeout" => FailureClass::Timeout,
        "protocol_error" => FailureClass::ProtocolViolation,
        "never_started" => FailureClass::NeverStarted,
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
        brain: meta
            .get("brain_id")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string(),
        status: meta
            .get("status")
            .and_then(|x| x.as_str())
            .unwrap_or("?")
            .to_string(),
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
        let f = facts(
            "protocol_error",
            &["Ich wuerde vorschlagen, die Datei anzupassen."],
            2,
        );
        assert_eq!(classify_run(&f), FailureClass::ProtocolViolation);
        assert!(classify_run(&f).blames_brain());
    }

    #[test]
    fn max_cycles_without_parse_errors_is_a_budget_problem() {
        let f = facts(
            "max_cycles",
            &["{\"protocol\":\"webagent/1\",\"actions\":[]}"],
            0,
        );
        assert_eq!(classify_run(&f), FailureClass::CycleBudget);
        assert!(classify_run(&f).blames_brain());
    }

    #[test]
    fn a_clean_run_passes() {
        let f = facts("done", &["{\"protocol\":\"webagent/1\",\"actions\":[]}"], 0);
        assert_eq!(classify_run(&f), FailureClass::Passed);
    }

    /// Ein `never_started`-Lauf hat nie gearbeitet: kein Brain-Fehlschlag, kein
    /// Abbruch, kein Erfolg — ein Nichtereignis fuer die Quoten.
    #[test]
    fn never_started_is_a_non_event() {
        let f = facts("never_started", &[], 0);
        assert_eq!(classify_run(&f), FailureClass::NeverStarted);
        assert!(!classify_run(&f).blames_brain());
        assert_ne!(classify_run(&f), FailureClass::Passed);
        assert_eq!(classify_run(&f).label(), "nie gestartet");
    }

    /// Eine Fertig-Meldung ohne gedeckte Datei-Aenderung darf nicht als
    /// bestanden und nicht als diffuses "sonstiges" durchgehen.
    #[test]
    fn false_done_is_blamed_on_the_brain_not_the_harness() {
        let f = facts("false_done", &["Fertig, alle Tests gruen."], 0);
        assert_eq!(classify_run(&f), FailureClass::FalseDone);
        assert!(classify_run(&f).blames_brain());
        assert_ne!(classify_run(&f), FailureClass::Passed);
    }

    #[test]
    fn external_block_beats_everything_else() {
        // Auch mit Parse-Fehlern: wer nie geantwortet hat, hat nichts falsch
        // gemacht.
        let f = facts(
            "protocol_error",
            &["No response, Please try again later."],
            3,
        );
        assert_eq!(classify_run(&f), FailureClass::ExternalBlock);
    }

    #[test]
    fn json_protocol_is_recognised_as_an_attempt() {
        assert!(looks_like_protocol_json(
            r#"{"protocol": "webagent/1", "actions": []}"#
        ));
        assert!(!looks_like_protocol_json("nur prosa"));
        // Auch mit Codefence und Einrueckung bleibt es ein Versuch.
        assert!(looks_like_protocol_json(
            "Hier:\n```json\n  {\"protocol\":\"webagent/1\",\"actions\":[]}\n```"
        ));

        // Regression 30.07.2026: claudes woertliche Weigerung. Sie ERWAEHNT das
        // Format mitten im Satz, versucht es aber nicht — und wurde deshalb
        // faelschlich als Harness-Fehler gemeldet.
        let weigerung = "Ich bleibe dabei: Ich kann hier keine echte Shell-Action \
             auf deinem Rechner ausloesen. Wenn ich hier {\"protocol\": \"webagent/1\", ...} \
             ausgebe, passiert nichts weiter, als dass du reinen Text siehst.";
        assert!(
            !looks_like_protocol_json(weigerung),
            "Reden ueber das Format ist kein Formatversuch"
        );
        let facts = facts("protocol_error", &[weigerung], 3);
        assert_ne!(
            classify_run(&facts),
            FailureClass::HarnessParseBug,
            "eine Weigerung darf nicht dem Harness angelastet werden"
        );
        assert!(has_raw_marker("bla\nWEBAGENT/1 WRITE\nid: x"));
        assert!(!has_raw_marker("bla blubb"));
    }
}
