//! circuit_breaker — pro Brain merken, ob es aktuell sinnlos ist, es zu befragen.
//!
//! Ohne das: ein blockiertes/rate-limitiertes Brain (qwen-Tageslimit, claude-
//! Session-Limit) wird bei jedem `/swarm`/`relay`-Aufruf erneut in den vollen
//! `wait_response`-Timeout gejagt, obwohl das Ergebnis vorhersehbar ist. Der
//! Breaker haelt fest: nach N aufeinanderfolgenden Fehlschlaegen fuer ein Brain
//! wird es fuer eine Cooldown-Zeit uebersprungen statt erneut versucht — degradiert
//! sichtbar (siehe [[external-blocks-flag-not-fail]]), statt den ganzen Lauf zu
//! blockieren.
//!
//! Zustand ist prozessuebergreifend auf Disk (JSON, atomic write), weil `/swarm`
//! und `relay` typischerweise als separate Prozesse/Aufrufe laufen.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::config::data_dir;

const DEFAULT_MAX_FAILURES: u32 = 3;
const DEFAULT_COOLDOWN_SECS: i64 = 900; // 15 min

lazy_static! {
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct BrainState {
    consecutive_failures: u32,
    /// Unix-Sekunden, bis zu denen dieses Brain uebersprungen wird. `None`/
    /// vergangen heisst: Breaker zu, Brain darf befragt werden.
    open_until: Option<i64>,
    last_reason: Option<String>,
}

type StateMap = HashMap<String, BrainState>;

fn state_path() -> PathBuf {
    data_dir().join("circuit_breaker").join("state.json")
}

fn max_failures() -> u32 {
    std::env::var("WEBAGENT_BREAKER_MAX_FAILURES")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_MAX_FAILURES)
}

fn cooldown_secs() -> i64 {
    std::env::var("WEBAGENT_BREAKER_COOLDOWN_S")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_COOLDOWN_SECS)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load(path: &PathBuf) -> StateMap {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(path: &PathBuf, state: &StateMap) {
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let Ok(json) = serde_json::to_string_pretty(state) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

/// Wie lange (Sekunden) `brain_id` noch uebersprungen werden sollte, falls der
/// Breaker offen ist. `None` = Brain darf befragt werden.
pub fn check(brain_id: &str) -> Option<i64> {
    check_at(brain_id, &state_path())
}

fn check_at(brain_id: &str, path: &PathBuf) -> Option<i64> {
    let _guard = WRITE_LOCK.lock();
    let state = load(path);
    let entry = state.get(brain_id)?;
    let until = entry.open_until?;
    let remaining = until - now_secs();
    if remaining > 0 {
        Some(remaining)
    } else {
        None
    }
}

/// Erfolgreicher Aufruf: setzt den Zaehler fuer `brain_id` zurueck.
pub fn record_success(brain_id: &str) {
    record_success_at(brain_id, &state_path());
}

fn record_success_at(brain_id: &str, path: &PathBuf) {
    let _guard = WRITE_LOCK.lock();
    let mut state = load(path);
    state.remove(brain_id);
    save(path, &state);
}

/// Fehlschlag (Timeout/Rate-Limit/Blocked): erhoeht den Zaehler; oeffnet den
/// Breaker, sobald `WEBAGENT_BREAKER_MAX_FAILURES` erreicht ist.
pub fn record_failure(brain_id: &str, reason: &str) {
    record_failure_at(brain_id, reason, &state_path());
}

fn record_failure_at(brain_id: &str, reason: &str, path: &PathBuf) {
    let _guard = WRITE_LOCK.lock();
    let mut state = load(path);
    let entry = state.entry(brain_id.to_string()).or_default();
    entry.consecutive_failures += 1;
    entry.last_reason = Some(reason.to_string());
    if entry.consecutive_failures >= max_failures() {
        entry.open_until = Some(now_secs() + cooldown_secs());
        eprintln!(
            "[circuit_breaker] {brain_id}: offen fuer {}s nach {} Fehlschlaegen ({reason})",
            cooldown_secs(),
            entry.consecutive_failures
        );
    }
    save(path, &state);
}

/// Telemetrie-Snapshot eines Brains fuer `/breaker` und externe Diagnose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BreakerSnapshot {
    pub brain_id: String,
    /// `true` = Breaker offen, Brain wird uebersprungen.
    pub open: bool,
    pub consecutive_failures: u32,
    /// Unix-Sekunden, bis der Breaker zu ist (`None` wenn nie geoeffnet).
    pub open_until: Option<i64>,
    /// Verbleibende Cooldown-Sekunden, falls noch offen.
    pub remaining_secs: Option<i64>,
    pub last_reason: Option<String>,
}

/// Lese-API: alle bekannten Brain-Zustaende (sortiert nach `brain_id`).
/// Brains ohne Eintrag erscheinen nicht (analog zu `brain_score::leaderboard`).
pub fn snapshots() -> Vec<BreakerSnapshot> {
    snapshots_at(&state_path())
}

fn snapshots_at(path: &PathBuf) -> Vec<BreakerSnapshot> {
    let _guard = WRITE_LOCK.lock();
    let state = load(path);
    let now = now_secs();
    let mut out: Vec<BreakerSnapshot> = state
        .into_iter()
        .map(|(brain_id, entry)| {
            let remaining = entry.open_until.and_then(|until| {
                let r = until - now;
                if r > 0 {
                    Some(r)
                } else {
                    None
                }
            });
            BreakerSnapshot {
                brain_id,
                open: remaining.is_some(),
                consecutive_failures: entry.consecutive_failures,
                open_until: entry.open_until,
                remaining_secs: remaining,
                last_reason: entry.last_reason,
            }
        })
        .collect();
    out.sort_by(|a, b| a.brain_id.cmp(&b.brain_id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_breaker_test_{nanos}_{n}.json"))
    }

    #[test]
    fn closed_breaker_allows_calls() {
        let path = unique_path();
        assert_eq!(check_at("kimi", &path), None);
    }

    #[test]
    fn opens_after_max_failures() {
        let path = unique_path();
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "blocked", &path);
        }
        let remaining = check_at("qwen", &path).expect("breaker should be open");
        assert!(remaining > 0 && remaining <= DEFAULT_COOLDOWN_SECS);
    }

    #[test]
    fn stays_closed_below_threshold() {
        let path = unique_path();
        for _ in 0..(DEFAULT_MAX_FAILURES - 1) {
            record_failure_at("mistral", "blocked", &path);
        }
        assert_eq!(check_at("mistral", &path), None);
    }

    #[test]
    fn success_resets_failure_count() {
        let path = unique_path();
        record_failure_at("zai", "timeout", &path);
        record_failure_at("zai", "timeout", &path);
        record_success_at("zai", &path);
        for _ in 0..(DEFAULT_MAX_FAILURES - 1) {
            record_failure_at("zai", "timeout", &path);
        }
        // Zaehler wurde zurueckgesetzt, also noch nicht offen.
        assert_eq!(check_at("zai", &path), None);
    }

    #[test]
    fn other_brains_are_independent() {
        let path = unique_path();
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "blocked", &path);
        }
        assert!(check_at("qwen", &path).is_some());
        assert_eq!(check_at("kimi", &path), None);
    }

    #[test]
    fn snapshots_report_open_and_partial_state() {
        let path = unique_path();
        // partial failures — closed breaker, but visible in telemetry
        record_failure_at("zai", "timeout", &path);
        record_failure_at("zai", "timeout", &path);
        // trip open
        for _ in 0..DEFAULT_MAX_FAILURES {
            record_failure_at("qwen", "blocked", &path);
        }
        // success removes entry entirely
        record_failure_at("kimi", "rate-limit", &path);
        record_success_at("kimi", &path);

        let snaps = snapshots_at(&path);
        assert_eq!(snaps.len(), 2, "kimi reset should disappear; zai+qwen remain");

        let zai = snaps.iter().find(|s| s.brain_id == "zai").expect("zai");
        assert!(!zai.open);
        assert_eq!(zai.consecutive_failures, 2);
        assert!(zai.open_until.is_none());
        assert_eq!(zai.remaining_secs, None);
        assert_eq!(zai.last_reason.as_deref(), Some("timeout"));

        let qwen = snaps.iter().find(|s| s.brain_id == "qwen").expect("qwen");
        assert!(qwen.open);
        assert_eq!(qwen.consecutive_failures, DEFAULT_MAX_FAILURES);
        assert!(qwen.open_until.is_some());
        assert!(qwen.remaining_secs.is_some_and(|r| r > 0 && r <= DEFAULT_COOLDOWN_SECS));
        assert_eq!(qwen.last_reason.as_deref(), Some("blocked"));

        // sorted by brain_id
        assert!(snaps.windows(2).all(|w| w[0].brain_id <= w[1].brain_id));
    }
}
