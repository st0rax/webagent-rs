//! canary — periodischer 8-Brain-Health-Check (Backlog B2).
//!
//! Self-contained Modul mit pub API. CLI-Subcommand + `mod canary` verdrahtet
//! der Orchestrator; hier nur die Mess-Logik:
//! pro Brain Latenz + pass/fail + reason → `Vec<CanaryResult>`.
//!
//! Default-Probe ist **leicht** (Selector-Datei / Spec vorhanden), ohne vollen
//! Browser-Relay — damit Canary headless und CI-tauglich bleibt. Schwere
//! Live-Relay-Probes koennen via [`run_canary_with`] injiziert werden.

use std::path::Path;
use std::time::Instant;

use crate::config::{available_brain_ids, brains};

/// Ein Canary-Ergebnis pro Brain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanaryResult {
    pub brain_id: String,
    /// `true` = Probe ok.
    pub ok: bool,
    pub latency_ms: u64,
    /// Kurzgrund: `ok`, `missing_selectors`, `missing_spec`, oder Probe-Fehlertext.
    pub reason: String,
}

/// Leichte Default-Probe: Brain-Spec vorhanden und Selector-Datei lesbar.
pub fn default_probe(brain_id: &str) -> Result<(), String> {
    let map = brains();
    let spec = map
        .get(brain_id)
        .ok_or_else(|| "missing_spec".to_string())?;
    let sel = spec
        .get("selectors")
        .map(String::as_str)
        .unwrap_or("")
        .trim();
    if sel.is_empty() {
        return Err("missing_selectors".into());
    }
    if !Path::new(sel).is_file() {
        return Err("missing_selectors".into());
    }
    Ok(())
}

/// Alle bekannten Brains mit injizierbarer Probe pruefen.
pub fn run_canary_with<F>(probe: F) -> Vec<CanaryResult>
where
    F: Fn(&str) -> Result<(), String>,
{
    let mut out = Vec::new();
    for brain_id in available_brain_ids() {
        let t0 = Instant::now();
        let (ok, reason) = match probe(&brain_id) {
            Ok(()) => (true, "ok".to_string()),
            Err(e) => (false, e),
        };
        out.push(CanaryResult {
            brain_id,
            ok,
            latency_ms: t0.elapsed().as_millis() as u64,
            reason,
        });
    }
    out
}

/// Standard-Canary ueber alle Brains (leichte Default-Probe).
pub fn run_canary() -> Vec<CanaryResult> {
    run_canary_with(default_probe)
}

/// True wenn jeder Brain `ok` ist.
pub fn all_ok(results: &[CanaryResult]) -> bool {
    !results.is_empty() && results.iter().all(|r| r.ok)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn run_canary_returns_one_entry_per_brain() {
        let results = run_canary();
        let ids = available_brain_ids();
        assert_eq!(results.len(), ids.len());
        assert!(!results.is_empty(), "registry should list brains");
        for (r, id) in results.iter().zip(ids.iter()) {
            assert_eq!(&r.brain_id, id);
            // reason always non-empty
            assert!(!r.reason.is_empty());
        }
    }

    #[test]
    fn run_canary_with_injected_probe_records_fail_and_latency() {
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let results = run_canary_with(|id| {
            CALLS.fetch_add(1, Ordering::SeqCst);
            if id == "qwen" || id.starts_with('q') {
                Err("rate_limit".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(CALLS.load(Ordering::SeqCst), results.len());
        assert!(results.iter().any(|r| !r.ok));
        for r in &results {
            // Instant resolution may be 0ms on fast machines — only check field exists
            let _ = r.latency_ms;
            if !r.ok {
                assert_eq!(r.reason, "rate_limit");
            } else {
                assert_eq!(r.reason, "ok");
            }
        }
    }

    #[test]
    fn all_ok_false_when_any_fail() {
        let sample = vec![
            CanaryResult {
                brain_id: "a".into(),
                ok: true,
                latency_ms: 1,
                reason: "ok".into(),
            },
            CanaryResult {
                brain_id: "b".into(),
                ok: false,
                latency_ms: 2,
                reason: "timeout_no_text".into(),
            },
        ];
        assert!(!all_ok(&sample));
        assert!(all_ok(&sample[..1]));
        assert!(!all_ok(&[]));
    }
}
