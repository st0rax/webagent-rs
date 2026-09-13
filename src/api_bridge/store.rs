//! Mandanten-Store und Responses-Lifecycle der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Persistenzformat bleibt `openai-local-state-v1`. Keine Aenderung an
//! Mandantentrennung, kein neues Speicherformat, keine SSE-Renderer.
//! T-921 verdrahtet `mod store`. Typen bleiben in der Root-Datei.
//!
//! Invarianten:
//! - Tenant-ID ist FNV-1a ueber den API-Key (`t` + 16 Hex).
//! - Store-Datei: `{data_dir}/openai-local-state-v1/{tenant}/store.json`.
//! - Eviction: `MAX_STORED_RESPONSES` und `MAX_STORED_RESPONSE_BYTES`.
//! - Retrieve/Delete/input_items sind tenant-isoliert.

use super::{
    api_error, authorize, conversation_prompt, responses_messages, ApiFlavor, BridgeConfig,
    ConversationMessage, HttpRequest, HttpResponse, LOCAL_STATE_FORMAT, MAX_STORED_RESPONSES,
    MAX_STORED_RESPONSE_BYTES, OnDiskStore, OpenAiAssistantFunction, OpenAiAssistantToolCall,
    PromptBundle, ResponseStore, ResponsesRequest, StoreHub, StoredResponse,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

pub(super) fn handle_response_retrieve(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let id = path.trim_start_matches("/v1/responses/");
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    match retrieve_response(&tenant_id(&config.api_key), id) {
        Some(stored) => HttpResponse::json(200, stored.response),
        None => api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        ),
    }
}

pub(super) fn handle_response_delete(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let id = path.trim_start_matches("/v1/responses/");
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    if delete_response(&tenant_id(&config.api_key), id) {
        HttpResponse::json(200, json!({"id":id,"object":"response","deleted":true}))
    } else {
        api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        )
    }
}

pub(super) fn handle_response_input_items(
    request: &HttpRequest,
    config: &BridgeConfig,
    path: &str,
) -> HttpResponse {
    if let Err(response) = authorize(&request.headers, config, ApiFlavor::OpenAi) {
        return response;
    }
    let suffix = "/input_items";
    let id = path
        .strip_prefix("/v1/responses/")
        .and_then(|value| value.strip_suffix(suffix))
        .unwrap_or_default();
    if id.is_empty() || id.contains('/') {
        return api_error(ApiFlavor::OpenAi, 404, "Response nicht gefunden.");
    }
    let Some(stored) = retrieve_response(&tenant_id(&config.api_key), id) else {
        return api_error(
            ApiFlavor::OpenAi,
            404,
            &format!("Response '{id}' nicht gefunden."),
        );
    };
    let data = response_input_items(&stored.messages);
    HttpResponse::json(
        200,
        json!({
            "object": "list",
            "data": data,
            "has_more": false,
            "first_id": data.first().and_then(|item| item.get("id")),
            "last_id": data.last().and_then(|item| item.get("id"))
        }),
    )
}

pub(super) fn responses_context(
    payload: &ResponsesRequest,
    tenant: &str,
) -> Result<(Vec<ConversationMessage>, PromptBundle), (u16, String)> {
    let mut messages = match payload.previous_response_id.as_deref() {
        Some(id) => retrieve_response(tenant, id).map_or_else(
            || Err((404, format!("Previous response '{id}' nicht gefunden."))),
            |stored| Ok(stored.messages),
        )?,
        None => Vec::new(),
    };
    messages.extend(responses_messages(&payload.input).map_err(|error| (400, error))?);
    let prompt = conversation_prompt(payload.instructions.clone(), &messages)
        .map_err(|error| (400, error))?;
    Ok((messages, prompt))
}

pub(super) fn store_hub() -> &'static Mutex<StoreHub> {
    static HUB: OnceLock<Mutex<StoreHub>> = OnceLock::new();
    HUB.get_or_init(|| Mutex::new(StoreHub::default()))
}

pub(super) fn tenant_id(api_key: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in api_key.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("t{hash:016x}")
}

pub(super) fn local_state_root() -> PathBuf {
    crate::config::data_dir().join(LOCAL_STATE_FORMAT)
}

pub(super) fn tenant_store_path(tenant: &str) -> PathBuf {
    local_state_root().join(tenant).join("store.json")
}

pub(super) fn load_tenant_from_disk(tenant: &str) -> ResponseStore {
    let path = tenant_store_path(tenant);
    let Ok(bytes) = fs::read(&path) else {
        return ResponseStore::default();
    };
    let Ok(disk) = serde_json::from_slice::<OnDiskStore>(&bytes) else {
        return ResponseStore::default();
    };
    if disk.format != LOCAL_STATE_FORMAT {
        return ResponseStore::default();
    }
    ResponseStore {
        entries: disk.entries,
        order: disk.order.into(),
    }
}

pub(super) fn persist_tenant(tenant: &str, store: &ResponseStore) {
    let path = tenant_store_path(tenant);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let disk = OnDiskStore {
        format: LOCAL_STATE_FORMAT.to_string(),
        entries: store.entries.clone(),
        order: store.order.iter().cloned().collect(),
    };
    let Ok(bytes) = serde_json::to_vec_pretty(&disk) else {
        return;
    };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

pub(super) fn with_tenant_store<R>(tenant: &str, f: impl FnOnce(&mut ResponseStore) -> R) -> R {
    let mut hub = store_hub()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if !hub.tenants.contains_key(tenant) {
        hub.tenants
            .insert(tenant.to_string(), load_tenant_from_disk(tenant));
    }
    let store = hub.tenants.get_mut(tenant).expect("tenant just inserted");
    let result = f(store);
    persist_tenant(tenant, store);
    result
}

#[cfg(test)]
pub(super) fn forget_cached_response_stores() {
    store_hub()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .tenants
        .clear();
}

pub(super) fn retrieve_response(tenant: &str, id: &str) -> Option<StoredResponse> {
    with_tenant_store(tenant, |store| store.entries.get(id).cloned())
}

pub(super) fn store_response(tenant: &str, id: String, response: StoredResponse) {
    with_tenant_store(tenant, |store| {
        if !store.entries.contains_key(&id) {
            store.order.push_back(id.clone());
        }
        store.entries.insert(id, response);
        let mut stored_bytes: usize = store.entries.values().fold(0, |total, entry| {
            total.saturating_add(
                serde_json::to_vec(&entry.messages).map_or(usize::MAX, |bytes| bytes.len()),
            )
        });
        while store.order.len() > MAX_STORED_RESPONSES || stored_bytes > MAX_STORED_RESPONSE_BYTES {
            if let Some(expired) = store.order.pop_front() {
                if let Some(removed) = store.entries.remove(&expired) {
                    let removed_bytes = serde_json::to_vec(&removed.messages)
                        .map_or(usize::MAX, |bytes| bytes.len());
                    stored_bytes = stored_bytes.saturating_sub(removed_bytes);
                }
            } else {
                break;
            }
        }
    });
}

pub(super) fn delete_response(tenant: &str, id: &str) -> bool {
    with_tenant_store(tenant, |store| {
        let removed = store.entries.remove(id).is_some();
        if removed {
            store.order.retain(|entry| entry != id);
        }
        removed
    })
}

pub(super) fn response_input_items(messages: &[ConversationMessage]) -> Vec<Value> {
    let mut items = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.role == "tool" {
            items.push(json!({
                "id": format!("item_{index}"),
                "type": "function_call_output",
                "call_id": message.tool_call_id,
                "output": message.content
            }));
            continue;
        }
        if message.role == "assistant" && !message.tool_calls.is_empty() {
            for call in &message.tool_calls {
                items.push(json!({
                    "id": call.id,
                    "type": "function_call",
                    "status": "completed",
                    "call_id": call.id,
                    "name": call.function.name,
                    "arguments": call.function.arguments
                }));
            }
            continue;
        }
        let content_type = if message.role == "assistant" {
            "output_text"
        } else {
            "input_text"
        };
        items.push(json!({
            "id": format!("item_{index}"),
            "type": "message",
            "status": "completed",
            "role": message.role,
            "content": response_content_parts(&message.content, content_type)
        }));
    }
    items
}

pub(super) fn response_content_parts(value: &Value, default_text_type: &str) -> Vec<Value> {
    if let Some(text) = value.as_str() {
        return vec![json!({
            "type": default_text_type,
            "text": text,
            "annotations": []
        })];
    }
    let Some(parts) = value.as_array() else {
        return vec![json!({
            "type": default_text_type,
            "text": value.to_string(),
            "annotations": []
        })];
    };
    parts
        .iter()
        .map(|part| {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
            if matches!(part_type, "text" | "input_text" | "output_text") {
                json!({
                    "type": default_text_type,
                    "text": part.get("text").and_then(Value::as_str).unwrap_or_default(),
                    "annotations": []
                })
            } else {
                part.clone()
            }
        })
        .collect()
}

pub(super) fn append_response_message(
    messages: &mut Vec<ConversationMessage>,
    answer: &crate::browser_inference::BrowserInferenceResponse,
) {
    messages.push(ConversationMessage {
        role: "assistant".to_string(),
        content: answer
            .text
            .as_ref()
            .map_or(Value::Null, |text| Value::String(text.clone())),
        tool_calls: answer
            .tool_calls
            .iter()
            .map(|call| OpenAiAssistantToolCall {
                id: call.id.clone(),
                kind: "function".to_string(),
                function: OpenAiAssistantFunction {
                    name: call.name.clone(),
                    arguments: serde_json::to_string(&call.arguments)
                        .unwrap_or_else(|_| "{}".to_string()),
                },
            })
            .collect(),
        tool_call_id: None,
    });
}
