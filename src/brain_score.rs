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

use crate::bench_scoring::wilson_lower_bound;
use crate::config::data_dir;

/// Wie viele der letzten Ereignisse pro Brain in den Score einfliessen. Aeltere
/// Ereignisse bleiben im Log (Historie), zaehlen aber nicht mehr fuer den
/// aktuellen Score -- das ist die "Recency"-Komponente ohne Decay-Formel.
const WINDOW_SIZE: usize = 40;

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

    // Storax-Vorgabe (2026-08-01): die Nutzungs-/Phase-A-Ereignisse aus
    // events.jsonl gehoeren sichtbar in den TUI-Baum — Erfolg, Latenz und
    // Prompt-Groesse sind genau die Kennzahlen, die dort aufklappbar sind.
    // Nur im Spiegelmodus (`--verbose`-Benchmark), damit Tests und der
    // normale Betrieb den Bus nicht mit Nutzungsdaten füllen.
    if crate::bench_events::echo_bus_enabled() {
        let level = if success {
            crate::bench_events::Level::Pass
        } else {
            crate::bench_events::Level::Fail
        };
        crate::bench_events::emit_detailed(
            level,
            Some(brain_id),
            &format!(
                "[brain:{brain_id}] {} {latency_ms}ms {prompt_chars}Z",
                if success { "ok" } else { "FEHLER" }
            ),
            reason,
        );
    }
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

/// Statistik fuer ein Brain aus dem rollierenden Fenster der letzten
/// `WINDOW_SIZE` Ereignisse. `None`, wenn noch keine Ereignisse vorliegen.
/// p95 der Latenz ERFOLGREICHER Aufrufe eines Brains, in Sekunden.
///
/// Grundlage fuer datenbasierte Timeouts: die fest verdrahtete
/// Multiplikatoren-Tabelle in [`crate::timeouts`] war nachweislich in beide
/// Richtungen falsch (Messung 2026-07-26 ueber 2072 Erfolgslaeufe: claude 1.8
/// verdrahtet vs. 0.9 gemessen, kimi 1.3 vs. 2.2). Fehlschlaege bleiben
/// draussen — deren Dauer ist ein Timeout, kein Antwortverhalten, und wuerde
/// die Schaetzung nach oben ziehen.
///
/// `None`, wenn weniger als `min_samples` Erfolge vorliegen; der Aufrufer
/// faellt dann auf seine Startwerte zurueck.
pub fn latency_p95_secs(brain_id: &str, min_samples: usize) -> Option<f64> {
    latency_p95_at(brain_id, min_samples, &events_path())
}

fn latency_p95_at(brain_id: &str, min_samples: usize, path: &PathBuf) -> Option<f64> {
    let all = load_events(path);
    let mut lat: Vec<f64> = all
        .iter()
        .filter(|e| e.brain_id == brain_id && e.success && e.latency_ms > 0)
        .map(|e| e.latency_ms as f64 / 1000.0)
        .collect();
    if lat.len() < min_samples.max(1) {
        return None;
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((lat.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    lat.get(idx).copied()
}

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

/// Normalisierte Routing-Gewichtung fuer ein Brain aus Performance-Zahlen.
///
/// Gewichtet Task-Erfolgsquote (50 %), Antwortzeit (30 %) und Robustheit
/// (20 %, Abzuege fuer JSON-Fehler und Reparaturen). Reine Funktion — nichts
/// wird gelesen oder geschrieben; das Ergebnis liegt immer in [0,1] und ist
/// deterministisch (gleiche Eingabe → gleiche Gewichtung).
pub fn calculate_brain_routing_weight(
    response_ms: u64,
    json_errors: u32,
    repair_count: u32,
    successful_tasks: u32,
    failed_tasks: u32,
) -> f64 {
    let total_tasks = (successful_tasks as f64) + (failed_tasks as f64);
    let task_score = if total_tasks > 0.0 {
        (successful_tasks as f64) / total_tasks
    } else {
        0.5
    };
    let latency_norm = (response_ms as f64 / 10_000.0).clamp(0.0, 1.0);
    let latency_score = 1.0 - latency_norm;
    let json_penalty = (json_errors as f64 * 0.15).clamp(0.0, 1.0);
    let repair_penalty = (repair_count as f64 * 0.10).clamp(0.0, 1.0);
    let quality_score = (1.0 - json_penalty - repair_penalty).clamp(0.0, 1.0);
    (0.50 * task_score + 0.30 * latency_score + 0.20 * quality_score).clamp(0.0, 1.0)
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

    #[test]
    fn record_event_beruehrt_den_bus_nur_im_spiegelmodus() {
        // Negativ-Pruefung: OHNE Spiegelmodus darf record_event nichts in den
        // Bus legen. Der Test serialisiert sich mit den anderen Bus-Tests und
        // prueft gezielt die Abwesenheit eines brain_score-Eintrags statt
        // `len() == 0` — letzteres waere gegen parallele Fremd-Events anfaellig.
        let _guard = crate::bench_events::test_bus_mutex().lock();
        crate::bench_events::clear();
        let path = unique_path();
        record_event_at("kimi", true, Some("grund"), 123, 456, &path);
        let events = crate::bench_events::snapshot();
        assert!(
            !events
                .iter()
                .any(|e| e.brain.as_deref() == Some("kimi") && e.text.starts_with("[brain:kimi]")),
            "ohne Spiegelmodus duerfen keine Nutzungs-Events in den Bus"
        );
    }

    #[test]
    fn routing_weight_fast_and_successful_beats_slow_and_flaky() {
        let good = calculate_brain_routing_weight(100, 0, 0, 10, 0);
        let bad = calculate_brain_routing_weight(5000, 2, 3, 5, 5);
        assert!(
            good > bad,
            "gutes Brain ({good}) muss hoeher gewichtet werden als schlechtes ({bad})"
        );
    }

    #[test]
    fn routing_weight_ist_deterministisch() {
        let a = calculate_brain_routing_weight(500, 1, 1, 8, 2);
        let b = calculate_brain_routing_weight(500, 1, 1, 8, 2);
        assert_eq!(
            a, b,
            "identische Eingaben muessen identische Scores liefern"
        );
    }

    #[test]
    fn routing_weight_fehler_und_reparaturen_senken_den_score() {
        let reference = calculate_brain_routing_weight(1000, 0, 0, 10, 0);
        let flawed = calculate_brain_routing_weight(1000, 0, 2, 10, 5);
        assert!(
            flawed < reference,
            "fehlgeschlagene Tasks/Reparaturen muessen den Score senken"
        );
    }

    #[test]
    fn routing_weight_nullfall_liefert_gueltigen_score() {
        let score = calculate_brain_routing_weight(0, 0, 0, 0, 0);
        assert!(!score.is_nan());
        assert!(!score.is_infinite());
        assert!((0.0..=1.0).contains(&score));
    }

    #[test]
    fn routing_weight_extremwerte_ohne_overflow() {
        let score =
            calculate_brain_routing_weight(u64::MAX, u32::MAX, u32::MAX, u32::MAX, u32::MAX);
        assert!(!score.is_nan());
        assert!(!score.is_infinite());
        assert!((0.0..=1.0).contains(&score));
    }
}
