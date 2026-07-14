//! Lokaler, brain-unabhängiger Langzeitspeicher (JSON-Lines-basiert).

use lazy_static::lazy_static;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

lazy_static! {
    /// Globale Sperre für alle Memory-Schreiboperationen, um Race Conditions
    /// bei parallelen Threads zu vermeiden (next_id + append müssen atomar sein).
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

static TOKEN_RE: OnceLock<Regex> = OnceLock::new();

fn token_regex() -> &'static Regex {
    TOKEN_RE.get_or_init(|| Regex::new(r"[A-Za-zÄÖÜäöüß0-9_-]{3,}").unwrap())
}

static STOP_WORDS: &[&str] = &[
    "aber", "alle", "auch", "dass", "eine", "einen", "einer", "eines", "fuer", "für", "haben",
    "hier", "nicht", "oder", "soll", "und", "von", "werden", "with", "from", "that", "this", "the",
    "webagent",
];

fn tokens(text: &str) -> HashSet<String> {
    token_regex()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|t| !STOP_WORDS.contains(&t.as_str()))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryEntry {
    pub id: u64,
    pub scope: String,
    pub kind: String,
    pub content: String,
    pub source: String,
    pub importance: f64,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredEntry {
    id: u64,
    scope: String,
    kind: String,
    content: String,
    source: String,
    importance: f64,
    created_at: String,
    updated_at: String,
}

pub struct MemoryStore {
    path: PathBuf,
}

impl MemoryStore {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }
        Self { path }
    }

    /// Fügt eine neue Erinnerung hinzu oder gibt die ID einer existierenden zurück.
    pub fn add(
        &self,
        content: &str,
        scope: &str,
        kind: &str,
        source: Option<&str>,
        importance: f64,
    ) -> Result<u64, String> {
        let text = content.trim();
        if text.is_empty() {
            return Err("Erinnerung darf nicht leer sein".to_string());
        }
        if text.len() > 8000 {
            return Err("Erinnerung ist zu lang (maximal 8000 Zeichen)".to_string());
        }

        let importance = importance.clamp(0.0, 1.0);
        let now = crate::now_rfc3339();
        let source = source
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("manual:{}", uuid_simple()));

        // Globale Sperre für next_id + append (atomar gegen parallele Threads)
        let _guard = WRITE_LOCK.lock().unwrap();

        // Prüfe, ob source bereits existiert
        if let Some(existing) = self.find_by_source(&source) {
            return Ok(existing.id);
        }

        let id = self.next_id();
        let entry = StoredEntry {
            id,
            scope: scope.to_string(),
            kind: kind.to_string(),
            content: text.to_string(),
            source,
            importance,
            created_at: now.clone(),
            updated_at: now,
        };

        self.append_entry(&entry)?;
        Ok(id)
    }

    /// Löscht eine Erinnerung anhand ihrer ID.
    pub fn delete(&self, memory_id: u64) -> Result<bool, String> {
        let _guard = WRITE_LOCK.lock().unwrap();

        let entries = self.load_all()?;
        let original_len = entries.len();
        let filtered: Vec<_> = entries.into_iter().filter(|e| e.id != memory_id).collect();

        if filtered.len() == original_len {
            return Ok(false);
        }

        self.write_all(&filtered)?;
        Ok(true)
    }

    /// Listet die neuesten Erinnerungen auf.
    pub fn list(&self, limit: usize) -> Result<Vec<MemoryEntry>, String> {
        let mut entries = self.load_all()?;
        entries.sort_by_key(|b| std::cmp::Reverse(b.id));
        Ok(entries
            .into_iter()
            .take(limit.max(1))
            .map(|e| self.to_memory_entry(&e))
            .collect())
    }

    /// Sucht Erinnerungen basierend auf Token-Overlap und Ranking.
    /// Reihenfolge wie Python: `ORDER BY id DESC` vor dem Scoring.
    pub fn search(
        &self,
        query: &str,
        scopes: &[&str],
        limit: usize,
    ) -> Result<Vec<MemoryEntry>, String> {
        let mut entries = self.load_all()?;
        entries.sort_by_key(|e| std::cmp::Reverse(e.id));
        let query_tokens = tokens(query);

        let mut scored: Vec<(f64, StoredEntry)> = Vec::new();

        for (position, entry) in entries
            .into_iter()
            .filter(|e| scopes.contains(&e.scope.as_str()))
            .take(1000)
            .enumerate()
        {
            let content_tokens = tokens(&entry.content);
            let overlap = query_tokens.intersection(&content_tokens).count();

            // Episode ohne Overlap überspringen
            if !query_tokens.is_empty() && overlap == 0 && entry.kind == "episode" {
                continue;
            }

            let recency = 1.0 / (position as f64 + 1.0).sqrt();
            let score = (overlap as f64) * 3.0 + entry.importance + recency;
            scored.push((score, entry));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored
            .into_iter()
            .take(limit.max(1))
            .map(|(_, e)| self.to_memory_entry(&e))
            .collect())
    }

    /// Zeichnet einen abgeschlossenen Run als Episode auf.
    pub fn record_run(&self, meta: &RunMeta) -> Result<Option<u64>, String> {
        if meta.status != "done" {
            return Ok(None);
        }

        let final_messages: Vec<_> = meta
            .completed_actions
            .values()
            .filter(|v| *v != "finish" && !v.starts_with("[Terminal-Ausgabe"))
            .collect();

        let result = final_messages
            .last()
            .map(|s| s.as_str())
            .unwrap_or("Erfolgreich abgeschlossen.");

        let content = format!("Aufgabe: {}\nErgebnis: {}", meta.task, result);
        let content = if content.len() > 8000 {
            &content[..8000]
        } else {
            &content
        };

        let id = self.add(
            content,
            "shared",
            "episode",
            Some(&format!("run:{}", meta.run_id)),
            0.35,
        )?;

        Ok(Some(id))
    }

    // === Hilfsfunktionen ===

    fn find_by_source(&self, source: &str) -> Option<StoredEntry> {
        self.load_all()
            .ok()?
            .into_iter()
            .find(|e| e.source == source)
    }

    fn next_id(&self) -> u64 {
        self.load_all()
            .ok()
            .and_then(|entries| entries.iter().map(|e| e.id).max())
            .map(|max_id| max_id + 1)
            .unwrap_or(1)
    }

    fn append_entry(&self, entry: &StoredEntry) -> Result<(), String> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("Fehler beim Öffnen von {:?}: {}", self.path, e))?;

        let line = serde_json::to_string(entry)
            .map_err(|e| format!("Fehler beim Serialisieren: {}", e))?;

        writeln!(file, "{}", line).map_err(|e| format!("Fehler beim Schreiben: {}", e))?;

        Ok(())
    }

    fn load_all(&self) -> Result<Vec<StoredEntry>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)
            .map_err(|e| format!("Fehler beim Öffnen von {:?}: {}", self.path, e))?;

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Fehler beim Lesen: {}", e))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: StoredEntry = serde_json::from_str(&line)
                .map_err(|e| format!("Fehler beim Deserialisieren: {}", e))?;
            entries.push(entry);
        }

        Ok(entries)
    }

    fn write_all(&self, entries: &[StoredEntry]) -> Result<(), String> {
        let tmp_path = self.path.with_extension("tmp");

        {
            let mut file = File::create(&tmp_path)
                .map_err(|e| format!("Fehler beim Erstellen von {:?}: {}", tmp_path, e))?;

            for entry in entries {
                let line = serde_json::to_string(entry)
                    .map_err(|e| format!("Fehler beim Serialisieren: {}", e))?;
                writeln!(file, "{}", line).map_err(|e| format!("Fehler beim Schreiben: {}", e))?;
            }
        }

        fs::rename(&tmp_path, &self.path).map_err(|e| format!("Fehler beim Umbenennen: {}", e))?;

        Ok(())
    }

    fn to_memory_entry(&self, stored: &StoredEntry) -> MemoryEntry {
        MemoryEntry {
            id: stored.id,
            scope: stored.scope.clone(),
            kind: stored.kind.clone(),
            content: stored.content.clone(),
            source: stored.source.clone(),
            importance: stored.importance,
            created_at: stored.created_at.clone(),
        }
    }
}

// Einfache UUID-Alternative ohne externe Crate
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:032x}", nanos)
}

/// Minimale RunMeta-Struktur für record_run
pub struct RunMeta {
    pub run_id: String,
    pub status: String,
    pub task: String,
    pub completed_actions: HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_tmp() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_memory_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn test_explicit_memory_roundtrip() {
        let tmp = unique_tmp();
        let store = MemoryStore::new(tmp.join("memory.jsonl"));

        let memory_id = store
            .add(
                "Der Benutzer bevorzugt minimierte Browserfenster.",
                "shared",
                "explicit",
                None,
                0.9,
            )
            .unwrap();

        let result = store
            .search("Browserfenster minimiert", &["shared"], 8)
            .unwrap();
        assert_eq!(
            result.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![memory_id]
        );

        assert!(store.delete(memory_id).unwrap());
        assert_eq!(store.list(20).unwrap().len(), 0);
    }

    #[test]
    fn test_search_filters_unrelated_episodes() {
        let tmp = unique_tmp();
        let store = MemoryStore::new(tmp.join("memory.jsonl"));

        store
            .add(
                "Aufgabe: Arduino aktualisieren",
                "shared",
                "episode",
                Some("run:1"),
                0.5,
            )
            .unwrap();

        let wanted = store
            .add(
                "Aufgabe: DeepSeek Profil testen",
                "shared",
                "episode",
                Some("run:2"),
                0.5,
            )
            .unwrap();

        let result = store.search("DeepSeek Profil", &["shared"], 8).unwrap();
        assert_eq!(
            result.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![wanted]
        );
    }

    #[test]
    fn test_record_run_is_idempotent() {
        let tmp = unique_tmp();
        let store = MemoryStore::new(tmp.join("memory.jsonl"));

        let mut actions = HashMap::new();
        actions.insert(
            "s".to_string(),
            "[Terminal-Ausgabe action_id=s]\nok".to_string(),
        );
        actions.insert("m".to_string(), "Fertig".to_string());

        let meta = RunMeta {
            run_id: "r1".to_string(),
            status: "done".to_string(),
            task: "Datei prüfen".to_string(),
            completed_actions: actions.clone(),
        };

        let first = store.record_run(&meta).unwrap();
        let second = store.record_run(&meta).unwrap();

        assert_eq!(first, second);
        assert_eq!(store.list(20).unwrap().len(), 1);
        assert!(store.list(20).unwrap()[0].content.contains("Fertig"));
    }

    #[test]
    fn test_failed_run_is_not_recorded() {
        let tmp = unique_tmp();
        let store = MemoryStore::new(tmp.join("memory.jsonl"));

        let meta = RunMeta {
            run_id: "r2".to_string(),
            status: "brain_incomplete".to_string(),
            task: "x".to_string(),
            completed_actions: HashMap::new(),
        };

        assert_eq!(store.record_run(&meta).unwrap(), None);
        assert_eq!(store.list(20).unwrap().len(), 0);
    }

    /// Python-parity fixture: gleiche IDs/Importance/Inhalte → gleiche Top-N-Reihenfolge.
    #[test]
    fn test_search_ranking_parity_fixture() {
        let tmp = unique_tmp();
        let store = MemoryStore::new(tmp.join("memory.jsonl"));

        store
            .add(
                "Aufgabe: Arduino Firmware flashen",
                "shared",
                "episode",
                Some("run:ep1"),
                0.35,
            )
            .unwrap();
        store
            .add(
                "Der Benutzer bevorzugt Rust statt Python",
                "shared",
                "explicit",
                Some("manual:rust"),
                0.9,
            )
            .unwrap();
        store
            .add(
                "Aufgabe: DeepSeek Profil testen",
                "shared",
                "episode",
                Some("run:ep2"),
                0.35,
            )
            .unwrap();
        store
            .add(
                "Browserfenster immer minimiert halten",
                "shared",
                "fact",
                Some("manual:browser"),
                0.7,
            )
            .unwrap();

        let deepseek = store.search("DeepSeek Profil", &["shared"], 3).unwrap();
        assert!(!deepseek.is_empty());
        assert!(deepseek[0].content.contains("DeepSeek"));
        assert!(deepseek.iter().all(|e| e.kind != "episode" || e.content.contains("DeepSeek")));

        let rust_mem = store.search("Rust Python", &["shared"], 3).unwrap();
        assert!(!rust_mem.is_empty());
        assert!(rust_mem[0].content.contains("Rust"));

        let browser = store.search("Browserfenster minimiert", &["shared"], 3).unwrap();
        assert!(!browser.is_empty());
        assert!(browser[0].content.contains("minimiert"));

        // Episoden ohne Token-Overlap werden ausgeschlossen (wie Python).
        let unrelated = store.search("Quantencomputer", &["shared"], 5).unwrap();
        assert!(unrelated.iter().all(|e| e.kind != "episode"));
    }

    #[test]
    fn test_parallel_writes() {
        use std::sync::Arc;
        use std::thread;

        let tmp = unique_tmp();
        let path = tmp.join("memory.jsonl");
        let path_arc = Arc::new(path.clone());

        let handles: Vec<_> = (0..40)
            .map(|i| {
                let p = Arc::clone(&path_arc);
                thread::spawn(move || {
                    let store = MemoryStore::new(p.as_ref());
                    store
                        .add(
                            &format!("Parallele Erinnerung {}", i),
                            "shared",
                            "fact",
                            Some(&format!("parallel:{}", i)),
                            0.5,
                        )
                        .unwrap()
                })
            })
            .collect();

        let ids: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        assert_eq!(ids.len(), 40);
        let unique_ids: HashSet<_> = ids.iter().collect();
        assert_eq!(unique_ids.len(), 40);

        let store = MemoryStore::new(&path);
        assert_eq!(store.list(50).unwrap().len(), 40);
    }
}
