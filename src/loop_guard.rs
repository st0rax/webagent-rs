//! Detect repeated shell read patterns that cause analysis death loops.

use regex::Regex;
use std::sync::OnceLock;

/// Pattern definitions for detecting repeated file reads.
struct ReadPattern {
    regex: Regex,
    label: &'static str,
}

fn read_patterns() -> &'static [ReadPattern] {
    static PATTERNS: OnceLock<Vec<ReadPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            ReadPattern {
                regex: Regex::new(r"(?i)get-content[^\n;|]*cli\.py").unwrap(),
                label: "read:cli.py",
            },
            ReadPattern {
                regex: Regex::new(r"(?i)select-object\s+-index").unwrap(),
                label: "read:slice",
            },
            ReadPattern {
                regex: Regex::new(r"(?i)get-content[^\n;|]*transcript\.jsonl").unwrap(),
                label: "read:transcript",
            },
            ReadPattern {
                regex: Regex::new(r"(?i)get-content[^\n;|]*controller").unwrap(),
                label: "read:controller",
            },
            ReadPattern {
                regex: Regex::new(r"(?i)select-string[^\n;|]*\.py").unwrap(),
                label: "read:select-string",
            },
        ]
    })
}

/// Return a stable label when command looks like a repeated file read.
pub fn shell_read_fingerprint(command: &str) -> Option<&'static str> {
    if command.is_empty() {
        return None;
    }
    let normalized = command.replace('\\', "/");
    for pattern in read_patterns() {
        if pattern.regex.is_match(&normalized) {
            return Some(pattern.label);
        }
    }
    None
}

/// Erkennt klar lesende Shell-Aktionen auch dann, wenn sich Datei, Suchwort
/// oder Offset jedes Mal ändern. Der Fingerprint-Guard allein sieht solche
/// variierenden Explorationsschleifen nicht.
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


/// Generate warning message for repeated read patterns.
pub fn loop_guard_message(fingerprint: &str, count: usize) -> String {
    format!(
        "[Controller] Wiederholtes Lesemuster ({}, {}x). \
         Nutze Select-String, Get-Content -TotalCount N oder gespeicherte \
         action_output-Artefakte. Schliesse mit message ab statt weiter zu lesen.",
        fingerprint, count
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_detects_cli_py_reads() {
        assert_eq!(
            shell_read_fingerprint("Get-Content src/webagent/cli.py"),
            Some("read:cli.py")
        );
    }

    #[test]
    fn test_fingerprint_detects_slice_reads() {
        assert_eq!(
            shell_read_fingerprint("Get-Content x | Select-Object -Index 580..620"),
            Some("read:slice")
        );
    }

    #[test]
    fn test_fingerprint_ignores_unrelated_commands() {
        assert_eq!(shell_read_fingerprint("Get-ChildItem C:\\Temp"), None);
    }

    #[test]
    fn test_loop_guard_message_includes_count() {
        let msg = loop_guard_message("read:cli.py", 5);
        assert!(msg.contains("read:cli.py"));
        assert!(msg.contains("5x"));
    }

    #[test]
    fn variable_dateilesen_werden_als_leseaktionen_erkannt() {
        assert!(is_shell_read_action(
            "Get-Content src/a.rs | Select-Object -Skip 20"
        ));
        assert!(is_shell_read_action(
            "Select-String -Path src/*.rs -Pattern foo"
        ));
        assert!(is_shell_read_action("rg -n foo src"));
        assert!(!is_shell_read_action("cargo test --lib"));
        assert!(!is_shell_read_action("Set-Content src/a.rs x"));
    }


}
