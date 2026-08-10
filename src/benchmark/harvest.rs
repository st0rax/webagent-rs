//! Ernte (Phase C): Patch-Pruefung gegen die Aufgaben-Scope, Malus-/
//! Kompensations-Logik und der eigentliche Ernte-Commit nach bestandener
//! Messung.

use super::git::{patch_touched_paths, run_eval_detail};
use super::tasks::proposed_fn_name;
use super::types::{BenchmarkConfig, HarvestCandidate};

pub fn validate_harvest_patch(patch: &str) -> Result<Vec<String>, String> {
    let (paths, deleted) = patch_touched_paths(patch);
    // Eine Datei zu entfernen ist nicht "verbessern". Das ist ein Eingriff mit
    // Datenverlust und gehoert vor eine menschliche Entscheidung, nicht in
    // eine automatische Ernte.
    if !deleted.is_empty() {
        return Err(format!(
            "Patch loescht Datei(en) `{}` — Loeschungen werden nie automatisch geerntet",
            deleted.join(", ")
        ));
    }
    if paths.is_empty() {
        return Err("Patch enthält keine nachvollziehbaren Dateien".to_string());
    }
    if paths.len() > 4 {
        return Err(format!(
            "Patch berührt {} Dateien (Maximum: 4)",
            paths.len()
        ));
    }
    for path in &paths {
        let allowed = path.starts_with("src/") && path.ends_with(".rs");
        if !allowed {
            return Err(format!(
                "Patch berührt gesperrten Pfad `{path}` (nur bestehende Rust-Dateien unter src/ sind automatisch erntbar)"
            ));
        }
    }
    Ok(paths)
}

/// Prüft zusätzlich zum Datei-Scope den fachlichen Auftrag. Private Helfer und
/// Testfunktionen sind normal; eine neue öffentliche API ist dagegen nur dann
/// im Scope, wenn die verfeinerte Aufgabe genau diese Funktion verlangt.
pub fn validate_task_scope(patch: &str, task: &str) -> Result<Vec<String>, String> {
    let paths = validate_harvest_patch(patch)?;
    let expected = proposed_fn_name(task);
    let added_public: Vec<String> = patch
        .lines()
        .filter_map(|line| line.strip_prefix("+pub fn "))
        .map(|rest| {
            rest.chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    if !added_public.is_empty() {
        let unexpected: Vec<String> = added_public
            .iter()
            .filter(|name| expected.as_deref() != Some(name.as_str()))
            .cloned()
            .collect();
        if !unexpected.is_empty() {
            return Err(format!(
                "neue öffentliche Funktion(en) außerhalb des Auftrags: {}",
                unexpected.join(", ")
            ));
        }
    }
    Ok(paths)
}

/// Zahl der zusätzlichen positiven Score-Ereignisse für einen Scope-Verstoß.
/// Ein Verstoß startet immer mit einem Malus. Liefert der Patch aber einen
/// vollständigen, objektiv nachgewiesenen Nutzen (Build, zusätzliche Tests und
/// Lint), dürfen zwei positive Evidenzpunkte diesen Malus überkompensieren.
pub fn scope_compensation_count(technical_pass: bool, lint_passed: bool) -> usize {
    usize::from(technical_pass && lint_passed) * 2
}

/// Spielt einen geernteten Patch wieder ein, verifiziert ihn erneut (Build +
/// Tests) und committet ihn mit dem Brain als Autor.
///
/// Die Wiederholung von Build und Tests ist kein Ritual: der Patch wurde auf
/// einem Tree gemessen, der seither zurückgesetzt wurde — erst der zweite grüne
/// Durchlauf belegt, dass der Code auch eigenständig trägt.
pub(crate) fn harvest_commit(
    cand: &HarvestCandidate,
    winner: &str,
    config: &BenchmarkConfig,
) -> Result<(), String> {
    let patch_path = crate::config::data_dir()
        .join("benchmark")
        .join(format!("harvest_{}.patch", cand.brain));
    if let Some(parent) = patch_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
    }
    std::fs::write(&patch_path, &cand.patch).map_err(|e| format!("Patch schreiben: {e}"))?;

    let apply = std::process::Command::new("git")
        .args(["apply", "--index"])
        .arg(&patch_path)
        .current_dir(&config.workdir)
        .output()
        .map_err(|e| format!("git apply: {e}"))?;
    if !apply.status.success() {
        return Err(format!(
            "Patch von {} liess sich nicht einspielen: {}",
            cand.brain,
            String::from_utf8_lossy(&apply.stderr).trim()
        ));
    }

    let (b_ok, _) = run_eval_detail(&config.build_eval, &config.workdir);
    if !b_ok {
        return Err(format!(
            "Nachkontrolle: Build rot — {} verworfen",
            cand.brain
        ));
    }
    let (t_ok, _) = run_eval_detail(&config.test_eval, &config.workdir);
    if !t_ok {
        return Err(format!(
            "Nachkontrolle: Tests rot — {} verworfen",
            cand.brain
        ));
    }
    if !config.lint_eval.trim().is_empty() {
        let (l_ok, _) = run_eval_detail(&config.lint_eval, &config.workdir);
        if !l_ok {
            return Err(format!(
                "Nachkontrolle: Lint rot — {} verworfen",
                cand.brain
            ));
        }
    }

    let msg = format!(
        "feat(bench): {headline} — vom webagent gebaut ({brain})\n\n\
         Abstimmungs-Sieger der Benchmark-Runde, umgesetzt von {brain} in \
         {iters} Iteration(en). Nach dem Wiedereinspielen erneut verifiziert: \
         `{build}` und `{test}` gruen.\n\n\
         Authored-by: {brain} (webagent benchmark harvest)\n",
        headline = crate::char_prefix(winner.trim(), 72),
        brain = cand.brain,
        iters = cand.iterations,
        build = config.build_eval,
        test = config.test_eval,
    );
    let commit = std::process::Command::new("git")
        .args(["commit", "-m", &msg])
        .current_dir(&config.workdir)
        .output()
        .map_err(|e| format!("git commit: {e}"))?;
    if !commit.status.success() {
        return Err(format!(
            "Commit fehlgeschlagen: {}",
            String::from_utf8_lossy(&commit.stderr).trim()
        ));
    }
    Ok(())
}

