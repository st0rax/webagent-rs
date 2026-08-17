//! Verdrahtung Binary → Lib: `/brute` und `webagent probe` sind dieselbe Funktion.

use std::sync::Mutex;

/// Signatur von [`crate`]-externem `cmd_probe` (src/commands/ui.rs).
#[allow(clippy::too_many_arguments)]
pub type ProbeFn = fn(
    Option<&str>,
    Option<&str>,
    Option<&str>,
    bool,
    bool,
    Option<&str>,
    bool,
    bool,
    bool,
    bool,
) -> i32;

static PROBE: Mutex<Option<ProbeFn>> = Mutex::new(None);

/// Setzt den Probe-Einstieg. `main` übergibt `cmd_probe`.
pub fn set_probe_fn(f: ProbeFn) {
    *PROBE.lock().expect("probe hook") = Some(f);
}

/// `/brute <url>`: derselbe Pfad wie `webagent probe --url <url> --write`.
pub fn run_brute_write(url: &str, headless: bool) -> i32 {
    let Some(url) = crate::repl::commands::brute_http_url(url) else {
        eprintln!("[brute] Nutzung: /brute <https://chat-url>");
        return 2;
    };
    let hook = PROBE.lock().expect("probe hook");
    match *hook {
        Some(f) => f(
            Some(&url),
            None,
            None,
            true,
            false,
            None,
            false,
            false,
            false,
            headless,
        ),
        None => {
            eprintln!("[brute] Probe-Funktion nicht verdrahtet (kein CLI-Einstieg)");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn brute_ohne_http_startet_keinen_probe() {
        assert_eq!(run_brute_write("ftp://x", true), 2);
        assert_eq!(run_brute_write("", true), 2);
        assert_eq!(run_brute_write("chat.example.com", true), 2);
    }

    #[test]
    fn brute_http_url_ruft_probe_mit_write() {
        static SAW_WRITE: AtomicBool = AtomicBool::new(false);
        #[allow(clippy::too_many_arguments)]
        fn fake(
            url: Option<&str>,
            _id: Option<&str>,
            _brain: Option<&str>,
            write: bool,
            verify: bool,
            _open: Option<&str>,
            dump: bool,
            generating: bool,
            stop_diff: bool,
            _headless: bool,
        ) -> i32 {
            assert_eq!(url, Some("https://chat.example.com/app"));
            assert!(write);
            assert!(!verify && !dump && !generating && !stop_diff);
            SAW_WRITE.store(true, Ordering::SeqCst);
            0
        }
        set_probe_fn(fake);
        assert_eq!(run_brute_write("https://chat.example.com/app", true), 0);
        assert!(SAW_WRITE.load(Ordering::SeqCst));
    }

    #[test]
    fn brute_und_evolve_pfade_starten_kein_subprozess() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let needle = format!("{}{}", "current", "_exe");
        for rel in ["src/tui.rs", "src/repl/mod.rs"] {
            let text = std::fs::read_to_string(root.join(rel)).unwrap();
            assert!(
                !text.contains(&needle),
                "{rel} darf brute/evolve nicht per Subprozess starten"
            );
        }
        let hooks = std::fs::read_to_string(root.join("src/bin_hooks.rs")).unwrap();
        assert!(hooks.contains("set_probe_fn"));
        assert!(hooks.contains("run_brute_write"));
        let tui = std::fs::read_to_string(root.join("src/tui.rs")).unwrap();
        assert!(tui.contains("run_evolve"));
        assert!(tui.contains("run_brute_write"));
    }
}
