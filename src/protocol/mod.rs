//! Strukturiertes Aktionsprotokoll webagent/1.

mod parser;
mod text;
mod types;

pub use parser::parse;
pub use text::{
    format_observation, format_observations_bundle, format_protocol_error,
    format_protocol_error_for, is_possibly_truncated, should_abort_protocol_repair,
    should_attempt_protocol_repair,
};
pub use types::{Action, ActionType, EditOperation, PROTOCOL_VERSION};

#[cfg(test)]
mod tests {
    use super::parser::repair_unescaped_quotes_in_strings;
    use super::*;
    use super::text::PROTOCOL_REPAIR_MAX_FAILURES;
    use super::types::error_code;
    use serde_json::{json, Value};

    fn valid_envelope() -> Value {
        json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "s1", "type": "shell", "command": "Get-Location", "timeout_seconds": 30}
            ]
        })
    }

    #[test]
    fn test_parse_valid_envelope() {
        let result = parse(&serde_json::to_string(&valid_envelope()).unwrap());
        assert!(result.valid);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].id, "s1");
        assert_eq!(result.actions[0].command, "Get-Location");
    }

    #[test]
    fn valid_protocol_may_discuss_rate_limits() {
        let env = serde_json::json!({
            "protocol": "webagent/1",
            "actions": [{
                "id": "search-rate-limit",
                "type": "shell",
                "command": "rg \"rate limit\" src"
            }]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].command, "rg \"rate limit\" src");
    }

    #[test]
    fn plain_capacity_notice_is_still_classified() {
        let result = parse("Rate limit erreicht. Bitte erneut versuchen.");
        assert!(!result.valid);
        assert_eq!(result.error, "Model capacity / rate limit.");
    }

    #[test]
    fn technical_prose_that_mentions_rate_limits_is_not_a_capacity_notice() {
        let response = format!(
            "Die Implementierung sollte einen Windows-1252-Fallback enthalten. {} rate limit.",
            "Ausführliche technische Erläuterung mit Code und Diagnose. ".repeat(12)
        );
        let result = parse(&response);
        assert!(!result.valid);
        assert_ne!(result.error, "Model capacity / rate limit.");
        assert!(
            result.error.starts_with("Ungültiges JSON:"),
            "{}",
            result.error
        );
    }

    #[test]
    fn test_parse_edit_action() {
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "C:/tmp/a.txt",
                 "old_string": "alt", "new_string": "neu"}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].action_type, ActionType::Edit);
        assert_eq!(result.actions[0].path, "C:/tmp/a.txt");
        assert_eq!(result.actions[0].old_string, "alt");
        assert_eq!(result.actions[0].new_string, "neu");
    }

    #[test]
    fn test_parse_edit_requires_fields() {
        for bad in [
            json!({"id": "e1", "type": "edit", "old_string": "a", "new_string": "b"}),
            json!({"id": "e1", "type": "edit", "path": "x", "new_string": "b"}),
            json!({"id": "e1", "type": "edit", "path": "x", "old_string": "a", "new_string": "a"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {env}");
        }
    }

    #[test]
    fn test_parse_write_action() {
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "w1", "type": "write", "path": "C:/tmp/neu.txt", "content": "zeile1\nzeile2\n"}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].action_type, ActionType::Write);
        assert_eq!(result.actions[0].content, "zeile1\nzeile2\n");
        // write ohne content wird abgelehnt (leerer content-String ist ok).
        let env = json!({"protocol": "webagent/1", "actions": [
            {"id": "w2", "type": "write", "path": "C:/tmp/neu.txt"}
        ]});
        assert!(!parse(&serde_json::to_string(&env).unwrap()).valid);
    }

    #[test]
    fn test_raw_write_envelope() {
        // Rohformat: mehrzeiliger Code MIT Quotes, ohne JSON-Escaping.
        let text = "WEBAGENT/1 WRITE\nid: w-raw-1\npath: src/foo.rs\n---CONTENT---\n\
                    fn main() {\n    println!(\"Hallo \\\"Welt\\\"\");\n}\n---END CONTENT---";
        let result = parse(text);
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].action_type, ActionType::Write);
        assert_eq!(result.actions[0].path, "src/foo.rs");
        assert_eq!(
            result.actions[0].content,
            "fn main() {\n    println!(\"Hallo \\\"Welt\\\"\");\n}"
        );
    }

    #[test]
    fn test_raw_edit_envelope_preserves_whitespace() {
        // Rohformat edit: Einrückung im Anker bleibt erhalten (nicht getrimmt).
        // Kein Zeilenfortsetzungs-`\` verwenden — das würde die führenden Spaces fressen.
        let text = "WEBAGENT/1 EDIT\nid: e-raw-1\npath: src/lib.rs\n---OLD---\n    return a * b;\n---NEW---\n    return a + b;\n---END EDIT---";
        let result = parse(text);
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].action_type, ActionType::Edit);
        assert_eq!(result.actions[0].path, "src/lib.rs");
        assert_eq!(result.actions[0].old_string, "    return a * b;");
        assert_eq!(result.actions[0].new_string, "    return a + b;");
    }

    #[test]
    fn fenced_raw_edit_preserves_rust_generics() {
        let text = "```text\nWEBAGENT/1 EDIT\nid: e-generic-1\npath: src/lib.rs\n---OLD---\npub value: Option<u32>,\n---NEW---\npub value: Option<u64>,\n---END EDIT---\n```";
        let result = parse(text);
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions[0].old_string, "pub value: Option<u32>,");
        assert_eq!(result.actions[0].new_string, "pub value: Option<u64>,");
    }

    #[test]
    fn test_raw_message_envelope() {
        let text =
            "WEBAGENT/1 MESSAGE\nid: answer-1\ntext: Ergebnis fertig.\n\nAlle Tests sind gruen.";
        let result = parse(text);
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].action_type, ActionType::Message);
        assert_eq!(
            result.actions[0].text,
            "Ergebnis fertig.\n\nAlle Tests sind gruen."
        );
    }

    #[test]
    fn complete_message_envelope_is_not_truncated() {
        let text = "WEBAGENT/1 MESSAGE\nid: answer-2\ntext: Fertig und sofort verwertbar.";
        assert!(!is_possibly_truncated(text));
        assert!(parse(text).valid);
    }

    #[test]
    fn test_raw_edit_rejects_identical_and_empty() {
        let same =
            "WEBAGENT/1 EDIT\nid: e1\npath: a.rs\n---OLD---\nx\n---NEW---\nx\n---END EDIT---";
        assert!(!parse(same).valid, "identisch muss abgelehnt werden");
        let empty_path = "WEBAGENT/1 WRITE\nid: w1\npath:   \n---CONTENT---\nx\n---END CONTENT---";
        // path nur Whitespace -> Regex matcht nicht (path braucht ein Nicht-WS-Zeichen);
        // faellt auf JSON-Pfad zurueck und ist dort ungueltig.
        assert!(!parse(empty_path).valid);
    }

    #[test]
    fn test_raw_write_truncation_detected() {
        // Abgeschnittene Roh-Hülle (kein ---END CONTENT---) gilt als unvollständig.
        let partial = "WEBAGENT/1 WRITE\nid: w1\npath: a.rs\n---CONTENT---\nfn main() {";
        assert!(is_possibly_truncated(partial), "unvollständig erwartet");
        let complete =
            "WEBAGENT/1 WRITE\nid: w1\npath: a.rs\n---CONTENT---\nfn main() {}\n---END CONTENT---";
        assert!(!is_possibly_truncated(complete));
    }

    #[test]
    fn closed_but_invalid_raw_envelopes_are_not_treated_as_streams() {
        let legacy_message =
            "WEBAGENT/1 MESSAGE\nid: final\n---CONTENT---\nfertig\n---END MESSAGE---";
        assert!(!is_possibly_truncated(legacy_message));
        assert!(!parse(legacy_message).valid);

        let open_message = "WEBAGENT/1 MESSAGE\nid: final\n---CONTENT---\nfertig";
        assert!(is_possibly_truncated(open_message));

        let closed_bad_edit = "WEBAGENT/1 EDIT\nid: bad\npath: a.rs\n---OLD---\nx\n---END EDIT---";
        assert!(!is_possibly_truncated(closed_bad_edit));
        assert!(!parse(closed_bad_edit).valid);
    }

    #[test]
    fn test_edit_batches_with_shell() {
        // edit/shell dürfen gemischt in einer Antwort stehen (seriell ausgeführt).
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y"},
                {"id": "s1", "type": "shell", "command": "cargo test", "timeout_seconds": 600}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 2);
    }

    #[test]
    fn complete_raw_edit_batch_is_not_truncated() {
        let complete = "WEBAGENT/1 EDIT_BATCH\n\
id: batch-complete-1\n\
---EDIT---\n\
path: src/a.rs\n\
---OLD---\n\
old\n\
---NEW---\n\
new\n\
---END EDIT---\n\
---END BATCH---";
        assert!(!is_possibly_truncated(complete));
        assert!(parse(complete).valid);
    }

    #[test]
    fn test_parse_valid_markdown_block() {
        let text = format!(
            "```json\n{}\n```",
            serde_json::to_string(&valid_envelope()).unwrap()
        );
        let result = parse(&text);
        assert!(result.valid);
    }

    #[test]
    fn test_parse_raw_complex_powershell_envelope() {
        let text = r#"WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
$html = "<div style='color:red'>Hallo</div>"
Write-Output $html
---END SCRIPT---"#;
        let result = parse(text);
        assert!(result.valid);
        assert_eq!(result.actions[0].id, "report-1");
        assert!(result.actions[0].command.contains("style='color:red'"));
        assert_eq!(result.actions[0].timeout_seconds, 300.0);
    }

    #[test]
    fn test_parse_legacy_run_envelope_as_shell() {
        let raw = "WEBAGENT/1 RUN\nid: check-cargo\ncommand: \"cargo check --lib\"\n---END RUN---";
        let result = parse(raw);
        assert!(result.valid, "{:?}", result.error);
        assert_eq!(result.actions.len(), 1);
        assert_eq!(result.actions[0].action_type, ActionType::Shell);
        assert_eq!(result.actions[0].command, "cargo check --lib");
        assert_eq!(result.actions[0].timeout_seconds, 120.0);
    }

    #[test]
    fn test_complex_powershell_envelope_must_be_complete() {
        let partial = r#"WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
Write-Output "x""#;
        assert!(is_possibly_truncated(partial));
        assert!(!parse(partial).valid);
    }

    #[test]
    fn test_nested_raw_script_envelope_is_safely_unwrapped() {
        let envelope = r#"WEBAGENT/1 SHELL
id: nested-1
timeout_seconds: 120
---SCRIPT---
$html = @'
<h1>Quote "Test"</h1>
'@
Write-Output $html
---END SCRIPT---"#;
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "nested-1", "type": "shell", "command": envelope}]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert!(result.actions[0].command.starts_with("$html = @'"));
        assert!(!result.actions[0].command.contains("WEBAGENT/1 SHELL"));
        assert_eq!(result.actions[0].timeout_seconds, 120.0);
    }

    #[test]
    fn test_parse_rendered_json_code_block_label() {
        for label in &["JSON\n", "json\n", "Json\r\n"] {
            let text = format!(
                "{}{}",
                label,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&text);
            assert!(result.valid, "failed for label {:?}", label);
            assert_eq!(result.actions[0].id, "s1");
        }
    }

    #[test]
    fn test_parse_rendered_deepseek_code_controls() {
        for labels in &["json\nKopieren\nHerunterladen\n", "JSON\nCopy\nDownload\n"] {
            let text = format!(
                "{}{}",
                labels,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&text);
            assert!(result.valid, "failed for labels {:?}", labels);
        }
    }

    #[test]
    fn test_parse_qwen_rendered_line_numbers_and_nbsp() {
        let rendered = format!(
            "JSON\n1\n2\n3\n{}",
            serde_json::to_string(&valid_envelope())
                .unwrap()
                .replace(' ', "\u{00a0}")
        );
        let result = parse(&rendered);
        assert!(result.valid);
        assert_eq!(result.actions[0].id, "s1");
    }

    #[test]
    fn test_parse_rejects_leading_prose_before_json_controls() {
        for prefix in &["Denke nach…\n", "Thinking\n", "Hier ist\n"] {
            let rendered = format!(
                "{}JSON\nCopy\n{}",
                prefix,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&rendered);
            assert!(!result.valid, "dangerous prefix accepted: {:?}", prefix);
        }
    }

    #[test]
    fn test_parse_rejects_thought_process_and_embedded_json() {
        for bad_prefix in &[
            "Thought Process\n\n",
            "Thought Process\njson\n",
            "Thinking...\n\n",
            "Thought Process\n\nVerstanden! Hier das JSON:\n",
        ] {
            let rendered = format!(
                "{}{}",
                bad_prefix,
                serde_json::to_string(&valid_envelope()).unwrap()
            );
            let result = parse(&rendered);
            assert!(!result.valid, "dangerous prefix accepted: {:?}", bad_prefix);
        }
    }

    #[test]
    fn test_repair_unescaped_windows_path_for_message_only() {
        let rendered = r#"{"protocol":"webagent/1","actions":[{"id":"answer","type":"message","text":"Pfad: C:\Users\storax\Desktop\webagent"}]}"#;
        let result = parse(rendered);
        assert!(result.valid);
        assert!(result.actions[0]
            .text
            .ends_with(r"C:\Users\storax\Desktop\webagent"));
    }

    #[test]
    fn test_never_repair_unescaped_windows_path_for_shell() {
        let rendered = r#"{"protocol":"webagent/1","actions":[{"id":"work","type":"shell","command":"Get-Item C:\Users\storax"}]}"#;
        assert!(!parse(rendered).valid);
    }

    #[test]
    fn test_message_must_be_separate_after_shell() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "work", "type": "shell", "command": "Get-Date"},
                {"id": "answer", "type": "message", "text": "fertig"}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("eigene"));
    }

    #[test]
    fn test_reject_text_outside_block() {
        let text = format!(
            "Hier:\n```json\n{}\n```",
            serde_json::to_string(&valid_envelope()).unwrap()
        );
        let result = parse(&text);
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_raw_fallback() {
        let result = parse("Get-Date");
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_wrong_protocol() {
        let bad = json!({"protocol": "other/1", "actions": [{"id": "x", "type": "finish"}]});
        let result = parse(&serde_json::to_string(&bad).unwrap());
        assert!(!result.valid);
    }

    #[test]
    fn test_reject_missing_id() {
        let bad = json!({
            "protocol": "webagent/1",
            "actions": [{"type": "finish"}]
        });
        let result = parse(&serde_json::to_string(&bad).unwrap());
        assert!(!result.valid);
    }

    #[test]
    fn test_parse_finish() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "done", "type": "finish"}]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert_eq!(result.actions[0].action_type, ActionType::Finish);
    }

    #[test]
    fn test_finish_must_be_the_only_action() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "work", "type": "shell", "command": "Get-Date"},
                {"id": "done", "type": "finish"}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("einzige Action"));
    }

    #[test]
    fn test_detects_possibly_truncated_streamed_json() {
        for text in &[
            r#"{"pr"#,
            r#"{"protocol":"webagent/1","actions":[{"id":"x","type":"shell","command":"unterminated"#,
            r#"{"protocol":"webagent/1","actions":["#,
        ] {
            assert!(is_possibly_truncated(text), "failed for text {:?}", text);
        }
    }

    #[test]
    fn test_truncated_message_with_windows_path_is_not_released_early() {
        let text = r#"{"protocol":"webagent/1","actions":[{"id":"answer","type":"message","text":"Pfad C:\Users\storax"#;
        assert!(is_possibly_truncated(text));
    }

    #[test]
    fn test_does_not_mark_non_json_or_complete_json_as_truncated() {
        for text in &[
            "Denke nach…",
            "not json",
            &serde_json::to_string(&valid_envelope()).unwrap(),
        ] {
            assert!(!is_possibly_truncated(text), "failed for text {:?}", text);
        }
    }

    #[test]
    fn test_format_observation_includes_action_id() {
        let obs = format_observation("step-1", "hello", "", Some(0), false);
        assert!(obs.contains("action_id=step-1"));
        assert!(obs.contains("hello"));
    }

    #[test]
    fn test_shell_timeout_accepts_numeric_range() {
        let doc = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "low", "type": "shell", "command": "Get-Date", "timeout_seconds": 0.1},
                {"id": "high", "type": "shell", "command": "Get-Date", "timeout_seconds": 3600}
            ]
        });
        let result = parse(&serde_json::to_string(&doc).unwrap());
        assert!(result.valid);
        assert_eq!(
            result
                .actions
                .iter()
                .map(|a| a.timeout_seconds)
                .collect::<Vec<_>>(),
            vec![0.1, 3600.0]
        );
    }

    #[test]
    fn test_shell_timeout_rejects_invalid_values() {
        for timeout in &[json!(0), json!(-1), json!(3600.1), json!(true), json!("30")] {
            let doc = json!({
                "protocol": "webagent/1",
                "actions": [
                    {"id": "bad", "type": "shell", "command": "Get-Date", "timeout_seconds": timeout}
                ]
            });
            let result = parse(&serde_json::to_string(&doc).unwrap());
            assert!(!result.valid, "should reject timeout {:?}", timeout);
        }
    }

    #[test]
    fn test_parse_intentionally_broken_answers_are_invalid() {
        for bad in &[
            "",
            "nur Prosa ohne JSON",
            "{not json",
            r#"{"protocol":"webagent/2","actions":[{"id":"x","type":"finish"}]}"#,
            r#"{"protocol":"webagent/1","actions":[]}"#,
            r#"{"actions":[{"id":"x","type":"finish"}]}"#,
        ] {
            assert!(!parse(bad).valid, "expected invalid for {:?}", bad);
        }
    }

    #[test]
    fn test_protocol_repair_policy_two_repairs_then_abort() {
        assert!(!should_attempt_protocol_repair(0));
        assert!(should_attempt_protocol_repair(1));
        assert!(should_attempt_protocol_repair(2));
        assert!(!should_attempt_protocol_repair(3));
        assert!(!should_abort_protocol_repair(0));
        assert!(!should_abort_protocol_repair(1));
        assert!(!should_abort_protocol_repair(2));
        assert!(should_abort_protocol_repair(3));
        assert_eq!(PROTOCOL_REPAIR_MAX_FAILURES, 3);
    }

    #[test]
    fn test_reject_unknown_field_per_action_type() {
        // Je Action-type ein unbekanntes Feld → invalid.
        let cases = [
            json!({"id": "s1", "type": "shell", "command": "Get-Date", "foo": 1}),
            json!({"id": "m1", "type": "message", "text": "hi", "foo": 1}),
            json!({"id": "f1", "type": "finish", "foo": 1}),
            json!({"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y", "foo": 1}),
            json!({"id": "w1", "type": "write", "path": "a.txt", "content": "c", "foo": 1}),
        ];
        for bad in cases {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {bad}");
            assert!(
                result.error.contains("unbekanntes Feld") && result.error.contains("foo"),
                "unerwartete Fehlermeldung: {}",
                result.error
            );
        }
    }

    #[test]
    fn test_reject_typo_field_command() {
        // Klassischer Tippfehler: "comand" statt "command" darf nicht als leerer
        // Befehl durchrutschen, sondern muss als unbekanntes Feld auffliegen.
        let env = json!({
            "protocol": "webagent/1",
            "actions": [{"id": "s1", "type": "shell", "comand": "Get-Date"}]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(!result.valid);
        assert!(result.error.contains("comand"), "{}", result.error);
    }

    #[test]
    fn test_reject_cross_type_field() {
        // Feld existiert im Protokoll, aber nicht für diesen type (text bei shell,
        // command bei message, path bei finish).
        for bad in [
            json!({"id": "s1", "type": "shell", "command": "Get-Date", "text": "x"}),
            json!({"id": "m1", "type": "message", "text": "hi", "command": "Get-Date"}),
            json!({"id": "f1", "type": "finish", "path": "a.txt"}),
            json!({"id": "w1", "type": "write", "path": "a.txt", "content": "c", "old_string": "x"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(!result.valid, "haette abgelehnt werden muessen: {bad}");
            assert!(
                result.error.contains("unbekanntes Feld"),
                "{}",
                result.error
            );
        }
    }

    #[test]
    fn test_reject_whitespace_only_path_edit_write() {
        for bad in [
            json!({"id": "e1", "type": "edit", "path": "   ", "old_string": "x", "new_string": "y"}),
            json!({"id": "w1", "type": "write", "path": "  \t ", "content": "c"}),
        ] {
            let env = json!({"protocol": "webagent/1", "actions": [bad.clone()]});
            let result = parse(&serde_json::to_string(&env).unwrap());
            assert!(
                !result.valid,
                "leerer/whitespace path muss abgelehnt werden: {bad}"
            );
            assert!(result.error.contains("path"), "{}", result.error);
        }
    }

    #[test]
    fn test_allowed_fields_still_accepts_full_valid_actions() {
        // Gegenprobe: alle erlaubten Felder je type werden weiterhin akzeptiert.
        let env = json!({
            "protocol": "webagent/1",
            "actions": [
                {"id": "e1", "type": "edit", "path": "a.txt", "old_string": "x", "new_string": "y"},
                {"id": "s1", "type": "shell", "command": "Get-Date", "timeout_seconds": 30}
            ]
        });
        let result = parse(&serde_json::to_string(&env).unwrap());
        assert!(result.valid, "{}", result.error);
        assert_eq!(result.actions.len(), 2);
    }

    #[test]
    fn shell_befehl_mit_eigenen_quotes_wird_repariert_statt_verworfen() {
        // Regression 2026-07-29: wörtliche Antworten aus den Läufen
        // 20260729_205020_936b8bb0 und 20260729_205525_94d2066c. Beide waren
        // strukturell vollständig, wurden aber wegen innerer Anführungszeichen
        // und Regex-Escapes als `protocol_invalid` verworfen — und das Brain
        // wiederholte danach denselben Befehl, bis der Lauf stallte.
        let faelle = [
            r#"{"protocol":"webagent/1","actions":[{"id":"step-2","type":"shell","command":"Select-String -Path src/self_research.rs -Pattern "isolated_query|assess_command_risk|#\[cfg\(test\)\]" -Context 2,4","timeout_seconds":30}]}"#,
            r#"{"protocol":"webagent/1","actions":[{"id":"step-3","type":"shell","command":"Select-String -Path src/self_research.rs -Pattern "isolated_query|mod tests|fn test"","timeout_seconds":30}]}"#,
        ];
        for roh in faelle {
            let r = parse(roh);
            assert!(r.valid, "nicht geparst: {roh}\n → {:?}", r.error);
            assert_eq!(r.actions.len(), 1);
            let cmd = &r.actions[0].command;
            assert!(
                cmd.starts_with("Select-String -Path src/self_research.rs -Pattern "),
                "cmd={cmd}"
            );
            assert!(cmd.contains("isolated_query"), "cmd={cmd}");
        }
    }

    #[test]
    fn quote_repair_laesst_gueltiges_json_unangetastet() {
        assert_eq!(
            repair_unescaped_quotes_in_strings(
                r#"{"protocol":"webagent/1","actions":[{"id":"a","type":"finish"}]}"#
            ),
            None
        );
        // Leerer String-Wert ist ein echtes String-Ende, kein Inhalt.
        assert_eq!(
            repair_unescaped_quotes_in_strings(r#"{"a":"","b":1}"#),
            None
        );
        // Schon korrekt escapte Quotes bleiben, wie sie sind.
        assert_eq!(
            repair_unescaped_quotes_in_strings(r#"{"a":"sagt \"hallo\"","b":1}"#),
            None
        );
    }

    #[test]
    fn test_format_protocol_error_demands_valid_webagent_json_only() {
        let msg = format_protocol_error("Ungültiges JSON: trailing comma");
        assert!(msg.contains(PROTOCOL_VERSION));
        assert!(msg.contains("Interpreter"));
        assert!(msg.contains("Ungültiges JSON: trailing comma"));
        assert!(msg.contains("Tool-Anforderung"));
        assert!(!msg.contains("Get-Location"));
    }

    #[test]
    fn kaputtes_edit_json_bekommt_rohformat_statt_shell_beispiel() {
        let invalid =
            r#"{"actions":[{"type":"edit","path":"src/x.rs","new_string":"fn x() { "kaputt" }"}]}"#;
        let msg = format_protocol_error_for("ungueltiges JSON", invalid);
        assert!(msg.contains("WEBAGENT/1 EDIT"), "{msg}");
        assert!(msg.contains("---OLD---"), "{msg}");
        assert!(msg.contains("---NEW---"), "{msg}");
        assert!(!msg.contains("Get-Location"), "{msg}");
    }

    #[test]
    fn kaputtes_shell_json_bekommt_escape_freies_rohformat() {
        let invalid = r#"{"actions":[{"type":"shell","command":"cd "C:\repo" && cargo test"}]}"#;
        let msg = format_protocol_error_for("ungueltiges JSON", invalid);
        assert!(msg.contains("WEBAGENT/1 SHELL"), "{msg}");
        assert!(msg.contains("kein cd"), "{msg}");
        assert!(!msg.contains("Get-Location"), "{msg}");
    }

    #[test]
    fn test_error_code_unknown_field() {
        assert_eq!(error_code("unbekanntes Feld"), "unknown_field");
        assert_eq!(error_code("Unbekannt"), "unknown_field");
    }

    #[test]
    fn test_error_code_missing_field() {
        assert_eq!(error_code("braucht ein Feld"), "missing_field");
        assert_eq!(error_code("Braucht"), "missing_field");
    }

    #[test]
    fn test_error_code_protocol_mismatch() {
        assert_eq!(
            error_code("protocol muss webagent/1 sein"),
            "protocol_mismatch"
        );
        assert_eq!(error_code("Protocol muss"), "protocol_mismatch");
    }

    #[test]
    fn test_error_code_duplicate_id() {
        assert_eq!(error_code("doppelte id"), "duplicate_id");
        assert_eq!(error_code("Doppelte"), "duplicate_id");
    }

    #[test]
    fn test_error_code_empty() {
        assert_eq!(error_code("leerer Wert"), "empty");
        assert_eq!(error_code("Leer"), "empty");
    }

    #[test]
    fn test_error_code_identical_old_new() {
        assert_eq!(error_code("identisch"), "identical_old_new");
        assert_eq!(
            error_code("alte und neue Werte sind identisch"),
            "identical_old_new"
        );
    }

    #[test]
    fn test_error_code_default_invalid() {
        assert_eq!(error_code("sonstiger fehler"), "invalid");
        assert_eq!(error_code(""), "invalid");
        assert_eq!(error_code("irgendwas ganz anderes"), "invalid");
    }

    #[test]
    fn raw_edit_in_reasoning_preamble_is_never_executed() {
        let raw = concat!(
            "Architected adaptive brain weight function with multi-parameter normalization\n",
            "Architected adaptive brain weight function with multi-parameter normalization\n",
            "WEBAGENT/1 EDIT\n",
            "id: edit-1\n",
            "path: src/brain_score.rs\n",
            "---OLD---\n",
            "fn alt() {}\n",
            "---NEW---\n",
            "fn neu() {}\n",
            "---END EDIT---"
        );
        let r = parse(raw);
        assert!(
            !r.valid,
            "eingebetteter Beispiel-Edit darf nicht ausgeführt werden"
        );
    }

    #[test]
    fn raw_block_must_still_reach_the_end_of_the_message() {
        // Auch Nachgeplapper macht einen Rohblock ungültig.
        let raw = concat!(
            "WEBAGENT/1 EDIT\n",
            "id: e1\n",
            "path: src/x.rs\n",
            "---OLD---\n",
            "a\n",
            "---NEW---\n",
            "b\n",
            "---END EDIT---\n",
            "Soll ich das so machen?"
        );
        assert!(!parse(raw).valid);
    }

    #[test]
    fn rendered_code_block_with_plain_label_is_stripped() {
        // kimi lieferte abwechselnd ein Label "JSON" (ging durch) und "plain"
        // (scheiterte) — nur das Sprachlabel
        // des gerenderten Code-Blocks wechselte.
        let raw = concat!(
            "plain\n",
            "Kopieren\n",
            r#"{"protocol":"webagent/1","actions":[{"id":"s1","type":"shell","command":"echo hi"}]}"#
        );
        let r = parse(raw);
        assert!(r.valid, "sollte gueltig sein, war: {}", r.error);
        assert_eq!(r.actions[0].id, "s1");
    }

    #[test]
    fn the_previously_working_json_label_still_works() {
        // Regressionsschutz: die Variante, die vorher schon durchging.
        let raw = concat!(
            "JSON\n",
            "Kopieren\n",
            r#"{"protocol":"webagent/1","actions":[{"id":"s2","type":"shell","command":"echo hi"}]}"#
        );
        assert!(parse(raw).valid);
    }

    #[test]
    fn prose_alone_is_still_rejected() {
        // Der Marker-Zuschnitt darf nicht dazu fuehren, dass Geschwaetz ohne
        // Block ploetzlich als gueltig gilt.
        assert!(
            !parse("Ich habe darueber nachgedacht und wuerde vorschlagen, die Datei zu aendern.")
                .valid
        );
    }
}
