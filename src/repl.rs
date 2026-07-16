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
    Whoami,
    Brains,
    /// Stehendes Ziel setzen/anzeigen/löschen (fließt in autonome Aufgaben ein).
    Goal { arg: Option<String> },
    /// Multi-Brain-Swarm: alle antworten, dann führt ein Orchestrator zusammen.
    /// `orchestrator = Some(n)` wählt Brain n (1-basiert) fest; `None` = Konsens.
    Swarm {
        orchestrator: Option<usize>,
        prompt: String,
    },
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
    // /switch und /model sind synonym — ein „Brain" ist das Modell/der Provider,
    // wie der /model-Parameter bei anderen Agenten.
    if trimmed == "/switch" || trimmed == "/model" {
        return Some(SlashCommand::Switch { target: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("/switch ")
        .or_else(|| trimmed.strip_prefix("/model "))
    {
        return Some(SlashCommand::Switch {
            target: Some(rest.trim().to_lowercase()),
        });
    }
    if trimmed == "/goal" {
        return Some(SlashCommand::Goal { arg: None });
    }
    if let Some(rest) = trimmed.strip_prefix("/goal ") {
        return Some(SlashCommand::Goal {
            arg: Some(rest.trim().to_string()),
        });
    }
    if trimmed == "/swarm" {
        return Some(SlashCommand::Swarm {
            orchestrator: None,
            prompt: String::new(),
        });
    }
    if let Some(rest) = trimmed.strip_prefix("/swarm ") {
        let rest = rest.trim();
        // Optionaler Orchestrator-Index: „/swarm 3 <prompt>" (1-8). Nur wenn das
        // erste Token eine Zahl 1-8 ist UND ein Prompt folgt — sonst ganzer Rest = Prompt.
        if let Some((head, tail)) = rest.split_once(char::is_whitespace) {
            if let Ok(n) = head.parse::<usize>() {
                if (1..=8).contains(&n) && !tail.trim().is_empty() {
                    return Some(SlashCommand::Swarm {
                        orchestrator: Some(n),
                        prompt: tail.trim().to_string(),
                    });
                }
            }
        }
        return Some(SlashCommand::Swarm {
            orchestrator: None,
            prompt: rest.to_string(),
        });
    }
    if trimmed == "/login" {
        return Some(SlashCommand::Login);
    }
    if trimmed == "/whoami" {
        return Some(SlashCommand::Whoami);
    }
    if trimmed == "/brains" || trimmed == "/modules" {
        return Some(SlashCommand::Brains);
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
    /// Stehendes Ziel: wird jeder autonomen Aufgabe als Kontext vorangestellt.
    goal: Option<String>,
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
            goal: None,
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

    /// Kurzbeschreibung des Login-/Session-Zustands eines Brains.
    fn state_label(state: SessionState) -> &'static str {
        match state {
            SessionState::Ready => "angemeldet",
            SessionState::LoginRequired => "Login nötig (/login)",
            SessionState::Cloudflare => "Cloudflare-Prüfung",
            SessionState::Error => "nicht erreichbar",
        }
    }

    /// pi.dev-artiger Startbanner: verfügbare Module, aktives Brain, eingeloggter
    /// Account und Session-Zustand.
    fn print_banner(&mut self, state: SessionState) {
        let brains = available_brain_ids();
        let modules: String = brains
            .iter()
            .map(|id| {
                if id == &self.brain_id {
                    format!("\x1b[1;36m▸{id}\x1b[0m")
                } else {
                    format!(" {id}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        let account = self.controller.brain().account_label();
        let who = match &account {
            Some(a) => format!("angemeldet als \x1b[1m{a}\x1b[0m"),
            None => Self::state_label(state).to_string(),
        };
        println!();
        println!("  \x1b[1mwebagent\x1b[0m · lokaler Browser-Agent ({} Module)", brains.len());
        println!("  Module:  {modules}");
        println!(
            "  Aktiv:   \x1b[1;36m{}\x1b[0m — {who} — session: {:?}",
            self.brain_id, state
        );
        println!("  Befehle: /model <brain>  /chat <text>  /goal <text>  /swarm <text>");
        println!("           /new  /brains  /whoami  /memory  /login  /exit");
        println!();
    }

    /// Aktuellen Account + Zustand des aktiven Brains ausgeben (`/whoami`).
    fn print_whoami(&mut self) {
        let state = self
            .brain_mut()
            .ensure_ready(5.0)
            .unwrap_or(SessionState::Error);
        let account = self.controller.brain().account_label();
        match account {
            Some(a) => println!(
                "[whoami] {}: angemeldet als {a} (session {:?})",
                self.brain_id, state
            ),
            None => println!(
                "[whoami] {}: {} (session {:?})",
                self.brain_id,
                Self::state_label(state),
                state
            ),
        }
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
            SlashCommand::Whoami => {
                self.print_whoami();
                ReplAction::Continue
            }
            SlashCommand::Brains => {
                println!("[brains] Verfügbar: {}", available_brain_ids().join("  "));
                println!("[brains] Aktiv: {} (/switch <brain> zum Wechseln)", self.brain_id);
                ReplAction::Continue
            }
            SlashCommand::Goal { arg } => {
                self.handle_goal(arg);
                ReplAction::Continue
            }
            SlashCommand::Swarm {
                orchestrator,
                prompt,
            } => {
                self.run_swarm(orchestrator, &prompt);
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

    /// `/goal` — stehendes Ziel setzen, anzeigen oder löschen.
    fn handle_goal(&mut self, arg: Option<String>) {
        match arg.as_deref().map(str::trim) {
            None | Some("") => match &self.goal {
                Some(g) => println!("[goal] Aktuelles Ziel: {g}"),
                None => println!("[goal] Kein Ziel gesetzt. Nutzung: /goal <text>  ·  /goal clear"),
            },
            Some("clear") | Some("löschen") | Some("loeschen") => {
                self.goal = None;
                println!("[goal] Ziel gelöscht.");
            }
            Some(text) => {
                self.goal = Some(text.to_string());
                println!(
                    "[goal] Ziel gesetzt: {text}\n[goal] Fließt ab jetzt als Kontext in jede autonome Aufgabe ein (/goal clear zum Entfernen)."
                );
            }
        }
    }

    /// Ein voller Frage-Zyklus gegen ein frisches Brain-Backend: start → ensure_ready
    /// → new_chat → send → wait_response → stop. Für den Swarm, wo jedes Brain der
    /// Reihe nach befragt wird (gemeinsames Profil ⇒ nur eine Session gleichzeitig).
    fn swarm_query(&self, brain_id: &str, prompt: &str) -> Result<String, String> {
        let mut backend = WebBrainBackend::from_config(brain_id)?;
        backend.start(self.headless)?;
        let ready_to = resolve_timeout("ensure_ready", brain_id, "", None);
        let state = backend.ensure_ready(ready_to).unwrap_or(SessionState::Error);
        if state != SessionState::Ready {
            let _ = backend.stop();
            return Err(Self::state_label(state).to_string());
        }
        let _ = backend.new_chat();
        let baseline = backend.send(prompt).inspect_err(|_| {
            let _ = backend.stop();
        })?;
        let wait_to = resolve_timeout("wait_response", brain_id, prompt, None);
        let out = match backend.wait_response(baseline, wait_to) {
            Ok(resp) if !resp.text.trim().is_empty() => Ok(resp.text.trim().to_string()),
            Ok(resp) => Err(format!("keine Antwort (status={})", resp.backend_status)),
            Err(e) => Err(e),
        };
        let _ = backend.stop();
        out
    }

    /// `/swarm [n] <prompt>` — Multi-Brain-Swarm.
    ///
    /// 1. **Battle Royale:** jedes Brain beantwortet den Prompt.
    /// 2. **Orchestrator:** entweder fest gewählt (`n` = 1-8) oder per **Konsens** —
    ///    jedes Brain nominiert eines, Mehrheit gewinnt.
    /// 3. **Zusammenführung:** der Orchestrator verdichtet alle Antworten zu einer.
    ///
    /// Das aktive Brain wird pausiert (gemeinsames Profil) und danach wiederhergestellt.
    fn run_swarm(&mut self, orchestrator: Option<usize>, prompt: &str) {
        if prompt.trim().is_empty() {
            println!("[swarm] Nutzung: /swarm <prompt>            — Orchestrator per Konsens");
            println!("[swarm]         /swarm <1-8> <prompt>     — Orchestrator fest wählen");
            return;
        }
        let targets = available_brain_ids();
        if let Some(n) = orchestrator {
            if !(1..=targets.len()).contains(&n) {
                println!("[swarm] Ungültiger Orchestrator-Index {n} (1-{}).", targets.len());
                return;
            }
        }
        self.stop_brain(); // gemeinsames Profil freigeben

        // ---- Phase 1: Battle Royale — jedes Brain antwortet ----
        println!("[swarm] Phase 1 — {} Brains antworten…", targets.len());
        let mut answers: Vec<(String, String)> = Vec::new();
        for (i, tb) in targets.iter().enumerate() {
            match self.swarm_query(tb, prompt) {
                Ok(a) => {
                    println!("[swarm {}/{}] \x1b[1m{tb}\x1b[0m: {a}", i + 1, targets.len());
                    answers.push((tb.clone(), a));
                }
                Err(e) => println!("[swarm {}/{}] {tb}: — {e}", i + 1, targets.len()),
            }
        }
        if answers.len() < 2 {
            println!("[swarm] Zu wenige Antworten ({}) für eine Zusammenführung.", answers.len());
            let _ = self.start_brain();
            return;
        }
        let names: Vec<String> = answers.iter().map(|(b, _)| b.clone()).collect();

        // ---- Phase 2: Orchestrator bestimmen ----
        let orch = match orchestrator {
            Some(n) => {
                let chosen = targets[n - 1].clone();
                if !names.contains(&chosen) {
                    println!("[swarm] {chosen} hat nicht geantwortet — nehme {} als Orchestrator.", names[0]);
                    names[0].clone()
                } else {
                    println!("[swarm] Phase 2 — Orchestrator (fest): \x1b[1m{chosen}\x1b[0m");
                    chosen
                }
            }
            None => {
                println!("[swarm] Phase 2 — Konsens: Brains nominieren einen Orchestrator…");
                let vote_prompt = format!(
                    "Mehrere KI-Modelle haben diese Aufgabe beantwortet: «{prompt}».\n\
                     Teilnehmer: {}.\n\
                     Welches EINE Modell soll die Antworten zu einer finalen Antwort zusammenführen?\n\
                     Antworte NUR mit genau einem Namen aus der Liste.",
                    names.join(", ")
                );
                let mut votes: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for voter in &names {
                    if let Ok(v) = self.swarm_query(voter, &vote_prompt) {
                        let low = v.to_lowercase();
                        // Erste Nennung eines Teilnehmernamens zählt als Stimme.
                        if let Some(pick) = names.iter().find(|n| low.contains(&n.to_lowercase())) {
                            *votes.entry(pick.clone()).or_insert(0) += 1;
                            println!("[swarm vote] {voter} → {pick}");
                        } else {
                            println!("[swarm vote] {voter} → (keine klare Nennung)");
                        }
                    }
                }
                // Gewinner: meiste Stimmen; bei Gleichstand die Reihenfolge in `names`.
                let winner = names
                    .iter()
                    .max_by_key(|n| votes.get(*n).copied().unwrap_or(0))
                    .cloned()
                    .unwrap_or_else(|| names[0].clone());
                let wv = votes.get(&winner).copied().unwrap_or(0);
                println!("[swarm] Konsens: \x1b[1m{winner}\x1b[0m ({wv} Stimme(n))");
                winner
            }
        };

        // ---- Phase 3: Orchestrator führt zusammen ----
        println!("[swarm] Phase 3 — {orch} führt zusammen…");
        let joined: String = answers
            .iter()
            .map(|(b, a)| format!("### {b}\n{a}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synth_prompt = format!(
            "Aufgabe: «{prompt}».\n\nDie beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
             Führe diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
             Nenne Widersprüche, wenn es welche gibt.",
        );
        match self.swarm_query(&orch, &synth_prompt) {
            Ok(final_answer) => {
                println!("\n[swarm ⇒ final via \x1b[1m{orch}\x1b[0m]\n{final_answer}\n");
            }
            Err(e) => println!("[swarm] Zusammenführung durch {orch} fehlgeschlagen: {e}"),
        }

        let _ = self.start_brain(); // aktives Brain wiederherstellen
        println!("[swarm] fertig. Aktiv weiterhin: {}", self.brain_id);
    }

    fn run_autonomous(&mut self, task: &str) {
        let _ = self.start_brain();
        // Stehendes Ziel als Kontext voranstellen.
        let effective = match &self.goal {
            Some(g) => format!("Übergeordnetes Ziel: {g}\n\nAktuelle Aufgabe: {task}"),
            None => task.to_string(),
        };
        let opts = RunOptions {
            skip_brain_start: true,
            skip_brain_stop: true,
        };
        match self.controller.run_with_options(
            &effective,
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
    fn parse_model_is_switch_alias() {
        assert_eq!(
            parse_slash_command("/model claude"),
            Some(SlashCommand::Switch {
                target: Some("claude".into())
            })
        );
        assert_eq!(
            parse_slash_command("/model"),
            Some(SlashCommand::Switch { target: None })
        );
    }

    #[test]
    fn parse_goal_and_swarm() {
        assert_eq!(
            parse_slash_command("/goal alles testen"),
            Some(SlashCommand::Goal {
                arg: Some("alles testen".into())
            })
        );
        assert_eq!(
            parse_slash_command("/goal"),
            Some(SlashCommand::Goal { arg: None })
        );
        assert_eq!(
            parse_slash_command("/swarm Was ist 2+2?"),
            Some(SlashCommand::Swarm {
                orchestrator: None,
                prompt: "Was ist 2+2?".into()
            })
        );
        // Führender 1-8-Index wählt den Orchestrator fest.
        assert_eq!(
            parse_slash_command("/swarm 3 Fasse zusammen"),
            Some(SlashCommand::Swarm {
                orchestrator: Some(3),
                prompt: "Fasse zusammen".into()
            })
        );
        // Zahl außerhalb 1-8 ist Teil des Prompts, kein Orchestrator.
        assert_eq!(
            parse_slash_command("/swarm 42 Dinge"),
            Some(SlashCommand::Swarm {
                orchestrator: None,
                prompt: "42 Dinge".into()
            })
        );
    }

    #[test]
    fn repl_action_roundtrip() {
        assert_eq!(parse_slash_command("/quit"), Some(SlashCommand::Exit));
    }
}
