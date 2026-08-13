//! Resume-Logik: Wiederherstellung einer Browser-Chat-Session nach
//! Unterbrechung (incomplete Response / conversation_ref-Restore).

use std::collections::HashMap;
use std::time::Duration;

use crate::brain::BrainBackend;
use crate::executor::ShellExecutor;
use crate::prompts::{
    resume_continue_prompt, resume_continue_prompt_with, resume_recovery_prompt,
    resume_recovery_prompt_with_instruction,
};
use crate::transcript::Transcript;

use super::{AgentController, BrainTurn};

const INCOMPLETE_RETRY_PROMPT: &str =
    "[Controller] Die letzte Web-Antwort war unvollständig oder leer. \
     Setze mit einer gültigen webagent/1-Antwort fort. \
     Wenn die Aufgabe abgeschlossen ist, sende eine message-Action.";

fn incomplete_retry_prompt(last_text: &str) -> String {
    let trimmed = last_text.trim_end();
    if trimmed.matches("WEBAGENT/1 ").count() > 1 {
        return "[Interpreter] Die letzte Antwort enthält mehrere aneinandergehängte \
                WEBAGENT/1-Rohdokumente. Rohformat erlaubt genau eine Action. Bündele \
                Dateiänderungen in genau einem WEBAGENT/1 EDIT_BATCH mit korrektem \
                ---END EDIT--- je Block und abschließendem ---END BATCH---. Sende \
                abhängige Shell-Checks erst nach der Batch-Observation in einer neuen Antwort."
            .to_string();
    }
    let missing = [
        ("WEBAGENT/1 SHELL", "---END SCRIPT---"),
        ("WEBAGENT/1 WRITE", "---END CONTENT---"),
        ("WEBAGENT/1 EDIT_BATCH", "---END BATCH---"),
        ("WEBAGENT/1 EDIT", "---END EDIT---"),
    ]
    .into_iter()
    .find(|(prefix, marker)| trimmed.starts_with(prefix) && !trimmed.ends_with(marker));
    match missing {
        Some((_, marker)) => format!(
            "[Interpreter] Die letzte Tool-Anforderung ist fast vollständig, aber der exakte \
             Abschlussmarker `{marker}` fehlt. Sende dieselbe Action mit einer neuen eindeutigen \
             ID erneut und beende sie exakt mit `{marker}`."
        ),
        None => INCOMPLETE_RETRY_PROMPT.to_string(),
    }
}

const RESUME_TRANSCRIPT_CHAR_BUDGET: usize = 8_000;

/// Exponential-Backoff-Basis/-Obergrenze zwischen incomplete-Retries.
const INCOMPLETE_RETRY_BACKOFF_BASE_MS: u64 = 500;
const INCOMPLETE_RETRY_BACKOFF_CAP_MS: u64 = 8_000;

/// Backoff-Dauer vor dem `retry_index`-ten incomplete-Retry (1-basiert):
/// `min(BASE * 2^(retry_index-1), CAP)`. Reine, überlauf­sichere Funktion, damit
/// wiederholte incomplete-Antworten nicht sofort neu gefeuert werden. `retry_index`
/// 0 wird wie 1 behandelt (BASE).
pub(crate) fn incomplete_retry_backoff(retry_index: usize) -> Duration {
    let exp = retry_index.saturating_sub(1).min(32) as u32;
    let scaled = INCOMPLETE_RETRY_BACKOFF_BASE_MS
        .checked_shl(exp)
        .unwrap_or(INCOMPLETE_RETRY_BACKOFF_CAP_MS)
        .min(INCOMPLETE_RETRY_BACKOFF_CAP_MS);
    Duration::from_millis(scaled)
}

/// Hosts that must never be treated as a live chat session for resume.
/// Includes reserved/test TLDs and classic documentation placeholders so a
/// leaked mock `conversation_ref` cannot short-circuit a real run (phantom finish).
fn is_blocked_resume_host(host: &str) -> bool {
    let h = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return true;
    }
    // Explicit documentation / mock hosts seen in fixtures and failed runs.
    if matches!(
        h.as_str(),
        "example.test"
            | "example.com"
            | "example.org"
            | "example.net"
            | "localhost"
            | "127.0.0.1"
            | "0.0.0.0"
            | "::1"
            | "[::1]"
            | "test"
            | "invalid"
            | "local"
    ) {
        return true;
    }
    // RFC 2606 / special-use and common mock TLDs.
    h.ends_with(".test")
        || h.ends_with(".invalid")
        || h.ends_with(".localhost")
        || h.ends_with(".example")
        || h.ends_with(".local")
}

pub(crate) fn is_valid_resume_conversation_ref(reference: &str) -> bool {
    let reference = reference.trim();
    if reference.is_empty() {
        return false;
    }
    let lower = reference.to_ascii_lowercase();
    if !(lower.starts_with("https://") || lower.starts_with("http://")) {
        return false;
    }
    // Strip scheme, then take authority (before path/query/fragment).
    let without_scheme = reference
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(reference);
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .trim();
    if authority.is_empty() {
        return false;
    }
    // Drop userinfo if present; host is before optional :port (IPv6 in []).
    let hostport = authority.rsplit('@').next().unwrap_or(authority);
    let host = if hostport.starts_with('[') {
        hostport
            .split(']')
            .next()
            .unwrap_or(hostport)
            .trim_start_matches('[')
    } else {
        hostport.split(':').next().unwrap_or(hostport)
    };
    if host.is_empty() || is_blocked_resume_host(host) {
        return false;
    }
    true
}

impl<B: BrainBackend, E: ShellExecutor> AgentController<B, E> {
    /// Versucht Recovery nach incomplete Response.
    pub(crate) fn recover_from_incomplete(
        &mut self,
        transcript: &mut Transcript,
        context: &str,
        last_text: &str,
    ) -> Option<BrainTurn> {
        self.incomplete_retries += 1;
        let _ = transcript.append(
            "system",
            &format!(
                "brain_incomplete_retry={}/{} context={}",
                self.incomplete_retries,
                Self::MAX_INCOMPLETE_RETRIES,
                context
            ),
            HashMap::new(),
        );

        if let Some(meta) = &self.meta {
            let _ = self.run_store.save(meta);
        }

        if self.incomplete_retries > Self::MAX_INCOMPLETE_RETRIES {
            return None;
        }

        // Exponential-Backoff (gedeckelt) statt sofortigem Neufeuern.
        std::thread::sleep(incomplete_retry_backoff(self.incomplete_retries));
        Some(self.run_once(&incomplete_retry_prompt(last_text), Some(transcript)))
    }

    /// Resume: Initial Turn (restore oder fallback).
    pub(crate) fn resume_initial_turn(
        &mut self,
        transcript: &mut Transcript,
        instruction: Option<&str>,
    ) -> BrainTurn {
        // Benoetigte Werte vorab kopieren, damit kein langlebiger &self.meta-Borrow
        // die spaeteren &mut self-Aufrufe (run_once) blockiert.
        let conv_ref = self.meta.as_ref().unwrap().conversation_ref.clone();
        let task = self.meta.as_ref().unwrap().task.clone();
        let mut restored = false;

        if let Some(cr) = conv_ref.as_ref() {
            if !is_valid_resume_conversation_ref(cr) {
                // Mock/placeholder refs (e.g. https://example.test/...) must not
                // look like a successful restore — that produced phantom done runs.
                let _ = transcript.append(
                    "system",
                    &format!(
                        "resume_invalid_conversation_ref={}; forcing new_chat fallback",
                        cr
                    ),
                    HashMap::new(),
                );
            } else {
                restored = self.brain.restore_conversation(cr).unwrap_or(false);
            }
        }

        let ready_timeout =
            crate::timeouts::resolve_timeout("ensure_ready", self.brain.brain_id(), "", None);
        if restored
            && self.brain.ensure_ready(ready_timeout).ok()
                == Some(crate::brain::SessionState::Ready)
        {
            let _ = transcript.append(
                "system",
                &format!(
                    "resume_restored conversation_ref={}",
                    conv_ref.as_ref().unwrap()
                ),
                HashMap::new(),
            );
            let prompt =
                instruction.map_or_else(resume_continue_prompt, resume_continue_prompt_with);
            let restored_turn = self.run_once(&prompt, Some(transcript));
            if restored_turn.complete {
                return restored_turn;
            }
            let _ = transcript.append(
                "system",
                "resume_restored_unresponsive; falling back to new chat",
                HashMap::new(),
            );
        }

        let _ = self.brain.new_chat();
        let tail = transcript
            .recovery_tail(RESUME_TRANSCRIPT_CHAR_BUDGET)
            .unwrap_or_default();
        let _ = transcript.append(
            "system",
            "resume_fallback=new_chat+transcript",
            HashMap::new(),
        );
        let prompt = instruction.map_or_else(
            || resume_recovery_prompt(&task, &tail),
            |current| resume_recovery_prompt_with_instruction(&task, &tail, current),
        );
        self.run_once(&prompt, Some(transcript))
    }
}

#[cfg(test)]
mod incomplete_prompt_tests {
    use super::incomplete_retry_prompt;

    #[test]
    fn names_missing_shell_end_marker_exactly() {
        let text = "WEBAGENT/1 SHELL\nid: inspect-1\n---SCRIPT---\nGet-Content a.rs\n---END SCRIPT";
        let prompt = incomplete_retry_prompt(text);
        assert!(prompt.contains("---END SCRIPT---"));
        assert!(prompt.contains("neuen eindeutigen ID"));
    }

    #[test]
    fn keeps_generic_repair_for_empty_response() {
        let prompt = incomplete_retry_prompt("");
        assert!(prompt.contains("unvollständig oder leer"));
    }

    #[test]
    fn explains_multiple_raw_actions_as_one_edit_batch() {
        let text = "WEBAGENT/1 EDIT\nid: a\n---END EDIT---\n\nWEBAGENT/1 SHELL\nid: b";
        let prompt = incomplete_retry_prompt(text);
        assert!(prompt.contains("EDIT_BATCH"));
        assert!(prompt.contains("abhängige Shell-Checks"));
    }
}
