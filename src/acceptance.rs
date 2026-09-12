//! Verifizierter Taskabschluss (T-806-Rest).
//!
//! Ein von einem Brain erzeugtes `CompletionReceipt` („Manifest") ist ein
//! ANTRAG, kein eigener Beweis. Erst die controllerseitige Gegenprüfung —
//! Run-ID des tatsächlich gelaufenen Runs, Brain-Bindung, Commit,
//! Geltungsbereich und der Status des Runs — entscheidet, ob der Abschluss
//! akzeptiert wird. Alte, leere, fremde oder manipulierte Belege dürfen nie
//! zu `done` führen und werden bei vorhandenem Run im Run-Eventlog als
//! Ablehnung persistiert.

use serde::{Deserialize, Serialize};

pub const ACCEPTANCE_VERSION: u32 = 1;

/// Ergebnis eines einzelnen Pflichtkriteriums. Bestanden werden darf nur
/// `Passed`; `Failed`, `Unreachable` und `NotRun` sind keine Abschluss-Basis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriterionOutcome {
    Passed,
    Failed,
    Unreachable,
    NotRun,
}

/// Vom Brain erzeugtes Abschluss-Manifest: ein Antrag, kein Beweis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionReceipt {
    pub version: u32,
    pub task_id: String,
    pub run_id: String,
    pub brain_id: String,
    pub commit: String,
    pub scope: String,
    /// Pflichtkriterien der Abnahme — nur `Passed` zählt.
    pub criteria: Vec<CriterionOutcome>,
    /// Artefakt-/Beleghashes, die der Controller zur Prüfung erhalten hat.
    pub artifact_hashes: Vec<String>,
}

/// Controllerseitige Erwartungen, gegen die der Antrag geprüft wird.
#[derive(Debug, Clone)]
pub struct ExpectedCompletion {
    pub task_id: String,
    pub run_id: String,
    pub brain_id: String,
    pub commit: String,
    /// Vom RunStore gelesener Run-Status; `done` ist Voraussetzung.
    pub run_status: String,
}

/// Prüft einen Abschluss-Antrag gegen die Erwartungen. Liefert `Ok(())` nur,
/// wenn Version, Bindung (task/run/brain/commit), Geltungsbereich, laufender
/// Run-Status und alle Pflichtkriterien stimmen.
pub fn verify_completion(
    receipt: &CompletionReceipt,
    expected: &ExpectedCompletion,
) -> Result<(), String> {
    if receipt.version != ACCEPTANCE_VERSION {
        return Err(format!(
            "Receipt-Version {} wird nicht unterstützt (erwartet {})",
            receipt.version, ACCEPTANCE_VERSION
        ));
    }
    if receipt.task_id.trim().is_empty()
        || receipt.run_id.trim().is_empty()
        || receipt.brain_id.trim().is_empty()
        || receipt.commit.trim().is_empty()
        || receipt.scope.trim().is_empty()
    {
        return Err("Receipt enthält leere Pflichtfelder (task_id, run_id, brain_id, commit, scope)".into());
    }
    if receipt.task_id != expected.task_id {
        return Err(format!(
            "Fremder Beleg: Receipt task_id={} != erwartet {}",
            receipt.task_id, expected.task_id
        ));
    }
    if receipt.run_id != expected.run_id {
        return Err(format!(
            "Alter/fremder Beleg: Receipt run_id={} != erwartet {}",
            receipt.run_id, expected.run_id
        ));
    }
    if receipt.brain_id != expected.brain_id {
        return Err(format!(
            "Fremde Brain-Bindung: Receipt brain_id={} != erwartet {}",
            receipt.brain_id, expected.brain_id
        ));
    }
    if receipt.commit != expected.commit {
        return Err(format!(
            "Veralteter Beleg: Receipt commit={} != aktueller Head {}",
            receipt.commit, expected.commit
        ));
    }
    if expected.run_status != "done" {
        return Err(format!(
            "Run {} ist nicht abgeschlossen (status={})",
            expected.run_id, expected.run_status
        ));
    }
    if receipt.criteria.is_empty() {
        return Err("Leerer Beleg: kein einziges Pflichtkriterium aufgeführt".into());
    }
    if receipt.artifact_hashes.is_empty() {
        return Err("Beleg ohne Artefakthashes ist kein belastbarer Nachweis".into());
    }
    for (i, outcome) in receipt.criteria.iter().enumerate() {
        if *outcome != CriterionOutcome::Passed {
            return Err(format!(
                "Pflichtkriterium {i} ist {outcome:?} — kein Abschluss, solange nicht alle bestanden sind"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(done_status: &str) -> (CompletionReceipt, ExpectedCompletion) {
        let r = CompletionReceipt {
            version: ACCEPTANCE_VERSION,
            task_id: "T-806".into(),
            run_id: "run-1".into(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            scope: "src/capability.rs".into(),
            criteria: vec![CriterionOutcome::Passed, CriterionOutcome::Passed],
            artifact_hashes: vec!["sha256:beleg".into()],
        };
        let e = ExpectedCompletion {
            task_id: "T-806".into(),
            run_id: "run-1".into(),
            brain_id: "claude".into(),
            commit: "abc123".into(),
            run_status: done_status.into(),
        };
        (r, e)
    }

    #[test]
    fn vollstaendiger_antrag_wird_akzeptiert() {
        let (r, e) = receipt("done");
        assert!(verify_completion(&r, &e).is_ok());
    }

    #[test]
    fn alter_oder_fremder_beleg_wird_abgelehnt() {
        let (mut r, e) = receipt("done");
        r.commit = "old999".into();
        assert!(verify_completion(&r, &e).is_err(), "alter Commit");
        let (mut r, e) = receipt("done");
        r.run_id = "run-999".into();
        assert!(verify_completion(&r, &e).is_err(), "fremde Run-ID");
        let (mut r, e) = receipt("done");
        r.brain_id = "chatgpt".into();
        assert!(verify_completion(&r, &e).is_err(), "fremde Brain-Bindung");
        let (mut r, e) = receipt("done");
        r.task_id = "T-999".into();
        assert!(verify_completion(&r, &e).is_err(), "fremde Task-ID");
    }

    #[test]
    fn laufender_run_wird_abgelehnt() {
        let (r, e) = receipt("claimed");
        assert!(verify_completion(&r, &e).is_err());
    }

    #[test]
    fn leere_felder_werden_abgelehnt() {
        let (mut r, e) = receipt("done");
        r.scope.clear();
        assert!(verify_completion(&r, &e).is_err());
        let (mut r, e) = receipt("done");
        r.criteria.clear();
        assert!(verify_completion(&r, &e).is_err(), "leere Kriterienliste");
        let (mut r, e) = receipt("done");
        r.artifact_hashes.clear();
        assert!(verify_completion(&r, &e).is_err(), "ohne Artefakthashes");
    }

    #[test]
    fn nicht_bestandene_kriterien_wie_unreachable_failed_notrun_werden_abgelehnt() {
        for outcome in [
            CriterionOutcome::Failed,
            CriterionOutcome::Unreachable,
            CriterionOutcome::NotRun,
        ] {
            let (mut r, e) = receipt("done");
            r.criteria[1] = outcome;
            assert!(
                verify_completion(&r, &e).is_err(),
                "{outcome:?} darf nie einen Abschluss begründen"
            );
        }
    }
}