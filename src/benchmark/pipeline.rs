//! Benchmark-Fertigungsstrasse (Phase B + Runden-Loop): Bench-Runs ueber den
//! Controller, Refinement der Aufgabe, die Handoff-Schleife und die
//! Auswertung samt Erntepool. Hier steht `run_benchmark`, der Einstieg des
//! Moduls.
//!
//! Die Zwischenkonsole `bench_say!` ist hier definiert und wird ueber die
//! Modulwurzel fuer die Schwesternmodule re-exportiert.

use std::time::Instant;

use crate::code_score::CodeEvent;

use super::git::{
    build_no_change_prompt, build_repair_prompt, capture_patch, reset_repo, run_eval_detail,
    tree_changed,
};
use super::handoff::HandoffQueue;
use super::harvest::{harvest_commit, persist_candidate, scope_compensation_count, validate_task_scope};
use super::report::{format_benchmark_report, print_leaderboard};
use super::tasks::{
    assign_tasks, build_refine_prompt, file_outline, misdirection_detail, parse_test_count,
    phantom_detail, proposed_fn_name, ranked_from_report, refinement_has_evidence,
    repair_focus_from_failures, suggestion_target, target_file_of, task_id,
    task_has_phantom_anchors, task_is_misdirected, task_is_redundant, task_targets_missing_file,
    usable_refinement,
};
use super::types::{BenchmarkConfig, BenchmarkReport, HarvestCandidate};
use super::{
    count_build_errors, is_availability_outage, is_external_block, is_improvement,
    is_nonretryable_run_fault, is_pass, is_protocol_fault, outcome_label, pick_harvest,
    progress_after_tests, Progress,
};

/// Meldet eine Zeile gleichzeitig an Konsole und Ereignisstrom.
///
/// EIN Aufrufpunkt fuer beides, damit die TUI und die Konsole nicht
/// auseinanderlaufen — frueher war die Benchmark-Ausgabe reines `println!`
/// und fuer [`crate::tui`] unsichtbar.
macro_rules! bench_say {
    ($level:expr, $brain:expr, $($arg:tt)*) => {{
        let text = format!($($arg)*);
        crate::bench_events::print_line(&format!("[benchmark] {text}"));
        crate::bench_events::emit($level, $brain, &text);
    }};
}
pub(crate) use bench_say;

/// Wall-Timeout je Brain-Run in Sekunden (ein Benchmark-Run darf nicht ewig
/// laufen — via `AgentController::set_wall_timeout_secs`).
///
/// 300 s reichten nicht: kimi (Ein-Brain-Lauf, 2026-08-01) brauchte allein fürs
/// gezielte Erkunden der Zieldatei (Gliederung, Select-String, 5 Shell-Steps)
/// über die Deadline und kam nie zum Editieren. 900 s lassen ein komplettes
/// Brain-Zyklus (Erkunden + Edit + cargo build + cargo test) zu.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
const BENCH_WALL_SECS: u64 = 900;
/// Grosszuegiger Circuit-Breaker je Brain-Run. Die eigentliche Zeitgrenze ist
/// die Wall-Deadline; unterschiedliche sinnvolle Arbeitszyklen sollen nicht an
/// einer starren 15-Turn-Choreografie scheitern.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
const BENCH_MAX_CYCLES: usize = 40;

/// Größe der gerankten Top-Liste in Phase A (nur Platz 1 wird zur Aufgabe).
const VOTE_TOP_K: usize = 10;
/// Zeichen-Cap der Projektfakten im Sammel-Prompt (wie autoresearch-self).
const FACTS_MAX_CHARS: usize = 1200;
/// Zwei komplette Runden ohne einen bestandenen Kandidaten sind ein
/// Fehlermuster, kein Erkenntnisgewinn. Der Supervisor beendet dann den
/// Benchmark statt Kontingent mit derselben Sackgasse zu verbrennen.
const MAX_CONSECUTIVE_UNPRODUCTIVE_ROUNDS: usize = 2;
/// Wartezeit, wenn in einer ganzen Runde kein einziges Brain erreichbar war.
/// Etwas kuerzer als das Circuit-Breaker-Fenster (900s), damit der Dauerlauf
/// zuegig weitermacht, sobald ein Anbieter wieder aufmacht — aber lang genug,
/// dass er nicht heiss gegen die Sperre laeuft.
const OUTAGE_COOLDOWN_SECS: u64 = 300;

/// Ein Brain baut die Aufgabe über den normalen Controller-Pfad (mit Wall-Timeout
/// und grosszuegigem Cycle-Circuit-Breaker). Liefert `(status, cycles)`.
#[cfg(feature = "webview")]
fn bench_run(
    brain_id: &str,
    task: &str,
    resume_id: Option<&str>,
    workdir: &std::path::Path,
    headless: bool,
    note: Option<crate::StageNote>,
    verbose: bool,
) -> Result<(String, u32, String), String> {
    use crate::browser::WebBrainBackend;
    use crate::controller::{AgentController, RunOptions};
    use crate::executor::PlatformShellExecutor;

    let backend = WebBrainBackend::from_config(brain_id)?;
    let executor = PlatformShellExecutor::new_in(workdir);
    let mut controller = AgentController::with_data_dir(
        backend,
        executor,
        BENCH_MAX_CYCLES,
        crate::config::data_dir(),
    );
    controller.set_workspace_root(workdir.to_path_buf());
    controller.set_wall_timeout_secs(BENCH_WALL_SECS);
    // Ohne --verbose wandern die Schritt-Zeilen IN die Timer-Zeile, statt sie
    // zu zerschneiden; mit --verbose laeuft beides nebeneinander.
    if let Some(n) = note {
        controller.set_progress(n, !verbose);
    }
    let options = RunOptions {
        suppress_memory_context: true,
        ..RunOptions::default()
    };
    let meta = if let Some(run_id) = resume_id {
        controller.continue_run(run_id, task, brain_id, headless, options)?
    } else {
        controller.run_with_options(task, brain_id, None, headless, options)?
    };
    Ok((meta.status, meta.cycles, meta.run_id))
}

#[cfg(not(feature = "webview"))]
fn bench_run(
    _brain_id: &str,
    _task: &str,
    _resume_id: Option<&str>,
    _workdir: &std::path::Path,
    _headless: bool,
    _note: Option<crate::StageNote>,
    _verbose: bool,
) -> Result<(String, u32, String), String> {
    Err("webview-Feature nicht aktiv — kein Brain-Backend verfügbar".to_string())
}

/// Übersetzt EINEN gevoteten Vorschlag in eine konkrete, bounded Coding-Aufgabe
/// (Phase A.5). Zwei Versuche: verlangt die Aufgabe etwas bereits Vorhandenes,
/// wäre der Bauauftrag wertlos ("ist schon implementiert" → keine Änderung →
/// fälschlich FAIL). Fällt die Verfeinerung aus, wird der unbelegte Vorschlag
/// verworfen statt als teurer Bauauftrag durchgereicht.
fn refine_one<Q>(
    winner: &str,
    facts: &str,
    refiner: &str,
    existing_api: &[String],
    src_files: &[String],
    root: &std::path::Path,
    query: &Q,
) -> String
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    if refiner.is_empty() {
        return String::new();
    }
    for attempt in 1..=2 {
        let mut prompt = build_refine_prompt(winner, facts, src_files);
        // Anker-Grundlage: der Refiner soll die echten Signaturen der
        // Sieger-Zieldatei SEHEN. Sonst verankert er die Lokalen Belege in
        // halluzinierten Symbolen (real beobachtet 2026-08-15, 12/12
        // Ablehnungen: `probe_brain`, `BrowserBackend`, `RiskLevel` — keins
        // davon existiert). Die Gliederung ist dieselbe, mit der auch der
        // Bauauftrag die Brains an echte Code-Realitaet bindet.
        if let Some(rel0) = suggestion_target(winner).or_else(|| target_file_of(winner)) {
            let rel = if rel0.starts_with("src/") {
                rel0
            } else {
                format!("src/{rel0}")
            };
            if let Some(gliederung) = file_outline(&root.join(&rel), 120) {
                prompt = format!(
                    "{prompt}\n\nGLIEDERUNG DER ZIELDATEI {rel} (bestehende Signaturen mit \
                     Zeilennummern). Nutze als Anker und in den Lokalen Belegen NUR Symbole, \
                     die in dieser Gliederung vorkommen — erfinde keine.\n{gliederung}"
                );
            }
        }
        if attempt > 1 {
            prompt.push_str(
                "

WICHTIG: Dein vorheriger Vorschlag wurde abgelehnt — er verlangte etwas, das es BEREITS GIBT, nannte eine Zieldatei, die es NICHT gibt, ordnete ein vorhandenes Symbol der falschen Datei zu oder behauptete in den Lokalen Belegen ein bestehendes Symbol, das NIRGENDS im Quelltext existiert. Schlage etwas anderes vor, dessen Zieldatei und bestehende Symbole nachweislich zusammenpassen.",
            );
        }
        match query(refiner, &prompt) {
            Ok(text) => match usable_refinement(&text) {
                Some(t) if task_is_redundant(&t, existing_api) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: verlangt bereits vorhandene Funktion ({:?}) — {}",
                        proposed_fn_name(&t).unwrap_or_default(),
                        crate::char_prefix(&t, 160)
                    );
                    continue;
                }
                Some(t) if task_targets_missing_file(&t, src_files) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: Zieldatei {:?} ist nicht in der erlaubten Modulliste — {}",
                        target_file_of(&t).unwrap_or_default(),
                        crate::char_prefix(&t, 160)
                    );
                    continue;
                }
                Some(t) if task_is_misdirected(&t, root) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: vorhandenes Symbol steht nicht in Zieldatei {:?} [{}] — {}",
                        target_file_of(&t).unwrap_or_default(),
                        misdirection_detail(&t, root),
                        crate::char_prefix(&t, 160)
                    );
                    continue;
                }
                Some(t) if task_has_phantom_anchors(&t, root) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: Lokale Belege nennen Phantom-Symbole [{}] — {}",
                        phantom_detail(&t, root),
                        crate::char_prefix(&t, 160)
                    );
                    continue;
                }
                Some(t) if !refinement_has_evidence(&t) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: keine lokalen Belege oder kein automatisierbarer Abschluss — {}",
                        crate::char_prefix(&t, 160)
                    );
                    continue;
                }
                Some(t) => return t,
                None => break,
            },
            Err(e) => {
                bench_say!(
                    crate::bench_events::Level::Fail,
                    None,
                    "  Verfeinerung fehlgeschlagen ({e})."
                );
                break;
            }
        }
    }
    String::new()
}

/// Fährt den vollen Benchmark: `query` speist Phase A (Swarm-Abstimmung, in
/// CLI/REPL `repl::isolated_query`). Der Live-Teil (Phase B) läuft über den
/// Controller; getestet wird er e2e vom Orchestrator, nicht im Unit-Test.
pub fn run_benchmark<Q>(config: &BenchmarkConfig, query: Q) -> Result<BenchmarkReport, String>
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    // Sicherheitsmodell §5: nur auf sauberem Git-Tree starten.
    crate::autoresearch::guard_clean_tree(&config.workdir)?;

    // Mit `--verbose` spiegeln die Schritt-Zeilen des Controllers
    // (shell/edit/write/message) in den Ereignisbus — in der TUI wird der
    // "Ereignisstrom" sonst von den unterdrueckten stdout-Zeilen nichts sehen.
    crate::bench_events::set_echo_bus(config.verbose);

    let rounds = config.rounds.max(1);
    let mut repair_focus: Option<String> = None;
    let mut candidate_backlog = std::collections::VecDeque::<String>::new();
    let mut attempted_candidates = std::collections::HashSet::<String>::new();
    let mut winners: Vec<(usize, String)> = Vec::new();
    let mut harvested: Vec<(String, String)> = Vec::new();
    let mut unproductive_rounds = 0usize;

    let mut round = 0usize;
    loop {
        round += 1;
        if !config.loop_forever && round > rounds {
            break;
        }

        let total = if config.loop_forever {
            "∞".to_owned()
        } else {
            rounds.to_string()
        };
        // Gesperrte Brains gehoeren nicht in die Runde.
        //
        // Bisher liefen sie in jeder Sammel-, Abstimm- und Planungswelle mit und
        // zogen jede einzelne bis in ihren Timeout — bei drei gesperrten von
        // acht war das der groesste Zeitposten der Runde, ohne einen einzigen
        // verwertbaren Beitrag. Die Sperre ist ohnehin schon bekannt
        // (`circuit_breaker::check`), sie wurde nur nicht benutzt, um die
        // Teilnehmerliste zu bilden.
        let (round_brains, gesperrt): (Vec<String>, Vec<(String, i64)>) = {
            let mut aktiv = Vec::new();
            let mut blockiert = Vec::new();
            // Progressive Teamgroesse: bei `--loop` waechst das Feld pro Runde
            // um genau ein Brain (1, 2, ... bis zur vollen Liste), dann beginnt
            // die Welle von vorn — die Kachelwand faellt auf die erste Brain
            // zurueck und startet erneut. Bei festen Runden bleibt das volle
            // Team. Vorgabe Storax 08.08.2026.
            let team_size = if config.loop_forever {
                if config.brains.is_empty() {
                    0
                } else {
                    ((round - 1) % config.brains.len()) + 1
                }
            } else {
                config.brains.len()
            };
            for b in config.brains.iter().take(team_size) {
                match crate::circuit_breaker::check(b) {
                    Some(rest) => blockiert.push((b.clone(), rest)),
                    None => aktiv.push(b.clone()),
                }
            }
            (aktiv, blockiert)
        };
        if !gesperrt.is_empty() {
            let liste = gesperrt
                .iter()
                .map(|(b, rest)| format!("{b} ({rest}s)"))
                .collect::<Vec<_>>()
                .join(", ");
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: {} von {} Brains gesperrt und ausgeklammert: {liste}",
                gesperrt.len(),
                config.brains.len()
            );
        }
        if round_brains.is_empty() {
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: alle Brains gesperrt — warte {OUTAGE_COOLDOWN_SECS}s."
            );
            std::thread::sleep(std::time::Duration::from_secs(OUTAGE_COOLDOWN_SECS));
            continue;
        }
        // Aussagekraft der Runde: wie viele Brains standen im Feld? Ein Ergebnis
        // aus einem Feld von zwei sagt weniger als eines aus einem Feld von
        // acht — das wird an jedem Messpunkt mitgeschrieben, statt es spaeter
        // rekonstruieren zu muessen.
        let field_size = round_brains.len();

        bench_say!(
            crate::bench_events::Level::Info,
            None,
            "runde {round}/{total} — abstimmen… ({field_size} von {} Brains im Feld)",
            config.brains.len()
        );
        // Projektfakten sind lokal und günstig; nach einem Harvest müssen sie
        // aktuell sein, ohne dafür die Brains erneut befragen zu müssen.
        let facts = crate::self_research::gather_facts(&config.workdir, FACTS_MAX_CHARS);
        let veto_context = if config.vetoes.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nMENSCHLICHE VETOS: Vorschläge mit diesen Begriffen dürfen nicht gewählt oder geplant werden: {}.",
                config.vetoes.join(", ")
            )
        };
        let round_facts = match &repair_focus {
            Some(focus) => format!("{facts}\n\n{focus}"),
            None => facts.clone(),
        } + &veto_context;
        if repair_focus.is_some() {
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: Reparaturfokus aus gescheiterten Gates wird priorisiert."
            );
        }
        // Ideen werden als Arbeitsvorrat gesammelt. Eine neue Schwarmrecherche
        // ist nur nötig, wenn der Vorrat leer ist oder ein fehlgeschlagenes Gate
        // gezielt eine Reparaturpriorität erzeugt hat.
        if candidate_backlog.is_empty() || repair_focus.is_some() {
            let report = crate::self_research::run_self_research(
                &round_brains,
                &round_facts,
                config.suggestions,
                VOTE_TOP_K,
                config.parallel,
                &query,
            );
            let ranked = ranked_from_report(&report).into_iter().filter(|candidate| {
                let lower = candidate.to_lowercase();
                !attempted_candidates.contains(candidate)
                    && !config
                        .vetoes
                        .iter()
                        .any(|veto| !veto.trim().is_empty() && lower.contains(&veto.to_lowercase()))
            });
            for candidate in ranked {
                candidate_backlog.push_back(candidate);
            }
            bench_say!(
                crate::bench_events::Level::Info,
                None,
                "Arbeitsvorrat aktualisiert: {} offene Vorschläge.",
                candidate_backlog.len()
            );
        } else {
            bench_say!(
                crate::bench_events::Level::Info,
                None,
                "Arbeitsvorrat: {} offene Vorschläge — keine neue Sammlung nötig.",
                candidate_backlog.len()
            );
        }
        let Some(winner) = candidate_backlog.pop_front() else {
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: Arbeitsvorrat leer — keine verwertbaren Vorschläge."
            );
            continue;
        };
        attempted_candidates.insert(winner.clone());
        bench_say!(crate::bench_events::Level::Pass, None, "Sieger: {winner}");

        let existing_api = crate::self_research::collect_public_api(&config.workdir.join("src"));
        let src_files: Vec<String> =
            crate::self_research::collect_modules(&config.workdir.join("src"))
                .into_iter()
                .map(|(name, _lines)| format!("src/{name}"))
                .collect();
        let refiner = round_brains.first().cloned().unwrap_or_default();
        // Pre-Flight: der Sieger muss sich als EIN belegtes Work-Package
        // uebersetzen lassen, BEVOR der teure Plan-Konsens laeuft. Ein Sieger mit
        // Phantom-Anker (behauptetes Symbol existiert nicht oder steht woanders)
        // verbrennt sonst die gesamte Planungsphase: am 2026-08-14 hat der Schwarm
        // einen shell_policy-Test um das nicht existente `resolve_symlink_escape`
        // gewaehlt — alle 9 Verfeinerungen wurden verworfen, die Runde war verloren.
        // Die validierte Spezifikation bleibt als `preflight_spec` erhalten: schlaegt
        // die Verfeinerung des Konsensplans fehl, ersetzt sie den Plan als Auftrag,
        // statt die ganze Runde wegzuwerfen (real beobachtet 2026-08-14, Runde 2+4).
        let preflight_spec: String = if !refiner.is_empty() {
            let preflight = refine_one(
                &winner,
                &round_facts,
                &refiner,
                &existing_api,
                &src_files,
                &config.workdir,
                &query,
            );
            if preflight.is_empty() {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "Sieger ohne belegtes Work-Package verworfen (Phantom-Anker oder Redundanz) — naechster Kandidat aus dem Vorrat."
                );
                continue;
            }
            preflight
        } else {
            String::new()
        };
        winners.push((round, winner.clone()));

        // Phase A.5: Alle Brains planen den Sieger; der Konsensplan wird erst
        // danach zum verbindlichen Bauauftrag. Ein einzelner Erst-Refiner wäre
        // hier ein unnötiger, unbeobachteter Engpass.
        let plan_context = format!(
            "Projektfakten:\n{}\n\nErlaubte Zieldateien: {}\n\nBestehende öffentliche APIs: {}\n\nDer Plan muss genau eine kleine Änderung beschreiben.",
            crate::char_prefix(&round_facts, 1200), src_files.join(", "), existing_api.join(", ")
        );
        bench_say!(
            crate::bench_events::Level::Info,
            None,
            "Sieger wird gemeinschaftlich geplant…"
        );
        let plan_vote = crate::design_vote::run_design_vote(
            &crate::design_vote::DesignVoteConfig {
                brains: round_brains.clone(),
                topic: winner.clone(),
                context: plan_context,
                mode: crate::design_vote::VoteMode::ImplementationPlan,
            },
            // `bench_say!` statt `bench_events::emit`: `emit` schreibt NUR in den
            // Ringpuffer, den allein die TUI liest. Im CLI-Dauerlauf ging der
            // gesamte Fortschritt dieser Phase damit an niemanden — am
            // 30.07.2026 stand die Planung 57 Minuten ohne ein einziges
            // Lebenszeichen da und sah wie ein Haenger aus.
            &|msg| {
                bench_say!(
                    crate::bench_events::Level::Progress,
                    None,
                    "Plan-Konsens: {msg}"
                )
            },
            |b, p| query(b, p),
        );
        // Seit `design_vote` den Turniersieger notfalls selbst uebernimmt, bleibt
        // hier nur noch ein Fall uebrig: es kam ueberhaupt kein Entwurf zurueck,
        // weil kein Brain erreichbar war. Das ist eine Verfuegbarkeitsstoerung
        // wie jede andere — kein Grund, den Dauerlauf zu beenden.
        let Some(consensus_plan) = plan_vote.approved_text().map(str::to_owned) else {
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: kein einziger Planentwurf — kein Brain erreichbar. \
                 Warte {}s und versuche es erneut.",
                OUTAGE_COOLDOWN_SECS
            );
            std::thread::sleep(std::time::Duration::from_secs(OUTAGE_COOLDOWN_SECS));
            continue;
        };
        bench_say!(
            crate::bench_events::Level::Pass,
            None,
            "Plan-Konsens: {}",
            crate::char_prefix(&consensus_plan, 140)
        );

        // Treueprobe: Der Konsensplan muss den Zieldatei-Anker des Siegers nennen.
        // Real beobachtet 2026-08-14: Der Sieger handelte von browser/blocking.rs,
        // der Endplan des Leaders war ein nichtssagender Abschlussbericht ohne die
        // Datei — der Refiner hat daraus eigenmaechtig einen repl/commands.rs-Auftrag
        // gebaut und alle Brains bauten gegen die falsche Datei.
        let winner_target = crate::benchmark::tasks::suggestion_target(&winner);
        if let Some(target) = &winner_target {
            let base = target
                .rsplit('/')
                .next()
                .unwrap_or(target)
                .to_string();
            if !consensus_plan.contains(target) && !consensus_plan.contains(&base) {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "Plan-Konsens nennt den Sieger-Anker '{target}' nicht — der Plan \
                     driftete ab. Runde verworfen, naechster Kandidat aus dem Vorrat."
                );
                continue;
            }
        }

        // Turnier statt Mischmasch: jedes Brain bearbeitet exakt den gewählten
        // Sieger. Damit misst der Score die Qualität der Umsetzung und nicht,
        // ob ein zufällig zugeteilter Neben-Vorschlag leichter war.
        let assignments = assign_tasks(&round_brains, std::slice::from_ref(&winner), round);

        // Phase A.5 — jede zugeteilte Aufgabe konkretisieren. Gleiche Vorschlaege
        // nur einmal verfeinern (bei weniger Vorschlaegen als Brains).
        let mut refined_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut plan: Vec<(String, String)> = Vec::new();
        for (brain, raw) in &assignments {
            let eff = match refined_cache.get(raw) {
                Some(t) => t.clone(),
                None => {
                    // Auch ein einzelnes Brain muss seinen Entwurf als lokal
                    // belegtes Work-Package konkretisieren. Der fruehere
                    // Shortcut liess erfundene APIs direkt in den Baulauf.
                    let t =
                        crate::StageTimer::start(format!("verfeinern fuer {brain} via {refiner}"));
                    let refined = refine_one(
                        &consensus_plan,
                        &round_facts,
                        &refiner,
                        &existing_api,
                        &src_files,
                        &config.workdir,
                        &query,
                    );
                    let refined = if refined.is_empty() && !preflight_spec.is_empty() {
                        bench_say!(
                            crate::bench_events::Level::Info,
                            None,
                            "Plan-Refinement leer — validierte Pre-Flight-Spezifikation des Siegers als Auftrag uebernommen."
                        );
                        preflight_spec.clone()
                    } else {
                        refined
                    };
                    t.finish(crate::char_prefix(&refined, 90));
                    refined_cache.insert(raw.clone(), refined.clone());
                    refined
                }
            };
            if eff.is_empty() {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain),
                    "{brain}: Vorschlag ohne belegtes Work-Package verworfen"
                );
                continue;
            }
            // Treueprobe auf dem verfeinerten Auftrag: Der Refiner muss beim
            // Zieldatei-Anker des Siegers bleiben. Driftet er ab, ist das
            // Work-Package kein legitimer Ausbau des Siegers und der Baulauf
            // wuerde die falsche Datei treffen.
            if let (Some(wt), Some(et)) = (
                &winner_target,
                crate::benchmark::tasks::target_file_of(&eff),
            ) {
                if !crate::benchmark::tasks::same_target(wt, &et) {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: verfeinerter Auftrag driftete auf {et} statt {wt} ab — uebersprungen"
                    );
                    continue;
                }
            }
            bench_say!(
                crate::bench_events::Level::Info,
                Some(brain),
                "{brain} -> {}",
                crate::char_prefix(&eff, 120)
            );
            plan.push((brain.clone(), eff));
        }

        // Baseline-Testzahl auf sauberem Tree: ein PASS muss die Testzahl
        // ERHOEHEN, sonst zaehlt eine verwaiste (nicht eingebundene) Datei als
        // Erfolg — real beobachtet 2026-07-21.
        let baseline_tests = {
            let t = crate::StageTimer::start("Baseline-Tests".to_string());
            let (ok, out) = run_eval_detail(&config.test_eval, &config.workdir);
            match super::baseline_test_count(ok, &out) {
                Ok(n) => {
                    t.finish(&format!("Baseline: {n} Tests"));
                    n
                }
                Err(e) => {
                    t.finish("Baseline ROT — Benchmark abgebrochen");
                    return Err(e);
                }
            }
        };

        // Phase B — Arbeitsschlange statt fester Liste: bleibt ein Brain stecken,
        // wandert SEINE Aufgabe an ein Brain, das sie noch nicht versucht hat.
        let mut harvest_pool: Vec<HarvestCandidate> = Vec::new();
        let mut round_failures: Vec<String> = Vec::new();
        // Wie viele Brains in dieser Runde ueberhaupt zum Messen kamen (also
        // NICHT extern blockiert waren). Siehe Auswertung am Rundenende.
        let mut round_attempted = 0usize;
        let mut hq = HandoffQueue::new(&plan, &round_brains, config.max_handoffs);
        while let Some((brain_owned, effective_owned, handoff_from, handoff_context)) = hq.next() {
            let brain = &brain_owned;
            let effective = &effective_owned;

            if let Some(prev) = &handoff_from {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain),
                    "{brain} uebernimmt die Aufgabe von {prev}."
                );
            }
            let task = crate::benchmark::tasks::build_task_prompt_for_brain_in(
                effective,
                &config.workdir,
                brain,
                handoff_context.as_deref(),
            );
            let tid = task_id(effective);
            crate::autoresearch::guard_clean_tree(&config.workdir)
                .map_err(|e| format!("{e} (vor dem Run von {brain})"))?;
            let baseline = crate::autoresearch::git_head_sha(&config.workdir)?;
            let started = Instant::now();

            // Repair-Loop: schlägt Build/Test fehl, geht die echte Fehlerausgabe
            // als Kontext zurück ans Brain (bis zu max_iterations). Zwischen den
            // Iterationen wird NICHT resettet — das Brain repariert sein eigenes
            // Werk, wie ein echter Coding-Agent am Compilerfehler.
            let max_iter = config.max_iterations.max(1);
            let stall_limit = config.stall_limit.max(1);
            let mut attempt_task = task.clone();
            let mut run_id: Option<String> = None;
            let mut cycles = 0u32;
            let mut did_change = false;
            let mut compiled = false;
            let mut tests_passed = false;
            let mut iterations = 0u32;
            // Fortschritts-Gedaechtnis: bestes bisher erreichtes Stadium und wie
            // viele Iterationen seither nichts besser wurde.
            let mut best: Option<Progress> = None;
            let mut stalls = 0u32;
            let mut stalled = false;
            // Externe Blockade (Anbieter-Limit, Oberflaeche liefert nichts):
            // zaehlt NICHT als Kompetenz-Fehlschlag.
            let mut unavailable = false;
            let mut protocol_fault = false;
            let mut last_gate_failure: Option<String> = None;

            for iter in 1..=max_iter {
                iterations = iter;
                let t = crate::StageTimer::start(format!(
                    "{brain} Iteration {iter}/{max_iter}: Brain baut"
                ));
                let mut terminal_status: Option<String> = None;
                match bench_run(
                    brain,
                    &attempt_task,
                    run_id.as_deref(),
                    &config.workdir,
                    config.headless,
                    Some(t.note_handle()),
                    config.verbose,
                ) {
                    Ok((status, c, continued_run_id)) => {
                        // `continue_run` liefert kumulative Zyklen derselben
                        // Agent-Session. Nicht über Iterationen doppelt zählen.
                        cycles = c;
                        run_id = Some(continued_run_id);
                        if is_external_block(&status) {
                            unavailable = true;
                            t.finish("Brain nicht verfuegbar (extern)");
                            break;
                        }
                        if is_nonretryable_run_fault(&status) {
                            protocol_fault = is_protocol_fault(&status);
                            t.finish("Terminaler Run-Status — vorhandenen Diff prüfen");
                            terminal_status = Some(status);
                        } else {
                            t.finish(&format!("Brain fertig ({c} Zyklen)"));
                        }
                    }
                    Err(e) => {
                        last_gate_failure = Some(format!("Runner-Fehler bei {brain}: {e}"));
                        t.finish("Brain-Run fehlgeschlagen");
                        bench_say!(
                            crate::bench_events::Level::Fail,
                            Some(brain),
                            "{brain}: run fehlgeschlagen — {e}"
                        );
                    }
                }

                did_change = tree_changed(&config.workdir);
                if let Some(status) = terminal_status {
                    if super::terminal_status_blocks_evaluation(&status, did_change) {
                        stalled = true;
                        bench_say!(
                            crate::bench_events::Level::Fail,
                            Some(brain),
                            "{brain}: terminaler Run-Status `{status}` ohne Diff — keine weiteren Retries für diese Aufgabe."
                        );
                        break;
                    }
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: Run endete mit `{status}`, hat aber Code geaendert — Diff wird trotzdem gebaut und getestet."
                    );
                }
                if !did_change {
                    // Beleg zum Urteil, nicht nur das Urteil.
                    //
                    // Am 12.08.2026 meldete der Executor fuer gemini drei
                    // erfolgreiche Edits mit Zeilenzahlen ("edit ok:
                    // src/controller.rs — Ersetzung ab Zeile 22. Datei jetzt
                    // 2118 Zeilen"), und diese Messung sagte trotzdem "keine
                    // Aenderung". Dasselbe Symptom ist fuer den 30.07.2026
                    // dokumentiert; die damalige Haertung von `tree_changed`
                    // gegen Git-Fehler hat die Ursache also nicht getroffen.
                    //
                    // Aus dem Log war der Fall nicht aufzuklaeren, weil zum
                    // Urteil kein Zustand mitgeschrieben wurde. Ohne
                    // Arbeitsverzeichnis und HEAD laesst sich nicht einmal
                    // unterscheiden, ob in einem anderen Baum gemessen wurde,
                    // ob zwischendurch zurueckgesetzt wurde oder ob die
                    // Aenderung nie ankam. Drei Werte, einmal pro Fall — das
                    // macht die naechste Wiederholung diagnostizierbar.
                    let head_jetzt = crate::autoresearch::git_head_sha(&config.workdir)
                        .unwrap_or_else(|e| format!("<unlesbar: {e}>"));
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: keine Aenderung messbar — workdir={}, HEAD={}, baseline={}{}",
                        config.workdir.display(),
                        crate::char_prefix(&head_jetzt, 12),
                        crate::char_prefix(&baseline, 12),
                        if head_jetzt.trim() == baseline.trim() {
                            ""
                        } else {
                            " (HEAD ist vom Baseline abgewichen!)"
                        }
                    );
                    // Keine Änderung. Frueher hiess das sofort Abbruch — dabei
                    // haben claude und kimi (2026-07-21) den Code erkundet und
                    // dann „fertig" gemeldet, ohne je zu editieren (kimi sogar
                    // per message-Action „Implementierung erfolgreich"). Ein
                    // einziger Anstoss kann so ein Brain ueber die Linie bringen:
                    // nachschieben, dass ein Edit PFLICHT ist, und als Stillstand
                    // zaehlen (das stall_limit deckelt endloses Nicht-Editieren).
                    stalls += 1;
                    last_gate_failure = Some(format!(
                        "Kein-Edit-Gate bei {brain}: Aufgabe wurde nach Iteration {iter} nicht verändert."
                    ));
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: Iteration {iter}/{max_iter} — keine Änderung, Anstoss ({stalls}/{stall_limit})"
                    );
                    if stalls >= stall_limit || iter >= max_iter {
                        stalled = stalls >= stall_limit;
                        break;
                    }
                    attempt_task = build_no_change_prompt(&task);
                    continue;
                }
                let tb = crate::StageTimer::start(format!("{brain}: {}", config.build_eval));
                let (b_ok, b_out) = run_eval_detail(&config.build_eval, &config.workdir);
                tb.finish(if b_ok { "Build ok" } else { "Build ROT" });
                compiled = b_ok;
                if !compiled {
                    last_gate_failure = Some(format!(
                        "Build-Gate bei {brain}: {}",
                        crate::char_prefix(&b_out, 700)
                    ));
                    let now = Progress {
                        stage: 1,
                        errors: count_build_errors(&b_out),
                    };
                    if is_improvement(best, now) {
                        bench_say!(
                            crate::bench_events::Level::Progress,
                            Some(brain),
                            "{brain}: Iteration {iter}/{max_iter} — Build rot, aber naeher dran ({} Fehler)",
                            now.errors
                        );
                        best = Some(now);
                        stalls = 0;
                    } else {
                        stalls += 1;
                        bench_say!(
                            crate::bench_events::Level::Warn,
                            Some(brain),
                            "{brain}: Iteration {iter}/{max_iter} — Build rot, kein Fortschritt ({}/{stall_limit})",
                            stalls
                        );
                    }
                    if stalls >= stall_limit {
                        stalled = true;
                        break;
                    }
                    if iter < max_iter {
                        attempt_task = build_repair_prompt(&task, &config.build_eval, &b_out);
                        continue;
                    }
                    break;
                }
                let tt = crate::StageTimer::start(format!("{brain}: {}", config.test_eval));
                let (t_ok, t_out) = run_eval_detail(&config.test_eval, &config.workdir);
                let after = parse_test_count(&t_out).unwrap_or(0);
                tt.finish(&format!(
                    "Tests {} ({after} bestanden)",
                    if t_ok { "ok" } else { "ROT" }
                ));
                // Eine grüne task-spezifische Suite ist der Beleg. Eine höhere
                // Testzahl ist wertvoll, aber kein universelles Erfolgskriterium:
                // Bugfixes und Refactorings dürfen bestehende Tests reparieren.
                tests_passed = t_ok;
                if t_ok && after <= baseline_tests {
                    bench_say!(
                        crate::bench_events::Level::Info,
                        None,
                        "  Tests gruen; Testzahl unveraendert ({after} <= {baseline_tests})"
                    );
                }
                if tests_passed {
                    bench_say!(
                        crate::bench_events::Level::Pass,
                        Some(brain),
                        "{brain}: Iteration {iter}/{max_iter} — grün"
                    );
                    break;
                }
                last_gate_failure = Some(format!(
                    "Test-Gate bei {brain}: {}",
                    crate::char_prefix(&t_out, 700)
                ));
                let now = progress_after_tests(&t_out);
                if is_improvement(best, now) {
                    bench_say!(
                        crate::bench_events::Level::Progress,
                        Some(brain),
                        "{brain}: Iteration {iter}/{max_iter} — Tests rot, aber naeher dran ({} rot)",
                        now.errors
                    );
                    best = Some(now);
                    stalls = 0;
                } else {
                    stalls += 1;
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: Iteration {iter}/{max_iter} — Tests rot, kein Fortschritt ({stalls}/{stall_limit})"
                    );
                }
                if stalls >= stall_limit {
                    stalled = true;
                    break;
                }
                if iter < max_iter {
                    attempt_task = build_repair_prompt(&task, &config.test_eval, &t_out);
                }
            }
            if stalled {
                // Im Turnier hat jedes Brain den Sieger bereits als eigenen
                // Plan-Eintrag. Ein zusätzlicher Handoff würde denselben
                // Kandidaten doppelt starten; deshalb nur protokollieren und
                // den nächsten regulär geplanten Kandidaten abwarten.
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain),
                    "{brain}: {} — nächster regulärer Kandidat übernimmt einmalig.",
                    if protocol_fault {
                        "Protokollfehler"
                    } else {
                        "terminaler Fehler oder kein Fortschritt"
                    }
                );
            }
            let latency_ms = started.elapsed().as_millis() as u64;

            if unavailable {
                // Kein CodeEvent: ein ausgesperrtes Brain ist kein schlechtes
                // Brain. Wuerde es als Fehlschlag zaehlen, saenke der Score mit
                // der Anbieter-Auslastung statt mit der Faehigkeit.
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain),
                    "{brain}: extern blockiert — nicht gewertet (kein Messpunkt)."
                );
                reset_repo(&config.workdir, &baseline)?;
                continue;
            }

            round_attempted += 1;

            if !is_pass(did_change, compiled, tests_passed) {
                if let Some(failure) = last_gate_failure.as_deref() {
                    round_failures.push(failure.to_string());
                }
            }

            // Der Diff wird für JEDEN echten Edit geprüft, nicht erst für
            // grüne Kandidaten. So erscheinen Scope-Verstöße auch dann im
            // Leistungsindex, wenn der Build ohnehin rot ist.
            let patch_scope = if did_change {
                match capture_patch(&config.workdir) {
                    Ok(patch) if !patch.trim().is_empty() => Some((patch, None)),
                    Ok(_) => None,
                    Err(e) => Some((String::new(), Some(e))),
                }
            } else {
                None
            };
            let scope_error = patch_scope.as_ref().and_then(|(patch, capture_error)| {
                capture_error
                    .clone()
                    .or_else(|| validate_task_scope(patch, effective).err())
            });
            let scope_lint_ok =
                if scope_error.is_some() && is_pass(did_change, compiled, tests_passed) {
                    if config.lint_eval.trim().is_empty() {
                        true
                    } else {
                        let (ok, _) = run_eval_detail(&config.lint_eval, &config.workdir);
                        ok
                    }
                } else {
                    false
                };

            let event = CodeEvent {
                brain_id: brain.clone(),
                task_id: tid.clone(),
                did_change,
                compiled,
                tests_passed,
                cycles,
                iterations,
                latency_ms,
                handoff_from: handoff_from.clone(),
                stalled,
                field_size,
                ts: crate::now_rfc3339(),
            };
            crate::code_score::record(&event);
            if let Some(error) = &scope_error {
                // Erst der Malus: Scope-Verstöße sind ein Risikosignal und
                // dürfen nicht in der normalen technischen Messung verschwinden.
                let mut policy_event = event.clone();
                policy_event.task_id = format!("{}:scope-violation", tid);
                policy_event.did_change = false;
                policy_event.compiled = false;
                policy_event.tests_passed = false;
                crate::code_score::record(&policy_event);
                let compensation = scope_compensation_count(
                    is_pass(did_change, compiled, tests_passed),
                    scope_lint_ok,
                );
                for n in 0..compensation {
                    let mut benefit_event = event.clone();
                    benefit_event.task_id = format!("{}:scope-benefit-{}", tid, n + 1);
                    crate::code_score::record(&benefit_event);
                }
                if compensation > 0 {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: Scope-Verstoss mit {compensation}x Qualitätsausgleich bewertet — {error}"
                    );
                } else {
                    bench_say!(
                        crate::bench_events::Level::Fail,
                        Some(brain),
                        "{brain}: Scope-Verstoss mit Malus bewertet (kein nachgewiesener Qualitätsausgleich) — {error}"
                    );
                }
            }
            if is_pass(did_change, compiled, tests_passed) {
                if let Some(prev) = &handoff_from {
                    bench_say!(
                        crate::bench_events::Level::Pass,
                        Some(brain),
                        "RETTUNG: {brain} loest, woran {prev} gescheitert ist."
                    );
                }
            }

            bench_say!(
                if is_pass(did_change, compiled, tests_passed) {
                    crate::bench_events::Level::Pass
                } else {
                    crate::bench_events::Level::Fail
                },
                Some(brain),
                "{brain}: {iterations} Iteration(en), did_change={} build={} test={} -> {}",
                yes_no(did_change),
                ok_x(compiled),
                ok_x(tests_passed),
                outcome_label(did_change, compiled, tests_passed)
            );

            if stalled {
                let context = Some(format!(
                    "Vorarbeit: Brain {brain} blieb bei '{effective}' stehen mit {}.",
                    last_gate_failure.as_deref().unwrap_or("unbekanntem Gate")
                ));
                match hq.on_stall(brain, effective, context) {
                    Some(next_brain) => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: Aufgabe an {next_brain} weitergereicht."
                    ),
                    None => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: keine weiteren Brains für diese Aufgabe verfügbar."
                    ),
                }
            }

            // Bestandene Arbeit sichern, BEVOR der Reset sie verwirft.
            if config.harvest && is_pass(did_change, compiled, tests_passed) {
                match patch_scope {
                    Some((patch, None)) if crate::bench_harvest::has_substantive_change(&patch) => {
                        match validate_task_scope(&patch, effective) {
                            Ok(paths) => {
                                bench_say!(
                                crate::bench_events::Level::Pass,
                                Some(brain),
                                "  {brain}: Patch gesichert ({} Dateien, {} Zeilen) — Kandidat für die Ernte",
                                paths.len(), patch.lines().count()
                            );
                                // Crash-Sicherheit: der Patch liegt sofort auf Platte,
                                // damit ein Prozessabbruch vor der Ernte die Arbeit
                                // nicht verwirft (real beobachtet 2026-08-14 Runde 5).
                                match persist_candidate(
                                    brain,
                                    effective,
                                    &patch,
                                ) {
                                    Ok(path) => bench_say!(
                                        crate::bench_events::Level::Info,
                                        Some(brain),
                                        "  {brain}: Patch crash-sicher unter {} abgelegt",
                                        path.display()
                                    ),
                                    Err(e) => bench_say!(
                                        crate::bench_events::Level::Warn,
                                        Some(brain),
                                        "  {brain}: Patch-Persistenz fehlgeschlagen: {e}"
                                    ),
                                }
                                harvest_pool.push(HarvestCandidate {
                                    brain: brain.clone(),
                                    task: effective.clone(),
                                    patch,
                                    iterations,
                                    latency_ms,
                                });
                            }
                            Err(e) => bench_say!(
                                crate::bench_events::Level::Fail,
                                Some(brain),
                                "  {brain}: Patch ausserhalb des sicheren Rahmens — verworfen: {e}"
                            ),
                        }
                    }
                    Some((patch, None)) if !crate::bench_harvest::has_substantive_change(&patch) => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "  {brain}: leerer Patch — nichts zu ernten"
                    ),
                    Some((_patch, None)) => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "  {brain}: Patch konnte nicht validiert werden — nichts zu ernten"
                    ),
                    None => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "  {brain}: kein Patch — nichts zu ernten"
                    ),
                    Some((_patch, Some(e))) => {
                        bench_say!(
                            crate::bench_events::Level::Fail,
                            Some(brain),
                            "  {brain}: Patch-Sicherung fehlgeschlagen — {e}"
                        )
                    }
                }
            }

            // Reset: jedes Brain misst denselben Sieger unabhängig.
            reset_repo(&config.workdir, &baseline)?;
        }

        // Ernte: genau EIN Patch gewinnt das Turnier. Mehrere Kandidaten
        // hintereinander einzuspielen lässt eine Runde unvorhersehbar wachsen
        // und war die Ursache für fachfremde Nebenänderungen.
        let harvest_count_before = harvested.len();
        if config.harvest {
            if harvest_pool.is_empty() {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "Nichts zu ernten — kein Brain hat bestanden."
                );
            }
            if let Some(cand) = pick_harvest(&harvest_pool) {
                let t = crate::StageTimer::start(format!(
                    "Ernte: {} einspielen + nachpruefen",
                    cand.brain
                ));
                match harvest_commit(cand, &cand.task, config) {
                    Ok(()) => {
                        t.finish("geerntet und committet");
                        bench_say!(
                            crate::bench_events::Level::Pass,
                            Some(&cand.brain),
                            "GEERNTET: {} ({} Iteration(en)) — Code bleibt im Repo.",
                            cand.brain,
                            cand.iterations
                        );
                        harvested.push((cand.brain.clone(), cand.task.clone()));
                    }
                    Err(e) => {
                        t.finish("Ernte fehlgeschlagen");
                        bench_say!(
                            crate::bench_events::Level::Fail,
                            Some(&cand.brain),
                            "Ernte verworfen ({}): {e}",
                            cand.brain
                        );
                        let head = crate::autoresearch::git_head_sha(&config.workdir)?;
                        reset_repo(&config.workdir, &head)?;
                    }
                }
            }
        }

        // Harness-Gesundheit der Runde melden.
        //
        // `runs_report` konnte diesen Fall schon immer benennen ("das ist der
        // Harness, nicht das Brain") — es rief ihn nur niemand auf, weil er an
        // einem manuellen Unterbefehl hing. Deshalb blieb am 29.07.2026 ein
        // Leck von 145 verworfenen Brain-Turns (29 % aller Turns) einen ganzen
        // Tag unbemerkt. Eine Messung, die niemand sieht, ist keine Messung.
        {
            let runs_dir = crate::config::data_dir().join("runs");
            let letzte = crate::runs_report::recent_runs(&runs_dir, config.brains.len());
            let harness = letzte
                .iter()
                .filter(|(_, f)| {
                    crate::runs_report::classify_run(f)
                        == crate::runs_report::FailureClass::HarnessParseBug
                })
                .count();
            let p_err: usize = letzte.iter().map(|(_, f)| f.protocol_errors).sum();
            // Die Zuordnung muss stimmen, sonst ist der Melder selbst
            // irrefuehrend: am 30.07.2026 stand „0 mit erkennbarem Format — das
            // ist der Harness, nicht das Brain" im Log, obwohl die drei
            // Verwerfungen claudes begruendete Weigerung waren und mit dem
            // Harness nichts zu tun hatten.
            if harness > 0 {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "runde {round}: {p_err} verworfene Brain-Antwort(en), davon {harness} \
                     mit erkennbarem Format — DAS ist der Harness, nicht das Brain."
                );
            } else if p_err > 0 {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "runde {round}: {p_err} verworfene Brain-Antwort(en), keine davon mit \
                     erkennbarem Format — der Harness ist hier nicht die Ursache."
                );
            }
        }

        // Ein Harvest in dieser Runde setzt die Produktivitätsuhr zurück;
        // andernfalls beendet der autonome Loop sich nach zwei Sackgassen.
        if harvested.len() > harvest_count_before {
            unproductive_rounds = 0;
            repair_focus = None;
        } else {
            repair_focus = repair_focus_from_failures(&round_failures);
            if let Some(focus) = &repair_focus {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "Kein Harvest — Folgerunde wird als gezielter Reparaturauftrag geplant: {}",
                    crate::char_prefix(focus, 160)
                );
            }
            // Eine Runde, in der KEIN Brain zum Messen kam, ist eine
            // Verfuegbarkeitsstoerung — keine Sackgasse im Code. Sie darf das
            // Abbruchbudget nicht verbrauchen.
            //
            // Sonst beendet ein Anbieter-Limit den ganzen Nachtlauf: am
            // 29.07.2026 waren drei von acht Brains gleichzeitig gesperrt
            // (mistral/qwen Nachrichtenlimit, gemini Login). Zwei solche Runden
            // und `run_benchmark` gab einen Fehler zurueck — der Dauerlauf stand
            // still, obwohl am Code nie etwas gescheitert war. Die
            // Unterscheidung existiert bereits in `is_external_block` und wird
            // fuer den Score genauso gezogen: ein ausgesperrtes Brain ist kein
            // schlechtes Brain.
            if is_availability_outage(round_attempted, &round_failures) {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    None,
                    "runde {round}: kein Brain war erreichbar — Verfuegbarkeitsstoerung, \
                     zaehlt nicht als unproduktiv. Warte {}s auf Entsperrung.",
                    OUTAGE_COOLDOWN_SECS
                );
                std::thread::sleep(std::time::Duration::from_secs(OUTAGE_COOLDOWN_SECS));
                continue;
            }

            unproductive_rounds += 1;
            if unproductive_rounds >= MAX_CONSECUTIVE_UNPRODUCTIVE_ROUNDS {
                return Err(format!(
                    "Benchmark nach {unproductive_rounds} unproduktiven Runden angehalten — kein Kandidat bestand Build/Test/Scope-Gates."
                ));
            }
        }
    }

    let board = crate::code_score::leaderboard();
    print_leaderboard(&board);

    // Ergebnis als Wiki-Seite ablegen (nachvollziehbar, WAS gebaut werden sollte).
    let wiki_slug = {
        let wiki = crate::wiki_memory::WikiMemory::new(
            crate::config::data_dir().join("memory").join("wiki"),
        );
        let title = format!("code-benchmark-{}", crate::now_run_stamp());
        let body = format_benchmark_report(&winners, &board);
        match wiki.write_page(&title, &body) {
            Ok(slug) => {
                bench_say!(
                    crate::bench_events::Level::Info,
                    None,
                    "Ergebnis abgelegt als [[{slug}]]."
                );
                Some(slug)
            }
            Err(e) => {
                bench_say!(
                    crate::bench_events::Level::Fail,
                    None,
                    "Wiki-Ablage fehlgeschlagen: {e}"
                );
                None
            }
        }
    };

    Ok(BenchmarkReport {
        winners,
        leaderboard: board,
        wiki_slug,
        harvested,
    })
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "ja"
    } else {
        "nein"
    }
}

fn ok_x(b: bool) -> &'static str {
    if b {
        "ok"
    } else {
        "x"
    }
}
