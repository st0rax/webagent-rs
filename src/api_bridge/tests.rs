//! API-Bridge-Tests, aus der Produktionsdatei herausgeloest.
//!
//! # Modulgrenze
//!
//! Diese Datei ist der zukuenftige `#[cfg(test)] mod tests` von
//! `src/api_bridge.rs`. T-913 verdrahtet `mod tests` und entfernt die
//! Testsektion aus der Root-Datei. Keine Produktivlogik, keine geloeschten
//! Assertions. Gliederung nur als Abschnittskommentare, damit T-913 ohne
//! Umbenennung einhaengen kann.
//!
//! Abschnitte: Auth/Fehler, Routing/Streaming, Katalog/Auto-Router,
//! Prompt/Tools/Protokoll, Store/Lifecycle, Transport/HTTP, SDK-Blackbox.

use super::*;
use std::io::{Read, Write};

// --- Prompt / Content -------------------------------------------------------

#[test]
fn accepts_plain_and_block_text() {
    assert_eq!(text_content(&json!("Hallo")).unwrap(), "Hallo");
    assert_eq!(
        text_content(&json!([
            {"type": "text", "text": "Hal"},
            {"type": "text", "text": "lo"}
        ]))
        .unwrap(),
        "Hallo"
    );
}

#[test]
fn single_user_message_is_forwarded_without_wrapper_or_transport_prompt() {
    let task = conversation_task(
        None,
        &[ConversationMessage {
            role: "user".to_string(),
            content: json!("Hallo"),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
    )
    .unwrap();

    assert_eq!(task, "Hallo");
    assert!(!task.contains("Provider-Bridge"));
    assert!(!task.contains("WEBAGENT_INFERENCE/1"));
}

#[test]
fn extracts_openai_image_and_audio_parts_for_browser_upload() {
    let request = OpenAiRequest {
        model: "webagent/chatgpt".to_string(),
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        messages: vec![ConversationMessage {
            role: "user".to_string(),
            content: json!([
                {"type":"text","text":"Beschreibe die Dateien:"},
                {"type":"image_url","image_url":{"url":"data:image/png;base64,AQI="}},
                {"type":"input_audio","input_audio":{"data":"SGk=","format":"wav"}}
            ]),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
    };
    let prompt = openai_prompt(&request).unwrap();
    assert!(prompt
        .text
        .contains("[image attachment: image/png, 2 bytes]"));
    assert!(prompt
        .text
        .contains("[audio attachment: audio/wav, 2 bytes]"));
    assert_eq!(prompt.attachments.len(), 2);
    assert_eq!(prompt.attachments[0].file_name, "image-1.png");
    assert_eq!(prompt.attachments[0].data, vec![1, 2]);
    assert_eq!(prompt.attachments[1].file_name, "audio-2.wav");
    assert_eq!(prompt.attachments[1].data, b"Hi");
}

#[test]
fn extracts_anthropic_base64_image_part() {
    let content = json!([{
        "type":"image",
        "source":{"type":"base64","media_type":"image/jpeg","data":"AQI="}
    }]);
    let mut attachments = Vec::new();
    let text = content_to_prompt(&content, &mut attachments).unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0].mime_type, "image/jpeg");
    assert_eq!(attachments[0].data, vec![1, 2]);
    assert!(text.contains("image attachment"));
}

#[test]
fn remote_media_and_invalid_base64_fail_closed() {
    let mut attachments = Vec::new();
    let error = content_to_prompt(
        &json!([{"type":"image_url","image_url":"https://example.invalid/x.png"}]),
        &mut attachments,
    )
    .unwrap_err();
    assert!(error.contains("data-URL"));
    assert!(decode_base64("not-base64!").is_err());
    assert!(decode_base64("a===").is_err());
}

#[test]
fn response_input_items_keep_multimodal_content_shape() {
    let messages = vec![ConversationMessage {
        role: "user".to_string(),
        content: json!([
            {"type":"input_text","text":"Sehen"},
            {"type":"input_image","image_url":"data:image/png;base64,AQI="}
        ]),
        tool_calls: Vec::new(),
        tool_call_id: None,
    }];
    let items = response_input_items(&messages);
    assert_eq!(items[0]["content"][0]["type"], "input_text");
    assert_eq!(items[0]["content"][1]["type"], "input_image");
    assert_eq!(
        items[0]["content"][1]["image_url"],
        "data:image/png;base64,AQI="
    );
}

#[test]
fn rejects_non_text_content() {
    let error = text_content(&json!([{"type": "image", "source": {}}])).unwrap_err();
    assert!(error.contains("nicht unterstuetzt"));
}

// --- Katalog / Auto-Router --------------------------------------------------

#[test]
fn model_resolution_routes_each_available_brain() {
    assert_eq!(resolve_model("webagent", "chatgpt").unwrap(), "chatgpt");
    assert_eq!(resolve_model("webagent/auto", "chatgpt").unwrap(), "auto");
    assert_eq!(resolve_model("wa/chatgpt", "chatgpt").unwrap(), "chatgpt");
    assert_eq!(resolve_model("auto", "chatgpt").unwrap(), "auto");
    assert_eq!(resolve_model("chatgpt", "claude").unwrap(), "chatgpt");
    assert_eq!(
        resolve_model("webagent/claude", "chatgpt").unwrap(),
        "claude"
    );
    assert!(resolve_model("gpt-5", "chatgpt").is_err());
    assert!(resolve_model("webagent/not-configured", "chatgpt").is_err());
}

#[test]
fn model_catalog_contains_all_builtin_brains() {
    let brains = available_brains();
    for expected in [
        "chatgpt", "claude", "deepseek", "gemini", "kimi", "mistral", "qwen", "zai",
    ] {
        assert!(brains.contains(&expected.to_string()), "{expected} fehlt");
    }
}

#[test]
fn model_catalog_is_conservative_about_unverified_media_inputs() {
    assert_eq!(advertised_input_modalities("chatgpt"), ["text", "image"]);
    assert_eq!(advertised_input_modalities("claude"), ["text", "image"]);
    // Live bestätigt (docs/CURRENT_WORK.md): gemini Bild+Audio, deepseek/
    // kimi/mistral Bild. qwen/zai/perplexity haben keinen Media-Smoke.
    assert_eq!(
        advertised_input_modalities("gemini"),
        ["text", "image", "audio"]
    );
    for brain in ["deepseek", "kimi", "mistral"] {
        assert_eq!(
            advertised_input_modalities(brain),
            ["text", "image"],
            "{brain} Bild-Input ist live bestätigt"
        );
    }
    for brain in ["qwen", "zai", "perplexity"] {
        assert_eq!(
            advertised_input_modalities(brain),
            ["text"],
            "{brain} darf Medien nicht als belegt melden"
        );
    }
}

#[test]
fn model_output_modalities_are_advertised_correctly() {
    assert_eq!(advertised_output_modalities("auto"), ["text", "image"]);
    assert_eq!(advertised_output_modalities("chatgpt"), ["text", "image"]);
    for brain in [
        "claude",
        "deepseek",
        "gemini",
        "kimi",
        "mistral",
        "qwen",
        "zai",
        "perplexity",
    ] {
        assert_eq!(
            advertised_output_modalities(brain),
            ["text"],
            "{brain} hat keine verifizierte Bild-Generation"
        );
    }
}

#[test]
fn model_metadata_has_stable_catalog_shape() {
    let auto = model_metadata("auto");
    assert_eq!(auto["id"], "webagent/auto");
    assert_eq!(auto["virtual"], true);
    assert_eq!(auto["routing"]["audio"], "gemini");
    assert_eq!(auto["modalities"]["input"][2], "audio");
    let meta = model_metadata("chatgpt");
    assert_eq!(meta["id"], "webagent/chatgpt");
    assert_eq!(meta["context_window"], 128000);
    assert_eq!(meta["max_tokens"], 16384);
    assert_eq!(meta["modalities"]["output"][1], "image");
    let meta = model_metadata("kimi");
    assert_eq!(meta["modalities"]["input"][1], "image");
    assert_eq!(meta["modalities"]["output"][0], "text");
    assert!(meta["modalities"]["output"].as_array().unwrap().len() == 1);
}

#[test]
fn audio_capability_refusals_are_not_returned_as_transcripts() {
    assert!(is_audio_capability_refusal(
        "Ich kann die Audiodatei in dieser Umgebung nicht zuverlässig transkribieren."
    ));
    assert!(is_audio_capability_refusal(
        "I'm unable to transcribe audio files in this chat."
    ));
    assert!(is_audio_capability_refusal(
        "I don't have native audio transcription in this interface."
    ));
    assert!(!is_audio_capability_refusal(
        "Die Prüfziffer lautet sieben acht neun."
    ));
    assert!(!is_audio_capability_refusal(
        "The transcript contains a clearly spoken number."
    ));
}

#[test]
fn auto_router_classifies_requests_deterministically() {
    use crate::browser_inference::{BrowserAttachment, BrowserAttachmentKind};

    let audio = BrowserAttachment {
        kind: BrowserAttachmentKind::Audio,
        file_name: "sample.wav".to_string(),
        mime_type: "audio/wav".to_string(),
        data: vec![1, 2, 3],
    };
    let image = BrowserAttachment {
        kind: BrowserAttachmentKind::Image,
        file_name: "sample.png".to_string(),
        mime_type: "image/png".to_string(),
        data: vec![4, 5, 6],
    };

    assert_eq!(
        classify_auto_route("anything", &[], false, AutoPurpose::ImageGeneration),
        AutoRoute::ImageGeneration
    );
    assert_eq!(
        classify_auto_route("debug Rust code", &[audio], true, AutoPurpose::Chat),
        AutoRoute::AudioInput,
        "media routing must outrank tools and text heuristics"
    );
    assert_eq!(
        classify_auto_route("debug Rust code", &[image], true, AutoPurpose::Chat),
        AutoRoute::ImageInput
    );
    assert_eq!(
        classify_auto_route("debug Rust code", &[], true, AutoPurpose::Chat),
        AutoRoute::Tools
    );
    assert_eq!(
        classify_auto_route(
            "Please debug this Rust function",
            &[],
            false,
            AutoPurpose::Chat
        ),
        AutoRoute::Coding
    );
    assert_eq!(
        classify_auto_route(
            "Suche aktuelle Quellen im Internet",
            &[],
            false,
            AutoPurpose::Chat
        ),
        AutoRoute::CurrentResearch
    );
    assert_eq!(
        classify_auto_route("Sag einfach hallo", &[], false, AutoPurpose::Chat),
        AutoRoute::Default
    );
}

#[test]
fn auto_attach_timeout_error_names_routed_brain() {
    let msg = annotate_auto_routed_inference_error(
        true,
        "gemini",
        "keine Antwort erhalten (timeout_budget=90s, backend_status=idle, generation_complete=false, raw_chars=0)",
        Duration::from_secs(91),
    );
    assert!(
        msg.starts_with("auto_attach_timeout: routed=gemini after 91s"),
        "got {msg}"
    );
    assert!(msg.contains("keine Antwort erhalten"), "got {msg}");
}

#[test]
fn auto_attach_upload_error_adds_via_route() {
    let msg = annotate_auto_routed_inference_error(
        true,
        "chatgpt",
        "Browseroberflaeche stellt keinen nutzbaren Datei-Upload bereit (no_file_input)",
        Duration::from_secs(2),
    );
    assert!(msg.contains("no_file_input"), "got {msg}");
    assert!(msg.contains("via=auto→chatgpt"), "got {msg}");

    let partial = annotate_auto_routed_inference_error(
        true,
        "qwen",
        "Browseroberflaeche hat nur 0 von 1 Dateien uebernommen",
        Duration::from_millis(500),
    );
    assert!(partial.contains("0 von 1"), "got {partial}");
    assert!(partial.contains("via=auto→qwen"), "got {partial}");
}

#[test]
fn non_auto_errors_stay_unannotated() {
    let msg = annotate_auto_routed_inference_error(
        false,
        "gemini",
        "no_file_input",
        Duration::from_secs(1),
    );
    assert_eq!(msg, "Browser-Inference fehlgeschlagen: no_file_input");
    assert!(!msg.contains("via=auto"));
}

#[test]
fn auto_selection_follows_preference_order_and_skips_locked() {
    let available: Vec<String> = ["chatgpt", "claude", "gemini"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let no_locks = |_: &str| true;
    assert_eq!(
        first_available_auto_brain_in(&["claude", "chatgpt"], &available, no_locks).as_deref(),
        Some("claude"),
        "erste verfuegbare Praeferenz gewinnt"
    );
    assert_eq!(
        first_available_auto_brain_in(&["claude", "chatgpt", "gemini"], &available, |brain| {
            brain != "claude" && brain != "chatgpt"
        })
        .as_deref(),
        Some("gemini"),
        "gesperrte Brains uebersprungen"
    );
    assert_eq!(
        first_available_auto_brain_in(&["perplexity", "deepseek"], &available, no_locks,),
        None,
        "keine Praeferenz verfuegbar → None"
    );
}

#[test]
fn rejects_unsupported_roles_before_browser_execution() {
    let request = OpenAiRequest {
        model: "webagent".to_string(),
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        messages: vec![ConversationMessage {
            role: "invalid".to_string(),
            content: json!("unsafe"),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
    };
    assert!(openai_task(&request)
        .unwrap_err()
        .contains("Rolle 'invalid'"));
}

#[test]
fn plain_inference_prompt_does_not_request_webagent_actions() {
    let request = OpenAiRequest {
        model: "webagent".to_string(),
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        messages: vec![ConversationMessage {
            role: "user".to_string(),
            content: json!("Hallo"),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
    };

    let prompt = openai_task(&request).unwrap();
    assert_eq!(prompt, "Hallo");
    assert!(!prompt.contains("WEBAGENT_INFERENCE/1"));
    assert!(!prompt.contains("final-"));
}

// --- Tools / Protokoll ------------------------------------------------------

#[test]
fn openai_tools_and_choice_are_normalized() {
    let request = OpenAiRequest {
        model: "webagent".to_string(),
        stream: None,
        tools: vec![OpenAiTool {
            kind: "function".to_string(),
            function: OpenAiFunction {
                name: "read_file".to_string(),
                description: Some("Datei lesen".to_string()),
                parameters: json!({"type": "object"}),
            },
        }],
        tool_choice: Some(json!("required")),
        messages: vec![ConversationMessage {
            role: "user".to_string(),
            content: json!("Lies eine Datei"),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }],
    };

    let tools = openai_tools(&request.tools).unwrap();
    let choice = openai_tool_choice(request.tool_choice.as_ref(), &tools).unwrap();
    assert_eq!(tools[0].name, "read_file");
    assert_eq!(
        choice,
        crate::browser_inference::BrowserToolChoice::Required
    );
    assert!(require_clean_text_tools(&tools, &choice).is_err());
    assert!(
        require_clean_text_tools(&tools, &crate::browser_inference::BrowserToolChoice::None)
            .is_ok()
    );
    assert!(openai_tool_choice(
        Some(&json!({"type":"function","function":{"name":"missing"}})),
        &tools
    )
    .is_err());
}

#[test]
fn tool_result_is_replayed_as_visible_browser_context() {
    let request = OpenAiRequest {
        model: "webagent".to_string(),
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        messages: vec![ConversationMessage {
            role: "tool".to_string(),
            content: json!("Dateiinhalt"),
            tool_calls: Vec::new(),
            tool_call_id: Some("call_1".to_string()),
        }],
    };

    assert_eq!(openai_task(&request).unwrap(), "Dateiinhalt");
}

#[test]
fn pi_system_null_assistant_and_tool_error_roundtrip() {
    let request: OpenAiRequest = serde_json::from_value(json!({
        "model":"chatgpt", "messages":[
            {"role":"system","content":"Du bist ein Coding-Agent."},
            {"role":"user","content":"Lies missing.txt"},
            {"role":"assistant","content":null,"tool_calls":[
                {"id":"call_1","type":"function","function":{"name":"read","arguments":"{\"path\":\"missing.txt\"}"}}
            ]},
            {"role":"tool","tool_call_id":"call_1","content":{"error":"ENOENT"}}
        ]
    })).unwrap();
    let prompt = openai_prompt(&request).unwrap();
    assert!(prompt.text.contains("Coding-Agent"));
    assert!(prompt.text.contains("call_1"));
    assert!(prompt.text.contains("ENOENT"));
    assert!(prompt.text.contains("missing.txt"));
}

#[test]
fn browser_tool_call_maps_to_openai_response() {
    let answer = crate::browser_inference::BrowserInferenceResponse {
        text: None,
        tool_calls: vec![crate::browser_inference::BrowserToolCall {
            id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "README.md"}),
        }],
    };

    let message = openai_message(&answer);
    assert!(message["content"].is_null());
    assert_eq!(message["tool_calls"][0]["id"], "call_1");
    assert_eq!(message["tool_calls"][0]["function"]["name"], "read_file");
    assert_eq!(answer.finish_reason(), "tool_calls");
}

#[test]
fn responses_task_accepts_string_and_message_input() {
    let string_request = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!("Hallo"),
        instructions: Some("Antworte kurz.".to_string()),
        stream: Some(false),
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    let prompt = responses_task(&string_request).unwrap();
    assert!(prompt.contains("Antworte kurz."));
    assert!(prompt.ends_with("Hallo"));

    let message_request = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!([{"role":"user","content":"Ping"}]),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    assert_eq!(responses_task(&message_request).unwrap(), "Ping");

    let block_request = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!([{"role":"user","content":[{"type":"input_text","text":"Teil 1"},{"type":"input_text","text":" Teil 2"}]}]),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    assert_eq!(responses_task(&block_request).unwrap(), "Teil 1 Teil 2");
    assert!(responses_content(
        &json!([{"type":"input_image","image_url":"data:image/png;base64,SGk="}])
    )
    .is_ok());
    let remote_image_request = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!([{"role":"user","content":[{"type":"input_image","image_url":"https://example.invalid/x"}]}]),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    assert!(responses_task(&remote_image_request).is_err());
}

#[test]
fn responses_renderer_emits_completion_contract() {
    let response = response_object("resp_test", "webagent/chatgpt", "OK");
    assert_eq!(response["object"], "response");
    assert_eq!(response["status"], "completed");
    assert_eq!(response["output"][0]["type"], "message");
    assert_eq!(response["output"][0]["content"][0]["text"], "OK");

    let answer = crate::browser_inference::BrowserInferenceResponse {
        text: Some("OK".to_string()),
        tool_calls: Vec::new(),
    };
    let sse =
        String::from_utf8(responses_sse("resp_test", "webagent/chatgpt", &answer).body).unwrap();
    assert!(sse.contains("response.created"));
    assert!(sse.contains("response.output_text.delta"));
    assert!(sse.contains("response.completed"));
    assert!(
        sse.find("event: response.output_item.added").unwrap()
            < sse.find("event: response.content_part.added").unwrap()
    );
    assert!(
        sse.find("event: response.content_part.added").unwrap()
            < sse.find("event: response.output_text.delta").unwrap()
    );
    assert_eq!(response["usage"], Value::Null);
    assert_eq!(response["previous_response_id"], Value::Null);
    let seqs = sse_sequence_numbers(&sse);
    assert!(!seqs.is_empty());
    assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());
}

fn sse_sequence_numbers(sse: &str) -> Vec<u64> {
    sse.split("\n\n")
        .filter_map(|frame| {
            let data = frame.lines().find_map(|l| l.strip_prefix("data: "))?;
            serde_json::from_str::<Value>(data)
                .ok()?
                .get("sequence_number")?
                .as_u64()
        })
        .collect()
}

#[test]
fn responses_sse_sequence_numbers_sind_monoton_und_lueckenlos() {
    let answer = crate::browser_inference::BrowserInferenceResponse {
        text: Some("OK".to_string()),
        tool_calls: Vec::new(),
    };
    let sse =
        String::from_utf8(responses_sse("resp_seq", "webagent/chatgpt", &answer).body).unwrap();
    let seqs = sse_sequence_numbers(&sse);
    assert!(seqs.len() >= 8, "zu wenige Events: {seqs:?}");
    assert_eq!(seqs, (0..seqs.len() as u64).collect::<Vec<_>>());
    assert!(sse.contains("\"sequence_number\":0"));
    let tool_answer = crate::browser_inference::BrowserInferenceResponse {
        text: None,
        tool_calls: vec![crate::browser_inference::BrowserToolCall {
            id: "call_1".into(),
            name: "read".into(),
            arguments: json!({}),
        }],
    };
    let tool_sse =
        String::from_utf8(responses_sse("resp_tool", "webagent/chatgpt", &tool_answer).body)
            .unwrap();
    let tool_seqs = sse_sequence_numbers(&tool_sse);
    assert_eq!(tool_seqs, (0..tool_seqs.len() as u64).collect::<Vec<_>>());
}

#[test]
fn responses_tools_and_choice_follow_responses_shape() {
    let tools = responses_tools(&[json!({
        "type": "function",
        "name": "read_file",
        "description": "Read a file",
        "parameters": {"type": "object"}
    })])
    .unwrap();
    assert_eq!(tools[0].name, "read_file");
    assert_eq!(tools[0].description.as_deref(), Some("Read a file"));
    assert_eq!(
        responses_tool_choice(Some(&json!({"type":"function","name":"read_file"})), &tools)
            .unwrap(),
        crate::browser_inference::BrowserToolChoice::Function("read_file".to_string())
    );
    assert_eq!(
        responses_tool_choice(Some(&json!("required")), &tools).unwrap(),
        crate::browser_inference::BrowserToolChoice::Required
    );
    assert!(responses_tools(&[json!({"type":"computer"})]).is_err());
    assert!(responses_tool_choice(Some(&json!("read_file")), &tools).is_err());
}

#[test]
fn anthropic_tools_normalizes_input_schema() {
    let tools = anthropic_tools(&[json!({
        "name": "read_file",
        "description": "Datei lesen",
        "input_schema": {"type": "object", "properties": {"path": {"type": "string"}}}
    })])
    .unwrap();
    assert_eq!(tools[0].name, "read_file");
    assert_eq!(tools[0].description.as_deref(), Some("Datei lesen"));
    assert_eq!(tools[0].parameters["type"], "object");
    assert_eq!(tools[0].parameters["properties"]["path"]["type"], "string");

    let fallback =
        anthropic_tools(&[json!({"name": "search", "parameters": {"type": "object"}})]).unwrap();
    assert_eq!(fallback[0].name, "search");
    assert_eq!(fallback[0].parameters["type"], "object");

    let empty_schema = anthropic_tools(&[json!({"name": "noop"})]).unwrap();
    assert_eq!(empty_schema[0].parameters, json!({}));
    assert!(anthropic_tools(&[json!({})]).is_err());
    assert!(anthropic_tools(&[json!({"name": ""})]).is_err());
    assert!(anthropic_tools(&[json!("not-an-object")]).is_err());
}

#[test]
fn anthropic_tool_choice_maps_anthropic_forms() {
    let tools = anthropic_tools(&[json!({
        "name": "read_file",
        "input_schema": {"type": "object"}
    })])
    .unwrap();
    assert_eq!(
        anthropic_tool_choice(Some(&json!({"type": "auto"})), &tools).unwrap(),
        crate::browser_inference::BrowserToolChoice::Auto
    );
    assert_eq!(
        anthropic_tool_choice(Some(&json!({"type": "any"})), &tools).unwrap(),
        crate::browser_inference::BrowserToolChoice::Required
    );
    assert_eq!(
        anthropic_tool_choice(Some(&json!({"type": "tool", "name": "read_file"})), &tools).unwrap(),
        crate::browser_inference::BrowserToolChoice::Function("read_file".to_string())
    );
    assert_eq!(
        anthropic_tool_choice(None, &tools).unwrap(),
        crate::browser_inference::BrowserToolChoice::Auto
    );
    assert_eq!(
        anthropic_tool_choice(None, &[]).unwrap(),
        crate::browser_inference::BrowserToolChoice::None
    );
    assert!(
        anthropic_tool_choice(Some(&json!({"type": "tool", "name": "missing"})), &tools).is_err()
    );
    assert!(anthropic_tool_choice(Some(&json!("auto")), &tools).is_err());
    assert!(anthropic_tool_choice(Some(&json!({"type": "unknown"})), &tools).is_err());
    assert!(anthropic_tool_choice(Some(&json!({})), &tools).is_err());
}

#[test]
fn anthropic_response_renders_tool_use_blocks() {
    let answer = crate::browser_inference::BrowserInferenceResponse {
        text: None,
        tool_calls: vec![crate::browser_inference::BrowserToolCall {
            id: "toolu_1".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path": "README.md"}),
        }],
    };
    let response = anthropic_response("msg_1", "claude-3-5-sonnet", &answer);
    assert_eq!(response["type"], "message");
    assert_eq!(response["stop_reason"], "tool_use");
    assert_eq!(response["content"][0]["type"], "tool_use");
    assert_eq!(response["content"][0]["id"], "toolu_1");
    assert_eq!(response["content"][0]["name"], "read_file");
    assert_eq!(response["content"][0]["input"]["path"], "README.md");

    let text_answer = crate::browser_inference::BrowserInferenceResponse {
        text: Some("Hallo".to_string()),
        tool_calls: Vec::new(),
    };
    let text_response = anthropic_response("msg_2", "claude-3-5-sonnet", &text_answer);
    assert_eq!(text_response["stop_reason"], "end_turn");
    assert_eq!(text_response["content"][0]["type"], "text");
    assert_eq!(text_response["content"][0]["text"], "Hallo");
}

#[test]
fn responses_task_accepts_function_call_transcript() {
    let request = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!([{"type":"function_call_output","call_id":"call_7","output":{"ok":true}}]),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    assert!(responses_task(&request).unwrap().contains("ok"));

    let continuation = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!([{
            "type":"function_call",
            "call_id":"call_7",
            "name":"read_file",
            "arguments":{"path":"README.md"}
        }, {
            "type":"function_call_output",
            "call_id":"call_7",
            "output":[{"type":"input_text","text":"Dateiinhalt"}]
        }]),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: None,
        store: true,
    };
    assert!(responses_task(&continuation)
        .unwrap()
        .contains("Dateiinhalt"));
}

#[test]
fn responses_tool_call_sse_uses_output_item_events() {
    let answer = crate::browser_inference::BrowserInferenceResponse {
        text: None,
        tool_calls: vec![crate::browser_inference::BrowserToolCall {
            id: "call_8".to_string(),
            name: "read_file".to_string(),
            arguments: json!({"path":"README.md"}),
        }],
    };
    let sse =
        String::from_utf8(responses_sse("resp_tool", "webagent/chatgpt", &answer).body).unwrap();
    assert!(sse.contains("response.output_item.added"));
    assert!(sse.contains("response.function_call_arguments.delta"));
    assert!(sse.contains("response.function_call_arguments.done"));
    assert!(sse.contains("response.output_item.done"));
    assert!(!sse.contains("response.output_text.delta"));
}

#[test]
fn responses_state_is_stored_and_can_extend_context() {
    let id = "resp_state_contract".to_string();
    let mut messages = responses_messages(&json!("Mein Codewort ist Otter.")).unwrap();
    append_response_message(
        &mut messages,
        &crate::browser_inference::BrowserInferenceResponse {
            text: Some("Verstanden.".to_string()),
            tool_calls: Vec::new(),
        },
    );
    let response = response_with_state(
        response_object("resp_state_contract", "webagent/chatgpt", "Verstanden."),
        None,
    );
    store_response(
        "tenant-state",
        id.clone(),
        StoredResponse {
            response: response.clone(),
            messages,
        },
    );

    let mut stored = retrieve_response("tenant-state", &id).unwrap();
    stored
        .messages
        .extend(responses_messages(&json!("Wie lautet es?")).unwrap());
    let task = conversation_task(None, &stored.messages).unwrap();
    assert!(task.starts_with(
        "Bisheriger Verlauf deiner Unterhaltung. Beitraege unter [du] hast du selbst geschrieben."
    ));
    assert!(task.contains("[user]\nMein Codewort ist Otter."));
    assert!(task.contains("[du]\nVerstanden."));
    assert!(!task.contains("[brain]"));
    assert!(task.ends_with("Aktuelle Nachricht an dich:\n\nWie lautet es?"));
    assert!(response["previous_response_id"].is_null());
}

#[test]
fn responses_request_defaults_to_stored() {
    let request: ResponsesRequest = serde_json::from_value(json!({
        "model": "webagent/chatgpt",
        "input": "Hallo"
    }))
    .unwrap();
    assert!(request.store);
    assert!(request.previous_response_id.is_none());

    let chained = response_with_state(
        response_object("resp_child", "webagent/chatgpt", "OK"),
        Some("resp_parent"),
    );
    assert_eq!(chained["previous_response_id"], "resp_parent");
}

// --- Routing / Streaming / Store --------------------------------------------

#[test]
fn incremental_route_is_only_used_for_text_streams() {
    assert!(is_incremental_text_request(
        br#"{"model":"webagent/chatgpt","input":"x","stream":true}"#
    ));
    assert!(!is_incremental_text_request(
        br#"{"model":"webagent/chatgpt","input":"x","stream":false}"#
    ));
    assert!(!is_incremental_text_request(
        br#"{"model":"webagent/chatgpt","input":"x","stream":true,"tools":[{"type":"function","name":"f"}]}"#
    ));
    assert!(is_incremental_text_request(
        br#"{"model":"webagent/chatgpt","input":"x","stream":true,"tools":[{"type":"function","name":"f"}],"tool_choice":"none"}"#
    ));
}

#[test]
fn response_retrieval_requires_auth_and_returns_stored_object() {
    let id = "resp_retrieve_contract";
    store_response(
        &tenant_id("test-secret"),
        id.to_string(),
        StoredResponse {
            response: response_with_state(
                response_object(id, "webagent/chatgpt", "Gespeichert"),
                None,
            ),
            messages: Vec::new(),
        },
    );
    let config = BridgeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        brain: "chatgpt".to_string(),
        timeout_secs: None,
        headless: true,
        api_key: "test-secret".to_string(),
        fake_reply: None,
    };
    let authorized = HttpRequest {
        method: "GET".to_string(),
        path: format!("/v1/responses/{id}"),
        query: String::new(),
        headers: BTreeMap::from([(
            "authorization".to_string(),
            "Bearer test-secret".to_string(),
        )]),
        body: Vec::new(),
    };
    let response = handle_response_retrieve(&authorized, &config, &authorized.path);
    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["id"], id);
    assert_eq!(body["output_text"], "Gespeichert");

    let unauthorized = HttpRequest {
        headers: BTreeMap::new(),
        ..authorized
    };
    assert_eq!(
        handle_response_retrieve(&unauthorized, &config, &unauthorized.path).status,
        401
    );
}

#[test]
fn response_input_items_and_delete_follow_lifecycle_contract() {
    let messages = vec![
        ConversationMessage {
            role: "user".to_string(),
            content: json!("Hallo"),
            tool_calls: Vec::new(),
            tool_call_id: None,
        },
        ConversationMessage {
            role: "assistant".to_string(),
            content: Value::Null,
            tool_calls: vec![OpenAiAssistantToolCall {
                id: "call_lifecycle".to_string(),
                kind: "function".to_string(),
                function: OpenAiAssistantFunction {
                    name: "read_file".to_string(),
                    arguments: "{\"path\":\"README.md\"}".to_string(),
                },
            }],
            tool_call_id: None,
        },
        ConversationMessage {
            role: "tool".to_string(),
            content: json!("Inhalt"),
            tool_calls: Vec::new(),
            tool_call_id: Some("call_lifecycle".to_string()),
        },
    ];
    let items = response_input_items(&messages);
    assert_eq!(items[0]["type"], "message");
    assert_eq!(items[0]["content"][0]["type"], "input_text");
    assert_eq!(items[1]["type"], "function_call");
    assert_eq!(items[2]["type"], "function_call_output");
    let id = "resp_delete_contract";
    store_response(
        "tenant-delete",
        id.to_string(),
        StoredResponse {
            response: response_object(id, "webagent/chatgpt", "x"),
            messages,
        },
    );
    assert!(delete_response("tenant-delete", id));
    assert!(retrieve_response("tenant-delete", id).is_none());
    assert!(!delete_response("tenant-delete", id));
}

#[test]
fn persistent_state_ueberlebt_restart_und_trennt_mandanten() {
    let tenant_a = tenant_id("key-alpha");
    let tenant_b = tenant_id("key-beta");
    assert_ne!(tenant_a, tenant_b);
    let parent_id = "resp_persist_parent";
    let mut messages = responses_messages(&json!("Codewort Luchs")).unwrap();
    append_response_message(
        &mut messages,
        &crate::browser_inference::BrowserInferenceResponse {
            text: Some("gemerkt".to_string()),
            tool_calls: Vec::new(),
        },
    );
    store_response(
        &tenant_a,
        parent_id.to_string(),
        StoredResponse {
            response: response_with_state(
                response_object(parent_id, "webagent/chatgpt", "gemerkt"),
                None,
            ),
            messages,
        },
    );

    forget_cached_response_stores();
    assert!(retrieve_response(&tenant_a, parent_id).is_some());
    assert!(retrieve_response(&tenant_b, parent_id).is_none());

    let chained = ResponsesRequest {
        model: "webagent/chatgpt".to_string(),
        input: json!("Wie lautet es?"),
        instructions: None,
        stream: None,
        tools: Vec::new(),
        tool_choice: None,
        previous_response_id: Some(parent_id.to_string()),
        store: true,
    };
    forget_cached_response_stores();
    let (history, _) = responses_context(&chained, &tenant_a).unwrap();
    assert!(history.iter().any(|m| m.content == json!("Codewort Luchs")));
    assert!(responses_context(&chained, &tenant_b).is_err());

    forget_cached_response_stores();
    assert!(delete_response(&tenant_a, parent_id));
    forget_cached_response_stores();
    assert!(retrieve_response(&tenant_a, parent_id).is_none());
    let path = tenant_store_path(&tenant_a);
    let raw = fs::read_to_string(&path).unwrap();
    assert!(raw.contains(LOCAL_STATE_FORMAT));
}

// --- Auth / Fehler / Transport ----------------------------------------------

#[test]
fn timing_safe_comparison_requires_equal_content() {
    assert!(constant_time_equal("same", "same"));
    assert!(!constant_time_equal("same", "diff"));
    assert!(!constant_time_equal("short", "longer"));
}

#[test]
fn response_renderer_sets_protocol_and_length() {
    let rendered = render_http_response(&HttpResponse::json(200, json!({"ok": true})));
    let text = String::from_utf8(rendered).unwrap();
    assert!(text.starts_with("HTTP/1.1 200 OK\r\n"));
    assert!(text.contains("Content-Type: application/json; charset=utf-8\r\n"));
    assert!(text.contains("X-Request-Id: req_"));
    assert!(text.ends_with("{\"ok\":true}"));
}

#[test]
fn rejects_unsupported_semantic_fields_ohne_sie_zu_ignorieren() {
    assert!(reject_unsupported_openai_fields(&json!({"model":"webagent/chatgpt"})).is_ok());
    assert!(reject_unsupported_openai_fields(&json!({"n":1})).is_ok());
    assert!(reject_unsupported_openai_fields(&json!({"logprobs":false})).is_ok());
    assert!(reject_unsupported_openai_fields(&json!({"unknown_future":true})).is_ok());

    let seed = reject_unsupported_openai_fields(&json!({"seed":7})).unwrap_err();
    let seed_body: Value = serde_json::from_slice(&seed.body).unwrap();
    assert_eq!(seed.status, 400);
    assert_eq!(seed_body["error"]["code"], "unsupported_parameter");
    assert_eq!(seed_body["error"]["param"], "seed");

    let n = reject_unsupported_openai_fields(&json!({"n":2})).unwrap_err();
    let n_body: Value = serde_json::from_slice(&n.body).unwrap();
    assert_eq!(n_body["error"]["code"], "unsupported_value");
    assert_eq!(n_body["error"]["param"], "n");

    let logprobs = reject_unsupported_openai_fields(&json!({"logprobs":true})).unwrap_err();
    let lp: Value = serde_json::from_slice(&logprobs.body).unwrap();
    assert_eq!(lp["error"]["param"], "logprobs");

    let tier = reject_unsupported_openai_fields(&json!({"service_tier":"default"})).unwrap_err();
    let tb: Value = serde_json::from_slice(&tier.body).unwrap();
    assert_eq!(tb["error"]["param"], "service_tier");
}

#[test]
fn completion_ids_folgen_openai_praefixen() {
    assert!(completion_id("resp").starts_with("resp_"));
    assert!(completion_id("chatcmpl").starts_with("chatcmpl-"));
    assert!(completion_id("req").starts_with("req_"));
}

#[test]
fn multipart_audio_request_preserves_binary_file_and_fields() {
    let boundary = "webagent-test-boundary";
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"model\"\r\n\r\nwebagent/gemini\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"response_format\"\r\n\r\ntext\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(&[0, 1, 2, 255]);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = HttpRequest {
        method: "POST".to_string(),
        path: "/v1/audio/transcriptions".to_string(),
        query: String::new(),
        headers: BTreeMap::from([(
            "content-type".to_string(),
            format!("multipart/form-data; boundary={boundary}"),
        )]),
        body,
    };

    let parts = multipart_parts(&request).unwrap();
    assert_eq!(multipart_text(&parts, "model"), Some("webagent/gemini"));
    assert_eq!(multipart_text(&parts, "response_format"), Some("text"));
    let file = parts.iter().find(|part| part.name == "file").unwrap();
    assert_eq!(file.file_name.as_deref(), Some("voice.wav"));
    assert_eq!(file.content_type.as_deref(), Some("audio/wav"));
    assert_eq!(file.data, [0, 1, 2, 255]);
}

#[test]
fn multipart_audio_request_requires_boundary() {
    let request = HttpRequest {
        method: "POST".to_string(),
        path: "/v1/audio/transcriptions".to_string(),
        query: String::new(),
        headers: BTreeMap::from([(
            "content-type".to_string(),
            "multipart/form-data".to_string(),
        )]),
        body: Vec::new(),
    };
    assert!(multipart_parts(&request).unwrap_err().contains("boundary"));
}

#[test]
fn api_error_uses_provider_specific_shapes() {
    let openai = String::from_utf8(api_error(ApiFlavor::OpenAi, 401, "x").body).unwrap();
    let anthropic = String::from_utf8(api_error(ApiFlavor::Anthropic, 401, "x").body).unwrap();
    assert!(openai.contains("\"error\":{"));
    assert!(anthropic.contains("\"type\":\"error\""));
}

#[test]
fn accepts_x_api_key_header() {
    let config = BridgeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        brain: "chatgpt".to_string(),
        timeout_secs: None,
        headless: true,
        api_key: "test-secret".to_string(),
        fake_reply: None,
    };
    let headers = BTreeMap::from([("x-api-key".to_string(), "test-secret".to_string())]);

    assert!(authorize(&headers, &config, ApiFlavor::OpenAi).is_ok());
}

#[test]
fn rejects_non_positive_timeout_before_binding() {
    let error = serve(BridgeConfig {
        bind: "127.0.0.1:0".parse().unwrap(),
        brain: "chatgpt".to_string(),
        timeout_secs: Some(0.0),
        headless: true,
        api_key: "test-secret".to_string(),
        fake_reply: None,
    })
    .unwrap_err();

    assert!(error.contains("--timeout-secs muss groesser als 0 sein"));
}

#[test]
fn connection_limiter_rejects_excess_and_recovers_after_release() {
    let limiter = Arc::new(ConnectionLimiter::default());
    let mut permits: Vec<_> = (0..MAX_CONCURRENT_CONNECTIONS)
        .map(|_| limiter.try_acquire().expect("Kapazitaet verfuegbar"))
        .collect();

    assert!(limiter.try_acquire().is_none());
    drop(permits.pop());
    assert!(limiter.try_acquire().is_some());
}

// T-101: Die Bridge fuehrt Responses-Runs ueber den Kern (SessionService)
// statt eigener Session-Logik — belegt, dass die prozessweit-einmalige
// Registrierung einen sauberen Start->Delta->Complete->Done-Zyklus traegt.
#[test]
fn responses_flow_uses_core_session_service_cycle() {
    let a = session_service();
    let b = session_service();
    // Eine stabile, prozessweite Instanz (keine Wegwerf-Session pro Request).
    assert!(std::ptr::eq(a, b));

    let rid = "resp-test-1";
    let handle = a
        .start(rid, "claude", "beispiel aufgabe")
        .expect("neuer Lauf registrierbar");
    let _ = handle.push(crate::session::SessionEvent::Started {
        run_id: rid.into(),
        brain: "claude".into(),
        task: "beispiel aufgabe".into(),
    });
    assert_eq!(handle.status(), "running");

    let _ = handle.push(crate::session::SessionEvent::TextDelta {
        text: "hall".into(),
    });
    let _ = handle.push(crate::session::SessionEvent::TextDelta { text: "o".into() });
    let _ = handle.push(crate::session::SessionEvent::TextComplete);
    let _ = handle.push(crate::session::SessionEvent::Done {
        status: "done".into(),
    });

    let snap = a.snapshot(rid).expect("Lauf im Kern sichtbar");
    assert!(snap.done);
    assert_eq!(snap.status, "done");
    assert_eq!(snap.run_id, rid);
    // Error-Arm: Fehler setzt Status, aber endet erst mit terminalem Done.
    let rid2 = "resp-test-2";
    let h2 = a
        .start(rid2, "claude", "t")
        .expect("zweiter Lauf registrierbar");
    let _ = h2.push(crate::session::SessionEvent::Error {
        message: "kaputt".into(),
    });
    assert_eq!(h2.status(), "error");
}

fn spawn_fake_bridge(reply: &str) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback bind");
    let addr = listener.local_addr().expect("local addr");
    let config = BridgeConfig {
        bind: addr,
        brain: "chatgpt".to_string(),
        timeout_secs: None,
        headless: true,
        api_key: "t404-secret".to_string(),
        fake_reply: Some(reply.to_string()),
    };
    thread::spawn(move || {
        let _ = accept_loop(listener, config);
    });
    addr
}

fn http_exchange(addr: SocketAddr, request: &str) -> String {
    let mut stream =
        TcpStream::connect_timeout(&addr, Duration::from_secs(5)).expect("connect fake bridge");
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .expect("read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("write timeout");
    stream.write_all(request.as_bytes()).expect("write request");
    let _ = stream.flush();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

fn t404_script_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/t404")
}

fn t404_dump_dir() -> PathBuf {
    if let Ok(path) = std::env::var("WEBAGENT_T404_DUMP") {
        let path = PathBuf::from(path);
        let _ = fs::create_dir_all(&path);
        return path;
    }
    let path = std::env::temp_dir().join(format!("webagent-t404-{}", std::process::id()));
    let _ = fs::create_dir_all(&path);
    path
}

fn t404_probe(mut cmd: std::process::Command) -> bool {
    cmd.stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn t404_python_bin() -> Option<&'static str> {
    for bin in ["python", "python3"] {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(["-c", "pass"]);
        if t404_probe(cmd) {
            return Some(bin);
        }
    }
    None
}

fn t404_have_python_openai() -> bool {
    t404_python_bin().is_some_and(|bin| {
        let mut cmd = std::process::Command::new(bin);
        cmd.args(["-c", "import openai"]);
        t404_probe(cmd)
    })
}

fn t404_have_node() -> bool {
    let mut cmd = std::process::Command::new("node");
    cmd.args(["-e", "process.exit(0)"]);
    t404_probe(cmd)
}

fn t404_have_node_openai() -> bool {
    if !t404_have_node() {
        return false;
    }
    let dir = t404_script_dir();
    let mut cjs = std::process::Command::new("node");
    cjs.args(["-e", "require('openai')"]).current_dir(&dir);
    if t404_probe(cjs) {
        return true;
    }
    let mut esm = std::process::Command::new("node");
    esm.args([
        "--input-type=module",
        "-e",
        "import('openai').then(() => process.exit(0)).catch(() => process.exit(1))",
    ])
    .current_dir(&dir);
    t404_probe(esm)
}

fn run_t404_script(program: &str, args: &[&str], addr: SocketAddr, dump_dir: &std::path::Path) {
    let output = std::process::Command::new(program)
        .args(args)
        .current_dir(t404_script_dir())
        .env("WEBAGENT_T404_BASE", format!("http://{addr}/v1"))
        .env("WEBAGENT_T404_KEY", "t404-secret")
        .env("WEBAGENT_T404_EXPECT", "T404_FAKE_OK")
        .env("WEBAGENT_T404_DUMP", dump_dir)
        .output()
        .unwrap_or_else(|error| panic!("{program} nicht startbar: {error}"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{program} {:?} failed ({})\nstdout:\n{stdout}\nstderr:\n{stderr}",
        args,
        output.status
    );
}

// --- SDK-Blackbox -----------------------------------------------------------

#[test]
fn t404_sdk_blackbox_official_sdks_and_two_clients() {
    let addr = spawn_fake_bridge("T404_FAKE_OK");
    let dump_dir = t404_dump_dir();

    let models = http_exchange(
        addr,
        "GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t404-secret\r\nConnection: close\r\n\r\n",
    );
    assert!(models.contains("\"object\":\"list\"") || models.contains("\"object\": \"list\""));
    assert!(models.contains("webagent/chatgpt"));
    fs::write(dump_dir.join("raw_models.http"), &models).expect("dump models");

    let chat_body = r#"{"model":"webagent/chatgpt","messages":[{"role":"user","content":"ping"}]}"#;
    let chat = http_exchange(
        addr,
        &format!(
            "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t404-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{chat_body}",
            chat_body.len()
        ),
    );
    assert!(chat.contains("T404_FAKE_OK"));
    assert!(chat.contains("chat.completion"));
    fs::write(dump_dir.join("raw_chat.http"), &chat).expect("dump chat");

    let stream_body = r#"{"model":"webagent/chatgpt","stream":true,"messages":[{"role":"user","content":"ping"}]}"#;
    let stream = http_exchange(
        addr,
        &format!(
            "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t404-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{stream_body}",
            stream_body.len()
        ),
    );
    assert!(stream.contains("chat.completion.chunk"));
    assert!(stream.contains("T404_FAKE_OK") || stream.contains("T40"));
    assert!(stream.contains("[DONE]"));
    fs::write(dump_dir.join("raw_chat_stream.http"), &stream).expect("dump stream");

    let resp_body = r#"{"model":"webagent/chatgpt","input":"ping"}"#;
    let responses = http_exchange(
        addr,
        &format!(
            "POST /v1/responses HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t404-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{resp_body}",
            resp_body.len()
        ),
    );
    assert!(responses.contains("T404_FAKE_OK"));
    assert!(
        responses.contains("\"object\":\"response\"")
            || responses.contains("\"object\": \"response\"")
    );
    fs::write(dump_dir.join("raw_responses.http"), &responses).expect("dump responses");

    let seed_body =
        r#"{"model":"webagent/chatgpt","seed":7,"messages":[{"role":"user","content":"x"}]}"#;
    let seed = http_exchange(
        addr,
        &format!(
            "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer t404-secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{seed_body}",
            seed_body.len()
        ),
    );
    assert!(seed.contains("unsupported_parameter"));
    assert!(seed.contains("seed"));
    fs::write(dump_dir.join("raw_seed_reject.http"), &seed).expect("dump seed");

    if t404_have_python_openai() {
        let py = t404_python_bin().expect("python-openai implies a python bin");
        run_t404_script(py, &["openai_python.py"], addr, &dump_dir);
    } else {
        eprintln!("t404: skip openai_python.py (python-openai missing)");
    }
    if let Some(py) = t404_python_bin() {
        run_t404_script(py, &["urllib_client.py"], addr, &dump_dir);
    } else {
        eprintln!("t404: skip urllib_client.py (python missing)");
    }
    if t404_have_node_openai() {
        run_t404_script("node", &["openai_js.mjs"], addr, &dump_dir);
    } else {
        eprintln!("t404: skip openai_js.mjs (node openai missing)");
    }
    if t404_have_node() {
        run_t404_script("node", &["fetch_client.mjs"], addr, &dump_dir);
    } else {
        eprintln!("t404: skip fetch_client.mjs (node missing)");
    }
}
