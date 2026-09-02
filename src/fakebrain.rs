//! fakebrain — deterministisches Fake-Brain fuer Tests.
//!
//! Implementiert [`crate::brain::BrainBackend`] ohne echten Browser und liefert
//! vorprogrammierbare Antwort-/Fehler-Szenarien. Damit lassen sich die
//! Steuerungs-Pfade deterministisch testen, die ohne echtes LLM nicht
//! reproduzierbar sind: Textdelta (inkrementelle Antwort), Tool-Loop,
//! Abort, Retry und Exactly-once.

use crate::brain::{BrainBackend, BrainResponse, SessionState};

/// Ein vorprogrammiertes Antwort-Szenario.
#[derive(Debug, Clone)]
pub enum Scenario {
    /// Liefert sofort eine statische Antwort (inkrementell als Delta).
    Text(String, u32),
    /// Fehler bei `start`/`send`/"wait" (simuliert Abort-/Retry-Ausloeser).
    Fail(String),
    /// Warten auf mehrere Polls (simuliert langsame/gestreamte Antwort).
    Stream(Vec<String>),
    /// Antwort, die nie `generation_complete` setzt (simuliert Haenger).
    Hanging,
}

/// Deterministisches Fake-Brain — konfigurierbar via Scenario-Liste.
#[derive(Debug, Clone)]
pub struct FakeBrain {
    id: String,
    state: SessionState,
    scenarios: Vec<Scenario>,
    cursor: usize,
    next_index: i32,
    /// Intro-Delta: groesse der Schritt-Snapshots in Zeichen.
    pub delta_chars: usize,
}

impl FakeBrain {
    /// Neues Fake-Brain mit einer Szenario-Liste.
    pub fn new(id: &str, scenarios: Vec<Scenario>) -> Self {
        Self {
            id: id.to_string(),
            state: SessionState::Ready,
            scenarios,
            cursor: 0,
            next_index: 0,
            delta_chars: 3,
        }
    }

    fn next_scenario(&mut self) -> Option<Scenario> {
        let s = self.scenarios.get(self.cursor).cloned();
        if self.cursor < self.scenarios.len() {
            self.cursor += 1;
        }
        s
    }

    fn index(&mut self) -> i32 {
        let i = self.next_index;
        self.next_index += 1;
        i
    }
}

impl Default for FakeBrain {
    fn default() -> Self {
        Self::new("fake", vec![Scenario::Text("hallo fake".into(), 0)])
    }
}

impl BrainBackend for FakeBrain {
    fn brain_id(&self) -> &str {
        &self.id
    }

    fn start(&mut self, _headless: bool) -> Result<(), String> {
        self.state = SessionState::Ready;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        self.state = SessionState::Unbestimmt;
        Ok(())
    }

    fn ensure_ready(&mut self, _timeout: f64) -> Result<SessionState, String> {
        Ok(self.state)
    }

    fn session_state(&self) -> SessionState {
        self.state
    }

    fn new_chat(&mut self) -> Result<(), String> {
        self.next_index = 0;
        Ok(())
    }

    fn send(&mut self, text: &str) -> Result<i32, String> {
        if text.contains("__fail__") {
            return Err("fakebrain: send fehlgeschlagen (__fail__)".into());
        }
        Ok(self.next_index)
    }

    fn wait_response(
        &mut self,
        _baseline_count: i32,
        _timeout: f64,
    ) -> Result<BrainResponse, String> {
        match self.next_scenario() {
            None => Err("fakebrain: keine Szenarien mehr".into()),
            Some(Scenario::Fail(m)) => Err(m),
            Some(Scenario::Text(text, polls)) => {
                let idx = self.index();
                Ok(BrainResponse {
                    text,
                    message_index: idx,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    raw_html: String::new(),
                    first_text_ms: Some(1),
                    stop_first_seen_ms: Some(2),
                    stop_gone_ms: Some(3),
                    completion_ms: Some(4),
                    completion_reason: Some("complete".into()),
                    polls: Some(polls),
                })
            }
            Some(Scenario::Stream(_)) => {
                // stream wird nur via wait_response_streaming sauber serviert;
                // final hier gleich voll ausliefern.
                let idx = self.index();
                Ok(BrainResponse {
                    text: "stream-final".into(),
                    message_index: idx,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    raw_html: String::new(),
                    first_text_ms: Some(1),
                    stop_first_seen_ms: None,
                    stop_gone_ms: None,
                    completion_ms: Some(9),
                    completion_reason: Some("complete".into()),
                    polls: Some(3),
                })
            }
            Some(Scenario::Hanging) => {
                let idx = self.index();
                Ok(BrainResponse {
                    text: "WEBAGENT/1 MESSAGE\nid: pending\n---MESSAGE---\nwarte".into(),
                    message_index: idx,
                    generation_complete: false,
                    backend_status: "pending".into(),
                    raw_html: String::new(),
                    first_text_ms: None,
                    stop_first_seen_ms: None,
                    stop_gone_ms: None,
                    completion_ms: None,
                    completion_reason: None,
                    polls: Some(0),
                })
            }
        }
    }

    fn wait_response_streaming(
        &mut self,
        baseline_count: i32,
        timeout: f64,
        on_update: &mut dyn FnMut(&str),
    ) -> Result<BrainResponse, String> {
        let scenario = self
            .next_scenario()
            .ok_or("fakebrain: keine Szenarien mehr")?;
        let final_response = match scenario {
            Scenario::Fail(m) => return Err(m),
            Scenario::Text(text, polls) => {
                // inkrementelle Deltas (Textdelta) zeichenweise/blockweise
                let steps = text
                    .chars()
                    .collect::<Vec<char>>()
                    .chunks(self.delta_chars.max(1))
                    .map(|c| c.iter().collect::<String>())
                    .collect::<Vec<_>>();
                for chunk in &steps {
                    on_update(chunk);
                }
                if timeout < 0.0 {
                    return Err("fakebrain: timeout".into());
                }
                let idx = self.index();
                BrainResponse {
                    text: text.clone(),
                    message_index: idx,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    raw_html: String::new(),
                    first_text_ms: Some(1),
                    stop_first_seen_ms: None,
                    stop_gone_ms: None,
                    completion_ms: Some(5),
                    completion_reason: Some("complete".into()),
                    polls: Some(polls),
                }
            }
            Scenario::Stream(parts) => {
                for p in &parts {
                    on_update(p);
                }
                let idx = self.index();
                BrainResponse {
                    text: parts.join(""),
                    message_index: idx,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    raw_html: String::new(),
                    first_text_ms: Some(1),
                    stop_first_seen_ms: None,
                    stop_gone_ms: None,
                    completion_ms: Some(9),
                    completion_reason: Some("complete".into()),
                    polls: Some(parts.len() as u32),
                }
            }
            Scenario::Hanging => {
                let idx = self.index();
                BrainResponse {
                    text: "WEBAGENT/1 MESSAGE\nid: pending\n---MESSAGE---\nwarte".into(),
                    message_index: idx,
                    generation_complete: false,
                    backend_status: "pending".into(),
                    raw_html: String::new(),
                    first_text_ms: None,
                    stop_first_seen_ms: None,
                    stop_gone_ms: None,
                    completion_ms: None,
                    completion_reason: None,
                    polls: Some(0),
                }
            }
        };
        let _ = baseline_count;
        Ok(final_response)
    }

    fn is_logged_in(&self) -> bool {
        self.state != SessionState::LoginRequired
    }

    fn click_login(&mut self) -> Result<(), String> {
        self.state = SessionState::LoginRequired;
        Ok(())
    }

    fn wait_for_login(&mut self, _poll_interval: f64) -> Result<(), String> {
        self.state = SessionState::Ready;
        Ok(())
    }

    fn get_conversation_ref(&self) -> Option<String> {
        Some("fake://conversation".into())
    }

    fn restore_conversation(&mut self, _reference: &str) -> Result<bool, String> {
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::AgentController;
    use crate::executor::{ExecutionResult, ShellExecutor};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct RecordingExecutor {
        commands: Rc<RefCell<Vec<String>>>,
    }

    impl RecordingExecutor {
        fn new() -> (Self, Rc<RefCell<Vec<String>>>) {
            let commands = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    commands: commands.clone(),
                },
                commands,
            )
        }
    }

    impl ShellExecutor for RecordingExecutor {
        fn execute(&self, command: &str, _timeout_seconds: f64) -> ExecutionResult {
            self.commands.borrow_mut().push(command.to_string());
            ExecutionResult {
                stdout: format!("fake-output:{command}"),
                stderr: String::new(),
                exit_code: Some(0),
                timed_out: false,
                error: None,
            }
        }
    }

    fn response(actions: serde_json::Value) -> String {
        serde_json::json!({
            "protocol": "webagent/1",
            "actions": actions,
        })
        .to_string()
    }

    fn finish_response() -> String {
        response(serde_json::json!([{"id": "finish-1", "type": "finish"}]))
    }

    fn shell_response(id: &str) -> String {
        response(serde_json::json!([{
            "id": id,
            "type": "shell",
            "command": "Get-Location",
            "timeout_seconds": 5
        }]))
    }

    fn isolated_data_dir(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "webagent_fakebrain_{label}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn text_scenario_liefert_fertige_antwort() {
        let mut fb = FakeBrain::new("fake", vec![Scenario::Text("hello".into(), 1)]);
        assert_eq!(fb.brain_id(), "fake");
        assert_eq!(fb.session_state(), SessionState::Ready);
        let r = fb.wait_response(0, 5.0).unwrap();
        assert_eq!(r.text, "hello");
        assert!(r.generation_complete);
    }

    #[test]
    fn fail_scenario_liefert_fehler() {
        let mut fb = FakeBrain::new("fake", vec![Scenario::Fail("boom".into())]);
        // BrainResponse implementiert kein PartialEq, daher pruefen wir den
        // Fehlerpfad ueber `matches!` statt `assert_eq!` auf Result.
        assert!(matches!(fb.wait_response(0, 5.0), Err(ref m) if m == "boom"));
        assert_eq!(
            fb.send("__fail__"),
            Err("fakebrain: send fehlgeschlagen (__fail__)".into())
        );
    }

    #[test]
    fn streaming_liefert_textdelta_in_schritten() {
        let mut fb = FakeBrain::new(
            "fake",
            vec![Scenario::Stream(vec!["a".into(), "bcd".into()])],
        );
        let mut seen = Vec::new();
        fb.wait_response_streaming(0, 5.0, &mut |t| seen.push(t.to_string()))
            .unwrap();
        assert_eq!(seen, vec!["a".to_string(), "bcd".to_string()]);
    }

    #[test]
    fn text_scenario_streaming_deltas_bleiben_nicht_leer() {
        let mut fb = FakeBrain::new("fake", vec![Scenario::Text("hallo".into(), 2)]);
        let mut joined = String::new();
        fb.wait_response_streaming(0, 5.0, &mut |t| joined.push_str(t))
            .unwrap();
        assert_eq!(joined, "hallo");
    }

    #[test]
    fn hanging_scenario_blockiert_generation() {
        let mut fb = FakeBrain::new("fake", vec![Scenario::Hanging]);
        let r = fb.wait_response(0, 5.0).unwrap();
        assert!(!r.generation_complete);
    }

    #[test]
    fn exactly_once_index_incrementiert_monoton() {
        let mut fb = FakeBrain::new(
            "fake",
            vec![Scenario::Text("a".into(), 0), Scenario::Text("b".into(), 0)],
        );
        let r1 = fb.wait_response(0, 5.0).unwrap();
        let r2 = fb.wait_response(0, 5.0).unwrap();
        assert!(r2.message_index > r1.message_index);
    }

    #[test]
    fn controller_fuehrt_fake_toolloop_bis_finish_aus() {
        let (executor, commands) = RecordingExecutor::new();
        let data_dir = isolated_data_dir("toolloop");
        let brain = FakeBrain::new(
            "fake",
            vec![
                Scenario::Text(shell_response("shell-1"), 0),
                Scenario::Text(finish_response(), 0),
            ],
        );
        let mut controller = AgentController::with_data_dir(brain, executor, 4, data_dir.clone());

        let meta = controller
            .run("fuehre den Test aus", "fake", None, true)
            .unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(commands.borrow().as_slice(), ["Get-Location"]);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn controller_retried_incomplete_turn_und_beendet_danach() {
        let (executor, commands) = RecordingExecutor::new();
        let data_dir = isolated_data_dir("retry");
        let brain = FakeBrain::new(
            "fake",
            vec![Scenario::Hanging, Scenario::Text(finish_response(), 0)],
        );
        let mut controller = AgentController::with_data_dir(brain, executor, 2, data_dir.clone());

        let meta = controller
            .run("beende den Lauf", "fake", None, true)
            .unwrap();

        assert_eq!(meta.status, "done");
        assert!(commands.borrow().is_empty());
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn controller_bricht_nach_drei_brain_fehlern_protokoll_sicher_ab() {
        let (executor, commands) = RecordingExecutor::new();
        let data_dir = isolated_data_dir("abort");
        let invalid = "diese Antwort ist absichtlich kein webagent-protokoll und lang genug, damit der Controller sie als begonnenen, aber ungültigen Turn behandelt";
        let brain = FakeBrain::new(
            "fake",
            vec![
                Scenario::Text(invalid.into(), 0),
                Scenario::Text(invalid.into(), 0),
                Scenario::Text(invalid.into(), 0),
            ],
        );
        let mut controller = AgentController::with_data_dir(brain, executor, 4, data_dir.clone());

        let meta = controller
            .run("bearbeite die Aufgabe", "fake", None, true)
            .unwrap();

        assert_eq!(meta.status, "protocol_error");
        assert_eq!(commands.borrow().len(), 0);
        let _ = std::fs::remove_dir_all(data_dir);
    }

    #[test]
    fn controller_fuehrt_doppelte_action_id_nur_einmal_aus() {
        let (executor, commands) = RecordingExecutor::new();
        let data_dir = isolated_data_dir("exactly_once");
        let brain = FakeBrain::new(
            "fake",
            vec![
                Scenario::Text(shell_response("same-id"), 0),
                Scenario::Text(shell_response("same-id"), 0),
                Scenario::Text(finish_response(), 0),
            ],
        );
        let mut controller = AgentController::with_data_dir(brain, executor, 5, data_dir.clone());

        let meta = controller.run("lies den Ort", "fake", None, true).unwrap();

        assert_eq!(meta.status, "done");
        assert_eq!(commands.borrow().as_slice(), ["Get-Location"]);
        assert_eq!(meta.completed_actions.len(), 2);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
