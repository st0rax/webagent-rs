//! Embedded WebView (wry/tao) — dedizierter UI-Event-Loop-Thread, sync API via mpsc.
//!
//! Ersetzt Chrome+CDP: ein verstecktes Fenster (`with_visible(false)`) pro Tab,
//! Steuerung über [`PageDriver`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::platform::run_return::EventLoopExtRunReturn;
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use crate::page_driver::{PageDriver, PageDriverError, Result};

type ViewId = u64;

/// Interne Befehle an den UI-Thread.
enum RuntimeMessage {
    OpenPage {
        profile_dir: PathBuf,
        url: String,
        headless: bool,
        respond: Sender<Result<(ViewId, WebViewPageDriver)>>,
    },
    ClosePage {
        view_id: ViewId,
        respond: Sender<Result<()>>,
    },
    Shutdown,
}

/// Befehle eines einzelnen Tabs (vom Agenten-Thread).
enum PageMessage {
    Evaluate {
        expression: String,
        respond: Sender<Result<Value>>,
    },
    Navigate {
        url: String,
        timeout: Duration,
        respond: Sender<Result<()>>,
    },
    CurrentUrl {
        respond: Sender<Result<String>>,
    },
    PressKey {
        key: String,
        code: String,
        virtual_key: i64,
        text: String,
        respond: Sender<Result<()>>,
    },
    InsertText {
        text: String,
        respond: Sender<Result<()>>,
    },
    ClickAt {
        x: f64,
        y: f64,
        respond: Sender<Result<()>>,
    },
}

struct PageSlot {
    page_rx: Receiver<PageMessage>,
    window: tao::window::Window,
    webview: wry::WebView,
}

struct SharedRuntime {
    cmd_rx: Receiver<RuntimeMessage>,
    pages: HashMap<ViewId, PageSlot>,
    next_id: ViewId,
    web_context: Option<wry::WebContext>,
}

/// Laufzeit mit UI-Event-Loop (ein Prozess, mehrere versteckte Tabs möglich).
pub struct WebViewRuntime {
    tx: Sender<RuntimeMessage>,
    thread: Option<JoinHandle<()>>,
}

impl WebViewRuntime {
    /// Startet den UI-Thread.
    pub fn launch(profile_dir: &Path, headless: bool) -> Result<Self> {
        let profile_dir = profile_dir.to_path_buf();
        std::fs::create_dir_all(&profile_dir).map_err(|e| {
            PageDriverError::Launch(format!("Profilverzeichnis: {e}"))
        })?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<RuntimeMessage>();
        let cmd_tx_thread = cmd_tx.clone();

        let handle = thread::Builder::new()
            .name("webagent-webview".into())
            .spawn(move || run_event_loop(cmd_rx, profile_dir, headless))
            .map_err(|e| PageDriverError::Launch(e.to_string()))?;

        thread::sleep(Duration::from_millis(80));

        Ok(Self {
            tx: cmd_tx_thread,
            thread: Some(handle),
        })
    }

    /// Öffnet einen neuen Tab und liefert einen [`WebViewPageDriver`].
    pub fn open_page(
        &self,
        profile_dir: &Path,
        url: &str,
        headless: bool,
    ) -> Result<WebViewPageDriver> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.tx
            .send(RuntimeMessage::OpenPage {
                profile_dir: profile_dir.to_path_buf(),
                url: url.to_string(),
                headless,
                respond: resp_tx,
            })
            .map_err(|_| PageDriverError::Launch("WebView-Thread beendet".into()))?;
        self.wake_and_wait(resp_rx, Duration::from_secs(60))
    }

    /// Schließt einen Tab.
    pub fn close_page(&self, view_id: ViewId) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.tx
            .send(RuntimeMessage::ClosePage {
                view_id,
                respond: resp_tx,
            })
            .map_err(|_| PageDriverError::Protocol("WebView-Thread beendet".into()))?;
        self.wake_and_wait(resp_rx, Duration::from_secs(15))
    }

    fn wake_and_wait<T>(&self, resp_rx: Receiver<Result<T>>, timeout: Duration) -> Result<T> {
        match resp_rx.recv_timeout(timeout) {
            Ok(inner) => inner,
            Err(_) => Err(PageDriverError::Timeout(
                "WebView-Befehl nicht rechtzeitig beantwortet".into(),
            )),
        }
    }
}

impl Drop for WebViewRuntime {
    fn drop(&mut self) {
        let _ = self.tx.send(RuntimeMessage::Shutdown);
        if let Some(h) = self.thread.take() {
            let _ = h.join();
        }
    }
}

/// Konkreter Page-Driver über den WebView-Thread.
#[derive(Clone)]
pub struct WebViewPageDriver {
    view_id: ViewId,
    page_tx: Sender<PageMessage>,
}

impl WebViewPageDriver {
    pub fn view_id(&self) -> ViewId {
        self.view_id
    }

    fn call<T>(&self, build: impl FnOnce(Sender<Result<T>>) -> PageMessage) -> Result<T> {
        let (tx, rx) = mpsc::channel();
        let msg = build(tx);
        self.page_tx
            .send(msg)
            .map_err(|_| PageDriverError::Protocol("WebView-Tab beendet".into()))?;
        rx.recv_timeout(Duration::from_secs(45))
            .map_err(|_| PageDriverError::Timeout("Page-Befehl timeout".into()))?
    }
}

impl PageDriver for WebViewPageDriver {
    fn evaluate(&mut self, expression: &str) -> Result<Value> {
        self.call(|respond| PageMessage::Evaluate {
            expression: expression.to_string(),
            respond,
        })
    }

    fn navigate(&mut self, url: &str, timeout: Duration) -> Result<()> {
        self.call(|respond| PageMessage::Navigate {
            url: url.to_string(),
            timeout,
            respond,
        })
    }

    fn current_url(&mut self) -> Result<String> {
        self.call(|respond| PageMessage::CurrentUrl { respond })
    }

    fn press_key(&mut self, key: &str, code: &str, virtual_key: i64, text: &str) -> Result<()> {
        self.call(|respond| PageMessage::PressKey {
            key: key.to_string(),
            code: code.to_string(),
            virtual_key,
            text: text.to_string(),
            respond,
        })
    }

    fn insert_text(&mut self, text: &str) -> Result<()> {
        self.call(|respond| PageMessage::InsertText {
            text: text.to_string(),
            respond,
        })
    }

    fn click_at(&mut self, x: f64, y: f64) -> Result<()> {
        self.call(|respond| PageMessage::ClickAt { x, y, respond })
    }
}

fn run_event_loop(cmd_rx: Receiver<RuntimeMessage>, _default_profile: PathBuf, _default_headless: bool) {
    let mut event_loop: EventLoop<()> = EventLoop::new();

    let mut rt = SharedRuntime {
        cmd_rx,
        pages: HashMap::new(),
        next_id: 1,
        web_context: None,
    };

    let mut shutdown = false;
    while !shutdown {
        shutdown = pump_runtime(&mut rt, &event_loop);

        let _ = event_loop.run_return(|event, _, control_flow| {
            *control_flow = ControlFlow::Exit;
            if let Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } = &event
            {
                // Fenster-X schließt nur den Tab, nicht den ganzen Agenten-Loop.
            }
        });
    }
}

fn pump_runtime(rt: &mut SharedRuntime, event_loop: &EventLoop<()>) -> bool {
    while let Ok(msg) = rt.cmd_rx.try_recv() {
        match msg {
            RuntimeMessage::Shutdown => return true,
            RuntimeMessage::OpenPage {
                profile_dir,
                url,
                headless,
                respond,
            } => {
                let result = open_page(rt, event_loop, &profile_dir, &url, headless);
                let _ = respond.send(result);
            }
            RuntimeMessage::ClosePage { view_id, respond } => {
                let result = close_page(rt, view_id);
                let _ = respond.send(result);
            }
        }
    }

    let view_ids: Vec<ViewId> = rt.pages.keys().copied().collect();
    for vid in view_ids {
        let mut pending: Vec<PageMessage> = Vec::new();
        if let Some(slot) = rt.pages.get(&vid) {
            while let Ok(msg) = slot.page_rx.try_recv() {
                pending.push(msg);
            }
        }
        if pending.is_empty() {
            continue;
        }
        let mut slot = rt.pages.remove(&vid).expect("slot");
        for msg in pending {
            dispatch_page(&mut slot, msg, event_loop);
        }
        rt.pages.insert(vid, slot);
    }

    false
}

fn open_page(
    rt: &mut SharedRuntime,
    event_loop: &EventLoop<()>,
    profile_dir: &Path,
    url: &str,
    headless: bool,
) -> Result<(ViewId, WebViewPageDriver)> {
    std::fs::create_dir_all(profile_dir).map_err(|e| {
        PageDriverError::Launch(format!("Profilverzeichnis: {e}"))
    })?;

    if rt.web_context.is_none() {
        rt.web_context = Some(wry::WebContext::new(Some(profile_dir.to_path_buf())));
    }

    let view_id = rt.next_id;
    rt.next_id += 1;

    let window = WindowBuilder::new()
        .with_title(format!("webagent-{view_id}"))
        .with_inner_size(LogicalSize::new(1280.0, 900.0))
        .with_visible(!headless)
        .build(event_loop)
        .map_err(|e| PageDriverError::Launch(e.to_string()))?;

    let mut web_context = rt
        .web_context
        .take()
        .ok_or_else(|| PageDriverError::Launch("WebContext fehlt".into()))?;

    let init_script = r#"
Object.defineProperty(navigator, 'webdriver', { get: function() { return undefined; } });
"#;

    let webview = WebViewBuilder::new()
        .with_web_context(&mut web_context)
        .with_visible(!headless)
        .with_initialization_script(init_script)
        .with_url(url)
        .build(&window)
        .map_err(|e| PageDriverError::Launch(e.to_string()))?;

    rt.web_context = Some(web_context);

    let (page_tx, page_rx) = mpsc::channel();
    rt.pages.insert(
        view_id,
        PageSlot {
            page_rx,
            window,
            webview,
        },
    );

    let driver = WebViewPageDriver { view_id, page_tx };

    Ok((view_id, driver))
}

fn close_page(rt: &mut SharedRuntime, view_id: ViewId) -> Result<()> {
    rt.pages.remove(&view_id);
    Ok(())
}

fn dispatch_page(
    slot: &mut PageSlot,
    msg: PageMessage,
    event_loop: &EventLoop<()>,
) {
    match msg {
        PageMessage::Evaluate { expression, respond } => {
            let r = eval_js(&slot.webview, &expression, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::Navigate { url, timeout, respond } => {
            let r = navigate_url(&slot.webview, &url, timeout, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::CurrentUrl { respond } => {
            let r = current_url(&slot.webview);
            let _ = respond.send(r);
        }
        PageMessage::PressKey {
            key,
            code,
            virtual_key,
            text,
            respond,
        } => {
            let r = press_key_js(&slot.webview, &key, &code, virtual_key, &text, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::InsertText { text, respond } => {
            let r = insert_text_js(&slot.webview, &text, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::ClickAt { x, y, respond } => {
            let r = click_at_js(&slot.webview, x, y, event_loop);
            let _ = respond.send(r);
        }
    }
}

fn wrap_eval(expression: &str) -> String {
    format!(
        r#"(async function(){{try{{var __v=await({expression});return JSON.stringify({{ok:true,value:__v}});}}catch(e){{return JSON.stringify({{ok:false,error:String(e)}});}}}})()"#
    )
}

fn parse_eval_result(raw: String) -> Result<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "null" || trimmed == "undefined" {
        return Ok(Value::Null);
    }
    let v: Value = serde_json::from_str(trimmed)
        .map_err(|e| PageDriverError::Protocol(format!("JSON-Parse: {e} ({trimmed})")))?;
    if let Some(ok) = v.get("ok").and_then(|x| x.as_bool()) {
        if ok {
            return Ok(v.get("value").cloned().unwrap_or(Value::Null));
        }
        let err = v
            .get("error")
            .and_then(|x| x.as_str())
            .unwrap_or("JS-Fehler");
        return Err(PageDriverError::Protocol(format!("JS-Ausnahme: {err}")));
    }
    Ok(v)
}

fn pump_once(event_loop: &EventLoop<()>) {
    let _ = event_loop.run_return(|_, _, control_flow| {
        *control_flow = ControlFlow::Exit;
    });
}

fn eval_js(
    webview: &wry::WebView,
    expression: &str,
    event_loop: &EventLoop<()>,
) -> Result<Value> {
    let (tx, rx) = mpsc::channel();
    let js = wrap_eval(expression);
    webview
        .evaluate_script_with_callback(&js, move |result| {
            let _ = tx.send(result);
        })
        .map_err(|e| PageDriverError::Protocol(e.to_string()))?;
    let deadline = Instant::now() + Duration::from_secs(35);
    let raw = loop {
        if let Ok(r) = rx.try_recv() {
            break r;
        }
        if Instant::now() >= deadline {
            return Err(PageDriverError::Timeout("evaluate timeout".into()));
        }
        pump_once(event_loop);
        thread::sleep(Duration::from_millis(5));
    };
    parse_eval_result(raw)
}

fn navigate_url(
    webview: &wry::WebView,
    url: &str,
    timeout: Duration,
    event_loop: &EventLoop<()>,
) -> Result<()> {
    webview
        .load_url(url)
        .map_err(|e| PageDriverError::Protocol(e.to_string()))?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match eval_js(webview, "document.readyState", event_loop) {
            Ok(v) if v.as_str() == Some("complete") || v.as_str() == Some("interactive") => {
                return Ok(());
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}

fn current_url(webview: &wry::WebView) -> Result<String> {
    webview
        .url()
        .map_err(|e| PageDriverError::Protocol(e.to_string()))
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn press_key_js(
    webview: &wry::WebView,
    key: &str,
    code: &str,
    virtual_key: i64,
    text: &str,
    event_loop: &EventLoop<()>,
) -> Result<()> {
    let key = js_string(key);
    let code = js_string(code);
    let text = js_string(text);
    let js = format!(
        r#"(function(){{
var el=document.activeElement||document.body;
var o={{key:{key},code:{code},bubbles:true}};
if({vk}){{o.keyCode={vk};o.which={vk};}}
el.dispatchEvent(new KeyboardEvent('keydown',o));
if({text}.length){{try{{document.execCommand('insertText',false,{text});}}catch(e){{}}}}
el.dispatchEvent(new KeyboardEvent('keyup',o));
return true;}})()"#,
        key = key,
        code = code,
        vk = virtual_key,
        text = text
    );
    eval_js(webview, &js, event_loop)?;
    Ok(())
}

fn insert_text_js(
    webview: &wry::WebView,
    text: &str,
    event_loop: &EventLoop<()>,
) -> Result<()> {
    let t = js_string(text);
    let js = format!(
        r#"(function(){{
var el=document.activeElement||document.body;
el.focus();
try{{document.execCommand('insertText',false,{t});return true;}}catch(e){{}}
try{{
  if(el.isContentEditable){{el.textContent=(el.textContent||'')+{t};el.dispatchEvent(new InputEvent('input',{{bubbles:true,data:{t}}}));}}
  else if('value' in el){{el.value=(el.value||'')+{t};el.dispatchEvent(new Event('input',{{bubbles:true}}));}}
  return true;
}}catch(e2){{return false;}}
}})()"#
    );
    eval_js(webview, &js, event_loop)?;
    Ok(())
}

fn click_at_js(
    webview: &wry::WebView,
    x: f64,
    y: f64,
    event_loop: &EventLoop<()>,
) -> Result<()> {
    let js = format!(
        r#"(function(){{
var x={x},y={y};
var el=document.elementFromPoint(x,y);
if(!el)return false;
['mousedown','mouseup','click'].forEach(function(t){{
  el.dispatchEvent(new MouseEvent(t,{{clientX:x,clientY:y,bubbles:true,button:0}}));
}});
try{{el.focus();}}catch(e){{}}
return true;}})()"#
    );
    eval_js(webview, &js, event_loop)?;
    Ok(())
}