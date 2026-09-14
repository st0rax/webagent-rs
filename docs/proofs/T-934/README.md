# T-934: Pi bridge repair

**Referenz:** Nachweis zu T-934, Stand 2026-09-14. Verbindlich sind START_HERE.md und docs/TASKBOARD.json.

Owner: spock (TASKBOARD.json, claimed 2026-09-14). Port: fix/T-934-port-master
auf origin/master 7b27105 (viewport-clamp composer click) — Scope aus cb51e1f
sauber uebertragen (content.rs, provider_handlers.rs, tests.rs, cli.rs, main.rs),
ohne Ballast. Alter Branch: fix/T-934-pi-bridge-roundtrip (Repair base e2a8cd2).

## Scope

The OpenAI chat handler now forwards client tool schemas and tool_choice to browser inference.
System/developer messages, assistant messages with null content and tool calls, and tool results
are retained as visible conversation context. Pi executes its own tools; the bridge returns calls.
The API server is headless by default; --debug enables visible windows.
WebView navigation uses its requested timeout plus two seconds instead of an unrelated eight-second cap.

## Verification (Port-Stand 2026-09-14)

- cargo fmt --all -- --check gruen.
- cargo clippy --features webview --all-targets -- -D warnings gruen.
- cargo test --features webview --lib api_bridge: 60 passed (inkl.
  pi_system_null_assistant_and_tool_error_roundtrip).
- cargo test --features webview --lib voll: 1401 passed, 6 failed — alle 6
  (5x browser::verify, 1x startup::betriebs_markdown) auch auf sauberem
  origin/master 7b27105 ohne Port vorhanden (pre-existing, kein Port-Regress).
- cargo build --features webview gruen.
- Live-Pi-Belege pi-hi.jsonl / pi-read.jsonl vom Vorgaenger-Stand (echte Pi-Events,
  PI_HI_OK ohne Retry, read-Fixture + ENOENT-Fortsetzung mit Nonce).
- Live-Reverifikation mit der neuen EXE (T-934-Logik + 7b27105-Composer-Fix)
  steht noch aus — das ist der naechste Schritt des Testers.

## Running artifact (Port-Build)

Windows executable: C:/Users/storax/projects/GitHub/webagent-rs-t934-final/runtime-new/webagent.exe

SHA256: AEA16F810B0D4795E10F1AA030C0400CBA5C010C817A69A33A41E47228D3FFE4

WebView2Loader.dll is deployed beside the EXE. Vorgeschlagener Parallel-Test
neben der laufenden :8788-Bridge:
runtime-new\webagent.exe api serve --brain chatgpt --timeout-secs 120 --port 8789
(Pi-Config dann auf http://127.0.0.1:8789/v1 zeigen). Alte :8788-Instanz
(PID 396, SHA 998093BD, Stand cb51e1f ohne 7b27105) bleibt unberuehrt.

## Boundaries

Live verification covers Pi's OpenAI chat API. Anthropic tool-use blocks are not part of this repair.
Malformed or unknown model-generated tool calls still fail validation; an empty envelope is not
silently reported as successful execution. Other providers and long-running coding sessions are
not certified by these tests. No taskboard completion is implied by a models-endpoint health check.
