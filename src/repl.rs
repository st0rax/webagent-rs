//! Interaktive REPL mit persistenter Browser-Session und Slash-Befehlen.
//!
//! Portiert die Kern-UX aus `cli.py::cmd_repl` (ohne Genius-Council).

use std::io::{self, BufRead, Write};

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::config::{available_brain_ids, data_dir};
use crate::controller::{AgentController, RunOptions};
use crate::executor::PlatformShellExecutor;
use crate::memory::MemoryStore;
use crate::timeouts::resolve_timeout;

/// Ergebnis der Zeilenverarbeitung.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplAction {
    Continue,
    Exit,
}

/// Slash-Befehl und optionales Argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashCommand {
    Exit,
    New,
    Memory { query: Option<String> },
    Remember { text: String },
    Forget { id: u64 },
    Switch { target: Option<String> },
    Login,
    Chat { message: String },
    Unknown { raw: String },
}

/// Parst eine REPL-Zeile in einen Slash-Befehl oder `None` (autonomer Task).
pub fn parse_slash_command(line: &str) -> Option<SlashCommand> {
    let trimmed = line.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    if trimmed == "/exit" || trimmed == "/quit" {
        return Some(SlashCommand::Exit);
    }
    if trimmed == "/new" {
        return Some(SlashCommand::New);
    }
    if trimmed == "/memory" {
        return Some(SlashCommand::Memory { query: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/memory ") {
        return Some(SlashCommand::Memory {
            query: Some(rest.trim().to_string()),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/remember ") {
        return Some(SlashCommand::Remember {
            text: rest.trim().to_string(),
        });
    }
    if trimmed == "/remember" {
        return Some(SlashCommand::Remember {
            text: String::new(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/forget ") {
        if let Ok(id) = rest.trim().parse::<u64>() {
            return Some(SlashCommand::Forget { id });
        }
        return Some(SlashCommand::Forget { id: 0 });
    }
    if trimmed == "/forget" {
        return Some(SlashCommand::Forget { id: 0 });
    }
    if trimmed == "/switch" {
        return Some(SlashCommand::Switch { target: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/switch ") {
        return Some(SlashCommand::Switch {
            target: Some(rest.trim().to_lowercase()),
        });
    }
    if trimmed == "/login" {
        return Some(SlashCommand::Login);
    }
    if let Some(rest) = trimmed.strip_prefix("/chat ") {
        return Some(SlashCommand::Chat {
            message: rest.trim().to_string(),
        });
    }
    Some(SlashCommand::Unknown {
        raw: trimmed.to_string(),
    })
}

struct ReplSession {
    brain_id: String,
    controller: AgentController<WebBrainBackend, PlatformShellExecutor>,
    headless: bool,
    resume: Option<String>,
    memory: MemoryStore,
    brain_open: bool,
}

impl ReplSession {
    fn new(brain_id: &str, headless: bool) -> Result<Self, String> {
        let backend = WebBrainBackend::from_config(brain_id)?;
        let executor = PlatformShellExecutor::new();
        let controller = AgentController::with_data_dir(backend, executor, 100, data_dir());
        let memory_path = data_dir().join("memory.jsonl");
        Ok(Self {
            brain_id: brain_id.to_string(),
            controller,
            headless,
            resume: None,
            memory: MemoryStore::new(memory_path),
            brain_open: false,
        })
    }

    fn brain_mut(&mut self) -> &mut WebBrainBackend {
        self.controller.brain_mut()
    }

    fn start_brain(&mut self) -> Result<SessionState, String> {
        if !self.brain_open {
            let headless = self.headless;
            self.brain_mut().start(headless)?;
            self.brain_open = true;
        }
        let timeout = resolve_timeout("ensure_ready", &self.brain_id, "", None);
        Ok(self
            .brain_mut()
            .ensure_ready(timeout)
            .unwrap_or(SessionState::Error))
    }

    fn stop_brain(&mut self) {
        if self.brain_open {
            let _ = self.brain_mut().stop();
            self.brain_open = false;
        }
    }

    fn shutdown(&mut self) {
        self.stop_brain();
    }

    fn print_banner(&self, state: SessionState) {
        println!(
            "[repl] Brain={} session_state={:?}. Befehle: /memory /remember /forget /switch /login /chat /new /exit",
            self.brain_id, state
        );
    }

    fn handle_line(&mut self, line: &str) -> ReplAction {
        if let Some(cmd) = parse_slash_command(line) {
            return self.handle_slash(cmd);
        }
        let task = line.trim();
        if task.is_empty() {
            return ReplAction::Continue;
        }
        self.run_autonomous(task);
        ReplAction::Continue
    }

    fn handle_slash(&mut self, cmd: SlashCommand) -> ReplAction {
        match cmd {
            SlashCommand::Exit => ReplAction::Exit,
            SlashCommand::New => {
                if let Err(e) = self.brain_mut().new_chat() {
                    eprintln!("[repl] /new Fehler: {e}");
                } else {
                    self.resume = None;
                    println!("[repl] Neue Konversation.");
                }
                ReplAction::Continue
            }
            SlashCommand::Memory { query } => {
                let scopes = ["shared", self.brain_id.as_str()];
                let entries = if let Some(q) = query.filter(|s| !s.is_empty()) {
                    self.memory.search(&q, &scopes, 20).unwrap_or_default()
                } else {
                    self.memory.list(20).unwrap_or_default()
                };
                if entries.is_empty() {
                    println!("[memory] Keine Erinnerungen gefunden.");
                }
                for entry in entries {
                    let preview: String = entry
                        .content
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let preview = if preview.chars().count() > 180 {
                        format!("{}...", crate::char_prefix(&preview, 177))
                    } else {
                        preview
                    };
                    println!(
                        "[memory:{}] {}/{} {}",
                        entry.id, entry.kind, entry.scope, preview
                    );
                }
                ReplAction::Continue
            }
            SlashCommand::Remember { text } => {
                if text.is_empty() {
                    println!("[memory] Nutzung: /remember <fakt oder präferenz>");
                    return ReplAction::Continue;
                }
                match self.memory.add(&text, "shared", "explicit", None, 0.9) {
                    Ok(id) => println!("[memory] Gespeichert als memory:{id}"),
                    Err(e) => eprintln!("[memory] Fehler: {e}"),
                }
                ReplAction::Continue
            }
            SlashCommand::Forget { id } => {
                if id == 0 {
                    println!("[memory] Nutzung: /forget <id>");
                    return ReplAction::Continue;
                }
                match self.memory.delete(id) {
                    Ok(true) => println!("[memory] memory:{id} gelöscht."),
                    Ok(false) => println!("[memory] memory:{id} nicht gefunden."),
                    Err(e) => eprintln!("[memory] Fehler: {e}"),
                }
                ReplAction::Continue
            }
            SlashCommand::Switch { target } => {
                let available = available_brain_ids().join(", ");
                let Some(target) = target.filter(|t| !t.is_empty()) else {
                    println!("[switch] Verfügbar: {available}");
                    println!("[switch] Nutzung: /switch <brain>");
                    return ReplAction::Continue;
                };
                if !available_brain_ids().iter().any(|id| id == &target) {
                    println!("[switch] Unbekannt: {target}. Verfügbar: {available}");
                    return ReplAction::Continue;
                }
                if target == self.brain_id {
                    println!("[switch] {target} ist bereits aktiv.");
                    return ReplAction::Continue;
                }
                let old = self.brain_id.clone();
                self.stop_brain();
                let backend = match WebBrainBackend::from_config(&target) {
                    Ok(b) => b,
                    Err(e) => {
                        eprintln!("[switch] Wechsel fehlgeschlagen: {e}");
                        let _ = self.start_brain();
                        return ReplAction::Continue;
                    }
                };
                *self.controller.brain_mut() = backend;
                self.brain_id = target.clone();
                match self.start_brain() {
                    Ok(state) => {
                        self.resume = None;
                        println!("[switch] Gehirn={target} session_state={state:?}");
                    }
                    Err(e) => {
                        eprintln!("[switch] Wechsel fehlgeschlagen: {e}");
                        if let Ok(fallback) = WebBrainBackend::from_config(&old) {
                            *self.controller.brain_mut() = fallback;
                            self.brain_id = old;
                            let _ = self.start_brain();
                            println!("[switch] Fallback zu {}", self.brain_id);
                        }
                    }
                }
                ReplAction::Continue
            }
            SlashCommand::Login => {
                match self.brain_mut().click_login() {
                    Ok(()) => println!("[system] Anmelden geklickt."),
                    Err(e) => eprintln!("[system] Fehler: {e}"),
                }
                ReplAction::Continue
            }
            SlashCommand::Chat { message } => {
                if message.is_empty() {
                    println!("[system] Nutzung: /chat <nachricht>");
                    return ReplAction::Continue;
                }
                match self.brain_mut().send(&message) {
                    Ok(baseline) => {
                        println!("[brain] ...");
                        let timeout =
                            resolve_timeout("wait_response", &self.brain_id, &message, None);
                        match self.brain_mut().wait_response(baseline, timeout) {
                            Ok(resp) => {
                                println!("[brain] {}", resp.text);
                                if !resp.generation_complete {
                                    println!("[brain] Hinweis: status={}", resp.backend_status);
                                }
                            }
                            Err(e) => eprintln!("[brain] Fehler: {e}"),
                        }
                    }
                    Err(e) => eprintln!("[brain] Fehler: {e}"),
                }
                ReplAction::Continue
            }
            SlashCommand::Unknown { raw } => {
                println!("[repl] Unbekannter Befehl: {raw}");
                ReplAction::Continue
            }
        }
    }

    fn run_autonomous(&mut self, task: &str) {
        let _ = self.start_brain();
        let opts = RunOptions {
            skip_brain_start: true,
            skip_brain_stop: true,
        };
        match self.controller.run_with_options(
            task,
            &self.brain_id,
            self.resume.as_deref(),
            self.headless,
            opts,
        ) {
            Ok(meta) => {
                self.resume = Some(meta.run_id.clone());
                println!(
                    "[repl] status={} run_id={} cycles={}",
                    meta.status, meta.run_id, meta.cycles
                );
            }
            Err(e) => eprintln!("[repl] Fehler: {e}"),
        }
    }
}

/// Startet die REPL. Liest Aufgaben von stdin, bis `/exit` oder EOF.
pub fn run_repl(brain_id: &str, headless: bool) -> i32 {
    let mut session = match ReplSession::new(brain_id, headless) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[repl] {e}");
            return 2;
        }
    };

    match session.start_brain() {
        Ok(state) => session.print_banner(state),
        Err(e) => {
            eprintln!("[repl] Start fehlgeschlagen: {e}");
            return 2;
        }
    }

    let stdin = io::stdin();
    loop {
        print!("\n> ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }

        if session.handle_line(line.trim()) == ReplAction::Exit {
            break;
        }
    }

    session.shutdown();
    println!("[repl] beendet.");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_slash_commands() {
        assert_eq!(parse_slash_command("/exit"), Some(SlashCommand::Exit));
        assert_eq!(parse_slash_command("/new"), Some(SlashCommand::New));
        assert_eq!(
            parse_slash_command("/memory foo"),
            Some(SlashCommand::Memory {
                query: Some("foo".into())
            })
        );
        assert_eq!(
            parse_slash_command("/remember test"),
            Some(SlashCommand::Remember {
                text: "test".into()
            })
        );
        assert_eq!(
            parse_slash_command("/forget 42"),
            Some(SlashCommand::Forget { id: 42 })
        );
        assert_eq!(
            parse_slash_command("/switch claude"),
            Some(SlashCommand::Switch {
                target: Some("claude".into())
            })
        );
        assert_eq!(parse_slash_command("/login"), Some(SlashCommand::Login));
        assert_eq!(
            parse_slash_command("/chat hi"),
            Some(SlashCommand::Chat {
                message: "hi".into()
            })
        );
        assert_eq!(parse_slash_command("run task"), None);
    }

    #[test]
    fn repl_action_roundtrip() {
        assert_eq!(parse_slash_command("/quit"), Some(SlashCommand::Exit));
    }
}
