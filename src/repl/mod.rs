//! Interaktive REPL mit persistenter Browser-Session und Slash-Befehlen.
//!
//! Portiert die Kern-UX aus `cli.py::cmd_repl` (ohne Genius-Council).

mod autonomous;
pub mod commands;
mod pool;
mod text;

pub use commands::parse_slash_command;
pub use commands::{ReplAction, SlashCommand};
pub use pool::{isolated_query, PersistentQueryPool};

use text::{boxed, display_chat_text, fmt_duration, fmt_est_tokens, get_facts_string};

use std::io::{self, BufRead, Write};

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::config::{available_brain_ids, data_dir};
use crate::controller::AgentController;
use crate::executor::PlatformShellExecutor;
use crate::memory::MemoryStore;
use crate::timeouts::resolve_timeout;

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
            // Wortwahl ist hier keine Kosmetik: der Breaker entscheidet ueber
            // Textsuche, und das Wort „login" macht daraus eine harte Sperre
            // ueber sechs Stunden. Ein fehlender Nachweis ist aber kein Beleg
            // fuer eine Anmelde-Wand — er darf nur weich sperren. Der Test
            // `unbestimmt_sperrt_nicht_hart` haelt das fest.
            SessionState::Unbestimmt => "Seite nicht bereit — kein Anmelde-Nachweis gefunden",
            SessionState::Error => "nicht erreichbar",
        }
    }

    /// Willkommensbox à la grok CLI / pi.dev: Wortmarke, Brain-Roster mit
    /// hervorgehobenem aktiven Modul, angemeldeter Account, stehendes Ziel und
    /// ein Schnellstart-Hinweis. Der Befehlsindex folgt darunter, damit die Box
    /// ruhig bleibt.
    fn print_banner(&mut self, state: SessionState) {
        const INNER: usize = 72;
        let brains = available_brain_ids();
        let roster: String = brains
            .iter()
            .map(|id| {
                if id == &self.brain_id {
                    format!("\x1b[1;36m▸{id}\x1b[0m")
                } else {
                    format!("\x1b[2m{id}\x1b[0m")
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        let account = self.controller.brain().account_label();
        let who = match &account {
            Some(a) => format!("angemeldet als \x1b[1m{a}\x1b[0m"),
            None => Self::state_label(state).to_string(),
        };
        let goal_line = match &self.goal {
            Some(g) => format!("\x1b[2mZiel:\x1b[0m    {}", crate::char_prefix(g, 52)),
            None => "\x1b[2mZiel:\x1b[0m    \x1b[2m—\x1b[0m".to_string(),
        };

        let content = vec![
            "\x1b[1m▚▞ webagent\x1b[0m  \x1b[2mlokaler Browser-Agent — Chat & autonome Aufgaben\x1b[0m".to_string(),
            String::new(),
            format!("\x1b[2mBrains:\x1b[0m  {roster}"),
            format!("\x1b[2mAktiv:\x1b[0m   \x1b[1;36m{}\x1b[0m — {who}", self.brain_id),
            goal_line,
            String::new(),
            "\x1b[2mTippe eine Aufgabe — oder\x1b[0m /help \x1b[2mfür alle Befehle,\x1b[0m /pool \x1b[2mfür das Dashboard.\x1b[0m".to_string(),
        ];
        println!();
        for line in boxed(&content, INNER) {
            println!("  {line}");
        }
        println!();
    }

    /// Voller Befehlsindex (`/help`) — aus dem Banner ausgelagert, damit der
    /// Start ruhig ist und die Referenz bei Bedarf komplett kommt.
    fn print_help(&self) {
        println!("\n  \x1b[1mBefehle\x1b[0m");
        let rows = [
            ("<Aufgabe>", "autonom bearbeiten (Plan/Act/Observe)"),
            ("/chat <text>", "einmalige Frage ans aktive Brain"),
            ("/model <brain>", "aktives Brain wechseln"),
            (
                "/goal <text>",
                "stehendes Ziel setzen (Kontext jeder Aufgabe)",
            ),
            ("/swarm <text>", "alle Brains fragen + Synthese"),
            ("/pool [n]", "Worker-Pool-Dashboard (TUI)"),
            ("/diff", "Git-Änderungen im Arbeitsverzeichnis"),
            (
                "/autoresearch.self [N]",
                "Swarm-Selbstbewertung (Prioritäten)",
            ),
            ("/wiki [suche|lint]", "Wiki-Gedächtnis"),
            ("/score", "Leistungsindex je Brain"),
            ("/canary", "alle Brains kurz anpingen"),
            ("/brains  /whoami", "Roster · aktiver Account"),
            ("/memory  /remember  /forget", "Erinnerungen"),
            ("/new", "neue Konversation"),
            ("/login  /login-all", "Login-Fenster öffnen"),
            ("/help  /exit", "diese Hilfe · beenden"),
        ];
        for (cmd, desc) in rows {
            println!("    \x1b[1;36m{cmd:<28}\x1b[0m \x1b[2m{desc}\x1b[0m");
        }
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
            SlashCommand::Help => {
                self.print_help();
                ReplAction::Continue
            }
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
            SlashCommand::Facts => {
                let facts = get_facts_string();
                println!("{}", facts);
                ReplAction::Continue
            }
            SlashCommand::Autoresearch { eval_cmd, goal } => {
                self.run_autoresearch(&eval_cmd, &goal);
                ReplAction::Continue
            }
            SlashCommand::AutoresearchSelf { suggestions, top } => {
                self.run_self_research(suggestions, top);
                ReplAction::Continue
            }
            SlashCommand::Wiki { arg } => {
                self.handle_wiki(arg.as_deref());
                ReplAction::Continue
            }
            SlashCommand::Pool { active } => {
                // Pool übernimmt Terminal + Browser-Profile; eigenes Brain vorher
                // freigeben, danach wieder starten.
                let n = active.unwrap_or(8);
                println!("[pool] Starte Worker-Pool-TUI ({n} aktiv, headless) — 'q' kehrt zum Chat zurück.");
                self.stop_brain();
                let code = crate::tui::run_tui(n, "", 5, true, None, None, false);
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
            SlashCommand::Status => {
                println!(
                    "[status] brain={} tasks={}",
                    self.brain_id, self.stats.tasks
                );
                ReplAction::Continue
            }
            SlashCommand::Resume { id } => {
                println!(
                    "[resume] {}",
                    id.as_deref()
                        .unwrap_or("(letzter Lauf — in der Session-TUI)")
                );
                ReplAction::Continue
            }
            SlashCommand::Dashboard => {
                let _ = crate::tui::run_tui(2, "", 5, true, None, Some("workers"), false);
                ReplAction::Continue
            }
            SlashCommand::Evolve { args } => {
                println!("[evolve] starte Benchmark (dieselbe Pipeline wie /benchmark).");
                let candidates = crate::config::available_brain_ids();
                crate::tui::run_evolve(&args, &candidates);
                ReplAction::Continue
            }
            SlashCommand::Brute { url } => {
                match crate::repl::commands::brute_http_url(&url) {
                    None => {
                        println!("[brute] Nutzung: /brute <https://chat-url>");
                    }
                    Some(u) => {
                        println!("[brute] {u} — probe --write (derselbe Einstieg wie die CLI).");
                        let code = crate::bin_hooks::run_brute_write(&u, true);
                        if code != 0 {
                            println!("[brute] probe beendet mit Code {code}.");
                        }
                    }
                }
                ReplAction::Continue
            }
            SlashCommand::Unknown { raw } => {
                println!("[repl] Unbekannter Befehl: {raw}");
                ReplAction::Continue
            }
        }
    }

    /// `/wiki` — Index anzeigen, `/wiki <suchbegriff>` — Suche,
    /// `/wiki lint` — mechanischer Lint-Report. Rein lokal, braucht kein Brain.
    fn handle_wiki(&self, arg: Option<&str>) {
        let wiki = crate::wiki_memory::WikiMemory::new(data_dir().join("memory").join("wiki"));
        match arg.map(str::trim).filter(|s| !s.is_empty()) {
            None => match wiki.context_block(usize::MAX) {
                Ok(index) if index.trim().is_empty() => {
                    println!(
                        "[wiki] Wiki ist leer. Seiten liegen unter data/memory/wiki/<slug>.md \
(erste Zeile '# Titel'); der Index wird von write_page gepflegt."
                    );
                }
                Ok(index) => {
                    println!("[wiki] Index (data/memory/wiki/index.md):");
                    println!("{index}");
                }
                Err(e) => eprintln!("[wiki] Fehler: {e}"),
            },
            Some("lint") => match wiki.lint() {
                Ok(report) if report.is_clean() => {
                    println!("[wiki] Lint sauber — keine Befunde.");
                }
                Ok(report) => {
                    for (page, target) in &report.broken_links {
                        println!("[wiki] kaputter Link: [[{target}]] in Seite '{page}'");
                    }
                    for page in &report.orphan_pages {
                        println!("[wiki] Orphan (nirgends verlinkt): '{page}'");
                    }
                    for page in &report.empty_pages {
                        println!("[wiki] leere Seite (nur Titel): '{page}'");
                    }
                    for page in &report.index_missing {
                        println!("[wiki] fehlt im index.md: '{page}'");
                    }
                }
                Err(e) => eprintln!("[wiki] Lint-Fehler: {e}"),
            },
            Some(query) => match wiki.search(query, 10) {
                Ok(hits) if hits.is_empty() => {
                    println!("[wiki] Keine Treffer für '{query}'.");
                }
                Ok(hits) => {
                    for page in hits {
                        let first_line = page.body.lines().next().unwrap_or("").trim();
                        println!("[wiki:{}] {} — {}", page.slug, page.title, first_line);
                    }
                }
                Err(e) => eprintln!("[wiki] Such-Fehler: {e}"),
            },
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

    /// Abschluss-Zusammenfassung der Session (qwen-code-Vorbild).
    fn print_summary(&self, elapsed_secs: u64) {
        const INNER: usize = 72;
        let s = &self.stats;
        let mut content = vec![
            "\x1b[1m▚▞ Session-Zusammenfassung\x1b[0m".to_string(),
            String::new(),
            format!("\x1b[2mDauer\x1b[0m     {}", fmt_duration(elapsed_secs)),
        ];
        if s.requests() == 0 {
            content.push("\x1b[2mAnfragen\x1b[0m  keine — bis zum nächsten Mal.".to_string());
        } else {
            content.push(format!(
                "\x1b[2mAnfragen\x1b[0m  \x1b[1m{}\x1b[0m gesamt  ·  {} Aufgaben (\x1b[32m{} ok\x1b[0m, \x1b[31m{} Fehler\x1b[0m)",
                s.requests(),
                s.tasks,
                s.tasks_ok,
                s.tasks_failed,
            ));
            content.push(format!(
                "\x1b[2m         \x1b[0m  {} Chats  ·  {} Swarms",
                s.chats, s.swarms
            ));
            if s.cycles > 0 {
                content.push(format!(
                    "\x1b[2mZyklen\x1b[0m    {} \x1b[2m(Plan/Act/Observe)\x1b[0m",
                    s.cycles
                ));
            }
            content.push(format!(
                "\x1b[2mTokens\x1b[0m    {} rein · {} raus \x1b[2m(≈ Zeichen/4)\x1b[0m",
                fmt_est_tokens(s.chars_in),
                fmt_est_tokens(s.chars_out)
            ));
            if !s.brains_used.is_empty() {
                let brains: Vec<&str> = s.brains_used.iter().map(String::as_str).collect();
                content.push(format!("\x1b[2mBrains\x1b[0m    {}", brains.join(", ")));
            }
        }
        println!();
        for line in boxed(&content, INNER) {
            println!("  {line}");
        }
    }
}

/// Startet die REPL. Liest Aufgaben von stdin, bis `/exit` oder EOF.
/// Startübersicht vor der ersten Eingabe: welche Brains da sind, welche
/// wirklich einsatzbereit sind und was sie können — live geprüft, nicht aus
/// der Konfiguration gelesen.
///
/// Überspringbar mit `WEBAGENT_NO_WELCOME=1`; acht Browserstarts dauern, und
/// wer nur schnell etwas fragen will, braucht den Bericht nicht jedes Mal.
fn show_welcome() {
    if std::env::var("WEBAGENT_NO_WELCOME")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return;
    }
    let brains = crate::config::available_brain_ids();
    if brains.is_empty() {
        return;
    }
    println!("\n  webagent — prüfe {} Brains live…", brains.len());
    let shots = crate::config::data_dir().join("shots");
    let statuses = crate::welcome::probe_all_with_shots(&brains, true, 4, Some(&shots));
    print!(
        "{}",
        crate::welcome::render(&statuses, &crate::now_rfc3339())
    );
    // Failover zuerst: nur wenn KEIN Brain benutzbar ist, wird angemeldet.
    // Solange eins laeuft, arbeitet der Pool damit weiter und niemand wird
    // unterbrochen.
    let _ = crate::welcome::login_if_nothing_usable(&statuses, std::time::Duration::from_secs(600));
    // Kachelseite gleich mitschreiben: die Bilder liegen ja schon da.
    let _ = crate::welcome::write_wall_html(&shots, &brains, 0, 1);
    println!("\n  Bilderwand: {}", shots.join("wall.html").display());
    println!("\n  [Enter] weiter zur Eingabe");
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
}

pub fn run_repl(brain_id: &str, headless: bool) -> i32 {
    show_welcome();
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
        assert_eq!(parse_slash_command("/help"), Some(SlashCommand::Help));
        assert_eq!(parse_slash_command("/?"), Some(SlashCommand::Help));
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
    fn parse_autoresearch() {
        // Regulärer Fall: Eval-Befehl (darf Leerzeichen/Pipes enthalten) :: Ziel.
        assert_eq!(
            parse_slash_command("/autoresearch cargo test --lib :: mehr Tests grün"),
            Some(SlashCommand::Autoresearch {
                eval_cmd: "cargo test --lib".into(),
                goal: "mehr Tests grün".into()
            })
        );
        // Fehlender " :: "-Trenner → leere Felder = Usage-Pfad.
        assert_eq!(
            parse_slash_command("/autoresearch nur text ohne trenner"),
            Some(SlashCommand::Autoresearch {
                eval_cmd: String::new(),
                goal: String::new()
            })
        );
        assert_eq!(
            parse_slash_command("/autoresearch"),
            Some(SlashCommand::Autoresearch {
                eval_cmd: String::new(),
                goal: String::new()
            })
        );
        // Leerer Goal-Teil zählt ebenfalls als Usage-Fall.
        assert_eq!(
            parse_slash_command("/autoresearch cargo test :: "),
            Some(SlashCommand::Autoresearch {
                eval_cmd: String::new(),
                goal: String::new()
            })
        );
    }

    #[test]
    fn parse_autoresearch_self() {
        // Ohne Argumente → Defaults (Handler nutzt N=10, K=10).
        assert_eq!(
            parse_slash_command("/autoresearch.self"),
            Some(SlashCommand::AutoresearchSelf {
                suggestions: None,
                top: None
            })
        );
        // Nur N.
        assert_eq!(
            parse_slash_command("/autoresearch.self 5"),
            Some(SlashCommand::AutoresearchSelf {
                suggestions: Some(5),
                top: None
            })
        );
        // N und --top K.
        assert_eq!(
            parse_slash_command("/autoresearch.self 8 --top 3"),
            Some(SlashCommand::AutoresearchSelf {
                suggestions: Some(8),
                top: Some(3)
            })
        );
        // --top=K-Schreibweise, N kann fehlen.
        assert_eq!(
            parse_slash_command("/autoresearch.self --top=4"),
            Some(SlashCommand::AutoresearchSelf {
                suggestions: None,
                top: Some(4)
            })
        );
        // Kollidiert NICHT mit dem metrik-getriebenen /autoresearch.
        assert_eq!(
            parse_slash_command("/autoresearch cargo test :: mehr grün"),
            Some(SlashCommand::Autoresearch {
                eval_cmd: "cargo test".into(),
                goal: "mehr grün".into()
            })
        );
    }

    #[test]
    fn parse_wiki() {
        // Ohne Argument: Index anzeigen.
        assert_eq!(
            parse_slash_command("/wiki"),
            Some(SlashCommand::Wiki { arg: None })
        );
        // Mit Suchbegriff (darf Leerzeichen enthalten).
        assert_eq!(
            parse_slash_command("/wiki deployment ablauf"),
            Some(SlashCommand::Wiki {
                arg: Some("deployment ablauf".into())
            })
        );
        // "lint" ist ein reguläres Argument; der Handler unterscheidet.
        assert_eq!(
            parse_slash_command("/wiki lint"),
            Some(SlashCommand::Wiki {
                arg: Some("lint".into())
            })
        );
        // Nur Whitespace hinter /wiki → leeres Argument (Handler zeigt Index).
        assert_eq!(
            parse_slash_command("/wiki   "),
            Some(SlashCommand::Wiki { arg: None })
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
