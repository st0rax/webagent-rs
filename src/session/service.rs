//! SessionService – UI-neutraler Halt fuer lebende Lauefe und ihre Event-Stroeme.
//!
//! Der Service verwaltet, welche Lauefe existieren, welchen Status sie haben
//! und wo ihr geordneter Event-Strom liegt. Er kennt weder Webview noch TUI
//! noch Brain; derselbe Lauf kann aus verschiedenen Sichten (Web-UI,
//! Responses-SSE, REPL) beobachtet werden. Terminales [`SessionEvent::Done`]
//! schliesst einen Lauf fuer weitere Events und setzt den Laufstatus.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::events::{EventStream, SessionEvent, Since};
use crate::source_scope::{QuelleCommand, QuelleReport, SourceScope};

/// Lebender Lauf samt Stream. Gleichzeitig abgreifbar; der Zustand liegt
/// geteilt hinter einer [`Mutex`], die Sequenznummern bleiben stabil.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    core: Arc<Mutex<SessionCore>>,
}

#[derive(Debug)]
struct SessionCore {
    run_id: String,
    brain: String,
    task: String,
    created_at: String,
    status: String,
    stream: EventStream,
    sources: SourceScope,
}

impl SessionHandle {
    pub fn run_id(&self) -> String {
        self.core.lock().unwrap().run_id.clone()
    }

    pub fn brain(&self) -> String {
        self.core.lock().unwrap().brain.clone()
    }

    pub fn task(&self) -> String {
        self.core.lock().unwrap().task.clone()
    }

    pub fn created_at(&self) -> String {
        self.core.lock().unwrap().created_at.clone()
    }

    pub fn status(&self) -> String {
        self.core.lock().unwrap().status.clone()
    }

    /// Hoeheste ausgegebene Sequenznummer des Stroms.
    pub fn last_seq(&self) -> u64 {
        self.core.lock().unwrap().stream.last_seq()
    }

    /// Terminal geschlossen?
    pub fn is_done(&self) -> bool {
        self.core.lock().unwrap().stream.is_done()
    }

    /// Events strikt ab `seq` (exklusiv); siehe [`Since`] fuer Luecken.
    pub fn events_since(&self, seq: u64) -> Since {
        self.core.lock().unwrap().stream.events_since(seq)
    }

    /// Legt ein Event ab und aktualisiert den Laufstatus.
    pub fn push(&self, event: SessionEvent) -> Result<u64, String> {
        let mut core = self.core.lock().unwrap();
        let seq = core.stream.push(event.clone())?;
        match &event {
            SessionEvent::Started { .. } => core.status = "running".to_string(),
            SessionEvent::Status { state } => core.status = state.clone(),
            SessionEvent::Error { .. } => core.status = "error".to_string(),
            SessionEvent::Done { status } => core.status = status.clone(),
            _ => {}
        }
        Ok(seq)
    }

    /// Momentaufnahme fuer Listen/UI.
    pub fn snapshot(&self) -> SessionSnapshot {
        let core = self.core.lock().unwrap();
        let active = core.sources.active(&core.brain);
        SessionSnapshot {
            run_id: core.run_id.clone(),
            brain: core.brain.clone(),
            task: core.task.clone(),
            status: core.status.clone(),
            created_at: core.created_at.clone(),
            last_seq: core.stream.last_seq(),
            done: core.stream.is_done(),
            source: active.source,
            source_kind: active.kind.as_str().to_string(),
            source_persisted: active.persisted,
        }
    }

    pub fn active_source(&self) -> crate::source_scope::ActiveSource {
        let core = self.core.lock().unwrap();
        core.sources.active(&core.brain)
    }

    pub fn apply_quelle(&self, cmd: &QuelleCommand) -> Result<QuelleReport, String> {
        self.core.lock().unwrap().sources.apply(cmd)
    }
}

/// Leichte, serialisierbare Momentaufnahme eines Laufs.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionSnapshot {
    pub run_id: String,
    pub brain: String,
    pub task: String,
    pub status: String,
    pub created_at: String,
    pub last_seq: u64,
    pub done: bool,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_source_kind")]
    pub source_kind: String,
    #[serde(default)]
    pub source_persisted: bool,
}

fn default_source() -> String {
    crate::source_scope::BROWSER_SOURCE.to_string()
}

fn default_source_kind() -> String {
    "browser".to_string()
}

/// Registrierung der lebenden Lauefe. Pro Prozess eine Instanz Ihrer Wahl —
/// fuer Tests einfach eine frische [`SessionService::new`].
#[derive(Debug, Clone, Default)]
pub struct SessionService {
    sessions: Arc<Mutex<HashMap<String, SessionHandle>>>,
}

impl SessionService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registriert einen neuen Lauf mit leerem Strom. Schlaegt fehl, wenn der
    /// Lauf bereits registriert ist (eine Session pro Run).
    pub fn start(&self, run_id: &str, brain: &str, task: &str) -> Result<SessionHandle, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.contains_key(run_id) {
            return Err(format!("Lauf {run_id:?} ist bereits registriert"));
        }
        let core = SessionCore {
            run_id: run_id.to_string(),
            brain: brain.to_string(),
            task: task.to_string(),
            created_at: crate::now_rfc3339(),
            status: "registered".to_string(),
            stream: EventStream::new(run_id),
            sources: SourceScope::load_default(),
        };
        let handle = SessionHandle {
            core: Arc::new(Mutex::new(core)),
        };
        sessions.insert(run_id.to_string(), handle.clone());
        Ok(handle)
    }

    pub fn get(&self, run_id: &str) -> Option<SessionHandle> {
        self.sessions.lock().unwrap().get(run_id).cloned()
    }

    /// Event auf den registrierten Lauf legen.
    pub fn push(&self, run_id: &str, event: SessionEvent) -> Result<u64, String> {
        let handle = self
            .get(run_id)
            .ok_or_else(|| format!("Lauf {run_id:?} ist nicht registriert"))?;
        handle.push(event)
    }

    pub fn events_since(&self, run_id: &str, seq: u64) -> Option<Since> {
        self.get(run_id).map(|h| h.events_since(seq))
    }

    /// Snapshot eines Laufs fuer Listen/UI.
    pub fn snapshot(&self, run_id: &str) -> Option<SessionSnapshot> {
        self.get(run_id).map(|h| h.snapshot())
    }

    /// Alle registrierten Laufe als Snapshots, neueste zuerst (Run-Stempel).
    pub fn list(&self) -> Vec<SessionSnapshot> {
        let mut out: Vec<SessionSnapshot> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .map(|h| h.snapshot())
            .collect();
        out.sort_by(|a, b| b.run_id.cmp(&a.run_id));
        out
    }

    /// Entfernt einen registrierten Lauf (z. B. nach Ablauf der Aufbewahrung).
    pub fn remove(&self, run_id: &str) -> bool {
        self.sessions.lock().unwrap().remove(run_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start_test(service: &SessionService, id: &str) -> SessionHandle {
        service
            .start(id, "claude", format!("aufgabe von {id}").as_str())
            .unwrap()
    }

    #[test]
    fn start_get_snapshot_und_doppelte_registrierung() {
        let service = SessionService::new();
        let handle = start_test(&service, "run-1");
        assert_eq!(handle.run_id(), "run-1");
        assert_eq!(handle.brain(), "claude");
        assert_eq!(handle.status(), "registered");
        assert_eq!(handle.last_seq(), 0);

        assert!(service
            .start("run-1", "claude", "nochmal")
            .unwrap_err()
            .contains("bereits registriert"));

        let snap = service.snapshot("run-1").unwrap();
        assert_eq!(snap.run_id, "run-1");
        assert!(!snap.done);
        assert!(service.get("unbekannt").is_none());
    }

    #[test]
    fn status_folgt_den_events_runter_auf_terminal() {
        let service = SessionService::new();
        let handle = start_test(&service, "run-s");
        handle
            .push(SessionEvent::Started {
                run_id: "run-s".to_string(),
                brain: "claude".to_string(),
                task: "t".to_string(),
            })
            .unwrap();
        assert_eq!(handle.status(), "running");
        handle
            .push(SessionEvent::Status {
                state: "login_required".to_string(),
            })
            .unwrap();
        assert_eq!(handle.status(), "login_required");
        handle
            .push(SessionEvent::Done {
                status: "done".to_string(),
            })
            .unwrap();
        assert_eq!(handle.status(), "done");
        assert!(handle.is_done());
        assert!(handle
            .push(SessionEvent::TextDelta {
                text: "zu spaet".to_string(),
            })
            .is_err());
        let snap = handle.snapshot();
        assert!(snap.done);
        assert_eq!(snap.status, "done");
    }

    #[test]
    fn error_event_setzt_status_aber_endet_nicht_terminal() {
        let service = SessionService::new();
        let handle = start_test(&service, "run-e");
        handle
            .push(SessionEvent::Error {
                message: "kaputt".to_string(),
            })
            .unwrap();
        assert_eq!(handle.status(), "error");
        // Fehler ist kein Terminalzustand: ein retry darf weiter pushen.
        handle
            .push(SessionEvent::Done {
                status: "error".to_string(),
            })
            .unwrap();
    }

    #[test]
    fn service_push_events_since_und_remove() {
        let service = SessionService::new();
        let handle = start_test(&service, "run-p");
        let s1 = service
            .push(
                "run-p",
                SessionEvent::TextDelta {
                    text: "hallo".to_string(),
                },
            )
            .unwrap();
        let s2 = service
            .push(
                "run-p",
                SessionEvent::TextDelta {
                    text: " welt".to_string(),
                },
            )
            .unwrap();
        let since = service.events_since("run-p", s1).unwrap();
        assert_eq!(since.events().len(), 1);
        assert_eq!(since.events()[0].seq, s2);
        assert_eq!(handle.last_seq(), s2);

        assert_eq!(service.events_since("fremd", 0), None);
        assert_eq!(
            service.push("fremd", SessionEvent::TextDelta { text: "x".into() }),
            Err("Lauf \"fremd\" ist nicht registriert".to_string())
        );
        assert!(service.remove("run-p"));
        assert_eq!(service.len(), 0);
        assert!(service.is_empty());
    }

    #[test]
    fn list_sortiert_neueste_zuerst_und_ist_isolierbar() {
        let a = SessionService::new();
        let b = SessionService::new();
        start_test(&a, "run-1");
        start_test(&a, "run-2");
        start_test(&b, "run-other");
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 1);

        let list = a.list();
        assert_eq!(list.len(), 2);
        assert!(list[0].run_id > list[1].run_id, "neueste zuerst: {list:?}");
        // Isoliert: b sieht nichts von a.
        assert!(b.list().iter().all(|s| s.run_id == "run-other"));
    }

    #[test]
    fn session_source_default_ist_browser_und_isoliert() {
        let service = SessionService::new();
        let a = start_test(&service, "run-src-a");
        let b = start_test(&service, "run-src-b");
        assert_eq!(a.snapshot().source, "default");
        assert_eq!(a.snapshot().source_kind, "browser");
        let cmd = crate::source_scope::parse_quelle_args("claude openrouter").unwrap();
        let report = a.apply_quelle(&cmd).unwrap();
        assert!(!report.persisted);
        assert_eq!(a.snapshot().source, "openrouter");
        assert_eq!(a.snapshot().source_kind, "api");
        assert_eq!(b.snapshot().source, "default");
        assert_eq!(b.snapshot().source_kind, "browser");
    }
}
