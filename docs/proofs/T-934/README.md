# T-934: Pi bridge repair

Owner: chatgpt-codex. Claim published to origin/master in c8682d65585be1cc0919a97a12288f7aaa49ebba.
Repair base: e2a8cd2. Branch: fix/T-934-pi-bridge-roundtrip.

## Scope

The OpenAI chat handler now forwards client tool schemas and tool_choice to browser inference.
System/developer messages, assistant messages with null content and tool calls, and tool results
are retained as visible conversation context. Pi executes its own tools; the bridge returns calls.
The API server is headless by default; --debug enables visible windows.
WebView navigation uses its requested timeout plus two seconds instead of an unrelated eight-second cap.

## Verification

- 60 api_bridge tests passed, including pi_system_null_assistant_and_tool_error_roundtrip.
- cargo build --features webview passed.
- Real installed Pi with standard tools enabled returned PI_HI_OK without retry.
- Real Pi invoked read twice, successfully read read-fixture.txt, received ENOENT for the deliberately
  absent second file, and returned a final answer containing the exact nonce and the missing-file error.
- Logs: pi-hi.jsonl and pi-read.jsonl. These are real Pi events, not fake inference replies.

## Running artifact

Windows executable: C:/Users/storax/projects/GitHub/webagent-bridge-repair/runtime/webagent.exe

SHA256: 603489AB385E682CA195A81645E8DBB7B6E606D69B530D6D2DE2431547D62CBA

WebView2Loader.dll is deployed beside the EXE. Listener: 127.0.0.1:8788.
Arguments: api serve --brain chatgpt --timeout-secs 120.

## Boundaries

Live verification covers Pi's OpenAI chat API. Anthropic tool-use blocks are not part of this repair.
Malformed or unknown model-generated tool calls still fail validation; an empty envelope is not
silently reported as successful execution. Other providers and long-running coding sessions are
not certified by these tests. No taskboard completion is implied by a models-endpoint health check.
