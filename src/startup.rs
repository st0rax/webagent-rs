//! Einstieg ohne Subcommand — testbar ohne TUI/WebView.

/// Was `webagent` ohne Subcommand startet, und was `repl` / `tui` bleiben.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliEntry {
    /// Scrollback + Prompt (Default ohne Subcommand).
    SessionTui,
    /// Zeilenweise REPL (`webagent repl`).
    Repl,
    /// Pool / Wand / Bench (`webagent tui`).
    PoolTui,
    /// Jeder andere Subcommand.
    Other,
}

/// Mappt das erste CLI-Token (oder dessen Fehlen) auf den Einstieg.
///
/// `None` ist der Default nach `Cli::parse()` ohne Subcommand — das muss
/// die Session-TUI sein, nicht die REPL.
pub fn resolve_cli_entry(subcommand: Option<&str>) -> CliEntry {
    match subcommand {
        None => CliEntry::SessionTui,
        Some("repl") => CliEntry::Repl,
        Some("tui") => CliEntry::PoolTui,
        Some(_) => CliEntry::Other,
    }
}

/// `--view` der Session-TUI, die der Default-Einstieg oeffnet.
pub fn default_session_view() -> &'static str {
    "session"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ohne_subcommand_ist_session_tui() {
        assert_eq!(resolve_cli_entry(None), CliEntry::SessionTui);
        assert_eq!(default_session_view(), "session");
    }

    #[test]
    fn explizites_repl_bleibt_repl() {
        assert_eq!(resolve_cli_entry(Some("repl")), CliEntry::Repl);
    }

    #[test]
    fn explizites_tui_bleibt_pool_wand() {
        assert_eq!(resolve_cli_entry(Some("tui")), CliEntry::PoolTui);
    }

    /// Session-Einstieg, Transcript-Mapper und Slash-Parser bleiben ohne
    /// Win32, sonst bricht Linux/Android (Termux) der Default-Pfad.
    /// Nur Produktionscode: die Needles stehen in diesem Test selbst.
    #[test]
    fn session_pfad_hat_kein_win32() {
        let win = format!("{}::", "windows");
        let api = format!("{}::", "winapi");
        let hwnd = format!("{}{}", "HWN", "D");
        let iconic = format!("{}{}", "IsIcon", "ic");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for path in [
            "src/startup.rs",
            "src/transcript.rs",
            "src/repl/commands.rs",
        ] {
            let full = root.join(path);
            let raw = std::fs::read_to_string(&full)
                .unwrap_or_else(|e| panic!("{}: {e}", full.display()));
            let cut = raw.find("#[cfg(test)]").unwrap_or(raw.len());
            let text = &raw[..cut];
            for needle in [win.as_str(), api.as_str(), hwnd.as_str(), iconic.as_str()] {
                assert!(
                    !text.contains(needle),
                    "{path} enthaelt {needle} — Session-Pfad muss plattformrein bleiben"
                );
            }
        }
    }
}
