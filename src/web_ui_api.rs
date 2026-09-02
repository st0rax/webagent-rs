//! HTTP-API der lokalen Web-UI (T-202).
//!
//! Sitzt auf [`SessionService`] / [`EventStream`] und `doctor` ohne Browserstart.
//! Keine eigene Tool-Implementierung.

use crate::session::{SessionEvent, SessionService, Since};
use serde::Deserialize;
use serde_json::{json, Value};

/// Gemeinsamer Zustand eines UI-Prozesses.
#[derive(Debug, Default, Clone)]
pub struct UiState {
    pub sessions: SessionService,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl ApiResponse {
    fn json(status: u16, value: Value) -> Self {
        let body = serde_json::to_vec(&value).unwrap_or_else(|_| b"{}".to_vec());
        Self {
            status,
            content_type: "application/json",
            body,
        }
    }
}

#[derive(Deserialize, Default)]
struct NewSession {
    #[serde(default)]
    brain: String,
    #[serde(default)]
    task: String,
}

#[derive(Deserialize, Default)]
struct ChatBody {
    #[serde(default)]
    text: String,
}

#[derive(Deserialize, Default)]
struct UploadBody {
    #[serde(default)]
    filename: String,
}

/// Routet eine UI-API-Anfrage. Statische Assets bleiben in `web_ui::lookup`.
pub fn dispatch(method: &str, path: &str, query: &str, body: &str, state: &UiState) -> ApiResponse {
    let method = method.to_ascii_uppercase();
    match (method.as_str(), path) {
        ("GET", "/api/health/brains") => health_brains(),
        ("GET", "/api/capability") => capability_all(),
        ("GET", "/api/sessions") => list_sessions(state),
        ("POST", "/api/sessions") => create_session(state, body),
        ("GET", p) if p.starts_with("/api/capability/") => {
            capability_one(p.trim_start_matches("/api/capability/"))
        }
        _ => route_session(method.as_str(), path, query, body, state)
            .unwrap_or_else(|| ApiResponse::json(404, json!({"error": "not_found"}))),
    }
}

fn route_session(
    method: &str,
    path: &str,
    query: &str,
    body: &str,
    state: &UiState,
) -> Option<ApiResponse> {
    let rest = path.strip_prefix("/api/sessions/")?;
    let (id, tail) = match rest.split_once('/') {
        Some((id, rest)) => (id, rest),
        None => (rest, ""),
    };
    if id.is_empty() {
        return None;
    }
    Some(match (method, tail) {
        ("GET", "") => get_session(state, id),
        ("POST", "chat") => chat(state, id, body),
        ("POST", "stop") => stop(state, id),
        ("GET", "events") => events(state, id, query),
        ("POST", "upload") => upload(state, id, body),
        _ => ApiResponse::json(404, json!({"error": "not_found"})),
    })
}

fn health_brains() -> ApiResponse {
    let brains = crate::config::brains();
    let runs_dir = crate::config::runs_dir().to_string_lossy().to_string();
    let report = crate::doctor::run_doctor(None, Some(&brains), &runs_dir, None, None);
    ApiResponse::json(
        200,
        json!({
            "ok": report.ok(),
            "timestamp": report.timestamp,
            "healthy": report.healthy_brain_ids(),
            "unhealthy": report.unhealthy_brain_ids(),
            "brains": report.brains,
        }),
    )
}

fn capability_all() -> ApiResponse {
    let levels: Vec<Value> = crate::capability::levels_all()
        .into_iter()
        .map(|lvl| {
            json!({
                "brain_id": lvl.brain_id,
                "surveyed": lvl.surveyed,
                "available": lvl.available,
                "have": lvl.have,
            })
        })
        .collect();
    ApiResponse::json(200, json!({ "brains": levels }))
}

fn capability_one(brain: &str) -> ApiResponse {
    if brain.is_empty() {
        return ApiResponse::json(404, json!({"error": "not_found"}));
    }
    let lvl = crate::capability::level_of(brain);
    ApiResponse::json(
        200,
        json!({
            "brain_id": lvl.brain_id,
            "surveyed": lvl.surveyed,
            "available": lvl.available,
            "have": lvl.have,
        }),
    )
}

fn list_sessions(state: &UiState) -> ApiResponse {
    ApiResponse::json(200, json!({ "sessions": state.sessions.list() }))
}

fn create_session(state: &UiState, body: &str) -> ApiResponse {
    let parsed: NewSession = serde_json::from_str(body).unwrap_or_default();
    if parsed.brain.trim().is_empty() {
        return ApiResponse::json(400, json!({"error": "brain_required"}));
    }
    let run_id = format!(
        "ui-{}",
        crate::now_rfc3339()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    );
    match state
        .sessions
        .start(&run_id, parsed.brain.trim(), parsed.task.trim())
    {
        Ok(handle) => {
            let _ = handle.push(SessionEvent::Started {
                run_id: run_id.clone(),
                brain: parsed.brain.trim().to_string(),
                task: parsed.task.trim().to_string(),
            });
            ApiResponse::json(
                201,
                serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
            )
        }
        Err(error) => ApiResponse::json(409, json!({"error": error})),
    }
}

fn get_session(state: &UiState, id: &str) -> ApiResponse {
    match state.sessions.snapshot(id) {
        Some(snap) => ApiResponse::json(200, serde_json::to_value(snap).unwrap_or(json!({}))),
        None => ApiResponse::json(404, json!({"error": "session_not_found"})),
    }
}

fn chat(state: &UiState, id: &str, body: &str) -> ApiResponse {
    let Some(handle) = state.sessions.get(id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    if handle.is_done() {
        return ApiResponse::json(409, json!({"error": "session_done"}));
    }
    let parsed: ChatBody = serde_json::from_str(body).unwrap_or_default();
    if parsed.text.trim().is_empty() {
        return ApiResponse::json(400, json!({"error": "text_required"}));
    }
    if let Err(error) = handle.push(SessionEvent::TextDelta { text: parsed.text }) {
        return ApiResponse::json(409, json!({"error": error}));
    }
    let _ = handle.push(SessionEvent::TextComplete);
    ApiResponse::json(
        200,
        serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
    )
}

fn stop(state: &UiState, id: &str) -> ApiResponse {
    let Some(handle) = state.sessions.get(id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    if handle.is_done() {
        return ApiResponse::json(409, json!({"error": "session_done"}));
    }
    if let Err(error) = handle.push(SessionEvent::Done {
        status: "cancelled".into(),
    }) {
        return ApiResponse::json(409, json!({"error": error}));
    }
    ApiResponse::json(
        200,
        serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
    )
}

fn events(state: &UiState, id: &str, query: &str) -> ApiResponse {
    let since = query
        .split('&')
        .find_map(|p| p.strip_prefix("since="))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    match state.sessions.events_since(id, since) {
        Some(Since::Exact { events }) => ApiResponse::json(
            200,
            json!({ "since": since, "gap": false, "events": events }),
        ),
        Some(Since::Gap { events }) => ApiResponse::json(
            200,
            json!({ "since": since, "gap": true, "events": events }),
        ),
        None => ApiResponse::json(404, json!({"error": "session_not_found"})),
    }
}

fn upload(state: &UiState, id: &str, body: &str) -> ApiResponse {
    if state.sessions.get(id).is_none() {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    }
    let parsed: UploadBody = serde_json::from_str(body).unwrap_or_default();
    if parsed.filename.trim().is_empty() {
        return ApiResponse::json(400, json!({"error": "filename_required"}));
    }
    // Kein Schreiben auf die Platte: T-202 nimmt den Upload nur an.
    ApiResponse::json(
        202,
        json!({
            "accepted": true,
            "filename": parsed.filename,
            "stored": false
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post_session(state: &UiState) -> String {
        let resp = dispatch(
            "POST",
            "/api/sessions",
            "",
            r#"{"brain":"chatgpt","task":"hi"}"#,
            state,
        );
        assert_eq!(resp.status, 201);
        let v: Value = serde_json::from_slice(&resp.body).unwrap();
        v["run_id"].as_str().unwrap().to_string()
    }

    #[test]
    fn health_braucht_keinen_browser() {
        let state = UiState::default();
        let resp = dispatch("GET", "/api/health/brains", "", "", &state);
        assert_eq!(resp.status, 200);
        let v: Value = serde_json::from_slice(&resp.body).unwrap();
        assert!(v.get("timestamp").is_some());
        assert!(v.get("brains").is_some());
    }

    #[test]
    fn capability_listet_brains() {
        let state = UiState::default();
        let resp = dispatch("GET", "/api/capability", "", "", &state);
        assert_eq!(resp.status, 200);
        let v: Value = serde_json::from_slice(&resp.body).unwrap();
        assert!(v["brains"].as_array().unwrap().len() >= 1);
        let one = dispatch("GET", "/api/capability/chatgpt", "", "", &state);
        assert_eq!(one.status, 200);
        let one_v: Value = serde_json::from_slice(&one.body).unwrap();
        assert_eq!(one_v["brain_id"], "chatgpt");
    }

    #[test]
    fn session_chat_stop_events() {
        let state = UiState::default();
        let id = post_session(&state);
        let chat = dispatch(
            "POST",
            &format!("/api/sessions/{id}/chat"),
            "",
            r#"{"text":"hallo"}"#,
            &state,
        );
        assert_eq!(chat.status, 200);
        let ev = dispatch(
            "GET",
            &format!("/api/sessions/{id}/events"),
            "since=0",
            "",
            &state,
        );
        assert_eq!(ev.status, 200);
        let v: Value = serde_json::from_slice(&ev.body).unwrap();
        assert!(v["events"].as_array().unwrap().len() >= 2);
        let stop = dispatch("POST", &format!("/api/sessions/{id}/stop"), "", "", &state);
        assert_eq!(stop.status, 200);
        let again = dispatch("POST", &format!("/api/sessions/{id}/stop"), "", "", &state);
        assert_eq!(again.status, 409);
    }

    #[test]
    fn upload_schreibt_nicht_und_braucht_session() {
        let state = UiState::default();
        let missing = dispatch(
            "POST",
            "/api/sessions/nope/upload",
            "",
            r#"{"filename":"a.txt"}"#,
            &state,
        );
        assert_eq!(missing.status, 404);
        let id = post_session(&state);
        let ok = dispatch(
            "POST",
            &format!("/api/sessions/{id}/upload"),
            "",
            r#"{"filename":"a.txt"}"#,
            &state,
        );
        assert_eq!(ok.status, 202);
        let v: Value = serde_json::from_slice(&ok.body).unwrap();
        assert_eq!(v["stored"], false);
    }
}
