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
    Memory {
        query: Option<String>,
    },
    Remember {
        text: String,
    },
    Forget {
        id: u64,
    },
    Switch {
        target: Option<String>,
    },
    Login,
    Chat {
        message: String,
    },
    Whoami,
    Brains,
    /// Leistungsindex-Tabelle (Reliability aus echten swarm/relay-Aufrufen).
    Score,
    /// Canary-Health-Tabelle (`/canary`).
    Canary,
    /// Einheitliches Login für alle Brains (sequenziell), schreibt profiles/<brain>.
    LoginAll,
    /// Stehendes Ziel setzen/anzeigen/löschen (fließt in autonome Aufgaben ein).
    Goal {
        arg: Option<String>,
    },
    /// Multi-Brain-Swarm: alle antworten, dann führt ein Orchestrator zusammen.
    /// `orchestrator = Some(n)` wählt Brain n (1-basiert) fest; `None` = Konsens.
    Swarm {
        orchestrator: Option<usize>,
        prompt: String,
    },
    /// Worker-Pool-TUI aus dem Chat heraus starten (`/pool [n]`, n = active).
    Pool {
        active: Option<usize>,
    },
    /// Git-Änderungen im Arbeitsverzeichnis zeigen (`/diff`).
    Diff,
    Unknown {
        raw: String,
    },
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
    if trimmed == "/diff" {
        return Some(SlashCommand::Diff);
    }
    if trimmed == "/pool" || trimmed == "/tui" || trimmed == "/workers" {
        return Some(SlashCommand::Pool { active: None });
    }
    if let Some(rest) = trimmed
        .strip_prefix("/pool ")
        .or_else(|| trimmed.strip_prefix("/tui "))
        .or_else(|| trimmed.strip_prefix("/workers "))
    {
        let active = rest.trim().parse::<usize>().ok().filter(|n| *n >= 1);
        return Some(SlashCommand::Pool { active });
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
    if trimmed == "/score" || trimmed == "/leaderboard" {
        return Some(SlashCommand::Score);
    }
    if trimmed == "/canary" {
        return Some(SlashCommand::Canary);
    }
    if trimmed == "/login-all" {
        return Some(SlashCommand::LoginAll);
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

/// Zähler für die Abschluss-Zusammenfassung (qwen-code-Vorbild). Web-Chats
/// liefern keine echten Token-Zahlen, daher Schätzung über Zeichen/4.
#[derive(Default)]
struct SessionStats {
    tasks: u32,
    tasks_ok: u32,
    tasks_failed: u32,
    cycles: u32,
    chats: u32,
    swarms: u32,
    chars_in: usize,
    chars_out: usize,
    brains_used: std::collections::BTreeSet<String>,
}

impl SessionStats {
    fn requests(&self) -> u32 {
        self.tasks + self.chats + self.swarms
    }
}

/// Antworttext für die `/chat`-Anzeige aufbereiten: steckt das Brain nach einer
/// autonomen Aufgabe noch im webagent/1-Protokoll-Modus, kommt die Chat-Antwort
/// als JSON-Envelope zurück. Dann den Klartext der message-Actions zeigen statt
/// des rohen JSON; alles andere unverändert durchreichen.
fn display_chat_text(raw: &str) -> String {
    let parsed = crate::protocol::parse(raw);
    if parsed.valid {
        let texts: Vec<&str> = parsed
            .actions
            .iter()
            .filter(|a| {
                a.action_type == crate::protocol::ActionType::Message && !a.text.trim().is_empty()
            })
            .map(|a| a.text.trim())
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    // Fallback für Envelope-Varianten, die protocol::parse ablehnt (z.B. ohne
    // "protocol"-Feld oder als einzelnes message-Objekt).
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw.trim()) {
        let collect_msgs = |arr: &[serde_json::Value]| -> Vec<String> {
            arr.iter()
                .filter(|a| a.get("type").and_then(|t| t.as_str()) == Some("message"))
                .filter_map(|a| a.get("text").and_then(|t| t.as_str()))
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        };
        let msgs = match (&v, v.get("actions").and_then(|a| a.as_array())) {
            (_, Some(actions)) => collect_msgs(actions),
            (serde_json::Value::Object(_), None) => collect_msgs(std::slice::from_ref(&v)),
            _ => Vec::new(),
        };
        if !msgs.is_empty() {
            return msgs.join("\n");
        }
    }
    raw.trim().to_string()
}

/// Zeichenzahl → grobe Token-Schätzung (~4 Zeichen/Token), kompakt formatiert.
fn fmt_est_tokens(chars: usize) -> String {
    let tokens = chars / 4;
    if tokens >= 1000 {
        format!("≈{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("≈{tokens}")
    }
}

/// Sekunden → "1h 02m 03s" / "4m 05s" / "12s".
fn fmt_duration(total_secs: u64) -> String {
    let (h, m, s) = (total_secs / 3600, (total_secs % 3600) / 60, total_secs % 60);
    if h > 0 {
        format!("{h}h {m:02}m {s:02}s")
    } else if m > 0 {
        format!("{m}m {s:02}s")
    } else {
        format!("{s}s")
    }
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
    /// Zähler für die Abschluss-Zusammenfassung beim Beenden.
    stats: SessionStats,
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
            stats: SessionStats::default(),
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
        println!(
            "  \x1b[1mwebagent\x1b[0m · lokaler Browser-Agent ({} Module)",
            brains.len()
        );
        println!("  Module:  {modules}");
        println!(
            "  Aktiv:   \x1b[1;36m{}\x1b[0m — {who} — session: {:?}",
            self.brain_id, state
        );
        println!("  Befehle: /model <brain>  /chat <text>  /goal <text>  /swarm <text>  /pool [n]  /diff");
        println!("           /new  /brains  /whoami  /score  /canary  /memory  /login  /login-all  /exit");
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

    /// Leistungsindex-Tabelle ausgeben (`/score`) -- Reliability aus echten
    /// swarm/relay-Aufrufen, nicht aus einem separaten Benchmark. Brains ohne
    /// Ereignisse (noch nie ueber /swarm oder relay befragt) fehlen schlicht,
    /// statt mit einer erfundenen 0 zu erscheinen.
    fn print_score(&self) {
        let board = crate::brain_score::leaderboard();
        if board.is_empty() {
            println!("[score] Noch keine Daten -- /swarm oder relay muessen erst laufen.");
            return;
        }
        println!("[score] Leistungsindex (Reliability aus den letzten Aufrufen je Brain):");
        for s in board {
            let reason = s
                .last_reason
                .map(|r| format!("  letzter Fehlschlag: {r}"))
                .unwrap_or_default();
            println!(
                "  {:<10} reliability={:.2}  {}/{} Erfolge  ⌀{}ms{reason}",
                s.brain_id, s.reliability, s.window_successes, s.window_events, s.avg_latency_ms
            );
        }
    }

    /// Canary-Tabelle ausgeben (`/canary`).
    fn print_canary(&self) {
        let results = crate::canary::run_canary();
        if results.is_empty() {
            println!("[canary] keine Brains registriert");
            return;
        }
        println!("[canary] {} Brains:", results.len());
        for r in results {
            let status = if r.ok { "ok" } else { "FAIL" };
            println!(
                "  {:<10} {status:<4}  latency_ms={}  reason={}",
                r.brain_id, r.latency_ms, r.reason
            );
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
                        self.stats.brains_used.insert(target.clone());
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
            SlashCommand::LoginAll => {
                // Pausiert die REPL-Session, loggt alle Brains sequenziell ein,
                // startet das aktive Brain danach wieder.
                self.stop_brain();
                println!("[login-all] Sequentielles Login für alle Brains (profiles/<brain>)…");
                let results =
                    crate::login::login_all(std::time::Duration::from_secs(300), 0, false);
                let ok = results.iter().filter(|r| r.ok).count();
                let skip = results.iter().filter(|r| r.skipped).count();
                for r in &results {
                    let tag = if r.skipped {
                        "skip"
                    } else if r.ok {
                        "ok"
                    } else {
                        "FAIL"
                    };
                    println!("[login-all] [{tag}] {}: {}", r.brain_id, r.message);
                }
                println!(
                    "[login-all] fertig: {ok}/{} ok ({skip} übersprungen)",
                    results.len()
                );
                if let Err(e) = self.start_brain() {
                    eprintln!("[login-all] aktives Brain nicht neu gestartet: {e}");
                }
                ReplAction::Continue
            }
            SlashCommand::Whoami => {
                self.print_whoami();
                ReplAction::Continue
            }
            SlashCommand::Brains => {
                println!("[brains] Verfügbar: {}", available_brain_ids().join("  "));
                println!(
                    "[brains] Aktiv: {} (/switch <brain> zum Wechseln)",
                    self.brain_id
                );
                ReplAction::Continue
            }
            SlashCommand::Score => {
                self.print_score();
                ReplAction::Continue
            }
            SlashCommand::Canary => {
                self.print_canary();
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
                if !prompt.trim().is_empty() {
                    self.stats.swarms += 1;
                    self.stats.chars_in += prompt.chars().count();
                }
                self.run_swarm(orchestrator, &prompt);
                ReplAction::Continue
            }
            SlashCommand::Diff => {
                self.print_diff();
                ReplAction::Continue
            }
            SlashCommand::Pool { active } => {
                // Pool übernimmt Terminal + Browser-Profile; eigenes Brain vorher
                // freigeben, danach wieder starten.
                let n = active.unwrap_or(8);
                println!("[pool] Starte Worker-Pool-TUI ({n} aktiv, headless) — 'q' kehrt zum Chat zurück.");
                self.stop_brain();
                let code = crate::tui::run_tui(n, "", 5, true);
                if code != 0 {
                    println!("[pool] TUI beendet mit Code {code}.");
                }
                match self.start_brain() {
                    Ok(state) => println!(
                        "[pool] Zurück im Chat. Aktiv: {} (session {state:?})",
                        self.brain_id
                    ),
                    Err(e) => eprintln!("[pool] Brain-Neustart fehlgeschlagen: {e}"),
                }
                ReplAction::Continue
            }
            SlashCommand::Chat { message } => {
                if message.is_empty() {
                    println!("[system] Nutzung: /chat <nachricht>");
                    return ReplAction::Continue;
                }
                self.stats.chats += 1;
                self.stats.chars_in += message.chars().count();
                self.stats.brains_used.insert(self.brain_id.clone());
                match self.brain_mut().send(&message) {
                    Ok(baseline) => {
                        println!("[brain] ...");
                        let timeout =
                            resolve_timeout("wait_response", &self.brain_id, &message, None);
                        match self.brain_mut().wait_response(baseline, timeout) {
                            Ok(resp) => {
                                let display = display_chat_text(&resp.text);
                                self.stats.chars_out += display.chars().count();
                                println!("[brain] {display}");
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
    /// Reihe nach befragt wird. `profile_override` erlaubt ein isoliertes
    /// Laufzeit-Profil (Swarm-Teilkopie) statt des Shared-Profils.
    fn swarm_query(
        &self,
        brain_id: &str,
        prompt: &str,
        profile_override: Option<std::path::PathBuf>,
    ) -> Result<String, String> {
        // Wiederholt blockierte/fehlgeschlagene Brains werden fuer eine Cooldown-
        // Zeit uebersprungen statt bei jedem Swarm erneut den vollen Timeout zu
        // kosten (siehe circuit_breaker.rs).
        if let Some(remaining) = crate::circuit_breaker::check(brain_id) {
            return Err(format!(
                "circuit_open: uebersprungen, noch {remaining}s Cooldown"
            ));
        }
        let started = std::time::Instant::now();
        let prompt_chars = prompt.chars().count();
        let mut backend = WebBrainBackend::from_config(brain_id)?;
        if let Some(p) = profile_override {
            backend = backend.with_profile_override(p);
        }
        backend.start(self.headless)?;
        let ready_to = resolve_timeout("ensure_ready", brain_id, "", None);
        let state = backend
            .ensure_ready(ready_to)
            .unwrap_or(SessionState::Error);
        if state != SessionState::Ready {
            let _ = backend.stop();
            let label = Self::state_label(state).to_string();
            crate::circuit_breaker::record_failure(brain_id, &label);
            crate::brain_score::record_event(
                brain_id,
                false,
                Some(&label),
                started.elapsed().as_millis() as u64,
                prompt_chars,
            );
            return Err(label);
        }
        let _ = backend.new_chat();
        let baseline = backend.send(prompt).inspect_err(|_| {
            let _ = backend.stop();
        })?;
        let wait_to = resolve_timeout("wait_response", brain_id, prompt, None);
        let out = match backend.wait_response(baseline, wait_to) {
            // Externe Blockierung (Tageslimit/Login/Cloudflare) ist kein Beitrag zur
            // Zusammenführung -- sonst landet die Limit-Seite als vermeintliche
            // "Antwort" im Battle-Royale-Ergebnis (siehe [[external-blocks-flag-not-fail]]).
            Ok(resp) if resp.backend_status == "blocked" || resp.backend_status == "rate_limit" => {
                Err(format!("blockiert: {}", resp.text.trim()))
            }
            Ok(resp) if !resp.text.trim().is_empty() => Ok(resp.text.trim().to_string()),
            Ok(resp) => Err(format!("keine Antwort (status={})", resp.backend_status)),
            Err(e) => Err(e),
        };
        let _ = backend.stop();
        let latency_ms = started.elapsed().as_millis() as u64;
        match &out {
            Ok(_) => {
                crate::circuit_breaker::record_success(brain_id);
                crate::brain_score::record_event(brain_id, true, None, latency_ms, prompt_chars);
            }
            Err(e) => {
                crate::circuit_breaker::record_failure(brain_id, e);
                crate::brain_score::record_event(
                    brain_id,
                    false,
                    Some(e),
                    latency_ms,
                    prompt_chars,
                );
            }
        }
        out
    }

    /// `/swarm [n] <prompt>` — Multi-Brain-Swarm (schlüssiger Ablauf).
    ///
    /// **Ablauf**
    /// 1. **Antworten:** jedes verfügbare Brain bekommt denselben Prompt, jeweils
    ///    in einem **isolierten** Profil (`prepare_swarm_profile` → Kopie aus
    ///    `reference/<brain>` oder `profiles/<brain>`). Kein Shared-Pool.
    /// 2. **Orchestrator wählen** (wer synthetisiert):
    ///    - `/swarm N …` → Brain N (1-basiert), falls es in Phase 1 geantwortet hat
    ///    - sonst: **Reliability** unter den Antwortenden (`brain_score`, kein
    ///      zusätzlicher Browser-Roundtrip)
    ///    - optional teuer: `WEBAGENT_SWARM_VOTE=1` → jedes antwortende Brain
    ///      stimmt ab (sieht Antwort-Kurzfassungen im Prompt)
    /// 3. **Synthese:** nur der Orchestrator bekommt alle Antworten und liefert final.
    /// 4. Swarm-Profile aufräumen, REPL-Brain wieder starten.
    ///
    /// Früher Phase-2-„Konsens“: jedes Brain nochmal voll befragen *ohne* die
    /// Antworten zu sehen → teuer und inhaltlich blind. Default ist jetzt Score.
    fn run_swarm(&mut self, orchestrator: Option<usize>, prompt: &str) {
        if prompt.trim().is_empty() {
            println!("[swarm] Nutzung: /swarm <prompt>         — Orchestrator per Reliability");
            println!("[swarm]         /swarm <1-8> <prompt>  — Orchestrator fest");
            println!("[swarm]         WEBAGENT_SWARM_VOTE=1  — teure Live-Abstimmung");
            return;
        }
        let targets = available_brain_ids();
        if let Some(n) = orchestrator {
            if !(1..=targets.len()).contains(&n) {
                println!(
                    "[swarm] Ungültiger Orchestrator-Index {n} (1-{}).",
                    targets.len()
                );
                return;
            }
        }
        self.stop_brain(); // aktives REPL-Brain pausieren

        let run_id = crate::now_run_stamp();
        // Cleanup immer, auch bei early return (Drop-Guard über Scope-Ende).
        struct SwarmCleanup {
            run_id: String,
        }
        impl Drop for SwarmCleanup {
            fn drop(&mut self) {
                let _ = crate::config::cleanup_swarm_profiles(&self.run_id);
            }
        }
        let _cleanup = SwarmCleanup {
            run_id: run_id.clone(),
        };

        let profiles: Vec<(String, std::path::PathBuf)> = targets
            .iter()
            .map(|tb| {
                let p = crate::config::prepare_swarm_profile(&run_id, tb);
                (tb.clone(), p)
            })
            .collect();
        let profile_of = |brain: &str| -> Option<std::path::PathBuf> {
            profiles
                .iter()
                .find(|(b, _)| b == brain)
                .map(|(_, p)| p.clone())
        };

        // Stehendes /goal analog zu run_autonomous voranstellen (leer wenn keins).
        let goal_ctx = match &self.goal {
            Some(g) => format!("Übergeordnetes Ziel: {g}\n\n"),
            None => String::new(),
        };
        let framed_prompt = format!("{goal_ctx}{prompt}");

        // ---- Phase 1: Antworten (isoliert, sequenziell) ----
        println!(
            "[swarm] Phase 1/3 — {} Brains antworten (isolierte Profile)…",
            targets.len()
        );
        let mut answers: Vec<(String, String)> = Vec::new();
        for (i, tb) in targets.iter().enumerate() {
            let prof = profile_of(tb);
            match self.swarm_query(tb, &framed_prompt, prof) {
                Ok(a) => {
                    let preview: String = a.chars().take(200).collect();
                    println!(
                        "[swarm {}/{}] \x1b[1m{tb}\x1b[0m: {preview}{}",
                        i + 1,
                        targets.len(),
                        if a.chars().count() > 200 { "…" } else { "" }
                    );
                    answers.push((tb.clone(), a));
                }
                Err(e) => println!("[swarm {}/{}] {tb}: — {e}", i + 1, targets.len()),
            }
        }
        if answers.is_empty() {
            println!("[swarm] Keine Antworten — Abbruch.");
            let _ = self.start_brain();
            return;
        }
        if answers.len() == 1 {
            println!(
                "[swarm] Nur eine Antwort ({}) — überspringe Synthese.",
                answers[0].0
            );
            println!("\n[swarm ⇒ final]\n{}\n", answers[0].1);
            let _ = self.start_brain();
            return;
        }
        let names: Vec<String> = answers.iter().map(|(b, _)| b.clone()).collect();

        // ---- Phase 2: Orchestrator ----
        let live_vote = matches!(
            std::env::var("WEBAGENT_SWARM_VOTE")
                .unwrap_or_default()
                .to_lowercase()
                .as_str(),
            "1" | "true" | "yes" | "on"
        );
        let orch = match orchestrator {
            Some(n) => {
                let chosen = targets[n - 1].clone();
                if !names.contains(&chosen) {
                    println!(
                        "[swarm] Phase 2/3 — {chosen} hat nicht geantwortet → Fallback {}",
                        names[0]
                    );
                    names[0].clone()
                } else {
                    println!("[swarm] Phase 2/3 — Orchestrator (fest): \x1b[1m{chosen}\x1b[0m");
                    chosen
                }
            }
            None if live_vote => {
                // Teuer: jeder Antwortende stimmt ab — mit Kurzfassungen der Antworten.
                println!("[swarm] Phase 2/3 — Live-Abstimmung (WEBAGENT_SWARM_VOTE=1)…");
                let mut snippets = String::new();
                for (b, a) in &answers {
                    let snip: String = a.chars().take(280).collect();
                    snippets.push_str(&format!("\n### {b}\n{snip}\n"));
                }
                let vote_prompt = format!(
                    "{goal_ctx}Aufgabe: «{prompt}».\n\
                     Folgende Modelle haben geantwortet (Kurzfassung):{snippets}\n\
                     Welches EINE Modell aus der Liste [{list}] soll die finale Synthese machen?\n\
                     Antworte NUR mit genau einem Namen aus der Liste.",
                    list = names.join(", ")
                );
                let mut votes: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for voter in &names {
                    let prof = profile_of(voter);
                    if let Ok(v) = self.swarm_query(voter, &vote_prompt, prof) {
                        let low = v.to_lowercase();
                        if let Some(pick) = names.iter().find(|n| low.contains(&n.to_lowercase())) {
                            *votes.entry(pick.clone()).or_insert(0) += 1;
                            println!("[swarm vote] {voter} → {pick}");
                        } else {
                            println!("[swarm vote] {voter} → (keine klare Nennung: {v})");
                        }
                    }
                }
                let winner = names
                    .iter()
                    .max_by_key(|n| votes.get(*n).copied().unwrap_or(0))
                    .cloned()
                    .unwrap_or_else(|| names[0].clone());
                let wv = votes.get(&winner).copied().unwrap_or(0);
                println!("[swarm] Phase 2/3 — Abstimmung: \x1b[1m{winner}\x1b[0m ({wv} Stimme(n))");
                winner
            }
            None => {
                // Default: Reliability unter den Antwortenden — kein extra Browser-Round.
                let board = crate::brain_score::leaderboard();
                let score_of = |id: &str| -> f64 {
                    board
                        .iter()
                        .find(|s| s.brain_id == id)
                        .map(|s| s.reliability)
                        .unwrap_or(0.5)
                };
                let winner = names
                    .iter()
                    .max_by(|a, b| {
                        score_of(a)
                            .partial_cmp(&score_of(b))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .cloned()
                    .unwrap_or_else(|| names[0].clone());
                let sc = score_of(&winner);
                println!(
                    "[swarm] Phase 2/3 — Orchestrator per Reliability: \x1b[1m{winner}\x1b[0m (score={sc:.2})"
                );
                println!("[swarm]         (Live-Vote: WEBAGENT_SWARM_VOTE=1)");
                winner
            }
        };

        // ---- Phase 3: Synthese (nur Orchestrator) ----
        println!("[swarm] Phase 3/3 — {orch} synthetisiert…");
        let joined: String = answers
            .iter()
            .map(|(b, a)| format!("### {b}\n{a}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synth_prompt = format!(
            "{goal_ctx}Aufgabe: «{prompt}».\n\nDie beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
             Führe diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
             Nenne Widersprüche, wenn es welche gibt. Du bist der Orchestrator ({orch}).",
        );
        match self.swarm_query(&orch, &synth_prompt, profile_of(&orch)) {
            Ok(final_answer) => {
                self.stats.chars_out += final_answer.chars().count();
                self.stats.brains_used.insert(orch.clone());
                println!("\n[swarm ⇒ final via \x1b[1m{orch}\x1b[0m]\n{final_answer}\n");
            }
            Err(e) => {
                println!("[swarm] Synthese durch {orch} fehlgeschlagen: {e}");
                // Fallback: längste/erste Antwort zeigen statt totaler Leere
                if let Some((b, a)) = answers.first() {
                    println!("[swarm] Fallback — erste Antwort ({b}):\n{a}\n");
                }
            }
        }

        // _cleanup Drop räumt Profile; REPL-Brain wieder
        let _ = self.start_brain();
        println!("[swarm] fertig. Aktiv weiterhin: {}", self.brain_id);
    }

    /// `/diff` — was hat sich im Arbeitsverzeichnis (git) geändert?
    fn print_diff(&self) {
        let run_git = |args: &[&str]| -> Option<String> {
            std::process::Command::new("git")
                .args(args)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim_end().to_string())
        };
        let Some(status) = run_git(&["status", "--short"]) else {
            println!("[diff] Kein git-Repository im Arbeitsverzeichnis (oder git fehlt).");
            return;
        };
        if status.is_empty() {
            println!("[diff] Arbeitsverzeichnis sauber — keine Änderungen.");
            return;
        }
        println!("[diff] git status --short:\n{status}");
        if let Some(stat) = run_git(&["diff", "--stat"]) {
            if !stat.is_empty() {
                println!("\n[diff] git diff --stat:\n{stat}");
            }
        }
        println!("\n[diff] Details: git diff <datei> im Terminal.");
    }

    fn run_autonomous(&mut self, task: &str) {
        let _ = self.start_brain();
        // Stehendes Ziel als Kontext voranstellen.
        let effective = match &self.goal {
            Some(g) => format!("Übergeordnetes Ziel: {g}\n\nAktuelle Aufgabe: {task}"),
            None => task.to_string(),
        };
        self.stats.tasks += 1;
        self.stats.chars_in += task.chars().count();
        self.stats.brains_used.insert(self.brain_id.clone());
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
                if meta.status == "done" {
                    self.stats.tasks_ok += 1;
                } else {
                    self.stats.tasks_failed += 1;
                }
                self.stats.cycles += meta.cycles;
                println!(
                    "[repl] status={} run_id={} cycles={}",
                    meta.status, meta.run_id, meta.cycles
                );
            }
            Err(e) => {
                self.stats.tasks_failed += 1;
                eprintln!("[repl] Fehler: {e}");
            }
        }
    }

    /// Abschluss-Zusammenfassung der Session (qwen-code-Vorbild).
    fn print_summary(&self, elapsed_secs: u64) {
        let s = &self.stats;
        println!();
        println!("  \x1b[1m── Session-Zusammenfassung ──────────────────────\x1b[0m");
        println!("  Dauer      {}", fmt_duration(elapsed_secs));
        if s.requests() == 0 {
            println!("  Anfragen   keine");
            return;
        }
        println!(
            "  Anfragen   {} gesamt · {} Aufgaben ({} ok, {} Fehler) · {} Chats · {} Swarms",
            s.requests(),
            s.tasks,
            s.tasks_ok,
            s.tasks_failed,
            s.chats,
            s.swarms
        );
        if s.cycles > 0 {
            println!("  Zyklen     {} (Plan/Act/Observe)", s.cycles);
        }
        if !s.brains_used.is_empty() {
            let brains: Vec<&str> = s.brains_used.iter().map(String::as_str).collect();
            println!("  Brains     {}", brains.join(", "));
        }
        println!(
            "  ~Tokens    {} rein · {} raus (Zeichen/4, Schätzung — Web-Chat liefert keine echten Zahlen)",
            fmt_est_tokens(s.chars_in),
            fmt_est_tokens(s.chars_out)
        );
    }
}

/// Startet die REPL. Liest Aufgaben von stdin, bis `/exit` oder EOF.
pub fn run_repl(brain_id: &str, headless: bool) -> i32 {
    let session_start = std::time::Instant::now();
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

    session.print_summary(session_start.elapsed().as_secs());
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
            parse_slash_command("/login-all"),
            Some(SlashCommand::LoginAll)
        );
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
    fn parse_score() {
        assert_eq!(parse_slash_command("/score"), Some(SlashCommand::Score));
        assert_eq!(
            parse_slash_command("/leaderboard"),
            Some(SlashCommand::Score)
        );
    }

    #[test]
    fn parse_canary() {
        assert_eq!(parse_slash_command("/canary"), Some(SlashCommand::Canary));
    }

    #[test]
    fn repl_action_roundtrip() {
        assert_eq!(parse_slash_command("/quit"), Some(SlashCommand::Exit));
        assert_eq!(
            parse_slash_command("/pool"),
            Some(SlashCommand::Pool { active: None })
        );
        assert_eq!(
            parse_slash_command("/tui"),
            Some(SlashCommand::Pool { active: None })
        );
        assert_eq!(
            parse_slash_command("/pool 4"),
            Some(SlashCommand::Pool { active: Some(4) })
        );
        assert_eq!(
            parse_slash_command("/pool quatsch"),
            Some(SlashCommand::Pool { active: None })
        );
    }

    #[test]
    fn chat_display_unwraps_protocol_json() {
        // Volles webagent/1-Envelope -> nur der message-Text.
        let envelope = r#"{"protocol":"webagent/1","actions":[{"id":"answer-1","type":"message","text":"pong"}]}"#;
        assert_eq!(display_chat_text(envelope), "pong");
        // Envelope ohne "protocol"-Feld (parse lehnt ab) -> Fallback greift.
        let no_proto = r#"{"actions":[{"id":"a","type":"message","text":"hallo"}]}"#;
        assert_eq!(display_chat_text(no_proto), "hallo");
        // Einzelnes message-Objekt.
        let single = r#"{"id":"answer-1","type":"message","text":"solo"}"#;
        assert_eq!(display_chat_text(single), "solo");
        // Mehrere messages -> zusammengefügt; finish wird ignoriert.
        let multi = r#"{"protocol":"webagent/1","actions":[{"id":"1","type":"message","text":"a"},{"id":"2","type":"finish"},{"id":"3","type":"message","text":"b"}]}"#;
        assert_eq!(display_chat_text(multi), "a\nb");
        // Klartext bleibt unangetastet.
        assert_eq!(display_chat_text("  ganz normal  "), "ganz normal");
        // Kaputtes JSON -> Rohtext.
        assert_eq!(display_chat_text("{nicht json"), "{nicht json");
    }

    #[test]
    fn session_summary_formatting() {
        assert_eq!(fmt_duration(12), "12s");
        assert_eq!(fmt_duration(245), "4m 05s");
        assert_eq!(fmt_duration(3723), "1h 02m 03s");
        assert_eq!(fmt_est_tokens(120), "≈30");
        assert_eq!(fmt_est_tokens(8_400), "≈2.1k");

        let mut s = SessionStats::default();
        assert_eq!(s.requests(), 0);
        s.tasks = 2;
        s.chats = 3;
        s.swarms = 1;
        assert_eq!(s.requests(), 6);
    }
}
