//! Lokale Web-UI: eingebettete Assets und Loopback-HTTP.
//!
//! T-201: eine portable Binary, kein Dateibaum neben der exe. Endpunkte
//! (Session/Chat/…) kommen in T-202; das Grok-Layout in T-203.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread,
    time::Duration,
};

const INDEX_HTML: &str = include_str!("../web/index.html");
const HEALTH_JSON: &[u8] = br#"{"status":"ok","ui":"embedded"}"#;
const READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Standard-Loopback-Port der Web-UI (API-Bridge bleibt 8787).
pub const DEFAULT_PORT: u16 = 8788;

/// Laufzeitkonfiguration. `bind` muss Loopback sein.
#[derive(Clone, Debug)]
pub struct UiConfig {
    pub bind: SocketAddr,
    pub open_browser: bool,
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
    let listener = bind_listener(config.bind)?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("gebundene Adresse unlesbar: {error}"))?;
    let url = format!("http://{local}/");
    eprintln!("[ui] Web-UI auf {url} (eingebettete Assets)");
    if config.open_browser {
        open_browser(&url);
    }
    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                thread::spawn(move || {
                    if let Err(error) = handle_connection(&mut stream) {
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

fn handle_connection(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Lese-Timeout: {error}"))?;
    stream
        .set_write_timeout(Some(READ_TIMEOUT))
        .map_err(|error| format!("Schreib-Timeout: {error}"))?;
    let mut buf = [0u8; 2048];
    let n = stream
        .read(&mut buf)
        .map_err(|error| format!("Lesen: {error}"))?;
    let text = std::str::from_utf8(&buf[..n]).unwrap_or("");
    let path = parse_get_path(text).unwrap_or("/");
    let (status, ctype, body) = match lookup(path) {
        Some((ctype, body)) => (200, ctype, body),
        None => (404, "text/plain; charset=utf-8", b"not found" as &[u8]),
    };
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(body))
        .map_err(|error| format!("Schreiben: {error}"))
}

fn parse_get_path(request: &str) -> Option<&str> {
    let line = request.lines().next()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    if method.eq_ignore_ascii_case("GET") {
        Some(path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream as StdTcp;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn index_ist_zur_compilezeit_eingebettet_und_klein() {
        let html = index_html();
        assert!(html.contains("WebAgent"));
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(
            html.len() < 8_192,
            "Platzhalter-HTML muss klein bleiben (T-201 Budget), war {}",
            html.len()
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

    #[test]
    fn get_index_ohne_dateien_auf_der_platte() {
        let listener = bind_listener("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Ok(mut stream) = listener.accept().map(|(s, _)| s) {
                let _ = handle_connection(&mut stream);
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
}
