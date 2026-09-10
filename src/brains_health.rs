//! Pre-flight health check ohne Browser-Start (Python `cmd_brains_health`).

use crate::config::{available_brain_ids, bot2bot_root, brains, shared_profile_dir};
use std::path::Path;

/// Führt den Brains-Health-Check aus. Exit 0 = ok, 2 = Profil fehlt (ohne allow_empty).
pub fn run_brains_health(allow_empty_profile: bool) -> i32 {
    let shared = crate::config::use_shared_browser();
    let profile = shared_profile_dir();
    let entries: Vec<_> = if profile.is_dir() {
        std::fs::read_dir(&profile)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|n| n != ".gitkeep"))
            .collect()
    } else {
        vec![]
    };
    let profile_ok = !entries.is_empty();

    println!("[brains-health] shared_browser={shared}");
    let label = if profile_ok {
        "ok"
    } else if allow_empty_profile {
        "empty (login required)"
    } else {
        "MISSING"
    };
    println!(
        "[brains-health] shared_profile={label} ({})",
        profile.display()
    );

    let archive = profile.parent().unwrap_or(Path::new(".")).join("_archive");
    if archive.is_dir() {
        let count = std::fs::read_dir(&archive).map(|d| d.count()).unwrap_or(0);
        println!("[brains-health] archived_profiles={count}");
    }

    let b2b = bot2bot_root();
    println!(
        "[brains-health] bot2bot={}",
        if b2b.is_dir() { "ok" } else { "MISSING" }
    );

    let brain_map = brains();
    for id in available_brain_ids() {
        if let Some(spec) = brain_map.get(&id) {
            let sel = spec.get("selectors").map(String::as_str).unwrap_or("");
            // Selektoren sind ok, wenn sie on-disk liegen ODER in die Binary
            // eingebettet sind (self-contained exe ohne selectors/-Ordner).
            let sel_ok =
                Path::new(sel).is_file() || crate::config::embedded_selector(&id).is_some();
            let url = spec.get("url").map(String::as_str).unwrap_or("");
            // Level = fahrbare Webchat-Optionen. Zeigt die Luecke zwischen
            // "Text rein/raus" und dem, was die Oberflaeche per Maus hergibt.
            let lvl = crate::capability::level_of(&id);
            // Scheibe 5: unbekannte Mechanik ehrlich als unverified ausweisen.
            // Ein frischer, hash-konformer Beleg macht ein Brain verified; alles
            // andere (nie gemessen, TTL abgelaufen, Selektoren geaendert) bleibt
            // unverified — auch neue Brain-URLs aus der generischen Discovery.
            let verify = crate::config::load_selectors(&id)
                .ok()
                .and_then(|sel| {
                    crate::capability::capability("chat").and_then(|chat| {
                        Some(crate::capability_proof::brain_verification(
                            &id,
                            crate::capability_proof::selector_hash_for(&chat, &sel),
                        ))
                    })
                })
                .unwrap_or(crate::capability_proof::BrainVerification::Unverified);
            println!(
                "  {}: selectors={} url={url} verification={:?}",
                lvl.label(),
                if sel_ok { "ok" } else { "MISSING" },
                verify
            );
        }
    }

    if !profile_ok && !allow_empty_profile {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brains_health_lists_all_brains() {
        let code = run_brains_health(true);
        assert!(code == 0 || code == 2);
    }
}
