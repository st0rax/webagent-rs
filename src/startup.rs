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

    #[test]
    fn betriebs_markdown_hat_eine_wahrheit() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
// Lebende Betriebsdokumente. Datierte Uebergaben gehoeren NICHT hierher:
        // sie werden nach ihrer Uebergabe archiviert und duerfen dann ein
        // Archiv-Banner tragen. Der dauerhafte Einstieg ist `START_HERE.md`,
        // der laufende Stand `docs/CURRENT_WORK.md`. Aufgabentafel,
        // Arbeitsvertrag und Umsetzungsstatus sind lebende Multidev-Dokumente.
        let living = [
            "README.md",
            "AGENTS.md",
            "CONVENTIONS.md",
            "CONTRIBUTING.md",
            "START_HERE.md",
            "GOALS.md",
            "docs/OVERVIEW.md",
            "docs/PROTOCOL_SCHEMA.md",
            "docs/COLLABORATION.md",
            "docs/CURRENT_WORK.md",
            "docs/API_BRIDGE.md",
            "docs/REFERENZEN.md",
            "docs/TASKBOARD.md",
            "docs/WORK_CONTRACT.md",
            "docs/WEB_UI_API_TOOL_RESET_STATUS.md",
            "docs/WEB_UI_API_TOOL_RESET.md",
        ];
        for rel in living {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            let head: String = text.lines().take(8).collect::<Vec<_>>().join("\n");
            assert!(
                !head.contains("**Archiv"),
                "{rel} ist Betrieb und darf kein Archiv-Banner tragen"
            );
        }
        let conventions = std::fs::read_to_string(root.join("CONVENTIONS.md")).unwrap();
        assert!(
            !conventions.contains("zuerst portiert")
                && !conventions.contains("../tests/test_protocol.py")
                && !conventions.contains("START_HERE.md`** — einziger Einstiegspunkt"),
            "CONVENTIONS.md darf keinen Port-Auftrag und keinen START_HERE-Einstieg enthalten"
        );
        fn walk(dir: &std::path::Path, acc: &mut Vec<std::path::PathBuf>) {
            let Ok(rd) = std::fs::read_dir(dir) else {
                return;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                    // `.github` traegt Issue-/PR-Vorlagen, also Werkzeug der
                    // Plattform statt Projektdokumentation. Ein Banner darin
                    // wuerde in jedem erzeugten Issue und PR mitlaufen.
                    if name == "target" || name == ".git" || name == ".github" {
                        continue;
                    }
                    walk(&p, acc);
                } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                    acc.push(p);
                }
            }
        }
        let mut files = Vec::new();
        walk(root, &mut files);
        for path in files {
            let rel = path.strip_prefix(root).unwrap();
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            if living.contains(&rel_s.as_str()) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let head: String = text.lines().take(8).collect::<Vec<_>>().join("\n");
            let head_l = head.to_ascii_lowercase();
            assert!(
                head_l.contains("**archiv") || head_l.contains("**referenz"),
                "{rel_s} braucht Archiv- oder Referenz-Banner in den ersten Zeilen"
            );
        }
    }

    #[test]
    fn code_score_und_brain_score_teilen_eine_wilson_rechnung() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let needle = format!("{}::{}", "scoring", "wilson_lower_bound");
        for rel in ["src/code_score.rs", "src/brain_score.rs"] {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            let cut = text.find("#[cfg(test)]").unwrap_or(text.len());
            assert!(
                text[..cut].contains(&needle),
                "{rel} muss {needle} nutzen, keine zweite Kopie"
            );
        }
        let shared = std::fs::read_to_string(root.join("src/scoring.rs")).unwrap();
        assert!(
            shared.contains("pub fn wilson_lower_bound"),
            "wilson_lower_bound muss die gelieferte Funktion sein"
        );
    }

    #[test]
    fn tui_ruft_nicht_den_zweiten_kachel_pfad() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tui.rs");
        let raw = std::fs::read_to_string(&path).unwrap();
        let cut = raw.find("#[cfg(test)]").unwrap_or(raw.len());
        let text = &raw[..cut];
        let a = format!("{}{}", "arrange_brain", "_grid");
        let b = format!("{}{}", "toggle_brain", "_grid");
        assert!(!text.contains(&a), "TUI darf {a} nicht aufrufen");
        assert!(!text.contains(&b), "toter zweiter Pfad {b} muss weg");
    }

    #[test]
    fn release_workflow_nennt_die_drei_binaries() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
        let yml =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for needle in [
            "webagent-windows-x86_64.exe",
            "WebView2Loader.dll",
            "webagent-linux-x86_64",
            "webagent-aarch64-linux-android",
            "--no-default-features --features tui",
        ] {
            assert!(yml.contains(needle), "release.yml fehlt {needle}");
        }
    }
}
