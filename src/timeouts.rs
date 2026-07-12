//! Dynamic timeout policy — replaces flat 120s defaults.

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
    let operation_base = get_operation_base();
    let brain_multipliers = get_brain_multipliers();

    let base = operation_base.get(operation).copied().unwrap_or(90.0);
    let mult = brain_multipliers
        .get(&brain_id.to_lowercase())
        .copied()
        .unwrap_or(1.0);

    let msg_extra = if message.is_empty() {
        0.0
    } else {
        let blocks = message.chars().count() as f64 / CHARS_PER_EXTRA_BLOCK;
        (blocks * SECONDS_PER_BLOCK).min(MAX_MESSAGE_EXTRA)
    };

    let mut computed = (base + msg_extra) * mult;
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

fn get_operation_base() -> HashMap<&'static str, f64> {
    let mut map = HashMap::new();
    map.insert("ensure_ready", 45.0);
    map.insert("wait_response", 90.0);
    map.insert("relay", 90.0);
    map.insert("login", 300.0);
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

    #[test]
    fn test_chatgpt_shorter_than_claude() {
        let chatgpt = resolve_timeout("wait_response", "chatgpt", "hi", None);
        let claude = resolve_timeout("wait_response", "claude", "hi", None);
        assert!(claude > chatgpt);
    }

    #[test]
    fn test_long_message_increases_timeout() {
        let short_msg = "x".repeat(100);
        let long_msg = "x".repeat(5000);
        let short = resolve_timeout("wait_response", "chatgpt", &short_msg, None);
        let long = resolve_timeout("wait_response", "chatgpt", &long_msg, None);
        assert!(long > short);
    }

    #[test]
    fn test_override_is_minimum() {
        let auto = resolve_timeout("relay", "kimi", "test", None);
        let with_override = resolve_timeout("relay", "kimi", "test", Some(300.0));
        assert!(with_override >= 300.0);
        assert!(with_override >= auto);
    }

    #[test]
    fn test_env_multiplier() {
        std::env::set_var("WEBAGENT_TIMEOUT_MULT", "2");
        let base = resolve_timeout("ensure_ready", "chatgpt", "", None);
        assert!(base >= 80.0); // 45.0 * 1.0 * 2.0 = 90.0, clamped by min 30.0
        std::env::remove_var("WEBAGENT_TIMEOUT_MULT");
    }
}
