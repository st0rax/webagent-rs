//! Konfiguration: Pfade, Brain-Definitionen, Umgebungsvariablen.
//!
//! Portiert aus ../src/webagent/config.py

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

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

/// bot2bot/ — Consensus/Genius-Workspace-Root
pub fn bot2bot_root() -> PathBuf {
    root_dir().join("bot2bot")
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
pub fn use_shared_browser() -> bool {
    env::var("WEBAGENT_SHARED_BROWSER")
        .unwrap_or_default()
        .to_lowercase()
        == "1"
}

/// Brain-Definitionen: ID -> {url, selectors, profile_dir}
///
/// Portiert aus BRAINS-Dict in config.py. Selektoren-Pfade sind relativ zu src/.
pub fn brains() -> HashMap<String, HashMap<String, String>> {
    let src = src_dir();
    let profiles = profiles_dir();

    let mut brains = HashMap::new();

    // ChatGPT
    let mut chatgpt = HashMap::new();
    chatgpt.insert("url".to_string(), "https://chatgpt.com/".to_string());
    chatgpt.insert(
        "selectors".to_string(),
        src.join("webagent/brains/chatgpt_selectors.json")
            .to_string_lossy()
            .to_string(),
    );
    chatgpt.insert(
        "profile_dir".to_string(),
        profiles.join("chatgpt").to_string_lossy().to_string(),
    );
    brains.insert("chatgpt".to_string(), chatgpt);

    // Claude
    let mut claude = HashMap::new();
    claude.insert("url".to_string(), "https://claude.ai/new".to_string());
    claude.insert(
        "selectors".to_string(),
        src.join("webagent/brains/claude_selectors.json")
            .to_string_lossy()
            .to_string(),
    );
    claude.insert(
        "profile_dir".to_string(),
        profiles.join("claude").to_string_lossy().to_string(),
    );
    brains.insert("claude".to_string(), claude);

    // DeepSeek
    let mut deepseek = HashMap::new();
    deepseek.insert("url".to_string(), "https://chat.deepseek.com/".to_string());
    deepseek.insert(
        "selectors".to_string(),
        src.join("webagent/brains/deepseek_selectors.json")
            .to_string_lossy()
            .to_string(),
    );
    deepseek.insert(
        "profile_dir".to_string(),
        profiles.join("deepseek").to_string_lossy().to_string(),
    );
    brains.insert("deepseek".to_string(), deepseek);

    // Gemini
    let mut gemini = HashMap::new();
    gemini.insert(
        "url".to_string(),
        "https://gemini.google.com/app".to_string(),
    );
    gemini.insert(
        "selectors".to_string(),
        src.join("webagent/brains/gemini_selectors.json")
            .to_string_lossy()
            .to_string(),
    );
    gemini.insert(
        "profile_dir".to_string(),
        profiles.join("gemini").to_string_lossy().to_string(),
    );
    brains.insert("gemini".to_string(), gemini);

    // Kimi
    let mut kimi = HashMap::new();
    kimi.insert("url".to_string(), "https://kimi.moonshot.cn/".to_string());
    kimi.insert(
        "selectors".to_string(),
        src.join("webagent/brains/kimi_selectors.json")
            .to_string_lossy()
            .to_string(),
    );
    kimi.insert(
        "profile_dir".to_string(),
        profiles.join("kimi").to_string_lossy().to_string(),
    );
    brains.insert("kimi".to_string(), kimi);

    brains
}

/// Gibt die Liste aller verfügbaren Brain-IDs zurück (sortiert).
pub fn available_brain_ids() -> Vec<String> {
    let mut ids: Vec<String> = brains().keys().cloned().collect();
    ids.sort();
    ids
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
        assert!(!brains.is_empty(), "Mindestens ein Brain sollte konfiguriert sein");
        
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
    fn test_ensure_data_dirs() {
        // Sollte nicht fehlschlagen (erstellt Verzeichnisse oder sie existieren bereits)
        assert!(ensure_data_dirs().is_ok());
        assert!(data_dir().exists());
        assert!(runs_dir().exists());
    }
}
