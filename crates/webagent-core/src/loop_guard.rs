//! Fortschritts- und Loop-Erkennung für Shell-Leseaktionen.

use std::hash::{Hash, Hasher};

/// Fingerprint nur für eine tatsächlich identische Leseaktion mit identischem
/// Ergebnis. Verschiedene Dateien, Bereiche oder Suchbegriffe sind legitimer
/// Fortschritt und dürfen niemals in derselben groben Klasse landen.
pub fn shell_read_fingerprint(command: &str, observation: &str) -> Option<String> {
    if !is_shell_read_action(command) {
        return None;
    }
    let normalized = command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('\\', "/")
        .to_ascii_lowercase();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    observation.hash(&mut hasher);
    Some(format!("read:{:016x}", hasher.finish()))
}

pub fn is_shell_read_action(command: &str) -> bool {
    let c = command.trim().to_ascii_lowercase();
    [
        "get-content",
        "select-string",
        "get-childitem",
        "rg ",
        "rg.exe ",
        "type ",
        "grep ",
        "sed -n",
        "head ",
        "tail ",
    ]
    .iter()
    .any(|needle| c.starts_with(needle) || c.contains(&format!("| {needle}")))
}

pub fn read_budget_message(count: u32) -> String {
    format!(
        "[Controller] LESE-CHECKPOINT ({count} reine Leseaktionen seit dem letzten erfolgreichen \
         Edit). Pruefe, ob der vorhandene Kontext bereits fuer eine Umsetzung reicht. Weitere \
         gezielte Reads bleiben erlaubt; vermeide Wiederholungen und wechsle zu EDIT/WRITE, \
         sobald ein belastbarer Anker vorliegt."
    )
}

pub fn loop_guard_message(fingerprint: &str, count: usize) -> String {
    format!(
        "[Controller] Exakt dieselbe Leseaktion lieferte dasselbe Ergebnis ({fingerprint}, {count}x). \
         Ändere Suchziel oder Strategie statt die Aktion zu wiederholen."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identischer_read_und_output_hat_identischen_fingerprint() {
        let a = shell_read_fingerprint("Get-Content src/a.rs", "inhalt").unwrap();
        let b = shell_read_fingerprint("  get-content  src/a.rs ", "inhalt").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn andere_datei_oder_observation_ist_fortschritt() {
        let base = shell_read_fingerprint("Get-Content src/a.rs", "inhalt").unwrap();
        assert_ne!(
            base,
            shell_read_fingerprint("Get-Content src/b.rs", "inhalt").unwrap()
        );
        assert_ne!(
            base,
            shell_read_fingerprint("Get-Content src/a.rs", "neuer inhalt").unwrap()
        );
    }

    #[test]
    fn nicht_lesende_befehle_bekommen_keinen_loop_fingerprint() {
        assert!(shell_read_fingerprint("cargo test", "ok").is_none());
    }

    #[test]
    fn variable_dateilesen_werden_als_leseaktionen_erkannt() {
        assert!(is_shell_read_action(
            "Get-Content src/a.rs | Select-Object -Skip 20"
        ));
        assert!(is_shell_read_action("rg foo src"));
        assert!(!is_shell_read_action("cargo test"));
    }
}
