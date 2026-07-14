//! First-run OOBE wizard (Python `oobe.run_oobe_wizard` subset).

use crate::config::{available_brain_ids, bot2bot_root, data_dir};
use serde::{Deserialize, Serialize};
use std::io::{self, Write};
use std::path::PathBuf;

const DEFAULT_BRAINS: &[&str] = &["chatgpt", "kimi", "gemini"];

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OobeState {
    pub completed: bool,
    #[serde(default)]
    pub completed_at: String,
    #[serde(default)]
    pub active_brains: Vec<String>,
    #[serde(default)]
    pub login_skipped: bool,
}

pub fn oobe_state_path() -> PathBuf {
    data_dir().join("oobe_state.json")
}

pub fn load_oobe_state() -> OobeState {
    let path = oobe_state_path();
    if !path.is_file() {
        return OobeState::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_oobe_state(active_brains: &[String], login_skipped: bool) -> std::io::Result<()> {
    let payload = OobeState {
        completed: true,
        completed_at: crate::now_rfc3339(),
        active_brains: active_brains.to_vec(),
        login_skipped,
    };
    let path = oobe_state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut f, &payload)?;
    f.write_all(b"\n")?;
    Ok(())
}

pub fn needs_oobe() -> bool {
    !load_oobe_state().completed
}

pub fn registry_path() -> PathBuf {
    bot2bot_root().join("agents").join("registry.json")
}

fn parse_brain_list(spec: &str) -> Vec<String> {
    let all = available_brain_ids();
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return DEFAULT_BRAINS
            .iter()
            .filter(|b| all.contains(&b.to_string()))
            .map(|s| (*s).to_string())
            .collect();
    }
    if trimmed.eq_ignore_ascii_case("all") {
        return all;
    }
    trimmed
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty() && all.contains(&s.to_string()))
        .map(str::to_string)
        .collect()
}

/// OOBE-Wizard: Brain-Auswahl, optional Login, State speichern.
pub fn run_oobe_wizard(
    interactive: bool,
    skip_login: bool,
    brains: &str,
    yes: bool,
) -> Result<(), String> {
    println!();
    println!("=== WebAgent OOBE ===");
    println!("bot2bot:  {}", bot2bot_root().display());
    println!("data:     {}", data_dir().display());
    println!();

    let picked = if !brains.trim().is_empty() {
        parse_brain_list(brains)
    } else if !interactive || yes {
        parse_brain_list("")
    } else {
        let all = available_brain_ids();
        println!("Verfügbare Brains: {}", all.join(", "));
        print!("Welche Brains (Komma-Liste, Enter=Standard)? ");
        io::stdout().flush().ok();
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        parse_brain_list(&line)
    };

    println!("[oobe] Aktiv: {}", picked.join(", "));

    let do_login = !skip_login && !picked.is_empty() && interactive && !yes;
    if do_login {
        println!("[oobe] Login-Schritt: webagent login --brain <id> für jedes Brain.");
        for brain in &picked {
            println!("[oobe]   webagent login --brain {brain}");
        }
    } else {
        println!("[oobe] Login übersprungen.");
    }

    save_oobe_state(&picked, skip_login || !do_login).map_err(|e| e.to_string())?;
    println!("=== OOBE abgeschlossen ===");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_brain_list_all() {
        let ids = parse_brain_list("all");
        assert!(!ids.is_empty());
        assert!(ids.contains(&"chatgpt".to_string()));
    }

    #[test]
    fn test_parse_brain_list_subset() {
        let ids = parse_brain_list("chatgpt,gemini");
        assert_eq!(ids, vec!["chatgpt".to_string(), "gemini".to_string()]);
    }

    #[test]
    fn test_registry_path_under_bot2bot() {
        let p = registry_path();
        assert!(p.ends_with("registry.json"));
    }
}
