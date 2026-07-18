//! brain_score — Leistungsindex je Brain aus echter Nutzung, kein synthetischer
//! Bonus/Malus-Zaehler.
//!
//! Konzept (mit dem Nutzer abgestimmt): ein einzelner Bonus/Malus-Wert verwischt
//! genau das, was interessant ist (welches Brain ist WOFUER gut), und reagiert
//! entweder zu traege oder zu nervoes auf einzelne Ausreisser. Deshalb zwei
//! getrennte Ideen, hier nur die erste umgesetzt:
//!
//! 1. **Reliability-Score** (dieses Modul): ein Wilson-Score-Lower-Bound auf
//!    Erfolg/Fehlschlag-Ereignissen aus echten `swarm_query`/`relay_single_turn`-
//!    Aufrufen, über ein rollierendes Fenster der letzten `WINDOW_SIZE` Ereignisse
//!    (nicht kontinuierliche Exponential-Decay — einfacher zu pruefen, gleicher
//!    Effekt: alte Ausreisser fallen irgendwann ganz aus dem Fenster). Bei wenig
//!    Daten bleibt der Score automatisch vorsichtig (Wilson zieht in Richtung 0.5)
//!    statt durch 1-2 Ereignisse sofort auszuschlagen.
//! 2. **Faehigkeitsprofil** (Follow-up, nicht hier): explizit per `/benchmark`,
//!    strukturiert nach Dimension (reasoning/code/kreativ/...) statt einer
//!    Gesamtnote — siehe `/benchmark`-Befehl fuer den ersten Teil davon
//!    (maximale Prompt-Laenge).
//!
//! Externe Blockierungen (Tageslimit/Login/Cloudflare) zaehlen als Fehlschlag im
//! Sinne von "gerade nicht nutzbar" — aber der Grund wird mitgespeichert, damit
//! ein Blockade-Cluster von echten Qualitaetsproblemen unterscheidbar bleibt
//! (siehe [[external-blocks-flag-not-fail]]: die Ursache wird sichtbar gemacht,
//! nicht versteckt, auch wenn sie hier in den Score einfliesst).

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};

use crate::config::data_dir;

/// Wie viele der letzten Ereignisse pro Brain in den Score einfliessen. Aeltere
/// Ereignisse bleiben im Log (Historie), zaehlen aber nicht mehr fuer den
/// aktuellen Score -- das ist die "Recency"-Komponente ohne Decay-Formel.
const WINDOW_SIZE: usize = 40;
/// 95%-Konfidenz-Z-Wert fuer den Wilson-Score.
const Z: f64 = 1.96;

lazy_static! {
    static ref WRITE_LOCK: Mutex<()> = Mutex::new(());
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Event {
    brain_id: String,
    ts: String,
    success: bool,
    reason: Option<String>,
    latency_ms: u64,
    prompt_chars: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BrainStats {
    pub brain_id: String,
    /// Wilson-Score-Lower-Bound, 0.0-1.0. Hoeher = zuverlaessiger.
    pub reliability: f64,
    pub window_events: usize,
    pub window_successes: usize,
    pub avg_latency_ms: u64,
    pub last_reason: Option<String>,
}

fn events_path() -> PathBuf {
    data_dir().join("brain_score").join("events.jsonl")
}

/// Ein Ereignis anhaengen (JSON-Lines, append-only -- volle Historie bleibt
/// erhalten, auch wenn der Score nur das Fenster der letzten `WINDOW_SIZE` nutzt).
pub fn record_event(
    brain_id: &str,
    success: bool,
    reason: Option<&str>,
    latency_ms: u64,
    prompt_chars: usize,
) {
    record_event_at(
        brain_id,
        success,
        reason,
        latency_ms,
        prompt_chars,
        &events_path(),
    );
}

fn record_event_at(
    brain_id: &str,
    success: bool,
    reason: Option<&str>,
    latency_ms: u64,
    prompt_chars: usize,
    path: &PathBuf,
) {
    let _guard = WRITE_LOCK.lock();
    if let Some(parent) = path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let event = Event {
        brain_id: brain_id.to_string(),
        ts: crate::now_rfc3339(),
        success,
        reason: reason.map(str::to_string),
        latency_ms,
        prompt_chars,
    };
    let Ok(line) = serde_json::to_string(&event) else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{line}");
}

fn load_events(path: &PathBuf) -> Vec<Event> {
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

/// Wilson-Score-Lower-Bound fuer `successes` von `n` Versuchen. `n == 0` liefert
/// 0.5 (voelliger Unsicherheit) statt 0.0 oder 1.0 -- ein Brain ohne Daten ist
/// nicht "schlecht", es ist unbekannt.
fn wilson_lower_bound(successes: usize, n: usize) -> f64 {
    if n == 0 {
        return 0.5;
    }
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = Z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    ((center - margin) / denom).clamp(0.0, 1.0)
}

/// Statistik fuer ein Brain aus dem rollierenden Fenster der letzten
/// `WINDOW_SIZE` Ereignisse. `None`, wenn noch keine Ereignisse vorliegen.
pub fn stats(brain_id: &str) -> Option<BrainStats> {
    stats_at(brain_id, &events_path())
}

fn stats_at(brain_id: &str, path: &PathBuf) -> Option<BrainStats> {
    let all = load_events(path);
    let mut window: Vec<&Event> = all.iter().filter(|e| e.brain_id == brain_id).collect();
    if window.is_empty() {
        return None;
    }
    if window.len() > WINDOW_SIZE {
        window = window.split_off(window.len() - WINDOW_SIZE);
    }
    let n = window.len();
    let successes = window.iter().filter(|e| e.success).count();
    let avg_latency_ms = if n > 0 {
        window.iter().map(|e| e.latency_ms).sum::<u64>() / n as u64
    } else {
        0
    };
    let last_reason = window
        .iter()
        .rev()
        .find(|e| !e.success)
        .and_then(|e| e.reason.clone());
    Some(BrainStats {
        brain_id: brain_id.to_string(),
        reliability: wilson_lower_bound(successes, n),
        window_events: n,
        window_successes: successes,
        avg_latency_ms,
        last_reason,
    })
}

/// Statistik fuer alle Brains, die mindestens ein Ereignis haben -- absteigend
/// nach Reliability sortiert.
pub fn leaderboard() -> Vec<BrainStats> {
    leaderboard_at(&events_path())
}

fn leaderboard_at(path: &PathBuf) -> Vec<BrainStats> {
    let all = load_events(path);
    let mut per_brain: HashMap<String, ()> = HashMap::new();
    for e in &all {
        per_brain.entry(e.brain_id.clone()).or_insert(());
    }
    let mut result: Vec<BrainStats> = per_brain
        .keys()
        .filter_map(|id| stats_at(id, path))
        .collect();
    result.sort_by(|a, b| {
        b.reliability
            .partial_cmp(&a.reliability)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_path() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_score_test_{nanos}_{n}.jsonl"))
    }

    #[test]
    fn wilson_no_data_is_uncertain_not_zero() {
        assert_eq!(wilson_lower_bound(0, 0), 0.5);
    }

    #[test]
    fn wilson_prefers_more_evidence_at_same_ratio() {
        // 90% aus 10 Versuchen ist weniger sicher als 90% aus 100 -- der Score
        // muss das widerspiegeln (weniger Daten -> vorsichtigerer, niedrigerer
        // Lower Bound), sonst waere ein frueher Zufallstreffer genauso viel wert
        // wie eine belastbare Historie.
        let few = wilson_lower_bound(9, 10);
        let many = wilson_lower_bound(90, 100);
        assert!(many > few, "many={many} sollte > few={few} sein");
    }

    #[test]
    fn no_events_yields_none() {
        let path = unique_path();
        assert_eq!(stats_at("kimi", &path), None);
    }

    #[test]
    fn reliable_brain_scores_higher_than_flaky_one() {
        let path = unique_path();
        for _ in 0..10 {
            record_event_at("kimi", true, None, 1000, 20, &path);
        }
        for _ in 0..10 {
            record_event_at("qwen", false, Some("blocked"), 500, 20, &path);
        }
        let kimi = stats_at("kimi", &path).unwrap();
        let qwen = stats_at("qwen", &path).unwrap();
        assert!(kimi.reliability > qwen.reliability);
        assert_eq!(kimi.window_successes, 10);
        assert_eq!(qwen.window_successes, 0);
        assert_eq!(qwen.last_reason.as_deref(), Some("blocked"));
    }

    #[test]
    fn window_drops_old_events_beyond_window_size() {
        let path = unique_path();
        // Erst WINDOW_SIZE Fehlschlaege, dann genug Erfolge, um sie komplett aus
        // dem Fenster zu verdraengen.
        for _ in 0..WINDOW_SIZE {
            record_event_at("zai", false, Some("timeout"), 100, 10, &path);
        }
        for _ in 0..WINDOW_SIZE {
            record_event_at("zai", true, None, 100, 10, &path);
        }
        let s = stats_at("zai", &path).unwrap();
        assert_eq!(s.window_events, WINDOW_SIZE);
        assert_eq!(s.window_successes, WINDOW_SIZE);
        assert_eq!(s.last_reason, None);
    }

    #[test]
    fn leaderboard_sorts_by_reliability_descending() {
        let path = unique_path();
        for _ in 0..5 {
            record_event_at("kimi", true, None, 100, 10, &path);
        }
        for _ in 0..5 {
            record_event_at("qwen", false, Some("blocked"), 100, 10, &path);
        }
        let board = leaderboard_at(&path);
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].brain_id, "kimi");
        assert_eq!(board[1].brain_id, "qwen");
    }
}
