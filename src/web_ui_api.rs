//! HTTP-API der lokalen Web-UI (T-202 / T-301).
//!
//! Sitzt auf [`SessionService`] / [`EventStream`] und `doctor`. Chat (T-301)
//! treibt ein persistentes Brain pro UI-Sitzung; Unit-Tests injizieren FakeBrain
//! und starten keinen Browser.

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::group_run::{run_group, stub_respond, GroupRegistry, GroupSpec};
use crate::session::{SessionEvent, SessionHandle, SessionService, Since};
use crate::source_scope::{parse_quelle_args, QuelleCommand, QuelleSpec};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Factory fuer das Brain einer UI-Sitzung. Produktion: sichtbares WebView.
/// Tests: FakeBrain, nie ein echter Browser.
type ChatFactory = Arc<dyn Fn(&str) -> Result<Box<dyn BrainBackend + Send>, String> + Send + Sync>;

struct LiveBrain {
    brain: Box<dyn BrainBackend + Send>,
    started: bool,
    composer_opened: bool,
}

struct SessionChat {
    live: Mutex<LiveBrain>,
    cancel: AtomicBool,
    generating: AtomicBool,
}

/// Gemeinsamer Zustand eines UI-Prozesses.
pub struct UiState {
    pub sessions: SessionService,
    pub groups: GroupRegistry,
    chats: Arc<Mutex<HashMap<String, Arc<SessionChat>>>>,
    factory: ChatFactory,
}

impl Clone for UiState {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            groups: self.groups.clone(),
            chats: Arc::clone(&self.chats),
            factory: Arc::clone(&self.factory),
        }
    }
}

impl std::fmt::Debug for UiState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiState")
            .field("sessions", &self.sessions)
            .field("groups", &self.groups)
            .finish_non_exhaustive()
    }
}

impl Default for UiState {
    fn default() -> Self {
        Self::with_chat_factory(production_chat_factory)
    }
}

impl UiState {
    /// Injizierbare Brain-Factory (Tests: FakeBrain; Produktion: WebBrainBackend).
    pub fn with_chat_factory<F>(factory: F) -> Self
    where
        F: Fn(&str) -> Result<Box<dyn BrainBackend + Send>, String> + Send + Sync + 'static,
    {
        Self {
            sessions: SessionService::new(),
            groups: GroupRegistry::default(),
            chats: Arc::new(Mutex::new(HashMap::new())),
            factory: Arc::new(factory),
        }
    }
}

fn production_chat_factory(brain_id: &str) -> Result<Box<dyn BrainBackend + Send>, String> {
    let backend = WebBrainBackend::from_config(brain_id)?;
    Ok(Box::new(backend))
}

/// Wachsende Snapshots (Brain-Vertrag) oder Chunks (FakeBrain Text/Stream)
/// werden zu eindeutigen Suffix-Deltas. Doppelte Volltexte entfallen.
fn unique_suffix(last_sent: &mut String, snapshot: &str) -> Option<String> {
    if snapshot.is_empty() {
        return None;
    }
    if snapshot == last_sent.as_str() {
        return None;
    }
    if let Some(delta) = snapshot.strip_prefix(last_sent.as_str()) {
        if delta.is_empty() {
            return None;
        }
        last_sent.push_str(delta);
        return Some(delta.to_string());
    }
    // Kuerzerer Snapshot: veralteter Poll, kein neues Chunk.
    if last_sent.starts_with(snapshot) {
        return None;
    }
    last_sent.push_str(snapshot);
    Some(snapshot.to_string())
}

fn composer_missing(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("composer-feld nicht gefunden") || lower.contains("composer not found")
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

#[derive(Deserialize, Default)]
struct SourceBody {
    #[serde(default)]
    brain: String,
    #[serde(default)]
    source: String,
    #[serde(default)]
    save: bool,
}

#[derive(Deserialize, Default)]
struct NewGroup {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    brains: Vec<String>,
}

#[derive(Deserialize, Default)]
struct GroupRunBody {
    #[serde(default)]
    task: String,
    #[serde(default)]
    rounds: u32,
    #[serde(default)]
    leader: String,
}

#[derive(Deserialize, Default)]
struct QuelleBody {
    #[serde(default)]
    session: String,
    #[serde(default)]
    brain: String,
    #[serde(default)]
    spec: String,
    #[serde(default)]
    save: bool,
    #[serde(default)]
    line: String,
}

/// Routet eine UI-API-Anfrage. Statische Assets bleiben in `web_ui::lookup`.
pub fn dispatch(method: &str, path: &str, query: &str, body: &str, state: &UiState) -> ApiResponse {
    let method = method.to_ascii_uppercase();
    match (method.as_str(), path) {
        ("GET", "/api/health/brains") => health_brains(),
        ("GET", "/api/capability") => capability_all(),
        ("GET", "/api/sessions") => list_sessions(state),
        ("POST", "/api/sessions") => create_session(state, body),
        ("GET", "/api/sources") => list_sources(state, query),
        ("POST", "/api/quelle") => post_quelle(state, query, body),
        ("GET", "/api/groups") => list_groups(state),
        ("POST", "/api/groups") => create_group(state, body),
        ("GET", p) if p.starts_with("/api/capability/") => {
            capability_one(p.trim_start_matches("/api/capability/"))
        }
        _ => route_group(method.as_str(), path, body, state)
            .or_else(|| route_session(method.as_str(), path, query, body, state))
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
        ("GET", "source") => get_source(state, id),
        ("POST", "source") => set_source(state, id, body),
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
    let text = parsed.text.trim();
    if text.is_empty() {
        return ApiResponse::json(400, json!({"error": "text_required"}));
    }
    let chat = match get_or_create_chat(state, id, &handle.brain()) {
        Ok(chat) => chat,
        Err(error) => return ApiResponse::json(400, json!({"error": error})),
    };
    if chat.generating.swap(true, Ordering::SeqCst) {
        return ApiResponse::json(409, json!({"error": "busy"}));
    }
    let result = drive_chat_turn(&handle, &chat, text);
    chat.generating.store(false, Ordering::SeqCst);
    result
}

fn get_or_create_chat(
    state: &UiState,
    id: &str,
    brain_id: &str,
) -> Result<Arc<SessionChat>, String> {
    let mut map = state.chats.lock().unwrap();
    if let Some(existing) = map.get(id) {
        return Ok(Arc::clone(existing));
    }
    let brain = (state.factory)(brain_id)?;
    let chat = Arc::new(SessionChat {
        live: Mutex::new(LiveBrain {
            brain,
            started: false,
            composer_opened: false,
        }),
        cancel: AtomicBool::new(false),
        generating: AtomicBool::new(false),
    });
    map.insert(id.to_string(), Arc::clone(&chat));
    Ok(chat)
}

fn drive_chat_turn(handle: &SessionHandle, chat: &SessionChat, text: &str) -> ApiResponse {
    if chat.cancel.load(Ordering::SeqCst) || handle.is_done() {
        return ApiResponse::json(409, json!({"error": "session_done"}));
    }
    let mut live = chat.live.lock().unwrap();
    if let Err(error) = ensure_session_brain(&mut live, &handle.brain()) {
        let _ = handle.push(SessionEvent::Error {
            message: error.clone(),
        });
        return ApiResponse::json(502, json!({"error": error}));
    }
    let baseline = match send_on_open_composer(&mut live, text) {
        Ok(baseline) => baseline,
        Err(error) => {
            let _ = handle.push(SessionEvent::Error {
                message: error.clone(),
            });
            return ApiResponse::json(502, json!({"error": error}));
        }
    };
    let wait_timeout =
        crate::timeouts::resolve_timeout("wait_response", &handle.brain(), text, None);
    let mut last_sent = String::new();
    let mut on_update = |snapshot: &str| {
        if chat.cancel.load(Ordering::SeqCst) || handle.is_done() {
            return;
        }
        if let Some(delta) = unique_suffix(&mut last_sent, snapshot) {
            let _ = handle.push(SessionEvent::TextDelta { text: delta });
        }
    };
    let response = match live
        .brain
        .wait_response_streaming(baseline, wait_timeout, &mut on_update)
    {
        Ok(response) => response,
        Err(error) => {
            if chat.cancel.load(Ordering::SeqCst) || handle.is_done() {
                let _ = live.brain.stop();
                return ApiResponse::json(
                    200,
                    serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
                );
            }
            let _ = handle.push(SessionEvent::Error {
                message: error.clone(),
            });
            return ApiResponse::json(502, json!({"error": error}));
        }
    };
    if chat.cancel.load(Ordering::SeqCst) || handle.is_done() {
        let _ = live.brain.stop();
        return ApiResponse::json(
            200,
            serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
        );
    }
    if response.backend_status == "rate_limit" || response.backend_status == "blocked" {
        let error = format!("{}: {}", response.backend_status, response.text.trim());
        let _ = handle.push(SessionEvent::Error {
            message: error.clone(),
        });
        return ApiResponse::json(502, json!({"error": error}));
    }
    if let Some(delta) = unique_suffix(&mut last_sent, &response.text) {
        let _ = handle.push(SessionEvent::TextDelta { text: delta });
    }
    if handle.is_done() {
        return ApiResponse::json(
            200,
            serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
        );
    }
    let _ = handle.push(SessionEvent::TextComplete);
    ApiResponse::json(
        200,
        serde_json::to_value(handle.snapshot()).unwrap_or(json!({})),
    )
}

fn ensure_session_brain(live: &mut LiveBrain, brain_id: &str) -> Result<(), String> {
    if live.started {
        return Ok(());
    }
    // T-301: sichtbares WebView, nie headless auf dem UI-Chat-Pfad.
    live.brain.start(false)?;
    let ready_timeout = crate::timeouts::resolve_timeout("ensure_ready", brain_id, "", None);
    let state = live
        .brain
        .ensure_ready(ready_timeout)
        .unwrap_or(SessionState::Error);
    if state != SessionState::Ready {
        let _ = live.brain.stop();
        live.started = false;
        return Err(format!("session_state={state:?}"));
    }
    live.started = true;
    Ok(())
}

fn send_on_open_composer(live: &mut LiveBrain, text: &str) -> Result<i32, String> {
    if !live.composer_opened {
        live.brain.new_chat()?;
        live.composer_opened = true;
    }
    match live.brain.send(text) {
        Ok(baseline) => Ok(baseline),
        Err(error) if live.composer_opened && composer_missing(&error) => {
            live.brain.new_chat()?;
            live.brain.send(text)
        }
        Err(error) => Err(error),
    }
}

fn stop(state: &UiState, id: &str) -> ApiResponse {
    let Some(handle) = state.sessions.get(id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    if handle.is_done() {
        return ApiResponse::json(409, json!({"error": "session_done"}));
    }
    let chat = {
        let mut map = state.chats.lock().unwrap();
        map.remove(id)
    };
    if let Some(chat) = chat {
        chat.cancel.store(true, Ordering::SeqCst);
        if let Ok(mut live) = chat.live.try_lock() {
            let _ = live.brain.stop();
        }
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

fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|p| {
        p.strip_prefix(&format!("{key}="))
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
    })
}

fn list_sources(state: &UiState, query: &str) -> ApiResponse {
    let brain = query_param(query, "brain");
    let session_id = query_param(query, "session");
    if let Some(id) = session_id {
        let Some(handle) = state.sessions.get(&id) else {
            return ApiResponse::json(404, json!({"error": "session_not_found"}));
        };
        let brain = brain.unwrap_or_else(|| handle.brain());
        let cmd = QuelleCommand {
            brain: Some(brain.clone()),
            spec: QuelleSpec::List,
            save: false,
        };
        return match handle.apply_quelle(&cmd) {
            Ok(report) => {
                let active = handle.active_source();
                ApiResponse::json(
                    200,
                    json!({
                        "brain": brain,
                        "active": active,
                        "text": report.text,
                    }),
                )
            }
            Err(error) => ApiResponse::json(400, json!({"error": error})),
        };
    }
    let scope = crate::source_scope::SourceScope::load_default();
    let listed = scope.list(brain.as_deref());
    ApiResponse::json(200, json!({ "sources": listed, "brain": brain }))
}

fn get_source(state: &UiState, id: &str) -> ApiResponse {
    let Some(handle) = state.sessions.get(id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    ApiResponse::json(
        200,
        serde_json::to_value(handle.active_source()).unwrap_or(json!({})),
    )
}

fn set_source(state: &UiState, id: &str, body: &str) -> ApiResponse {
    let Some(handle) = state.sessions.get(id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    let parsed: SourceBody = serde_json::from_str(body).unwrap_or_default();
    if parsed.source.trim().is_empty() {
        return ApiResponse::json(400, json!({"error": "source_required"}));
    }
    let brain = if parsed.brain.trim().is_empty() {
        handle.brain()
    } else {
        parsed.brain.trim().to_string()
    };
    let cmd = QuelleCommand {
        brain: Some(brain),
        spec: QuelleSpec::Set(parsed.source.trim().to_string()),
        save: parsed.save,
    };
    match handle.apply_quelle(&cmd) {
        Ok(report) => ApiResponse::json(
            200,
            json!({
                "persisted": report.persisted,
                "active": report.active,
                "text": report.text,
                "session": handle.snapshot(),
            }),
        ),
        Err(error) => ApiResponse::json(400, json!({"error": error})),
    }
}

fn list_groups(state: &UiState) -> ApiResponse {
    ApiResponse::json(200, json!({ "groups": state.groups.list() }))
}

fn slug_group_id(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        format!("g-{}", crate::now_run_stamp())
    } else {
        slug
    }
}

fn create_group(state: &UiState, body: &str) -> ApiResponse {
    let parsed: NewGroup = serde_json::from_str(body).unwrap_or_default();
    let id = if parsed.id.trim().is_empty() {
        slug_group_id(&parsed.name)
    } else {
        parsed.id.trim().to_string()
    };
    match GroupSpec::new(id, parsed.name, parsed.brains) {
        Ok(spec) => match state.groups.insert(spec) {
            Ok(stored) => {
                ApiResponse::json(201, serde_json::to_value(&stored).unwrap_or(json!({})))
            }
            Err(error) => ApiResponse::json(409, json!({"error": error})),
        },
        Err(error) => ApiResponse::json(400, json!({"error": error})),
    }
}

fn route_group(method: &str, path: &str, body: &str, state: &UiState) -> Option<ApiResponse> {
    let rest = path.strip_prefix("/api/groups/")?;
    let (id, tail) = match rest.split_once('/') {
        Some((id, rest)) => (id, rest),
        None => (rest, ""),
    };
    if id.is_empty() {
        return None;
    }
    Some(match (method, tail) {
        ("GET", "") => get_group(state, id),
        ("POST", "run") => run_group_api(state, id, body),
        _ => ApiResponse::json(404, json!({"error": "not_found"})),
    })
}

fn get_group(state: &UiState, id: &str) -> ApiResponse {
    match state.groups.get(id) {
        Some(spec) => ApiResponse::json(200, serde_json::to_value(&spec).unwrap_or(json!({}))),
        None => ApiResponse::json(404, json!({"error": "group_not_found"})),
    }
}

fn run_group_api(state: &UiState, id: &str, body: &str) -> ApiResponse {
    let Some(spec) = state.groups.get(id) else {
        return ApiResponse::json(404, json!({"error": "group_not_found"}));
    };
    let parsed: GroupRunBody = serde_json::from_str(body).unwrap_or_default();
    if parsed.task.trim().is_empty() {
        return ApiResponse::json(400, json!({"error": "task_required"}));
    }
    let rounds = if parsed.rounds == 0 { 1 } else { parsed.rounds };
    let leader = if parsed.leader.trim().is_empty() {
        spec.brains[0].clone()
    } else {
        parsed.leader.trim().to_string()
    };
    match run_group(
        &state.sessions,
        &spec,
        parsed.task.trim(),
        rounds,
        &leader,
        stub_respond,
    ) {
        Ok(handle) => ApiResponse::json(
            201,
            json!({
                "group": spec,
                "session": handle.snapshot(),
                "run_id": handle.run_id(),
            }),
        ),
        Err(error) => ApiResponse::json(400, json!({"error": error})),
    }
}

fn post_quelle(state: &UiState, query: &str, body: &str) -> ApiResponse {
    let parsed: QuelleBody = serde_json::from_str(body).unwrap_or_default();
    let cmd = if !parsed.line.trim().is_empty() {
        let line = parsed.line.trim();
        let rest = line.strip_prefix("/quelle").unwrap_or(line).trim();
        match parse_quelle_args(rest) {
            Ok(cmd) => cmd,
            Err(error) => return ApiResponse::json(400, json!({"error": error})),
        }
    } else {
        let spec = parsed.spec.trim();
        let spec = if spec == "list" {
            QuelleSpec::List
        } else if spec.is_empty() {
            if parsed.brain.trim().is_empty() {
                QuelleSpec::List
            } else {
                QuelleSpec::Show
            }
        } else {
            QuelleSpec::Set(spec.to_string())
        };
        QuelleCommand {
            brain: if parsed.brain.trim().is_empty() {
                None
            } else {
                Some(parsed.brain.trim().to_string())
            },
            spec,
            save: parsed.save,
        }
    };
    let session_id = if parsed.session.trim().is_empty() {
        query_param(query, "session").unwrap_or_default()
    } else {
        parsed.session.trim().to_string()
    };
    if session_id.is_empty() {
        return ApiResponse::json(400, json!({"error": "session_required"}));
    }
    let Some(handle) = state.sessions.get(&session_id) else {
        return ApiResponse::json(404, json!({"error": "session_not_found"}));
    };
    match handle.apply_quelle(&cmd) {
        Ok(report) => ApiResponse::json(
            200,
            json!({
                "persisted": report.persisted,
                "active": report.active,
                "text": report.text,
                "session": handle.snapshot(),
            }),
        ),
        Err(error) => ApiResponse::json(400, json!({"error": error})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fakebrain::{FakeBrain, Scenario};
    use std::time::Duration;

    fn test_ui(scenarios: Vec<Scenario>) -> UiState {
        UiState::with_chat_factory(move |_id| {
            Ok(Box::new(FakeBrain::new("fake", scenarios.clone())))
        })
    }

    fn event_kind(event: &Value) -> &str {
        event
            .as_object()
            .and_then(|m| m.keys().next())
            .map(String::as_str)
            .unwrap_or("")
    }

    fn text_deltas(events: &[Value]) -> Vec<String> {
        events
            .iter()
            .filter_map(|row| {
                row["event"]["TextDelta"]["text"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect()
    }

    fn fetch_events(state: &UiState, id: &str) -> (Value, Vec<Value>) {
        let ev = dispatch(
            "GET",
            &format!("/api/sessions/{id}/events"),
            "since=0",
            "",
            state,
        );
        assert_eq!(ev.status, 200);
        let v: Value = serde_json::from_slice(&ev.body).unwrap();
        let events = v["events"].as_array().cloned().unwrap_or_default();
        (v, events)
    }

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
        assert!(!v["brains"].as_array().unwrap().is_empty());
        let one = dispatch("GET", "/api/capability/chatgpt", "", "", &state);
        assert_eq!(one.status, 200);
        let one_v: Value = serde_json::from_slice(&one.body).unwrap();
        assert_eq!(one_v["brain_id"], "chatgpt");
    }

    #[test]
    fn session_chat_stop_events() {
        let state = test_ui(vec![Scenario::Text("ok".into(), 0)]);
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

    #[test]
    fn source_switch_ist_session_scope_ohne_auto_routing() {
        let state = UiState::default();
        let id = post_session(&state);
        let got = dispatch("GET", &format!("/api/sessions/{id}/source"), "", "", &state);
        assert_eq!(got.status, 200);
        let v: Value = serde_json::from_slice(&got.body).unwrap();
        assert_eq!(v["source"], "default");
        assert_eq!(v["kind"], "browser");

        let set = dispatch(
            "POST",
            &format!("/api/sessions/{id}/source"),
            "",
            r#"{"source":"openrouter","save":false}"#,
            &state,
        );
        assert_eq!(set.status, 200, "{}", String::from_utf8_lossy(&set.body));
        let set_v: Value = serde_json::from_slice(&set.body).unwrap();
        assert_eq!(set_v["persisted"], false);
        assert_eq!(set_v["active"]["source"], "openrouter");
        assert_eq!(set_v["session"]["source"], "openrouter");

        let other = dispatch(
            "POST",
            "/api/sessions",
            "",
            r#"{"brain":"chatgpt","task":"x"}"#,
            &state,
        );
        assert_eq!(other.status, 201);
        let other_v: Value = serde_json::from_slice(&other.body).unwrap();
        assert_eq!(other_v["source"], "default");

        let cmd = dispatch(
            "POST",
            "/api/quelle",
            "",
            &format!(r#"{{"session":"{id}","line":"/quelle claude default"}}"#),
            &state,
        );
        assert_eq!(cmd.status, 200);
        let cmd_v: Value = serde_json::from_slice(&cmd.body).unwrap();
        assert_eq!(cmd_v["persisted"], false);
        assert_eq!(cmd_v["active"]["source"], "default");
        assert_eq!(cmd_v["active"]["kind"], "browser");
    }

    #[test]
    fn groups_create_list_and_run_share_session_stream() {
        let state = UiState::default();
        let bad = dispatch(
            "POST",
            "/api/groups",
            "",
            r#"{"name":"zu klein","brains":["A"]}"#,
            &state,
        );
        assert_eq!(bad.status, 400);

        let created = dispatch(
            "POST",
            "/api/groups",
            "",
            r#"{"id":"demo","name":"Demo 2er-Gruppe","brains":["A","B"]}"#,
            &state,
        );
        assert_eq!(
            created.status,
            201,
            "{}",
            String::from_utf8_lossy(&created.body)
        );
        let listed = dispatch("GET", "/api/groups", "", "", &state);
        assert_eq!(listed.status, 200);
        let listed_v: Value = serde_json::from_slice(&listed.body).unwrap();
        assert_eq!(listed_v["groups"].as_array().unwrap().len(), 1);

        let run = dispatch(
            "POST",
            "/api/groups/demo/run",
            "",
            r#"{"task":"2+2","rounds":1,"leader":"A"}"#,
            &state,
        );
        assert_eq!(run.status, 201, "{}", String::from_utf8_lossy(&run.body));
        let run_v: Value = serde_json::from_slice(&run.body).unwrap();
        let run_id = run_v["run_id"].as_str().unwrap();
        assert_eq!(run_v["session"]["brain"], "group:demo");
        assert_eq!(run_v["session"]["status"], "done");

        let ev = dispatch(
            "GET",
            &format!("/api/sessions/{run_id}/events"),
            "since=0",
            "",
            &state,
        );
        assert_eq!(ev.status, 200);
        let ev_v: Value = serde_json::from_slice(&ev.body).unwrap();
        assert_eq!(ev_v["gap"], false);
        let events = ev_v["events"].as_array().unwrap();
        assert!(events.len() >= 4);
        let last = events.last().unwrap();
        assert!(last["event"].get("Done").is_some() || last["event"]["status"] == "done");
    }

    #[test]
    fn unique_suffix_strips_snapshots_and_chunks() {
        let mut last = String::new();
        assert_eq!(unique_suffix(&mut last, "Hel").as_deref(), Some("Hel"));
        assert_eq!(unique_suffix(&mut last, "Hello").as_deref(), Some("lo"));
        assert_eq!(unique_suffix(&mut last, "Hello"), None);
        last.clear();
        assert_eq!(unique_suffix(&mut last, "Hel").as_deref(), Some("Hel"));
        assert_eq!(unique_suffix(&mut last, "lo").as_deref(), Some("lo"));
        assert_eq!(last, "Hello");
        assert_eq!(unique_suffix(&mut last, "Hel"), None);
    }

    #[test]
    fn chat_multi_turn_unique_deltas_ohne_echo_und_ohne_done() {
        let state = test_ui(vec![
            Scenario::Stream(vec!["Hel".into(), "lo".into()]),
            Scenario::Stream(vec!["Wor".into(), "ld".into()]),
        ]);
        let id = post_session(&state);
        let prompt = "hallo welt, bitte nicht echo";
        let first = dispatch(
            "POST",
            &format!("/api/sessions/{id}/chat"),
            "",
            &format!(r#"{{"text":"{prompt}"}}"#),
            &state,
        );
        assert_eq!(
            first.status,
            200,
            "{}",
            String::from_utf8_lossy(&first.body)
        );
        let second = dispatch(
            "POST",
            &format!("/api/sessions/{id}/chat"),
            "",
            r#"{"text":"noch eine frage"}"#,
            &state,
        );
        assert_eq!(
            second.status,
            200,
            "{}",
            String::from_utf8_lossy(&second.body)
        );
        let (body, events) = fetch_events(&state, &id);
        assert_eq!(body["gap"], false);
        let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seqs={seqs:?}");
        let deltas = text_deltas(&events);
        assert_eq!(deltas, vec!["Hel", "lo", "Wor", "ld"]);
        assert_ne!(deltas.first().map(String::as_str), Some(prompt));
        assert!(!deltas.iter().any(|d| d == prompt));
        assert!(!deltas.iter().any(|d| d == "Hello" || d == "World"));
        assert!(!events.iter().any(|e| event_kind(&e["event"]) == "Done"));
        let snap: Value = serde_json::from_slice(&second.body).unwrap();
        assert_eq!(snap["done"], false);
    }

    #[test]
    fn chat_stop_reconnect_mid_turn() {
        let template = {
            let mut brain = FakeBrain::new(
                "fake",
                vec![Scenario::Stream(vec![
                    "aa".into(),
                    "bb".into(),
                    "cc".into(),
                ])],
            );
            brain.chunk_delay = Duration::from_millis(50);
            brain
        };
        let state = UiState::with_chat_factory(move |_id| Ok(Box::new(template.clone())));
        let id = post_session(&state);
        let state_thread = state.clone();
        let id_thread = id.clone();
        let worker = std::thread::spawn(move || {
            dispatch(
                "POST",
                &format!("/api/sessions/{id_thread}/chat"),
                "",
                r#"{"text":"langsam"}"#,
                &state_thread,
            )
        });
        let mut mid = None;
        for _ in 0..40 {
            let (body, events) = fetch_events(&state, &id);
            if !text_deltas(&events).is_empty() {
                assert_eq!(body["gap"], false);
                mid = Some(events);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mid = mid.expect("reconnect muss Deltas mitten im Turn sehen");
        assert!(
            !mid.iter().any(|e| event_kind(&e["event"]) == "Done"),
            "kein Done vor Stop"
        );
        let stop = dispatch("POST", &format!("/api/sessions/{id}/stop"), "", "", &state);
        assert_eq!(stop.status, 200);
        let chat_resp = worker.join().expect("chat-thread");
        assert_eq!(chat_resp.status, 200);
        let again_chat = dispatch(
            "POST",
            &format!("/api/sessions/{id}/chat"),
            "",
            r#"{"text":"zu spaet"}"#,
            &state,
        );
        assert_eq!(again_chat.status, 409);
        let (_, events) = fetch_events(&state, &id);
        let last = events.last().expect("events");
        assert_eq!(last["event"]["Done"]["status"], "cancelled");
        let deltas = text_deltas(&events);
        assert!(!deltas.iter().any(|d| d == "langsam"));
    }

    #[test]
    fn chat_echo_ist_kein_assistenten_delta() {
        let state = test_ui(vec![Scenario::Text("antwort".into(), 0)]);
        let id = post_session(&state);
        let prompt = "genau dieser prompt darf nicht als delta erscheinen";
        let chat = dispatch(
            "POST",
            &format!("/api/sessions/{id}/chat"),
            "",
            &format!(r#"{{"text":"{prompt}"}}"#),
            &state,
        );
        assert_eq!(chat.status, 200);
        let (_, events) = fetch_events(&state, &id);
        let deltas = text_deltas(&events);
        assert!(!deltas.is_empty());
        assert_ne!(deltas[0], prompt);
        assert!(!deltas.iter().any(|d| d == prompt));
        assert_eq!(deltas.concat(), "antwort");
    }
}
