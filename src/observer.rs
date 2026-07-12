//! observer — Zuverlässige Antworterkennung (plattformreine Teile).
//!
//! Die DOM-abhängige `ResponseObserver.wait_for_response`-Logik wird später
//! als Trait am Browser-Rand implementiert.

use regex::Regex;
use std::sync::OnceLock;

/// Regex für transiente UI-Status-Labels (Denke nach, Thinking, etc.).
fn transient_status_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:denke\s+nach|thinking(?:\s*\.\.\.)?(?:\s+skip)?|reasoning|ueberlege|überlege|思考|generating|loading|skip|thought\s*process)\s*[.….]*$"
        )
        .unwrap()
    })
}

/// Regex für Thinking-Präfixe, die aus Antworten entfernt werden sollen.
fn thinking_prefix_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)^(?:Thought\s*Process|Thinking\.{0,3}|Reasoning|Verstanden!?|Ich sehe das Problem)[\s:.…\n]*"
        )
        .unwrap()
    })
}

/// Regex für reine Zeitanzeigen (z.B. "11:05").
fn clock_only_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\d{1,2}:\d{2}$").unwrap()
    })
}

/// Regex für Claude-Limit-Meldungen.
fn claude_limit_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)(?:usage\s+limit|message\s+limit|rate\s+limit|nachrichtenlimit|limit\s+reached|too\s+many\s+messages|quota\s+exceeded|keine\s+kostenlosen|free\s+messages?\s+(?:used|left|remaining)|you\s+have\s+reached|daily\s+limit|conversation\s+limit|out\s+of\s+(?:free\s+)?messages|send\s+limit)"
        )
        .unwrap()
    })
}

/// True für UI-Fortschritts-Labels, die keine echten Modellantworten sind.
pub fn is_transient_response_text(text: &str) -> bool {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    
    if normalized.is_empty() {
        return true;
    }
    
    if clock_only_regex().is_match(normalized) {
        return true;
    }
    
    transient_status_regex().is_match(normalized)
}

/// True wenn Claude Web UI eine Usage/Rate-Limit-Banner statt einer Antwort zeigt.
pub fn is_claude_limit_response_text(text: &str) -> bool {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let normalized = normalized.trim();
    
    if normalized.is_empty() {
        return false;
    }
    
    claude_limit_regex().is_match(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_progress_labels_are_not_final_responses() {
        let cases = vec![
            "",
            "   ",
            "Denke nach…",
            "Denke nach...",
            "Thinking…",
            "Thinking...",
            "Thinking... Skip",
            "11:05",
            "9:32",
            "reasoning",
            "Überlege …",
            "Skip",
            "skip",
            "Thought Process",
            "thought process",
        ];
        
        for text in cases {
            assert!(
                is_transient_response_text(text),
                "Expected '{}' to be transient",
                text
            );
        }
    }

    #[test]
    fn test_real_response_text_is_not_transient() {
        let cases = vec![
            r#"{"protocol":"webagent/1","actions":[]}"#,
            "Thinking\nHere is the actual answer",
            "Denke nach: Ergebnis liegt vor",
            "Eine normale Antwort",
        ];
        
        for text in cases {
            assert!(
                !is_transient_response_text(text),
                "Expected '{}' to NOT be transient",
                text
            );
        }
    }

    #[test]
    fn test_claude_limit_messages_are_detected() {
        let cases = vec![
            "You have reached your usage limit for today.",
            "Message limit reached. Try again later.",
            "Nachrichtenlimit erreicht",
            "Rate limit exceeded",
        ];
        
        for text in cases {
            assert!(
                is_claude_limit_response_text(text),
                "Expected '{}' to be a Claude limit message",
                text
            );
        }
    }

    #[test]
    fn test_non_limit_responses_are_not_claude_limits() {
        let cases = vec![
            "METHODOLOGY:C REASON:Balanced mix.",
            "The counter is 8.",
            "Thinking\nHere is the actual answer",
        ];
        
        for text in cases {
            assert!(
                !is_claude_limit_response_text(text),
                "Expected '{}' to NOT be a Claude limit message",
                text
            );
        }
    }
}
