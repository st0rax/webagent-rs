//! HTTP-Antwortheader und SSE-Frames der Provider-Bridge.
//!
//! # Modulgrenze
//!
//! Dieses Modul schreibt Bytes: Statuszeile, Pflichtheader und SSE-Frames.
//! Es aendert keine Handlerlogik, keine Timeoutwerte und keine Headervertraege
//! (`Content-Type`, `Content-Length`, `Connection: close`, `Cache-Control`,
//! `X-Request-Id`). T-913 verdrahtet `mod wire`.
//!
//! Invarianten:
//! - JSON-Antworten: `Cache-Control: no-store`, `Content-Length` = Body-Laenge.
//! - Live-SSE-Header: `Content-Type: text/event-stream; charset=utf-8`,
//!   `Cache-Control: no-cache`.
//! - Responses-SSE: `sequence_number` ab 0, monoton, ohne Luecke.

use super::{completion_id, HttpResponse};
use serde_json::{json, Value};
use std::{io::Write, net::TcpStream};

/// Schreibt eine vollstaendige HTTP-Antwort (Header + Body) und flusht.
pub fn write_http_response(stream: &mut TcpStream, response: HttpResponse) -> Result<(), String> {
    let bytes = render_http_response(&response);
    stream
        .write_all(&bytes)
        .map_err(|error| format!("HTTP-Antwort nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("HTTP-Antwort nicht abschliessbar: {error}"))
}

/// Baut die Drahtbytes einer HTTP-Antwort (unverändertes Header-Set).
pub fn render_http_response(response: &HttpResponse) -> Vec<u8> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let request_id = completion_id("req");
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Request-Id: {}\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
        request_id
    );
    let mut bytes = headers.into_bytes();
    bytes.extend_from_slice(&response.body);
    bytes
}

/// SSE-Antwortanfang ohne Content-Length (unklarer Stream).
pub fn write_sse_headers(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Request-Id: {}\r\n\r\n",
                completion_id("req")
            )
            .as_bytes(),
        )
        .map_err(|error| format!("SSE-Header nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Header nicht flushbar: {error}"))
}

/// Ein benanntes SSE-Event mit monotonem `sequence_number`.
pub fn write_sse_event(
    stream: &mut TcpStream,
    event: &str,
    data: Value,
    seq: &mut u64,
) -> Result<(), String> {
    let frame = format!("event: {event}\ndata: {}\n\n", sse_data(event, data, seq));
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Event nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Event nicht flushbar: {error}"))
}

/// Responses-SSE: `sequence_number` steigt streng monoton ab 0, ohne Luecke.
pub fn sse_data(event: &str, mut data: Value, seq: &mut u64) -> Value {
    if data.get("type").is_none() {
        data["type"] = json!(event);
    }
    data["sequence_number"] = json!(*seq);
    *seq += 1;
    data
}

/// Ein `data:`-Frame (Chat-Completions-Chunks).
pub fn write_data_frame(stream: &mut TcpStream, data: Value) -> Result<(), String> {
    let frame = format!("data: {data}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Datenframe nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Datenframe nicht flushbar: {error}"))
}

/// SSE-Kommentar / Keep-Alive.
pub fn write_sse_comment(stream: &mut TcpStream, comment: &str) -> Result<(), String> {
    let frame = format!(": {comment}\n\n");
    stream
        .write_all(frame.as_bytes())
        .map_err(|error| format!("SSE-Keepalive nicht schreibbar: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("SSE-Keepalive nicht flushbar: {error}"))
}

#[cfg(test)]
mod tests {
    use super::sse_data;
    use serde_json::json;

    #[test]
    fn sse_sequence_numbers_are_monotonic_from_zero() {
        let mut seq = 0u64;
        let first = sse_data("response.created", json!({}), &mut seq);
        let second = sse_data(
            "response.in_progress",
            json!({"type": "response.in_progress"}),
            &mut seq,
        );
        assert_eq!(first["sequence_number"], 0);
        assert_eq!(first["type"], "response.created");
        assert_eq!(second["sequence_number"], 1);
        assert_eq!(second["type"], "response.in_progress");
        assert_eq!(seq, 2);
    }

    #[test]
    fn json_response_header_contract_is_stable() {
        let headers = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Request-Id: {}\r\n\r\n",
            200,
            "OK",
            "application/json; charset=utf-8",
            11,
            "req_test"
        );
        assert!(headers.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(headers.contains("Content-Type: application/json; charset=utf-8\r\n"));
        assert!(headers.contains("Content-Length: 11\r\n"));
        assert!(headers.contains("Connection: close\r\n"));
        assert!(headers.contains("Cache-Control: no-store\r\n"));
        assert!(headers.contains("X-Request-Id: req_test\r\n"));
    }

    #[test]
    fn live_sse_header_contract_is_stable() {
        let headers = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: close\r\nX-Request-Id: {}\r\n\r\n",
            "req_test"
        );
        assert!(headers.contains("Content-Type: text/event-stream; charset=utf-8\r\n"));
        assert!(headers.contains("Cache-Control: no-cache\r\n"));
        assert!(!headers.contains("Content-Length:"));
    }
}
