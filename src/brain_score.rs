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
use crate::scoring::wilson_lower_bound;

/// Wie viele der letzten Ereignisse pro Brain in den Score einfliessen. Aeltere
/// Ereignisse bleiben im Log (Historie), zaehlen aber nicht mehr fuer den
/// aktuellen Score -- das ist die "Recency"-Komponente ohne Decay-Formel.
const WINDOW_SIZE: usize = 40;

/// Maximale Laenge des Banner-Textes im Turn-Record (PII-Schutz).
const BANNER_TRUNCATE: usize = 200;

// ---------------------------------------------------------------------------
// T-936: Phase und Beobachtung je Turn
// ---------------------------------------------------------------------------

/// Sendephase, die ein Browser-Turn erreicht hat. Die Fuenf-Zustaende aus dem
/// Objective werden durch Phase + Beobachtungen (element_w/h, clamp, pasted vs
/// expected, banner) in den Daten unterscheidbar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SendPhase {
    #[default]
    SelectorResolve,
    Focus,
    Clear,
    Insert,
    ContentCheck,
    Submit,
    Settle,
    Read,
}

impl SendPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelectorResolve => "selector_resolve",
            Self::Focus => "focus",
            Self::Clear => "clear",
            Self::Insert => "insert",
            Self::ContentCheck => "content_check",
            Self::Submit => "submit",
            Self::Settle => "settle",
            Self::Read => "read",
        }
    }
}

/// Laufzeit-Beobachtungen eines Sendevorgangs. Wird pro Turn in die
/// events.jsonl geschrieben und ermoeglicht die Unterscheidung der
/// Fuenf-Zustaende (nicht angemeldet, ausgeloggt, kein Selektor getroffen,
/// Klick daneben, Fuellen gelaufen).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnObservation {
    pub phase: SendPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_w: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_h: Option<f64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub clamp_triggered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_arrived: Option<bool>,
    /// T-937: wie der Fokus zustande kam — "keyboard" (Tab), "el_focus"
    /// (In-Page focus(), tabindex=-1-faehig), "click" (letzter Rueckfall)
    /// oder "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_method: Option<String>,
    /// T-937: verbrauchte Tab-Drucke (keyboard) / Rueckfallstufe sonst.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_tries: Option<u32>,
    /// T-937: Zeichen fuer "der Composer ist per Tab nie erreichbar (tabindex=-1),
    /// el_focus hat geholfen" — muss von "Fokus kam nie an" unterscheidbar sein.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tabindex_fallback: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pasted_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_chars: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

// Beobachtungs-Puffer: send_generic (send.rs) traegt den aktuellen Stand ein,
// record_event_at (unten) konsumiert ihn beim Schreiben.
lazy_static! {
    static ref PENDING_TURN: std::sync::Mutex<Option<TurnObservation>> =
        std::sync::Mutex::new(None);
}

/// Aktuelle Turn-Beobachtung eintragen (von send.rs / composer.rs).
pub fn set_pending_turn(obs: TurnObservation) {
    *PENDING_TURN.lock().unwrap() = Some(obs);
}

/// Einzelne Felder der laufenden Turn-Beobachtung ergaenzen/aktualisieren,
/// ohne den Rest zu ueberschreiben. Legt eine leere Beobachtung an, wenn noch
/// keine vorhanden ist.
pub fn update_pending_turn(update: impl FnOnce(&mut TurnObservation)) {
    let mut guard = PENDING_TURN.lock().unwrap();
    update(guard.get_or_insert_with(TurnObservation::default));
}

/// Kopie des aktuellen Stands (send.rs nutzt sie nach dem Fuell-Loop, um
/// ContentCheck vs. Insert zu unterscheiden).
pub fn pending_turn_snapshot() -> Option<TurnObservation> {
    PENDING_TURN.lock().unwrap().clone()
}

/// Standardisierte Fehlernachricht mit Phase-Präfix statt eines kombinierten
/// Sammelstrings. Der alte Substring ("Composer-Feld nicht gefunden") bleibt
/// im Detailteil erhalten, damit bestehende externe Erkennung (is_send_disabled
/// etc.) weiter funktioniert.
pub fn phase_error(phase: SendPhase, detail: &str) -> String {
    format!("send_phase={}: {}", phase.as_str(), detail)
}

/// Banner-Text fuer den Turn-Record: gekuerzt auf BANNER_TRUNCATE Zeichen,
/// getrimmt — kein PII-/Langtext-Risiko in events.jsonl.
pub fn turn_banner(banner: &str) -> String {
    banner.trim().chars().take(BANNER_TRUNCATE).collect()
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    turn: Option<TurnObservation>,
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
    let turn = PENDING_TURN.lock().unwrap().take();
    let event = Event {
        brain_id: brain_id.to_string(),
        ts: crate::now_rfc3339(),
        success,
        reason: reason.map(str::to_string),
        latency_ms,
        prompt_chars,
        turn,
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

    // -----------------------------------------------------------------------
    // T-936: Turn-Beobachtungen
    // -----------------------------------------------------------------------

    /// Signatur eines Zustands in den Daten: Phase + welche Beobachtungen
    /// gesetzt sind. Die DoD-Fuenf-Zustaende muessen sich paarweise dadurch
    /// unterscheiden.
    fn signature(obs: &TurnObservation) -> (String, bool, bool, bool, Option<bool>, bool, bool) {
        (
            obs.phase.as_str().to_string(),
            obs.element_w.is_some(),
            obs.element_h.is_some(),
            obs.clamp_triggered,
            obs.focus_arrived,
            obs.pasted_chars == obs.expected_chars,
            obs.banner.is_some(),
        )
    }

    #[test]
    fn turn_fuenf_zustaende_sind_in_den_daten_unterscheidbar() {
        // 1. Nicht angemeldet: Composer Insert erreicht, aber Login-Banner zeigt.
        let nicht_angemeldet = TurnObservation {
            phase: SendPhase::Insert,
            banner: Some("Bitte melde dich an".to_string()),
            pasted_chars: Some(0),
            expected_chars: Some(12),
            ..Default::default()
        };
        // 2. Ausgeloggt mit Reauth: Composition abgebrochen, Reauth-Banner da.
        let ausgeloggt_reauth = TurnObservation {
            phase: SendPhase::SelectorResolve,
            banner: Some("session expired".to_string()),
            ..Default::default()
        };
        // 3. Kein Selektor getroffen: kein Element lokalisiert (keine Masse),
        //    nichts eingefuegt.
        let kein_selektor = TurnObservation {
            phase: SendPhase::Insert,
            element_w: None,
            pasted_chars: Some(0),
            expected_chars: Some(77),
            ..Default::default()
        };
        // 4. Klick neben dem Element: Elementmerkmale da, Feld blieb aber leer.
        let klick_daneben = TurnObservation {
            phase: SendPhase::ContentCheck,
            element_w: Some(640.0),
            element_h: Some(48.0),
            clamp_triggered: true,
            focus_arrived: Some(true),
            pasted_chars: Some(0),
            expected_chars: Some(43),
            ..Default::default()
        };
        // 5. Fuellen in Deadline gelaufen: teilweise eingefuegt, Deadline verfehlt.
        let deadline_past = TurnObservation {
            phase: SendPhase::Insert,
            element_w: Some(640.0),
            pasted_chars: Some(240),
            expected_chars: Some(4000),
            clamp_triggered: true,
            ..Default::default()
        };
        let states = [
            (&nicht_angemeldet, "nicht_angemeldet"),
            (&ausgeloggt_reauth, "ausgeloggt_reauth"),
            (&kein_selektor, "kein_selektor"),
            (&klick_daneben, "klick_daneben"),
            (&deadline_past, "deadline_past"),
        ];
        for (i, (a, a_name)) in states.iter().enumerate() {
            for (b, b_name) in states.iter().skip(i + 1) {
                assert_ne!(
                    signature(a),
                    signature(b),
                    "{a_name} und {b_name} muessen in den Daten unterscheidbar sein"
                );
            }
        }
    }

    #[test]
    fn record_event_at_konsumiert_pending_turn() {
        let path = unique_path();
        set_pending_turn(TurnObservation {
            phase: SendPhase::Settle,
            selector_index: Some(3),
            element_w: Some(640.0),
            element_h: Some(48.0),
            pasted_chars: Some(4000),
            expected_chars: Some(4000),
            ..Default::default()
        });
        record_event_at("kimi", false, Some("blocked"), 1500, 4000, &path);
        // Puffer ist konsumiert: der naechste Ereignis ohne neuen Pending hat
        // keinen turn-Anteil (kein Stale-Data-Anhaengen an Folgeturns).
        record_event_at("kimi", false, Some("blocked"), 1500, 4000, &path);
        let events = load_events(&path);
        assert_eq!(events.len(), 2);
        let first = events[0].turn.as_ref().expect("turn-Beobachtung fehlt");
        assert_eq!(first.phase, SendPhase::Settle);
        assert_eq!(first.selector_index, Some(3));
        assert_eq!(first.element_w, Some(640.0));
        assert_eq!(first.element_h, Some(48.0));
        assert_eq!(first.pasted_chars, Some(4000));
        assert!(events[1].turn.is_none(), "ohne Pending kein turn im Event");
    }

    #[test]
    fn phase_error_nennt_phase_und_behaelt_detail() {
        let msg = phase_error(SendPhase::Insert, "Composer-Feld nicht gefunden (Timeout)");
        assert!(
            msg.starts_with("send_phase=insert: "),
            "unbekanntes Präfix: {msg}"
        );
        assert!(
            msg.contains("Composer-Feld nicht gefunden"),
            "alter Substring muss fuer externe Erkennung erhalten bleiben"
        );
        for phase in [
            SendPhase::SelectorResolve,
            SendPhase::Focus,
            SendPhase::Clear,
            SendPhase::Insert,
            SendPhase::ContentCheck,
            SendPhase::Submit,
            SendPhase::Settle,
            SendPhase::Read,
        ] {
            assert!(
                phase_error(phase, "x").starts_with(&format!("send_phase={}: ", phase.as_str()))
            );
        }
    }

    #[test]
    fn turn_banner_kuerzt_auf_200_zeichen() {
        let lang = format!("  {}  ", "x".repeat(500));
        let kurz = turn_banner(&lang);
        assert_eq!(kurz.chars().count(), BANNER_TRUNCATE);
        assert_eq!(kurz.chars().next(), Some('x'));
        assert_eq!(turn_banner(""), "");
    }
}
