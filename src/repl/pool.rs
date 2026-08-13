//! Langlebige Abfrage-Pools und isolierte Einzel-Abfragen gegen Brain-Backends
//! für Benchmark, Swarm und Self-Research.
//!
//! Aus `repl::mod` extrahiert (Schritt 7) — reine Moves, keine Logikänderung.

use std::sync::{mpsc, Mutex};

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::timeouts::resolve_timeout;

use super::ReplSession;

/// Ein voller Frage-Zyklus gegen ein frisches, isoliertes Brain-Backend:
/// start → ensure_ready → new_chat → send → wait_response → stop. Für Swarm und
/// Self-Research, wo jedes Brain der Reihe nach in einem eigenen Laufzeit-Profil
/// (`profile_override`, Swarm-Teilkopie) befragt wird. Bucht Circuit-Breaker- und
/// Reliability-Ereignisse; blockierte Antworten (Tageslimit/Login/Cloudflare)
/// zählen als Fehler statt als Beitrag (siehe [[external-blocks-flag-not-fail]]).
pub fn isolated_query(
    brain_id: &str,
    prompt: &str,
    headless: bool,
    profile_override: Option<std::path::PathBuf>,
) -> Result<String, String> {
    // Wiederholt blockierte/fehlgeschlagene Brains werden fuer eine Cooldown-
    // Zeit uebersprungen statt bei jedem Aufruf erneut den vollen Timeout zu
    // kosten (siehe circuit_breaker.rs).
    if let Some(remaining) = crate::circuit_breaker::check(brain_id) {
        return Err(format!(
            "circuit_open: uebersprungen, noch {remaining}s Cooldown"
        ));
    }
    let started = std::time::Instant::now();
    let prompt_chars = prompt.chars().count();
    crate::bench_events::emit(
        crate::bench_events::Level::Progress,
        Some(brain_id),
        "Browser wird gestartet und Sitzung geprüft…",
    );
    let mut backend = WebBrainBackend::from_config(brain_id)?;
    if let Some(p) = profile_override {
        backend = backend.with_profile_override(p);
    }
    backend.start(headless)?;
    let ready_to = resolve_timeout("ensure_ready", brain_id, "", None);
    let state = backend
        .ensure_ready(ready_to)
        .unwrap_or(SessionState::Error);
    if state != SessionState::Ready {
        let _ = backend.stop();
        let label = ReplSession::state_label(state).to_string();
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
    crate::bench_events::emit(
        crate::bench_events::Level::Progress,
        Some(brain_id),
        "Sitzung bereit; Eingabe wird gesendet…",
    );
    let _ = backend.new_chat();
    let baseline = backend.send(prompt).inspect_err(|_| {
        let _ = backend.stop();
    })?;
    let wait_to = resolve_timeout("wait_response", brain_id, prompt, None);
    crate::bench_events::emit(
        crate::bench_events::Level::Progress,
        Some(brain_id),
        "Antwort wird abgewartet…",
    );
    let out = match backend.wait_response(baseline, wait_to) {
        // Externe Blockierung (Tageslimit/Login/Cloudflare) ist kein Beitrag zur
        // Zusammenführung -- sonst landet die Limit-Seite als vermeintliche
        // "Antwort" im Ergebnis (siehe [[external-blocks-flag-not-fail]]).
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
            crate::brain_score::record_event(brain_id, false, Some(e), latency_ms, prompt_chars);
        }
    }
    out
}

/// Langlebiger Abfrage-Pool für einen Benchmark-Run. Jeder Brain besitzt genau
/// einen eigenen Thread; damit bleibt die WebView-Thread-Affinität gewahrt und
/// dieselbe Browser-Sitzung wird zwischen Sammeln, Rangwahl und Ratifikation
/// wiederverwendet.
pub struct PersistentQueryPool {
    workers: std::collections::HashMap<String, mpsc::Sender<PersistentCommand>>,
    joins: Mutex<Vec<std::thread::JoinHandle<()>>>,
}

enum PersistentCommand {
    Query {
        prompt: String,
        fresh_chat: bool,
        reply: mpsc::Sender<Result<String, String>>,
    },
    Shutdown,
}

impl PersistentQueryPool {
    pub fn new(brains: &[String], headless: bool) -> Self {
        let mut workers = std::collections::HashMap::new();
        let mut joins = Vec::new();
        for brain in brains {
            if workers.contains_key(brain) {
                continue;
            }
            let (tx, rx) = mpsc::channel();
            let brain_id = brain.clone();
            let join = std::thread::spawn(move || {
                let mut session = PersistentBrain::new(&brain_id, headless);
                while let Ok(command) = rx.recv() {
                    match command {
                        PersistentCommand::Query {
                            prompt,
                            fresh_chat,
                            reply,
                        } => {
                            let _ = reply.send(session.query(&prompt, fresh_chat));
                        }
                        PersistentCommand::Shutdown => break,
                    }
                }
                session.shutdown();
            });
            workers.insert(brain.clone(), tx);
            joins.push(join);
        }
        Self {
            workers,
            joins: Mutex::new(joins),
        }
    }

    pub fn query(&self, brain_id: &str, prompt: &str) -> Result<String, String> {
        self.query_fresh(brain_id, prompt)
    }

    /// Sendet eine unabhängige Frage in einen frischen Chat, behält aber den
    /// Browser-Prozess und das Profil des Brains bei.
    pub fn query_fresh(&self, brain_id: &str, prompt: &str) -> Result<String, String> {
        self.send_query(brain_id, prompt, true)
    }

    /// Setzt die aktuelle Unterhaltung desselben Brains fort. Das ist für
    /// Refinement- und Repair-Schritte gedacht, die auf vorherigem Kontext
    /// aufbauen; insbesondere wird dabei kein `new_chat` ausgelöst.
    pub fn query_continue(&self, brain_id: &str, prompt: &str) -> Result<String, String> {
        self.send_query(brain_id, prompt, false)
    }

    fn send_query(&self, brain_id: &str, prompt: &str, fresh_chat: bool) -> Result<String, String> {
        let sender = self
            .workers
            .get(brain_id)
            .ok_or_else(|| format!("kein persistenter Worker für {brain_id}"))?;
        let (reply_tx, reply_rx) = mpsc::channel();
        sender
            .send(PersistentCommand::Query {
                prompt: prompt.to_string(),
                fresh_chat,
                reply: reply_tx,
            })
            .map_err(|_| format!("persistente Sitzung {brain_id} beendet"))?;
        reply_rx
            .recv()
            .map_err(|_| format!("persistente Sitzung {brain_id} antwortet nicht"))?
    }
}

impl Drop for PersistentQueryPool {
    fn drop(&mut self) {
        for worker in self.workers.values() {
            let _ = worker.send(PersistentCommand::Shutdown);
        }
        if let Ok(mut joins) = self.joins.lock() {
            for join in joins.drain(..) {
                let _ = join.join();
            }
        }
    }
}

struct PersistentBrain {
    brain_id: String,
    backend: Option<WebBrainBackend>,
    headless: bool,
    started: bool,
}

impl PersistentBrain {
    fn new(brain_id: &str, headless: bool) -> Self {
        Self {
            brain_id: brain_id.to_string(),
            backend: None,
            headless,
            started: false,
        }
    }

    fn backend(&mut self) -> Result<&mut WebBrainBackend, String> {
        if self.backend.is_none() {
            self.backend = Some(WebBrainBackend::from_config(&self.brain_id)?);
        }
        self.backend
            .as_mut()
            .ok_or_else(|| "Backend fehlt".to_string())
    }

    fn query(&mut self, prompt: &str, fresh_chat: bool) -> Result<String, String> {
        if let Some(remaining) = crate::circuit_breaker::check(&self.brain_id) {
            return Err(format!(
                "circuit_open: uebersprungen, noch {remaining}s Cooldown"
            ));
        }
        let started_at = std::time::Instant::now();
        let prompt_chars = prompt.chars().count();
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(&self.brain_id),
            if self.started {
                "Bestehende Browser-Sitzung wird wiederverwendet…"
            } else {
                "Browser-Sitzung wird einmalig gestartet…"
            },
        );
        if !self.started {
            let headless = self.headless;
            self.backend()?.start(headless)?;
            self.started = true;
        }
        let ready_to = resolve_timeout("ensure_ready", &self.brain_id, "", None);
        let state = self
            .backend()?
            .ensure_ready(ready_to)
            .unwrap_or(SessionState::Error);
        if state != SessionState::Ready {
            self.shutdown();
            let label = ReplSession::state_label(state).to_string();
            crate::circuit_breaker::record_failure(&self.brain_id, &label);
            crate::brain_score::record_event(
                &self.brain_id,
                false,
                Some(&label),
                started_at.elapsed().as_millis() as u64,
                prompt_chars,
            );
            return Err(label);
        }
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(&self.brain_id),
            "Sitzung bereit; Eingabe wird gesendet…",
        );
        if fresh_chat {
            self.backend()?.new_chat()?;
        }
        let baseline = self.backend()?.send(prompt)?;
        let wait_to = resolve_timeout("wait_response", &self.brain_id, prompt, None);
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some(&self.brain_id),
            "Antwort wird abgewartet…",
        );
        let out = match self.backend()?.wait_response(baseline, wait_to) {
            Ok(resp) if resp.backend_status == "blocked" || resp.backend_status == "rate_limit" => {
                Err(format!("blockiert: {}", resp.text.trim()))
            }
            Ok(resp) if !resp.text.trim().is_empty() => Ok(resp.text.trim().to_string()),
            Ok(resp) => Err(format!("keine Antwort (status={})", resp.backend_status)),
            Err(error) => Err(error),
        };
        let latency_ms = started_at.elapsed().as_millis() as u64;
        match &out {
            Ok(_) => crate::circuit_breaker::record_success(&self.brain_id),
            Err(error) => crate::circuit_breaker::record_failure(&self.brain_id, error),
        }
        crate::brain_score::record_event(
            &self.brain_id,
            out.is_ok(),
            out.as_ref().err().map(String::as_str),
            latency_ms,
            prompt_chars,
        );
        out
    }

    fn shutdown(&mut self) {
        if self.started {
            if let Some(backend) = self.backend.as_mut() {
                let _ = backend.stop();
            }
            self.started = false;
        }
    }
}
