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

use super::{HttpRequest, MAX_REQUEST_BYTES};
use std::{collections::BTreeMap, io::Read, net::TcpStream};

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
    use super::find_bytes;

    #[test]
    fn find_bytes_locates_header_terminator() {
        let buf = b"GET / HTTP/1.1\r\nHost: x\r\n\r\nbody";
        let at = find_bytes(buf, b"\r\n\r\n").unwrap();
        assert_eq!(&buf[at..at + 4], b"\r\n\r\n");
        assert!(find_bytes(b"nope", b"\r\n\r\n").is_none());
    }
}
