//! JSON- und SSE-Antwortkoerper der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Header und `sequence_number` bleiben in `wire.rs`. Keine Handler-Orchestrierung.
//! T-933 verdrahtet `mod response_protocol`.

use super::{unix_seconds, wire::sse_data, HttpResponse};
use serde_json::{json, Value};

pub(crate) fn anthropic_response(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> Value {
    if answer.tool_calls.is_empty() {
        return json!({
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [{"type": "text", "text": answer.text.as_deref().unwrap_or_default()}],
            "stop_reason": "end_turn",
            "stop_sequence": null
        });
    }
    let content: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.arguments
            })
        })
        .collect();
    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": content,
        "stop_reason": "tool_use",
        "stop_sequence": null
    })
}

pub(crate) fn openai_message(answer: &crate::browser_inference::BrowserInferenceResponse) -> Value {
    if answer.tool_calls.is_empty() {
        return json!({"role": "assistant", "content": answer.text});
    }
    let tool_calls: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
                }
            })
        })
        .collect();
    json!({"role": "assistant", "content": null, "tool_calls": tool_calls})
}

pub(crate) fn openai_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let delta = if answer.tool_calls.is_empty() {
        json!({"role": "assistant", "content": answer.text})
    } else {
        let tool_calls: Vec<Value> = answer
            .tool_calls
            .iter()
            .enumerate()
            .map(|(index, call)| {
                json!({
                    "index": index,
                    "id": call.id,
                    "type": "function",
                    "function": {
                        "name": call.name,
                        "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
                    }
                })
            })
            .collect();
        json!({"role": "assistant", "tool_calls": tool_calls})
    };
    let first = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": null, "logprobs": null}],
        "usage": null
    });
    let last = json!({
        "id": id,
        "object": "chat.completion.chunk",
        "created": unix_seconds(),
        "model": model,
        "choices": [{"index": 0, "delta": {}, "finish_reason": answer.finish_reason(), "logprobs": null}],
        "usage": null
    });
    HttpResponse::sse(format!("data: {first}\n\ndata: {last}\n\ndata: [DONE]\n\n"))
}

pub(crate) fn response_object(id: &str, model: &str, text: &str) -> Value {
    json!({
        "id": id,
        "object": "response",
        "created_at": unix_seconds(),
        "status": "completed",
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "max_output_tokens": null,
        "max_tool_calls": null,
        "model": model,
        "output": [{
            "id": format!("{id}_msg"),
            "type": "message",
            "status": "completed",
            "role": "assistant",
            "content": [{"type": "output_text", "text": text, "annotations": []}]
        }],
        "output_text": text,
        "parallel_tool_calls": false,
        "previous_response_id": null,
        "prompt_cache_key": null,
        "reasoning": null,
        "store": true,
        "temperature": null,
        "tool_choice": "none",
        "tools": [],
        "top_p": null,
        "truncation": null,
        "usage": null,
        "user": null,
        "metadata": {}
    })
}

pub(crate) fn response_object_from_answer(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> Value {
    if answer.tool_calls.is_empty() {
        return response_object(id, model, answer.text.as_deref().unwrap_or_default());
    }
    let output: Vec<Value> = answer
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function_call",
                "status": "completed",
                "call_id": call.id,
                "name": call.name,
                "arguments": serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string())
            })
        })
        .collect();
    json!({
        "id": id,
        "object": "response",
        "created_at": unix_seconds(),
        "status": "completed",
        "background": false,
        "error": null,
        "incomplete_details": null,
        "instructions": null,
        "max_output_tokens": null,
        "max_tool_calls": null,
        "model": model,
        "output": output,
        "output_text": "",
        "parallel_tool_calls": false,
        "previous_response_id": null,
        "prompt_cache_key": null,
        "reasoning": null,
        "store": true,
        "temperature": null,
        "tool_choice": "auto",
        "tools": [],
        "top_p": null,
        "truncation": null,
        "usage": null,
        "user": null,
        "metadata": {}
    })
}

pub(crate) fn response_with_state(
    mut response: Value,
    previous_response_id: Option<&str>,
) -> Value {
    response["previous_response_id"] = previous_response_id.map_or(Value::Null, |id| json!(id));
    response
}

#[cfg(test)]
pub(crate) fn responses_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let response = response_with_state(response_object_from_answer(id, model, answer), None);
    responses_sse_with_object(id, model, answer, response)
}

pub(crate) fn responses_sse_with_object(
    id: &str,
    _model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
    completed: Value,
) -> HttpResponse {
    let text = answer.text.as_deref().unwrap_or_default();
    let mut created = completed.clone();
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created["output_text"] = json!("");
    let mut seq = 0u64;
    let mut body = format!(
        "event: response.created\ndata: {}\n\n",
        sse_data(
            "response.created",
            json!({"type":"response.created","response":created}),
            &mut seq
        )
    );
    body.push_str(&format!(
        "event: response.in_progress\ndata: {}\n\n",
        sse_data(
            "response.in_progress",
            json!({"type":"response.in_progress","response":created}),
            &mut seq
        )
    ));
    if answer.tool_calls.is_empty() {
        let item_id = format!("{id}_msg");
        body.push_str(&format!(
            "event: response.output_item.added\ndata: {}\n\n",
            sse_data("response.output_item.added", json!({"type":"response.output_item.added","output_index":0,"item":{"id":item_id,"type":"message","status":"in_progress","role":"assistant","content":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.content_part.added\ndata: {}\n\n",
            sse_data("response.content_part.added", json!({"type":"response.content_part.added","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":"","annotations":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_text.delta\ndata: {}\n\n",
            sse_data("response.output_text.delta", json!({"type":"response.output_text.delta","item_id":item_id,"output_index":0,"content_index":0,"delta":text}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_text.done\ndata: {}\n\n",
            sse_data("response.output_text.done", json!({"type":"response.output_text.done","item_id":item_id,"output_index":0,"content_index":0,"text":text}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.content_part.done\ndata: {}\n\n",
            sse_data("response.content_part.done", json!({"type":"response.content_part.done","item_id":item_id,"output_index":0,"content_index":0,"part":{"type":"output_text","text":text,"annotations":[]}}), &mut seq)
        ));
        body.push_str(&format!(
            "event: response.output_item.done\ndata: {}\n\n",
            sse_data("response.output_item.done", json!({"type":"response.output_item.done","output_index":0,"item":completed["output"][0]}), &mut seq)
        ));
    } else {
        for (index, call) in answer.tool_calls.iter().enumerate() {
            let item = json!({"id":call.id,"type":"function_call","status":"in_progress","call_id":call.id,"name":call.name,"arguments":""});
            body.push_str(&format!(
                "event: response.output_item.added\ndata: {}\n\n",
                sse_data(
                    "response.output_item.added",
                    json!({"type":"response.output_item.added","output_index":index,"item":item}),
                    &mut seq
                )
            ));
            let arguments =
                serde_json::to_string(&call.arguments).unwrap_or_else(|_| "{}".to_string());
            body.push_str(&format!(
                "event: response.function_call_arguments.delta\ndata: {}\n\n",
                sse_data("response.function_call_arguments.delta", json!({"type":"response.function_call_arguments.delta","item_id":call.id,"output_index":index,"delta":arguments}), &mut seq)
            ));
            body.push_str(&format!(
                "event: response.function_call_arguments.done\ndata: {}\n\n",
                sse_data("response.function_call_arguments.done", json!({"type":"response.function_call_arguments.done","item_id":call.id,"output_index":index,"arguments":arguments}), &mut seq)
            ));
            body.push_str(&format!(
                "event: response.output_item.done\ndata: {}\n\n",
                sse_data("response.output_item.done", json!({"type":"response.output_item.done","output_index":index,"item":completed["output"][index]}), &mut seq)
            ));
        }
    }
    body.push_str(&format!(
        "event: response.completed\ndata: {}\n\n",
        sse_data(
            "response.completed",
            json!({"type":"response.completed","response":completed}),
            &mut seq
        )
    ));
    HttpResponse::sse(body)
}

pub(crate) fn anthropic_sse(
    id: &str,
    model: &str,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) -> HttpResponse {
    let started = json!({
        "type": "message_start",
        "message": {
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": [],
            "stop_reason": null,
            "stop_sequence": null
        }
    });
    let mut body = format!("event: message_start\ndata: {started}\n\n");
    if answer.tool_calls.is_empty() {
        let text = answer.text.as_deref().unwrap_or_default();
        let block_start = json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}});
        let delta = json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": text}});
        let block_stop = json!({"type": "content_block_stop", "index": 0});
        body.push_str(&format!(
            "event: content_block_start\ndata: {block_start}\n\n\
             event: content_block_delta\ndata: {delta}\n\n\
             event: content_block_stop\ndata: {block_stop}\n\n"
        ));
    } else {
        for (index, call) in answer.tool_calls.iter().enumerate() {
            let block_start = json!({
                "type": "content_block_start",
                "index": index,
                "content_block": {
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": call.arguments
                }
            });
            let block_stop = json!({"type": "content_block_stop", "index": index});
            body.push_str(&format!(
                "event: content_block_start\ndata: {block_start}\n\n\
                 event: content_block_stop\ndata: {block_stop}\n\n"
            ));
        }
    }
    let message_delta = json!({"type": "message_delta", "delta": {"stop_reason": if answer.tool_calls.is_empty() { "end_turn" } else { "tool_use" }, "stop_sequence": null}});
    let message_stop = json!({"type": "message_stop"});
    body.push_str(&format!(
        "event: message_delta\ndata: {message_delta}\n\n\
         event: message_stop\ndata: {message_stop}\n\n"
    ));
    HttpResponse::sse(body)
}
