//! comms — webagent-eigene Agent-zu-Agent-Kommunikation.
//!
//! Ersetzt die externe `bot2bot`-Abhängigkeit: Nachrichten werden in webagents
//! eigenem Datenverzeichnis gehalten (`data/comms/`). append-only `history.jsonl`
//! als vollständiges Protokoll, plus `inbox/<agent>.jsonl` als ungelesene Queue
//! pro Empfänger. Damit ist webagent für internes Messaging self-contained.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Eine Nachricht zwischen zwei Agenten/Entitäten.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub ts: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    #[serde(default)]
    pub in_reply_to: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "sent".to_string()
}

/// Datei-basierter Nachrichten-Store unter einem Verzeichnis (`data/comms/`).
pub struct CommsStore {
    dir: PathBuf,
}

impl CommsStore {
    /// Öffnet/erstellt den Store unter `dir` (legt `dir` + `inbox/` an).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = fs::create_dir_all(dir.join("inbox"));
        Self { dir }
    }

    /// Store unter dem Standardpfad `data/comms/`.
    pub fn default_store() -> Self {
        Self::new(crate::config::data_dir().join("comms"))
    }

    fn history_path(&self) -> PathBuf {
        self.dir.join("history.jsonl")
    }

    fn inbox_path(&self, agent: &str) -> PathBuf {
        // Agent-Slug säubern, damit keine Pfadausbrüche möglich sind.
        let slug: String = agent
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        self.dir.join("inbox").join(format!("{slug}.jsonl"))
    }

    fn new_id() -> String {
        // Zeitstempel + kleiner Zufallssuffix (ohne externe uuid-Crate).
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        format!(
            "{}_{:08x}",
            crate::now_run_stamp(),
            (nanos ^ pid).wrapping_mul(0x9e3779b9)
        )
    }

    fn append_line(path: &Path, msg: &Message) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{line}")
    }

    fn read_lines(path: &Path) -> Vec<Message> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Message>(l).ok())
            .collect()
    }

    /// Sendet eine Nachricht: an die History anhängen UND in die Empfänger-Inbox.
    pub fn send(
        &self,
        from: &str,
        to: &str,
        subject: &str,
        body: &str,
        in_reply_to: Option<String>,
    ) -> std::io::Result<Message> {
        let msg = Message {
            id: Self::new_id(),
            ts: crate::now_rfc3339(),
            from: from.to_string(),
            to: to.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            in_reply_to,
            status: "sent".to_string(),
        };
        Self::append_line(&self.history_path(), &msg)?;
        Self::append_line(&self.inbox_path(to), &msg)?;
        Ok(msg)
    }

    /// Ungelesene Nachrichten eines Agenten (ohne sie zu entfernen).
    pub fn inbox(&self, agent: &str) -> Vec<Message> {
        Self::read_lines(&self.inbox_path(agent))
    }

    /// Ungelesene Nachrichten holen UND als gelesen markieren (Inbox leeren).
    pub fn drain_inbox(&self, agent: &str) -> Vec<Message> {
        let path = self.inbox_path(agent);
        let msgs = Self::read_lines(&path);
        if !msgs.is_empty() {
            let _ = fs::remove_file(&path);
        }
        msgs
    }

    /// Letzte `n` Einträge aus der vollständigen History (chronologisch).
    pub fn history_tail(&self, n: usize) -> Vec<Message> {
        let all = Self::read_lines(&self.history_path());
        let start = all.len().saturating_sub(n);
        all[start..].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "test_comms_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn send_lands_in_history_and_inbox() {
        let store = CommsStore::new(unique_dir());
        let m = store.send("claude", "qwen", "Plan", "Bitte bewerten", None).unwrap();
        assert_eq!(m.from, "claude");
        assert_eq!(m.to, "qwen");
        let inbox = store.inbox("qwen");
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].subject, "Plan");
        assert_eq!(store.history_tail(10).len(), 1);
    }

    #[test]
    fn drain_marks_read() {
        let store = CommsStore::new(unique_dir());
        store.send("a", "b", "s1", "b1", None).unwrap();
        store.send("a", "b", "s2", "b2", None).unwrap();
        let drained = store.drain_inbox("b");
        assert_eq!(drained.len(), 2);
        // Nach dem Drainen ist die Inbox leer, die History bleibt.
        assert!(store.inbox("b").is_empty());
        assert_eq!(store.history_tail(10).len(), 2);
    }

    #[test]
    fn inboxes_are_per_recipient() {
        let store = CommsStore::new(unique_dir());
        store.send("a", "b", "s", "hi b", None).unwrap();
        store.send("a", "c", "s", "hi c", None).unwrap();
        assert_eq!(store.inbox("b").len(), 1);
        assert_eq!(store.inbox("c").len(), 1);
        assert_eq!(store.inbox("b")[0].body, "hi b");
    }

    #[test]
    fn reply_threads_via_in_reply_to() {
        let store = CommsStore::new(unique_dir());
        let first = store.send("a", "b", "frage", "?", None).unwrap();
        let reply = store
            .send("b", "a", "antwort", "!", Some(first.id.clone()))
            .unwrap();
        assert_eq!(reply.in_reply_to, Some(first.id));
    }

    #[test]
    fn slug_prevents_path_escape() {
        let store = CommsStore::new(unique_dir());
        // Bösartiger Empfängername darf keinen Pfadausbruch erzeugen.
        store.send("a", "../../evil", "s", "b", None).unwrap();
        // Landet in einem gesäuberten Inbox-Namen, nicht ausserhalb.
        assert_eq!(store.inbox("../../evil").len(), 1);
    }
}
