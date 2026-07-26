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
    resolve_with(
        operation,
        message,
        override_timeout,
        measured_or_static(operation, brain_id),
    )
}

/// Brain-Anteil: gemessene p95 falls vorhanden, sonst der Kaltstart-Schaetzwert.
///
/// Liefert die *Basiszeit in Sekunden* fuer diese Operation, nicht mehr einen
/// dimensionslosen Faktor — die Messung ist eine Zeit, und eine Zeit soll auch
/// eine bleiben statt in einen Faktor zurueckgerechnet zu werden.
fn measured_or_static(operation: &str, brain_id: &str) -> f64 {
    let statisch = static_base(operation, brain_id);
    // `login` ist Wartezeit auf einen MENSCHEN, keine Modellantwort — die
    // Antwortlatenz sagt darueber nichts aus.
    if operation == "login" {
        return statisch;
    }
    let min_samples = env_float("WEBAGENT_TIMEOUT_MIN_SAMPLES", 20.0).max(1.0) as usize;
    match crate::brain_score::latency_p95_secs(&brain_id.to_lowercase(), min_samples) {
        Some(p95) => {
            // Sicherheitsaufschlag auf die p95: die restlichen 5 Prozent
            // duerfen nicht systematisch abgeschnitten werden.
            let faktor = env_float("WEBAGENT_TIMEOUT_P95_FACTOR", 1.6);
            (p95 * faktor).max(env_float("WEBAGENT_TIMEOUT_MEASURED_FLOOR", 30.0))
        }
        None => statisch,
    }
}

fn resolve_with(
    _operation: &str,
    message: &str,
    override_timeout: Option<f64>,
    base: f64,
) -> f64 {
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
    map.insert("ensure_ready", env_float("WEBAGENT_TIMEOUT_ENSURE_READY", 45.0));
    map.insert("wait_response", env_float("WEBAGENT_TIMEOUT_WAIT_RESPONSE", 90.0));
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

    #[test]
    fn test_chatgpt_shorter_than_claude() {
        let _g = env_guard();
        let chatgpt = resolve_timeout("wait_response", "chatgpt", "hi", None);
        let claude = resolve_timeout("wait_response", "claude", "hi", None);
        assert!(claude > chatgpt);
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
