//! Konfiguration: Pfade, Brain-Definitionen, Umgebungsvariablen.
//!
//! Portiert aus ../src/webagent/config.py

use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

/// Root-Verzeichnis der WebAgent-Installation (Elternverzeichnis von src/).
pub fn root_dir() -> PathBuf {
    // Zur Compile-Zeit: CARGO_MANIFEST_DIR zeigt auf das Projekt-Root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// src/-Verzeichnis
pub fn src_dir() -> PathBuf {
    root_dir().join("src")
}

/// data/-Verzeichnis für Runs, Memory, etc.
pub fn data_dir() -> PathBuf {
    root_dir().join("data")
}

/// data/runs/ — Run-Metadaten und Transcripts
pub fn runs_dir() -> PathBuf {
    data_dir().join("runs")
}

/// data/memory/ — MemoryStore-Datenbank
pub fn memory_dir() -> PathBuf {
    data_dir().join("memory")
}

/// profiles/ — Browser-Profile (shared + brain-spezifisch)
pub fn profiles_dir() -> PathBuf {
    root_dir().join("profiles")
}

/// profiles/shared/ — Gemeinsames Browser-Profil (wenn shared_browser aktiviert)
pub fn shared_profile_dir() -> PathBuf {
    profiles_dir().join("shared")
}

/// Maximale Observation-Länge (Zeichen) — Python `MAX_OBSERVATION_CHARS`.
pub const MAX_OBSERVATION_CHARS: usize = 12_000;
/// Loop-Guard Warn-/Abort-Schwellen — Python `LOOP_GUARD_*`.
pub const LOOP_GUARD_WARN_COUNT: usize = 3;
pub const LOOP_GUARD_ABORT_COUNT: usize = 8;

/// bot2bot/ — Agent-Messaging-Root (Desktop-Sibling oder Override).
pub fn bot2bot_root() -> PathBuf {
    if let Ok(override_path) = env::var("WEBAGENT_BOT2BOT_ROOT") {
        let s = override_path.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    let link = data_dir().join("install_bot2bot_root.txt");
    if let Ok(content) = std::fs::read_to_string(&link) {
        let s = content.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    root_dir()
        .parent()
        .map(|p| p.join("bot2bot"))
        .unwrap_or_else(|| root_dir().join("bot2bot"))
}

/// consensus_workspace() — Eindeutiger Workspace-Pfad für Genius-Council
pub fn consensus_workspace() -> PathBuf {
    let stamp = crate::now_run_stamp();
    bot2bot_root().join(format!("consensus_{}", stamp))
}

/// Erstellt alle notwendigen Datenverzeichnisse, falls sie nicht existieren.
pub fn ensure_data_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(runs_dir())?;
    std::fs::create_dir_all(memory_dir())?;
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(bot2bot_root())?;
    Ok(())
}

/// Gibt `true` zurück, wenn shared_browser aktiviert ist (Umgebungsvariable).
/// Python-Name: `WEBAGENT_USE_SHARED_BROWSER`; Legacy-Alias: `WEBAGENT_SHARED_BROWSER`.
pub fn use_shared_browser() -> bool {
    let v = env::var("WEBAGENT_USE_SHARED_BROWSER")
        .or_else(|_| env::var("WEBAGENT_SHARED_BROWSER"))
        .unwrap_or_default();
    matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Tabs zwischen Relay-Hops offen halten. Default: an wenn shared browser an.
pub fn persist_browser_tabs() -> bool {
    let v = env::var("WEBAGENT_PERSIST_TABS").unwrap_or_default();
    match v.trim().to_lowercase().as_str() {
        "0" | "false" | "no" | "off" => false,
        "1" | "true" | "yes" | "on" => true,
        _ => use_shared_browser(),
    }
}

/// Fester Debug-Port für den Shared-Browser-Pool (ein Chromium, viele Tabs).
pub fn shared_debug_port() -> u16 {
    env::var("WEBAGENT_SHARED_DEBUG_PORT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(9222)
}

/// selectors/-Verzeichnis (ROOT/selectors/<brain>.json), wie SELECTORS_DIR in config.py.
pub fn selectors_dir() -> PathBuf {
    root_dir().join("selectors")
}

/// Statische Brain-Tabelle: (id, url) — exakt wie das BRAINS-Dict in config.py.
pub const BRAIN_TABLE: &[(&str, &str)] = &[
    ("chatgpt", "https://chatgpt.com/"),
    ("deepseek", "https://chat.deepseek.com/"),
    ("kimi", "https://www.kimi.com/"),
    ("gemini", "https://gemini.google.com/app"),
    ("qwen", "https://chat.qwen.ai/"),
    ("claude", "https://claude.ai/new"),
    ("mistral", "https://chat.mistral.ai/chat"),
    ("zai", "https://chat.z.ai/"),
];

/// Brain-Definitionen: ID -> {url, selectors, profile_dir}.
///
/// Portiert aus BRAINS-Dict in config.py. Selektoren liegen unter
/// ROOT/selectors/<brain>.json; jedes Brain erhaelt ein eigenes Profil unter
/// profiles/<brain> (Referenzprofil-Ansatz), das doctor prueft.
pub fn brains() -> HashMap<String, HashMap<String, String>> {
    let sel = selectors_dir();
    let profiles = profiles_dir();
    // Optionaler Override: alle Brains dasselbe Profil nutzen lassen (z.B. das
    // eingeloggte Shared-Profil des Python-webagent) via WEBAGENT_PROFILE_DIR.
    let profile_override = env::var("WEBAGENT_PROFILE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut brains = HashMap::new();
    for (id, url) in BRAIN_TABLE {
        let mut b = HashMap::new();
        b.insert("url".to_string(), url.to_string());
        b.insert(
            "selectors".to_string(),
            sel.join(format!("{id}.json")).to_string_lossy().to_string(),
        );
        let profile_dir = profile_override
            .clone()
            .unwrap_or_else(|| profiles.join(id).to_string_lossy().to_string());
        b.insert("profile_dir".to_string(), profile_dir);
        brains.insert(id.to_string(), b);
    }
    brains
}

/// Laedt die Selektor-JSON eines Brains (wie config.load_selectors in Python).
pub fn load_selectors(brain_id: &str) -> std::io::Result<serde_json::Value> {
    let path = selectors_dir().join(format!("{brain_id}.json"));
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Gibt die Liste aller verfügbaren Brain-IDs zurück (sortiert).
pub fn available_brain_ids() -> Vec<String> {
    let mut ids: Vec<String> = brains().keys().cloned().collect();
    ids.sort();
    ids
}

/// Deterministischer Chrome-Remote-Debugging-Port je Brain (kollisionsarm).
/// Basisport via `WEBAGENT_DEBUG_PORT_BASE` überschreibbar (Standard 9222).
pub fn debug_port(brain_id: &str) -> u16 {
    let base: u16 = env::var("WEBAGENT_DEBUG_PORT_BASE")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(9222);
    base.wrapping_add((fnv1a(brain_id) % 400) as u16)
}

/// FNV-1a-Hash (gemeinfrei) für die stabile Port-Zuteilung.
fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_dir_exists() {
        let root = root_dir();
        assert!(root.exists(), "Root-Verzeichnis sollte existieren");
        assert!(root.is_dir(), "Root sollte ein Verzeichnis sein");
    }

    #[test]
    fn test_brains_config() {
        let brains = brains();
        assert!(
            !brains.is_empty(),
            "Mindestens ein Brain sollte konfiguriert sein"
        );

        // ChatGPT sollte vorhanden sein
        assert!(brains.contains_key("chatgpt"));
        let chatgpt = &brains["chatgpt"];
        assert!(chatgpt.contains_key("url"));
        assert!(chatgpt.contains_key("selectors"));
        assert!(chatgpt.contains_key("profile_dir"));
    }

    #[test]
    fn test_available_brain_ids() {
        let ids = available_brain_ids();
        assert!(!ids.is_empty());
        assert!(ids.contains(&"chatgpt".to_string()));

        // Sollte sortiert sein
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_debug_port_deterministic_and_in_range() {
        let p1 = debug_port("chatgpt");
        assert_eq!(p1, debug_port("chatgpt"), "deterministisch");
        assert!((9222..9622).contains(&p1), "in Range: {p1}");
        // Die 8 konfigurierten Brains sollten großteils verschiedene Ports haben.
        let ports: std::collections::HashSet<u16> =
            BRAIN_TABLE.iter().map(|(id, _)| debug_port(id)).collect();
        assert!(ports.len() >= 6, "zu viele Port-Kollisionen: {ports:?}");
    }

    #[test]
    fn test_parity_constants() {
        assert_eq!(MAX_OBSERVATION_CHARS, 12_000);
        assert_eq!(LOOP_GUARD_WARN_COUNT, 3);
        assert_eq!(LOOP_GUARD_ABORT_COUNT, 8);
    }

    #[test]
    fn test_persist_browser_tabs_defaults() {
        let shared_key = "WEBAGENT_USE_SHARED_BROWSER";
        let tabs_key = "WEBAGENT_PERSIST_TABS";
        let prev_shared = env::var(shared_key).ok();
        let prev_tabs = env::var(tabs_key).ok();
        env::set_var(shared_key, "1");
        env::remove_var(tabs_key);
        assert!(persist_browser_tabs());
        env::set_var(tabs_key, "0");
        assert!(!persist_browser_tabs());
        match prev_tabs {
            Some(v) => env::set_var(tabs_key, v),
            None => env::remove_var(tabs_key),
        }
        match prev_shared {
            Some(v) => env::set_var(shared_key, v),
            None => env::remove_var(shared_key),
        }
    }

    #[test]
    fn test_use_shared_browser_env_names() {
        let key = "WEBAGENT_USE_SHARED_BROWSER";
        let prev = env::var(key).ok();
        env::set_var(key, "1");
        assert!(use_shared_browser());
        env::set_var(key, "0");
        assert!(!use_shared_browser());
        match prev {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }
    }

    #[test]
    fn test_ensure_data_dirs() {
        // Sollte nicht fehlschlagen (erstellt Verzeichnisse oder sie existieren bereits)
        assert!(ensure_data_dirs().is_ok());
        assert!(data_dir().exists());
        assert!(runs_dir().exists());
    }
}
