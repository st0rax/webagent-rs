//! JSON-Lines-Transcript für Run-Protokollierung.

use crate::run_store::RunMeta;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

pub struct Transcript {
    path: PathBuf,
}

impl Transcript {
    pub fn new(meta: &RunMeta, runs_dir: &Path) -> Self {
        let dir = meta.dir(runs_dir);
        let path = dir.join("transcript.jsonl");
        Self { path }
    }

    /// Fügt einen Eintrag zum Transcript hinzu.
    pub fn append(
        &self,
        role: &str,
        content: &str,
        extra: HashMap<String, Value>,
    ) -> Result<(), String> {
        // Vor der Konsumierung von `extra` sichern: die Schleife unten zieht
        // alle Werte raus.
        let action_id = extra
            .get("action_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);
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

        writeln!(file, "{}", line).map_err(|e| format!("Fehler beim Schreiben: {}", e))?;

        // Storax-Vorgabe (2026-08-01): die vollstaendige Brain-Konversation
        // (transcript.jsonl) gehoert in den TUI-Baum — das ist die maximale
        // Tiefe. `message`-Eintraege laufen bereits als `[msg:…]`-Knoten des
        // Controllers ein; ohne sie zu spiegeln bleibt die Anzeige ohne
        // Dopplung.
        if crate::bench_events::echo_bus_enabled() && role != "message" {
            let level = if role == "system" {
                crate::bench_events::Level::Info
            } else {
                crate::bench_events::Level::Progress
            };
            let head = match &action_id {
                Some(aid) => format!("[t:{role} action_id={aid}]"),
                None => format!("[t:{role}]"),
            };
            crate::bench_events::emit_detailed(
                level,
                None,
                &format!("{head} {}", crate::char_prefix(content, 60)),
                Some(content),
            );
        }

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
        let role = entry.get("role").and_then(|v| v.as_str()).unwrap_or("?");

        let mut content = entry
            .get("content")
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
        let lines: Vec<String> = entries
            .iter()
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
    pub fn compact_summary(
        &self,
        max_entries: usize,
        char_budget: usize,
    ) -> Result<String, String> {
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
        let lines: Vec<String> = tail
            .iter()
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

/// Erzeugt eine einzelne valide JSON-Zeile fuer ein strukturiertes Ereignis.
///
/// Deterministisch: die Felder werden alphabetisch nach Schluessel sortiert
/// (BTreeMap), und Sonderzeichen werden von serde_json korrekt escaped.
/// `event` ist immer dabei; weitere Felder kommen aus `fields`.
pub fn emit_structured_log(event: &str, fields: &[(&str, &str)]) -> String {
    let mut map = BTreeMap::new();
    map.insert("event".to_string(), event.to_string());
    for (k, v) in fields {
        map.insert(k.to_string(), v.to_string());
    }
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".to_string())
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
            transcript
                .append("system", &format!("event-{}", i), HashMap::new())
                .unwrap();
        }

        let summary = transcript.compact_summary(5, 500).unwrap();

        assert!(summary.contains("event-19"));
        assert!(!summary.contains("event-0"));

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn transcript_append_spiegelt_konversation_nur_im_spiegelmodus() {
        // Positiv: im Spiegelmodus legt append die vollstaendige Konversation
        // in den Bus (maximale Tiefe), inkl. action_id im Kopf.
        // Negativ: `message`-Eintraege werden NICHT gespiegelt (Dopplung mit
        // den `[msg:…]`-Knoten des Controllers) und ohne Spiegelmodus legt
        // append gar nichts in den Bus.
        let _guard = crate::bench_events::test_bus_mutex().lock();

        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let store = RunStore::new(runs_dir.clone(), tmp.join("logs"));
        let meta = store.create("mock", "task").unwrap();
        let transcript = Transcript::new(&meta, &runs_dir);

        crate::bench_events::set_echo_bus(true);
        crate::bench_events::clear();
        let mut extra = HashMap::new();
        extra.insert("action_id".to_string(), Value::String("step-7".to_string()));
        transcript
            .append("brain", "Antwort mit vollem Prompt", extra)
            .unwrap();
        transcript
            .append("message", "Antwort als Message-Action", HashMap::new())
            .unwrap();
        let events = crate::bench_events::snapshot();
        assert!(
            events
                .iter()
                .any(|e| e.text.starts_with("[t:brain action_id=step-7]")),
            "brain-Turn muss im Spiegelmodus in den Bus"
        );
        assert!(
            !events.iter().any(|e| e.text.starts_with("[t:message")),
            "message-Eintraege kommen als [msg:…]-Knoten — keine Dopplung"
        );

        crate::bench_events::set_echo_bus(false);
        crate::bench_events::clear();
        transcript.append("brain", "nochmal", HashMap::new()).unwrap();
        let events = crate::bench_events::snapshot();
        assert!(
            !events
                .iter()
                .any(|e| e.text.starts_with("[t:brain")),
            "ohne Spiegelmodus darf die Konversation nicht in den Bus"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn emit_structured_log_einfach() {
        let result = emit_structured_log("action_started", &[("id", "123")]);
        assert!(result.contains("\"event\":\"action_started\""));
        assert!(result.contains("\"id\":\"123\""));
    }

    #[test]
    fn emit_structured_log_felder_stabil_sortiert() {
        let result = emit_structured_log("test_event", &[("z_key", "1"), ("a_key", "2")]);
        let pos_a = result.find("\"a_key\"").unwrap();
        let pos_e = result.find("\"event\"").unwrap();
        let pos_z = result.find("\"z_key\"").unwrap();
        assert!(pos_a < pos_e && pos_e < pos_z);
        assert!(result.contains("\"a_key\":\"2\""));
        assert!(result.contains("\"z_key\":\"1\""));
    }

    #[test]
    fn emit_structured_log_escaped_sonderzeichen() {
        let result = emit_structured_log("escape_test", &[("val", "\"quote\"\\backslash\nnewline")]);
        assert!(result.contains("\\\"quote\\\""));
        assert!(result.contains("\\\\backslash"));
        assert!(result.contains("\\nnewline"));
    }

    #[test]
    fn emit_structured_log_leere_felder() {
        assert_eq!(
            emit_structured_log("empty_event", &[]),
            "{\"event\":\"empty_event\"}"
        );
    }

    #[test]
    fn emit_structured_log_unicode() {
        let result = emit_structured_log("🚀_start", &[("user", "äöüß"), ("emoji", "🦀")]);
        assert!(result.contains("🚀_start"));
        assert!(result.contains("äöüß"));
        assert!(result.contains("🦀"));
    }
}
