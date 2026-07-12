//! AgentController – Plan/Act/Observe-Loop.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use std::env;

use crate::brain::BrainBackend;
use crate::executor::ShellExecutor;
use crate::loop_guard::{loop_guard_message, shell_read_fingerprint};
use crate::memory::MemoryStore;
use crate::prompts::{autonomous_task_prompt, resume_continue_prompt, resume_recovery_prompt};
use crate::protocol::{self, Action};
use crate::run_store::{RunMeta, RunStore};
use crate::transcript::Transcript;

const INCOMPLETE_RETRY_PROMPT: &str = 
    "[Controller] Die letzte Web-Antwort war unvollständig oder leer. \
     Setze mit einer gültigen webagent/1-Antwort fort. \
     Wenn die Aufgabe abgeschlossen ist, sende eine message-Action.";

// Konfigurationskonstanten (aus CONVENTIONS.md: keine externe config-Crate)
const MAX_OBSERVATION_CHARS: usize = 16_000;
const LOOP_GUARD_WARN_COUNT: usize = 3;
const LOOP_GUARD_ABORT_COUNT: usize = 5;
const RESUME_TRANSCRIPT_CHAR_BUDGET: usize = 8_000;
const MEMORY_CONTEXT_LIMIT: usize = 5;
const CONTROLLER_HEARTBEAT_INTERVAL_SECONDS: f64 = 30.0;

/// Ergebnis eines einzelnen Brain-Turns.
#[derive(Debug, Clone)]
pub struct BrainTurn {
    pub text: String,
    pub complete: bool,
}

/// AgentController orchestriert Brain + Executor im Plan/Act/Observe-Loop.
pub struct AgentController<B: BrainBackend, E: ShellExecutor> {
    brain: B,
    executor: E,
    max_cycles: usize,
    run_store: RunStore,
    memory: MemoryStore,
    meta: Option<RunMeta>,
    completed_actions: HashMap<String, String>,
    incomplete_retries: usize,
}

impl<B: BrainBackend, E: ShellExecutor> AgentController<B, E> {
    pub const MAX_INCOMPLETE_RETRIES: usize = 5;

    pub fn new(brain: B, executor: E, max_cycles: usize) -> Self {
        let data_dir = env::current_dir()
            .unwrap_or_else(|_| env::temp_dir())
            .join("data");
        let runs_dir = data_dir.join("runs");
        let logs_dir = data_dir.join("logs");
        let memory_path = data_dir.join("memory.jsonl");

        Self {
            brain,
            executor,
            max_cycles,
            run_store: RunStore::new(runs_dir, logs_dir),
            memory: MemoryStore::new(memory_path),
            meta: None,
            completed_actions: HashMap::new(),
            incomplete_retries: 0,
        }
    }

    /// Persistiert conversation_ref in RunMeta.
    fn persist_conversation_ref(&mut self) {
        if let Some(meta) = &mut self.meta {
            if let Some(ref_val) = self.brain.get_conversation_ref() {
                meta.conversation_ref = Some(ref_val);
                self.run_store.save(meta);
            }
        }
    }

    /// Führt einen einzelnen Brain-Turn aus.
    pub fn run_once(&mut self, message: &str, transcript: Option<&mut Transcript>) -> BrainTurn {
        if let Some(t) = transcript {
            let _ = t.append("user", message, HashMap::new());
        }

        let baseline = match self.brain.send(message) {
            Ok(b) => b,
            Err(_) => 0,
        };
        
        let mut response = match self.brain.wait_response(baseline, 60.0) {
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
            if let Some(t) = transcript {
                let mut extra = HashMap::new();
                extra.insert("fragment".to_string(), serde_json::Value::String(response.text.clone()));
                let _ = t.append(
                    "system",
                    "brain_stream_fragment; rereading same assistant message",
                    extra,
                );
            }
            response = match self.brain.wait_response(baseline, 60.0) {
                Ok(r) => r,
                Err(_) => break,
            };
            rereads += 1;
        }

        if let Some(t) = transcript {
            let mut extra = HashMap::new();
            extra.insert("complete".to_string(), serde_json::Value::String(response.generation_complete.to_string()));
            extra.insert("status".to_string(), serde_json::Value::String(response.backend_status.clone()));
            let _ = t.append(
                "brain",
                &response.text,
                extra,
            );
        }

        if response.generation_complete {
            self.persist_conversation_ref();
        }

        BrainTurn {
            text: response.text,
            complete: response.generation_complete,
        }
    }

    /// Beendet Run mit brain_incomplete Status.
    fn finish_brain_incomplete(&mut self, meta: &mut RunMeta, transcript: &mut Transcript) -> RunMeta {
        meta.status = "brain_incomplete".to_string();
        self.run_store.save(meta);
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

    /// Versucht Recovery nach incomplete Response.
    fn recover_from_incomplete(
        &mut self,
        transcript: &mut Transcript,
        context: &str,
    ) -> Option<BrainTurn> {
        self.incomplete_retries += 1;
        let _ = transcript.append(
            "system",
            &format!(
                "brain_incomplete_retry={}/{} context={}",
                self.incomplete_retries,
                Self::MAX_INCOMPLETE_RETRIES,
                context
            ),
            HashMap::new(),
        );

        if let Some(meta) = &self.meta {
            self.run_store.save(meta);
        }

        if self.incomplete_retries > Self::MAX_INCOMPLETE_RETRIES {
            return None;
        }

        std::thread::sleep(Duration::from_secs(2));
        Some(self.run_once(INCOMPLETE_RETRY_PROMPT, Some(transcript)))
    }

    /// Speichert completed action.
    fn record_completed_action(&mut self, action_id: &str, result: &str) {
        self.completed_actions.insert(action_id.to_string(), result.to_string());
        if let Some(meta) = &mut self.meta {
            meta.completed_actions.insert(action_id.to_string(), result.to_string());
            self.run_store.save(meta);
        }
    }

    /// Trackt Observation-Bytes.
    fn track_observation_bytes(&mut self, observation: &str) -> usize {
        if let Some(meta) = &mut self.meta {
            let added = observation.as_bytes().len();
            let total: usize = meta
                .extra
                .get("observation_bytes")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0)
                + added;
            meta.extra.insert("observation_bytes".to_string(), total.to_string());
            self.run_store.save(meta);
            total
        } else {
            0
        }
    }

    /// Begrenzt Observation auf MAX_OBSERVATION_CHARS und archiviert vollständige Ausgabe.
    fn bounded_observation(&mut self, action_id: &str, observation: &str) -> String {
        if observation.len() <= MAX_OBSERVATION_CHARS || self.meta.is_none() {
            return observation.to_string();
        }

        let meta = self.meta.as_ref().unwrap();
        let action_dir = self.run_store.runs_dir().join(&meta.run_id).join("action_output");
        std::fs::create_dir_all(&action_dir).ok();

        let safe_id: String = action_id
            .chars()
            .map(|c| if c.is_alphanumeric() || "._-".contains(c) { c } else { '_' })
            .collect();
        let artifact = action_dir.join(format!("{}.txt", safe_id));

        std::fs::write(&artifact, observation).ok();

        let head_size = (MAX_OBSERVATION_CHARS as f64 * 0.65) as usize;
        let tail_size = MAX_OBSERVATION_CHARS - head_size;
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
                if action.action_type == "shell" {
                    observations.push(format!(
                        "[Controller] action_id={} wurde bereits ausgefuehrt; \
                         gespeicherte Observation wird erneut geliefert. \
                         Fuer einen korrigierten oder erneut versuchten Befehl \
                         ist eine neue, runweit eindeutige Action-ID erforderlich.\n{}",
                        action.id, stored
                    ));
                } else if action.action_type == "finish" {
                    finished = true;
                }
                continue;
            }

            match action.action_type.as_str() {
                "finish" => {
                    finished = true;
                    let mut extra = HashMap::new();
                    extra.insert("action_id".to_string(), serde_json::Value::String(action.id.clone()));
                    let _ = transcript.append("system", "finish", extra);
                    self.record_completed_action(&action.id, "finish");
                    break;
                }
                "message" => {
                    let mut extra = HashMap::new();
                    extra.insert("action_id".to_string(), serde_json::Value::String(action.id.clone()));
                    let _ = transcript.append("message", &action.text, extra);
                    println!("{}", action.text);
                    self.record_completed_action(&action.id, &action.text);
                    finished = true;
                    break;
                }
                "shell" => {
                    println!("[shell:{}] {}", action.id, action.command);
                    let result = self.executor.execute(&action.command, action.timeout_seconds);
                    let observation = protocol::format_observation(
                        &action.id,
                        &result.stdout,
                        &result.stderr,
                        result.exit_code,
                        result.interrupted,
                    );
                    let observation = self.bounded_observation(&action.id, &observation);
                    observations.push(observation.clone());
                    self.record_completed_action(&action.id, &observation);
                    self.track_observation_bytes(&observation);

                    // Loop-Guard
                    if let Some(meta) = &mut self.meta {
                        if let Some(fp) = shell_read_fingerprint(&action.command) {
                            let counts_key = "loop_fingerprints";
                            let mut counts: HashMap<String, usize> = meta
                                .extra
                                .get(counts_key)
                                .and_then(|s| serde_json::from_str(s).ok())
                                .unwrap_or_default();

                            let n = counts.entry(fp.clone()).or_insert(0);
                            *n += 1;
                            let count = *n;

                            meta.extra.insert(
                                counts_key.to_string(),
                                serde_json::to_string(&counts).unwrap_or_default(),
                            );
                            self.run_store.save(meta);

                            if count >= LOOP_GUARD_WARN_COUNT {
                                observations.push(loop_guard_message(&fp, count));
                            }
                            if count >= LOOP_GUARD_ABORT_COUNT {
                                meta.status = "analysis_loop".to_string();
                                self.run_store.save(meta);
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
                _ => {}
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
        let parsed = protocol::parse(response_text);

        if !parsed.valid {
            let detail = parsed.error.clone();
            let _ = transcript.append("system", &format!("protocol_invalid: {}", detail), HashMap::new());

            let failures: usize = self
                .meta
                .as_ref()
                .and_then(|m| m.extra.get("protocol_error_streak"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0)
                + 1;

            if let Some(meta) = &mut self.meta {
                meta.extra.insert("protocol_error_streak".to_string(), failures.to_string());
                self.run_store.save(meta);
            }

            if failures >= 3 {
                let _ = transcript.append(
                    "system",
                    "protocol_repair_aborted after=3 consecutive errors",
                    HashMap::new(),
                );
                return (String::new(), false);
            }

            let turn = self.run_once(&protocol::format_protocol_error(&detail), Some(transcript));
            if !turn.complete {
                return (String::new(), false);
            }
            return (turn.text, false);
        }

        // Protocol valid → reset error streak und incomplete retries
        if let Some(meta) = &mut self.meta {
            meta.extra.remove("protocol_error_streak");
            self.run_store.save(meta);
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

    /// Resume: Initial Turn (restore oder fallback).
    fn resume_initial_turn(&mut self, transcript: &mut Transcript) -> BrainTurn {
        let meta = self.meta.as_ref().unwrap();
        let mut restored = false;

        if let Some(ref conv_ref) = meta.conversation_ref {
            restored = self.brain.restore_conversation(conv_ref);
        }

        if restored && self.brain.ensure_ready() == crate::brain::SessionState::Ready {
            let _ = transcript.append(
                "system",
                &format!(
                    "resume_restored conversation_ref={}",
                    meta.conversation_ref.as_ref().unwrap()
                ),
                HashMap::new(),
            );
            let restored_turn = self.run_once(&resume_continue_prompt(), Some(transcript));
            if restored_turn.complete {
                return restored_turn;
            }
            let _ = transcript.append(
                "system",
                "resume_restored_unresponsive; falling back to new chat",
                HashMap::new(),
            );
        }

        self.brain.new_chat();
        let tail = transcript.recovery_tail(RESUME_TRANSCRIPT_CHAR_BUDGET).unwrap_or_default();
        let _ = transcript.append("system", "resume_fallback=new_chat+transcript", HashMap::new());
        self.run_once(&resume_recovery_prompt(&meta.task, &tail), Some(transcript))
    }

    /// Hauptschleife: run().
    pub fn run(
        &mut self,
        task: &str,
        brain_id: &str,
        resume_id: Option<&str>,
        headless: bool,
    ) -> Result<RunMeta, String> {
        let (mut meta, mut transcript, task) = if let Some(rid) = resume_id {
            let meta = self.run_store.load(rid)?;
            if meta.brain_id != brain_id {
                return Err(format!(
                    "Resume erfordert brain_id={:?}, erhalten {:?}",
                    meta.brain_id, brain_id
                ));
            }
            let transcript = Transcript::new(&meta);
            let task = meta.task.clone();
            (meta, transcript, task)
        } else {
            let meta = self.run_store.create(brain_id, task);
            let transcript = Transcript::new(&meta);
            (meta, transcript, task.to_string())
        };

        self.meta = Some(meta.clone());
        self.completed_actions = meta.completed_actions.clone();
        meta.extra.insert("owner_pid".to_string(), std::process::id().to_string());
        self.run_store.save(&meta);

        // Start Brain + Executor
        self.brain.start(headless).map_err(|e| {
            meta.status = "failed".to_string();
            meta.extra.insert("error_type".to_string(), "RuntimeError".to_string());
            meta.extra.insert("error".to_string(), e.clone());
            self.run_store.save(&meta);
            let mut extra = HashMap::new();
            extra.insert("error_type".to_string(), serde_json::Value::String("RuntimeError".to_string()));
            extra.insert("error".to_string(), serde_json::Value::String(e.clone()));
            let _ = transcript.append(
                "system",
                &format!("run_finished status={}", meta.status),
                extra,
            );
            e
        })?;

        let state = self.brain.ensure_ready();
        let _ = transcript.append("system", &format!("session_state={:?}", state), HashMap::new());

        if state != crate::brain::SessionState::Ready {
            meta.status = format!("{:?}", state).to_lowercase();
            self.run_store.save(&meta);
            let _ = transcript.append("system", &format!("run_finished status={}", meta.status), HashMap::new());
            self.brain.stop().ok();
            return Ok(meta);
        }

        // Pending response oder Resume oder Initial
        let mut turn = if resume_id.is_some() {
            if let Some(pending) = meta.extra.remove("pending_response") {
                let _ = transcript.append("system", "resume_pending_response", HashMap::new());
                self.run_store.save(&meta);
                BrainTurn {
                    text: pending,
                    complete: true,
                }
            } else {
                let _ = transcript.append("system", &format!("resume run {}", resume_id.unwrap()), HashMap::new());
                self.resume_initial_turn(&mut transcript)
            }
        } else {
            let memories = self.memory.search(
                &task,
                &["shared", brain_id],
                MEMORY_CONTEXT_LIMIT,
            );
            let memory_context: String = memories
                .iter()
                .map(|e| format!("- [memory:{} {}] {}", e.id, e.kind, e.content))
                .collect::<Vec<_>>()
                .join("\n");

            meta.extra.insert(
                "memory_ids".to_string(),
                serde_json::to_string(&memories.iter().map(|e| &e.id).collect::<Vec<_>>())
                    .unwrap_or_default(),
            );
            self.run_store.save(&meta);

            self.run_once(&autonomous_task_prompt(&task, &memory_context), Some(&mut transcript))
        };

        // Incomplete recovery initial
        while !turn.complete {
            if let Some(recovered) = self.recover_from_incomplete(&mut transcript, "initial") {
                turn = recovered;
            } else {
                let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                self.executor.stop();
                self.brain.stop().ok();
                return Ok(final_meta);
            }
        }

        let mut response_text = turn.text;
        let mut finished = false;
        let mut cycle = meta.cycles;
        let loop_started = Instant::now();
        let mut last_heartbeat = loop_started;
        let heartbeat_interval = Duration::from_secs_f64(CONTROLLER_HEARTBEAT_INTERVAL_SECONDS);

        while !finished && cycle < self.max_cycles {
            cycle += 1;
            meta.cycles = cycle;
            self.run_store.save(&meta);

            let now = Instant::now();
            if now.duration_since(last_heartbeat) >= heartbeat_interval {
                let elapsed = now.duration_since(loop_started).as_secs_f64();
                let _ = transcript.append(
                    "system",
                    &format!("heartbeat cycle={} elapsed_s={:.1}", cycle, elapsed),
                    HashMap::new(),
                );
                self.run_store.save(&meta);
                last_heartbeat = now;
            }

            let (new_response, new_finished) = self.handle_response(&response_text, &mut transcript);
            response_text = new_response;
            finished = new_finished;

            while response_text.is_empty() && !finished {
                if let Some(recovered) = self.recover_from_incomplete(&mut transcript, "cycle") {
                    if !recovered.complete {
                        let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                        self.executor.stop();
                        self.brain.stop().ok();
                        return Ok(final_meta);
                    }
                    let (new_response, new_finished) =
                        self.handle_response(&recovered.text, &mut transcript);
                    response_text = new_response;
                    finished = new_finished;
                } else {
                    let final_meta = self.finish_brain_incomplete(&mut meta, &mut transcript);
                    self.executor.stop();
                    self.brain.stop().ok();
                    return Ok(final_meta);
                }
            }
        }

        meta.status = if finished { "done" } else { "max_cycles" }.to_string();

        if finished {
            meta.extra.remove("pending_response");
        } else {
            meta.extra.insert("pending_response".to_string(), response_text);
        }

        self.run_store.save(&meta);

        if meta.status == "done" {
            if let Some(memory_id) = self.memory.record_run(&meta) {
                meta.extra.insert("episode_memory_id".to_string(), memory_id);
                self.run_store.save(&meta);
            }
        }

        let _ = transcript.append("system", &format!("run_finished status={}", meta.status), HashMap::new());

        self.brain.stop().ok();

        Ok(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::{BrainResponse, SessionState};
    use crate::executor::ExecutionResult;
    use std::cell::RefCell;
    use std::rc::Rc;

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
            }
        }

        fn with_responses(mut self, responses: Vec<&str>, complete: Vec<bool>) -> Self {
            self.responses = responses.iter().map(|s| s.to_string()).collect();
            self.complete_flags = complete;
            self
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

        fn ensure_ready(&self) -> SessionState {
            self.session_state_value
        }

        fn session_state(&self) -> SessionState {
            self.session_state_value
        }

        fn new_chat(&mut self) {
            *self.new_chat_calls.borrow_mut() += 1;
            *self.conversation_ref.borrow_mut() = Some("https://example.test/chat/new".to_string());
        }

        fn send(&mut self, text: &str) -> Result<i32, String> {
            self.messages.borrow_mut().push(text.to_string());
            Ok(0)
        }

        fn wait_response(&mut self, _baseline_count: i32, _timeout: f64) -> Result<BrainResponse, String> {
            let idx = *self.response_index.borrow();
            *self.response_index.borrow_mut() = idx + 1;
            let text = self
                .responses
                .get(idx)
                .cloned()
                .unwrap_or_else(|| "{}".to_string());
            let complete = self.complete_flags.get(idx).copied().unwrap_or(true);
            Ok(BrainResponse {
                text,
                generation_complete: complete,
                backend_status: "ok".to_string(),
            })
        }

        fn is_logged_in(&self) -> bool {
            true
        }

        fn click_login(&mut self) {}

        fn wait_for_login(&mut self, _poll_interval: f64) {}

        fn get_conversation_ref(&self) -> Option<String> {
            self.conversation_ref.borrow().clone()
        }

        fn restore_conversation(&mut self, ref_val: &str) -> bool {
            self.restore_calls.borrow_mut().push(ref_val.to_string());
            if self.restore_result {
                *self.conversation_ref.borrow_mut() = Some(ref_val.to_string());
            }
            self.restore_result
        }
    }

    struct MockExecutor {
        commands: Rc<RefCell<Vec<String>>>,
        started: Rc<RefCell<bool>>,
    }

    impl MockExecutor {
        fn new() -> Self {
            Self {
                commands: Rc::new(RefCell::new(Vec::new())),
                started: Rc::new(RefCell::new(false)),
            }
        }
    }

    impl ShellExecutor for MockExecutor {
        fn execute(&self, command: &str, _timeout: f64) -> ExecutionResult {
            self.commands.borrow_mut().push(command.to_string());
            ExecutionResult {
                stdout: format!("out:{}", command),
                stderr: String::new(),
                exit_code: 0,
                interrupted: false,
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

    #[test]
    fn test_conversation_ref_persisted_after_complete_brain_response() {
        let brain = MockBrain::new()
            .with_responses(vec![&finish_response()], vec![true]);
        let conv_ref = brain.conversation_ref.clone();
        *conv_ref.borrow_mut() = Some("https://example.test/chat/persisted".to_string());

        let executor = MockExecutor::new();
        let mut controller = AgentController::new(brain, executor, 5);

        let meta = controller.run("Testaufgabe", "mock", None, false).unwrap();
        assert_eq!(meta.status, "done");

        let store = RunStore::new();
        let reloaded = store.load(&meta.run_id).unwrap();
        assert_eq!(
            reloaded.conversation_ref,
            Some("https://example.test/chat/persisted".to_string())
        );
    }

    #[test]
    fn test_successful_run_records_episode_once() {
        let brain = MockBrain::new()
            .with_responses(vec![&finish_response()], vec![true]);
        let executor = MockExecutor::new();
        let mut controller = AgentController::new(brain, executor, 5);

        let meta = controller.run("Merke diesen Testlauf", "mock", None, false).unwrap();

        let episodes: Vec<_> = controller
            .memory
            .list()
            .into_iter()
            .filter(|e| e.kind == "episode")
            .collect();
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].source, format!("run:{}", meta.run_id));
        assert_eq!(
            meta.extra.get("episode_memory_id"),
            Some(&episodes[0].id)
        );
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

        let mut controller = AgentController::new(brain, executor, 10);
        let meta = controller.run("Dedupe", "mock", None, false).unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(commands.borrow().len(), 1);
        assert_eq!(commands.borrow()[0], "Write-Output first");
        assert!(meta.completed_actions.contains_key("dup-1"));
    }

    #[test]
    fn test_resume_restores_conversation_when_possible() {
        let store = RunStore::new();
        let mut meta = store.create("mock", "Fortsetzen");
        meta.conversation_ref = Some("https://example.test/chat/old".to_string());
        meta.completed_actions.insert(
            "prev-1".to_string(),
            "[Terminal-Ausgabe action_id=prev-1]\nold".to_string(),
        );
        store.save(&meta);

        let brain = MockBrain::new()
            .with_responses(vec![&finish_response()], vec![true]);
        let restore_calls = brain.restore_calls.clone();
        let new_chat_calls = brain.new_chat_calls.clone();

        let executor = MockExecutor::new();
        let mut controller = AgentController::new(brain, executor, 5);

        let result = controller
            .run("ignored", "mock", Some(&meta.run_id), false)
            .unwrap();

        assert_eq!(result.status, "done");
        assert_eq!(
            restore_calls.borrow().as_slice(),
            &["https://example.test/chat/old"]
        );
        assert_eq!(*new_chat_calls.borrow(), 0);
    }

    #[test]
    fn test_resume_rejects_mismatched_brain_id() {
        let store = RunStore::new();
        let meta = store.create("mock", "Brain mismatch");
        store.save(&meta);

        let brain = MockBrain::new();
        let executor = MockExecutor::new();
        let mut controller = AgentController::new(brain, executor, 5);

        let result = controller.run("x", "other", Some(&meta.run_id), false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("brain_id"));
    }
}
