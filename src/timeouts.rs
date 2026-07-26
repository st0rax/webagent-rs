//! Timeout-Politik: bevorzugt GEMESSEN, nur notfalls geschaetzt.
//!
//! Die frueheren Werte waren durchweg Schaetzungen — feste Basiszeiten je
//! Operation und eine handgepflegte Multiplikatoren-Tabelle je Brain. Eine
//! Messung ueber 2072 Erfolgslaeufe (2026-07-26) zeigte, dass diese Tabelle in
//! BEIDE Richtungen danebenlag: claude war mit 1.8 hinterlegt, gemessen 0.9;
//! kimi mit 1.3, gemessen 2.2. Folge: bei den einen liefen Fehlschlaege
//! minutenlang aus, die anderen wurden mitten in der Arbeit abgeschnitten.
//!
//! Deshalb kommt der Brain-Anteil jetzt aus der tatsaechlichen p95-Latenz
//! erfolgreicher Aufrufe (`brain_score`). Die statische Tabelle dient nur noch
//! als Kaltstart, solange zu wenig Messwerte vorliegen. Jede Konstante ist per
//! Umgebungsvariable ueberschreibbar — nichts hier ist unveraenderlich.

use std::collections::HashMap;

const CHARS_PER_EXTRA_BLOCK: f64 = 500.0;
const SECONDS_PER_BLOCK: f64 = 15.0;
const MAX_MESSAGE_EXTRA: f64 = 180.0;

/// Compute timeout from operation, brain speed, and message size.
///
/// `override_timeout` > 0 sets a minimum (CLI `--timeout`); `None` or `0.0` = auto only.
pub fn resolve_timeout(
    operation: &str,
    brain_id: &str,
    message: &str,
    override_timeout: Option<f64>,
) -> f64 {
    resolve_from(
        operation,
        brain_id,
        message,
        override_timeout,
        measured_p95_cached(operation, brain_id),
    )
}

/// Kern der Berechnung mit EXPLIZIT uebergebenem Messwert.
///
/// Getrennt, damit Tests nicht von der Ereignisdatei der jeweiligen Maschine
/// abhaengen — genau das machte `resolve_timeout` sonst umgebungsabhaengig und
/// die Testaussage wertlos.
pub(crate) fn resolve_from(
    operation: &str,
    brain_id: &str,
    message: &str,
    override_timeout: Option<f64>,
    p95: Option<f64>,
) -> f64 {
    let base = match p95 {
        Some(p) => (p * env_float("WEBAGENT_TIMEOUT_P95_FACTOR", 1.6))
            .max(env_float("WEBAGENT_TIMEOUT_MEASURED_FLOOR", 30.0)),
        None => static_base(operation, brain_id),
    };
    resolve_with(operation, message, override_timeout, base)
}

/// p95 aus `brain_score`, einmal je Prozess eingelesen.
///
/// Ohne Cache laege bei JEDEM Timeout-Aufruf ein vollstaendiger Durchlauf der
/// Ereignisdatei (hier: 2435 Zeilen) — die Datei aendert sich waehrend eines
/// Laufs kaum, ein Snapshot beim ersten Zugriff genuegt.
fn measured_p95_cached(operation: &str, brain_id: &str) -> Option<f64> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    // `login` ist Wartezeit auf einen MENSCHEN, keine Modellantwort — die
    // Antwortlatenz sagt darueber nichts aus.
    if operation == "login" {
        return None;
    }
    static CACHE: OnceLock<Mutex<HashMap<String, Option<f64>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = brain_id.to_lowercase();
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(v) = guard.get(&key) {
        return *v;
    }
    let min_samples = env_float("WEBAGENT_TIMEOUT_MIN_SAMPLES", 20.0).max(1.0) as usize;
    let v = crate::brain_score::latency_p95_secs(&key, min_samples);
    guard.insert(key, v);
    v
}

fn resolve_with(_operation: &str, message: &str, override_timeout: Option<f64>, base: f64) -> f64 {
    let msg_extra = if message.is_empty() {
        0.0
    } else {
        let blocks = message.chars().count() as f64 / CHARS_PER_EXTRA_BLOCK;
        (blocks * SECONDS_PER_BLOCK).min(MAX_MESSAGE_EXTRA)
    };

    let mut computed = base + msg_extra;
    computed *= env_float("WEBAGENT_TIMEOUT_MULT", 1.0);

    let min_t = env_float("WEBAGENT_TIMEOUT_MIN", 30.0);
    let max_t = env_float("WEBAGENT_TIMEOUT_MAX", 600.0);
    computed = computed.max(min_t).min(max_t);

    if let Some(ovr) = override_timeout {
        if ovr > 0.0 {
            return computed.max(ovr.min(max_t));
        }
    }
    computed
}

/// Kaltstart-Schaetzung: Operationsbasis x Brain-Faktor. Nur noch gueltig,
/// solange fuer das Brain zu wenige Messwerte vorliegen.
fn static_base(operation: &str, brain_id: &str) -> f64 {
    let base = get_operation_base()
        .get(operation)
        .copied()
        .unwrap_or_else(|| env_float("WEBAGENT_TIMEOUT_BASE_DEFAULT", 90.0));
    let mult = get_brain_multipliers()
        .get(&brain_id.to_lowercase())
        .copied()
        .unwrap_or(1.0);
    base * mult
}

fn get_operation_base() -> HashMap<&'static str, f64> {
    let mut map = HashMap::new();
    // Startwerte, jeweils per Env ueberschreibbar — keine Zahl hier ist fix.
    map.insert(
        "ensure_ready",
        env_float("WEBAGENT_TIMEOUT_ENSURE_READY", 45.0),
    );
    map.insert(
        "wait_response",
        env_float("WEBAGENT_TIMEOUT_WAIT_RESPONSE", 90.0),
    );
    map.insert("relay", env_float("WEBAGENT_TIMEOUT_RELAY", 90.0));
    map.insert("login", env_float("WEBAGENT_TIMEOUT_LOGIN", 300.0));
    map
}

fn get_brain_multipliers() -> HashMap<String, f64> {
    let mut map = HashMap::new();
    map.insert("chatgpt".to_string(), 1.0);
    map.insert("deepseek".to_string(), 1.2);
    map.insert("kimi".to_string(), 1.3);
    map.insert("qwen".to_string(), 1.2);
    map.insert("zai".to_string(), 1.2);
    map.insert("gemini".to_string(), 1.5);
    map.insert("mistral".to_string(), 1.5);
    map.insert("claude".to_string(), 1.8);
    map
}

fn env_float(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // resolve_timeout liest prozess-globale Env-Vars (WEBAGENT_TIMEOUT_*). Da Rust
    // Tests parallel ausführt, würde test_env_multiplier sonst die anderen Tests
    // verfälschen. Alle env-empfindlichen Tests laufen daher über dieses Lock seriell.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_guard() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Kaltstart (keine Messwerte): die statische Tabelle gilt, dort ist
    /// claude langsamer hinterlegt.
    #[test]
    fn kaltstart_nutzt_die_statische_tabelle() {
        let _g = env_guard();
        let chatgpt = resolve_from("wait_response", "chatgpt", "hi", None, None);
        let claude = resolve_from("wait_response", "claude", "hi", None, None);
        assert!(claude > chatgpt, "statische Tabelle nicht angewandt");
    }

    /// Sobald Messwerte vorliegen, schlagen sie die Schaetzung — auch wenn sie
    /// ihr widersprechen. Genau das war der Befund: claude ist entgegen der
    /// Tabelle (1.8) nicht langsamer als chatgpt.
    #[test]
    fn messwerte_schlagen_die_schaetztabelle() {
        let _g = env_guard();
        let geschaetzt = resolve_from("wait_response", "claude", "hi", None, None);
        let gemessen = resolve_from("wait_response", "claude", "hi", None, Some(20.0));
        assert!(
            gemessen < geschaetzt,
            "Messwert wurde ignoriert: gemessen={gemessen} geschaetzt={geschaetzt}"
        );
        // Der Aufschlag auf die p95 muss drauf sein (20s * 1.6 = 32s),
        // aber der Boden von 30s darf nicht unterschritten werden.
        assert!(gemessen >= 30.0, "Untergrenze verletzt: {gemessen}");
    }

    #[test]
    fn test_long_message_increases_timeout() {
        let _g = env_guard();
        let short_msg = "x".repeat(100);
        let long_msg = "x".repeat(5000);
        let short = resolve_timeout("wait_response", "chatgpt", &short_msg, None);
        let long = resolve_timeout("wait_response", "chatgpt", &long_msg, None);
        assert!(long > short);
    }

    #[test]
    fn test_override_is_minimum() {
        let _g = env_guard();
        let auto = resolve_timeout("relay", "kimi", "test", None);
        let with_override = resolve_timeout("relay", "kimi", "test", Some(300.0));
        assert!(with_override >= 300.0);
        assert!(with_override >= auto);
    }

    #[test]
    fn test_env_multiplier() {
        let _g = env_guard();
        std::env::set_var("WEBAGENT_TIMEOUT_MULT", "2");
        let base = resolve_timeout("ensure_ready", "chatgpt", "", None);
        assert!(base >= 80.0); // 45.0 * 1.0 * 2.0 = 90.0, clamped by min 30.0
        std::env::remove_var("WEBAGENT_TIMEOUT_MULT");
    }
}
