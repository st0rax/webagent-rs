//! OpenAI-Chat-, Anthropic- und Responses-Handler der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Dieses Modul enthaelt die **fachlichen Provider-Handler**. Es aendert keine
//! Protokoll- oder JSON-Semantik und keine Browser-Inferenz. Rendering,
//! Auth, Transport und Routing bleiben in `src/api_bridge.rs` bzw. den
//! Schwestermodulen; T-913 verdrahtet `mod provider_handlers`.
//!
//! | Funktion | Rolle |
//! |---|---|
//! | [`handle_openai`] | OpenAI Chat Completions, gepuffert |
//! | [`handle_openai_incremental`] | OpenAI Chat Completions, SSE |
//! | [`handle_anthropic`] | Anthropic Messages |
//! | [`handle_responses`] | OpenAI Responses, gepuffert |
//! | [`handle_responses_incremental`] | OpenAI Responses, SSE |
//!
//! # Kopplung (explizit, nur Parent-Helfer)
//!
//! Auth (`authorize`, `api_error`), Decode, Tool-/Prompt-Normalizer,
//! `run_task_blocking` / `run_task_streaming`, SSE-Schreibhelfer,
//! Response-Store, SessionService. Kein Zugriff auf Browser-Selektoren
//! oder Providerprofile.

use super::{
    anthropic_prompt, anthropic_response, anthropic_sse, anthropic_tool_choice, anthropic_tools,
    api_error, append_response_message, authorize, completion_id, decode_json, model_not_found,
    openai_message, openai_prompt, openai_sse, openai_tool_choice, openai_tools,
    reject_unsupported_openai_body, require_clean_text_tools, resolve_model, response_object,
    response_object_from_answer, response_with_state, responses_context, responses_sse_with_object,
    responses_tool_choice, responses_tools, run_task_blocking, run_task_streaming, session_service,
    store_response, stream_answer_snapshot, tenant_id, unix_seconds, write_data_frame,
    write_http_response, write_sse_comment, write_sse_event, write_sse_headers, AnthropicRequest,
    ApiFlavor, BridgeConfig, HttpRequest, HttpResponse, OpenAiRequest, ResponsesRequest,
    StoredResponse,
};
use serde_json::json;
use std::{
    io::Write,
    net::TcpStream,
    time::{Duration, Instant},
};

pub(super) fn handle_openai(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    if let Err(response) = reject_unsupported_openai_body(&request.body) {
        return response;
    }
    let payload: OpenAiRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return model_not_found(ApiFlavor::OpenAi, &error),
    };
    let prompt = match openai_prompt(&payload) {
        Ok(prompt) => prompt,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tools = match openai_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tool_choice = match openai_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &tools,
        tool_choice,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("chatcmpl");
    if payload.stream.unwrap_or(false) {
        return openai_sse(&id, &payload.model, &answer);
    }
    let message = openai_message(&answer);
    HttpResponse::json(
        200,
        json!({
            "id": id,
            "object": "chat.completion",
            "created": unix_seconds(),
            "model": payload.model,
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": answer.finish_reason(),
                "logprobs": null
            }],
            // Eine Browser-Oberflaeche gibt keine Tokenzahlen her. OpenAI liefert
            // hier aber immer ein Objekt, und Clients lesen `usage.total_tokens`
            // direkt -- bei `null` laufen sie in einen Fehler. Nullen sind
            // ehrlich und brechen niemanden; geschaetzte Zahlen waeren erfunden.
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0},
            "system_fingerprint": null
        }),
    )
}

pub(super) fn handle_openai_incremental(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return write_http_response(stream, response);
    }
    if let Err(response) = reject_unsupported_openai_body(&request.body) {
        return write_http_response(stream, response);
    }
    let payload: OpenAiRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => {
            return write_http_response(stream, model_not_found(ApiFlavor::OpenAi, &error))
        }
    };
    let prompt = match openai_prompt(&payload) {
        Ok(prompt) => prompt,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let tools = openai_tools(&payload.tools)
        .map_err(|error| format!("Inkrementeller Tool-Request unerwartet: {error}"))?;
    let choice = openai_tool_choice(payload.tool_choice.as_ref(), &tools)
        .map_err(|error| format!("Inkrementeller tool_choice unerwartet: {error}"))?;
    if let Err(error) = require_clean_text_tools(&tools, &choice) {
        return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error));
    }

    let id = completion_id("chatcmpl");
    write_sse_headers(stream)?;
    write_data_frame(
        stream,
        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}),
    )?;

    let mut last_sent = String::new();
    let mut last_keepalive = Instant::now();
    let mut stream_error: Option<String> = None;
    let answer = {
        let mut on_update = |snapshot: &str| {
            if stream_error.is_some() {
                return;
            }
            let cleaned = stream_answer_snapshot(snapshot);
            if cleaned.is_empty() {
                if last_keepalive.elapsed() >= Duration::from_secs(5) {
                    if let Err(error) = write_sse_comment(stream, "keep-alive") {
                        stream_error = Some(error);
                    } else {
                        last_keepalive = Instant::now();
                    }
                }
                return;
            }
            if cleaned == last_sent {
                if last_keepalive.elapsed() >= Duration::from_secs(5) {
                    if let Err(error) = write_sse_comment(stream, "keep-alive") {
                        stream_error = Some(error);
                    } else {
                        last_keepalive = Instant::now();
                    }
                }
                return;
            }
            if let Some(delta) = cleaned.strip_prefix(&last_sent) {
                if !delta.is_empty() {
                    if let Err(error) = write_data_frame(
                        stream,
                        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"content":delta},"finish_reason":null}]}),
                    ) {
                        stream_error = Some(error);
                        return;
                    }
                }
                last_sent = cleaned;
                last_keepalive = Instant::now();
            }
        };
        run_task_streaming(
            config,
            &brain,
            &prompt.text,
            &prompt.attachments,
            &mut on_update,
        )
    };
    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            if stream_error.is_none() {
                let _ = write_data_frame(
                    stream,
                    json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{},"finish_reason":"error"}],"error":{"message":error,"type":"server_error"}}),
                );
            }
            return Ok(());
        }
    };
    let text = stream_answer_snapshot(answer.text.as_deref().unwrap_or_default());
    if let Some(delta) = text
        .strip_prefix(&last_sent)
        .filter(|delta| !delta.is_empty())
    {
        write_data_frame(
            stream,
            json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{"content":delta},"finish_reason":null}]}),
        )?;
    }
    if let Some(error) = stream_error {
        return Err(error);
    }
    write_data_frame(
        stream,
        json!({"id":id,"object":"chat.completion.chunk","created":unix_seconds(),"model":payload.model,"choices":[{"index":0,"delta":{},"finish_reason":answer.finish_reason()}]}),
    )?;
    stream
        .write_all(b"data: [DONE]\n\n")
        .map_err(|error| format!("Chat-SSE-Abschluss nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("Chat-SSE-Abschluss nicht flushbar: {error}"))
}

pub(super) fn handle_anthropic(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::Anthropic) {
        return response;
    }
    let payload: AnthropicRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    if payload.max_tokens == 0 {
        return api_error(
            ApiFlavor::Anthropic,
            400,
            "max_tokens muss groesser als 0 sein.",
        );
    }
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return model_not_found(ApiFlavor::Anthropic, &error),
    };
    let prompt = match anthropic_prompt(&payload) {
        Ok(prompt) => prompt,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let tools = match anthropic_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    let tool_choice = match anthropic_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::Anthropic, 400, &error),
    };
    if let Err(error) = require_clean_text_tools(&tools, &tool_choice) {
        return api_error(ApiFlavor::Anthropic, 400, &error);
    }
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &[],
        crate::browser_inference::BrowserToolChoice::None,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::Anthropic, 502, &error),
    };
    let id = completion_id("msg");
    let response = anthropic_response(&id, &payload.model, &answer);
    if payload.stream.unwrap_or(false) {
        return anthropic_sse(&id, &payload.model, &answer);
    }
    HttpResponse::json(200, response)
}

pub(super) fn handle_responses(request: &HttpRequest, config: &BridgeConfig) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    if let Err(response) = reject_unsupported_openai_body(&request.body) {
        return response;
    }
    let payload: ResponsesRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => return model_not_found(ApiFlavor::OpenAi, &error),
    };
    let tools = match responses_tools(&payload.tools) {
        Ok(tools) => tools,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    let tool_choice = match responses_tool_choice(payload.tool_choice.as_ref(), &tools) {
        Ok(choice) => choice,
        Err(error) => return api_error(ApiFlavor::OpenAi, 400, &error),
    };
    if let Err(error) = require_clean_text_tools(&tools, &tool_choice) {
        return api_error(ApiFlavor::OpenAi, 400, &error);
    }
    let tenant = tenant_id(&config.api_key);
    let (mut messages, prompt) = match responses_context(&payload, &tenant) {
        Ok(context) => context,
        Err((status, error)) => return api_error(ApiFlavor::OpenAi, status, &error),
    };
    let answer = match run_task_blocking(
        config,
        &brain,
        &prompt.text,
        &prompt.attachments,
        &[],
        crate::browser_inference::BrowserToolChoice::None,
    ) {
        Ok(answer) => answer,
        Err(error) => return api_error(ApiFlavor::OpenAi, 502, &error),
    };
    let id = completion_id("resp");
    append_response_message(&mut messages, &answer);
    let response = response_object_from_answer(&id, &payload.model, &answer);
    let response = response_with_state(response, payload.previous_response_id.as_deref());
    if payload.store {
        store_response(
            &tenant,
            id.clone(),
            StoredResponse {
                response: response.clone(),
                messages,
            },
        );
    }
    if payload.stream.unwrap_or(false) {
        return responses_sse_with_object(&id, &payload.model, &answer, response);
    }
    HttpResponse::json(200, response)
}

pub(super) fn handle_responses_incremental(
    stream: &mut TcpStream,
    request: &HttpRequest,
    config: &BridgeConfig,
) -> Result<(), String> {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return write_http_response(stream, response);
    }
    if let Err(response) = reject_unsupported_openai_body(&request.body) {
        return write_http_response(stream, response);
    }
    let payload: ResponsesRequest = match decode_json(&request.body) {
        Ok(payload) => payload,
        Err(error) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error))
        }
    };
    let brain = match resolve_model(&payload.model, &config.brain) {
        Ok(brain) => brain,
        Err(error) => {
            return write_http_response(stream, model_not_found(ApiFlavor::OpenAi, &error))
        }
    };
    let tools = responses_tools(&payload.tools)
        .map_err(|error| format!("Inkrementeller Tool-Request unerwartet: {error}"))?;
    let choice = responses_tool_choice(payload.tool_choice.as_ref(), &tools)
        .map_err(|error| format!("Inkrementeller tool_choice unerwartet: {error}"))?;
    if let Err(error) = require_clean_text_tools(&tools, &choice) {
        return write_http_response(stream, api_error(ApiFlavor::OpenAi, 400, &error));
    }
    let tenant = tenant_id(&config.api_key);
    let (mut messages, prompt) = match responses_context(&payload, &tenant) {
        Ok(context) => context,
        Err((status, error)) => {
            return write_http_response(stream, api_error(ApiFlavor::OpenAi, status, &error))
        }
    };

    let id = completion_id("resp");
    let item_id = format!("{id}_msg");

    // Lauf im Kern registrieren: der Responses-Fluss fuehrt seine Events ueber
    // den UI-neutralen SessionService statt eigener Bridge-Session-Logik.
    let session = session_service();
    let _sess = match session.start(&id, &brain, &prompt.text) {
        Ok(handle) => {
            let _ = handle.push(crate::session::SessionEvent::Started {
                run_id: id.clone(),
                brain: brain.clone(),
                task: prompt.text.clone(),
            });
            Some(handle)
        }
        Err(_) => None, // doppelte id: prioritaet auf SSE-Ausgabe, nicht auf Session
    };

    let mut started = response_with_state(
        response_object(&id, &payload.model, ""),
        payload.previous_response_id.as_deref(),
    );
    started["status"] = json!("in_progress");
    started["output"] = json!([]);
    started["output_text"] = json!("");
    write_sse_headers(stream)?;
    let mut seq = 0u64;
    write_sse_event(
        stream,
        "response.created",
        json!({"type":"response.created","response":started}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.in_progress",
        json!({"type":"response.in_progress","response":started}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.output_item.added",
        json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]}}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.content_part.added",
        json!({"type":"response.content_part.added","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}),
        &mut seq,
    )?;

    let mut last_sent = String::new();
    let mut last_keepalive = Instant::now();
    let mut stream_error: Option<String> = None;
    let answer = {
        let mut on_update = |snapshot: &str| {
            if stream_error.is_some() {
                return;
            }
            let cleaned = stream_answer_snapshot(snapshot);
            if cleaned.is_empty() || cleaned == last_sent {
                if last_keepalive.elapsed() >= Duration::from_secs(5) {
                    if let Err(error) = write_sse_comment(stream, "keep-alive") {
                        stream_error = Some(error);
                    } else {
                        last_keepalive = Instant::now();
                    }
                }
                return;
            }
            if let Some(delta) = cleaned.strip_prefix(&last_sent) {
                if !delta.is_empty() {
                    if let Some(h) = _sess.as_ref() {
                        let _ = h.push(crate::session::SessionEvent::TextDelta {
                            text: delta.to_string(),
                        });
                    }
                    if let Err(error) = write_sse_event(
                        stream,
                        "response.output_text.delta",
                        json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":delta}),
                        &mut seq,
                    ) {
                        stream_error = Some(error);
                        return;
                    }
                }
                last_sent = cleaned;
                last_keepalive = Instant::now();
            }
        };
        run_task_streaming(
            config,
            &brain,
            &prompt.text,
            &prompt.attachments,
            &mut on_update,
        )
    };

    let answer = match answer {
        Ok(answer) => answer,
        Err(error) => {
            if let Some(h) = _sess.as_ref() {
                let _ = h.push(crate::session::SessionEvent::Error {
                    message: error.clone(),
                });
                let _ = h.push(crate::session::SessionEvent::Done {
                    status: "error".to_string(),
                });
            }
            let mut failed = started;
            failed["status"] = json!("failed");
            failed["error"] = json!({"code":"server_error","message":error});
            if stream_error.is_none() {
                write_sse_event(
                    stream,
                    "response.failed",
                    json!({"type":"response.failed","response":failed}),
                    &mut seq,
                )?;
            }
            return Ok(());
        }
    };
    let text = stream_answer_snapshot(answer.text.as_deref().unwrap_or_default());
    if let Some(delta) = text
        .strip_prefix(&last_sent)
        .filter(|delta| !delta.is_empty())
    {
        write_sse_event(
            stream,
            "response.output_text.delta",
            json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":delta}),
            &mut seq,
        )?;
    }
    if let Some(error) = stream_error {
        return Err(error);
    }

    append_response_message(&mut messages, &answer);
    let completed = response_with_state(
        response_object_from_answer(&id, &payload.model, &answer),
        payload.previous_response_id.as_deref(),
    );
    if payload.store {
        store_response(
            &tenant,
            id.clone(),
            StoredResponse {
                response: completed.clone(),
                messages,
            },
        );
    }
    write_sse_event(
        stream,
        "response.output_text.done",
        json!({"type":"response.output_text.done","item_id":item_id,"output_index":0,"content_index":0,"text":text}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.content_part.done",
        json!({"type":"response.content_part.done","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.output_item.done",
        json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]}),
        &mut seq,
    )?;
    write_sse_event(
        stream,
        "response.completed",
        json!({"type":"response.completed","response":completed}),
        &mut seq,
    )?;

    if let Some(h) = _sess.as_ref() {
        let _ = h.push(crate::session::SessionEvent::TextComplete);
        let _ = h.push(crate::session::SessionEvent::Done {
            status: "done".to_string(),
        });
    }
    Ok(())
}
