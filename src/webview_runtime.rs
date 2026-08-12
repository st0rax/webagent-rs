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
#[cfg(windows)]
use wry::WebViewBuilderExtWindows;

use crate::page_driver::{PageDriver, PageDriverError, Result};

pub(crate) type ViewId = u64;

/// Position fuer "headless"-Fenster: weit ausserhalb jedes realen Desktops, aber
/// fuer Chromium ein normales, fokussierbares Fenster (siehe `open_page`).
const OFFSCREEN_POS: (f64, f64) = (-32000.0, -32000.0);

/// Interne Befehle an den UI-Thread.
enum RuntimeMessage {
    OpenPage {
        profile_dir: PathBuf,
        url: String,
        headless: bool,
        /// Fenstertitel-Label (z.B. Brain-Name) — leer = generischer Titel.
        title: String,
        respond: Sender<Result<(ViewId, WebViewPageDriver)>>,
    },
    ClosePage {
        view_id: ViewId,
        respond: Sender<Result<()>>,
    },
    /// Fenster auf eine Bildschirmposition holen (Brain-Kachelansicht) oder wieder
    /// off-screen parken. `None` = zurueck auf [`OFFSCREEN_POS`], also exakt
    /// das Verhalten ohne Kachelansicht.
    SetBounds {
        view_id: ViewId,
        bounds: Option<crate::brain_grid::Rect>,
        respond: Sender<Result<()>>,
    },
    /// Fokus ausdruecklich auf eine Kachel holen (`true`) oder ihn ans
    /// Terminalfenster zurueckgeben (`false`). Siehe [`WebViewRuntime::focus_view`].
    FocusView {
        view_id: ViewId,
        focus: bool,
        respond: Sender<Result<()>>,
    },
    Shutdown,
}

/// Befehle eines einzelnen Tabs (vom Agenten-Thread).
pub(crate) enum PageMessage {
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
    ClickAtTrusted {
        x: f64,
        y: f64,
        respond: Sender<Result<()>>,
    },
    CapturePng {
        respond: Sender<Result<Vec<u8>>>,
    },
}

struct PageSlot {
    page_rx: Receiver<PageMessage>,
    /// Wird fuer die CloseRequested-Zuordnung ueber `window.id()` gelesen und haelt
    /// das Fenster am Leben — faellt der Slot, schliesst sich das Fenster.
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
        std::fs::create_dir_all(&profile_dir)
            .map_err(|e| PageDriverError::Launch(format!("Profilverzeichnis: {e}")))?;

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
        title: &str,
    ) -> Result<WebViewPageDriver> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.tx
            .send(RuntimeMessage::OpenPage {
                profile_dir: profile_dir.to_path_buf(),
                url: url.to_string(),
                headless,
                title: title.to_string(),
                respond: resp_tx,
            })
            .map_err(|_| PageDriverError::Launch("WebView-Thread beendet".into()))?;
        let (_view_id, driver) = self.wake_and_wait(resp_rx, Duration::from_secs(60))?;
        Ok(driver)
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

    /// Holt ein Tab-Fenster auf den Bildschirm (Brain-Kachelansicht) oder parkt es wieder.
    ///
    /// `None` stellt exakt den Zustand ohne Kachelansicht wieder her: zurueck auf
    /// [`OFFSCREEN_POS`] und wieder aus der Taskleiste. Das Fenster bleibt in
    /// beiden Faellen sichtbar und fokussierbar — `with_visible(false)` wuerde
    /// den Enter-Absendeweg zerstoeren (siehe `open_page`).
    pub fn set_bounds(&self, view_id: ViewId, bounds: Option<crate::brain_grid::Rect>) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.tx
            .send(RuntimeMessage::SetBounds {
                view_id,
                bounds,
                respond: resp_tx,
            })
            .map_err(|_| PageDriverError::Protocol("WebView-Thread beendet".into()))?;
        self.wake_and_wait(resp_rx, Duration::from_secs(15))
    }

    /// Holt den Fokus ausdruecklich auf eine Kachel (`focus = true`) oder gibt
    /// ihn ans Terminalfenster zurueck (`focus = false`).
    ///
    /// Der Normalfall ist „kein Fokus": jedes Kachelfenster traegt
    /// `WS_EX_NOACTIVATE`, damit ein Klick oder ein Auftauchen auf dem Schirm
    /// dem Terminal nicht mitten im Tippen den Fokus wegreisst. Nur dieser
    /// Aufruf nimmt das Flag fuer die Dauer der Uebernahme weg — angestossen
    /// ausschliesslich durch Alt+Nummer bzw. Esc in der TUI.
    pub fn focus_view(&self, view_id: ViewId, focus: bool) -> Result<()> {
        let (resp_tx, resp_rx) = mpsc::channel();
        self.tx
            .send(RuntimeMessage::FocusView {
                view_id,
                focus,
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
    pub(crate) view_id: ViewId,
    pub(crate) page_tx: Sender<PageMessage>,
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

    fn click_at_trusted(&mut self, x: f64, y: f64) -> Result<()> {
        self.call(|respond| PageMessage::ClickAtTrusted { x, y, respond })
    }

    fn capture_png(&mut self) -> Result<Vec<u8>> {
        self.call(|respond| PageMessage::CapturePng { respond })
    }
}

/// Baut den EventLoop. Auf Windows verweigert tao `EventLoop::new()` ausserhalb des
/// Main-Threads (panic in event_loop.rs) — dieser Loop laeuft aber bewusst im
/// dedizierten `webagent-webview`-Thread, damit die PageDriver-API sync bleibt.
/// `with_any_thread(true)` ist der von tao selbst genannte Weg dafuer.
fn build_event_loop() -> EventLoop<()> {
    #[cfg(windows)]
    {
        use tao::event_loop::EventLoopBuilder;
        use tao::platform::windows::EventLoopBuilderExtWindows;
        EventLoopBuilder::new().with_any_thread(true).build()
    }
    #[cfg(not(windows))]
    {
        EventLoop::new()
    }
}

fn run_event_loop(
    cmd_rx: Receiver<RuntimeMessage>,
    _default_profile: PathBuf,
    _default_headless: bool,
) {
    let mut event_loop: EventLoop<()> = build_event_loop();

    let mut rt = SharedRuntime {
        cmd_rx,
        pages: HashMap::new(),
        next_id: 1,
        web_context: None,
    };

    let mut shutdown = false;
    while !shutdown {
        shutdown = pump_runtime(&mut rt, &mut event_loop);

        // CloseRequested muss den Tab wirklich abraeumen. Der Handler war frueher
        // leer (nur ein Kommentar), wodurch das Fenster-X schlicht nichts tat: der
        // Nutzer konnte ein `login`-Fenster nicht schliessen und musste den Prozess
        // abschiessen. Die WindowId erst sammeln und nach `run_return` verarbeiten —
        // in der Closure ist `rt` nicht erneut ausleihbar.
        let mut to_close: Vec<tao::window::WindowId> = Vec::new();
        let _ = event_loop.run_return(|event, _, control_flow| {
            *control_flow = ControlFlow::Exit;
            if let Event::WindowEvent {
                window_id,
                event: WindowEvent::CloseRequested,
                ..
            } = &event
            {
                to_close.push(*window_id);
            }
        });
        for wid in to_close {
            // PageSlot fallen lassen => Window + WebView werden zerstoert, und der
            // page_rx verschwindet, sodass der wartende Agenten-Thread ein Err
            // bekommt statt ewig zu blockieren.
            rt.pages.retain(|_, slot| slot.window.id() != wid);
        }
    }
}

fn pump_runtime(rt: &mut SharedRuntime, event_loop: &mut EventLoop<()>) -> bool {
    while let Ok(msg) = rt.cmd_rx.try_recv() {
        match msg {
            RuntimeMessage::Shutdown => return true,
            RuntimeMessage::OpenPage {
                profile_dir,
                url,
                headless,
                title,
                respond,
            } => {
                let result = open_page(rt, event_loop, &profile_dir, &url, headless, &title);
                let _ = respond.send(result);
            }
            RuntimeMessage::ClosePage { view_id, respond } => {
                let result = close_page(rt, view_id);
                let _ = respond.send(result);
            }
            RuntimeMessage::SetBounds {
                view_id,
                bounds,
                respond,
            } => {
                let result = set_bounds(rt, view_id, bounds);
                let _ = respond.send(result);
            }
            RuntimeMessage::FocusView {
                view_id,
                focus,
                respond,
            } => {
                let result = focus_view(rt, view_id, focus);
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
    event_loop: &mut EventLoop<()>,
    profile_dir: &Path,
    url: &str,
    headless: bool,
    title: &str,
) -> Result<(ViewId, WebViewPageDriver)> {
    std::fs::create_dir_all(profile_dir)
        .map_err(|e| PageDriverError::Launch(format!("Profilverzeichnis: {e}")))?;

    if rt.web_context.is_none() {
        rt.web_context = Some(wry::WebContext::new(Some(profile_dir.to_path_buf())));
    }

    let view_id = rt.next_id;
    rt.next_id += 1;

    // "headless" heisst hier: fuer den Nutzer unsichtbar, fuer Chromium aber ein
    // normales Fenster. `with_visible(false)` erfuellt nur die erste Haelfte: ein
    // nie gezeigtes Fenster kann keinen Fokus bekommen, also landen Tastendruecke
    // (press_enter) nirgends. Bei Brains ohne matchenden Send-Button ist Enter der
    // einzige Absende-Weg — der Relay lief dadurch headless in jeden Timeout, waehrend
    // er headed in Sekunden antwortete. Fenster off-screen statt versteckt.
    // Brain-Name im Fenstertitel: bei mehreren (auch off-screen) Fenstern —
    // Swarm, Worker-Pool — ist sonst im Task-Manager/Alt-Tab nicht erkennbar,
    // welches Fenster zu welchem Brain gehört (Storax-Wunsch 2026-07-20).
    let window_title = if title.trim().is_empty() {
        format!("webagent-{view_id}")
    } else {
        format!("webagent · {} ({view_id})", title.trim())
    };
    let mut builder = WindowBuilder::new()
        .with_title(window_title)
        .with_inner_size(LogicalSize::new(1280.0, 900.0))
        .with_visible(true);
    if headless {
        builder = builder.with_position(tao::dpi::LogicalPosition::new(
            OFFSCREEN_POS.0,
            OFFSCREEN_POS.1,
        ));
        // Off-Screen-Fenster nicht in der Taskleiste zeigen: dort ist es nur
        // ein toter Eintrag, den man weder sinnvoll fokussieren noch
        // maximieren kann (Storax-Beschwerde 2026-07-20). Fokussierbar fuer
        // den Enter-Absendeweg bleibt das Fenster trotzdem.
        #[cfg(windows)]
        {
            use tao::platform::windows::WindowBuilderExtWindows;
            builder = builder.with_skip_taskbar(true);
        }
    }
    let window = builder
        .build(event_loop)
        .map_err(|e| PageDriverError::Launch(e.to_string()))?;

    // Agentenfenster duerfen den Fokus nicht an sich reissen. Off-screen fiel das
    // nicht auf; in der Kachelansicht liegen sie neben der TUI, und jedes
    // absendende oder neu auftauchende Brain wuerde dem Nutzer den Fokus mitten
    // im Tippen wegnehmen. Nur der interaktive (nicht-headless) Fall — z.B.
    // `login` — bleibt ein normales, aktivierbares Fenster.
    if headless {
        set_no_activate(&window, true);
    }

    let mut web_context = rt
        .web_context
        .take()
        .ok_or_else(|| PageDriverError::Launch("WebContext fehlt".into()))?;

    let init_script = r#"
Object.defineProperty(navigator, 'webdriver', { get: function() { return undefined; } });
"#;

    let webview = WebViewBuilder::with_web_context(&mut web_context)
        .with_visible(true)
        .with_additional_browser_args(browser_args())
        .with_initialization_script(init_script)
        .with_url(url)
        .build(&window)
        .map_err(|e| PageDriverError::Launch(e.to_string()))?;

    // Native JS-Dialoge (alert/confirm/prompt/beforeunload) sind fuer einen
    // Agenten-gesteuerten Tab reine Blocker: ohne Nutzer, der sie wegklickt,
    // frieren sie die geteilte Event-Loop fuer ALLE Tabs ein (siehe run_event_loop
    // -- eval_js pumpt dieselbe Loop, in der ein natives Modal haengen bleibt).
    // Deaktivieren statt behandeln: das Skript bekommt sofort einen Default-Wert
    // zurueck, es gibt gar nichts mehr zum Haengenbleiben.
    if let Err(e) = disable_native_script_dialogs(&webview) {
        crate::bench_events::eprint_line(&format!(
            "[webview] Konnte native JS-Dialoge nicht deaktivieren: {e}"
        ));
    }

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

/// Schaltet `WS_EX_NOACTIVATE` fuer ein Fenster an oder aus.
///
/// tao 0.29 kennt das Flag nicht (`WindowBuilderExtWindows`/`WindowExtWindows`
/// bieten nur `skip_taskbar`, `undecorated_shadow`, `rtl`, `enable`, …) — es
/// gibt aber `hwnd()`, also wird das erweiterte Fensterstil-Bit direkt per
/// `SetWindowLongPtrW` gesetzt.
///
/// Wichtig fuer den Absendeweg: `WS_EX_NOACTIVATE` nimmt dem Fenster nur die
/// *Aktivierung*, nicht die Sichtbarkeit. Genau das ist der Unterschied zu
/// `with_visible(false)` (siehe `open_page`), das den Enter-Weg zerstoert hat.
/// `press_key` laeuft ohnehin als DOM-`KeyboardEvent` ueber
/// `evaluate_script` — dafuer braucht es keinen Betriebssystem-Fokus, nur eine
/// laufende, gerenderte Seite. Die bleibt hier erhalten.
#[cfg(windows)]
fn set_no_activate(window: &tao::window::Window, no_activate: bool) {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    let hwnd = HWND(window.hwnd() as *mut core::ffi::c_void);
    unsafe {
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let bit = WS_EX_NOACTIVATE.0 as isize;
        let updated = if no_activate {
            current | bit
        } else {
            current & !bit
        };
        if updated != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
        }
    }
}

#[cfg(not(windows))]
fn set_no_activate(_window: &tao::window::Window, _no_activate: bool) {}

/// Holt ein Fenster auf den Schirm, ohne ihm den Fokus zu geben.
///
/// `Window::set_minimized(false)` genuegt hier nicht: gemessen am 07.08.2026
/// blieben die Brain-Fenster nach dem Aufruf weiter als Symbol liegen
/// (`IsIconic` = true, Rechteck -32000,-32000 160x28), waehrend die
/// Kachelansicht „2 Fenster gekachelt" meldete. `SW_SHOWNOACTIVATE` macht
/// beides in einem Schritt und passt zum Entwurf: die Kacheln sind sichtbar,
/// aber nicht aktivierbar — der Fokus gehoert dem Terminal (Alt+1…9 ist der
/// einzige bewusste Weg in eine Kachel).
#[cfg(all(windows, feature = "webview"))]
fn show_without_activating(window: &tao::window::Window) {
    use tao::platform::windows::WindowExtWindows;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    let hwnd = HWND(window.hwnd() as *mut core::ffi::c_void);
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
}

#[cfg(not(all(windows, feature = "webview")))]
fn show_without_activating(_window: &tao::window::Window) {}

/// Gibt den Fokus aktiv an das Terminalfenster zurueck, in dem die TUI laeuft.
///
/// Ohne diesen Schritt landet der Fokus nach einer Uebernahme irgendwo —
/// Windows waehlt beim Deaktivieren kein bestimmtes Nachfolgefenster.
#[cfg(all(windows, feature = "webview"))]
fn focus_terminal_window() {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    if let Some(hwnd) = crate::brain_grid::terminal_window_hwnd() {
        unsafe {
            let _ = SetForegroundWindow(HWND(hwnd as *mut core::ffi::c_void));
        }
    }
}

#[cfg(not(all(windows, feature = "webview")))]
fn focus_terminal_window() {}

/// Ausdrueckliche Fokusuebernahme fuer eine Kachel bzw. Rueckgabe ans Terminal.
fn focus_view(rt: &mut SharedRuntime, view_id: ViewId, focus: bool) -> Result<()> {
    let slot = rt
        .pages
        .get(&view_id)
        .ok_or_else(|| PageDriverError::Protocol(format!("Tab {view_id} existiert nicht")))?;
    if focus {
        // Erst das Flag weg, sonst verweigert Windows die Aktivierung.
        set_no_activate(&slot.window, false);
        slot.window.set_focus();
        #[cfg(windows)]
        {
            use tao::platform::windows::WindowExtWindows;
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
            unsafe {
                let _ = SetForegroundWindow(HWND(
                    slot.window.hwnd() as *mut core::ffi::c_void
                ));
            }
        }
    } else {
        // Zurueck in den Normalzustand: nicht aktivierbar, Fokus ans Terminal.
        set_no_activate(&slot.window, true);
        focus_terminal_window();
    }
    Ok(())
}

/// Positioniert ein Tab-Fenster. Laeuft ausschliesslich im UI-Thread — tao
/// erlaubt Fensterzugriffe nur dort, deshalb der Umweg ueber `RuntimeMessage`.
fn set_bounds(
    rt: &mut SharedRuntime,
    view_id: ViewId,
    bounds: Option<crate::brain_grid::Rect>,
) -> Result<()> {
    let slot = rt
        .pages
        .get(&view_id)
        .ok_or_else(|| PageDriverError::Protocol(format!("Tab {view_id} existiert nicht")))?;
    // In beiden Richtungen: die Kachel bleibt nicht aktivierbar. Beim Auftauchen
    // auf dem Schirm ist das der Kern des Entwurfs, beim Parken schadet es nicht.
    set_no_activate(&slot.window, true);
    match bounds {
        Some(rect) => {
            // Zuerst aus dem minimierten Zustand holen. Ein minimiertes Fenster
            // nimmt Groesse und Position klaglos an und bleibt trotzdem als
            // Symbol liegen — die Kachelansicht meldete dann „3 Fenster
            // gekachelt", waehrend der Bildschirm leer blieb (gemessen
            // 07.08.2026: Rechteck -32000,-32000 160x28, also die Signatur
            // eines Symbols, nicht die des Park-Platzes, wo die Fenster ihre
            // echte Groesse behalten).
            slot.window.set_minimized(false);
            show_without_activating(&slot.window);
            slot.window
                .set_inner_size(tao::dpi::PhysicalSize::new(rect.width, rect.height));
            slot.window
                .set_outer_position(tao::dpi::PhysicalPosition::new(rect.x, rect.y));
            // In der Kachelansicht soll das Fenster normal erreichbar sein; off-screen
            // war der Taskleisteneintrag nur ein toter Klick.
            #[cfg(windows)]
            {
                use tao::platform::windows::WindowExtWindows;
                slot.window.set_skip_taskbar(false);
            }
        }
        None => {
            #[cfg(windows)]
            {
                use tao::platform::windows::WindowExtWindows;
                slot.window.set_skip_taskbar(true);
            }
            slot.window
                .set_outer_position(tao::dpi::LogicalPosition::new(
                    OFFSCREEN_POS.0,
                    OFFSCREEN_POS.1,
                ));
        }
    }
    Ok(())
}

/// Schaltet `alert()`/`confirm()`/`prompt()`/`beforeunload` fuer diesen Tab ab.
/// WebView2 liefert dann sofort einen Default-Wert statt eine native Modal zu
/// zeigen -- ohne Nutzer da, um sie wegzuklicken, wuerde sie sonst die geteilte
/// Event-Loop (ein Thread fuer alle Tabs) dauerhaft blockieren.
fn disable_native_script_dialogs(webview: &wry::WebView) -> std::result::Result<(), String> {
    use wry::WebViewExtWindows;
    unsafe {
        let controller = webview.controller();
        let core = controller.CoreWebView2().map_err(|e| e.to_string())?;
        let settings = core.Settings().map_err(|e| e.to_string())?;
        settings
            .SetAreDefaultScriptDialogsEnabled(false)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn dispatch_page(slot: &mut PageSlot, msg: PageMessage, event_loop: &mut EventLoop<()>) {
    match msg {
        PageMessage::Evaluate {
            expression,
            respond,
        } => {
            let r = eval_js(&slot.webview, &expression, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::Navigate {
            url,
            timeout,
            respond,
        } => {
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
        PageMessage::ClickAtTrusted { x, y, respond } => {
            let r = click_at_trusted_cdp(&slot.webview, x, y, event_loop);
            let _ = respond.send(r);
        }
        PageMessage::CapturePng { respond } => {
            let r = capture_png(&slot.webview, event_loop);
            let _ = respond.send(r);
        }
    }
}

/// Baut die WebView2-Argumente fuer `with_additional_browser_args`.
///
/// WICHTIG: Das MUSS der Builder-Weg sein, NICHT die Umgebungsvariable
/// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` — wry 0.47 setzt die Argumente
/// immer selbst (`options.set_additional_browser_arguments`, webview2/mod.rs),
/// und sobald sie explizit gesetzt sind, ignoriert WebView2 die
/// Umgebungsvariable komplett. Bis 2026-08-11 landeten weder die Perf-Flags
/// noch das Fake-Audio dort, wo sie wirken sollten.
///
/// Weil `with_additional_browser_args` die wry-Defaults ERSETZT, werden sie
/// hier mitgefuehrt: `--disable-features=msWebOOUI,msPdfOOUI,
/// msSmartScreenProtection` (wry-Default) plus `--autoplay-policy=...` bei
/// aktivem Autoplay.
///
/// Perf-Flags gegen Chromiums Okklusions-Drosselung: ein vollstaendig
/// verdecktes Fenster wird als "backgrounded" behandelt — Timer und
/// Streaming-JS werden gedrosselt, und ein Verify-Lauf wirkte eingefroren, bis
/// die Maus das Fenster aktivierte (beobachtet 2026-08-11, alle Brains).
fn browser_args() -> String {
    // `CalculateNativeWinOcclusion` ist die Windows-Occlusion-Erkennung von
    // Chromium selbst: verdeckte Fenster gelten als "hintergrund", und Timer/
    // Streaming werden gedrosselt, bis das Fenster in den Vordergrund kommt
    // (beobachtet 2026-08-11: verify-Fenster kamen erst nach min/max in Gang).
    // Die backgrounding-Flags reduzieren das Throttling, aber nur das
    // Abschalten der Erkennung beendet es.
    let mut args =
        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection,CalculateNativeWinOcclusion"
            .to_string();

    for a in [
        "--autoplay-policy=no-user-gesture-required",
        "--disable-backgrounding-occluded-windows",
        "--disable-background-timer-throttling",
        "--disable-renderer-backgrounding",
    ] {
        args.push(' ');
        args.push_str(a);
    }

    // Fake-Audio opt-in: eine WAV-Datei ersetzt die Mikrofon-Freigabe, damit
    // die Transkription belegbar im Composer landet. Bewusst Opt-in —
    // `--use-fake-ui-for-media-stream` erteilt die Freigabe ohne Rueckfrage.
    if let Ok(path) = std::env::var("WEBAGENT_FAKE_AUDIO") {
        let path = path.trim().to_string();
        if !path.is_empty() && std::path::Path::new(&path).is_file() {
            for a in [
                "--use-fake-ui-for-media-stream".to_string(),
                "--use-fake-device-for-media-stream".to_string(),
                format!("--use-file-for-fake-audio-capture={path}"),
            ] {
                args.push(' ');
                args.push_str(&a);
            }
            crate::bench_events::eprint_line(&format!("[webview] Mikrofon wird aus {path} gespeist"));
        } else {
            crate::bench_events::eprint_line(&format!(
                "[webview] WEBAGENT_FAKE_AUDIO zeigt auf keine Datei: {path} — ignoriert"
            ));
        }
    }

    args
}

/// Nimmt den Seiteninhalt als PNG auf (`ICoreWebView2::CapturePreview`).
///
/// Bewusst der WebView2-eigene Weg statt eines Fenster-Screenshots per GDI:
/// er braucht kein sichtbares, unverdecktes Fenster und liefert genau den
/// Seiteninhalt ohne Fensterrahmen — headless nutzbar, also auch aus einem
/// Automationslauf heraus.
fn capture_png(webview: &wry::WebView, event_loop: &mut EventLoop<()>) -> Result<Vec<u8>> {
    use std::sync::mpsc;
    use webview2_com::CapturePreviewCompletedHandler;
    use windows::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
    use windows::Win32::System::Com::STREAM_SEEK_SET;
    use wry::WebViewExtWindows;

    let proto = |e: String| PageDriverError::Protocol(e);
    unsafe {
        let core = webview
            .controller()
            .CoreWebView2()
            .map_err(|e| proto(e.to_string()))?;
        // `true` = der Stream gibt seinen Speicher beim Freigeben selbst zurueck.
        let stream = CreateStreamOnHGlobal(None, true).map_err(|e| proto(e.to_string()))?;

        let (tx, rx) = mpsc::channel();
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |hr| {
            let _ = tx.send(hr);
            Ok(())
        }));
        core.CapturePreview(
            webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
        .map_err(|e| proto(e.to_string()))?;

        // Wie bei `eval_js`: die Event-Loop muss weiterlaufen, sonst feuert der
        // Completion-Handler nie und wir warten auf uns selbst.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match rx.try_recv() {
                Ok(hr) => {
                    hr.map_err(|e| proto(e.to_string()))?;
                    break;
                }
                Err(_) if Instant::now() >= deadline => {
                    return Err(PageDriverError::Timeout("CapturePreview timeout".into()));
                }
                Err(_) => {}
            }
            pump_once(event_loop);
            thread::sleep(Duration::from_millis(5));
        }

        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|e| proto(e.to_string()))?;
        let mut out = Vec::new();
        let mut buf = [0u8; 64 * 1024];
        loop {
            let mut got: u32 = 0;
            stream
                .Read(
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    buf.len() as u32,
                    Some(&mut got),
                )
                .ok()
                .map_err(|e| proto(e.to_string()))?;
            if got == 0 {
                break;
            }
            out.extend_from_slice(&buf[..got as usize]);
        }
        if out.is_empty() {
            return Err(proto("CapturePreview lieferte 0 Bytes".into()));
        }
        Ok(out)
    }
}

/// Verpackt einen JS-Ausdruck fuer `evaluate_script_with_callback`.
///
/// Zwei Fallen, beide gemessen (siehe Tests unten):
/// - **Kein `async`/`await`.** WebView2 serialisiert das Ergebnis des Skripts; eine
///   async-IIFE liefert ein Promise, und das serialisiert zu `{}`. Der frühere
///   Wrapper tat genau das — jedes `evaluate` kam als `{}` zurueck, wodurch jede
///   DOM-Abfrage (Login-Status, Composer, Antworten) leer aussah. `awaitPromise`
///   gab es nur bei CDP; hier braucht es kein Ausdruck.
/// - **Kein `JSON.stringify`.** Gibt das Skript einen String zurueck, kodiert
///   WebView2 ihn ein zweites Mal (`"\"{\\\"ok\\\":true}\""`). Das Objekt direkt
///   zurueckgeben liefert sauberes `{"ok":true,...}`.
fn wrap_eval(expression: &str) -> String {
    format!(
        r#"(function(){{try{{return {{ok:true,value:({expression})}};}}catch(e){{return {{ok:false,error:String(e)}};}}}})()"#
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

fn pump_once(event_loop: &mut EventLoop<()>) {
    let _ = event_loop.run_return(|_, _, control_flow| {
        *control_flow = ControlFlow::Exit;
    });
}

fn eval_js(
    webview: &wry::WebView,
    expression: &str,
    event_loop: &mut EventLoop<()>,
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
    event_loop: &mut EventLoop<()>,
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

/// Baut das Skript fuer [`press_key_js`].
///
/// Eigene Funktion, damit der Absendeweg pruefbar bleibt: Enter wird als
/// DOM-`KeyboardEvent` an `document.activeElement` geschickt, also **innerhalb**
/// der Seite. Er haengt damit nicht am Fenster-Fokus des Betriebssystems —
/// genau deshalb darf die Kachel `WS_EX_NOACTIVATE` tragen. Was er braucht, ist
/// eine sichtbare, laufende Seite; die nimmt ihm `WS_EX_NOACTIVATE` nicht,
/// `with_visible(false)` dagegen schon.
fn press_key_script(key: &str, code: &str, virtual_key: i64, text: &str) -> String {
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
    js
}

fn press_key_js(
    webview: &wry::WebView,
    key: &str,
    code: &str,
    virtual_key: i64,
    text: &str,
    event_loop: &mut EventLoop<()>,
) -> Result<()> {
    let js = press_key_script(key, code, virtual_key, text);
    eval_js(webview, &js, event_loop)?;
    Ok(())
}

fn insert_text_js(
    webview: &wry::WebView,
    text: &str,
    event_loop: &mut EventLoop<()>,
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
    event_loop: &mut EventLoop<()>,
) -> Result<()> {
    let js = format!(
        r#"(function(){{
var x={x},y={y};
var el=document.elementFromPoint(x,y);
if(!el)return false;
// Der vollstaendige Ereignisweg in der Reihenfolge eines echten Mausklicks:
// pointerdown -> mousedown -> pointerup -> mouseup -> click. Radix-basierte
// Oberflaechen (Perplexitys Modellmenue, gemessen 2026-08-12) oeffnen sich
// auf pointerdown und blieben ohne diese beiden Ereignisse geschlossen.
var pd=new PointerEvent('pointerdown',{{clientX:x,clientY:y,bubbles:true,pointerId:1,isPrimary:true,button:0,pointerType:'mouse'}});
var pu=new PointerEvent('pointerup',{{clientX:x,clientY:y,bubbles:true,pointerId:1,isPrimary:true,button:0,pointerType:'mouse'}});
['pointerdown','mousedown','pointerup','mouseup','click'].forEach(function(t){{
  el.dispatchEvent(t==='pointerdown'?pd:t==='pointerup'?pu:new MouseEvent(t,{{clientX:x,clientY:y,bubbles:true,button:0}}));
}});
try{{el.focus();}}catch(e){{}}
return true;}})()"#
    );
    eval_js(webview, &js, event_loop)?;
    Ok(())
}

/// CDP-Methode aufrufen und auf den Completion-Handler warten (on-device:
/// `CallDevToolsProtocolMethod` spricht denselben CDP-Kanal wie die
/// Remote-Debugging-Session, aber in-prozess — kein Port, kein WebSocket).
///
/// Wie bei `capture_png` und `eval_js`: die Event-Loop muss weiterlaufen,
/// sonst feuert der Completion-Handler nie und wir warten auf uns selbst.
fn call_cdp(
    webview: &wry::WebView,
    method: &str,
    params: &str,
    event_loop: &mut EventLoop<()>,
) -> Result<()> {
    use std::sync::mpsc;
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;
    use wry::WebViewExtWindows;

    let proto = |e: String| PageDriverError::Protocol(e);
    unsafe {
        let core = webview
            .controller()
            .CoreWebView2()
            .map_err(|e| proto(e.to_string()))?;

        // `CallDevToolsProtocolMethod` nimmt PCWSTR — `&str` ist kein
        // `Param<PCWSTR>`, HSTRING dagegen schon.
        let method_wide = HSTRING::from(method);
        let params_wide = HSTRING::from(params);

        let (tx, rx) = mpsc::channel();
        let handler = CallDevToolsProtocolMethodCompletedHandler::create(Box::new(
            move |hr, _json| {
                let _ = tx.send(hr);
                Ok(())
            },
        ));
        core.CallDevToolsProtocolMethod(&method_wide, &params_wide, &handler)
            .map_err(|e| proto(e.to_string()))?;

        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match rx.try_recv() {
                Ok(hr) => {
                    hr.map_err(|e| proto(e.to_string()))?;
                    break;
                }
                Err(_) if Instant::now() >= deadline => {
                    return Err(PageDriverError::Timeout(format!(
                        "{method}: CDP-Antwort ausstehend"
                    )));
                }
                Err(_) => {}
            }
            pump_once(event_loop);
            thread::sleep(Duration::from_millis(5));
        }
    }
    Ok(())
}

/// Die drei Events der vertrauenswuerdigen Pointer-Sequenz, als (Eventtyp,
/// CDP-Parameter-JSON). Als eigene Funktion, damit ein Test exakt pruefen kann,
/// welche Sequenz an die Engine geht — dieselbe Unterscheidung zwischen
/// Mechanismus-Test und Verhalten, die `press_key_script` schon traegt.
fn cdp_click_events(x: f64, y: f64) -> Vec<(&'static str, String)> {
    let base = |extra: &str| {
        format!(
            r#"{{"x":{x},"y":{y},"button":"left","pointerType":"mouse",{extra}}}"#,
            extra = extra
        )
    };
    [
        ("mouseMoved", r#""buttons":0"#),
        ("mousePressed", r#""buttons":1,"clickCount":1"#),
        ("mouseReleased", r#""buttons":0,"clickCount":1"#),
    ]
    .iter()
    .map(|(e, extra)| (*e, base(extra)))
    .collect()
}

/// Vertrauenswuerdiger Linksklick an Viewport-Koordinaten ueber die
/// vollstaendige CDP-Pointer-Sequenz (moved -> pressed -> released), wie sie
/// auch Puppeteer/Playwright senden. Anders als [`click_at_js`] sind diese
/// Events `isTrusted=true` — genau das verlangt qwens Denkstufen-Menue.
fn click_at_trusted_cdp(
    webview: &wry::WebView,
    x: f64,
    y: f64,
    event_loop: &mut EventLoop<()>,
) -> Result<()> {
    for (_event, params) in cdp_click_events(x, y) {
        call_cdp(webview, "Input.dispatchMouseEvent", &params, event_loop)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{cdp_click_events, parse_eval_result, press_key_script, wrap_eval};

    // Der vertrauenswuerdige Klick ist eine vollstaendige Pointer-Sequenz
    // (moved -> pressed -> released) mit Koordinaten, button und clickCount —
    // nicht ein einziges synthetisches DOM-Event. Genau daran haengt, ob
    // qwens Denkstufen-Menue aufpoppt: die Oberflaeche lauscht auf trusted
    // pointerdown, und das liefert nur `Input.dispatchMouseEvent` ueber CDP.
    #[test]
    fn cdp_klick_sendet_vollstaendige_pointer_sequenz() {
        let events = cdp_click_events(123.0, 456.0);
        let types: Vec<&str> = events.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            types,
            vec!["mouseMoved", "mousePressed", "mouseReleased"]
        );
        for (t, params) in &events {
            assert!(
                params.contains("\"x\":123") && params.contains("\"y\":456"),
                "{t} muss die Koordinaten tragen: {params}"
            );
            assert!(params.contains("\"button\":\"left\""), "{t}: {params}");
            assert!(
                params.contains("\"pointerType\":\"mouse\""),
                "{t}: {params}"
            );
        }
        assert!(
            events[1].1.contains("\"clickCount\":1"),
            "pressed braucht clickCount: {}",
            events[1].1
        );
    }

    // Die Bedingung, unter der die ganze Kachelansicht steht: Enter absenden
    // darf nicht am Betriebssystem-Fokus haengen. Tut es auch nicht — der
    // Tastendruck ist ein DOM-Event an `document.activeElement`, das ueber
    // `evaluate_script` in die Seite gereicht wird. Deshalb duerfen die
    // Kachelfenster `WS_EX_NOACTIVATE` tragen, ohne den Absendeweg zu beruehren.
    // Kaeme hier je ein Betriebssystem-Weg (SendInput, keybd_event, SendMessage
    // an das Fenster) hinein, waere diese Annahme gebrochen — der Test schlaegt
    // dann an, bevor der Relay stillschweigend in Timeouts laeuft.
    #[test]
    fn enter_wird_im_dom_ausgeloest_nicht_ueber_den_fensterfokus() {
        let js = press_key_script("Enter", "Enter", 13, "\r");
        assert!(
            js.contains("document.activeElement"),
            "Enter muss ans fokussierte DOM-Element gehen: {js}"
        );
        assert!(
            js.contains("new KeyboardEvent('keydown'"),
            "Enter muss ein DOM-KeyboardEvent sein: {js}"
        );
        for os_weg in ["SendInput", "keybd_event", "PostMessage", "SendMessage"] {
            assert!(
                !js.contains(os_weg),
                "{os_weg} wuerde den Absendeweg an den Fensterfokus binden: {js}"
            );
        }
        assert!(js.contains("13"), "keyCode fehlt: {js}");
    }

    // Regression: der Wrapper darf kein Promise und keinen String liefern.
    // WebView2 serialisiert das Skript-Ergebnis; eine async-IIFE kommt als "{}"
    // zurueck und JSON.stringify als doppelt kodierter String. Beides liess
    // jede DOM-Abfrage leer aussehen (logged_in=false fuer alle Provider).
    #[test]
    fn wrap_eval_is_sync_and_returns_object() {
        let js = wrap_eval("1+1");
        assert!(
            !js.contains("async"),
            "async-IIFE liefert ein Promise: {js}"
        );
        assert!(!js.contains("await"), "await erzwingt ein Promise: {js}");
        assert!(
            !js.contains("JSON.stringify"),
            "stringify erzeugt doppelt kodierten String: {js}"
        );
        assert!(js.contains("ok:true"), "Erfolgs-Huelle fehlt: {js}");
        assert!(js.contains("(1+1)"), "Ausdruck nicht eingebettet: {js}");
    }

    #[test]
    fn parse_eval_result_unwraps_ok_value() {
        let v = parse_eval_result(r#"{"ok":true,"value":2}"#.to_string()).unwrap();
        assert_eq!(v, serde_json::json!(2));
    }

    #[test]
    fn parse_eval_result_maps_js_exception_to_err() {
        let e = parse_eval_result(r#"{"ok":false,"error":"ReferenceError: x"}"#.to_string());
        assert!(e.is_err(), "JS-Ausnahme muss Err werden");
    }

    // Das war das reale Symptom: der async-Wrapper lieferte wortwoertlich "{}".
    #[test]
    fn parse_eval_result_on_empty_object_yields_no_value() {
        let v = parse_eval_result("{}".to_string()).unwrap();
        assert!(
            v.get("value").is_none(),
            "leeres Objekt darf keinen Wert vortaeuschen"
        );
    }
}
