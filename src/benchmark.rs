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

use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::code_score::{CodeEvent, CodeStats};
use crate::self_research::SelfResearchReport;

/// Meldet eine Zeile gleichzeitig an Konsole und Ereignisstrom.
///
/// EIN Aufrufpunkt fuer beides, damit die TUI und die Konsole nicht
/// auseinanderlaufen — frueher war die Benchmark-Ausgabe reines `println!`
/// und fuer [`crate::tui`] unsichtbar.
macro_rules! bench_say {
    ($level:expr, $brain:expr, $($arg:tt)*) => {{
        let text = format!($($arg)*);
        println!("[benchmark] {text}");
        crate::bench_events::emit($level, $brain, &text);
    }};
}

/// Wall-Timeout je Brain-Run in Sekunden (ein Benchmark-Run darf nicht ewig
/// laufen — via `AgentController::set_wall_timeout_secs`).
const BENCH_WALL_SECS: u64 = 300;
/// Controller-Zyklen je Brain-Run: klein, damit das Brain fokussiert am Sieger
/// baut statt an einer offenen Aufgabe.
const BENCH_MAX_CYCLES: usize = 15;
/// Timeout je Eval-Kommando (`cargo build`/`cargo test`) in Sekunden.
const EVAL_TIMEOUT_SECS: u64 = 300;
/// Größe der gerankten Top-Liste in Phase A (nur Platz 1 wird zur Aufgabe).
const VOTE_TOP_K: usize = 10;
/// Zeichen-Cap der Projektfakten im Sammel-Prompt (wie autoresearch-self).
const FACTS_MAX_CHARS: usize = 1200;

/// Konfiguration eines Benchmark-Laufs.
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Zu bewertende Brains (leer ⇒ vom Aufrufer mit allen registrierten füllen).
    pub brains: Vec<String>,
    /// Wie oft der ganze Zyklus (Abstimmen → bauen) wiederholt wird.
    pub rounds: usize,
    /// Vorschläge je Brain in der Sammelphase (Phase A).
    pub suggestions: usize,
    /// Eval-Kommando „baut es?" (Default `cargo build --lib`).
    pub build_eval: String,
    /// Eval-Kommando „Tests grün?" (Default `cargo test --lib`).
    pub test_eval: String,
    /// Git-Repo-Root, in dem gebaut/gemessen wird.
    pub workdir: PathBuf,
    /// Headless-Browser für die Brain-Runs.
    pub headless: bool,
    /// Maximale Repair-Iterationen je Brain: schlägt Build/Test fehl, geht die
    /// Fehlerausgabe als Kontext zurück ans Brain (1 = kein Repair-Loop).
    pub max_iterations: u32,
    /// Ernte-Modus: der Code des besten bestandenen Brains wird nach der Runde
    /// wieder eingespielt und committet, statt verworfen zu werden.
    ///
    /// Ohne das ist der Benchmark ein reines Messgerät — deepseeks bestandene
    /// Läufe (2/4 PASS, 2026-07-21) landeten vollständig im `git reset --hard`.
    /// Die Messung bleibt unberührt: jedes Brain startet weiterhin auf
    /// identischer Baseline, geerntet wird erst NACH dem letzten Brain.
    pub harvest: bool,
    /// Ausklappen: die einzelnen Schritte (Shell-Kommandos, Datei-Aktionen,
    /// Brain-Antworten) zusaetzlich als eigene Zeilen ausgeben. Ohne das steckt
    /// nur der jeweils AKTUELLE Schritt in der mitlaufenden Timer-Zeile.
    pub verbose: bool,
    /// Wie viele Brains in den Lesephasen (Sammeln, Abstimmen) gleichzeitig
    /// befragt werden. Bauen bleibt sequenziell — die Brains teilen sich EINEN
    /// Git-Worktree, nebenlaeufige Edits wuerden einander ueberschreiben.
    pub parallel: usize,
    /// Nach wie vielen Iterationen OHNE Fortschritt ein Brain aufgibt und die
    /// Aufgabe weitergereicht wird. `max_iterations` ist nur noch die harte
    /// Obergrenze — wer vorankommt, darf sie ausschoepfen, wer sich im Kreis
    /// dreht, wird frueher gestoppt.
    pub stall_limit: u32,
    /// Wie oft eine Aufgabe hoechstens an ein weiteres Brain weitergereicht wird.
    pub max_handoffs: usize,
    /// Lint-Kommando fuer das Ernte-Tor (leer = kein Lint-Gate).
    ///
    /// Build und Tests sagen "es laeuft", nicht "es ist sauber". geminis
    /// geernteter Beitrag (2026-07-21) kompilierte und war gruen, brachte aber
    /// eine doppelte `use super::*;` mit — die Ernte hatte dafuer kein Auge.
    pub lint_eval: String,
    /// Endlos-Schleife: nach der letzten Runde sofort wieder von vorne.
    pub loop_forever: bool,
}

/// Ein bestandener Brain-Lauf, dessen Diff für die spätere Ernte aufbewahrt wird.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarvestCandidate {
    /// Brain, das diesen Code gebaut hat.
    pub brain: String,
    /// Die Aufgabe, die dieses Brain zugeteilt bekam (fuer die Commit-Message).
    pub task: String,
    /// Der komplette Diff gegen die Baseline (`git diff --cached`).
    pub patch: String,
    /// Benötigte Repair-Iterationen (weniger = souveräner gelöst).
    pub iterations: u32,
    /// Gesamtdauer des Brain-Laufs.
    pub latency_ms: u64,
}

/// Wählt den zu erntenden Kandidaten: wenige Iterationen schlagen viele, bei
/// Gleichstand entscheidet die kürzere Laufzeit.
///
/// „Beim ersten Versuch grün" ist das stärkere Signal als „nach neun Korrekturen
/// grün" — beide bestehen, aber nur eines davon ist verlässliche Arbeit.
pub fn pick_harvest(candidates: &[HarvestCandidate]) -> Option<&HarvestCandidate> {
    candidates
        .iter()
        .filter(|c| !c.patch.trim().is_empty())
        .min_by_key(|c| (c.iterations, c.latency_ms))
}

/// Endergebnis eines Benchmark-Laufs.
#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    /// Je Runde der gevotete Sieger `(runde, text)`.
    pub winners: Vec<(usize, String)>,
    /// Code-Rangliste über alle bislang gespeicherten Events.
    pub leaderboard: Vec<CodeStats>,
    /// Slug der abgelegten Wiki-Seite, falls geschrieben.
    pub wiki_slug: Option<String>,
    /// Tatsächlich geerntete Beiträge `(brain, aufgabe)` — das ist der Teil,
    /// der als Code im Repo bleibt statt nur als Messpunkt.
    pub harvested: Vec<(String, String)>,
}

/// Stabile, kurze Task-Kennung aus dem Sieger-Text (Hash) — gleiche Aufgabe ⇒
/// gleiche `task_id` über Brains und Läufe hinweg.
pub fn task_id(winner: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    winner.trim().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Baut den Aufgaben-Prompt aus dem gevoteten Sieger (Spec §2, Phase A).
pub fn build_task_prompt(winner: &str) -> String {
    format!(
        "Implementiere folgenden Verbesserungsvorschlag im Rust-Projekt webagent-rs \
         (aktuelles Verzeichnis) mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Ergänze \
         Tests. `cargo test --lib` muss grün bleiben. Mache genau diese eine \
         Änderung, nichts darüber hinaus. Ändere weder Cargo.toml noch Cargo.lock, \
         füge keine Dependencies hinzu und bearbeite keine Build-/CI-Skripte.\n\nVorschlag: {winner}",
        winner = winner.trim()
    )
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
///
/// `round` rotiert die Zuteilung: Brain `i` baut Rang `(i + round) % k`. Über
/// mehrere Runden sieht damit jedes Brain jeden Rang, sodass keines dauerhaft
/// die leichteren oder schwereren Aufgaben zieht und der Score fair bleibt.
/// Gibt es weniger Vorschläge als Brains, teilen sich mehrere Brains einen Rang
/// (dann gewinnt beim Ernten der beste — wie im Turnier).
pub fn assign_tasks(brains: &[String], ranked: &[String], round: usize) -> Vec<(String, String)> {
    if ranked.is_empty() {
        return Vec::new();
    }
    brains
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let idx = (i + round) % ranked.len();
            (b.clone(), ranked[idx].clone())
        })
        .collect()
}

/// Wie weit eine Iteration gekommen ist — die Grundlage dafür, ob sich
/// Weitermachen lohnt.
///
/// `stage` ist die grobe Stufe, `errors` die Feinauflösung darin (Compilerfehler
/// bzw. rote Tests). Zwölf Fehler auf elf zu drücken ist Fortschritt, auch wenn
/// die Stufe gleich bleibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// 0 = nichts geändert, 1 = Build rot, 2 = Tests rot, 3 = grün.
    pub stage: u8,
    /// Verbleibende Fehler auf dieser Stufe (kleiner ist besser).
    pub errors: u32,
}

/// Zählt Compilerfehler in einer `cargo build`-Ausgabe.
pub fn count_build_errors(output: &str) -> u32 {
    output
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            // `error[E0308]:` und `error:` — aber nicht die Schlusszeile
            // "error: could not compile …", die nur den Sammelabbruch meldet.
            (t.starts_with("error[") || t.starts_with("error:")) && !t.contains("could not compile")
        })
        .count() as u32
}

/// Zählt fehlgeschlagene Tests aus einer `cargo test`-Ausgabe.
pub fn count_failed_tests(output: &str) -> u32 {
    let mut total = 0u32;
    for part in output.split_whitespace().collect::<Vec<_>>().windows(2) {
        if part[1].starts_with("failed") {
            if let Ok(n) = part[0].parse::<u32>() {
                total += n;
            }
        }
    }
    total
}

/// Bewertet den Stand NACH dem Testlauf.
///
/// Der Fallstrick, den ein echter Lauf offengelegt hat (deepseek, 2026-07-21):
/// `cargo build --lib` war grün, `cargo test --lib` meldete zehnmal in Folge
/// „0 bestanden" — die Test-Binary ließ sich gar nicht erst übersetzen. Naiv
/// gezählt sind das *null* fehlgeschlagene Tests, also scheinbar der beste
/// denkbare Stand auf Stufe 2; ein Brain, das später wirklich Tests laufen
/// lässt und drei davon rot sieht, hätte damit „schlechter" abgeschnitten.
///
/// Deshalb: laufen gar keine Tests, ist das kein Stufe-2-Ergebnis. Der
/// Test-Build ist strenger als `build --lib` (er übersetzt auch die
/// `#[cfg(test)]`-Module), also bleibt es Stufe 1.
pub fn progress_after_tests(output: &str) -> Progress {
    let ran = parse_test_count(output).is_some() || count_failed_tests(output) > 0;
    if ran {
        Progress {
            stage: 2,
            errors: count_failed_tests(output),
        }
    } else {
        Progress {
            stage: 1,
            errors: count_build_errors(output),
        }
    }
}

/// `true`, wenn `now` näher an grün ist als das bisher Beste.
///
/// Ohne dieses Maß entschied allein das Schleifenlimit über den Abbruch: ein
/// Brain, das sich Iteration für Iteration von zwölf Fehlern auf zwei
/// herunterarbeitet, wurde genauso hart gestoppt wie eines, das zehnmal
/// dieselbe kaputte Zeile schreibt (Beobachtung Storax, deepseek lief
/// regelmäßig ins Limit statt zu scheitern).
pub fn is_improvement(best: Option<Progress>, now: Progress) -> bool {
    match best {
        None => now.stage > 0,
        Some(b) => now.stage > b.stage || (now.stage == b.stage && now.errors < b.errors),
    }
}

/// `true`, wenn der Run-Status eine EXTERNE Blockade meldet (Anbieter-Limit,
/// Login, Cloudflare, Oberflaeche ohne Antwort) statt eines Fehlversuchs.
///
/// Solche Laeufe duerfen nicht in den Score: sonst faellt die Bewertung eines
/// Brains mit der Auslastung seines Anbieters statt mit seiner Faehigkeit.
pub fn is_external_block(status: &str) -> bool {
    let low = status.to_lowercase();
    [
        "brain_unavailable",
        "blocked",
        "login_required",
        "cloudflare",
        "rate_limit",
    ]
    .iter()
    .any(|p| low.contains(p))
}

/// `true`, wenn ein Versuch objektiv besteht (geändert UND gebaut UND grün).
pub fn is_pass(did_change: bool, compiled: bool, tests_passed: bool) -> bool {
    did_change && compiled && tests_passed
}

/// Menschenlesbare Outcome-Klassifikation für die Live-Ausgabe.
pub fn outcome_label(did_change: bool, compiled: bool, tests_passed: bool) -> &'static str {
    if !did_change {
        "SKIP (keine Änderung)"
    } else if !compiled {
        "FAIL (build)"
    } else if !tests_passed {
        "FAIL (test)"
    } else {
        "PASS"
    }
}

/// Markdown-Body für die Wiki-Ablage (`code-benchmark-<stamp>`): welche Sieger je
/// Runde gebaut werden sollten + die aktuelle Code-Rangliste.
pub fn format_benchmark_report(winners: &[(usize, String)], board: &[CodeStats]) -> String {
    let mut out = String::from(
        "Vote-driven Code-Benchmark: pro Runde stimmt der Schwarm über den nächsten \
         Verbesserungsschritt ab; jedes Brain baut den Sieger sequenziell, gemessen \
         wird objektiv (Compiler + Tests, kein Selbst-Report).\n\n",
    );
    out.push_str("## Gevotete Sieger je Runde\n");
    if winners.is_empty() {
        out.push_str("(keine — keine Stimmen gesammelt)\n");
    } else {
        for (round, winner) in winners {
            out.push_str(&format!("{round}. {winner}\n"));
        }
    }
    out.push_str("\n## Code-Rangliste\n");
    out.push_str("| brain | attempts | change% | compile% | pass% | wilson_pass |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    if board.is_empty() {
        out.push_str("| (keine Daten) | 0 | – | – | – | – |\n");
    } else {
        for s in board {
            out.push_str(&format!(
                "| {} | {} | {:.0}% | {:.0}% | {:.0}% | {:.3} |\n",
                s.brain_id,
                s.attempts,
                s.change_rate * 100.0,
                s.compile_rate * 100.0,
                s.pass_rate * 100.0,
                s.wilson_pass
            ));
        }
    }
    out
}

/// Druckt die Code-Rangliste auf stdout und Ereignisstrom (Live-Ausgabe am Ende, Spec §4).
fn print_leaderboard(board: &[CodeStats]) {
    bench_say!(crate::bench_events::Level::Info, None, "Code-Rangliste:");
    bench_say!(
        crate::bench_events::Level::Info,
        None,
        "  brain            attempts  change%  compile%  pass%   wilson_pass  schwer  rettung  aufgegeben"
    );
    for s in board {
        bench_say!(
            crate::bench_events::Level::Info,
            Some(&s.brain_id),
            "  {:<15}  {:>8}  {:>6.0}%  {:>7.0}%  {:>5.0}%   {:>11.3}  {:>6}  {:>7}  {:>10}",
            s.brain_id,
            s.attempts,
            s.change_rate * 100.0,
            s.compile_rate * 100.0,
            s.pass_rate * 100.0,
            s.wilson_pass,
            s.hard_attempts,
            s.rescues,
            s.abandoned
        );
    }
    let rescues: usize = board.iter().map(|s| s.rescues).sum();
    if rescues > 0 {
        bench_say!(
            crate::bench_events::Level::Info,
            None,
            "  ({rescues} Rettung(en): bestanden an Aufgaben, die ein anderes Brain aufgegeben hatte)"
        );
    }
}

/// `true`, wenn der Working Tree Änderungen enthält (inkl. neu angelegter,
/// untracked Dateien — `git diff --quiet` allein übersähe die von write-Actions
/// erzeugten neuen Dateien; deshalb `git status --porcelain`).
fn tree_changed(workdir: &Path) -> bool {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(workdir)
        .output()
    {
        Ok(out) => !String::from_utf8_lossy(&out.stdout).trim().is_empty(),
        Err(_) => false,
    }
}

/// Führt ein Eval-Kommando aus und wertet nur den Exit-Code (0 = ok) — nutzt den
/// Kommando-Runner mit Timeout aus autoresearch.
/// Führt ein Eval-Kommando aus: `(exit==0, Ausgabe)`. Die Ausgabe geht als
/// Kontext in den Repair-Versuch (Compiler-/Testfehler zurück ans Brain).
fn run_eval_detail(cmd: &str, workdir: &Path) -> (bool, String) {
    match crate::autoresearch::run_eval_with_timeout(cmd, workdir, EVAL_TIMEOUT_SECS) {
        Ok((code, out)) => (code == Some(0), out),
        Err(e) => (false, e),
    }
}

/// Baut den Folgeauftrag nach einem fehlgeschlagenen Build/Test: die
/// Original-Aufgabe plus die echte Fehlerausgabe als Kontext.
///
/// Ohne diesen Schritt zählte ein Brain als Fehlschlag, obwohl es oft nur einen
/// Tippfehler von grün entfernt war — echte Coding-Agenten lesen den
/// Compilerfehler und korrigieren (Storax-Vorschlag 2026-07-21).
/// Anstoss, wenn ein Brain „fertig" meldet, ohne eine Datei geändert zu haben.
///
/// Das trifft die Explore-and-give-up-Fälle: claude las die Zieldatei und
/// verweigerte, kimi meldete per message-Action „Implementierung erfolgreich" —
/// beide ohne einen einzigen Edit. Der Anstoss macht unmissverständlich, dass
/// ein Edit die Aufgabe IST, nicht eine Meldung darüber.
pub fn build_no_change_prompt(task: &str) -> String {
    format!(
        "{task}\n\n--- KEIN EDIT ERKANNT ---\n\
         Du hast KEINE Datei geändert. Eine Nachricht oder Zusammenfassung zählt \
         NICHT — die Aufgabe ist erst gelöst, wenn die Datei tatsächlich \
         geändert ist. Gib JETZT genau eine Änderung im Rohformat aus \
         (WEBAGENT/1 EDIT oder WEBAGENT/1 WRITE) und sonst nichts. Behaupte \
         keinen Erfolg, ohne editiert zu haben.",
        task = task.trim()
    )
}

pub fn build_repair_prompt(task: &str, stage: &str, output: &str) -> String {
    format!(
        "{task}\n\n--- KORREKTUR NÖTIG ---\n\
         Deine bisherige Änderung ist im Arbeitsverzeichnis vorhanden, aber \
         `{stage}` schlägt fehl. Lies die Fehlerausgabe, finde die Ursache und \
         korrigiere sie mit dem Rohformat (WEBAGENT/1 EDIT/WRITE). Fange NICHT \
         von vorne an — repariere das Vorhandene.\n\n\
         Fehlerausgabe (gekürzt):\n{out}",
        task = task.trim(),
        stage = stage,
        out = crate::char_prefix(output.trim(), 2500)
    )
}

/// Setzt den Working Tree hart auf `baseline` zurück und entfernt untracked
/// Dateien — jeder Brain-Run startet identisch, der Benchmark hinterlässt nichts.
fn reset_repo(workdir: &Path, baseline: &str) -> Result<(), String> {
    // `git add -A` davor, damit auch neu angelegte (untracked) Dateien vom
    // Full-Reset erfasst werden; `git clean -fd` fegt den Rest weg.
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workdir)
        .output();
    crate::autoresearch::git_reset_hard(workdir, baseline)?;
    let _ = std::process::Command::new("git")
        .args(["clean", "-fd"])
        .current_dir(workdir)
        .output();
    Ok(())
}

/// Sichert die Arbeit des laufenden Brains als Patch, BEVOR `reset_repo` sie
/// verwirft. `git add -A` davor, damit neue Dateien im Diff auftauchen.
fn capture_patch(workdir: &Path) -> Result<String, String> {
    let _ = std::process::Command::new("git")
        .args(["add", "-A"])
        .current_dir(workdir)
        .output();
    let out = std::process::Command::new("git")
        .args(["diff", "--cached", "--binary"])
        .current_dir(workdir)
        .output()
        .map_err(|e| format!("git diff fehlgeschlagen: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Schutzgitter für autonom geernteten Code. Ein Benchmark darf kleine,
/// nachvollziehbare Rust-Änderungen verbessern, aber weder Dependencies noch
/// Build-/CI-Konfiguration umformen. Solche Eingriffe brauchen eine bewusste
/// menschliche Entscheidung und werden deshalb nie automatisch geerntet.
pub fn validate_harvest_patch(patch: &str) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for line in patch.lines() {
        let Some(path) = line.strip_prefix("+++ b/") else {
            continue;
        };
        if path == "/dev/null" {
            continue;
        }
        paths.push(path.to_string());
    }
    paths.sort();
    paths.dedup();
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
fn harvest_commit(
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

/// Ein Brain baut die Aufgabe über den normalen Controller-Pfad (mit Wall-Timeout
/// und kleinem `max_cycles`). Liefert `(status, cycles)`.
#[cfg(feature = "webview")]
fn bench_run(
    brain_id: &str,
    task: &str,
    headless: bool,
    note: Option<crate::StageNote>,
    verbose: bool,
) -> Result<(String, u32), String> {
    use crate::browser::WebBrainBackend;
    use crate::controller::AgentController;
    use crate::executor::PlatformShellExecutor;

    let backend = WebBrainBackend::from_config(brain_id)?;
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::with_data_dir(
        backend,
        executor,
        BENCH_MAX_CYCLES,
        crate::config::data_dir(),
    );
    controller.set_wall_timeout_secs(BENCH_WALL_SECS);
    // Ohne --verbose wandern die Schritt-Zeilen IN die Timer-Zeile, statt sie
    // zu zerschneiden; mit --verbose laeuft beides nebeneinander.
    if let Some(n) = note {
        controller.set_progress(n, !verbose);
    }
    let meta = controller.run(task, brain_id, None, headless)?;
    Ok((meta.status, meta.cycles))
}

#[cfg(not(feature = "webview"))]
fn bench_run(
    _brain_id: &str,
    _task: &str,
    _headless: bool,
    _note: Option<crate::StageNote>,
    _verbose: bool,
) -> Result<(String, u32), String> {
    Err("webview-Feature nicht aktiv — kein Brain-Backend verfügbar".to_string())
}

/// Übersetzt EINEN gevoteten Vorschlag in eine konkrete, bounded Coding-Aufgabe
/// (Phase A.5). Zwei Versuche: verlangt die Aufgabe etwas bereits Vorhandenes,
/// wäre der Bauauftrag wertlos ("ist schon implementiert" → keine Änderung →
/// fälschlich FAIL). Fällt die Verfeinerung aus, trägt der Rohvorschlag.
fn refine_one<Q>(
    winner: &str,
    facts: &str,
    refiner: &str,
    existing_api: &[String],
    src_files: &[String],
    query: &Q,
) -> String
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    if refiner.is_empty() {
        return winner.to_string();
    }
    for attempt in 1..=2 {
        let mut prompt = build_refine_prompt(winner, facts, src_files);
        if attempt > 1 {
            prompt.push_str(
                "

WICHTIG: Dein vorheriger Vorschlag verlangte eine Funktion, die es                  BEREITS GIBT. Schlage etwas anderes vor, das noch NICHT existiert.",
            );
        }
        match query(refiner, &prompt) {
            Ok(text) => match usable_refinement(&text) {
                Some(t) if task_is_redundant(&t, existing_api) => {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  verworfen: verlangt bereits vorhandene Funktion ({:?})",
                        proposed_fn_name(&t).unwrap_or_default()
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
    winner.to_string()
}

/// Arbeitsschlange der Phase B samt Handoff-Buchhaltung.
///
/// Steckt in einer eigenen Struct, weil genau hier der Endlos-Pingpong sass:
/// `refined_cache` dedupliziert Aufgabentexte, deshalb tragen bei wenigen
/// Vorschlaegen MEHRERE Plan-Eintraege dieselbe Aufgabe — und `tried` ist nach
/// eben diesem Text gekeyt. Ohne Kill-Flag lief eine laengst ausgefallene
/// Aufgabe fuer jeden weiteren Eintrag erneut durch einen vollen Brain-Run.
/// Als reine Datenstruktur ist das ohne Netzwerk und ohne Brains testbar.
pub(crate) struct HandoffQueue {
    /// (Brain, Aufgabentext, abgebendes Brain — None = frischer Plan-Eintrag)
    queue: std::collections::VecDeque<(String, String, Option<String>)>,
    tried: std::collections::HashMap<String, Vec<String>>,
    dropped: std::collections::HashSet<String>,
    brains: Vec<String>,
    max_handoffs: usize,
}

impl HandoffQueue {
    pub(crate) fn new(plan: &[(String, String)], brains: &[String], max_handoffs: usize) -> Self {
        Self {
            queue: plan
                .iter()
                .map(|(b, t)| (b.clone(), t.clone(), None))
                .collect(),
            tried: std::collections::HashMap::new(),
            dropped: std::collections::HashSet::new(),
            brains: brains.to_vec(),
            max_handoffs,
        }
    }

    /// Naechster Auftrag. Ausgefallene Aufgaben werden hier verworfen, damit
    /// sie keinen weiteren Brain-Run kosten.
    pub(crate) fn next(&mut self) -> Option<(String, String, Option<String>)> {
        while let Some((brain, effective, from)) = self.queue.pop_front() {
            if self.dropped.contains(&effective) {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain.as_str()),
                    "{effective} bereits ausgefallen — ueberspringe Eintrag fuer {brain}."
                );
                continue;
            }
            // Handoffs sind bereits beim Einreihen vermerkt; nur frische
            // Plan-Eintraege muessen hier nachgetragen werden.
            if from.is_none() {
                self.tried
                    .entry(effective.clone())
                    .or_default()
                    .push(brain.clone());
            }
            return Some((brain, effective, from));
        }
        None
    }

    /// Reicht eine steckengebliebene Aufgabe weiter. `Some(nb)` = uebernimmt
    /// `nb`, `None` = niemand mehr uebrig, Aufgabe faellt endgueltig aus.
    ///
    /// Die Reservierung passiert BEIM EINREIHEN, nicht erst beim Poppen —
    /// sonst waehlen zwei Stalls derselben Aufgabe dasselbe naechste Brain.
    pub(crate) fn on_stall(&mut self, brain: &str, effective: &str) -> Option<String> {
        let already = self.tried.entry(effective.to_string()).or_default();
        let cap = self.max_handoffs.max(1) + 1;
        let next = if already.len() < cap {
            self.brains.iter().find(|b| !already.contains(b)).cloned()
        } else {
            None
        };
        match next {
            Some(nb) => {
                already.push(nb.clone());
                self.queue
                    .push_back((nb.clone(), effective.to_string(), Some(brain.to_string())));
                Some(nb)
            }
            None => {
                self.dropped.insert(effective.to_string());
                None
            }
        }
    }

    /// Nur fuer Tests: der Produktionspfad fragt den Zustand nicht ab, weil
    /// `next()` ausgefallene Aufgaben selbst wegwirft.
    #[cfg(test)]
    pub(crate) fn is_dropped(&self, effective: &str) -> bool {
        self.dropped.contains(effective)
    }
}

/// Fährt den vollen Benchmark: `query` speist Phase A (Swarm-Abstimmung, in
/// CLI/REPL `repl::isolated_query`). Der Live-Teil (Phase B) läuft über den
/// Controller; getestet wird er e2e vom Orchestrator, nicht im Unit-Test.
pub fn run_benchmark<Q>(config: &BenchmarkConfig, query: Q) -> Result<BenchmarkReport, String>
where
    Q: Fn(&str, &str) -> Result<String, String> + Sync,
{
    // Sicherheitsmodell §5: nur auf sauberem Git-Tree starten.
    if !crate::autoresearch::git_status_clean(&config.workdir)? {
        return Err(format!(
            "Working Tree in {} ist nicht sauber — bitte committen oder stashen. \
             Der Benchmark läuft nur auf sauberem Stand und resettet nach jedem Run.",
            config.workdir.display()
        ));
    }

    let rounds = config.rounds.max(1);
    let facts = crate::self_research::gather_facts(&config.workdir, FACTS_MAX_CHARS);
    let mut winners: Vec<(usize, String)> = Vec::new();
    let mut harvested: Vec<(String, String)> = Vec::new();

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
        bench_say!(
            crate::bench_events::Level::Info,
            None,
            "runde {round}/{total} — abstimmen…"
        );
        // Phase A — Sammeln + Abstimmen. `&query` implementiert Fn ⇒ pro Runde
        // wiederverwendbar, ohne die Closure zu bewegen.
        let report = crate::self_research::run_self_research(
            &config.brains,
            &facts,
            config.suggestions,
            VOTE_TOP_K,
            config.parallel,
            &query,
        );
        let ranked = ranked_from_report(&report);
        if ranked.is_empty() {
            bench_say!(
                crate::bench_events::Level::Warn,
                None,
                "runde {round}: kein Sieger (keine Stimmen) — überspringe."
            );
            continue;
        }
        let winner = ranked[0].clone();
        bench_say!(crate::bench_events::Level::Pass, None, "Sieger: {winner}");
        winners.push((round, winner.clone()));

        // Turnier statt Mischmasch: jedes Brain bearbeitet exakt den gewählten
        // Sieger. Damit misst der Score die Qualität der Umsetzung und nicht,
        // ob ein zufällig zugeteilter Neben-Vorschlag leichter war.
        let assignments = assign_tasks(&config.brains, std::slice::from_ref(&winner), round);

        // Phase A.5 — jede zugeteilte Aufgabe konkretisieren. Gleiche Vorschlaege
        // nur einmal verfeinern (bei weniger Vorschlaegen als Brains).
        let refiner = config.brains.first().cloned().unwrap_or_default();
        let existing_api = crate::self_research::collect_public_api(&config.workdir.join("src"));
        let src_files: Vec<String> =
            crate::self_research::collect_modules(&config.workdir.join("src"))
                .into_iter()
                .map(|(name, _lines)| format!("src/{name}"))
                .collect();
        let mut refined_cache: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut plan: Vec<(String, String)> = Vec::new();
        for (brain, raw) in &assignments {
            let eff = match refined_cache.get(raw) {
                Some(t) => t.clone(),
                None => {
                    let t =
                        crate::StageTimer::start(format!("verfeinern fuer {brain} via {refiner}"));
                    let e = refine_one(raw, &facts, &refiner, &existing_api, &src_files, &query);
                    t.finish(crate::char_prefix(&e, 90));
                    refined_cache.insert(raw.clone(), e.clone());
                    e
                }
            };
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
            let (_ok, out) = run_eval_detail(&config.test_eval, &config.workdir);
            let n = parse_test_count(&out);
            t.finish(&format!("Baseline: {} Tests", n.unwrap_or(0)));
            n.unwrap_or(0)
        };

        // Phase B — Arbeitsschlange statt fester Liste: bleibt ein Brain stecken,
        // wandert SEINE Aufgabe an ein Brain, das sie noch nicht versucht hat.
        let mut harvest_pool: Vec<HarvestCandidate> = Vec::new();
        let mut hq = HandoffQueue::new(&plan, &config.brains, config.max_handoffs);
        while let Some((brain_owned, effective_owned, handoff_from)) = hq.next() {
            let brain = &brain_owned;
            let effective = &effective_owned;

            if let Some(prev) = &handoff_from {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain),
                    "{brain} uebernimmt die Aufgabe von {prev}."
                );
            }
            let task = build_task_prompt(effective);
            let tid = task_id(effective);
            if !crate::autoresearch::git_status_clean(&config.workdir)? {
                return Err(format!(
                    "Working Tree in {} ist vor dem Run von {brain} nicht sauber — Abbruch.",
                    config.workdir.display()
                ));
            }
            let baseline = crate::autoresearch::git_head_sha(&config.workdir)?;
            let started = Instant::now();

            // Repair-Loop: schlägt Build/Test fehl, geht die echte Fehlerausgabe
            // als Kontext zurück ans Brain (bis zu max_iterations). Zwischen den
            // Iterationen wird NICHT resettet — das Brain repariert sein eigenes
            // Werk, wie ein echter Coding-Agent am Compilerfehler.
            let max_iter = config.max_iterations.max(1);
            let stall_limit = config.stall_limit.max(1);
            let mut attempt_task = task.clone();
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

            for iter in 1..=max_iter {
                iterations = iter;
                let t = crate::StageTimer::start(format!(
                    "{brain} Iteration {iter}/{max_iter}: Brain baut"
                ));
                match bench_run(
                    brain,
                    &attempt_task,
                    config.headless,
                    Some(t.note_handle()),
                    config.verbose,
                ) {
                    Ok((status, c)) => {
                        cycles += c;
                        if is_external_block(&status) {
                            unavailable = true;
                            t.finish("Brain nicht verfuegbar (extern)");
                            break;
                        }
                        t.finish(&format!("Brain fertig ({c} Zyklen)"));
                    }
                    Err(e) => {
                        t.finish("Brain-Run fehlgeschlagen");
                        bench_say!(
                            crate::bench_events::Level::Fail,
                            Some(brain),
                            "{brain}: run fehlgeschlagen — {e}"
                        );
                    }
                }

                did_change = tree_changed(&config.workdir);
                if !did_change {
                    // Keine Änderung. Frueher hiess das sofort Abbruch — dabei
                    // haben claude und kimi (2026-07-21) den Code erkundet und
                    // dann „fertig" gemeldet, ohne je zu editieren (kimi sogar
                    // per message-Action „Implementierung erfolgreich"). Ein
                    // einziger Anstoss kann so ein Brain ueber die Linie bringen:
                    // nachschieben, dass ein Edit PFLICHT ist, und als Stillstand
                    // zaehlen (das stall_limit deckelt endloses Nicht-Editieren).
                    stalls += 1;
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
                // Gruen UND mehr Tests als vorher: nur dann ist der Code wirklich
                // eingebunden und getestet (verwaiste Datei erhoeht die Zahl nicht).
                tests_passed = t_ok && after > baseline_tests;
                if t_ok && after <= baseline_tests {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        None,
                        "  Tests gruen, aber Testzahl unveraendert ({after} <= {baseline_tests}) — nicht eingebunden"
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
                // Weitergeben an das erste Brain, das diese Aufgabe noch nicht
                // hatte. Nicht endlos: sonst reicht ein unloesbarer Vorschlag
                // die Runde durch alle acht Brains und frisst die Zeit auf.
                match hq.on_stall(brain, effective) {
                    Some(nb) => bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain),
                        "{brain}: {stall_limit}x kein Fortschritt — Aufgabe geht an {nb}."
                    ),
                    None => bench_say!(
                        crate::bench_events::Level::Fail,
                        Some(brain),
                        "{brain}: {stall_limit}x kein Fortschritt — niemand mehr uebrig, Aufgabe faellt aus."
                    ),
                }
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
                    .or_else(|| validate_harvest_patch(patch).err())
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

            // Bestandene Arbeit sichern, BEVOR der Reset sie verwirft.
            if config.harvest && is_pass(did_change, compiled, tests_passed) {
                match patch_scope {
                    Some((patch, None)) if !patch.trim().is_empty() => {
                        match validate_harvest_patch(&patch) {
                            Ok(paths) => {
                                bench_say!(
                                crate::bench_events::Level::Pass,
                                Some(brain),
                                "  {brain}: Patch gesichert ({} Dateien, {} Zeilen) — Kandidat für die Ernte",
                                paths.len(), patch.lines().count()
                            );
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
                    Some((patch, None)) if patch.trim().is_empty() => bench_say!(
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
    use super::*;
    use crate::self_research::RankedSuggestion;

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
        assert!(is_external_block("cloudflare"));
        // ... echte Fehlversuche NICHT, sonst verschwindet Unfaehigkeit aus
        // der Statistik und der Score wird bedeutungslos.
        assert!(!is_external_block("protocol_error"));
        assert!(!is_external_block("max_cycles"));
        assert!(!is_external_block("brain_incomplete"));
        assert!(!is_external_block("done"));
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

/// Erzeugt ein reproduzierbares, maschinenlesbares Ergebnisformat fuer Benchmark-Szenarien.
pub fn format_benchmark_result(name: &str, value: u64, unit: &str) -> String {
    format!("{}={}{}", name, value, unit)
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
