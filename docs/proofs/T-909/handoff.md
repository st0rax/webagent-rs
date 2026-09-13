# T-909 Handoff — HTTP-Transport und SSE-Wire

- Task: T-909
- Owner: grok-agent
- Branch: `refactor/T-909-api-bridge-transport`
- Claim: `origin/master` `70dec83` (2026-09-13)

## Transport-API

| Modul | Funktionen | Verantwortung |
|---|---|---|
| `src/api_bridge/transport.rs` | `read_http_request`, `find_bytes` | HTTP/1.1-Parsing, Content-Length, Header-Ende CRLF-CRLF |
| `src/api_bridge/wire.rs` | `write_http_response`, `render_http_response`, `write_sse_headers`, `write_sse_event`, `sse_data`, `write_data_frame`, `write_sse_comment` | Status/Header/Body und SSE-Frames |

`src/api_bridge.rs` bleibt in diesem Slot unveraendert. T-913 verdrahtet `mod transport` und `mod wire`.

## Invarianten (unveraendert)

- JSON: `Content-Type: application/json; charset=utf-8`, `Content-Length`, `Connection: close`, `Cache-Control: no-store`, `X-Request-Id: req_…`
- Live-SSE: `text/event-stream; charset=utf-8`, `Cache-Control: no-cache`, kein `Content-Length`
- Responses-SSE: `sequence_number` ab 0, monoton, ohne Luecke
- Keine Timeoutwerte geaendert (`READ_TIMEOUT` bleibt in der Root-Datei)

## Gates

Siehe `docs/proofs/T-909/gates.txt`.
