//! Minimaler Chrome-DevTools-Protocol-Client (CDP) über WebSocket.
//!
//! Ersetzt Playwright: startet ein Chromium mit `--remote-debugging-port`,
//! findet das Page-Target per HTTP und steuert die Seite über `Runtime.evaluate`
//! und `Page.navigate`. Bewusst synchron/blockierend wie das Python-Original.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// Fehler des CDP-Clients.
#[derive(Debug)]
pub enum CdpError {
    Launch(String),
    Discovery(String),
    Protocol(String),
    Timeout(String),
    InvalidEndpoint(String),
}

impl std::fmt::Display for CdpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CdpError::Launch(m) => write!(f, "chrome-launch: {m}"),
            CdpError::Discovery(m) => write!(f, "target-discovery: {m}"),
            CdpError::Protocol(m) => write!(f, "cdp-protocol: {m}"),
            CdpError::Timeout(m) => write!(f, "cdp-timeout: {m}"),
            CdpError::InvalidEndpoint(m) => write!(f, "cdp-endpoint: {m}"),
        }
    }
}
impl std::error::Error for CdpError {}

type Result<T> = std::result::Result<T, CdpError>;

/// Kandidatenpfade für die Chromium/Chrome-Binary je Plattform.
fn chrome_candidates() -> Vec<String> {
    if let Ok(explicit) = std::env::var("WEBAGENT_CHROME") {
        if !explicit.trim().is_empty() {
            return vec![explicit];
        }
    }
    #[cfg(windows)]
    {
        let pf = std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".into());
        let pf86 =
            std::env::var("ProgramFiles(x86)").unwrap_or_else(|_| "C:\\Program Files (x86)".into());
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        vec![
            format!("{pf}\\Google\\Chrome\\Application\\chrome.exe"),
            format!("{pf86}\\Google\\Chrome\\Application\\chrome.exe"),
            format!("{local}\\Google\\Chrome\\Application\\chrome.exe"),
            format!("{pf}\\Microsoft\\Edge\\Application\\msedge.exe"),
            format!("{pf86}\\Microsoft\\Edge\\Application\\msedge.exe"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome".into(),
            "/Applications/Chromium.app/Contents/MacOS/Chromium".into(),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            "google-chrome".into(),
            "google-chrome-stable".into(),
            "chromium".into(),
            "chromium-browser".into(),
        ]
    }
}

/// Ein gestarteter Chromium-Prozess mit Debug-Port.
pub struct ChromeProcess {
    child: Child,
    pub port: u16,
    pub binary: String,
}

impl ChromeProcess {
    /// Startet Chromium mit persistentem Profil und Remote-Debugging.
    pub fn launch(profile_dir: &PathBuf, headless: bool, port: u16) -> Result<Self> {
        std::fs::create_dir_all(profile_dir).ok();
        let mut last_err = String::from("keine Chromium-Binary gefunden");
        for bin in chrome_candidates() {
            let mut cmd = Command::new(&bin);
            cmd.arg(format!("--remote-debugging-port={port}"))
                .arg(format!("--user-data-dir={}", profile_dir.display()))
                .arg("--no-first-run")
                .arg("--no-default-browser-check")
                .arg("--disable-background-networking")
                .arg("--disable-features=Translate")
                // Versteckt navigator.webdriver & Co. — DAS ist der korrekte Flag
                // (blink-features), damit Provider den Browser nicht als Automation
                // erkennen und ein "nicht unterstuetzt"-Layout zeigen.
                .arg("--disable-blink-features=AutomationControlled")
                .arg("--remote-allow-origins=*")
                // Fenstergröße IMMER setzen: bei zu kleinem Fenster rendern manche
                // Provider (z.B. Qwen) ein Mobil-/"nicht unterstuetzt"-Layout ohne
                // funktionierenden Chat.
                .arg("--window-size=1280,900")
                .arg("--window-position=0,0");
            if headless {
                cmd.arg("--headless=new");
            }
            cmd.arg("about:blank");
            match cmd.spawn() {
                Ok(child) => {
                    let proc = ChromeProcess {
                        child,
                        port,
                        binary: bin,
                    };
                    proc.wait_ready(Duration::from_secs(45))?;
                    return Ok(proc);
                }
                Err(e) => {
                    last_err = format!("{bin}: {e}");
                    continue;
                }
            }
        }
        Err(CdpError::Launch(last_err))
    }

    /// Wartet, bis der Debug-Port HTTP beantwortet.
    fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if http_get(self.port, "/json/version").is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        Err(CdpError::Timeout(format!(
            "Debug-Port {} nicht bereit",
            self.port
        )))
    }

    /// WebSocket-URL des ersten Page-Targets (bei Bedarf per Neuanlage).
    pub fn page_ws_url(&self) -> Result<String> {
        let body = http_get(self.port, "/json").map_err(CdpError::Discovery)?;
        let targets: Value =
            serde_json::from_str(&body).map_err(|e| CdpError::Discovery(e.to_string()))?;
        if let Some(arr) = targets.as_array() {
            for t in arr {
                if t.get("type").and_then(|v| v.as_str()) == Some("page") {
                    if let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        // Kein Page-Target: neues anlegen.
        let created = http_get(self.port, "/json/new?about:blank").map_err(CdpError::Discovery)?;
        let t: Value =
            serde_json::from_str(&created).map_err(|e| CdpError::Discovery(e.to_string()))?;
        t.get("webSocketDebuggerUrl")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| CdpError::Discovery("kein webSocketDebuggerUrl".into()))
    }

    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for ChromeProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Roher HTTP/1.1-GET an den lokalen Debug-Port (kein externer HTTP-Client nötig).
///
/// Der DevTools-HTTP-Server hält die Verbindung offen (Keep-Alive), daher NICHT
/// bis EOF lesen, sondern `Content-Length` auswerten und exakt am Body-Ende
/// stoppen. Fehlt Content-Length, wird bis Leerlauf/Timeout gelesen.
fn http_get(port: u16, path: &str) -> std::result::Result<String, String> {
    http_get_to("127.0.0.1", port, path)
}

/// HTTP/1.1-GET zu einem beliebigen Host:Port (für Remote-CDP-Endpunkte).
fn http_get_to(host: &str, port: u16, path: &str) -> std::result::Result<String, String> {
    let mut stream = TcpStream::connect((host, port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let req = format!(
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| e.to_string())?;

    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];
    loop {
        if let Some(body) = extract_complete_body(&buf) {
            return Ok(body);
        }
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    extract_complete_body(&buf)
        .or_else(|| {
            // Fallback: Body ab Header-Ende, auch ohne/mit unklarem Content-Length.
            let text = String::from_utf8_lossy(&buf);
            text.split_once("\r\n\r\n").map(|(_, b)| b.to_string())
        })
        .ok_or_else(|| "unvollständige HTTP-Antwort".to_string())
}

/// Gibt den vollständigen Body zurück, sobald Header + `Content-Length` Bytes da sind.
fn extract_complete_body(buf: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(buf);
    let (headers, body) = text.split_once("\r\n\r\n")?;
    let content_length = headers.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        if k.trim().eq_ignore_ascii_case("content-length") {
            v.trim().parse::<usize>().ok()
        } else {
            None
        }
    })?;
    if body.len() >= content_length {
        Some(body[..content_length].to_string())
    } else {
        None
    }
}

/// Verbindung zu einem Page-Target.
pub struct CdpClient {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    next_id: u64,
}

/// Ergebnis der netzwerkfreien CDP-Endpunkt-Analyse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CdpEndpointKind {
    /// Direkte WebSocket-URL (`ws://` / `wss://`) — unverändert verbinden.
    WebSocket(String),
    /// HTTP-Debug-Port (`host:port`, optional mit `http(s)://`-Präfix).
    HttpAuthority { host: String, port: u16 },
}

/// Zerlegt einen CDP-Endpunkt ohne Netzwerk (Prefix-Abbau, Host/Port).
pub fn parse_cdp_endpoint(endpoint: &str) -> Result<CdpEndpointKind> {
    let e = endpoint.trim();
    if e.is_empty() {
        return Err(CdpError::Discovery("leerer CDP-Endpunkt".into()));
    }
    if e.starts_with("ws://") || e.starts_with("wss://") {
        return Ok(CdpEndpointKind::WebSocket(e.to_string()));
    }
    let authority = e
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    let (host, port) = parse_host_port(authority).map_err(CdpError::InvalidEndpoint)?;
    Ok(CdpEndpointKind::HttpAuthority {
        host: host.to_string(),
        port,
    })
}

/// Zerlegt einen konfigurierten CDP-Endpunkt in eine WebSocket-URL.
///
/// Unterstützt:
/// - direkte `ws://` / `wss://` URL -> 1:1 durchgereicht (Remote-Page-Target)
/// - `host:port` (optional mit Pfad, mit/ohne `http://`-Präfix) -> die erste
///   `page`-Target-WebSocket-URL wird per HTTP von `http://host:port/json`
///   aufgelöst (Chrome läuft auf einem ANDEREN Rechner, kein lokaler Launch)
///
/// `WEBAGENT_CDP_ENDPOINT` wird so ausgewertet, damit der Agent unter Android
/// (Termux) zu einem Desktop-Chrome per `ws://` verbindet.
pub fn resolve_cdp_ws(endpoint: &str) -> Result<String> {
    match parse_cdp_endpoint(endpoint)? {
        CdpEndpointKind::WebSocket(url) => Ok(url),
        CdpEndpointKind::HttpAuthority { host, port } => {
            let body = http_get_to(&host, port, "/json").map_err(CdpError::Discovery)?;
            let targets: Value =
                serde_json::from_str(&body).map_err(|e| CdpError::Discovery(e.to_string()))?;
            if let Some(arr) = targets.as_array() {
                for t in arr {
                    if t.get("type").and_then(|v| v.as_str()) == Some("page") {
                        if let Some(ws) = t.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) {
                            return Ok(ws.to_string());
                        }
                    }
                }
            }
            Err(CdpError::Discovery(
                "kein page-Target am Remote-Endpunkt".into(),
            ))
        }
    }
}

/// Spaltet `host:port` (oder reines `host`) in Host und Port (Default 9222).
fn parse_host_port(authority: &str) -> std::result::Result<(&str, u16), String> {
    match authority.rsplit_once(':') {
        Some((h, p)) => {
            let port = p
                .parse::<u16>()
                .map_err(|_| format!("ungültiger Port in '{authority}'"))?;
            Ok((if h.is_empty() { "127.0.0.1" } else { h }, port))
        }
        None => Ok((authority, 9222)),
    }
}

/// Liest die Umgebungsvariable `WEBAGENT_CDP_ENDPOINT` aus und gibt sie
/// (getrimmt) zurück, sofern gesetzt und nicht leer. `None` sonst — das ist die
/// Entscheidungsgrundlage, ob der Agent lokal ein Chromium startet oder sich
/// per `ws://` zu einem Remote-Chrome auf einem anderen Rechner verbindet.
pub fn cdp_endpoint_from_env() -> Option<String> {
    std::env::var("WEBAGENT_CDP_ENDPOINT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl CdpClient {
    /// Verbindet direkt zu einem konfigurierten CDP-Endpunkt
    /// (siehe [`resolve_cdp_ws`]) — überspringt den lokalen Chrome-Launch.
    pub fn connect_endpoint(endpoint: &str) -> Result<Self> {
        let ws_url = resolve_cdp_ws(endpoint)?;
        Self::connect(&ws_url)
    }

    pub fn connect(ws_url: &str) -> Result<Self> {
        let (ws, _resp) =
            tungstenite::connect(ws_url).map_err(|e| CdpError::Protocol(e.to_string()))?;
        // Lesetimeout auf dem Plain-Stream setzen, damit read() nicht ewig blockiert.
        if let MaybeTlsStream::Plain(tcp) = ws.get_ref() {
            let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));
        }
        let mut client = CdpClient { ws, next_id: 0 };
        // Grundlegende Domains aktivieren (Fehler tolerieren).
        let _ = client.call("Page.enable", json!({}));
        let _ = client.call("Runtime.enable", json!({}));
        Ok(client)
    }

    /// Sendet einen CDP-Befehl und liest bis zur passenden Antwort-ID.
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let payload = json!({ "id": id, "method": method, "params": params });
        self.ws
            .send(Message::Text(payload.to_string()))
            .map_err(|e| CdpError::Protocol(e.to_string()))?;

        let deadline = Instant::now() + Duration::from_secs(35);
        loop {
            if Instant::now() > deadline {
                return Err(CdpError::Timeout(format!("keine Antwort auf {method}")));
            }
            let msg = match self.ws.read() {
                Ok(m) => m,
                Err(tungstenite::Error::Io(ref e))
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    return Err(CdpError::Timeout(format!("read-timeout bei {method}")));
                }
                Err(e) => return Err(CdpError::Protocol(e.to_string())),
            };
            let text = match msg {
                Message::Text(t) => t.to_string(),
                Message::Binary(b) => String::from_utf8_lossy(&b).to_string(),
                Message::Close(_) => {
                    return Err(CdpError::Protocol("WebSocket geschlossen".into()))
                }
                _ => continue, // Ping/Pong/Frame ignorieren
            };
            let v: Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };
            // Events (ohne id) überspringen.
            if v.get("id").and_then(|x| x.as_u64()) != Some(id) {
                continue;
            }
            if let Some(err) = v.get("error") {
                return Err(CdpError::Protocol(err.to_string()));
            }
            return Ok(v.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    /// Wertet einen JS-Ausdruck in der Seite aus und gibt den Rückgabewert zurück.
    /// `await`-fähig (returnByValue).
    pub fn evaluate(&mut self, expression: &str) -> Result<Value> {
        let result = self.call(
            "Runtime.evaluate",
            json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true,
                "userGesture": true
            }),
        )?;
        if let Some(exc) = result.get("exceptionDetails") {
            return Err(CdpError::Protocol(format!("JS-Ausnahme: {exc}")));
        }
        Ok(result
            .get("result")
            .and_then(|r| r.get("value"))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Convenience: JS auswerten und als String zurückgeben ("" bei null).
    pub fn eval_string(&mut self, expression: &str) -> Result<String> {
        Ok(self
            .evaluate(expression)?
            .as_str()
            .unwrap_or("")
            .to_string())
    }

    /// Navigiert zu einer URL und wartet (best effort) auf document.readyState=complete.
    pub fn navigate(&mut self, url: &str, timeout: Duration) -> Result<()> {
        self.call("Page.navigate", json!({ "url": url }))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Ok(state) = self.eval_string("document.readyState") {
                if state == "complete" || state == "interactive" {
                    return Ok(());
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Ok(())
    }

    /// Aktuelle URL der Seite.
    pub fn current_url(&mut self) -> Result<String> {
        self.eval_string("location.href")
    }

    /// Sendet einen Tastendruck an das fokussierte Element. `text` wird beim
    /// keyDown mitgesendet (z.B. "\r" fuer Enter) — ohne das feuert Chrome kein
    /// keypress/beforeinput, und viele Web-Composer loesen dann kein Submit aus.
    pub fn press_key(&mut self, key: &str, code: &str, virtual_key: i64, text: &str) -> Result<()> {
        let mut down = json!({
            "type": "keyDown",
            "key": key,
            "code": code,
            "windowsVirtualKeyCode": virtual_key,
            "nativeVirtualKeyCode": virtual_key
        });
        if !text.is_empty() {
            down["text"] = json!(text);
            down["unmodifiedText"] = json!(text);
        }
        self.call("Input.dispatchKeyEvent", down)?;
        self.call(
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "key": key,
                "code": code,
                "windowsVirtualKeyCode": virtual_key,
                "nativeVirtualKeyCode": virtual_key
            }),
        )?;
        Ok(())
    }

    /// Fügt Text als "echtes Tippen" ins fokussierte Element ein (CDP
    /// `Input.insertText`). Löst beforeinput/input aus, sodass Frameworks
    /// (React/Vue) den Wert übernehmen — im Gegensatz zu direkter `.value`-Zuweisung.
    pub fn insert_text(&mut self, text: &str) -> Result<()> {
        self.call("Input.insertText", json!({ "text": text }))?;
        Ok(())
    }

    /// Echter Linksklick an Viewport-Koordinaten (mousePressed + mouseReleased) —
    /// gibt einem Element echten Fokus, wie Playwrights click(); JS-`el.focus()`
    /// reicht manchen Frameworks/`insertText` nicht.
    pub fn click_at(&mut self, x: f64, y: f64) -> Result<()> {
        for typ in ["mousePressed", "mouseReleased"] {
            self.call(
                "Input.dispatchMouseEvent",
                json!({"type": typ, "x": x, "y": y, "button": "left", "clickCount": 1}),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Echter End-to-End-Test gegen ein lokal installiertes Chromium/Edge.
    /// Ignoriert per Default (braucht einen Browser); explizit ausführen mit:
    ///   cargo test --lib cdp -- --ignored --nocapture
    #[test]
    #[ignore]
    fn cdp_end_to_end_smoke() {
        let dir = std::env::temp_dir().join(format!("cdp_smoke_{}", std::process::id()));
        let mut proc = ChromeProcess::launch(&dir, true, 9333).expect("chrome launch");
        let ws = proc.page_ws_url().expect("ws url");
        let mut client = CdpClient::connect(&ws).expect("connect");

        let data_url = "data:text/html,<html><body>\
            <div class='prose'>Hallo</div><div class='prose'>Welt</div>\
            <textarea id='c'></textarea></body></html>";
        client
            .navigate(data_url, Duration::from_secs(15))
            .expect("navigate");

        let count = client
            .evaluate("document.querySelectorAll('.prose').length")
            .expect("eval count");
        assert_eq!(count.as_i64(), Some(2), "prose-Anzahl");

        let text = client
            .eval_string("document.querySelectorAll('.prose')[1].innerText")
            .expect("eval text");
        assert!(text.contains("Welt"), "text={text}");

        let sum = client.evaluate("1+1").expect("eval arithmetic");
        assert_eq!(sum.as_i64(), Some(2));

        // Objektrückgabe (returnByValue) — genau das nutzt browser::probe_generation.
        let obj = client
            .evaluate(
                "(function(){return {count:document.querySelectorAll('.prose').length,\
                 text:document.querySelectorAll('.prose')[0].innerText,stop:false};})()",
            )
            .expect("eval object");
        assert_eq!(obj.get("count").and_then(|v| v.as_i64()), Some(2));
        assert!(obj
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .contains("Hallo"));
        assert_eq!(obj.get("stop").and_then(|v| v.as_bool()), Some(false));

        proc.kill();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_passthrough_ws_url() {
        let ep = "ws://192.168.1.10:9222/devtools/page/abc123";
        assert_eq!(resolve_cdp_ws(ep).unwrap(), ep);
        let ep2 = "wss://host.example/devtools/browser/xyz";
        assert_eq!(resolve_cdp_ws(ep2).unwrap(), ep2);
    }

    #[test]
    fn resolve_empty_endpoint_errors() {
        assert!(resolve_cdp_ws("").is_err());
        assert!(resolve_cdp_ws("   ").is_err());
    }

    #[test]
    fn parse_host_port_default_and_explicit() {
        assert_eq!(
            parse_host_port("example.com").unwrap(),
            ("example.com", 9222)
        );
        assert_eq!(
            parse_host_port("example.com:9333").unwrap(),
            ("example.com", 9333)
        );
        assert_eq!(
            parse_host_port("127.0.0.1:9222").unwrap(),
            ("127.0.0.1", 9222)
        );
        assert!(parse_host_port("example.com:notaport").is_err());
    }

    #[test]
    fn resolve_strips_http_prefix_without_network() {
        assert_eq!(
            parse_cdp_endpoint("http://example.com:9222").unwrap(),
            CdpEndpointKind::HttpAuthority {
                host: "example.com".into(),
                port: 9222,
            }
        );
        assert_eq!(
            parse_cdp_endpoint("https://192.168.0.5:9333/").unwrap(),
            CdpEndpointKind::HttpAuthority {
                host: "192.168.0.5".into(),
                port: 9333,
            }
        );
        assert_eq!(
            parse_cdp_endpoint("127.0.0.1:9222").unwrap(),
            CdpEndpointKind::HttpAuthority {
                host: "127.0.0.1".into(),
                port: 9222,
            }
        );
    }

    #[test]
    fn cdp_endpoint_from_env_selection() {
        // Ungesetzt -> None (lokaler Launch).
        std::env::remove_var("WEBAGENT_CDP_ENDPOINT");
        assert_eq!(cdp_endpoint_from_env(), None);

        // Leer / nur Whitespace -> None.
        std::env::set_var("WEBAGENT_CDP_ENDPOINT", "   ");
        assert_eq!(cdp_endpoint_from_env(), None);

        // Gesetzt -> Some (Remote-Verbindung, Whitespace getrimmt).
        std::env::set_var("WEBAGENT_CDP_ENDPOINT", " ws://host:9222/devtools/page/1 ");
        assert_eq!(
            cdp_endpoint_from_env(),
            Some("ws://host:9222/devtools/page/1".to_string())
        );

        // Aufräumen.
        std::env::remove_var("WEBAGENT_CDP_ENDPOINT");
        assert_eq!(cdp_endpoint_from_env(), None);
    }
}
