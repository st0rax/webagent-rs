//! bot2bot-worker — Webbrain-Bridge.
//!
//! Ein Web-Brain läuft als autonomer bot2bot-Worker: pollt `agents/<brain>/inbox/`
//! auf neue `*.msg.txt` (Dedup via `state.json` `processed[]`), führt die Task über
//! die bestehende Controller-Loop aus (1:1 wie `webagent run`) und schreibt das
//! Ergebnis als Legacy-`msg.txt` an den Absender zurück (bot2bot send-Aequivalent:
//! history append + `agents/<from>/inbox/`).
//!
//! Lane: dieses Modul neu. config.rs nur lesen. Nicht angerührt: repl.rs /
//! circuit_breaker.rs / canary.rs. Die Controller-Logik wird wiederverwendet, nicht
//! dupliziert.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::bot2bot_root;

/// Geparste bot2bot-Nachricht (Legacy-Format wie `send.ps1`).
#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub from: String,
    pub to: String,
    pub time: String,
    pub subject: String,
    pub body: String,
}

impl Msg {
    /// Parst den Legacy-Dateiinhalt: Header `Key: Value` bis zur ersten Leerzeile,
    /// der Rest ist der Body. `From`/`To`/`Time` sind Pflicht; `Subject` optional.
    pub fn parse(content: &str) -> Option<Msg> {
        let mut from: Option<String> = None;
        let mut to: Option<String> = None;
        let mut time: Option<String> = None;
        let mut subject = String::new();
        let mut in_body = false;
        let mut body_lines: Vec<&str> = Vec::new();

        for line in content.lines() {
            if !in_body {
                if line.trim().is_empty() {
                    in_body = true;
                    continue;
                }
                if let Some(rest) = line.strip_prefix("From:") {
                    from = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("To:") {
                    to = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("Time:") {
                    time = Some(rest.trim().to_string());
                } else if let Some(rest) = line.strip_prefix("Subject:") {
                    subject = rest.trim().to_string();
                }
                // Unbekannte Header werden ignoriert (forward-compat).
            } else {
                body_lines.push(line);
            }
        }

        let from = from?;
        let to = to?;
        let time = time?;
        let body = body_lines.join("\n").trim().to_string();

        Some(Msg {
            from,
            to,
            time,
            subject,
            body,
        })
    }
}

/// Worker-Zustand pro Brain (`agents/<brain>/state.json`).
/// `processed[]` dient als Dedup/Watermark; `last_seen` wird bei jedem Poll erneuert.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerState {
    pub name: String,
    pub registered: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default)]
    pub processed: Vec<String>,
}

impl WorkerState {
    /// Lädt `state.json`; fehlt die Datei, wird ein leerer Default zurückgegeben.
    pub fn load(path: &Path) -> WorkerState {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(st) = serde_json::from_str::<WorkerState>(&s) {
                return st;
            }
        }
        WorkerState {
            name: String::new(),
            registered: crate::now_rfc3339(),
            last_seen: String::new(),
            processed: Vec::new(),
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, s)
    }

    pub fn is_processed(&self, name: &str) -> bool {
        self.processed.iter().any(|p| p == name)
    }
}

/// Autonomer bot2bot-Worker für ein Brain.
pub struct Bot2BotWorker {
    brain_id: String,
    bot2bot_root: PathBuf,
    poll_secs: u64,
    once: bool,
    max_cycles: usize,
    headless: bool,
}

impl Bot2BotWorker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        brain_id: String,
        bot2bot_root: PathBuf,
        poll_secs: u64,
        once: bool,
        max_cycles: usize,
        headless: bool,
    ) -> Self {
        Self {
            brain_id,
            bot2bot_root,
            poll_secs,
            once,
            max_cycles,
            headless,
        }
    }

    fn inbox_dir(&self) -> PathBuf {
        self.bot2bot_root
            .join("agents")
            .join(&self.brain_id)
            .join("inbox")
    }

    fn read_dir(&self) -> PathBuf {
        self.inbox_dir().join("_read")
    }

    fn state_path(&self) -> PathBuf {
        self.bot2bot_root
            .join("agents")
            .join(&self.brain_id)
            .join("state.json")
    }

    fn history_path(&self) -> PathBuf {
        self.bot2bot_root
            .join("agents")
            .join(&self.brain_id)
            .join("history.jsonl")
    }

    /// Noch nicht verarbeitete Tasks: `*.msg.txt` in `inbox/`, nicht in `_read/`,
    /// nicht in `processed[]`.
    fn pending_tasks(&self, state: &WorkerState) -> Vec<(PathBuf, Msg)> {
        let inbox = self.inbox_dir();
        let mut out = Vec::new();
        let entries = match fs::read_dir(&inbox) {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };
            if !name.ends_with(".msg.txt") {
                continue;
            }
            if state.is_processed(&name) {
                continue;
            }
            if let Ok(content) = fs::read_to_string(&path) {
                if let Some(msg) = Msg::parse(&content) {
                    out.push((path, msg));
                }
            }
        }
        out
    }

    /// Ein Poll-Durchlauf: alle anstehenden Tasks abarbeiten. Gibt die Anzahl der
    /// verarbeiteten Tasks zurück.
    pub fn poll_once(&self, profile: &Path) -> usize {
        // Heartbeat (v2): bei JEDEM Poll schreiben -> der Supervisor erkennt
        // haengende Worker ueber das Datei-Aenderungsdatum (auch bei idle-
        // Browser mit abgelaufener Session, die nie einen Task verarbeiten).
        let hb = self
            .bot2bot_root
            .join("workers")
            .join(format!("heartbeat_{}.json", self.brain_id));
        if let Some(parent) = hb.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let beat = serde_json::json!({
            "brain": self.brain_id,
            "pid": std::process::id(),
            "last_seen": crate::now_rfc3339(),
        });
        let _ = fs::write(&hb, serde_json::to_string(&beat).unwrap_or_default());

        let state_path = self.state_path();
        let mut state = WorkerState::load(&state_path);
        if state.name.is_empty() {
            state.name = self.brain_id.clone();
        }

        let pending = self.pending_tasks(&state);
        let count = pending.len();
        for (path, msg) in pending {
            self.process_task(&path, &msg, &mut state, profile);
        }

        if count > 0 {
            state.last_seen = crate::now_rfc3339();
            let _ = state.save(&state_path);
        }
        count
    }

    /// Verarbeitet EINE Task: Controller-Loop ausführen + Ergebnis zurück an Absender.
    /// Fehlerisolation: ein fehlgeschlagener Task wird trotzdem nach `_read/` verschoben
    /// und als `processed` markiert (mit `status=error` im Writeback) — die Schleife
    /// hängt nicht.
    fn process_task(&self, path: &Path, msg: &Msg, state: &mut WorkerState, profile: &Path) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let run_result = self.run_controller(&msg.body, profile);

        let (status, body) = match &run_result {
            Ok(meta) => (
                meta.status.clone(),
                format!(
                    "status={} run_id={} cycles={}",
                    meta.status, meta.run_id, meta.cycles
                ),
            ),
            Err(e) => (format!("error: {e}"), format!("status=error detail={e}")),
        };

        // Selbst-Nachrichten nicht zurück schreiben (Echo-Loop vermeiden).
        if msg.from != self.brain_id {
            let subject = if msg.subject.is_empty() {
                "RE: task".to_string()
            } else {
                format!("RE: {}", msg.subject)
            };
            self.writeback(&msg.from, &subject, &body);
        }

        // Egal ob Erfolg oder Fehler: Task als erledigt markieren.
        let _ = self.move_to_read(path);
        state.processed.push(name.clone());
        state.last_seen = crate::now_rfc3339();
        let _ = state.save(&self.state_path());

        eprintln!(
            "[bot2bot-worker] {brain} task={name} -> {status}",
            brain = self.brain_id
        );
    }

    /// Führt die Controller-Loop mit dem Task-Body aus — exakt wie `cmd_run`
    /// (Backend + Executor + Controller), kein eigener Controller-Code.
    /// `profile` ist das isolierte Laufzeit-Profil (Q5/Swarm-Mechanik), damit
    /// mehrere Worker-Prozesse parallel ohne SingletonLock-Konflikt laufen.
    fn run_controller(
        &self,
        task: &str,
        profile: &Path,
    ) -> Result<crate::run_store::RunMeta, String> {
        use crate::browser::WebBrainBackend;
        use crate::controller::AgentController;
        use crate::executor::PlatformShellExecutor;

        let backend = WebBrainBackend::from_config(&self.brain_id)?
            .with_profile_override(profile.to_path_buf());
        let executor = PlatformShellExecutor::new();
        let mut controller = AgentController::new(backend, executor, self.max_cycles);
        // Inbox tasks MUST always be fresh runs: never pass resume_id.
        // Resuming a prior run (or a mock conversation_ref) can yield a phantom
        // finish with cycles=1 and no browser/file work.
        controller.run(task, &self.brain_id, None, self.headless)
    }

    /// Schreibt das Ergebnis als Legacy-`msg.txt` in `agents/<from>/inbox/` plus
    /// append an `history.jsonl` (bot2bot send-Aequivalent).
    fn writeback(&self, from: &str, subject: &str, body: &str) {
        let inbox = self.bot2bot_root.join("agents").join(from).join("inbox");
        let _ = fs::create_dir_all(&inbox);
        let ts = crate::now_run_stamp();
        let file = inbox.join(format!("{ts}_to_{}.msg.txt", sanitize(from)));
        let content = format!(
            "From: {}\nTo: {}\nTime: {}\nSubject: {}\n\n{}",
            self.brain_id,
            from,
            crate::now_rfc3339(),
            subject,
            body
        );
        let _ = fs::write(&file, &content);

        let _ = append_line(&self.history_path(), &content);
    }

    fn move_to_read(&self, path: &Path) -> std::io::Result<()> {
        let read_dir = self.read_dir();
        fs::create_dir_all(&read_dir)?;
        if let Some(name) = path.file_name() {
            fs::rename(path, read_dir.join(name))
        } else {
            Ok(())
        }
    }

    /// Einstiegspunkt: `--once` = ein Durchlauf; sonst Poll-Loop mit `poll_secs`.
    /// Event-getrieben: nur bei neuen Tasks Aktion; bei leer still (kein Spam).
    pub fn run(&self) -> i32 {
        // Isoliertes Laufzeit-Profil vorbereiten (Q5/Swarm-Mechanik), damit N
        // Worker-Prozesse parallel ohne Chromium-SingletonLock-Konflikt laufen.
        let run_id = format!(
            "b2bw_{}_{}_{}",
            self.brain_id,
            std::process::id(),
            crate::now_run_stamp()
        );
        let profile = crate::config::prepare_swarm_profile(&run_id, &self.brain_id);
        let _guard = WorkerProfileGuard {
            run_id: run_id.clone(),
        };

        if self.once {
            self.poll_once(&profile);
            return 0;
        }
        loop {
            self.poll_once(&profile);
            thread::sleep(Duration::from_secs(self.poll_secs));
        }
    }
}

/// Räumt das isolierte Worker-Profil beim Ende des Worker-Prozesses auf
/// (Entspricht dem `SwarmCleanup`-Guard in `repl.rs`).
struct WorkerProfileGuard {
    run_id: String,
}

impl Drop for WorkerProfileGuard {
    fn drop(&mut self) {
        let _ = crate::config::cleanup_swarm_profiles(&self.run_id);
    }
}

/// Agent-Namen säubern, damit kein Pfadausbruch über den Empfänger-Namen möglich ist.
fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")
}

/// CLI-Einstiegspunkt. Wird von `main.rs` aufgerufen; der clap-Subcommand +
/// dispatch-Arm in `main.rs` ist "braucht wiring" (Claude).
pub fn run_bot2bot_worker(
    brain: &str,
    poll_secs: u64,
    once: bool,
    max_cycles: u32,
    headless: bool,
) -> i32 {
    let worker = Bot2BotWorker::new(
        brain.to_string(),
        bot2bot_root(),
        poll_secs,
        once,
        max_cycles as usize,
        headless,
    );
    worker.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp_root() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "test_b2bw_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    fn write_msg(root: &Path, brain: &str, name: &str, content: &str) {
        let dir = root.join("agents").join(brain).join("inbox");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    const SAMPLE: &str =
        "From: claude\nTo: qwen\nTime: 2026-07-17T06:34:12+02:00\nSubject: GO x\n\nMach das.\n";

    #[test]
    fn parse_msg_basic() {
        let m = Msg::parse(SAMPLE).unwrap();
        assert_eq!(m.from, "claude");
        assert_eq!(m.to, "qwen");
        assert_eq!(m.time, "2026-07-17T06:34:12+02:00");
        assert_eq!(m.subject, "GO x");
        assert_eq!(m.body, "Mach das.");
    }

    #[test]
    fn parse_msg_no_subject() {
        let c = "From: a\nTo: b\nTime: t\n\nHallo";
        let m = Msg::parse(c).unwrap();
        assert_eq!(m.subject, "");
        assert_eq!(m.body, "Hallo");
    }

    #[test]
    fn pending_tasks_respects_processed_and_read() {
        let root = tmp_root();
        write_msg(&root, "qwen", "20260717T010000_from_claude.msg.txt", SAMPLE);
        write_msg(&root, "qwen", "20260717T010001_from_grok.msg.txt", SAMPLE);
        // Datei in _read/ muss ignoriert werden.
        let read = root.join("agents/qwen/inbox/_read").join("old.msg.txt");
        fs::create_dir_all(read.parent().unwrap()).unwrap();
        fs::write(&read, SAMPLE).unwrap();

        let w = Bot2BotWorker::new("qwen".into(), root.clone(), 30, true, 5, true);
        let mut state = WorkerState::load(&w.state_path());
        state.name = "qwen".into();
        state
            .processed
            .push("20260717T010001_from_grok.msg.txt".into());
        let pending = w.pending_tasks(&state);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1.from, "claude");
    }

    #[test]
    fn writeback_creates_msg_and_history() {
        let root = tmp_root();
        let w = Bot2BotWorker::new("deepseek".into(), root.clone(), 30, true, 5, true);
        w.writeback("claude", "RE: GO", "status=done run_id=x cycles=3");

        let inbox = root.join("agents/claude/inbox");
        let files: Vec<_> = fs::read_dir(&inbox).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let content = fs::read_to_string(files[0].path()).unwrap();
        assert!(content.contains("From: deepseek"));
        assert!(content.contains("To: claude"));
        assert!(content.contains("status=done run_id=x cycles=3"));

        let hist = fs::read_to_string(w.history_path()).unwrap();
        assert!(hist.contains("status=done run_id=x cycles=3"));
    }

    #[test]
    fn move_to_read_and_state_recorded() {
        let root = tmp_root();
        write_msg(&root, "qwen", "20260717T010000_from_claude.msg.txt", SAMPLE);
        let w = Bot2BotWorker::new("qwen".into(), root.clone(), 30, true, 5, true);
        let path = root.join("agents/qwen/inbox/20260717T010000_from_claude.msg.txt");
        w.move_to_read(&path).unwrap();
        assert!(!path.exists());
        assert!(root
            .join("agents/qwen/inbox/_read/20260717T010000_from_claude.msg.txt")
            .exists());
    }

    #[test]
    fn poll_once_records_processed_and_lastseen() {
        let root = tmp_root();
        write_msg(&root, "qwen", "20260717T010000_from_claude.msg.txt", SAMPLE);
        let w = Bot2BotWorker::new("qwen".into(), root.clone(), 30, true, 5, true);

        // Browser-freier Buchhaltungs-Pfad: State wie in `poll_once` aktualisieren.
        let mut state = WorkerState::load(&w.state_path());
        state.name = "qwen".into();
        let pending = w.pending_tasks(&state);
        let count = pending.len();
        for (path, _msg) in pending {
            let _ = w.move_to_read(&path);
            state
                .processed
                .push(path.file_name().unwrap().to_str().unwrap().to_string());
        }
        state.last_seen = crate::now_rfc3339();
        state.save(&w.state_path()).unwrap();

        assert_eq!(count, 1);
        let state = WorkerState::load(&w.state_path());
        assert!(state.is_processed("20260717T010000_from_claude.msg.txt"));
        assert!(!state.last_seen.is_empty());

        // Zweiter Poll findet nichts mehr.
        assert_eq!(w.pending_tasks(&state).len(), 0);
    }
}
