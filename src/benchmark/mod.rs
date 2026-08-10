//! benchmark — vote-driven, objektiver Code-Kompetenz-Benchmark.
//!
//! Anders als eine fixe Aufgabe misst der Benchmark den vollen
//! Selbst-Verbesserungs-Loop (siehe docs/BENCHMARK_PLAN.md):
//!
//! - **Phase A (Sammeln + Abstimmen):** `self_research::run_self_research` liefert
//!   eine Rangliste; der Platz-1-Vorschlag ([`winner_from_report`]) wird zur
//!   Benchmark-Aufgabe ([`build_task_prompt`]).
//! - **Phase B (Implementieren + Messen, pro Brain sequenziell):** sauberen
//!   Git-Tree prüfen, Baseline-SHA merken, mehrere Brains über den Controller
//!   (mit Wall-Timeout + kleinem `max_cycles`) DENSELBEN Abstimmungssieger bauen
//!   lassen, dann objektiv evaluieren (`did_change` →
//!   `cargo build --lib` → `cargo test --lib`), das
//!   [`CodeEvent`](crate::code_score::CodeEvent) speichern und den Tree hart auf
//!   die Baseline zurücksetzen (`git reset --hard` + `git clean -fd`). Jedes
//!   Brain startet identisch.
//! - **Phase C (Ernten):** der beste Diff eines BESTANDENEN Brains wird vor dem
//!   Reset gesichert und danach wieder eingespielt, erneut gebaut/getestet und
//!   mit dem Brain als Autor committet. Der Benchmark ist damit Fertigungsstraße UND
//!   Messgerät: er misst objektiv und behält, was die Messung bestanden hat.
//!   `--no-harvest` schaltet auf reines Messen zurück.
//!
//! `--rounds N` wiederholt Phase A+B N-mal (N Abstimmungen → N Sieger). Der Score
//! aggregiert über alle Events (`code_score::leaderboard`).
//!
//! Die reinen Helfer ([`build_task_prompt`], [`task_id`], [`winner_from_report`],
//! [`outcome_label`], [`is_pass`], [`format_benchmark_report`]) sind unit-getestet;
//! der Live-Teil (echtes Brain + `cargo` + Git) wird vom Orchestrator end-to-end
//! geprüft, nicht im Unit-Test.


mod git;
mod handoff;
mod harvest;
mod pipeline;
mod report;
mod tasks;
mod types;

pub(crate) use pipeline::bench_say;

// Bewertung ausgelagert nach `bench_scoring`, Ernte nach `bench_harvest`
// (Modul-Split). Hier re-exportiert, damit kein Aufrufer angefasst wird.
pub use crate::bench_harvest::{added_public_fns, harvest_rejection, pick_harvest};
pub use crate::bench_scoring::{
    count_build_errors, count_failed_tests, is_availability_outage, is_external_block,
    is_improvement, is_nonretryable_run_fault, is_pass, is_protocol_fault, outcome_label,
    progress_after_tests, Progress,
};

pub use git::{build_no_change_prompt, build_repair_prompt};
pub use harvest::{scope_compensation_count, validate_harvest_patch, validate_task_scope};
pub use pipeline::run_benchmark;
pub use report::{format_benchmark_report, format_benchmark_result};
pub use tasks::{assign_tasks, build_refine_prompt, build_task_prompt, build_task_prompt_in, file_outline, parse_test_count, proposed_fn_name, ranked_from_report, repair_focus_from_failures, target_file_of, task_id, task_is_redundant, task_targets_missing_file, usable_refinement, winner_from_report};
pub use types::{BenchmarkConfig, BenchmarkReport, HarvestCandidate};

#[cfg(test)]
mod refine_tests {
    use super::*;

    #[test]
    fn refine_prompt_demands_concrete_signature_and_tests() {
        let p = build_refine_prompt(
            "Sicherheitshaertung: Sandbox fuer Shell-Actions",
            "FAKTEN",
            &[],
        );
        assert!(p.contains("EXAKTER Rust-Signatur"), "{p}");
        assert!(p.contains("Testfaelle"), "{p}");
        assert!(p.contains("Sicherheitshaertung"), "Sieger muss drinstehen");
        assert!(p.contains("FAKTEN"), "Projektfakten muessen drinstehen");
        // Keine Architektur-Umbauten anfordern (sonst explorieren die Brains wieder).
        assert!(p.contains("KEINE Architektur-Umbauten"), "{p}");
    }

    #[test]
    fn test_count_gate_erkennt_verwaiste_datei() {
        // Anti-Schummel: eine nicht eingebundene Datei laesst die Testzahl gleich.
        let before = "test result: ok. 387 passed; 0 failed; 0 ignored";
        let after_orphan = "test result: ok. 387 passed; 0 failed; 0 ignored";
        let after_real = "test result: ok. 391 passed; 0 failed; 0 ignored";
        assert_eq!(parse_test_count(before), Some(387));
        assert_eq!(parse_test_count(after_orphan), Some(387));
        assert_eq!(parse_test_count(after_real), Some(391));
        // Mehrere Testbinaries werden summiert (lib + bin).
        let multi = "test result: ok. 10 passed; 0 failed\ntest result: ok. 5 passed; 0 failed";
        assert_eq!(parse_test_count(multi), Some(15));
        assert_eq!(parse_test_count("kein testoutput"), None);
    }

    #[test]
    fn refine_prompt_erzwingt_existierende_zieldatei() {
        let files = vec!["src/protocol.rs".to_string(), "src/executor.rs".to_string()];
        let p = build_refine_prompt("Sicherheit haerten", "FAKTEN", &files);
        assert!(p.contains("src/protocol.rs"), "Dateiliste fehlt");
        assert!(p.contains("erfinde KEINE Pfade"), "Verbot fehlt");
        // Ohne Liste ein sinnvoller Default statt leerer Aufzaehlung.
        let p2 = build_refine_prompt("x", "y", &[]);
        assert!(p2.contains("src/protocol.rs"));
    }

    #[test]
    fn repair_prompt_enthaelt_fehlerausgabe_und_verbietet_neuanfang() {
        let p = build_repair_prompt(
            "Baue pub fn foo() -> u8",
            "cargo build --lib",
            "error[E0433]: failed to resolve: use of undeclared crate `serde_jsonx`",
        );
        assert!(p.contains("Baue pub fn foo"), "Original-Aufgabe fehlt");
        assert!(p.contains("cargo build --lib"), "Stufe fehlt");
        assert!(p.contains("E0433"), "Fehlerausgabe fehlt");
        assert!(
            p.contains("NICHT von vorne"),
            "Neuanfang muss verboten sein"
        );
        // Sehr lange Ausgaben werden gekuerzt (Kontextbudget).
        let long = "x".repeat(9000);
        assert!(
            build_repair_prompt("t", "cargo test", &long)
                .chars()
                .count()
                < 3200
        );
    }

    #[test]
    fn redundanz_erkennung_verhindert_wertlose_runden() {
        // Storax-Beobachtung: Brains melden "ist bereits implementiert" und
        // aendern nichts -> wuerde faelschlich als FAIL zaehlen.
        let api = vec![
            "error_code".to_string(),
            "format_audit_line".to_string(),
            "parse".to_string(),
        ];
        let redundant = "In src/protocol.rs: fuege pub fn error_code(error: &str) -> &'static str \
                         hinzu, die Fehlermeldungen auf Slugs abbildet. Tests: a, b, c, d.";
        assert!(task_is_redundant(redundant, &api));

        let neu = "In src/protocol.rs: fuege pub fn action_summary(actions: &[Action]) -> String \
                   hinzu, die eine Kurzfassung liefert. Tests: leer, eine, viele, gemischt.";
        assert!(!task_is_redundant(neu, &api));

        // Ohne erkennbare Signatur nicht faelschlich als redundant werten.
        assert!(!task_is_redundant("Mach irgendwas mit Sicherheit.", &api));
    }

    #[test]
    fn proposed_fn_name_extrahiert_signatur() {
        assert_eq!(
            proposed_fn_name("... pub fn foo_bar(x: u8) -> bool ..."),
            Some("foo_bar".to_string())
        );
        assert_eq!(
            proposed_fn_name("Signatur: fn helper_fn() -> ()"),
            Some("helper_fn".to_string())
        );
        assert_eq!(proposed_fn_name("kein code hier"), None);
    }

    #[test]
    fn usable_refinement_rejects_too_short_and_keeps_real_specs() {
        assert_eq!(usable_refinement("   "), None);
        assert_eq!(usable_refinement("zu kurz"), None);
        let spec = "Datei src/foo.rs: fuege pub fn bar(x: &str) -> usize hinzu, die die \
                    Zeichenzahl liefert. Tests: leer, ascii, umlaute, lang.";
        assert_eq!(usable_refinement(spec), Some(spec.trim().to_string()));
        // Whitespace wird getrimmt.
        let padded = format!("   {spec}   ");
        assert_eq!(usable_refinement(&padded), Some(spec.trim().to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::git::{patch_touched_paths, tree_changed};
    use super::handoff::HandoffQueue;
    use crate::code_score::CodeStats;
    use crate::self_research::SelfResearchReport;
    use std::path::PathBuf;

    #[test]
    fn harvest_rejects_dead_code_and_test_deletions() {
        // Beide Muster stammen aus echten Ernten vom 2026-07-29.
        let dead = "+++ b/src/x.rs\n+pub fn normalize_research_suggestion(input: &str) -> String {\n+    input.trim().to_string()\n+}\n+    #[test]\n+    fn t() {\n+        assert_eq!(normalize_research_suggestion(\" a \"), \"a\");\n+    }\n";
        let reason = harvest_rejection(dead).expect("toter Code muss verworfen werden");
        assert!(reason.contains("normalize_research_suggestion"), "{reason}");

        let deletes = "+++ b/src/x.rs\n-    #[test]\n-    fn assess_command_risk_examples() {}\n+fn helper() {}\n";
        let reason = harvest_rejection(deletes).expect("Testloeschung muss verworfen werden");
        assert!(reason.contains("Test"), "{reason}");
    }

    #[test]
    fn harvest_rejects_a_function_that_only_its_own_tests_call() {
        // Real geerntet als 608d599: tote Observer-Struktur, in fuenf
        // mitgelieferten Tests aufgerufen. Der erste Filterentwurf hielt das
        // fuer "wird benutzt" und liess es durch.
        let selbsttest = concat!(
            "+++ b/src/observer.rs\n",
            "+pub fn set_expected_action_id(&mut self, id: u64) {\n",
            "+    self.expected_action_id = Some(id);\n",
            "+}\n",
            "+    #[test]\n",
            "+    fn t1() {\n",
            "+        let mut o = Observer::default();\n",
            "+        o.set_expected_action_id(7);\n",
            "+    }\n"
        );
        let reason = harvest_rejection(selbsttest)
            .expect("nur von eigenen Tests aufgerufen ist kein Aufrufer");
        assert!(reason.contains("set_expected_action_id"), "{reason}");
    }

    #[test]
    fn harvest_accepts_a_function_that_is_actually_used() {
        // Neue Funktion MIT Aufrufer im Produktivcode: das ist Verbesserung.
        let good = "+++ b/src/x.rs\n+pub fn parse_limit(s: &str) -> u32 { 0 }\n+    let n = parse_limit(raw);\n";
        assert!(harvest_rejection(good).is_none());
    }

    #[test]
    fn harvest_accepts_pure_refactoring_without_new_public_fns() {
        let refactor = "+++ b/src/x.rs\n-    let a = 1 + 1;\n+    let a = 2;\n";
        assert!(harvest_rejection(refactor).is_none());
    }
    use super::*;
    use crate::self_research::RankedSuggestion;

    // --- Harvest-Schutzgitter -------------------------------------------
    // Vorher wurden ausschliesslich `+++ b/`-Zeilen gelesen. Eine Loeschung
    // traegt dort `/dev/null`, ihr echter Pfad steht nur in `--- a/` — ein
    // Patch durfte damit eine erlaubte Datei aendern UND nebenbei eine
    // gesperrte loeschen, ohne dass das Gitter es sah.

    /// Baut einen Patch aus Einzelzeilen. Mehrzeilige String-Literale sind
    /// hier untauglich: `cargo fmt` ruecht sie ein und schiebt damit
    /// Leerzeichen vor `--- `/`+++ `, was den Parser scheitern laesst.
    fn patch_aus(zeilen: &[&str]) -> String {
        format!(
            "{}
",
            zeilen.join(
                "
"
            )
        )
    }

    #[test]
    fn loeschung_wird_erkannt_und_abgelehnt() {
        let patch = patch_aus(&[
            "diff --git a/Cargo.toml b/Cargo.toml",
            "deleted file mode 100644",
            "--- a/Cargo.toml",
            "+++ /dev/null",
            "@@ -1,2 +0,0 @@",
            "-[package]",
        ]);
        let (paths, deleted) = patch_touched_paths(&patch);
        assert!(
            paths.contains(&"Cargo.toml".to_string()),
            "Pfad unsichtbar: {paths:?}"
        );
        assert_eq!(deleted, vec!["Cargo.toml".to_string()]);
        let err = validate_harvest_patch(&patch).unwrap_err();
        assert!(err.contains("loescht"), "unerwartete Meldung: {err}");
    }

    /// Der eigentliche Angriffsfall: erlaubte Aenderung als Tarnung, daneben
    /// eine Loeschung ausserhalb von src/.
    #[test]
    fn getarnte_loeschung_neben_erlaubter_aenderung_faellt_auf() {
        let patch = patch_aus(&[
            "diff --git a/src/ok.rs b/src/ok.rs",
            "--- a/src/ok.rs",
            "+++ b/src/ok.rs",
            "@@ -1 +1,2 @@",
            "+// harmlos",
            "diff --git a/Cargo.toml b/Cargo.toml",
            "deleted file mode 100644",
            "--- a/Cargo.toml",
            "+++ /dev/null",
        ]);
        let err = validate_harvest_patch(&patch)
            .expect_err("getarnte Loeschung kam durch das Schutzgitter");
        assert!(err.contains("Cargo.toml"), "falscher Grund: {err}");
    }

    #[test]
    fn umbenennung_zeigt_beide_pfade() {
        let patch = patch_aus(&[
            "diff --git a/src/alt.rs b/src/neu.rs",
            "similarity index 100%",
            "rename from src/alt.rs",
            "rename to src/neu.rs",
        ]);
        let (paths, deleted) = patch_touched_paths(&patch);
        assert!(
            paths.contains(&"src/alt.rs".to_string()),
            "Quelle fehlt: {paths:?}"
        );
        assert!(
            paths.contains(&"src/neu.rs".to_string()),
            "Ziel fehlt: {paths:?}"
        );
        assert_eq!(deleted, vec!["src/alt.rs".to_string()]);
    }

    #[test]
    fn quotierte_pfade_werden_entpackt() {
        let patch = patch_aus(&[
            "diff --git \"a/src/mit leerzeichen.rs\" \"b/src/mit leerzeichen.rs\"",
            "--- \"a/src/mit leerzeichen.rs\"",
            "+++ \"b/src/mit leerzeichen.rs\"",
        ]);
        let (paths, _) = patch_touched_paths(&patch);
        assert_eq!(paths, vec!["src/mit leerzeichen.rs".to_string()]);
    }

    #[test]
    fn gewoehnliche_aenderung_bleibt_erntbar() {
        let patch = patch_aus(&[
            "diff --git a/src/benchmark.rs b/src/benchmark.rs",
            "--- a/src/benchmark.rs",
            "+++ b/src/benchmark.rs",
            "@@ -1 +1,2 @@",
            "+// noch eine Zeile",
        ]);
        let paths = validate_harvest_patch(&patch).expect("legitimer Patch abgelehnt");
        assert_eq!(paths, vec!["src/benchmark.rs".to_string()]);
    }

    // --- Handoff-Queue -----------------------------------------------------
    // Der reale Ausloeser: alle acht Brains bekommen DIESELBE Aufgabe, weil
    // `refined_cache` die Vorschlaege dedupliziert. Genau daran haing sich der
    // Endlos-Pingpong auf.

    fn acht_brains() -> Vec<String> {
        (1..=8).map(|i| format!("b{i}")).collect()
    }

    /// Plan, in dem alle Brains an derselben Aufgabe "T" sitzen.
    fn plan_alle_gleiche_aufgabe() -> Vec<(String, String)> {
        acht_brains()
            .into_iter()
            .map(|b| (b, "T".to_string()))
            .collect()
    }

    /// Faehrt die Queue leer und laesst JEDES gelieferte Brain sofort stallen.
    /// Bricht hart ab, statt zu haengen — ein Regress muss FEHLSCHLAGEN.
    fn leerfahren(q: &mut HandoffQueue) -> Vec<(String, String)> {
        let mut gesehen = Vec::new();
        let mut n = 0;
        while let Some((brain, effective, _)) = q.next() {
            n += 1;
            assert!(n < 100, "Endlosschleife: {n} Durchlaeufe ohne Ende");
            gesehen.push((brain.clone(), effective.clone()));
            q.on_stall(&brain, &effective);
        }
        gesehen
    }

    #[test]
    fn endlosschleife_ausgeschlossen() {
        let mut q = HandoffQueue::new(&plan_alle_gleiche_aufgabe(), &acht_brains(), 3);
        let gesehen = leerfahren(&mut q);
        // Die Queue laeuft leer, und die unloesbare Aufgabe ist endgueltig raus.
        assert!(q.is_dropped("T"));
        // Der eigentliche Regress-Waechter: frueher lief JEDER der acht
        // Plan-Eintraege plus jeder Handoff noch durch einen vollen Brain-Run,
        // obwohl die Aufgabe laengst ausgefallen war — im Log das endlose
        // "uebernimmt" -> "faellt aus" -> "uebernimmt". Nach dem Ausfall darf
        // kein Brain die Aufgabe mehr sehen, also hoechstens max_handoffs+1.
        assert!(
            gesehen.len() <= 4,
            "nach dem Ausfall liefen weitere Brain-Runs: {gesehen:?}"
        );
    }

    #[test]
    fn kein_brain_bekommt_aufgabe_zweimal() {
        let mut q = HandoffQueue::new(&plan_alle_gleiche_aufgabe(), &acht_brains(), 3);
        let gesehen = leerfahren(&mut q);
        let eindeutig: std::collections::HashSet<_> = gesehen.iter().collect();
        assert_eq!(
            eindeutig.len(),
            gesehen.len(),
            "ein Brain hat dieselbe Aufgabe mehrfach bekommen: {gesehen:?}"
        );
    }

    #[test]
    fn ausgefallene_aufgabe_wird_nicht_neu_eingereiht() {
        let mut q = HandoffQueue::new(&plan_alle_gleiche_aufgabe(), &acht_brains(), 3);
        // Bis zum ersten endgueltigen Ausfall fahren.
        let mut n = 0;
        while let Some((brain, effective, _)) = q.next() {
            n += 1;
            assert!(n < 100, "Endlosschleife");
            if q.on_stall(&brain, &effective).is_none() {
                break;
            }
        }
        assert!(q.is_dropped("T"));
        // Danach darf "T" nicht noch einmal ausgegeben werden.
        while let Some((_, effective, _)) = q.next() {
            assert_ne!(effective, "T", "ausgefallene Aufgabe wurde neu eingereiht");
        }
    }

    #[test]
    fn max_handoffs_wird_eingehalten() {
        let mut q = HandoffQueue::new(&plan_alle_gleiche_aufgabe(), &acht_brains(), 2);
        let gesehen = leerfahren(&mut q);
        let brains_fuer_t: std::collections::HashSet<_> = gesehen
            .iter()
            .filter(|(_, eff)| eff == "T")
            .map(|(b, _)| b.clone())
            .collect();
        // max_handoffs=2 heisst: Erstzuteilung + 2 Weitergaben = 3 Brains.
        assert!(
            brains_fuer_t.len() <= 3,
            "zu viele Brains an derselben Aufgabe: {brains_fuer_t:?}"
        );
    }

    fn stats(brain: &str, attempts: usize, wilson: f64) -> CodeStats {
        CodeStats {
            hard_attempts: 0,
            rescues: 0,
            abandoned: 0,
            brain_id: brain.to_string(),
            attempts,
            change_rate: 1.0,
            compile_rate: 1.0,
            pass_rate: 1.0,
            wilson_pass: wilson,
            avg_field: 0.0,
            significance: 0.0,
        }
    }

    #[test]
    fn task_id_is_stable_and_distinct() {
        assert_eq!(task_id("Sandbox einführen"), task_id("Sandbox einführen"));
        // Whitespace-robust (getrimmt).
        assert_eq!(
            task_id("  Sandbox einführen "),
            task_id("Sandbox einführen")
        );
        assert_ne!(task_id("Sandbox einführen"), task_id("Tests ergänzen"));
    }

    #[test]
    fn build_task_prompt_contains_winner_and_contract() {
        let p = build_task_prompt("Protokoll versionieren");
        assert!(p.contains("Protokoll versionieren"));
        assert!(p.contains("WEBAGENT/1"));
        assert!(p.contains("cargo test --lib"));
        assert!(p.contains("Rohformat"));
    }

    #[test]
    fn winner_from_report_takes_top_one() {
        let report = SelfResearchReport {
            catalog: vec!["Alpha".to_string(), "Beta".to_string()],
            ranked: vec![
                RankedSuggestion {
                    index: 2,
                    text: "Beta".to_string(),
                    points: 6,
                    approvals: 2,
                },
                RankedSuggestion {
                    index: 1,
                    text: "Alpha".to_string(),
                    points: 4,
                    approvals: 2,
                },
            ],
            consolidated_by: Some("claude".to_string()),
            collected: 2,
            voters: 2,
            brains_total: 2,
        };
        assert_eq!(winner_from_report(&report), Some("Beta".to_string()));
    }

    #[test]
    fn winner_from_report_none_without_votes() {
        let report = SelfResearchReport {
            catalog: vec!["Alpha".to_string()],
            ranked: Vec::new(),
            consolidated_by: None,
            collected: 1,
            voters: 0,
            brains_total: 1,
        };
        assert_eq!(winner_from_report(&report), None);
    }

    #[test]
    fn is_pass_requires_all_three() {
        assert!(is_pass(true, true, true));
        assert!(!is_pass(false, true, true));
        assert!(!is_pass(true, false, true));
        assert!(!is_pass(true, true, false));
    }

    #[test]
    fn outcome_label_classifies_each_stage() {
        assert_eq!(outcome_label(false, false, false), "SKIP (keine Änderung)");
        assert_eq!(outcome_label(true, false, false), "FAIL (build)");
        assert_eq!(outcome_label(true, true, false), "FAIL (test)");
        assert_eq!(outcome_label(true, true, true), "PASS");
    }

    #[test]
    fn repair_focus_keeps_internal_failures_and_bounds_noise() {
        let focus = repair_focus_from_failures(&[
            "Build-Gate bei a: cannot find value x".to_string(),
            "Build-Gate bei a: cannot find value x".to_string(),
            "Test-Gate bei b: assertion failed".to_string(),
        ])
        .expect("interne Befunde ergeben einen Reparaturfokus");
        assert!(focus.contains("REPARATURPRIORITÄT"));
        assert!(focus.contains("cannot find value x"));
        assert_eq!(focus.matches("cannot find value x").count(), 1);
    }

    #[test]
    fn gliederung_ersetzt_das_scheibchenweise_lesen() {
        // Belegt den Groessenunterschied, um den es geht: die Gliederung von
        // protocol.rs ist ein Bruchteil der Datei. Am 30.07.2026 verbrauchte
        // zai alle 15 Zyklen einer Runde mit Lesen und kam nie zum Editieren.
        let datei = crate::config::root_dir().join("src/protocol.rs");
        if !datei.is_file() {
            return; // im Zweifel nicht raten: Datei kann umbenannt worden sein
        }
        let voll = std::fs::read_to_string(&datei).expect("lesbar");
        let gliederung = file_outline(&datei, 120).expect("Signaturen vorhanden");
        assert!(
            gliederung.len() * 5 < voll.len(),
            "Gliederung ({} Zeichen) muss deutlich kleiner sein als die Datei ({})",
            gliederung.len(),
            voll.len()
        );
        assert!(gliederung.contains("GLIEDERUNG"));

        // Und sie landet im Aufgabentext, sobald der Plan eine Zieldatei nennt.
        let plan = "Zieldatei: src/protocol.rs. Implementiere dort eine Funktion.";
        assert_eq!(target_file_of(plan).as_deref(), Some("src/protocol.rs"));
        let prompt = build_task_prompt_in(plan, &crate::config::root_dir());
        assert!(prompt.contains("GLIEDERUNG"), "Gliederung fehlt im Prompt");

        // Ohne Zieldatei bleibt der Prompt unveraendert kurz.
        assert!(target_file_of("irgendein Vorschlag ohne Datei").is_none());
    }

    #[test]
    fn task_targets_missing_file_guards_against_fake_paths() {
        // Regression 2026-08-01: Plan auf nicht existierender Zieldatei → Brain
        // sucht ewig, aendert nichts (did_change=false). Der Refine-Guard soll
        // so einen Plan verwerfen.
        let files = vec![
            "src/protocol.rs".to_string(),
            "src/repl/mod.rs".to_string(),
        ];
        // Erlaubte Datei wird akzeptiert (auch mit src/ prefix).
        assert!(!task_targets_missing_file(
            "Zieldatei: src/repl/mod.rs. Fuege foo() hinzu.",
            &files
        ));
        assert!(!task_targets_missing_file(
            "Zieldatei: repl/mod.rs. Fuege foo() hinzu.",
            &files
        ));
        // Erfundene Datei wird verworfen.
        assert!(task_targets_missing_file(
            "Zieldatei: src/repl.rs. Fuege foo() hinzu.",
            &files
        ));
        // Ohne Zieldatei-Angabe nicht pruefbar → akzeptieren.
        assert!(!task_targets_missing_file(
            "Fuege foo() zu den Repl-Commands hinzu.",
            &files
        ));
        // Leere Modulliste = nichts pruefbar → akzeptieren (kein Pauschal-Kill).
        assert!(!task_targets_missing_file(
            "Zieldatei: src/irgendwas.rs.",
            &[]
        ));
    }

    #[test]
    fn gescheitertes_git_status_gilt_nicht_als_unveraendert() {
        // Regression 30.07.2026: `Err(_) => false` machte aus „git ging nicht"
        // ein „nichts geaendert". Ein transienter Git-Fehler loescht damit die
        // Arbeit eines Brains aus der Messung, und weil `is_pass` zwingend
        // `did_change` verlangt, ist danach keine Ernte mehr moeglich.
        //
        // Ein Verzeichnis ohne Git-Repo erzwingt den Fehlerfall.
        let stamp = std::process::id();
        let dir = std::env::temp_dir().join(format!("webagent_kein_repo_{stamp}"));
        let _ = std::fs::create_dir_all(&dir);
        assert!(
            tree_changed(&dir),
            "gescheitertes git status muss als Aenderung gelten, nicht als sauber"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn verfuegbarkeitsstoerung_verbraucht_kein_abbruchbudget() {
        // Realfall 29.07.2026, 23:44: mistral und qwen im Nachrichtenlimit,
        // gemini ausgeloggt. Kein Brain kam zum Messen — die Runde sagt ueber
        // den Code nichts aus und darf den Dauerlauf nicht beenden.
        assert!(is_availability_outage(0, &[]));

        // Gegenprobe: kam mindestens ein Brain dran, ist die Runde eine echte
        // Aussage, auch wenn der Rest gesperrt war.
        assert!(!is_availability_outage(1, &[]));
        assert!(!is_availability_outage(0, &["cargo test rot".to_string()]));
        assert!(!is_availability_outage(3, &["build rot".to_string()]));
    }

    #[test]
    fn format_report_shows_winners_and_table() {
        let winners = vec![
            (1, "Sandbox einführen".to_string()),
            (2, "Tests ergänzen".to_string()),
        ];
        let board = vec![stats("kimi", 4, 0.512), stats("qwen", 4, 0.180)];
        let body = format_benchmark_report(&winners, &board);
        assert!(body.contains("1. Sandbox einführen"));
        assert!(body.contains("2. Tests ergänzen"));
        assert!(body.contains("| brain | attempts | change% | compile% | pass% | wilson_pass |"));
        assert!(body.contains("| kimi | 4 |"));
        assert!(body.contains("0.512"));
    }

    #[test]
    fn format_report_handles_empty() {
        let body = format_benchmark_report(&[], &[]);
        assert!(body.contains("(keine — keine Stimmen gesammelt)"));
        assert!(body.contains("(keine Daten)"));
    }

    fn cand(brain: &str, iters: u32, ms: u64, patch: &str) -> HarvestCandidate {
        HarvestCandidate {
            brain: brain.to_string(),
            task: "Testaufgabe".to_string(),
            patch: patch.to_string(),
            iterations: iters,
            latency_ms: ms,
        }
    }

    #[test]
    fn harvest_prefers_fewer_iterations() {
        // Beim ersten Versuch gruen schlaegt "nach neun Korrekturen gruen" —
        // beide bestehen, nur eines davon ist verlaessliche Arbeit.
        let pool = vec![
            cand("zai", 9, 1_000, "diff --git a/src/x.rs"),
            cand("deepseek", 1, 90_000, "diff --git a/src/y.rs"),
        ];
        assert_eq!(pick_harvest(&pool).unwrap().brain, "deepseek");
    }

    #[test]
    fn harvest_breaks_iteration_tie_by_latency() {
        let pool = vec![
            cand("kimi", 2, 80_000, "diff --git a/src/x.rs"),
            cand("deepseek", 2, 20_000, "diff --git a/src/y.rs"),
        ];
        assert_eq!(pick_harvest(&pool).unwrap().brain, "deepseek");
    }

    #[test]
    fn harvest_ignores_empty_patches_and_empty_pool() {
        // Ein PASS ohne Diff waere nichts zum Einspielen — darf nicht gewinnen.
        let pool = vec![cand(
            "geist", 1, 10, "   
  ",
        )];
        assert!(pick_harvest(&pool).is_none());
        assert!(pick_harvest(&[]).is_none());
    }

    #[test]
    fn harvest_scope_allows_small_rust_only_patch() {
        let patch = "diff --git a/src/benchmark.rs b/src/benchmark.rs\n--- a/src/benchmark.rs\n+++ b/src/benchmark.rs\n@@ -1 +1 @@\n-old\n+new\n";
        assert_eq!(
            validate_harvest_patch(patch).unwrap(),
            vec!["src/benchmark.rs".to_string()]
        );
    }

    #[test]
    fn harvest_scope_rejects_dependency_change() {
        let patch = "diff --git a/Cargo.toml b/Cargo.toml\n--- a/Cargo.toml\n+++ b/Cargo.toml\n@@ -1 +1 @@\n-old\n+new\n";
        assert!(validate_harvest_patch(patch)
            .unwrap_err()
            .contains("gesperrten Pfad"));
    }

    #[test]
    fn task_scope_rejects_unrequested_public_api() {
        let patch = "diff --git a/src/benchmark.rs b/src/benchmark.rs\n--- a/src/benchmark.rs\n+++ b/src/benchmark.rs\n@@ -1 +1 @@\n+pub fn surprise_feature() {}\n";
        let task = "Ändere genau eine Funktion: pub fn requested_feature() -> bool";
        assert!(validate_task_scope(patch, task)
            .unwrap_err()
            .contains("surprise_feature"));
    }

    #[test]
    fn task_scope_allows_requested_public_api_and_private_helper() {
        let patch = "diff --git a/src/benchmark.rs b/src/benchmark.rs\n--- a/src/benchmark.rs\n+++ b/src/benchmark.rs\n@@ -1 +1 @@\n+pub fn requested_feature() -> bool { helper() }\n+fn helper() -> bool { true }\n";
        let task = "Implementiere pub fn requested_feature() -> bool";
        assert!(validate_task_scope(patch, task).is_ok());
    }

    #[test]
    fn scope_malus_is_only_offset_by_complete_quality_evidence() {
        assert_eq!(scope_compensation_count(false, true), 0);
        assert_eq!(scope_compensation_count(true, false), 0);
        assert_eq!(scope_compensation_count(true, true), 2);
    }

    #[test]
    fn harvest_flag_off_is_the_default_shape() {
        // Ohne --harvest bleibt der Benchmark ein reines Messgeraet.
        assert!(!bench_config_for_test().harvest);
    }

    fn bench_config_for_test() -> BenchmarkConfig {
        BenchmarkConfig {
            brains: vec!["deepseek".to_string()],
            rounds: 1,
            suggestions: 10,
            build_eval: "cargo build --lib".to_string(),
            test_eval: "cargo test --lib".to_string(),
            workdir: PathBuf::from("."),
            headless: true,
            max_iterations: 10,
            harvest: false,
            verbose: false,
            parallel: 4,
            stall_limit: 3,
            max_handoffs: 2,
            lint_eval: String::new(),
            vetoes: Vec::new(),
            loop_forever: false,
        }
    }

    #[test]
    fn assign_gives_every_brain_its_own_task() {
        // Fertigungsstrasse: acht Brains, acht verschiedene Raenge — sonst
        // kollidieren die Patches und nur einer waere erntbar.
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = ["r1", "r2", "r3"].iter().map(|s| s.to_string()).collect();
        let got = assign_tasks(&brains, &ranked, 0);
        let tasks: std::collections::HashSet<&String> = got.iter().map(|(_, t)| t).collect();
        assert_eq!(got.len(), 3);
        assert_eq!(tasks.len(), 3, "jedes Brain braucht eine eigene Aufgabe");
    }

    #[test]
    fn assign_rotates_across_rounds_so_scoring_stays_fair() {
        // Ueber die Runden muss jedes Brain jeden Rang sehen, sonst zieht eines
        // dauerhaft die leichteren Aufgaben und der Score waere verzerrt.
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = ["r1", "r2", "r3"].iter().map(|s| s.to_string()).collect();
        let seen: std::collections::HashSet<String> = (0..3)
            .flat_map(|round| assign_tasks(&brains, &ranked, round))
            .filter(|(b, _)| b == "a")
            .map(|(_, t)| t)
            .collect();
        assert_eq!(
            seen.len(),
            3,
            "Brain a muss ueber 3 Runden alle 3 Raenge bauen"
        );
    }

    #[test]
    fn assign_shares_ranks_when_fewer_suggestions_than_brains() {
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = vec!["nur_einer".to_string()];
        let got = assign_tasks(&brains, &ranked, 0);
        assert_eq!(got.len(), 3);
        assert!(got.iter().all(|(_, t)| t == "nur_einer"));
    }

    #[test]
    fn assign_rotates_the_first_builder_across_rounds() {
        // Bei nur einem Sieger-Rang darf nicht immer dasselbe Brain den
        // Implementierer-Job bekommen (chatgpt-Problem 2026-08-02).
        let brains: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let ranked: Vec<String> = vec!["sieger".to_string()];
        let firsts: Vec<String> = (0..3)
            .map(|round| assign_tasks(&brains, &ranked, round)[0].0.clone())
            .collect();
        assert_eq!(firsts, ["a", "b", "c"], "der erste Bauplatz muss rotieren");
    }

    #[test]
    fn assign_without_suggestions_is_empty() {
        let brains: Vec<String> = vec!["a".to_string()];
        assert!(assign_tasks(&brains, &[], 0).is_empty());
    }

    #[test]
    fn ranked_from_report_drops_empty_entries() {
        let report = SelfResearchReport {
            catalog: Vec::new(),
            ranked: vec![
                crate::self_research::RankedSuggestion {
                    index: 1,
                    text: "Fehlerhierarchie mit thiserror einfuehren".to_string(),
                    points: 10,
                    approvals: 2,
                },
                crate::self_research::RankedSuggestion {
                    index: 2,
                    text: "   ".to_string(),
                    points: 5,
                    approvals: 1,
                },
            ],
            consolidated_by: None,
            collected: 2,
            voters: 2,
            brains_total: 2,
        };
        assert_eq!(ranked_from_report(&report).len(), 1);
    }

    #[test]
    fn fewer_compiler_errors_counts_as_progress() {
        // Der eigentliche Punkt: wer sich von zwoelf Fehlern auf zwei
        // herunterarbeitet, kommt voran — auch wenn der Build weiter rot ist.
        let vorher = Progress {
            stage: 1,
            errors: 12,
        };
        let nachher = Progress {
            stage: 1,
            errors: 2,
        };
        assert!(is_improvement(Some(vorher), nachher));
        assert!(!is_improvement(Some(nachher), vorher));
    }

    #[test]
    fn same_errors_twice_is_no_progress() {
        // Dieselbe kaputte Zeile nochmal schreiben zaehlt als Stillstand.
        let p = Progress {
            stage: 1,
            errors: 5,
        };
        assert!(!is_improvement(Some(p), p));
    }

    #[test]
    fn reaching_the_next_stage_always_counts() {
        // Build gruen (Stufe 2) schlaegt roten Build, selbst wenn danach mehr
        // rote Tests offen sind als vorher Compilerfehler.
        let build_rot = Progress {
            stage: 1,
            errors: 1,
        };
        let tests_rot = Progress {
            stage: 2,
            errors: 40,
        };
        assert!(is_improvement(Some(build_rot), tests_rot));
    }

    #[test]
    fn first_change_is_progress_but_doing_nothing_is_not() {
        assert!(is_improvement(
            None,
            Progress {
                stage: 1,
                errors: 9
            }
        ));
        assert!(!is_improvement(
            None,
            Progress {
                stage: 0,
                errors: 0
            }
        ));
    }

    #[test]
    fn build_errors_are_counted_without_the_summary_line() {
        let out = "error[E0308]: mismatched types
                   error[E0061]: wrong args
                   error: could not compile `webagent` (lib) due to 2 previous errors
";
        // Die Schlusszeile meldet nur den Sammelabbruch — sie waere sonst ein
        // Phantom-Fehler, der jeden Fortschritt um genau eins verwaessert.
        assert_eq!(count_build_errors(out), 2);
    }

    #[test]
    fn failed_tests_are_summed_across_targets() {
        let out = "test result: FAILED. 380 passed; 3 failed; 0 ignored
                   test result: FAILED. 12 passed; 1 failed; 0 ignored
";
        assert_eq!(count_failed_tests(out), 4);
        assert_eq!(
            count_failed_tests("test result: ok. 400 passed; 0 failed"),
            0
        );
    }

    #[test]
    fn tests_that_never_ran_do_not_outrank_tests_that_ran_and_failed() {
        // Realfall deepseek 2026-07-21: `cargo test` meldete zehnmal "0
        // bestanden", weil die Test-Binary nicht uebersetzte. Naiv sind das
        // null rote Tests — der scheinbar beste Stand ueberhaupt.
        let nie_gelaufen = progress_after_tests(
            "error[E0425]: cannot find value `foo`
error: could not compile `webagent` (lib test)",
        );
        let gelaufen_rot =
            progress_after_tests("test result: FAILED. 380 passed; 3 failed; 0 ignored");
        assert_eq!(
            nie_gelaufen.stage, 1,
            "nicht uebersetzte Tests sind kein Stufe-2-Ergebnis"
        );
        assert_eq!(gelaufen_rot.stage, 2);
        assert!(
            is_improvement(Some(nie_gelaufen), gelaufen_rot),
            "Tests zum Laufen zu bringen muss als Fortschritt zaehlen"
        );
        assert!(!is_improvement(Some(gelaufen_rot), nie_gelaufen));
    }

    #[test]
    fn repeated_uncompilable_tests_stall_instead_of_looking_perfect() {
        let a = progress_after_tests("error[E0308]: mismatched types");
        let b = progress_after_tests("error[E0308]: mismatched types");
        assert!(
            !is_improvement(Some(a), b),
            "zweimal derselbe Fehler ist Stillstand"
        );
    }

    #[test]
    fn external_blocks_are_recognised_and_not_confused_with_failure() {
        // Nur diese duerfen aus der Wertung fallen ...
        assert!(is_external_block("brain_unavailable"));
        assert!(is_external_block("blocked: Nachrichtenlimit erreicht"));
        assert!(is_external_block("login_required"));
        assert!(is_external_block("loginrequired"));
        assert!(is_external_block("cloudflare"));
        // ... echte Fehlversuche NICHT, sonst verschwindet Unfaehigkeit aus
        // der Statistik und der Score wird bedeutungslos.
        assert!(!is_external_block("protocol_error"));
        assert!(!is_external_block("max_cycles"));
        assert!(!is_external_block("brain_incomplete"));
        assert!(!is_external_block("done"));
    }

    #[test]
    fn protocol_faults_stop_retries_without_becoming_external_blocks() {
        assert!(is_protocol_fault("protocol_error"));
        assert!(is_protocol_fault("Protocol_Invalid"));
        assert!(!is_protocol_fault("rate_limit"));
        assert!(!is_protocol_fault("done"));
    }

    #[test]
    fn terminal_run_faults_are_not_retried() {
        assert!(is_nonretryable_run_fault("protocol_error"));
        assert!(is_nonretryable_run_fault("wall_timeout"));
        assert!(is_nonretryable_run_fault("false_done"));
        assert!(is_nonretryable_run_fault("max_cycles"));
        assert!(!is_nonretryable_run_fault("done"));
    }

    #[test]
    fn no_change_prompt_demands_a_real_edit() {
        // Gegen das Phantom-Done: kimi meldete "Implementierung erfolgreich"
        // per message, ohne zu editieren. Der Anstoss muss unmissverstaendlich
        // einen echten Edit verlangen und Erfolgsmeldungen entwerten.
        let p = build_no_change_prompt("Implementiere pub fn foo() in src/x.rs");
        assert!(p.contains("KEINE Datei"));
        assert!(p.contains("WEBAGENT/1 EDIT") || p.contains("WEBAGENT/1 WRITE"));
        assert!(p.to_lowercase().contains("behaupte keinen erfolg"));
        // Die urspruengliche Aufgabe bleibt erhalten.
        assert!(p.contains("src/x.rs"));
    }
}


#[cfg(test)]
mod format_benchmark_result_tests {
    use super::*;

    #[test]
    fn standard_format() {
        assert_eq!(
            format_benchmark_result("selector_drift", 12, "ms"),
            "selector_drift=12ms"
        );
    }

    #[test]
    fn zero_value() {
        assert_eq!(
            format_benchmark_result("timeout", 0, "count"),
            "timeout=0count"
        );
    }

    #[test]
    fn empty_name() {
        assert_eq!(format_benchmark_result("", 42, "bytes"), "=42bytes");
    }

    #[test]
    fn deterministic() {
        let a = format_benchmark_result("test", 100, "ops");
        let b = format_benchmark_result("test", 100, "ops");
        assert_eq!(a, b);
    }

    #[test]
    fn max_value() {
        let expected = format!("large_output={}bytes", u64::MAX);
        assert_eq!(
            format_benchmark_result("large_output", u64::MAX, "bytes"),
            expected
        );
    }
}

