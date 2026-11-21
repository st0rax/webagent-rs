//! HTTP-Request-Parsing der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Dieses Modul liest Bytes vom Socket in eine [`HttpRequest`]-Huelle. Es
//! aendert keine Timeoutwerte, keine Handlerlogik und keine Headervertraege.
//! T-913 verdrahtet `mod transport`. Die Typen bleiben bis dahin in
//! `src/api_bridge.rs`; hier gelten dieselben Limits (`MAX_REQUEST_BYTES`).
//!
//! Invarianten:
//! - Header endet bei CRLF-CRLF; Body folgt `Content-Length`.
//! - `transfer-encoding` ausser `identity` wird abgelehnt.
//! - Query wird von der Pfadzeile getrennt, nicht ins Routing gemischt.
//! - T-946: die Verbindung wird waehrend des Browserturns beobachtet. Ein
//!   verwaisster Turn beendet sich selbst und gibt die Brain-Sperre frei.

use super::{HttpRequest, MAX_REQUEST_BYTES};
#[cfg(test)]
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    net::TcpStream,
    sync::Mutex,
    time::Duration,
};

/// Beobachtet, ob der HTTP-Aufrufer noch verbunden ist (T-946).
///
/// Bewusst ein Trait: die Bridge benutzt einen echten TCP-Socket, Tests einen
/// schaltbaren Stub. Damit ist der Beleg "Folgeanfrage wartet nicht auf einen
/// verwaisten Turn" ohne echten Browser und ohne Zugangsdaten pruefbar.
pub trait ClientPresence: Send + Sync {
    fn connected(&self) -> bool;
}

/// Immer verbunden — fuer `fake_reply` und Pfade ohne echten Socket.
pub struct AlwaysConnected;

impl ClientPresence for AlwaysConnected {
    fn connected(&self) -> bool {
        true
    }
}

/// Kurzer Peek-Timeout: ein EOF liegt sofort vor, ein ruhender aber
/// lebendiger Client liefert `WouldBlock`/`TimedOut` und gilt als verbunden.
const PRESENCE_PEEK_TIMEOUT: Duration = Duration::from_millis(20);

/// Echte TCP-Beobachtung ueber einen zweiten Handle auf demselben Socket.
pub struct TcpClientPresence {
    probe: Mutex<TcpStream>,
    restore_read_timeout: Option<Duration>,
}

impl TcpClientPresence {
    /// Zweiten Handle ableiten. Bei Misserfolg gilt der Client als verbunden —
    /// lieber einen echten Turn als einen falschen Abbruch.
    pub fn new(stream: &TcpStream) -> Option<Self> {
        Some(Self {
            probe: Mutex::new(stream.try_clone().ok()?),
            restore_read_timeout: stream.read_timeout().ok().flatten(),
        })
    }
}

impl ClientPresence for TcpClientPresence {
    fn connected(&self) -> bool {
        let Ok(probe) = self.probe.lock() else {
            return true;
        };
        // Der Handle teilt sich die Socket-Optionen mit dem Haupthandle. Wir
        // setzen das Lese-Timeout nur fuer diesen Peek und stellen es sofort
        // wieder her; waehrend des Turn liest niemand sonst von dem Socket.
        let _ = probe.set_read_timeout(Some(PRESENCE_PEEK_TIMEOUT));
        let mut byte = [0u8; 1];
        let peeked = probe.peek(&mut byte);
        let _ = probe.set_read_timeout(self.restore_read_timeout);
        match peeked {
            Ok(0) => false, // FIN: der Aufrufer ist weg
            Ok(_) => true,  // pipelined Daten: definitiv verbunden
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                true // keine Daten, aber kein EOF
            }
            Err(_) => false, // Reset oder anderer Socketfehler
        }
    }
}

/// Schaltbarer Stub fuer Tests (kein Socket, keine Browser-Abhaengigkeit).
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct TestPresence(pub Arc<AtomicBool>);

#[cfg(test)]
impl TestPresence {
    pub(crate) fn connected(value: bool) -> Self {
        Self(Arc::new(AtomicBool::new(value)))
    }

    pub(crate) fn set(&self, value: bool) {
        self.0.store(value, Ordering::SeqCst);
    }
}

#[cfg(test)]
impl ClientPresence for TestPresence {
    fn connected(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Liest eine HTTP/1.1-Anfrage (Header + Body nach Content-Length).
pub fn read_http_request(stream: &mut TcpStream) -> Result<HttpRequest, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let header_end;
    loop {
        let count = stream
            .read(&mut buffer)
            .map_err(|error| format!("HTTP-Header nicht lesbar: {error}"))?;
        if count == 0 {
            return Err("Verbindung vor vollstaendigem HTTP-Header geschlossen.".to_string());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err("HTTP-Header ist zu gross.".to_string());
        }
        if let Some(index) = find_bytes(&bytes, b"\r\n\r\n") {
            header_end = index + 4;
            break;
        }
    }

    let header_text = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| "HTTP-Header ist nicht UTF-8/ASCII.".to_string())?;
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| "HTTP-Request-Line fehlt.".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "HTTP-Methode fehlt.".to_string())?
        .to_string();
    let path = parts.next().ok_or_else(|| "HTTP-Pfad fehlt.".to_string())?;
    let (path, query) = match path.split_once('?') {
        Some((path_part, query_part)) => (path_part.to_string(), query_part.to_string()),
        None => (path.to_string(), String::new()),
    };
    let version = parts.next().unwrap_or("");
    if !version.starts_with("HTTP/1.") || parts.next().is_some() {
        return Err("Ungueltige HTTP-Request-Line.".to_string());
    }

    let mut headers = BTreeMap::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "Ungueltiger HTTP-Header.".to_string())?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }
    if headers
        .get("transfer-encoding")
        .is_some_and(|value| !value.eq_ignore_ascii_case("identity"))
    {
        return Err("Chunked HTTP-Anfragen werden nicht unterstuetzt.".to_string());
    }
    let content_length = headers
        .get("content-length")
        .map(|value| {
            value
                .parse::<usize>()
                .map_err(|_| "Ungueltige Content-Length.".to_string())
        })
        .transpose()?
        .unwrap_or(0);
    if content_length > MAX_REQUEST_BYTES {
        return Err("HTTP-Body ist zu gross.".to_string());
    }
    while bytes.len() - header_end < content_length {
        let remaining = content_length - (bytes.len() - header_end);
        let read_len = remaining.min(buffer.len());
        let count = stream
            .read(&mut buffer[..read_len])
            .map_err(|error| format!("HTTP-Body nicht lesbar: {error}"))?;
        if count == 0 {
            return Err("Verbindung vor vollstaendigem HTTP-Body geschlossen.".to_string());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    Ok(HttpRequest {
        method,
        path,
        query,
        headers,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

/// Erstes Vorkommen von `needle` in `haystack`.
pub fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::{find_bytes, ClientPresence, TcpClientPresence};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
        time::Duration,
    };

    #[test]
    fn find_bytes_locates_header_terminator() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        let at = find_bytes(buf, b"\r\n\r\n").unwrap();
        assert_eq!(&buf[at..at + 4], b"\r\n\r\n");
        assert!(find_bytes(b"nope", b"\r\n\r\n").is_none());
    }

    /// DoD T-946: ein geschlossener Peer wird am echten Socket erkannt —
    /// ohne Browser, ohne Zugangsdaten, nur lokal auf Loopback.
    #[test]
    fn tcp_presence_erkennt_geschlossenen_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let addr = listener.local_addr().expect("addr");
        let mut client = TcpStream::connect(addr).expect("connect");
        let (mut server, _) = listener.accept().expect("accept");
        server
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("read timeout");
        let presence = TcpClientPresence::new(&server).expect("clone");
        assert!(presence.connected(), "offener Client gilt als verbunden");

        client.write_all(b"x").expect("keepalive byte");
        assert!(presence.connected(), "Daten bedeuten: verbunden");
        // Wie read_http_request: die angekommenen Bytes verbrauchen, sonst
        // verdeckt das unaufgelesene Byte im Peek-Puffer das spaetere EOF.
        let mut drained = [0u8; 1];
        server.read_exact(&mut drained).expect("byte verbrauchen");
        assert_eq!(drained[0], b'x');

        drop(client);
        let mut gone = false;
        for _ in 0..40 {
            if !presence.connected() {
                gone = true;
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(gone, "EOF muss als getrennt erkannt werden");

        // Der Haupthandle behaelt trotzdem seinen Lese-Timeout (Restore).
        assert_eq!(
            server.read_timeout().expect("read timeout"),
            Some(Duration::from_secs(1))
        );
    }
}
