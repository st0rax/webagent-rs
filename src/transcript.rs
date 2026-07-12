//! JSON-Lines-Transcript für Run-Protokollierung.

use crate::run_store::RunMeta;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

pub struct Transcript {
    path: PathBuf,
}

impl Transcript {
    pub fn new(meta: &RunMeta, runs_dir: &PathBuf) -> Self {
        let dir = meta.dir(runs_dir);
        let path = dir.join("transcript.jsonl");
        Self { path }
    }

    /// Fügt einen Eintrag zum Transcript hinzu.
    pub fn append(&self, role: &str, content: &str, extra: HashMap<String, Value>) -> Result<(), String> {
        let mut entry = json!({
            "ts": crate::now_rfc3339(),
            "role": role,
            "content": content,
        });

        if let Some(obj) = entry.as_object_mut() {
            for (k, v) in extra {
                obj.insert(k, v);
            }
        }

        let line = serde_json::to_string(&entry)
            .map_err(|e| format!("JSON-Serialisierung fehlgeschlagen: {}", e))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| format!("Fehler beim Öffnen von {}: {}", self.path.display(), e))?;

        writeln!(file, "{}", line)
            .map_err(|e| format!("Fehler beim Schreiben: {}", e))?;

        Ok(())
    }

    /// Liest alle Einträge aus dem Transcript.
    pub fn read_all(&self) -> Result<Vec<Value>, String> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path)
            .map_err(|e| format!("Fehler beim Öffnen von {}: {}", self.path.display(), e))?;

        let reader = BufReader::new(file);
        let mut entries = Vec::new();

        for line in reader.lines() {
            let line = line.map_err(|e| format!("Fehler beim Lesen: {}", e))?;
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let entry: Value = serde_json::from_str(trimmed)
                    .map_err(|e| format!("JSON-Parse-Fehler: {}", e))?;
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    /// Formatiert einen Eintrag als Zeile.
    fn format_entry_line(&self, entry: &Value, compact: bool) -> String {
        let role = entry.get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        
        let mut content = entry.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let action_id = entry.get("action_id").and_then(|v| v.as_str());

        if compact {
            // Brain-Einträge kürzen
            if role == "brain" && content.len() > 400 {
                let prefix = crate::char_prefix(&content, 200);
                let suffix = crate::char_suffix(&content, 120);
                content = format!("{}...(brain truncated)...{}", prefix, suffix);
            }

            // Terminal-Ausgaben kürzen
            if content.contains("[Terminal-Ausgabe") && content.len() > 600 {
                let prefix = crate::char_prefix(&content, 250);
                let suffix = crate::char_suffix(&content, 150);
                content = format!("{}...(observation truncated)...{}", prefix, suffix);
            }
        }

        let mut prefix = format!("[{}]", role);
        if let Some(aid) = action_id {
            prefix.push_str(&format!(" action_id={}", aid));
        }

        format!("{} {}", prefix, content)
    }

    /// Deterministisches Transcript-Ende für Resume-Fallback.
    pub fn recovery_tail(&self, char_budget: usize) -> Result<String, String> {
        let entries = self.read_all()?;
        let lines: Vec<String> = entries.iter()
            .map(|e| self.format_entry_line(e, true))
            .collect();
        
        let text = lines.join("\n");

        if text.len() <= char_budget {
            return Ok(text);
        }

        let marker = "...(truncated from start)...\n";
        let keep = char_budget.saturating_sub(marker.len());
        let tail = crate::char_suffix(&text, keep);
        
        Ok(format!("{}{}", marker, tail))
    }

    /// Kurzsummary der letzten Transcript-Einträge für Kontext-Injektion.
    pub fn compact_summary(&self, max_entries: usize, char_budget: usize) -> Result<String, String> {
        let entries = self.read_all()?;
        
        if entries.is_empty() {
            return Ok("(leer)".to_string());
        }

        let start = if entries.len() > max_entries {
            entries.len() - max_entries
        } else {
            0
        };

        let tail = &entries[start..];
        let lines: Vec<String> = tail.iter()
            .map(|e| self.format_entry_line(e, true))
            .collect();
        
        let text = lines.join("\n");

        if text.len() <= char_budget {
            return Ok(text);
        }

        let marker = "...(summary truncated)...\n";
        let keep = char_budget.saturating_sub(marker.len());
        let tail_text = crate::char_suffix(&text, keep);
        
        Ok(format!("{}{}", marker, tail_text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_store::RunStore;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_tmp() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_transcript_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn test_recovery_tail_truncates_large_brain_entries() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir);
        let meta = store.create("mock", "task").unwrap();
        
        let transcript = Transcript::new(&meta, &runs_dir);
        
        let huge = "x".repeat(5000);
        transcript.append("brain", &huge, HashMap::new()).unwrap();
        
        let tail = transcript.recovery_tail(2000).unwrap();
        
        assert!(tail.contains("...(brain truncated)..."));
        assert!(tail.len() <= 2100);
        
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_compact_summary_limits_entries() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");
        
        let store = RunStore::new(runs_dir.clone(), logs_dir);
        let meta = store.create("mock", "task").unwrap();
        
        let transcript = Transcript::new(&meta, &runs_dir);
        
        for i in 0..20 {
            transcript.append("system", &format!("event-{}", i), HashMap::new()).unwrap();
        }
        
        let summary = transcript.compact_summary(5, 500).unwrap();
        
        assert!(summary.contains("event-19"));
        assert!(!summary.contains("event-0"));
        
        std::fs::remove_dir_all(&tmp).ok();
    }
}
