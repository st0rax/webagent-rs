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

/// Eine Swarm-Phase-1-Abfrage: `(brain, prompt) -> Antworttext`.
pub type SwarmQueryFn = fn(&str, &str) -> Result<String, String>;

static SWARM_QUERY: Mutex<Option<SwarmQueryFn>> = Mutex::new(None);
static SWARM_BRAINS: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Setzt die Swarm-Abfrage. Tests und die CLI können denselben Einstieg fahren.
pub fn set_swarm_query_fn(f: SwarmQueryFn) {
    *SWARM_QUERY.lock().expect("swarm hook") = Some(f);
}

/// Überschreibt die Brain-Liste für Phase 1 (leer = `available_brain_ids`).
pub fn set_swarm_brains(ids: &[&str]) {
    *SWARM_BRAINS.lock().expect("swarm brains") =
        Some(ids.iter().map(|s| (*s).to_string()).collect());
}

fn default_swarm_query(brain: &str, prompt: &str) -> Result<String, String> {
    crate::repl::isolated_query(brain, prompt, true, None)
}

/// Phase-1-Antworten für `/swarm`: Hook oder `isolated_query` je Brain.
pub fn swarm_answers(prompt: &str) -> Vec<(String, String)> {
    let query = SWARM_QUERY
        .lock()
        .expect("swarm hook")
        .unwrap_or(default_swarm_query);
    let brains = SWARM_BRAINS
        .lock()
        .expect("swarm brains")
        .clone()
        .unwrap_or_else(crate::config::available_brain_ids);
    let mut out = Vec::new();
    for brain in brains {
        if let Ok(text) = query(&brain, prompt) {
            if !text.is_empty() {
                out.push((brain, text));
            }
        }
    }
    out
}

/// Session-Karten aus einem echten Swarm-Lauf (Hook oder isolated_query).
pub fn session_swarm_cards(prompt: &str) -> Vec<crate::transcript::SessionTurn> {
    let answers = swarm_answers(prompt);
    crate::transcript::session_turns_from_swarm(prompt, &answers)
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
        assert!(
            tui.contains("session_swarm_cards"),
            "TUI /swarm muss die gelieferten Karten aus session_swarm_cards nehmen"
        );
        let empty = format!("session_turns_from_swarm({}&[])", "&prompt, ");
        assert!(
            !tui.contains(&empty),
            "TUI darf /swarm nicht mit leeren Antworten fahren"
        );
    }

    #[test]
    fn swarm_karten_kommen_aus_echten_antworten() {
        fn fake(brain: &str, prompt: &str) -> Result<String, String> {
            Ok(format!("{prompt} von {brain}"))
        }
        set_swarm_brains(&["claude", "chatgpt"]);
        set_swarm_query_fn(fake);
        let cards = session_swarm_cards("fasst zusammen");
        assert_eq!(cards.len(), 3, "{cards:?}");
        assert_eq!(cards[0].kind, crate::transcript::SessionTurnKind::User);
        assert_eq!(cards[0].body, "fasst zusammen");
        assert_eq!(cards[1].kind, crate::transcript::SessionTurnKind::Brain);
        assert_eq!(cards[2].kind, crate::transcript::SessionTurnKind::Brain);
        assert_eq!(cards[1].body, "claude: fasst zusammen von claude");
        assert_eq!(cards[2].body, "chatgpt: fasst zusammen von chatgpt");
    }
}
