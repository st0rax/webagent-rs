//! AgentController – Plan/Act/Observe-Loop.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use std::env;

use crate::brain::BrainBackend;
use crate::comms::CommsStore;
use crate::executor::ShellExecutor;
use crate::loop_guard::{
    is_shell_read_action, loop_guard_message, read_budget_message, shell_read_fingerprint,
};
use crate::memory::MemoryStore;
use crate::prompts::autonomous_task_prompt;
use crate::protocol::{self, Action};
use crate::run_store::{RunMeta, RunStore};
use crate::transcript::Transcript;

mod action_engine;
mod plan;
mod resume;
mod types;

pub use plan::validate_action_plan;
#[cfg(test)]
pub(crate) use resume::{incomplete_retry_backoff, is_valid_resume_conversation_ref};
pub use types::{BrainTurn, RunOptions};

// Konfigurationskonstanten (aus CONVENTIONS.md: keine externe config-Crate)
use crate::config::{max_observation_chars_for, LOOP_GUARD_ABORT_COUNT, LOOP_GUARD_WARN_COUNT};
const MEMORY_CONTEXT_LIMIT: usize = 5;
const CONTROLLER_HEARTBEAT_INTERVAL_SECONDS: f64 = 30.0;

/// Wie oft bei „fertig ohne durchgelaufenen Edit" nachgehakt wird, bevor der
/// Run als `false_done` endet. Zwei Versuche reichen: wer den Anker nach dem
/// zweiten Hinweis nicht trifft, trifft ihn auch beim fuenften nicht.
const MAX_NO_CHANGE_NUDGES: u32 = 2;

/// Länge, unter der eine Antwort ohne Protokoll-Nutzlast als „fast leer"
/// gilt (#11, abgestimmt 02.08., Daten 06.08.). Solche Antworten werden als
/// `brain_unavailable` verbucht statt als `protocol_invalid` — ein
/// Repair-Prompt („antworte im richtigen Format") kann nichts reparieren,
/// was nie begonnen wurde, und kostet 10-35 s Roundtrip.
const FAST_EMPTY_RESPONSE_CHARS: usize = 20;

fn protocol_completes_unstable_response(
    generation_complete: bool,
    backend_status: &str,
    text: &str,
) -> bool {
    !generation_complete
        && backend_status == "timeout_unstable"
        && !protocol::is_possibly_truncated(text)
        && protocol::parse(text).valid
}
/// Nach so vielen reinen Leseaktionen ohne erfolgreichen Datei-Write wird das
/// Brain aus variierender Exploration in die Umsetzung geschoben.
const READ_BUDGET_ACTIONS: u32 = 5;

/// AgentController orchestriert Brain + Executor im Plan/Act/Observe-Loop.
pub struct AgentController<B: BrainBackend, E: ShellExecutor> {
    brain: B,
    executor: E,
    max_cycles: usize,
    run_store: RunStore,
    memory: MemoryStore,
    /// Markdown-Wiki (Langzeitwissen); Layout entsteht erst beim ersten Zugriff.
    wiki: crate::wiki_memory::WikiMemory,
    runs_dir: std::path::PathBuf,
    meta: Option<RunMeta>,
    comms: CommsStore,
    completed_actions: HashMap<String, String>,
    incomplete_retries: usize,
    /// In DIESEM Run tatsächlich ausgeführte Arbeits-Actions (shell/edit/write).
    /// `done` mit 0 Act-Steps ist verdächtig: gemini lieferte am 2026-07-20 eine
    /// Stale-Antwort aus einer alten Konversation als sofortiges message-done,
    /// ohne je gearbeitet zu haben (Pfad c des Phantom-Done-Komplexes).
    act_steps: u32,
    /// Versuchte edit/write-Actions in DIESEM Run.
    file_actions_tried: u32,
    /// Davon mit exit 0 durchgelaufen.
    ///
    /// Deckt den Fall ab, den `act_steps` NICHT sieht: das Brain arbeitet
    /// sichtbar (Shell-Reads, Edit-Versuche), aber jeder Edit scheitert am
    /// Anker (`old_string nicht gefunden`) — und es meldet trotzdem „fertig"
    /// samt erfundenem Testergebnis. Real 2026-07-24, vier Laeufe in Folge.
    file_writes_ok: u32,
    shell_reads_since_write: u32,
    read_budget_warned: bool,
    /// Wie oft schon wegen „fertig ohne Edit" nachgehakt wurde (Deckel gegen
    /// Endlos-Nachhaken bei einem Brain, das partout nicht editiert).
    no_change_nudges: u32,
    /// Optionale Wall-Deadline (Sekunden) statt `config::max_run_wall_secs()`.
    /// Pro Controller injizierbar — Tests duerfen KEINE prozessglobale
    /// Env-Variable setzen (das brach parallel laufende Tests, Fund 2026-07-21).
    wall_secs_override: Option<u64>,
    /// Absolute Wall-Deadline des laufenden Runs, gesetzt beim Schleifenstart.
    ///
    /// `run_once` deckelt sein `wait_response`-Timeout auf die verbleibende
    /// Zeit bis hierher. Ohne das wartet ein einzelner Turn sein volles
    /// Per-Brain-Timeout (plus bis zu 3 Rereads) aus — real 2026-07-21 lief ein
    /// haengendes mistral/gemini bis 409s, obwohl die Deadline 300s war, weil
    /// die Wand nur ZWISCHEN Zyklen geprueft wurde, nicht waehrend des Wartens.
    wall_deadline_at: Option<Instant>,
    /// Absolute Notbremse. Produktive Writes dürfen die Arbeits-Lease bis zu
    /// dieser Grenze verlängern, aber niemals einen unbegrenzten Run erzeugen.
    wall_hard_deadline_at: Option<Instant>,
    /// Länge einer neuen Lease nach belegtem Fortschritt.
    wall_lease_secs: u64,
    /// Senke fuer „was tue ich gerade" — die mitlaufende Timer-Zeile.
    progress: Option<crate::StageNote>,
    /// Unterdrueckt die Schritt-fuer-Schritt-Ausgabe. Am Terminal wuerden diese
    /// Zeilen die sich selbst aktualisierende Timer-Zeile zerschiessen; der
    /// Inhalt steckt dann stattdessen IN der Timer-Zeile.
    quiet: bool,
    /// Expliziter Workspace fuer native Edit/Write-Actions. Ohne Override
    /// bleibt das historische Verhalten (naechster Git-Root ab Prozess-CWD).
    workspace_root: Option<std::path::PathBuf>,
}

impl<B: BrainBackend, E: ShellExecutor> AgentController<B, E> {
    pub const MAX_INCOMPLETE_RETRIES: usize = 5;

    /// Haengt die Fortschrittsanzeige an eine mitlaufende Timer-Zeile und
    /// schaltet die einzelnen Schritt-Zeilen ab (`quiet`), damit die Anzeige
    /// nicht von fremder Ausgabe zerschnitten wird.
    pub fn set_progress(&mut self, note: crate::StageNote, quiet: bool) {
        self.progress = Some(note);
        self.quiet = quiet;
    }

    /// Meldet den aktuellen Schritt an die Fortschritts-Senke (falls gesetzt).
    fn report_step(&self, what: &str) {
        if let Some(p) = &self.progress {
            p.set(what);
        }
    }

    pub fn brain(&self) -> &B {
        &self.brain
    }

    pub fn brain_mut(&mut self) -> &mut B {
        &mut self.brain
    }

    pub fn new(brain: B, executor: E, max_cycles: usize) -> Self {
        // Stabiler OS-Ort statt CWD-abhängigem ./data: vorher landeten Runs/
        // Memory/Wiki je nach Aufrufweg (run/bot2bot-worker vs. REPL) in
        // verschiedenen Verzeichnissen — u.a. blieb dadurch der Wiki-Kontext
        // bei `webagent run` leer, obwohl die REPL ihn sah (Fund 2026-07-20).
        Self::with_data_dir(brain, executor, max_cycles, crate::config::data_dir())
    }

    /// Wie `new`, aber mit explizitem Daten-Verzeichnis. Ermöglicht Test-Isolation
    /// (statt des Python-`monkeypatch`) und erlaubt `main`, den Datenpfad zu setzen.
    pub fn with_data_dir(
        brain: B,
        executor: E,
        max_cycles: usize,
        data_dir: std::path::PathBuf,
    ) -> Self {
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let memory_path = data_dir.join("memory.jsonl");

        Self {
            brain,
            executor,
            max_cycles,
            run_store: RunStore::new(runs_dir.clone(), logs_dir),
            memory: MemoryStore::new(memory_path),
            // Wiki-Wurzel wie config::data_dir()/memory/wiki — hier über das
            // uebergebene data_dir, damit Tests isoliert bleiben. Kein
            // ensure_layout beim Konstruieren (erst beim ersten Zugriff).
            wiki: crate::wiki_memory::WikiMemory::new(data_dir.join("memory").join("wiki")),
            runs_dir,
            meta: None,
            comms: CommsStore::new(data_dir.join("comms")),
            completed_actions: HashMap::new(),
            incomplete_retries: 0,
            act_steps: 0,
            file_actions_tried: 0,
            file_writes_ok: 0,
            shell_reads_since_write: 0,
            read_budget_warned: false,
            no_change_nudges: 0,
            wall_secs_override: None,
            wall_deadline_at: None,
            wall_hard_deadline_at: None,
            wall_lease_secs: 0,
            progress: None,
            quiet: false,
            workspace_root: None,
        }
    }

    /// Bindet native Datei-Aktionen an denselben Workspace, den der Aufrufer
    /// fuer Git-Messung, Build und Tests verwendet.
    pub fn set_workspace_root(&mut self, root: impl Into<std::path::PathBuf>) {
        self.workspace_root = Some(root.into());
    }

    /// Setzt die Wall-Deadline dieses Runs (Sekunden) und uebersteuert damit
    /// `config::max_run_wall_secs()` — fuer Tests und Aufrufer, die einen Run
    /// enger begrenzen wollen, ohne die Umgebung des Prozesses zu veraendern.
    pub fn set_wall_timeout_secs(&mut self, secs: u64) {
        self.wall_secs_override = Some(secs);
    }

    /// Persistiert conversation_ref in RunMeta.
    fn persist_conversation_ref(&mut self) {
        if let Some(meta) = &mut self.meta {
            if let Some(ref_val) = self.brain.get_conversation_ref() {
                meta.conversation_ref = Some(ref_val);
                let _ = self.run_store.save(meta);
            }
        }
    }

    /// Führt einen einzelnen Brain-Turn aus.
    /// Deckelt ein Timeout auf die verbleibende Wall-Zeit (Sekunden).
    ///
    /// Ohne aktive Deadline (kein Run-Loop, z.B. im Unit-Test) bleibt der Wert
    /// unveraendert. Mindestens 1s, damit ein knapp vor der Deadline gestarteter
    /// Turn nicht mit 0s sofort scheitert — der naechste Schleifenkopf beendet
    /// den Run dann regulaer mit wall_timeout.
    fn cap_to_wall(&self, timeout: f64) -> f64 {
        match self.wall_deadline_at {
            Some(deadline) => {
                let remaining = deadline
                    .checked_duration_since(Instant::now())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                timeout.min(remaining.max(1.0))
            }
            None => timeout,
        }
    }

    fn wall_expired(&self) -> bool {
        self.wall_deadline_at
            .is_some_and(|deadline| Instant::now() >= deadline)
    }

    /// Ein erfolgreicher Datei-Write ist ein objektiver Fortschrittsbeleg. Er
    /// erneuert die Arbeits-Lease ab jetzt; die absolute Notbremse bleibt hart.
    fn renew_progress_lease(&mut self) {
        let (Some(current), Some(hard)) = (self.wall_deadline_at, self.wall_hard_deadline_at)
        else {
            return;
        };
        let proposed = Instant::now() + Duration::from_secs(self.wall_lease_secs.max(1));
        let renewed = proposed.min(hard);
        if renewed > current {
            self.wall_deadline_at = Some(renewed);
            if let Some(meta) = &mut self.meta {
                let renewals = meta
                    .extra
                    .get("progress_lease_renewals")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
                    .saturating_add(1);
                meta.extra
                    .insert("progress_lease_renewals".to_string(), renewals.into());
                let _ = self.run_store.save(meta);
            }
        }
    }

    pub fn run_once(
        &mut self,
        message: &str,
        mut transcript: Option<&mut Transcript>,
    ) -> BrainTurn {
        if let Some(t) = transcript.as_deref_mut() {
            let _ = t.append("user", message, HashMap::new());
        }

        let brain_id = self.brain.brain_id().to_string();
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(&brain_id),
            "Browser: Auftrag wird gesendet",
        );
        let baseline = self.brain.send(message).unwrap_or_default();
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(&brain_id),
            "Browser: warte auf Modellantwort",
        );

        // Dynamisches Timeout je Brain-Geschwindigkeit und Nachrichtengröße,
        // statt eines pauschalen Werts (langsame Brains wie claude/gemini brauchen
        // mehr Zeit; hartkodierte 60s waren die Haupt-Timeout-Ursache).
        let wait_timeout =
            crate::timeouts::resolve_timeout("wait_response", self.brain.brain_id(), message, None);
        // Nie laenger warten als bis zur Wall-Deadline: ein einzelner haengender
        // Turn darf die Gesamtfrist nicht ueberziehen (Fund 2026-07-21).
        let wait_timeout = self.cap_to_wall(wait_timeout);

        let mut response = match self.brain.wait_response(baseline, wait_timeout) {
            Ok(r) => r,
            Err(e) => {
                return BrainTurn {
                    text: format!("{{\"error\": \"{}\"}}", e),
                    complete: false,
                };
            }
        };
        let mut rereads = 0;

        while response.generation_complete
            && protocol::is_possibly_truncated(&response.text)
            && rereads < 3
        {
            if let Some(t) = transcript.as_deref_mut() {
                let mut extra = HashMap::new();
                extra.insert(
                    "fragment".to_string(),
                    serde_json::Value::String(response.text.clone()),
                );
                let _ = t.append(
                    "system",
                    "brain_stream_fragment; rereading same assistant message",
                    extra,
                );
            }
            let reread_timeout = self.cap_to_wall(wait_timeout);
            response = match self.brain.wait_response(baseline, reread_timeout) {
                Ok(r) => r,
                Err(_) => break,
            };
            rereads += 1;
        }

        // Web-UIs ohne belastbares Stop-Signal können bis zum Timeout als
        // `unstable` gelten, obwohl bereits ein syntaktisch vollständiges,
        // streng parsebares Protokolldokument vorliegt. In diesem Fall ist das
        // Protokoll selbst das stärkere Fertigsignal. Ein abgeschnittener Block
        // bleibt ausdrücklich incomplete.
        let protocol_complete = protocol_completes_unstable_response(
            response.generation_complete,
            &response.backend_status,
            &response.text,
        );
        let effective_complete = response.generation_complete || protocol_complete;

        if let Some(t) = transcript {
            let mut extra = HashMap::new();
            extra.insert(
                "complete".to_string(),
                serde_json::Value::String(response.generation_complete.to_string()),
            );
            extra.insert(
                "status".to_string(),
                serde_json::Value::String(response.backend_status.clone()),
            );
            if protocol_complete {
                extra.insert("completion_salvaged_by_protocol".to_string(), true.into());
            }
            let _ = t.append("brain", &response.text, extra);
        }

        if effective_complete {
            self.persist_conversation_ref();
        }
        crate::bench_events::emit(
            if effective_complete {
                crate::bench_events::Level::Pass
            } else {
                crate::bench_events::Level::Warn
            },
            Some(&brain_id),
            &format!("Browser: Antwort empfangen ({})", response.backend_status),
        );

        BrainTurn {
            text: response.text,
            complete: effective_complete,
        }
    }

    /// Beendet Run mit brain_incomplete Status.
    fn finish_brain_incomplete(
        &mut self,
        meta: &mut RunMeta,
        transcript: &mut Transcript,
    ) -> RunMeta {
        meta.status = "brain_incomplete".to_string();
        let _ = self.run_store.save(meta);
        let _ = transcript.append(
            "system",
            &format!(
                "run_finished status={} incomplete_retries={}",
                meta.status, self.incomplete_retries
            ),
            HashMap::new(),
        );
        meta.clone()
    }

    /// Beendet den Run als `brain_unavailable` — das Brain ist extern blockiert
    /// (Anbieter-Limit) oder die Oberfläche liefert keinen Inhalt. Kein
    /// Kompetenz-Fehlschlag, deshalb ein eigener Status, den der Benchmark aus
    /// der Wertung nimmt.
    ///
    /// Fängt qwens Tageslimit ab: ohne diesen Zweig wiederholte der Controller
    /// sechsmal gegen ein Brain, das für zwei Stunden gesperrt war
    /// (Lauf 20260721_225309).
    fn finish_brain_unavailable(
        &mut self,
        meta: &mut RunMeta,
        transcript: &mut Transcript,
        reason: &str,
    ) -> RunMeta {
        meta.status = "brain_unavailable".to_string();
        let _ = self.run_store.save(meta);
        let _ = transcript.append(
            "system",
            &format!(
                "run_finished status=brain_unavailable reason={}",
                crate::char_prefix(reason.trim(), 100)
            ),
            HashMap::new(),
        );
        meta.clone()
    }

    /// Beendet Run mit wall_timeout Status (Gesamt-Deadline überschritten).
    /// Spiegelt die im Loop akkumulierten Felder aus self.meta ins finale meta,
    /// damit der Abbruch keine Fortschrittsdaten (completed_actions, extra) verliert.
    fn finish_wall_timeout(
        &mut self,
        meta: &mut RunMeta,
        transcript: &mut Transcript,
        elapsed: Duration,
        deadline: Duration,
    ) -> RunMeta {
        if let Some(sm) = self.meta.take() {
            meta.conversation_ref = sm.conversation_ref;
            meta.completed_actions = sm.completed_actions;
            for (k, v) in sm.extra {
                meta.extra.insert(k, v);
            }
        }
        meta.status = "wall_timeout".to_string();
        meta.extra.insert(
            "wall_elapsed_s".to_string(),
            serde_json::Value::String(format!("{:.1}", elapsed.as_secs_f64())),
        );
        meta.extra.insert(
            "wall_deadline_s".to_string(),
            serde_json::Value::Number(deadline.as_secs().into()),
        );
        meta.extra.insert(
            "act_steps".to_string(),
            serde_json::Value::Number(self.act_steps.into()),
        );
        let _ = self.run_store.save(meta);
        let _ = transcript.append(
            "system",
            &format!(
                "run_finished status={} wall_elapsed_s={:.1} deadline_s={}",
                meta.status,
                elapsed.as_secs_f64(),
                deadline.as_secs()
            ),
            HashMap::new(),
        );
        meta.clone()
    }
    /// Speichert completed action.
    fn record_completed_action(&mut self, action_id: &str, result: &str) {
        self.completed_actions
            .insert(action_id.to_string(), result.to_string());
        if let Some(meta) = &mut self.meta {
            meta.completed_actions
                .insert(action_id.to_string(), result.to_string());
            let _ = self.run_store.save(meta);
        }
    }

    /// Trackt Observation-Bytes.
    fn track_observation_bytes(&mut self, observation: &str) -> usize {
        if let Some(meta) = &mut self.meta {
            let added = observation.len();
            let total: usize = meta
                .extra
                .get("observation_bytes")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                + added;
            meta.extra.insert(
                "observation_bytes".to_string(),
                serde_json::Value::String(total.to_string()),
            );
            let _ = self.run_store.save(meta);
            total
        } else {
            0
        }
    }

    /// Hakt nach, wenn das Brain „fertig" meldet, obwohl jeder Edit-Versuch
    /// gescheitert ist.
    ///
    /// Den Anstoss gab es bisher nur im Benchmark; `webagent run` — also genau
    /// der Delegationspfad — nahm die Falschmeldung ungeprueft an. Deckel bei
    /// [`MAX_NO_CHANGE_NUDGES`], damit ein Brain, das partout nicht editiert,
    /// den Run nicht endlos verlaengert.
    ///
    /// `None` = kein Anlass zum Nachhaken (oder Deckel erreicht), der Run darf
    /// enden.
    fn no_change_nudge(&mut self) -> Option<String> {
        if self.file_actions_tried == 0 || self.file_writes_ok > 0 {
            return None;
        }
        if self.no_change_nudges >= MAX_NO_CHANGE_NUDGES {
            return None;
        }
        self.no_change_nudges += 1;
        if !self.quiet {
            crate::bench_events::print_line(&format!(
                "[warn] fertig gemeldet, aber kein Edit ist durchgelaufen — \
                 Anstoss {}/{MAX_NO_CHANGE_NUDGES}.",
                self.no_change_nudges
            ));
        }
        Some(
            "[Controller] KEIN EDIT ERKANNT — du meldest fertig, aber JEDER \
             deiner Edit-/Write-Versuche ist fehlgeschlagen (siehe die \
             exit_code-1-Observations oben). Die Datei ist unveraendert. Eine \
             Zusammenfassung oder ein behauptetes Testergebnis zaehlt NICHT. \
             Lies den Ist-Stand der Zieldatei neu ein und kopiere den \
             old_string EXAKT daraus (inkl. Einrueckung), dann gib die \
             Aenderung erneut aus. Behaupte keinen Erfolg, ohne editiert zu \
             haben."
                .to_string(),
        )
    }

    /// Begrenzt Observation auf `config::max_observation_chars()` und
    /// archiviert die vollständige Ausgabe.
    fn bounded_observation(&mut self, action_id: &str, observation: &str) -> String {
        // Kappung nach dem GEMESSENEN Limit dieses Brains, nicht nach einem
        // globalen Schaetzwert — siehe `config::max_observation_chars_for`.
        let cap = max_observation_chars_for(self.brain.brain_id());
        if observation.len() <= cap || self.meta.is_none() {
            return observation.to_string();
        }

        let meta = self.meta.as_ref().unwrap();
        let runs_dir = env::current_dir()
            .unwrap_or_else(|_| env::temp_dir())
            .join("data")
            .join("runs");
        let action_dir = runs_dir.join(&meta.run_id).join("action_output");
        std::fs::create_dir_all(&action_dir).ok();

        let safe_id: String = action_id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || "._-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let artifact = action_dir.join(format!("{}.txt", safe_id));

        std::fs::write(&artifact, observation).ok();

        let head_size = (cap as f64 * 0.65) as usize;
        let tail_size = cap - head_size;
        let omitted = observation.len() - head_size - tail_size;

        format!(
            "{}\n\n[Ausgabe gekürzt: {} Zeichen ausgelassen. Vollständig gespeichert: {}]\n\n{}",
            crate::char_prefix(observation, head_size),
            omitted,
            artifact.display(),
            crate::char_suffix(observation, tail_size)
        )
    }

    /// Führt Actions strikt seriell aus.
    fn execute_actions_serial(
        &mut self,
        actions: &[Action],
        transcript: &mut Transcript,
    ) -> (bool, Vec<String>) {
        let mut observations = Vec::new();
        let mut finished = false;

        for action in actions {
            if self.completed_actions.contains_key(&action.id) {
                let stored = self.completed_actions[&action.id].clone();
                match action.action_type {
                    protocol::ActionType::Shell
                    | protocol::ActionType::Edit
                    | protocol::ActionType::EditBatch
                    | protocol::ActionType::Write => {
                        observations.push(format!(
                            "[Controller] action_id={} wurde bereits ausgefuehrt; \
                             gespeicherte Observation wird erneut geliefert. \
                             Fuer einen korrigierten oder erneut versuchten Befehl \
                             ist eine neue, runweit eindeutige Action-ID erforderlich.\n{}",
                            action.id, stored
                        ));
                    }
                    protocol::ActionType::Finish => {
                        finished = true;
                    }
                    _ => {}
                }
                continue;
            }

            match action.action_type {
                protocol::ActionType::Finish => {
                    if let Some(nudge) = self.no_change_nudge() {
                        observations.push(nudge);
                        continue;
                    }
                    finished = true;
                    let mut extra = HashMap::new();
                    extra.insert(
                        "action_id".to_string(),
                        serde_json::Value::String(action.id.clone()),
                    );
                    let _ = transcript.append("system", "finish", extra);
                    self.record_completed_action(&action.id, "finish");
                    break;
                }
                protocol::ActionType::Message => {
                    let mut extra = HashMap::new();
                    extra.insert(
                        "action_id".to_string(),
                        serde_json::Value::String(action.id.clone()),
                    );
                    let _ = transcript.append("message", &action.text, extra);
                    self.report_step("Antwort");
                    if !self.quiet {
                        crate::bench_events::print_detailed(
                            &format!(
                                "[msg:{}] {}",
                                action.id,
                                crate::char_prefix(&action.text, 60)
                            ),
                            Some(&action.text),
                        );
                    }
                    self.record_completed_action(&action.id, &action.text);
                    if let Some(nudge) = self.no_change_nudge() {
                        observations.push(nudge);
                        continue;
                    }
                    finished = true;
                    break;
                }
                protocol::ActionType::Shell => {
                    self.report_step(&format!(
                        "shell: {}",
                        crate::char_prefix(&action.command, 70)
                    ));
                    let is_read = is_shell_read_action(&action.command);
                    let result = match crate::shell_policy::evaluate(&action.command) {
                        crate::shell_policy::Decision::Deny(reason) => {
                            crate::executor::ExecutionResult {
                                stdout: String::new(),
                                stderr: format!("[shell_policy] verweigert: {reason}"),
                                exit_code: None,
                                timed_out: false,
                                error: Some(format!("shell_policy_denied: {reason}")),
                            }
                        }
                        crate::shell_policy::Decision::Allow => self
                            .executor
                            .execute(&action.command, action.timeout_seconds),
                    };
                    let observation = protocol::format_observation(
                        &action.id,
                        &result.stdout,
                        &result.stderr,
                        result.exit_code,
                        result.timed_out,
                    );
                    let observation = self.bounded_observation(&action.id, &observation);
                    if !self.quiet {
                        crate::bench_events::print_detailed(
                            &format!("[shell:{}] {}", action.id, action.command),
                            Some(&observation),
                        );
                    }
                    observations.push(observation.clone());
                    self.record_completed_action(&action.id, &observation);
                    self.track_observation_bytes(&observation);
                    self.act_steps += 1;

                    if is_read {
                        self.shell_reads_since_write += 1;
                        if self.shell_reads_since_write >= READ_BUDGET_ACTIONS
                            && !self.read_budget_warned
                        {
                            observations.push(read_budget_message(self.shell_reads_since_write));
                            self.read_budget_warned = true;
                        }
                    }

                    // Loop-Guard
                    if let Some(meta) = &mut self.meta {
                        if let Some(fp) = shell_read_fingerprint(&action.command, &observation) {
                            let counts_key = "loop_fingerprints";
                            let mut counts: HashMap<String, usize> = meta
                                .extra
                                .get(counts_key)
                                .and_then(|v| v.as_str())
                                .and_then(|s| serde_json::from_str(s).ok())
                                .unwrap_or_default();

                            let n = counts.entry(fp.clone()).or_insert(0);
                            *n += 1;
                            let count = *n;

                            meta.extra.insert(
                                counts_key.to_string(),
                                serde_json::Value::String(
                                    serde_json::to_string(&counts).unwrap_or_default(),
                                ),
                            );
                            self.run_store.save(meta).ok();

                            if count >= LOOP_GUARD_WARN_COUNT {
                                observations.push(loop_guard_message(&fp, count));
                            }
                            if count >= LOOP_GUARD_ABORT_COUNT {
                                meta.status = "analysis_loop".to_string();
                                self.run_store.save(meta).ok();
                                let _ = transcript.append(
                                    "system",
                                    &format!("analysis_loop fingerprint={} count={}", fp, count),
                                    HashMap::new(),
                                );
                                finished = true;
                                break;
                            }
                        }
                    }
                }
                protocol::ActionType::Edit
                | protocol::ActionType::EditBatch
                | protocol::ActionType::Write => {
                    let action_engine::FileActionResult {
                        kind,
                        target,
                        result,
                    } = action_engine::execute_file_action(self.workspace_root.as_deref(), action);
                    self.report_step(&format!("{kind}: {target}"));
                    self.file_actions_tried += 1;
                    let (stdout, stderr, exit_code) = match result {
                        Ok(msg) => {
                            self.file_writes_ok += 1;
                            self.renew_progress_lease();
                            self.shell_reads_since_write = 0;
                            self.read_budget_warned = false;
                            (msg, String::new(), Some(0))
                        }
                        Err(msg) => (String::new(), msg, Some(1)),
                    };
                    let observation = protocol::format_observation(
                        &action.id, &stdout, &stderr, exit_code, false,
                    );
                    let observation = self.bounded_observation(&action.id, &observation);
                    if !self.quiet {
                        crate::bench_events::print_detailed(
                            &format!("[{kind}:{}] {target}", action.id),
                            Some(&observation),
                        );
                    }
                    observations.push(observation.clone());
                    self.record_completed_action(&action.id, &observation);
                    self.track_observation_bytes(&observation);
                    self.act_steps += 1;
                }
            }
        }

        (finished, observations)
    }

    /// Verarbeitet Brain-Response: Parse, Execute, Feedback-Loop.
    fn handle_response(
        &mut self,
        response_text: &str,
        transcript: &mut Transcript,
    ) -> (String, bool) {
        // Ausfall der Weboberflaeche VOR der Protokollpruefung abfangen: dann
        // hat das Brain nie geantwortet, und ein Repair-Prompt („antworte im
        // richtigen Format") laeuft ins Leere. Real 2026-07-21: zai lieferte
        // nur „No response, Please try again later." plus einen SyntaxError der
        // Seite; der Controller wertete das als Formatfehler und wartete
        // anschliessend 2,5 Minuten auf eine Korrektur, die nicht kommen konnte.
        if crate::brain::is_retryable_empty_response(response_text) {
            let _ = transcript.append(
                "system",
                &format!(
                    "brain_unavailable: Oberflaeche lieferte keine Antwort — {}",
                    crate::char_prefix(response_text.trim(), 120)
                ),
                HashMap::new(),
            );
            return ("brain_unavailable".to_string(), true);
        }

        // #11: fast leere Antwort ohne jede Protokoll-Nutzlast (kein `{`, kein
        // WEBAGENT/1) — das Brain hat nie zu antworten begonnen. Vorher landete
        // das als `protocol_invalid` und kostete einen teuren Repair-Roundtrip.
        // Real gemessen (chatgpt, brain_incomplete): kurze, inhaltslose Texte.
        // Antworten MIT Protokoll-Marker fallen bewusst NICHT hierher, selbst
        // wenn sie kurz sind — eine begonnene Antwort ist reparaturwuerdig.
        if !crate::browser::has_protocol_payload(response_text)
            && response_text.trim().chars().count() < FAST_EMPTY_RESPONSE_CHARS
        {
            let _ = transcript.append(
                "system",
                &format!(
                    "brain_unavailable: fast leere Antwort ohne Protokoll-Nutzlast — {}",
                    crate::char_prefix(response_text.trim(), 120)
                ),
                HashMap::new(),
            );
            return ("brain_unavailable".to_string(), true);
        }

        let parsed = protocol::parse(response_text);

        if !parsed.valid {
            let detail = parsed.error.clone();
            let _ = transcript.append(
                "system",
                &format!("protocol_invalid: {}", detail),
                HashMap::new(),
            );

            let failures: usize = self
                .meta
                .as_ref()
                .and_then(|m| m.extra.get("protocol_error_streak"))
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                + 1;

            if let Some(meta) = &mut self.meta {
                meta.extra.insert(
                    "protocol_error_streak".to_string(),
                    serde_json::Value::String(failures.to_string()),
                );
                self.run_store.save(meta).ok();
            }

            // B3: ein Repair, zweiter Fail → protocol_error (kein Endlos-Retry).
            if protocol::should_abort_protocol_repair(failures) {
                let _ = transcript.append(
                    "system",
                    &format!(
                        "protocol_repair_aborted after={} consecutive errors",
                        failures
                    ),
                    HashMap::new(),
                );
                if let Some(meta) = &mut self.meta {
                    meta.status = "protocol_error".to_string();
                    meta.extra.insert(
                        "protocol_error".to_string(),
                        serde_json::Value::String(detail),
                    );
                    self.run_store.save(meta).ok();
                }
                return (String::new(), false);
            }

            debug_assert!(protocol::should_attempt_protocol_repair(failures));
            let turn = self.run_once(
                &protocol::format_protocol_error_for(&detail, response_text),
                Some(transcript),
            );
            if !turn.complete {
                return (String::new(), false);
            }
            return (turn.text, false);
        }

        // Protocol valid → reset error streak und incomplete retries
        if let Some(meta) = &mut self.meta {
            meta.extra.remove("protocol_error_streak");
            self.run_store.save(meta).ok();
        }
        self.incomplete_retries = 0;

        let (finished, observations) = self.execute_actions_serial(&parsed.actions, transcript);

        if finished {
            return (response_text.to_string(), true);
        }

        if !observations.is_empty() {
            let feedback = protocol::format_observations_bundle(&observations);
            let turn = self.run_once(&feedback, Some(transcript));
            if !turn.complete {
                return (String::new(), false);
            }
            return (turn.text, false);
        }

        // Keine Actions → Fehler
        let turn = self.run_once(
            &protocol::format_protocol_error(
                "Keine ausführbare Action in der letzten gültigen Antwort.",
            ),
            Some(transcript),
        );
        if !turn.complete {
            return (String::new(), false);
        }
        (turn.text, false)
    }
    fn finish_run_cleanup(&mut self, opts: RunOptions) {
        self.wall_deadline_at = None;
        self.wall_hard_deadline_at = None;
        self.wall_lease_secs = 0;
        self.executor.stop();
        if !opts.skip_brain_stop {
            self.brain.stop().ok();
        }
    }

    /// Hauptschleife: run().
    pub fn run(
        &mut self,
        task: &str,
        brain_id: &str,
        resume_id: Option<&str>,
        headless: bool,
    ) -> Result<RunMeta, String> {
        Self::run_with_options(
            self,
            task,
            brain_id,
            resume_id,
            headless,
            RunOptions::default(),
        )
    }

    /// Wie `run`, mit optional offener Browser-Session (REPL-Persistenz).
    pub fn run_with_options(
        &mut self,
        task: &str,
        brain_id: &str,
        resume_id: Option<&str>,
        headless: bool,
        opts: RunOptions,
    ) -> Result<RunMeta, String> {
        self.run_with_continuation(task, brain_id, resume_id, None, headless, opts)
    }

    /// Setzt einen bestehenden Run mit einer konkreten neuen Beobachtung fort.
    /// Browser-Konversation, Transcript und bereits ausgeführte Action-IDs
    /// bleiben erhalten. Das ist insbesondere für Compiler-/Test-Reparaturen
    /// gedacht, die nicht als neue Aufgabe bei null beginnen sollen.
    pub fn continue_run(
        &mut self,
        run_id: &str,
        instruction: &str,
        brain_id: &str,
        headless: bool,
        opts: RunOptions,
    ) -> Result<RunMeta, String> {
        if instruction.trim().is_empty() {
            return Err("Continuation-Anweisung darf nicht leer sein".to_string());
        }
        self.run_with_continuation(
            "",
            brain_id,
            Some(run_id),
            Some(instruction),
            headless,
            opts,
        )
    }

    fn run_with_continuation(
        &mut self,
        task: &str,
        brain_id: &str,
        resume_id: Option<&str>,
        continuation: Option<&str>,
        headless: bool,
        opts: RunOptions,
    ) -> Result<RunMeta, String> {
        let runs_dir = self.runs_dir.clone();
        // Zählt nur die Act-Steps DIESES Aufrufs (REPL-Sessions rufen mehrfach).
        self.act_steps = 0;
        self.file_actions_tried = 0;
        self.file_writes_ok = 0;
        self.shell_reads_since_write = 0;
        self.read_budget_warned = false;
        self.no_change_nudges = 0;

        let (mut meta, mut transcript, task) = if let Some(rid) = resume_id {
            let meta = self.run_store.load(rid)?;
            if meta.brain_id != brain_id {
                return Err(format!(
                    "Resume erfordert brain_id={:?}, erhalten {:?}",
                    meta.brain_id, brain_id
                ));
            }
            let transcript = Transcript::new(&meta, &runs_dir);
            let task = meta.task.clone();
            (meta, transcript, task)
        } else {
            let meta = self.run_store.create(brain_id, task)?;
            let transcript = Transcript::new(&meta, &runs_dir);
            (meta, transcript, task.to_string())
        };

        self.meta = Some(meta.clone());
        self.completed_actions = meta.completed_actions.clone();
        meta.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(std::process::id().into()),
        );
        if continuation.is_some() {
            let count = meta
                .extra
                .get("continuation_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .saturating_add(1);
            meta.extra.insert(
                "continuation_count".to_string(),
                serde_json::Value::Number(count.into()),
            );
            // Auf dem Continuation-Pfad ist dieser Save Teil des Vertrags: geht
            // der strukturierte Zustand verloren, darf der Caller das nicht als
            // erfolgreichen Repair-Versuch missverstehen.
            self.run_store.save(&meta)?;
        } else {
            self.run_store.save(&meta).ok();
        }

        if let Ok(m) = self.comms.send(
            "webagent-rs",
            "self",
            "run_started",
            &format!("task for brain {}", brain_id),
            None,
        ) {
            crate::bench_events::eprint_line(&format!(
                "[comms] run_started id={}",
                &m.id[..8.min(m.id.len())]
            ));
        }

        // Start Brain + Executor (persistent shell session for the whole run)
        if !opts.skip_brain_start {
            self.brain.start(headless).inspect_err(|e| {
                meta.status = "failed".to_string();
                meta.extra.insert(
                    "error_type".to_string(),
                    serde_json::Value::String("RuntimeError".to_string()),
                );
                meta.extra
                    .insert("error".to_string(), serde_json::Value::String(e.clone()));
                self.run_store.save(&meta).ok();
                let mut extra = HashMap::new();
                extra.insert(
                    "error_type".to_string(),
                    serde_json::Value::String("RuntimeError".to_string()),
                );
                extra.insert("error".to_string(), serde_json::Value::String(e.clone()));
                let _ = transcript.append(
                    "system",
                    &format!("run_finished status={}", meta.status),
                    extra,
                );
            })?;
        }
        self.executor.start();

        let ready_timeout =
            crate::timeouts::resolve_timeout("ensure_ready", self.brain.brain_id(), "", None);
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(brain_id),
            "Browser: Sitzung wird geprüft",
        );
        let state = self
            .brain
            .ensure_ready(ready_timeout)
            .unwrap_or(crate::brain::SessionState::Error);
        let _ = transcript.append(
            "system",
            &format!("session_state={:?}", state),
            HashMap::new(),
        );
        crate::bench_events::emit(
            if state == crate::brain::SessionState::Ready {
                crate::bench_events::Level::Pass
            } else {
                crate::bench_events::Level::Warn
            },
            Some(brain_id),
            &format!("Browser: Sitzung {state:?}"),
        );

        if state != crate::brain::SessionState::Ready {
            meta.status = format!("{:?}", state).to_lowercase();
            self.run_store.save(&meta).ok();
            let _ = transcript.append(
                "system",
                &format!("run_finished status={}", meta.status),
                HashMap::new(),
            );
            self.finish_run_cleanup(opts);
            return Ok(meta);
        }

        // Gesamt-Wall-Clock-Deadline des Runs. Wird ab hier (nach Brain-Start /
        // ensure_ready) gemessen und an allen Schleifenköpfen sowie in den
        // Wait/Recover-Zweigen geprüft, damit ein in der Warte-/Sendephase
        // hängendes Brain (kein Fortschritt für max_cycles/loop_guard) nicht
        // endlos läuft. Abbruch ist sauber (Status wall_timeout), kein Panic.
        let wall_started = Instant::now();
        let wall_secs = self
            .wall_secs_override
            .unwrap_or_else(crate::config::max_run_wall_secs);
        let wall_deadline = Duration::from_secs(wall_secs);
        self.wall_lease_secs = wall_secs;
        // Die erste Lease entspricht dem bisherigen Wall-Timeout. Belegter
        // Fortschritt darf sie erneuern; nach spätestens 3 Leases greift die
        // absolute Notbremse.
        self.wall_deadline_at = Some(wall_started + wall_deadline);
        self.wall_hard_deadline_at = Some(wall_started + wall_deadline.saturating_mul(3));

        // Pending response oder Resume oder Initial
        let mut turn = if let Some(resume_id) = resume_id {
            if let Some(pending) = meta.extra.remove("pending_response") {
                let pending_str = pending.as_str().unwrap_or("").to_string();
                let _ = transcript.append("system", "resume_pending_response", HashMap::new());
                self.run_store.save(&meta).ok();
                BrainTurn {
                    text: pending_str,
                    complete: true,
                }
            } else {
                let _ = transcript.append(
                    "system",
                    &format!("resume run {}", resume_id),
                    HashMap::new(),
                );
                self.resume_initial_turn(&mut transcript, continuation)
            }
        } else {
            // Frischer Run: neuen Chat erzwingen, damit die Antworterkennung von
            // einer leeren Konversation ausgeht (baseline=0). Ohne das verfehlt die
            // Erkennung bei Brains mit bestehender Konversation den Antwortbeginn
            // (verifiziert: kimi/mistral liefen erst mit vorherigem new_chat).
            let _ = self.brain.new_chat();

            let memories: Vec<_> = if opts.suppress_memory_context {
                Vec::new()
            } else {
                self.memory
                    .search(&task, &["shared", brain_id], MEMORY_CONTEXT_LIMIT)
                    .unwrap_or_default()
            }
            .into_iter()
            // Alte Episoden können vollständige, normale Chat-Antworten
            // enthalten. Eine darin dokumentierte Protokollverweigerung
            // ist weder Wissen noch nützlicher Kontext, sondern erzeugt
            // bei Web-Chats besonders leicht eine Verweigerungsschleife.
            .filter(|episode| {
                let text = episode.content.to_ascii_lowercase();
                !text.contains("keinen tatsächlichen zugriff")
                    && !text.contains("keine technische kopplung")
                    && !text.contains("keinen zugriff auf dein lokales")
            })
            .collect();
            let mut memory_context: String = memories
                .iter()
                .map(|e| format!("- [memory:{} {}] {}", e.id, e.kind, e.content))
                .collect::<Vec<_>>()
                .join("\n");

            // Wiki-Index als Langzeitwissen anhängen. Fehler (z.B. Verzeichnis
            // nicht anlegbar) liefern einen leeren Block — sie dürfen den Run
            // NIEMALS blockieren.
            let wiki_block = if opts.suppress_memory_context {
                String::new()
            } else {
                self.wiki.context_block(1500).unwrap_or_default()
            };
            if !wiki_block.trim().is_empty() {
                if !memory_context.is_empty() {
                    memory_context.push_str("\n\n");
                }
                memory_context.push_str(
                    "Wiki-Index (Langzeitwissen; Seiten unter data/memory/wiki/, \
per edit/write-Action pflegbar):\n",
                );
                memory_context.push_str(&wiki_block);
            }

            let memory_ids: Vec<u64> = memories.iter().map(|e| e.id).collect();
            meta.extra.insert(
                "memory_ids".to_string(),
                serde_json::Value::String(serde_json::to_string(&memory_ids).unwrap_or_default()),
            );
            self.run_store.save(&meta).ok();

            // Repo-Kontext (Phase 2): begrenzter Dateibaum des Arbeitsverzeichnisses,
            // damit das Brain nicht jede Struktur-Frage per Shell-Roundtrip klärt.
            let mut prompt = autonomous_task_prompt(&task, &memory_context);
            let tree = match &self.workspace_root {
                Some(root) => crate::file_actions::worktree_context_in(root, 120),
                None => crate::file_actions::worktree_context(120),
            };
            if !tree.is_empty() {
                prompt.push_str("\n\n");
                prompt.push_str(&tree);
            }
            self.run_once(&prompt, Some(&mut transcript))
        };

        // Incomplete recovery initial
        while !turn.complete {
            if self.wall_expired() {
                let effective_deadline = self
                    .wall_deadline_at
                    .and_then(|deadline| deadline.checked_duration_since(wall_started))
                    .unwrap_or(wall_deadline);
                let final_meta = self.finish_wall_timeout(
                    &mut meta,
                    &mut transcript,
                    wall_started.elapsed(),
                    effective_deadline,
                );
                self.finish_run_cleanup(opts);
                return Ok(final_meta);
            }
            if crate::brain::is_retryable_empty_response(&turn.text) {
                // Anbieter-Block / kein Inhalt: Wiederholen ist zwecklos (qwen
                // war 2h gesperrt) — sofort als brain_unavailable beenden.
                let final_meta =
                    self.finish_brain_unavailable(&mut meta, &mut transcript, &turn.text);
                self.finish_run_cleanup(opts);
                return Ok(final_meta);
            }
            let incomplete_text = turn.text.clone();
            if let Some(recovered) =
                self.recover_from_incomplete(&mut transcript, "initial", &incomplete_text)
            {
                turn = recovered;
            } else {
                let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                self.finish_run_cleanup(opts);
                return Ok(final_meta);
            }
        }

        let mut response_text = turn.text;
        let mut finished = false;
        let mut cycle = meta.cycles;
        // Heartbeat und Wall-Deadline teilen sich denselben Startzeitpunkt.
        let loop_started = wall_started;
        let mut last_heartbeat = loop_started;
        let heartbeat_interval = Duration::from_secs_f64(CONTROLLER_HEARTBEAT_INTERVAL_SECONDS);

        while !finished
            && (cycle as usize)
                < self.max_cycles
                    + if self.file_writes_ok > 0 {
                        self.max_cycles
                    } else {
                        0
                    }
        {
            // Wall-Clock-Deadline am Schleifenkopf: greift auch, wenn interne
            // Wait/Recover-Zweige lange klemmen (kein Panic, sauberes finish).
            if self.wall_expired() {
                let effective_deadline = self
                    .wall_deadline_at
                    .and_then(|deadline| deadline.checked_duration_since(wall_started))
                    .unwrap_or(wall_deadline);
                let final_meta = self.finish_wall_timeout(
                    &mut meta,
                    &mut transcript,
                    wall_started.elapsed(),
                    effective_deadline,
                );
                self.finish_run_cleanup(opts);
                return Ok(final_meta);
            }
            cycle += 1;
            meta.cycles = cycle;
            self.run_store.save(&meta).ok();

            let now = Instant::now();
            if now.duration_since(last_heartbeat) >= heartbeat_interval {
                let elapsed = now.duration_since(loop_started).as_secs_f64();
                let _ = transcript.append(
                    "system",
                    &format!("heartbeat cycle={} elapsed_s={:.1}", cycle, elapsed),
                    HashMap::new(),
                );
                self.run_store.save(&meta).ok();
                last_heartbeat = now;
            }

            let (new_response, new_finished) =
                self.handle_response(&response_text, &mut transcript);
            response_text = new_response;
            finished = new_finished;

            // brain_unavailable (leer oder fast leer): Run terminal, Benchmark
            // nimmt ihn aus der Wertung — nicht als `done` durchreichen.
            if response_text == "brain_unavailable" {
                let final_meta =
                    self.finish_brain_unavailable(&mut meta, &mut transcript, &response_text);
                self.finish_run_cleanup(opts);
                return Ok(final_meta);
            }

            // Protocol-Repair abgebrochen → Run terminal (kein incomplete-Recovery).
            if self
                .meta
                .as_ref()
                .is_some_and(|m| m.status == "protocol_error")
            {
                break;
            }

            while response_text.is_empty() && !finished {
                if self.wall_expired() {
                    let effective_deadline = self
                        .wall_deadline_at
                        .and_then(|deadline| deadline.checked_duration_since(wall_started))
                        .unwrap_or(wall_deadline);
                    let final_meta = self.finish_wall_timeout(
                        &mut meta,
                        &mut transcript,
                        wall_started.elapsed(),
                        effective_deadline,
                    );
                    self.finish_run_cleanup(opts);
                    return Ok(final_meta);
                }
                if self
                    .meta
                    .as_ref()
                    .is_some_and(|m| m.status == "protocol_error")
                {
                    break;
                }
                if let Some(recovered) =
                    self.recover_from_incomplete(&mut transcript, "cycle", &response_text)
                {
                    if !recovered.complete {
                        let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                        self.finish_run_cleanup(opts);
                        return Ok(final_meta);
                    }
                    let (new_response, new_finished) =
                        self.handle_response(&recovered.text, &mut transcript);
                    response_text = new_response;
                    finished = new_finished;
                    if response_text == "brain_unavailable" {
                        let final_meta = self.finish_brain_unavailable(
                            &mut meta,
                            &mut transcript,
                            &response_text,
                        );
                        self.finish_run_cleanup(opts);
                        return Ok(final_meta);
                    }
                } else {
                    let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                    self.finish_run_cleanup(opts);
                    return Ok(final_meta);
                }
            }

            if self
                .meta
                .as_ref()
                .is_some_and(|m| m.status == "protocol_error")
            {
                break;
            }
        }

        // Die Helfer (persist_conversation_ref, record_completed_action,
        // Observation-/Loop-Zähler) mutieren self.meta. Im Python-Original ist
        // self._meta dasselbe Objekt wie meta; hier ist es ein Clone, daher die
        // helfer-eigenen Felder vor dem finalen Speichern ins lokale meta spiegeln.
        if let Some(sm) = self.meta.take() {
            meta.conversation_ref = sm.conversation_ref;
            meta.completed_actions = sm.completed_actions;
            // protocol_error / streak aus dem Helfer-Meta uebernehmen
            if sm.status == "protocol_error" {
                meta.status = sm.status.clone();
            }
            for (k, v) in sm.extra {
                meta.extra.insert(k, v);
            }
        }

        if meta.status != "protocol_error" {
            meta.status = if finished { "done" } else { "max_cycles" }.to_string();
        }

        // „fertig", obwohl JEDER Edit-Versuch gescheitert ist, ist keine
        // Erledigung, sondern eine Falschmeldung — real 2026-07-24: alle
        // edit-Actions liefen auf `old_string nicht gefunden` (exit 1), das
        // Brain meldete trotzdem fertig samt erfundener Testzahl. Das darf
        // nicht als `done` durchgehen, sonst vertraut der Orchestrator einem
        // leeren Diff.
        if meta.status == "done" && self.file_actions_tried > 0 && self.file_writes_ok == 0 {
            meta.status = "false_done".to_string();
            meta.extra.insert(
                "suspect_no_file_change".to_string(),
                serde_json::Value::Bool(true),
            );
            crate::bench_events::print_line(&format!(
                "[warn] status=false_done: {} Edit/Write-Versuch(e), aber KEINER \
                 erfolgreich — die Fertig-Meldung ist nicht gedeckt. Diff pruefen.",
                self.file_actions_tried
            ));
        }
        meta.extra.insert(
            "file_actions_tried".to_string(),
            serde_json::Value::Number(self.file_actions_tried.into()),
        );
        meta.extra.insert(
            "file_writes_ok".to_string(),
            serde_json::Value::Number(self.file_writes_ok.into()),
        );

        // Beobachtbarkeit für Pfad c (Phantom-/Stale-Done): wie viel wurde
        // wirklich gearbeitet? `done` ohne einen einzigen Act-Step ist bei
        // Arbeitsaufträgen verdächtig (Stale-Antwort aus alter Konversation).
        meta.extra.insert(
            "act_steps".to_string(),
            serde_json::Value::Number(self.act_steps.into()),
        );
        if meta.status == "done" && self.act_steps == 0 {
            meta.extra.insert(
                "suspect_no_actions".to_string(),
                serde_json::Value::Bool(true),
            );
            crate::bench_events::print_line(
                "[warn] status=done ohne ausgeführte Aktionen (act_steps=0) — \
                 Antwort könnte aus einer alten Konversation stammen. Bei \
                 Arbeitsaufträgen Ergebnis-Artefakt prüfen.",
            );
        }

        if finished {
            meta.extra.remove("pending_response");
        } else {
            meta.extra.insert(
                "pending_response".to_string(),
                serde_json::Value::String(response_text),
            );
        }

        if continuation.is_some() {
            self.run_store.save(&meta)?;
        } else {
            self.run_store.save(&meta).ok();
        }

        if meta.status == "done" {
            // Konvertiere RunMeta zu memory::RunMeta
            let memory_meta = crate::memory::RunMeta {
                run_id: meta.run_id.clone(),
                status: meta.status.clone(),
                task: meta.task.clone(),
                completed_actions: meta.completed_actions.clone(),
            };
            if let Ok(Some(memory_id)) = self.memory.record_run(&memory_meta) {
                meta.extra.insert(
                    "episode_memory_id".to_string(),
                    serde_json::Value::Number(memory_id.into()),
                );
                self.run_store.save(&meta).ok();
            }
        }

        let _ = transcript.append(
            "system",
            &format!("run_finished status={}", meta.status),
            HashMap::new(),
        );

        self.finish_run_cleanup(opts);

        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_protocol_salvages_timeout_unstable() {
        let message = r#"{"protocol":"webagent/1","actions":[{"id":"final","type":"message","text":"fertig"}]}"#;
        assert!(protocol_completes_unstable_response(
            false,
            "timeout_unstable",
            message
        ));
    }

    #[test]
    fn truncated_or_other_timeout_is_not_salvaged() {
        let truncated = "WEBAGENT/1 MESSAGE\nid: final\n---MESSAGE---\nfertig";
        assert!(!protocol_completes_unstable_response(
            false,
            "timeout_unstable",
            truncated
        ));
        let complete = r#"{"protocol":"webagent/1","actions":[{"id":"final","type":"message","text":"fertig"}]}"#;
        assert!(!protocol_completes_unstable_response(
            false,
            "timeout_still_generating",
            complete
        ));
    }
    use crate::brain::{BrainResponse, SessionState};
    use crate::executor::ExecutionResult;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Eindeutiges Daten-Verzeichnis pro Testaufruf — Tests laufen parallel und
    /// dürfen sich nicht dieselben runs/memory teilen (Ersatz für Python-monkeypatch).
    fn unique_data_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_controller_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    struct MockBrain {
        brain_id: String,
        messages: Rc<RefCell<Vec<String>>>,
        responses: Vec<String>,
        complete_flags: Vec<bool>,
        conversation_ref: Rc<RefCell<Option<String>>>,
        restore_calls: Rc<RefCell<Vec<String>>>,
        restore_result: bool,
        session_state_value: SessionState,
        new_chat_calls: Rc<RefCell<usize>>,
        started: Rc<RefCell<bool>>,
        response_index: Rc<RefCell<usize>>,
        /// Künstliche Verzögerung je wait_response (Wall-Timeout-Test).
        wait_sleep: Duration,
        /// Wenn true: bei jedem Turn eine frische, gültige shell-Action mit
        /// eindeutiger id liefern (nie abschließen) — simuliert einen Run, der
        /// nur durch die Wall-Deadline gestoppt werden kann.
        loop_shell: bool,
    }

    impl MockBrain {
        fn new() -> Self {
            Self {
                brain_id: "mock".to_string(),
                messages: Rc::new(RefCell::new(Vec::new())),
                responses: Vec::new(),
                complete_flags: Vec::new(),
                conversation_ref: Rc::new(RefCell::new(Some(
                    "https://example.test/chat/abc".to_string(),
                ))),
                restore_calls: Rc::new(RefCell::new(Vec::new())),
                restore_result: true,
                session_state_value: SessionState::Ready,
                new_chat_calls: Rc::new(RefCell::new(0)),
                started: Rc::new(RefCell::new(false)),
                response_index: Rc::new(RefCell::new(0)),
                wait_sleep: Duration::ZERO,
                loop_shell: false,
            }
        }

        fn with_responses(mut self, responses: Vec<&str>, complete: Vec<bool>) -> Self {
            self.responses = responses.iter().map(|s| s.to_string()).collect();
            self.complete_flags = complete;
            self
        }

        /// Endloser Fortschritt mit Verzögerung: jeder Turn eine frische
        /// shell-Action, `sleep` pro wait_response. Nur die Wall-Deadline stoppt.
        fn with_wall_stall(mut self, sleep: Duration) -> Self {
            self.wait_sleep = sleep;
            self.loop_shell = true;
            self
        }

        /// Wie oft `send` aufgerufen wurde — für Tests, die belegen wollen, dass
        /// nicht sinnlos wiederholt wurde.
        fn sent_message_count(&self) -> usize {
            self.messages.borrow().len()
        }
    }

    impl BrainBackend for MockBrain {
        fn brain_id(&self) -> &str {
            &self.brain_id
        }

        fn start(&mut self, _headless: bool) -> Result<(), String> {
            *self.started.borrow_mut() = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), String> {
            *self.started.borrow_mut() = false;
            Ok(())
        }

        fn ensure_ready(&mut self, _timeout: f64) -> Result<SessionState, String> {
            Ok(self.session_state_value)
        }

        fn session_state(&self) -> SessionState {
            self.session_state_value
        }

        fn new_chat(&mut self) -> Result<(), String> {
            *self.new_chat_calls.borrow_mut() += 1;
            *self.conversation_ref.borrow_mut() = Some("https://example.test/chat/new".to_string());
            Ok(())
        }

        fn send(&mut self, text: &str) -> Result<i32, String> {
            self.messages.borrow_mut().push(text.to_string());
            Ok(0)
        }

        fn wait_response(
            &mut self,
            _baseline_count: i32,
            _timeout: f64,
        ) -> Result<BrainResponse, String> {
            if !self.wait_sleep.is_zero() {
                std::thread::sleep(self.wait_sleep);
            }
            let idx = *self.response_index.borrow();
            *self.response_index.borrow_mut() = idx + 1;
            let (text, complete) = if self.loop_shell {
                (
                    shell_response(&format!("wall-{idx}"), "Write-Output tick"),
                    true,
                )
            } else {
                let text = self
                    .responses
                    .get(idx)
                    .cloned()
                    .unwrap_or_else(|| "{}".to_string());
                let complete = self.complete_flags.get(idx).copied().unwrap_or(true);
                (text, complete)
            };
            Ok(BrainResponse {
                text,
                message_index: idx as i32,
                generation_complete: complete,
                backend_status: "ok".to_string(),
                raw_html: String::new(),
            })
        }

        fn is_logged_in(&self) -> bool {
            true
        }

        fn click_login(&mut self) -> Result<(), String> {
            Ok(())
        }

        fn wait_for_login(&mut self, _poll_interval: f64) -> Result<(), String> {
            Ok(())
        }

        fn get_conversation_ref(&self) -> Option<String> {
            self.conversation_ref.borrow().clone()
        }

        fn restore_conversation(&mut self, ref_val: &str) -> Result<bool, String> {
            self.restore_calls.borrow_mut().push(ref_val.to_string());
            if self.restore_result {
                *self.conversation_ref.borrow_mut() = Some(ref_val.to_string());
            }
            Ok(self.restore_result)
        }
    }

    struct MockExecutor {
        commands: Rc<RefCell<Vec<String>>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                commands: Rc::new(RefCell::new(Vec::new())),
            }
        }
    }

    impl ShellExecutor for MockExecutor {
        fn execute(&self, command: &str, _timeout: f64) -> ExecutionResult {
            self.commands.borrow_mut().push(command.to_string());
            ExecutionResult {
                stdout: format!("out:{}", command),
                stderr: String::new(),
                exit_code: Some(0),
                timed_out: false,
                error: None,
            }
        }
    }

    fn finish_response() -> String {
        serde_json::json!({
            "protocol": "webagent/1",
            "actions": [{"id": "done-1", "type": "finish"}]
        })
        .to_string()
    }

    /// Edit-Action auf eine Datei, deren `old_string` garantiert NICHT
    /// vorkommt — bildet den realen Fall nach: Anker daneben, exit 1.
    fn failing_edit_response(action_id: &str, path: &str) -> String {
        serde_json::json!({
            "protocol": "webagent/1",
            "actions": [{
                "id": action_id,
                "type": "edit",
                "path": path,
                "old_string": "diesen-anker-gibt-es-nicht",
                "new_string": "egal"
            }]
        })
        .to_string()
    }

    fn successful_edit_response(action_id: &str, path: &str, old: &str, new: &str) -> String {
        serde_json::json!({
            "protocol": "webagent/1",
            "actions": [{
                "id": action_id,
                "type": "edit",
                "path": path,
                "old_string": old,
                "new_string": new
            }]
        })
        .to_string()
    }

    fn shell_response(action_id: &str, command: &str) -> String {
        serde_json::json!({
            "protocol": "webagent/1",
            "actions": [{
                "id": action_id,
                "type": "shell",
                "command": command,
                "timeout_seconds": 30
            }]
        })
        .to_string()
    }

    /// Legt eine Datei an, deren Inhalt den Test-Anker garantiert nicht enthaelt.
    fn datei_ohne_anker(data_dir: &std::path::Path) -> String {
        let p = data_dir.join("ziel.txt");
        std::fs::create_dir_all(data_dir).ok();
        std::fs::write(&p, "unveraenderter inhalt\n").unwrap();
        p.to_string_lossy().to_string()
    }

    /// Der reale Fall vom 2026-07-24: jeder Edit scheitert am Anker, das Brain
    /// meldet trotzdem fertig. Das darf nicht als `done` durchgehen.
    #[test]
    fn fertig_trotz_gescheiterter_edits_ist_false_done() {
        let data_dir = unique_data_dir();
        let ziel = datei_ohne_anker(&data_dir);
        let edit = failing_edit_response("e-1", &ziel);
        // Genug Finish-Antworten, um beide Anstoesse abzuarbeiten.
        let brain = MockBrain::new().with_responses(
            vec![
                &edit,
                &finish_response(),
                &finish_response(),
                &finish_response(),
            ],
            vec![true, true, true, true],
        );
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 10, data_dir.clone());

        let meta = controller
            .run("Aendere die Datei", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "false_done", "meta.extra={:?}", meta.extra);
        assert_eq!(
            meta.extra.get("suspect_no_file_change"),
            Some(&serde_json::Value::Bool(true))
        );
        assert_eq!(
            meta.extra.get("file_writes_ok"),
            Some(&serde_json::Value::Number(0.into()))
        );
        // Die Datei ist wirklich unangetastet.
        assert_eq!(
            std::fs::read_to_string(&ziel).unwrap(),
            "unveraenderter inhalt\n"
        );
    }

    #[test]
    fn expliziter_workspace_bindet_relative_edit_action() {
        let data_dir = unique_data_dir();
        let workspace = unique_data_dir().join("workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("ziel.txt"), "vorher\n").unwrap();
        let edit = successful_edit_response("e-1", "ziel.txt", "vorher", "nachher");
        let brain =
            MockBrain::new().with_responses(vec![&edit, &finish_response()], vec![true, true]);
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());
        controller.set_workspace_root(workspace.clone());

        let meta = controller
            .run("Aendere die relative Datei", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(
            std::fs::read_to_string(workspace.join("ziel.txt")).unwrap(),
            "nachher\n"
        );
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::remove_dir_all(workspace);
    }

    /// Der Anstoss darf nicht endlos nachhaken — sonst haengt ein Brain, das
    /// partout nicht editiert, den Run bis zum Wall-Timeout auf.
    #[test]
    fn anstoss_bei_fertig_ohne_edit_ist_gedeckelt() {
        let data_dir = unique_data_dir();
        let ziel = datei_ohne_anker(&data_dir);
        let edit = failing_edit_response("e-1", &ziel);
        let brain = MockBrain::new().with_responses(
            vec![
                &edit,
                &finish_response(),
                &finish_response(),
                &finish_response(),
                &finish_response(),
            ],
            vec![true, true, true, true, true],
        );
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 20, data_dir.clone());

        let meta = controller
            .run("Aendere die Datei", "mock", None, false)
            .unwrap();

        // Der Run endet (kein Endlos-Nachhaken) und ist als Falschmeldung markiert.
        assert_eq!(meta.status, "false_done");
    }

    /// Gegenprobe: ein Run ohne jeden Datei-Versuch (reine Frage) bleibt `done`.
    /// Der Waechter darf nur bei GESCHEITERTEN Edits zuschlagen.
    #[test]
    fn reine_antwort_ohne_dateiaktion_bleibt_done() {
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, unique_data_dir());

        let meta = controller
            .run("Nur eine Frage", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "done");
    }

    #[test]
    fn test_conversation_ref_persisted_after_complete_brain_response() {
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);

        let executor = MockExecutor::new();
        let data_dir = unique_data_dir();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, data_dir.clone());

        let meta = controller.run("Testaufgabe", "mock", None, false).unwrap();
        assert_eq!(meta.status, "done");

        // Ein frischer Run legt einen neuen Chat an; dessen conversation_ref
        // (vom Mock-new_chat gesetzt) muss nach Abschluss persistiert sein.
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let store = RunStore::new(runs_dir, logs_dir);
        let reloaded = store.load(&meta.run_id).unwrap();
        assert_eq!(
            reloaded.conversation_ref,
            Some("https://example.test/chat/new".to_string())
        );
    }

    #[test]
    fn test_successful_run_records_episode_once() {
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, unique_data_dir());

        let meta = controller
            .run("Merke diesen Testlauf", "mock", None, false)
            .unwrap();

        let episodes: Vec<_> = controller
            .memory
            .list(100)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| e.kind == "episode")
            .collect();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].source, format!("run:{}", meta.run_id));
        let expected_id = serde_json::Value::Number(episodes[0].id.into());
        assert_eq!(meta.extra.get("episode_memory_id"), Some(&expected_id));
    }

    #[test]
    fn benchmark_option_unterdrueckt_alte_episoden_im_prompt() {
        let data_dir = unique_data_dir();
        let memory = MemoryStore::new(data_dir.join("memory.jsonl"));
        memory
            .add(
                "ALTER_AUSFUEHRBARER_PLAN brain.rs C:/falscher/checkout",
                "shared",
                "episode",
                Some("test:stale"),
                1.0,
            )
            .unwrap();
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let messages = brain.messages.clone();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());

        controller
            .run_with_options(
                "Aendere brain.rs",
                "mock",
                None,
                false,
                RunOptions {
                    suppress_memory_context: true,
                    ..RunOptions::default()
                },
            )
            .unwrap();

        let sent = messages.borrow().join("\n");
        assert!(!sent.contains("ALTER_AUSFUEHRBARER_PLAN"), "{sent}");
        assert!(sent.contains("Aendere brain.rs"), "aktuelle Aufgabe fehlt");
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn expliziter_workspace_bindet_auch_prompt_dateibaum() {
        let data_dir = unique_data_dir();
        let workspace = unique_data_dir().join("benchmark-workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("NUR_IM_BENCHMARK.txt"), "test\n").unwrap();
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let messages = brain.messages.clone();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());
        controller.set_workspace_root(workspace.clone());

        controller
            .run_with_options(
                "Arbeite im gebundenen Workspace",
                "mock",
                None,
                false,
                RunOptions {
                    suppress_memory_context: true,
                    ..RunOptions::default()
                },
            )
            .unwrap();

        let sent = messages.borrow().join("\n");
        assert!(sent.contains("NUR_IM_BENCHMARK.txt"), "{sent}");
        assert!(sent.contains(&workspace.display().to_string()), "{sent}");
        let _ = std::fs::remove_dir_all(data_dir);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn test_done_without_actions_is_flagged_suspect() {
        // Pfad c (gemini 2026-07-20): sofortiges finish ohne einen Act-Step →
        // done bleibt done, aber act_steps=0 + suspect_no_actions=true.
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, unique_data_dir());
        let meta = controller.run("Arbeite!", "mock", None, false).unwrap();
        assert_eq!(meta.status, "done");
        assert_eq!(
            meta.extra.get("act_steps"),
            Some(&serde_json::Value::Number(0.into()))
        );
        assert_eq!(
            meta.extra.get("suspect_no_actions"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn test_done_with_actions_not_flagged() {
        let brain = MockBrain::new().with_responses(
            vec![&shell_response("s1", "Write-Output ok"), &finish_response()],
            vec![true, true],
        );
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, unique_data_dir());
        let meta = controller.run("Arbeite!", "mock", None, false).unwrap();
        assert_eq!(meta.status, "done");
        assert_eq!(
            meta.extra.get("act_steps"),
            Some(&serde_json::Value::Number(1.into()))
        );
        assert_eq!(meta.extra.get("suspect_no_actions"), None);
    }

    #[test]
    fn variierende_leseschleife_bekommt_umsetzungs_nudge() {
        let reads: Vec<String> = (1..=5)
            .map(|n| shell_response(&format!("read-{n}"), &format!("Get-Content src/x{n}.rs")))
            .collect();
        let refs: Vec<&str> = reads.iter().map(String::as_str).collect();
        let brain = MockBrain::new().with_responses(refs, vec![true; 5]);
        let messages = brain.messages.clone();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, unique_data_dir());

        let meta = controller
            .run("Implementiere eine Aenderung", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "max_cycles");
        let sent = messages.borrow();
        assert!(
            sent.iter()
                .any(|m| m.contains("LESE-CHECKPOINT") && m.contains("Weitere gezielte Reads")),
            "Umsetzungs-Nudge fehlt: {sent:?}"
        );
    }

    #[test]
    fn erfolgreicher_write_erweitert_statt_kappt_das_zyklenbudget() {
        let workspace = unique_data_dir().join("progress-workspace");
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(workspace.join("ziel.txt"), "vorher\n").unwrap();
        let edit = successful_edit_response("edit-progress", "ziel.txt", "vorher", "nachher");
        let shell = shell_response("test-progress", "Write-Output test-ok");
        let brain = MockBrain::new().with_responses(
            vec![&edit, &shell, &finish_response()],
            vec![true, true, true],
        );
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 2, unique_data_dir());
        controller.set_workspace_root(workspace.clone());

        let meta = controller
            .run("Editiere, teste und beende", "mock", None, false)
            .unwrap();

        assert_eq!(
            meta.status, "done",
            "Fortschritt muss Zusatzzyklen erhalten"
        );
        assert_eq!(meta.cycles, 3);
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn test_duplicate_action_id_not_reexecuted() {
        let brain = MockBrain::new().with_responses(
            vec![
                &shell_response("dup-1", "Write-Output first"),
                &shell_response("dup-1", "Write-Output second"),
                &finish_response(),
            ],
            vec![true, true, true],
        );
        let executor = MockExecutor::new();
        let commands = executor.commands.clone();

        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller.run("Dedupe", "mock", None, false).unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(commands.borrow().len(), 1);
        assert_eq!(commands.borrow()[0], "Write-Output first");
        assert!(meta.completed_actions.contains_key("dup-1"));
    }

    #[test]
    fn test_protocol_repair_recovers_after_one_invalid_answer() {
        // Kurze Nicht-Protokoll-Antworten (< 20 Zeichen ohne Marker) sind seit
        // #11 brain_unavailable — der Repair-Pfad wird mit einer LAENGEREN
        // fehlgeformten Antwort geprueft, die die Nutzlast begonnen hat.
        let brain = MockBrain::new().with_responses(
            vec![
                "das hier ist leider kein gueltiges json format",
                &finish_response(),
            ],
            vec![true, true],
        );
        let executor = MockExecutor::new();
        let messages = brain.messages.clone();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller
            .run("Repariere Protokoll", "mock", None, false)
            .unwrap();
        assert_eq!(meta.status, "done");
        // Mindestens ein Repair-Prompt an das Brain
        let sent = messages.borrow();
        assert!(
            sent.iter().any(|m| {
                m.contains("NUR mit gültigem")
                    || (m.contains("webagent/1") && m.contains("Ungültige"))
            }),
            "expected protocol repair prompt, got: {sent:?}"
        );
    }

    #[test]
    fn test_protocol_repair_aborts_as_protocol_error_after_third_fail() {
        let brain = MockBrain::new().with_responses(
            vec![
                "das hier ist kaputt eins und kein json",
                "das hier ist kaputt zwei und kein json",
                "das hier ist kaputt drei und kein json",
                &finish_response(),
            ],
            vec![true, true, true, true],
        );
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller
            .run("Drei Parse-Fails", "mock", None, false)
            .unwrap();
        assert_eq!(meta.status, "protocol_error");
        assert_eq!(
            meta.extra
                .get("protocol_error_streak")
                .and_then(|v| v.as_str()),
            Some("3")
        );
    }

    #[test]
    fn test_shell_policy_denies_destructive_command_without_executing() {
        let brain = MockBrain::new().with_responses(
            vec![
                &shell_response("del-1", "Remove-Item C:\\data -Recurse -Force"),
                &finish_response(),
            ],
            vec![true, true],
        );
        let executor = MockExecutor::new();
        let commands = executor.commands.clone();

        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller
            .run("Loesche alles", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "done");
        // Der Executor darf den destruktiven Befehl nie zu Gesicht bekommen.
        assert!(commands.borrow().is_empty());
        let observation = meta.completed_actions.get("del-1").expect("observation");
        assert!(
            observation.contains("shell_policy"),
            "erwarte Policy-Hinweis in der Observation, war: {observation}"
        );
    }

    #[test]
    fn test_resume_restores_conversation_when_possible() {
        let data_dir = unique_data_dir();
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let store = RunStore::new(runs_dir, logs_dir);
        let mut meta = store.create("mock", "Fortsetzen").unwrap();
        // Production-like host (not example.test / reserved mock TLDs).
        let live_ref = "https://chatgpt.com/c/old-session".to_string();
        meta.conversation_ref = Some(live_ref.clone());
        meta.completed_actions.insert(
            "prev-1".to_string(),
            "[Terminal-Ausgabe action_id=prev-1]\nold".to_string(),
        );
        store.save(&meta).ok();

        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let restore_calls = brain.restore_calls.clone();
        let new_chat_calls = brain.new_chat_calls.clone();

        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, data_dir);

        let result = controller
            .run("ignored", "mock", Some(&meta.run_id), false)
            .unwrap();

        assert_eq!(result.status, "done");
        assert_eq!(restore_calls.borrow().as_slice(), &[live_ref.as_str()]);
        assert_eq!(*new_chat_calls.borrow(), 0);
    }

    #[test]
    fn continuation_sends_concrete_instruction_in_restored_conversation() {
        let data_dir = unique_data_dir();
        let store = RunStore::new(data_dir.join("runs"), data_dir.join("logs"));
        let mut meta = store.create("mock", "Implementiere die Funktion").unwrap();
        let live_ref = "https://chatgpt.com/c/repair-session".to_string();
        meta.conversation_ref = Some(live_ref.clone());
        store.save(&meta).unwrap();

        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let messages = brain.messages.clone();
        let restore_calls = brain.restore_calls.clone();
        let new_chat_calls = brain.new_chat_calls.clone();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);

        let result = controller
            .continue_run(
                &meta.run_id,
                "cargo test scheitert: expected 2, got 1",
                "mock",
                false,
                RunOptions::default(),
            )
            .unwrap();

        assert_eq!(result.run_id, meta.run_id);
        assert_eq!(result.extra["continuation_count"].as_u64(), Some(1));
        assert_eq!(restore_calls.borrow().as_slice(), &[live_ref.as_str()]);
        assert_eq!(*new_chat_calls.borrow(), 0);
        let sent = messages.borrow();
        assert!(sent[0].contains("CONTINUATION_INSTRUCTION"), "{sent:?}");
        assert!(sent[0].contains("expected 2, got 1"), "{sent:?}");
    }

    #[test]
    fn continuation_fallback_reconstructs_task_and_current_instruction() {
        let data_dir = unique_data_dir();
        let store = RunStore::new(data_dir.join("runs"), data_dir.join("logs"));
        let mut meta = store.create("mock", "Urspruengliche Aufgabe").unwrap();
        meta.conversation_ref = Some("https://chatgpt.com/c/lost-session".to_string());
        store.save(&meta).unwrap();

        let mut brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        brain.restore_result = false;
        let messages = brain.messages.clone();
        let new_chat_calls = brain.new_chat_calls.clone();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);

        controller
            .continue_run(
                &meta.run_id,
                "Buildfehler E0425 reparieren",
                "mock",
                false,
                RunOptions::default(),
            )
            .unwrap();

        assert!(*new_chat_calls.borrow() >= 1);
        let sent = messages.borrow();
        assert!(sent[0].contains("Urspruengliche Aufgabe"), "{sent:?}");
        assert!(sent[0].contains("PRIOR_TRANSCRIPT"), "{sent:?}");
        assert!(sent[0].contains("Buildfehler E0425 reparieren"), "{sent:?}");
    }

    #[test]
    fn test_resume_rejects_example_test_conversation_ref() {
        let data_dir = unique_data_dir();
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let store = RunStore::new(runs_dir, logs_dir);
        let mut meta = store.create("mock", "Fortsetzen mit mock-ref").unwrap();
        meta.conversation_ref = Some("https://example.test/chat/old".to_string());
        store.save(&meta).ok();

        // One response for the new_chat recovery path (restore must not succeed).
        let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
        let restore_calls = brain.restore_calls.clone();
        let new_chat_calls = brain.new_chat_calls.clone();

        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, data_dir);

        let result = controller
            .run("ignored", "mock", Some(&meta.run_id), false)
            .unwrap();

        assert_eq!(result.status, "done");
        assert!(
            restore_calls.borrow().is_empty(),
            "example.test must not call restore_conversation: {:?}",
            restore_calls.borrow()
        );
        assert!(
            *new_chat_calls.borrow() >= 1,
            "invalid conversation_ref must force new_chat fallback"
        );
    }

    #[test]
    fn test_is_valid_resume_conversation_ref() {
        assert!(!is_valid_resume_conversation_ref(""));
        assert!(!is_valid_resume_conversation_ref("not-a-url"));
        assert!(!is_valid_resume_conversation_ref("ftp://chatgpt.com/c/x"));
        assert!(!is_valid_resume_conversation_ref(
            "https://example.test/chat/old"
        ));
        assert!(!is_valid_resume_conversation_ref("https://example.com/x"));
        assert!(!is_valid_resume_conversation_ref("http://localhost:9222/"));
        assert!(!is_valid_resume_conversation_ref("https://foo.test/bar"));
        assert!(is_valid_resume_conversation_ref(
            "https://chatgpt.com/c/abc123"
        ));
        assert!(is_valid_resume_conversation_ref(
            "https://gemini.google.com/app/xyz"
        ));
        assert!(is_valid_resume_conversation_ref(
            "http://chat.deepseek.com/a/chat/s/1"
        ));
    }

    #[test]
    fn test_resume_rejects_mismatched_brain_id() {
        let data_dir = unique_data_dir();
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let store = RunStore::new(runs_dir, logs_dir);
        let meta = store.create("mock", "Brain mismatch").unwrap();
        store.save(&meta).ok();

        let brain = MockBrain::new();
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 5, data_dir);

        let result = controller.run("x", "other", Some(&meta.run_id), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("brain_id"));
    }

    #[test]
    fn test_incomplete_retry_backoff_doubles_and_caps() {
        // 1-basiert: BASE * 2^(n-1), gedeckelt bei CAP=8s. retry 0 == retry 1.
        assert_eq!(incomplete_retry_backoff(0), Duration::from_millis(500));
        assert_eq!(incomplete_retry_backoff(1), Duration::from_millis(500));
        assert_eq!(incomplete_retry_backoff(2), Duration::from_millis(1000));
        assert_eq!(incomplete_retry_backoff(3), Duration::from_millis(2000));
        assert_eq!(incomplete_retry_backoff(4), Duration::from_millis(4000));
        assert_eq!(incomplete_retry_backoff(5), Duration::from_millis(8000));
        // Cap eingehalten, kein Overflow bei großen retry-Indizes.
        assert_eq!(incomplete_retry_backoff(6), Duration::from_millis(8000));
        assert_eq!(incomplete_retry_backoff(50), Duration::from_millis(8000));
        assert_eq!(
            incomplete_retry_backoff(usize::MAX),
            Duration::from_millis(8000)
        );
    }

    #[test]
    fn test_wall_timeout_aborts_runaway_loop() {
        // Ein Run, der nie abschließt (endlose frische shell-Actions mit
        // Verzögerung) und dessen max_cycles praktisch unerreichbar ist, muss
        // durch die Wall-Deadline sauber als wall_timeout beendet werden.
        // WICHTIG: keine prozessglobale Env-Variable setzen — das brach parallel
        // laufende Tests (deren Runs liefen in diese 1s-Deadline und endeten als
        // wall_timeout statt done). Deadline pro Controller injizieren.
        let brain = MockBrain::new().with_wall_stall(Duration::from_millis(300));
        let executor = MockExecutor::new();
        let mut controller =
            AgentController::with_data_dir(brain, executor, 1_000_000, unique_data_dir());
        controller.set_wall_timeout_secs(1);
        let meta = controller
            .run("Endlosschleife", "mock", None, false)
            .unwrap();

        assert_eq!(meta.status, "wall_timeout");
        assert_eq!(
            meta.extra.get("wall_deadline_s"),
            Some(&serde_json::Value::Number(1.into()))
        );
        assert!(
            meta.extra.contains_key("wall_elapsed_s"),
            "wall_elapsed_s sollte gesetzt sein"
        );
    }
    #[test]
    fn cap_to_wall_limits_wait_to_remaining_budget() {
        // Der Kern des 409s-statt-300s-Bugs: ein Turn mit langem Per-Brain-
        // Timeout darf die Gesamtfrist nicht ueberziehen.
        let brain = MockBrain::new();
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());

        // 2s Restbudget, aber ein Turn wollte 100s warten -> gedeckelt.
        controller.wall_deadline_at = Some(Instant::now() + Duration::from_secs(2));
        let capped = controller.cap_to_wall(100.0);
        assert!(capped <= 2.0 && capped > 0.5, "erwartet ~2s, war {capped}");

        // Kuerzer als das Restbudget bleibt unveraendert.
        assert_eq!(controller.cap_to_wall(0.5), 0.5);
    }

    #[test]
    fn cap_to_wall_is_inert_without_an_active_deadline() {
        // Ausserhalb eines Run-Loops (z.B. direkter run_once-Aufruf) darf der
        // Deckel nichts kappen, sonst verkuerzt er legitime Wartezeiten.
        let brain = MockBrain::new();
        let executor = MockExecutor::new();
        let controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        assert_eq!(controller.wall_deadline_at, None);
        assert_eq!(controller.cap_to_wall(100.0), 100.0);
    }

    #[test]
    fn erfolgreicher_fortschritt_erneuert_lease_bis_zur_notbremse() {
        let data_dir = unique_data_dir();
        let brain = MockBrain::new();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);
        let now = Instant::now();
        controller.wall_deadline_at = Some(now + Duration::from_secs(1));
        controller.wall_hard_deadline_at = Some(now + Duration::from_secs(10));
        controller.wall_lease_secs = 5;

        controller.renew_progress_lease();

        let renewed = controller.wall_deadline_at.expect("erneuerte Lease");
        assert!(renewed >= now + Duration::from_secs(4));
        assert!(renewed <= now + Duration::from_secs(10));
    }

    #[test]
    fn fortschritts_lease_ueberschreitet_absolute_notbremse_nicht() {
        let data_dir = unique_data_dir();
        let brain = MockBrain::new();
        let mut controller =
            AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);
        let now = Instant::now();
        let hard = now + Duration::from_secs(2);
        controller.wall_deadline_at = Some(now + Duration::from_secs(1));
        controller.wall_hard_deadline_at = Some(hard);
        controller.wall_lease_secs = 60;

        controller.renew_progress_lease();

        assert_eq!(controller.wall_deadline_at, Some(hard));
    }

    #[test]
    fn cap_to_wall_keeps_a_one_second_floor_past_the_deadline() {
        // Schon ueberfaellig: nicht 0s zurueckgeben (sofortiger Fehlschlag),
        // sondern 1s Restfrist — der naechste Schleifenkopf beendet den Run
        // dann regulaer als wall_timeout.
        let brain = MockBrain::new();
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        controller.wall_deadline_at = Some(Instant::now() - Duration::from_secs(5));
        assert_eq!(controller.cap_to_wall(100.0), 1.0);
    }

    #[test]
    fn provider_daily_limit_ends_as_unavailable_without_retrying() {
        // qwen (Lauf 20260721_225309) antwortete mit dem Tageslimit und
        // generation_complete=false, worauf der Controller sechsmal wiederholte
        // und den Fehlschlag gegen qwen wertete. Jetzt: EIN Versuch, Status
        // brain_unavailable, den der Benchmark aus der Wertung nimmt.
        let block = "Oops! There was an issue connecting to Qwen3.7-Plus.
                     You have reached the daily usage limit. Please wait 2 hours before trying again.";
        let brain = MockBrain::new().with_responses(vec![block], vec![false]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 15, unique_data_dir());
        let meta = controller.run("baue etwas", "qwen", None, false).unwrap();

        assert_eq!(meta.status, "brain_unavailable");
        assert!(
            crate::benchmark::is_external_block(&meta.status),
            "muss aus der Wertung fallen"
        );
        // Genau ein Sende-Versuch — keine sechs Wiederholungen.
        assert_eq!(controller.brain().sent_message_count(), 1);
    }

    #[test]
    fn fast_leere_antwort_ohne_protokoll_marker_ist_brain_unavailable() {
        // #11: chatgpt brain_incomplete-Fall — kurzer Text ohne `{`/WEBAGENT/1.
        // Vorher protocol_invalid + teurer Repair-Roundtrip; jetzt brain_unavailable.
        let brain = MockBrain::new().with_responses(vec!["Hmm, weiß nicht."], vec![true]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller.run("Antworte", "mock", None, false).unwrap();

        assert_eq!(meta.status, "brain_unavailable");
        assert!(
            crate::benchmark::is_external_block(&meta.status),
            "muss aus der Wertung fallen"
        );
        // Genau ein Sende-Versuch — kein Repair-Prompt, kein status=done.
        assert_eq!(controller.brain().sent_message_count(), 1);
    }

    #[test]
    fn kurze_antwort_mit_protokoll_marker_geht_in_repair() {
        // Kurz, aber das Brain HAT begonnen zu antworten (WEBAGENT/1-Marker
        // vorhanden, nur kaputt) → reparaturwuerdig, nicht brain_unavailable.
        let brain = MockBrain::new().with_responses(
            vec!["WEBAGENT/1 BROKEN", &finish_response()],
            vec![true, true],
        );
        let executor = MockExecutor::new();
        let messages = brain.messages.clone();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller.run("Repariere", "mock", None, false).unwrap();

        assert_eq!(meta.status, "done");
        let sent = messages.borrow();
        assert!(
            sent.iter().any(|m| {
                m.contains("NUR mit gültigem")
                    || (m.contains("webagent/1") && m.contains("Ungültige"))
            }),
            "erwarte Repair-Prompt, got: {sent:?}"
        );
    }

    #[test]
    fn kurze_antwort_mit_klammer_geht_in_repair() {
        // `{` ist ebenfalls Protokoll-Nutzlast (JSON-Versuch) — selbst "{}"
        // ist reparaturwuerdig statt brain_unavailable.
        let brain =
            MockBrain::new().with_responses(vec!["{}", &finish_response()], vec![true, true]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
        let meta = controller.run("Repariere", "mock", None, false).unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(controller.brain().sent_message_count(), 2);
    }
}
