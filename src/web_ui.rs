//! Lokale Web-UI: eingebettete Assets und Loopback-HTTP.
//!
//! T-201: eine portable Binary, kein Dateibaum neben der exe.
//! T-202: `/api/*` sitzt auf SessionService/Doctor (siehe `web_ui_api`).
//! T-203: Grok-Layout-Prototyp in `web/index.html` (Fake, ohne Backend).

use crate::web_ui_api::{dispatch, UiState};
use std::{
    io::Write,
    net::{SocketAddr, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

const INDEX_HTML: &str = include_str!("../web/index.html");
const HEALTH_JSON: &[u8] = br#"{"status":"ok","ui":"embedded"}"#;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Standard-Loopback-Port der Web-UI und der/des gemeinsamen API-Bridge-Servers.
pub const DEFAULT_PORT: u16 = 8788;

/// Laufzeitkonfiguration. `bind` muss Loopback sein.
///
/// `api_bridge`: optionale OpenAI-/Anthropic-kompatible `/v1/*`-Rolle auf
/// demselben Listener (Token-Schutz bleibt). Wenn gesetzt, bedient `/health`
/// die Bridge-Health (Skripte fragen `service` ab); ohne gesetzte API-Rolle
/// liefert `/health` das UI-Asset.
#[derive(Clone, Debug)]
pub struct UiConfig {
    pub bind: SocketAddr,
    pub open_browser: bool,
    pub api_bridge: Option<crate::api_bridge::BridgeConfig>,
}

/// Eingebettete Startseite (Compile-Zeit, keine Datei zur Laufzeit).
pub fn index_html() -> &'static str {
    INDEX_HTML
}

/// Liefert Content-Type und Bytes fuer einen GET-Pfad, oder `None`.
pub fn lookup(path: &str) -> Option<(&'static str, &'static [u8])> {
    let path = path.split('?').next().unwrap_or(path);
    match path {
        "/" | "/index.html" => Some(("text/html; charset=utf-8", INDEX_HTML.as_bytes())),
        "/health" => Some(("application/json", HEALTH_JSON)),
        _ => None,
    }
}

/// Bindet nur Loopback. `0.0.0.0` und aehnliches werden abgelehnt.
pub fn bind_listener(addr: SocketAddr) -> Result<TcpListener, String> {
    if !addr.ip().is_loopback() {
        return Err("Web-UI darf nur an eine Loopback-Adresse binden.".to_string());
    }
    TcpListener::bind(addr).map_err(|error| format!("Web-UI nicht bindbar: {error}"))
}

/// Startet den Dienst und blockiert.
pub fn serve(config: UiConfig) -> Result<(), String> {
    if let Some(bridge) = &config.api_bridge {
        crate::api_bridge::validate_bridge_config(bridge)?;
    }
    let listener = bind_listener(config.bind)?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("gebundene Adresse unlesbar: {error}"))?;
    let url = format!("http://{local}/");
    eprintln!("[ui] Web-UI auf {url} (eingebettete Assets)");
    if config.api_bridge.is_some() {
        eprintln!("[ui] API-Rolle aktiv: /v1/* auf {url} (Bearer-Schutz)");
    }
    if config.open_browser {
        open_browser(&url);
    }
    let state = Arc::new(UiState::default());
    let limiter = Arc::new(crate::api_bridge::ConnectionLimiter::default());
    let guard = Arc::new(config);
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let state = Arc::clone(&state);
                let limiter = Arc::clone(&limiter);
                let config = Arc::clone(&guard);
                thread::spawn(move || {
                    if let Err(error) = handle_connection(&mut stream, &state, &config, &limiter) {
                        eprintln!("[ui] Anfrage verworfen: {error}");
                    }
                });
            }
            Err(error) => eprintln!("[ui] Verbindung nicht annehmbar: {error}"),
        }
    }
    Ok(())
}

fn open_browser(url: &str) {
    let result = {
        #[cfg(windows)]
        {
            std::process::Command::new("cmd")
                .args(["/c", "start", "", url])
                .spawn()
        }
        #[cfg(not(windows))]
        {
            std::process::Command::new("xdg-open").arg(url).spawn()
        }
    };
    if let Err(error) = result {
        eprintln!("[ui] Browser nicht geoeffnet: {error} — URL: {url}");
    }
}

fn handle_connection(
    stream: &mut TcpStream,
    state: &UiState,
    config: &UiConfig,
    limiter: &Arc<crate::api_bridge::ConnectionLimiter>,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Lese-Timeout: {error}"))?;
    stream
        .set_write_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Schreib-Timeout: {error}"))?;
    let request = match crate::api_bridge::read_http_request(stream) {
        Ok(request) => request,
        Err(error) => {
            crate::api_bridge::write_http_response(
                stream,
                crate::api_bridge::api_error(
                    crate::api_bridge::ApiFlavor::OpenAi,
                    400,
                    &format!("Ungueltige HTTP-Anfrage: {error}"),
                ),
            )?;
            return Ok(());
        }
    };

    let is_bridge_path = config.api_bridge.is_some()
        && (request.path == "/health" || request.path.starts_with("/v1"));
    if is_bridge_path {
        let Some(permit) = limiter.try_acquire() else {
            crate::api_bridge::write_http_response(stream, crate::api_bridge::overload_response())?;
            return Ok(());
        };
        let _permit = permit;
        let bridge = config
            .api_bridge
            .as_ref()
            .expect("is_bridge_path setzt api_bridge");
        return crate::api_bridge::route_request(stream, &request, bridge);
    }

    let path = request.path.as_str();
    let body = String::from_utf8_lossy(&request.body).into_owned();
    let (status, ctype, body_bytes) = if path.starts_with("/api/") {
        let resp = dispatch(&request.method, path, &request.query, &body, state);
        (resp.status, resp.content_type, resp.body)
    } else if let Some((ctype, body)) = lookup(path) {
        (200, ctype, body.to_vec())
    } else {
        (404, "text/plain; charset=utf-8", b"not found".to_vec())
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&body_bytes))
        .map_err(|error| format!("Schreiben: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpStream as StdTcp;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn index_ist_zur_compilezeit_eingebettet_und_klein() {
        let html = index_html();
        assert!(html.contains("WebAgent"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(
            html.len() < 48_000,
            "eingebettetes Prototyp-HTML muss kompakt bleiben (T-203), war {}",
            html.len()
        );
    }

    #[test]
    fn prototypen_layout_hat_health_leiste_und_kategorien() {
        let html = index_html();
        assert!(html.contains("Quellen 3 von 5 bereit"));
        assert!(html.contains("Sitzungen"));
        assert!(html.contains("Brains"));
        assert!(html.contains("Gruppen"));
        assert!(html.contains("Laeufe"));
        assert!(html.contains("composer-input"));
        assert!(html.contains("role=\"banner\""));
        assert!(html.contains("aria-live=\"polite\""));
        assert!(html.contains("prefers-reduced-motion"));
        assert!(html.contains("Zur Eingabe springen"));
        assert!(html.contains(":focus-visible"));
        assert!(html.contains("id=\"source-switch\""));
        assert!(html.contains("id=\"model-label\""));
        assert!(html.contains("id=\"mode-chat\""));
        assert!(html.contains("id=\"source-save\""));
        assert!(
            !html.contains("fetch("),
            "T-203/T-602: Quellen-Schalter bleibt lokal, kein Backend-Fetch"
        );
    }

    #[test]
    fn lookup_kennt_nur_eingebettete_pfade() {
        assert!(lookup("/").is_some());
        assert!(lookup("/index.html").is_some());
        assert!(lookup("/health").is_some());
        assert!(lookup("/missing").is_none());
    }

    #[test]
    fn nicht_loopback_wird_abgelehnt() {
        let addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let err = bind_listener(addr).unwrap_err();
        assert!(err.contains("Loopback"));
    }

    fn test_config(api_bridge: Option<crate::api_bridge::BridgeConfig>) -> UiConfig {
        UiConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            open_browser: false,
            api_bridge,
        }
    }

    fn test_bridge_config() -> crate::api_bridge::BridgeConfig {
        crate::api_bridge::BridgeConfig {
            bind: "127.0.0.1:0".parse().unwrap(),
            brain: "chatgpt".to_string(),
            timeout_secs: None,
            headless: true,
            api_key: "test-secret".to_string(),
            fake_reply: None,
        }
    }

    #[test]
    fn get_index_ohne_dateien_auf_der_platte() {
        let listener = bind_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok(mut stream) = listener.accept().map(|(s, _)| s) {
                let state = UiState::default();
                let config = test_config(None);
                let limiter = Arc::new(crate::api_bridge::ConnectionLimiter::default());
                let _ = handle_connection(&mut stream, &state, &config, &limiter);
            }
        });
        thread::sleep(Duration::from_millis(20));
        let mut client = StdTcp::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).unwrap();
        assert!(out.contains("HTTP/1.1 200"));
        assert!(out.contains("WebAgent"));
        assert!(out.contains("eingebettet") || out.contains("Binary"));
    }

    fn one_shot(request: &[u8], api_bridge: Option<crate::api_bridge::BridgeConfig>) -> String {
        let listener = bind_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok(mut stream) = listener.accept().map(|(s, _)| s) {
                let state = UiState::default();
                let config = test_config(api_bridge);
                let limiter = Arc::new(crate::api_bridge::ConnectionLimiter::default());
                let _ = handle_connection(&mut stream, &state, &config, &limiter);
            }
        });
        thread::sleep(Duration::from_millis(20));
        let mut client = StdTcp::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
        client.write_all(request).unwrap();
        let mut out = String::new();
        client.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn post_session_laeuft_ueber_http_und_session_service() {
        let body = br#"{"brain":"chatgpt","task":"hi"}"#;
        let req = format!(
            "POST /api/sessions HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            std::str::from_utf8(body).unwrap()
        );
        let out = one_shot(req.as_bytes(), None);
        assert!(out.contains("HTTP/1.1 201"), "{out}");
        assert!(out.contains("\"run_id\""), "{out}");
        assert!(out.contains("chatgpt"), "{out}");
    }

    #[test]
    fn health_brains_laeuft_ueber_http_ohne_browser() {
        let out = one_shot(
            b"GET /api/health/brains HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            None,
        );
        assert!(out.contains("HTTP/1.1 200"), "{out}");
        assert!(out.contains("\"timestamp\""), "{out}");
        assert!(out.contains("\"brains\""), "{out}");
    }

    #[test]
    fn api_rolle_bedient_v1_models_auf_gemeinsamen_listener() {
        let config = test_bridge_config();
        let out = one_shot(
            b"GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            Some(config),
        );
        assert!(out.contains("HTTP/1.1 401"), "{out}");
    }

    #[test]
    fn api_rolle_zugriff_mit_token_liefert_modellliste() {
        let config = test_bridge_config();
        let out = one_shot(
            b"GET /v1/models HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer test-secret\r\n\r\n",
            Some(config),
        );
        assert!(out.contains("HTTP/1.1 200"), "{out}");
        assert!(out.contains("\"object\":\"list\""), "{out}");
        assert!(out.contains("\"auto\""), "{out}");
    }

    #[test]
    fn health_ohne_api_rolle_liefert_ui_asset() {
        let out = one_shot(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n", None);
        assert!(out.contains("HTTP/1.1 200"), "{out}");
        assert!(out.contains("\"ui\":\"embedded\""), "{out}");
    }

    #[test]
    fn health_mit_api_rolle_liefert_bridge_health() {
        let config = test_bridge_config();
        let out = one_shot(
            b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            Some(config),
        );
        assert!(out.contains("HTTP/1.1 200"), "{out}");
        assert!(
            out.contains("\"service\":\"webagent-provider-bridge\""),
            "{out}"
        );
    }
}
