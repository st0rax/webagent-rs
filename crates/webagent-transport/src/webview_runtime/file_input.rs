//! WebView2-Datei-Upload-Helfer, aus `webview_runtime.rs` ausgelagert.
//! Reine Bewegung ohne Verhaltensaenderung; Modulpfad bleibt `webview_runtime`.

use super::*;

/// After a successful `DOM.setFileInputFiles` on the FileChooserOpened node,
/// fire SPA `input`/`change` on that same node and report its `files.length`.
#[cfg(windows)]
fn spa_notify_chooser_file_input(
    webview: &wry::WebView,
    backend_node_id: i64,
    event_loop: &mut EventLoop<()>,
) -> Option<Value> {
    let object_id = resolve_backend_object_id(webview, backend_node_id, event_loop)?;
    let status = call_function_on_json(
        webview,
        &object_id,
        spa_file_input_notify_fn(),
        None,
        event_loop,
    )?;
    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        eprintln!("[upload] SPA notify on chooser backendNodeId={backend_node_id}: {status}");
    }
    Some(status)
}

/// Last resort when CDP set left the chooser FileList empty: defineProperty
/// FileList from in-memory bytes + bubble change/input on the same node.
#[cfg(windows)]
fn spa_filelist_hack_chooser(
    webview: &wry::WebView,
    backend_node_id: i64,
    files: &[(String, Vec<u8>)],
    event_loop: &mut EventLoop<()>,
) -> Option<Value> {
    // Cap payload so a huge attachment cannot stall the CDP roundtrip.
    const MAX_HACK_BYTES: usize = 2 * 1024 * 1024;
    let object_id = resolve_backend_object_id(webview, backend_node_id, event_loop)?;
    let mut payloads = Vec::with_capacity(files.len());
    let mut total = 0usize;
    for (name, data) in files {
        total = total.saturating_add(data.len());
        if total > MAX_HACK_BYTES {
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] FileList hack skipped: payload exceeds {MAX_HACK_BYTES} bytes");
            }
            return None;
        }
        payloads.push(serde_json::json!({
            "name": name,
            "mime": upload_mime_for_name(name),
            "b64": b64_encode_upload(data)
        }));
    }
    let args = serde_json::json!([{"value": payloads}]);
    let status = call_function_on_json(
        webview,
        &object_id,
        spa_filelist_hack_fn(),
        Some(args),
        event_loop,
    )?;
    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        eprintln!("[upload] FileList hack on chooser backendNodeId={backend_node_id}: {status}");
    }
    Some(status)
}

/// Vue remount: after set, the chooser input may be replaced. Watch briefly for
/// a newly mounted `input[type=file]` and re-apply setFileInputFiles + SPA events.
#[cfg(windows)]
fn remount_watch_set_file_input(
    webview: &wry::WebView,
    path_strings: &[String],
    files: &[(String, Vec<u8>)],
    event_loop: &mut EventLoop<()>,
) -> u64 {
    let find = serde_json::json!({
        "expression": r#"(() => {
          const inputs = Array.from(document.querySelectorAll('input[type=file]'));
          if (!inputs.length) return {count:0};
          const i = inputs[inputs.length - 1];
          return {
            count: inputs.length,
            files: (i.files && i.files.length) || 0,
            connected: i.isConnected !== false
          };
        })()"#,
        "returnByValue": true,
        "silent": true
    });
    let mut best_files = 0u64;
    for attempt in 0..8u32 {
        pump_once(event_loop);
        thread::sleep(Duration::from_millis(60));
        let snap = call_cdp_json(webview, "Runtime.evaluate", &find.to_string(), event_loop)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/result/value")
                    .or_else(|| value.pointer("/result/result/value"))
                    .cloned()
            })
            .unwrap_or(Value::Null);
        let count = snap.get("count").and_then(Value::as_u64).unwrap_or(0);
        let existing = snap.get("files").and_then(Value::as_u64).unwrap_or(0);
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!("[upload] remount watch attempt={attempt} snap={snap}");
        }
        if existing > 0 {
            return existing;
        }
        if count == 0 {
            continue;
        }
        // Resolve newest input via evaluate objectId, set files, notify.
        let evaluate = serde_json::json!({
            "expression": "(() => { var list=document.querySelectorAll('input[type=file]'); return list.length?list[list.length-1]:null; })()",
            "returnByValue": false,
            "silent": true
        });
        let object_id = call_cdp_json(
            webview,
            "Runtime.evaluate",
            &evaluate.to_string(),
            event_loop,
        )
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/objectId")
                .or_else(|| value.pointer("/result/result/objectId"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
        let Some(object_id) = object_id else {
            continue;
        };
        let params = serde_json::json!({
            "files": path_strings,
            "objectId": object_id
        });
        let _ = call_cdp_json(
            webview,
            "DOM.setFileInputFiles",
            &params.to_string(),
            event_loop,
        );
        let status = call_function_on_json(
            webview,
            &object_id,
            spa_file_input_notify_fn(),
            None,
            event_loop,
        );
        let after = status
            .as_ref()
            .and_then(|v| v.get("filesAfter").and_then(Value::as_u64))
            .unwrap_or(0);
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!(
                "[upload] remount re-apply set+SPA events: status={status:?} filesAfter={after}"
            );
        }
        if after == 0 {
            // Last resort on the remounted node as well.
            let mut payloads = Vec::new();
            let mut total = 0usize;
            let mut oversized = false;
            for (name, data) in files {
                total = total.saturating_add(data.len());
                if total > 2 * 1024 * 1024 {
                    oversized = true;
                    break;
                }
                payloads.push(serde_json::json!({
                    "name": name,
                    "mime": upload_mime_for_name(name),
                    "b64": b64_encode_upload(data)
                }));
            }
            if !oversized {
                let args = serde_json::json!([{"value": payloads}]);
                if let Some(hack) = call_function_on_json(
                    webview,
                    &object_id,
                    spa_filelist_hack_fn(),
                    Some(args),
                    event_loop,
                ) {
                    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                        eprintln!("[upload] remount FileList hack: {hack}");
                    }
                    best_files = hack
                        .get("files")
                        .and_then(Value::as_u64)
                        .unwrap_or(0)
                        .max(best_files);
                }
            }
        } else {
            best_files = after.max(best_files);
        }
        if best_files > 0 {
            return best_files;
        }
    }
    best_files
}

#[cfg(windows)]
/// Übergibt Dateien über WebView2s vertrauenswürdigen CDP-Kanal. Eine
/// `DataTransfer`-Zuweisung aus JS sieht für manche SPAs zwar wie ein FileList
/// aus, wird vom Upload-Backend aber als synthetisch verworfen. `DOM.setFileInputFiles`
/// ist derselbe native Pfad, den DevTools/Playwright verwenden.
pub(super) fn set_file_input_files_cdp(
    webview: &wry::WebView,
    files: &[(String, Vec<u8>)],
    event_loop: &mut EventLoop<()>,
) -> Result<Vec<PathBuf>> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("webagent-upload-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| {
        PageDriverError::Protocol(format!(
            "Upload-Temporaerverzeichnis {}: {e}",
            dir.display()
        ))
    })?;
    let mut paths = Vec::with_capacity(files.len());
    for (index, (name, data)) in files.iter().enumerate() {
        let safe = Path::new(name)
            .file_name()
            .and_then(|part| part.to_str())
            .filter(|part| !part.is_empty())
            .unwrap_or("upload.bin");
        let path = dir.join(format!("{index}-{safe}"));
        if let Err(error) = fs::write(&path, data) {
            let _ = fs::remove_dir_all(&dir);
            return Err(PageDriverError::Protocol(format!(
                "Upload-Datei {}: {error}",
                path.display()
            )));
        }
        paths.push(path);
    }

    if let Err(error) = call_cdp_json(webview, "DOM.enable", "{}", event_loop) {
        let _ = fs::remove_dir_all(&dir);
        return Err(error);
    }
    let path_strings: Vec<String> = paths
        .iter()
        // WebView2's CDP implementation expects POSIX separators even on
        // Windows.  Backslashes are accepted by the JSON parser but are
        // rejected by the underlying file-input bridge on some runtimes.
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();

    // Ask Chromium to create a real file-chooser transaction, but intercept
    // it before a native Windows dialog is shown.  `DOM.setFileInputFiles`
    // is reliable against the backendNodeId from this event; setting an
    // arbitrary hidden input directly is acknowledged as `{}` by some
    // WebView2 runtimes without changing its FileList.
    let chooser_backend_node_id = match open_intercepted_file_chooser_cdp(webview, event_loop) {
        Ok(id) => Some(id),
        Err(error) => {
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] intercepted file chooser unavailable: {error}");
            }
            None
        }
    };

    // Live TRACE (qwen #46): Page.fileChooserOpened reported backendNodeId=21,
    // but a subsequent DOM.querySelector("input[type=file]") resolved a
    // *different* node (backend=3144). setFileInputFiles on the wrong node
    // returned {} while input.files stayed 0. Also, getDocument(depth:-1) +
    // querySelector between the event and the set can invalidate the
    // intercepted chooser transaction.
    //
    // Rule: when FileChooserOpened supplies a backendNodeId, call
    // DOM.setFileInputFiles with THAT id immediately — never replace it with
    // a querySelector'd input.
    let mut used_chooser_node = false;
    let mut chooser_files_len: Option<u64> = None;
    let mut spa_events_dispatched = false;
    if let Some(backend_node_id) = chooser_backend_node_id {
        // Only the FileChooserOpened backendNodeId — no querySelector, no
        // getDocument, no resolveNode delay before the set.
        let params = serde_json::json!({
            "files": path_strings.clone(),
            "backendNodeId": backend_node_id
        });
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!(
                "[upload] DOM.setFileInputFiles via FileChooserOpened backendNodeId={backend_node_id} params={params}"
            );
        }
        match call_cdp_json(
            webview,
            "DOM.setFileInputFiles",
            &params.to_string(),
            event_loop,
        ) {
            Ok(cdp_response) => {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!("[upload] DOM.setFileInputFiles response: {cdp_response}");
                }
                if let Some(error) = cdp_response
                    .get("error")
                    .or_else(|| cdp_response.pointer("/result/error"))
                {
                    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                        eprintln!(
                            "[upload] DOM.setFileInputFiles on chooser backendNodeId={backend_node_id} error: {error}"
                        );
                    }
                } else {
                    used_chooser_node = true;
                    // SPA-side: WebView2 may accept setFileInputFiles without
                    // firing change/input. Notify THAT chooser node immediately
                    // and re-read files.length on the same backendNodeId.
                    if let Some(status) =
                        spa_notify_chooser_file_input(webview, backend_node_id, event_loop)
                    {
                        spa_events_dispatched = status
                            .get("events")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let files_after = status
                            .get("filesAfter")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        let files_before = status
                            .get("filesBefore")
                            .and_then(Value::as_u64)
                            .unwrap_or(0);
                        let connected = status
                            .get("connected")
                            .and_then(Value::as_bool)
                            .unwrap_or(true);
                        chooser_files_len = Some(files_after);
                        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                            eprintln!(
                                "[upload] chooser node files.length before={files_before} after={files_after} events={spa_events_dispatched} connected={connected}"
                            );
                        }
                        // CDP set left FileList empty → last-resort Vue FileList hack.
                        if files_after == 0 {
                            if let Some(hack) = spa_filelist_hack_chooser(
                                webview,
                                backend_node_id,
                                files,
                                event_loop,
                            ) {
                                let hacked = hack.get("files").and_then(Value::as_u64).unwrap_or(0);
                                chooser_files_len = Some(hacked);
                                spa_events_dispatched = true;
                            }
                        }
                        // Input removed/empty after set: watch for remount and re-apply.
                        let still_empty = chooser_files_len.unwrap_or(0) == 0;
                        if still_empty || !connected {
                            let remounted = remount_watch_set_file_input(
                                webview,
                                &path_strings,
                                files,
                                event_loop,
                            );
                            if remounted > 0 {
                                chooser_files_len = Some(remounted);
                                spa_events_dispatched = true;
                            }
                        }
                    } else if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                        eprintln!(
                            "[upload] SPA notify failed to resolve chooser backendNodeId={backend_node_id}"
                        );
                        // Still try remount path — node may already be gone.
                        let remounted =
                            remount_watch_set_file_input(webview, &path_strings, files, event_loop);
                        if remounted > 0 {
                            chooser_files_len = Some(remounted);
                            spa_events_dispatched = true;
                        }
                    }
                }
            }
            Err(error) => {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!(
                        "[upload] DOM.setFileInputFiles on chooser backendNodeId={backend_node_id} failed: {error}"
                    );
                }
            }
        }
        let _ = call_cdp_json(
            webview,
            "Page.setInterceptFileChooserDialog",
            r#"{"enabled":false}"#,
            event_loop,
        );
    }

    // querySelector path only when no chooser event (or chooser set failed).
    if !used_chooser_node {
        let document = match call_cdp_json(
            webview,
            "DOM.getDocument",
            r#"{"depth":-1,"pierce":true}"#,
            event_loop,
        ) {
            Ok(value) => value,
            Err(error) => {
                // Still try drag before failing closed.
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!(
                        "[upload] DOM.getDocument failed ({error}); trying trusted drag fallback"
                    );
                }
                match dispatch_file_drag_cdp(webview, files, &path_strings, event_loop) {
                    Ok(()) => return Ok(paths),
                    Err(drag_error) => {
                        let _ = fs::remove_dir_all(&dir);
                        return Err(PageDriverError::Protocol(format!(
                            "DOM.setFileInputFiles: getDocument={error}; drag fallback: {drag_error}"
                        )));
                    }
                }
            }
        };
        let root_node_id = document
            .pointer("/root/nodeId")
            .or_else(|| document.pointer("/result/root/nodeId"))
            .and_then(Value::as_i64)
            .filter(|id| *id > 0)
            .ok_or_else(|| {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!("[upload] DOM.getDocument response: {document}");
                }
                let _ = fs::remove_dir_all(&dir);
                PageDriverError::Protocol(
                    "DOM.setFileInputFiles: kein DOM-Dokument gefunden".into(),
                )
            })?;
        let query = serde_json::json!({
            "nodeId": root_node_id,
            "selector": "input[type=file]"
        });
        let found =
            match call_cdp_json(webview, "DOM.querySelector", &query.to_string(), event_loop) {
                Ok(value) => value,
                Err(error) => {
                    let _ = fs::remove_dir_all(&dir);
                    return Err(error);
                }
            };
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!("[upload] DOM.querySelector response (no/failed chooser path): {found}");
        }
        let node_id = found
            .pointer("/nodeId")
            .or_else(|| found.pointer("/result/nodeId"))
            .and_then(Value::as_i64)
            .filter(|id| *id > 0);
        let Some(node_id) = node_id else {
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] no file input node; trying trusted drag fallback");
            }
            match dispatch_file_drag_cdp(webview, files, &path_strings, event_loop) {
                Ok(()) => return Ok(paths),
                Err(error) => {
                    let _ = fs::remove_dir_all(&dir);
                    return Err(PageDriverError::Protocol(format!(
                        "DOM.setFileInputFiles: kein input[type=file]-Element gefunden; drag fallback: {error}"
                    )));
                }
            }
        };
        let describe = serde_json::json!({"nodeId": node_id});
        let backend_node_id = call_cdp_json(
            webview,
            "DOM.describeNode",
            &describe.to_string(),
            event_loop,
        )
        .ok()
        .and_then(|value| {
            value
                .pointer("/node/backendNodeId")
                .or_else(|| value.pointer("/result/node/backendNodeId"))
                .and_then(Value::as_i64)
                .filter(|id| *id > 0)
        });
        let evaluate = serde_json::json!({
            "expression": "document.querySelector('input[type=file]')",
            "returnByValue": false,
            "silent": true
        });
        let object_id = call_cdp_json(
            webview,
            "Runtime.evaluate",
            &evaluate.to_string(),
            event_loop,
        )
        .ok()
        .and_then(|value| {
            value
                .pointer("/result/objectId")
                .or_else(|| value.pointer("/result/result/objectId"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        });
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!(
                "[upload] DOM target (querySelector fallback) node={node_id} backend={backend_node_id:?} object={object_id:?}"
            );
        }
        let mut params = serde_json::json!({"files": path_strings.clone()});
        if let Some(ref object_id) = object_id {
            params["objectId"] = Value::String(object_id.clone());
        } else {
            params["nodeId"] = Value::from(node_id);
            if let Some(backend_node_id) = backend_node_id {
                params["backendNodeId"] = Value::from(backend_node_id);
            }
        }
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!("[upload] DOM.setFileInputFiles (querySelector fallback) params={params}");
        }
        let cdp_response = match call_cdp_json(
            webview,
            "DOM.setFileInputFiles",
            &params.to_string(),
            event_loop,
        ) {
            Ok(response) => response,
            Err(error) => {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!(
                        "[upload] querySelector setFileInputFiles failed ({error}); trying drag"
                    );
                }
                match dispatch_file_drag_cdp(webview, files, &path_strings, event_loop) {
                    Ok(()) => return Ok(paths),
                    Err(drag_error) => {
                        let _ = fs::remove_dir_all(&dir);
                        return Err(PageDriverError::Protocol(format!(
                            "DOM.setFileInputFiles: {error}; drag fallback: {drag_error}"
                        )));
                    }
                }
            }
        };
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!("[upload] DOM.setFileInputFiles response: {cdp_response}");
        }
        if let Some(error) = cdp_response
            .get("error")
            .or_else(|| cdp_response.pointer("/result/error"))
        {
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] querySelector setFileInputFiles error {error}; trying drag");
            }
            match dispatch_file_drag_cdp(webview, files, &path_strings, event_loop) {
                Ok(()) => return Ok(paths),
                Err(drag_error) => {
                    let _ = fs::remove_dir_all(&dir);
                    return Err(PageDriverError::Protocol(format!(
                        "DOM.setFileInputFiles: {error}; drag fallback: {drag_error}"
                    )));
                }
            }
        } else if let Some(object_id) = object_id.as_ref() {
            // Mirror chooser path: bubble input/change on the node we just set.
            if let Some(status) = call_function_on_json(
                webview,
                object_id,
                spa_file_input_notify_fn(),
                None,
                event_loop,
            ) {
                spa_events_dispatched = status
                    .get("events")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                chooser_files_len = status
                    .get("filesAfter")
                    .and_then(Value::as_u64)
                    .or(chooser_files_len);
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!("[upload] SPA notify on querySelector node: {status}");
                }
            }
        }
    }

    // Empty `{}` from setFileInputFiles is NOT proof the FileList was mutated.
    // Prefer chooser-node files.length (same backendNodeId); fall back to a
    // querySelector sum. If still 0, run trusted drag with real file payload.
    let query_count = {
        let check = serde_json::json!({
            "expression": "Array.from(document.querySelectorAll('input[type=file]')).reduce((n,i)=>n+(i.files?i.files.length:0),0)",
            "returnByValue": true,
            "silent": true
        });
        call_cdp_json(webview, "Runtime.evaluate", &check.to_string(), event_loop)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/result/value")
                    .or_else(|| value.pointer("/result/result/value"))
                    .and_then(Value::as_u64)
            })
            .unwrap_or(0)
    };
    let native_count = chooser_files_len.unwrap_or(0).max(query_count);
    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        eprintln!(
            "[upload] files proof: chooser_node={chooser_files_len:?} querySelector_sum={query_count} spa_events={spa_events_dispatched} chooser_used={used_chooser_node}"
        );
    }
    if native_count == 0 {
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
            eprintln!(
                "[upload] native file-input count=0 after set (chooser_used={used_chooser_node}); running trusted drag fallback"
            );
        }
        match dispatch_file_drag_cdp(webview, files, &path_strings, event_loop) {
            Ok(()) => {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!("[upload] trusted drag fallback dispatched");
                }
            }
            Err(error) => {
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!("[upload] trusted drag fallback unavailable: {error}");
                }
            }
        }
    } else if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        eprintln!("[upload] native file-input count={native_count}; skip drag fallback");
    }
    Ok(paths)
}

#[cfg(windows)]
fn open_intercepted_file_chooser_cdp(
    webview: &wry::WebView,
    event_loop: &mut EventLoop<()>,
) -> Result<i64> {
    use std::sync::mpsc;
    use webview2_com::{CoTaskMemPWSTR, DevToolsProtocolEventReceivedEventHandler};
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::System::WinRT::EventRegistrationToken;
    use wry::WebViewExtWindows;

    let proto = |message: String| PageDriverError::Protocol(message);
    unsafe {
        let core = webview
            .controller()
            .CoreWebView2()
            .map_err(|error| proto(error.to_string()))?;
        let event_name = HSTRING::from("Page.fileChooserOpened");
        let receiver = core
            .GetDevToolsProtocolEventReceiver(&event_name)
            .map_err(|error| proto(error.to_string()))?;
        let (event_tx, event_rx) = mpsc::channel();
        let handler =
            DevToolsProtocolEventReceivedEventHandler::create(Box::new(move |_sender, args| {
                if let Some(args) = args {
                    let mut raw = PWSTR::null();
                    if args.ParameterObjectAsJson(&mut raw).is_ok() {
                        let json = CoTaskMemPWSTR::from(raw).to_string();
                        let _ = event_tx.send(json);
                    }
                }
                Ok(())
            }));
        let mut token = EventRegistrationToken::default();
        receiver
            .add_DevToolsProtocolEventReceived(&handler, &mut token)
            .map_err(|error| proto(error.to_string()))?;

        let operation = (|| -> Result<i64> {
            call_cdp_json(
                webview,
                "Page.enable",
                r#"{"enableFileChooserOpenedEvent":true}"#,
                event_loop,
            )?;
            call_cdp_json(
                webview,
                "Page.setInterceptFileChooserDialog",
                r#"{"enabled":true}"#,
                event_loop,
            )?;
            // Prefer an existing file input; otherwise click an upload/attach
            // control (qwen/zai mount the chooser from a toolbar button, not a
            // pre-existing input). Fall back to a trusted CDP pointer click on
            // the resolved coordinates when DOM click alone is insufficient.
            let locate = serde_json::json!({
                "expression": r#"(() => {
                    const visible = (el) => {
                        if (!el) return false;
                        const r = el.getBoundingClientRect(), s = getComputedStyle(el);
                        return r.width > 0 && r.height > 0 && s.display !== 'none' && s.visibility !== 'hidden';
                    };
                    const inputs = Array.from(document.querySelectorAll('input[type=file]'));
                    // Prefer the last-mounted input: SPAs (qwen/zai) often leave
                    // stale file inputs ahead of the live chooser target. Live
                    // TRACE showed FileChooserOpened backendNodeId differing
                    // from document.querySelector('input[type=file]').
                    for (const i of inputs.slice().reverse()) {
                        try { i.click(); return {ok:true, via:'input', inputCount:inputs.length}; } catch (e) {}
                    }
                    for (const l of document.querySelectorAll('label')) {
                        const forId = l.getAttribute('for');
                        const target = forId ? document.getElementById(forId) : null;
                        if ((target && target.type === 'file') || l.querySelector('input[type=file]')) {
                            if (!visible(l)) continue;
                            const r = l.getBoundingClientRect();
                            // Coordinates only — trusted CDP click opens the chooser once.
                            return {ok:true, via:'label', x:r.left+r.width/2, y:r.top+r.height/2};
                        }
                    }
                    const re = /upload|attach|anhang|datei|file|paperclip|image|图片|上传|附件|bild/i;
                    const nodes = Array.from(document.querySelectorAll(
                        'button,[role="button"],label,a,[aria-label],div[role="button"]'));
                    const scored = [];
                    for (const el of nodes) {
                        if (!visible(el)) continue;
                        const hay = (
                            (el.getAttribute('aria-label') || '') + ' ' +
                            (el.getAttribute('title') || '') + ' ' +
                            (el.getAttribute('data-tooltip') || '') + ' ' +
                            (el.innerText || '')
                        ).trim();
                        if (!re.test(hay) && !el.querySelector('input[type=file]')) continue;
                        const r = el.getBoundingClientRect();
                        // Prefer composer-band controls; skip top chrome.
                        const band = r.top >= window.innerHeight * 0.35 ? 0 : 2;
                        scored.push({el, hay, r, band});
                    }
                    scored.sort((a, b) => a.band - b.band || b.r.width * b.r.height - a.r.width * a.r.height);
                    for (const item of scored) {
                        return {
                            ok: true,
                            via: 'attach',
                            text: item.hay.slice(0, 80),
                            x: item.r.left + item.r.width / 2,
                            y: item.r.top + item.r.height / 2
                        };
                    }
                    return {ok:false};
                })()"#,
                "returnByValue": true,
                "userGesture": true,
                "silent": true
            });
            let located =
                call_cdp_json(webview, "Runtime.evaluate", &locate.to_string(), event_loop)?;
            let locate_value = located
                .pointer("/result/value")
                .or_else(|| located.pointer("/result/result/value"))
                .cloned()
                .unwrap_or(Value::Null);
            let did_click = locate_value
                .get("ok")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] file-chooser trigger: {locate_value}");
            }
            if !did_click {
                return Err(PageDriverError::Protocol(
                    "kein input/attach-Trigger fuer File-Chooser-Interception".into(),
                ));
            }
            // Attach/label triggers need a trusted pointer; input[type=file]
            // already received an in-page .click() above.
            if let (Some(x), Some(y)) = (
                locate_value.get("x").and_then(Value::as_f64),
                locate_value.get("y").and_then(Value::as_f64),
            ) {
                if let Err(error) = click_at_trusted_cdp(webview, x, y, event_loop) {
                    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                        eprintln!("[upload] trusted attach click failed: {error}");
                    }
                }
            }

            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                match event_rx.try_recv() {
                    Ok(payload) => {
                        let value: Value = serde_json::from_str(&payload).map_err(|error| {
                            PageDriverError::Protocol(format!(
                                "Page.fileChooserOpened: ungueltiges JSON: {error}"
                            ))
                        })?;
                        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                            eprintln!("[upload] Page.fileChooserOpened: {value}");
                        }
                        return value
                            .get("backendNodeId")
                            .and_then(Value::as_i64)
                            .filter(|id| *id > 0)
                            .ok_or_else(|| {
                                PageDriverError::Protocol(
                                    "Page.fileChooserOpened ohne backendNodeId".into(),
                                )
                            });
                    }
                    Err(mpsc::TryRecvError::Empty) if Instant::now() < deadline => {
                        pump_once(event_loop);
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        return Err(PageDriverError::Timeout(
                            "Page.fileChooserOpened blieb aus".into(),
                        ));
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(PageDriverError::Protocol(
                            "Page.fileChooserOpened-Kanal geschlossen".into(),
                        ));
                    }
                }
            }
        })();

        let _ = receiver.remove_DevToolsProtocolEventReceived(token);
        if operation.is_err() {
            let _ = call_cdp_json(
                webview,
                "Page.setInterceptFileChooserDialog",
                r#"{"enabled":false}"#,
                event_loop,
            );
        }
        operation
    }
}

#[cfg(windows)]
fn dispatch_file_drag_cdp(
    webview: &wry::WebView,
    files: &[(String, Vec<u8>)],
    path_strings: &[String],
    event_loop: &mut EventLoop<()>,
) -> Result<()> {
    if files.is_empty() || path_strings.is_empty() {
        return Ok(());
    }
    let point_request = serde_json::json!({
        "expression": drag_drop_target_expression(),
        "returnByValue": true,
        "silent": true
    });
    let point_value = call_cdp_json(
        webview,
        "Runtime.evaluate",
        &point_request.to_string(),
        event_loop,
    )
    .ok()
    .and_then(|value| {
        value
            .pointer("/result/value")
            .or_else(|| value.pointer("/result/result/value"))
            .cloned()
    })
    .unwrap_or(Value::Null);
    let point = point_value
        .as_object()
        .and_then(|obj| {
            Some((
                obj.get("x")?.as_f64()?,
                obj.get("y")?.as_f64()?,
                obj.get("via")
                    .and_then(Value::as_str)
                    .unwrap_or("composer")
                    .to_string(),
            ))
        })
        .ok_or_else(|| {
            PageDriverError::Protocol("kein sichtbarer Composer fuer Drag/Drop".into())
        })?;
    // Real CDP drag payload: absolute paths in `files` plus MIME/title items
    // matching Input.DragData (empty item data previously looked like a no-op).
    let items: Vec<Value> = files
        .iter()
        .zip(path_strings.iter())
        .map(|((name, _), path)| {
            serde_json::json!({
                "mimeType": upload_mime_for_name(name),
                "data": path,
                "title": name
            })
        })
        .collect();
    let drag_data = serde_json::json!({
        "items": items,
        "files": path_strings,
        "dragOperationsMask": 1
    });
    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        eprintln!(
            "[upload] drag target {} at ({:.1},{:.1}) files={} payload_files={} payload_items={}",
            point.2,
            point.0,
            point.1,
            path_strings.len(),
            path_strings.len(),
            items.len()
        );
    }
    for event_type in ["dragEnter", "dragOver", "drop"] {
        let params = serde_json::json!({
            "type": event_type,
            "x": point.0,
            "y": point.1,
            "data": drag_data
        });
        call_cdp_json(
            webview,
            "Input.dispatchDragEvent",
            &params.to_string(),
            event_loop,
        )?;
    }
    // Post-drag signal: files.length and any visible attachment card count.
    if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
        let proof = serde_json::json!({
            "expression": r#"(() => {
              var files = Array.from(document.querySelectorAll('input[type=file]')).reduce((n,i)=>n+(i.files?i.files.length:0),0);
              var signal = document.querySelectorAll('[class*="attachment" i],[class*="file-preview" i],[class*="image-thumbnail" i],img[src^="blob:"]').length;
              return {files: files, signal: signal};
            })()"#,
            "returnByValue": true,
            "silent": true
        });
        if let Ok(value) =
            call_cdp_json(webview, "Runtime.evaluate", &proof.to_string(), event_loop)
        {
            let snap = value
                .pointer("/result/value")
                .or_else(|| value.pointer("/result/result/value"))
                .cloned()
                .unwrap_or(Value::Null);
            eprintln!("[upload] post-drag signal: {snap}");
        }
    }
    Ok(())
}

#[cfg(windows)]
fn upload_mime_for_name(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("wav") => "audio/wav",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("ogg") => "audio/ogg",
        Some("webm") => "audio/webm",
        _ => "application/octet-stream",
    }
}

#[cfg(not(windows))]
pub(super) fn set_file_input_files_cdp(
    _webview: &wry::WebView,
    _files: &[(String, Vec<u8>)],
    _event_loop: &mut EventLoop<()>,
) -> Result<Vec<PathBuf>> {
    Err(PageDriverError::NotAvailable(
        "Datei-Upload über CDP ist unter Linux nicht verfügbar".into(),
    ))
}
