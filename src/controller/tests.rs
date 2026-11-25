use super::*;

#[test]
fn session_state_run_status_uses_canonical_run_store_values() {
    assert_eq!(session_state_run_status(SessionState::Ready), "running");
    assert_eq!(
        session_state_run_status(SessionState::LoginRequired),
        "login_required"
    );
    assert_eq!(
        session_state_run_status(SessionState::Cloudflare),
        "cloudflare"
    );
    assert_eq!(
        session_state_run_status(SessionState::Unbestimmt),
        "brain_incomplete"
    );
    assert_eq!(session_state_run_status(SessionState::Error), "error");
}

#[test]
fn complete_protocol_salvages_timeout_unstable() {
    let message =
        r#"{"protocol":"webagent/1","actions":[{"id":"final","type":"message","text":"fertig"}]}"#;
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
    let complete =
        r#"{"protocol":"webagent/1","actions":[{"id":"final","type":"message","text":"fertig"}]}"#;
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
    send_error: Option<String>,
    wait_calls: Rc<RefCell<usize>>,
    /// Je Antwort-Index eine Kette wachsender Stream-Snapshots. Ist sie
    /// gesetzt, liefert `wait_response_streaming` die Snapshots nacheinander
    /// und der letzte Snapshot wird zur Antwort. Default: leer → gepuffertes
    /// Einzelsnapshot über den Trait-Default.
    streaming_snapshots: Vec<Vec<String>>,
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
            send_error: None,
            wait_calls: Rc::new(RefCell::new(0)),
            streaming_snapshots: Vec::new(),
        }
    }

    fn with_streaming(mut self, chains: Vec<Vec<String>>) -> Self {
        self.streaming_snapshots = chains;
        self
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

    fn with_send_error(mut self, error: &str) -> Self {
        self.send_error = Some(error.to_string());
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
        if let Some(error) = &self.send_error {
            return Err(error.clone());
        }
        Ok(0)
    }

    fn wait_response(
        &mut self,
        _baseline_count: i32,
        _timeout: f64,
    ) -> Result<BrainResponse, String> {
        *self.wait_calls.borrow_mut() += 1;
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
            first_text_ms: None,
            stop_first_seen_ms: None,
            stop_gone_ms: None,
            completion_ms: None,
            completion_reason: None,
            polls: None,
        })
    }

    fn wait_response_streaming(
        &mut self,
        baseline_count: i32,
        timeout: f64,
        on_update: &mut dyn FnMut(&str),
    ) -> Result<BrainResponse, String> {
        // Ist eine Stream-Kette konfiguriert, liefern wir echte wachsende
        // Snapshots (nacheinander), beginnend mit dem ersten Snapshot.
        // Der Index wird VOR dem if-let-Block in eine lokale Variable
        // kopiert, damit kein RefCell-Borrow bis ins Blockende haengt.
        let idx = *self.response_index.borrow();
        if let Some(chain) = self.streaming_snapshots.get(idx).cloned() {
            *self.wait_calls.borrow_mut() += 1;
            *self.response_index.borrow_mut() += 1;
            for snapshot in &chain {
                on_update(snapshot);
                if !self.wait_sleep.is_zero() {
                    std::thread::sleep(self.wait_sleep);
                }
            }
            let last = chain.last().cloned().unwrap_or_default();
            return Ok(BrainResponse {
                text: last,
                message_index: idx as i32,
                generation_complete: true,
                backend_status: "ok".to_string(),
                raw_html: String::new(),
                first_text_ms: None,
                stop_first_seen_ms: None,
                stop_gone_ms: None,
                completion_ms: None,
                completion_reason: None,
                polls: None,
            });
        }
        // Keine Stream-Kette: genau EIN gepufferter Snapshot mit dem finalen
        // Text (Trait-Default-Verhalten) — gepufferte Ausgabe zaelt nicht
        // als inkrementelles Streaming, wird aber trotzdem geliefert.
        let response = self.wait_response(baseline_count, timeout)?;
        if !response.text.is_empty() {
            on_update(&response.text);
        }
        Ok(response)
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

#[test]
fn send_error_never_waits_for_and_recycles_a_stale_response() {
    let brain = MockBrain::new()
        .with_responses(
            vec![&shell_response("stale-action", "Get-Location")],
            vec![true],
        )
        .with_send_error("kein Absende-Beweis");
    let wait_calls = brain.wait_calls.clone();
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, unique_data_dir());

    let turn = controller.run_once("neue Observation", None);

    assert!(!turn.complete);
    assert!(turn.text.contains("kein Absende-Beweis"), "{}", turn.text);
    assert!(turn.text.contains("\"stage\":\"send\""), "{}", turn.text);
    assert_eq!(
        *wait_calls.borrow(),
        0,
        "stale Antwort darf nie gelesen werden"
    );
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
    let brain = MockBrain::new().with_responses(vec![&edit, &finish_response()], vec![true, true]);
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
fn cross_brain_handoff_starts_fresh_run_without_restoring_source_conversation() {
    let data_dir = unique_data_dir();
    let store = RunStore::new(data_dir.join("runs"), data_dir.join("logs"));
    let mut source = store.create("source", "source task").unwrap();
    source.conversation_ref = Some("https://chatgpt.com/c/source-session".to_string());
    store.save(&source).unwrap();
    let handoff = CrossBrainHandoffEnvelope::new(
        &source.run_id,
        "source",
        "mock",
        1,
        "cargo test failed with E0425",
    )
    .unwrap();

    let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
    let messages = brain.messages.clone();
    let restore_calls = brain.restore_calls.clone();
    let new_chat_calls = brain.new_chat_calls.clone();
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());
    let target_task = format!(
        "Implement target task\n\nBounded handoff context:\n{}",
        handoff.context()
    );

    let target = controller
        .run_cross_brain_handoff(&target_task, "mock", &handoff, false, RunOptions::default())
        .unwrap();

    assert_ne!(target.run_id, source.run_id);
    assert!(restore_calls.borrow().is_empty());
    assert!(*new_chat_calls.borrow() >= 1);
    assert!(messages.borrow()[0].contains(handoff.context()));
    assert_eq!(target.cross_brain_handoff().unwrap(), Some(handoff.clone()));
    assert_ne!(target.conversation_ref, source.conversation_ref);
    assert_eq!(
        store.load(&source.run_id).unwrap().conversation_ref,
        source.conversation_ref
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
fn message_part_stream_records_all_parts_before_finish() {
    let response = r#"{"protocol":"webagent/1","actions":[
            {"id":"final-part-001","type":"message_part","text":"erster Nachweis"},
            {"id":"final-part-002","type":"message_part","text":"zweiter Nachweis"},
            {"id":"final-complete","type":"finish"}
        ]}"#;
    let brain = MockBrain::new().with_responses(vec![response], vec![true]);
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, unique_data_dir());

    let meta = controller
        .run(
            "Liefere einen langen strukturierten Nachweis",
            "mock",
            None,
            false,
        )
        .unwrap();

    assert_eq!(meta.status, "done");
    assert_eq!(
        meta.completed_actions
            .get("final-part-001")
            .map(String::as_str),
        Some("erster Nachweis")
    );
    assert_eq!(
        meta.completed_actions
            .get("final-part-002")
            .map(String::as_str),
        Some("zweiter Nachweis")
    );
    assert_eq!(
        meta.completed_actions
            .get("final-complete")
            .map(String::as_str),
        Some("finish")
    );
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
            m.contains("NUR mit gültigem") || (m.contains("webagent/1") && m.contains("Ungültige"))
        }),
        "expected protocol repair prompt, got: {sent:?}"
    );
    assert!(
        sent.iter().any(|m| {
            m.contains("<ORIGINAL_TASK>Repariere Protokoll</ORIGINAL_TASK>")
                && m.contains("warte nicht auf eine neue Nutzeranweisung")
        }),
        "repair must re-anchor the unfinished task, got: {sent:?}"
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
    let mut controller = AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);

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
fn continuation_after_protocol_error_processes_valid_response() {
    let data_dir = unique_data_dir();
    let store = RunStore::new(data_dir.join("runs"), data_dir.join("logs"));
    let mut meta = store.create("mock", "Implementiere die Funktion").unwrap();
    meta.status = "protocol_error".to_string();
    meta.cycles = 2;
    meta.conversation_ref = Some("https://chatgpt.com/c/protocol-repair".to_string());
    meta.completed_actions
        .insert("prior-read".to_string(), "ok".to_string());
    meta.extra.insert(
        "protocol_error_streak".to_string(),
        serde_json::Value::String("3".to_string()),
    );
    store.save(&meta).unwrap();

    let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());
    let result = controller
        .continue_run(
            &meta.run_id,
            "Die nächste Antwort ist wieder protokollkonform; fahre fort.",
            "mock",
            false,
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(result.status, "done");
    assert_eq!(result.run_id, meta.run_id);
    assert!(result.cycles > 2);
    assert!(result.completed_actions.contains_key("prior-read"));
    assert!(!result.extra.contains_key("protocol_error_streak"));
    assert_eq!(store.load(&meta.run_id).unwrap().status, "done");
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
    let mut controller = AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);

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
    let mut controller = AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);
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
    let mut controller = AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir);
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
            m.contains("NUR mit gültigem") || (m.contains("webagent/1") && m.contains("Ungültige"))
        }),
        "erwarte Repair-Prompt, got: {sent:?}"
    );
}

#[test]
fn kurze_antwort_mit_klammer_geht_in_repair() {
    // `{` ist ebenfalls Protokoll-Nutzlast (JSON-Versuch) — selbst "{}"
    // ist reparaturwuerdig statt brain_unavailable.
    let brain = MockBrain::new().with_responses(vec!["{}", &finish_response()], vec![true, true]);
    let executor = MockExecutor::new();
    let mut controller = AgentController::with_data_dir(brain, executor, 10, unique_data_dir());
    let meta = controller.run("Repariere", "mock", None, false).unwrap();

    assert_eq!(meta.status, "done");
    assert_eq!(controller.brain().sent_message_count(), 2);
}
#[test]
fn fresh_chat_is_created_before_worker_task() {
    let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
    let new_chat_calls = brain.new_chat_calls.clone();
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, unique_data_dir());
    controller.set_fresh_chat(true);

    let _meta = controller
        .run("finish the isolated task", "mock", None, false)
        .unwrap();

    assert!(
        *new_chat_calls.borrow() >= 1,
        "expected a fresh chat before task dispatch, calls={}",
        *new_chat_calls.borrow()
    );
}

fn stream_events_from(runs_dir: &std::path::Path, run_id: &str) -> Vec<serde_json::Value> {
    let dir = runs_dir.join(run_id);
    let path = dir.join("transcript.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    text.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Das `brain_stream`-Event trägt die Stream-Statistik im `content`-Feld
/// (Transkript-Konvention: role=system, content=Eventname).
fn find_stream_event(events: &[serde_json::Value]) -> serde_json::Value {
    events
        .iter()
        .find(|e| e.get("content").and_then(|v| v.as_str()) == Some("brain_stream"))
        .cloned()
        .unwrap_or_else(|| panic!("kein brain_stream-Event: {events:?}"))
}

/// Der Controller konsumiert denselben Ereignisstrom wie Relay/Swarm/UI:
/// wachsende Snapshots erscheinen als `brain_stream`-Event mit Append-
/// Deltas (echtes Praefixwachstum), gepufferte Endantworten gelten nicht
/// als Streaming.
#[test]
fn brain_stream_event_erfasst_inkrementelles_praefixwachstum() {
    let data_dir = unique_data_dir();
    // Kette mit einem echten Praefix-Wachstum (Fragment -> Zwischenstand)
    // und EINER Revision (Zwischenstand -> finale Antwort): der Brain hat
    // den Zwischenstand (falsche Action-ID) durch die finale Antwort
    // ersetzt. Ein solcher Ersatz zaehlt nie als inkrementelles Streaming.
    let s0 = "{\"protocol\"";
    let s1 =
            "{\"protocol\": \"webagent/1\", \"actions\": [{\"id\": \"wrong-id\", \"type\": \"finish\"}]}";
    let s2 = finish_response();
    // s1 beginnt mit s0 (Append), s2 beginnt NICHT mit s1 (Replace).
    let chain = vec![s0.to_string(), s1.to_string(), s2];
    let brain = MockBrain::new().with_streaming(vec![chain]);
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());

    let meta = controller.run("Pruefe etwas", "mock", None, false).unwrap();

    assert_eq!(meta.status, "done", "meta={:?}", meta.extra);
    let events = stream_events_from(&data_dir.join("runs"), &meta.run_id);
    let stream = find_stream_event(&events);
    assert_eq!(stream["snapshots"], 3, "{stream}");
    assert_eq!(stream["revisions"], 1, "{stream}");
    assert_eq!(stream["saw_text"], true, "{stream}");
    assert_eq!(
        stream["streamed_incrementally"], false,
        "Zwischenstand->Endantwort-Ersatz ist eine Revision, kein Praefixwachstum: {stream}"
    );
    let _ = std::fs::remove_dir_all(&data_dir);
}

/// Eine reine Praefix-Kette OHNE Revision ist echtes inkrementelles
/// Streaming. Die Praefixe werden per Slice aus der finalen Antwort
/// gebildet, damit sie garantiert echt wachsen (Unicode/ASCII-sicher).
#[test]
fn reine_praefixkette_ohne_revision_ist_inkrementell() {
    let data_dir = unique_data_dir();
    let finish = finish_response();
    let p1 = finish[..finish.len() - 24].to_string();
    let p2 = finish[..finish.len() - 4].to_string();
    // p1 Paefix von p2, p2 Praefix von finish — reines Wachstum.
    let chain = vec![p1, p2, finish];
    let brain = MockBrain::new().with_streaming(vec![chain]);
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());

    let meta = controller.run("Pruefe", "mock", None, false).unwrap();

    assert_eq!(meta.status, "done");
    let events = stream_events_from(&data_dir.join("runs"), &meta.run_id);
    let stream = find_stream_event(&events);
    assert_eq!(stream["snapshots"], 3, "{stream}");
    assert_eq!(stream["revisions"], 0, "{stream}");
    assert_eq!(stream["streamed_incrementally"], true, "{stream}");
    let _ = std::fs::remove_dir_all(&data_dir);
}

/// Eine BUFFERED finale Antwort (nur ein Snapshot) ist kein Streaming:
/// `streamed_incrementally` muss false sein, damit gepufferte Ausgabe nie
/// als inkrementelle Lieferung gezaehlt wird.
#[test]
fn gepufferte_endantwort_zaehlt_nicht_als_streaming() {
    let data_dir = unique_data_dir();
    let brain = MockBrain::new().with_responses(vec![&finish_response()], vec![true]);
    let mut controller =
        AgentController::with_data_dir(brain, MockExecutor::new(), 5, data_dir.clone());

    let meta = controller.run("Berechne", "mock", None, false).unwrap();

    assert_eq!(meta.status, "done");
    let events = stream_events_from(&data_dir.join("runs"), &meta.run_id);
    let stream = find_stream_event(&events);
    assert_eq!(
        stream["snapshots"], 1,
        "genau ein (gepufferter) Snapshot: {stream}"
    );
    assert_eq!(
        stream["streamed_incrementally"], false,
        "gepufferte Ausgabe darf nie als Streaming gelten: {stream}"
    );
    let _ = std::fs::remove_dir_all(&data_dir);
}
