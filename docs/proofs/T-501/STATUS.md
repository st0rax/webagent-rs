> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.

# T-501 STATUS

Date: 2026-09-04
Harness: focused_rerun + retry-consolidation (opencode), Bridge auf 127.0.0.1:8790
API: webagent api serve /v1/chat/completions (stream) + /v1/responses

## streaming (ok:true, clean token STREAM_OK)
- chatgpt
- claude
- deepseek
- gemini
- perplexity
- qwen

## streaming (ok:false)
- kimi: echoes the instruction text (token appears inside prose, not as exact reply) - false positive in naive substring match
- mistral: returns a wall-clock timestamp (e.g. 11:51) instead of the requested token - not a valid reply
- zai: returns empty stream (0 deltas) consistently across attempts - no content emitted

## api_responses (ok:true, clean token RESP_OK)
- chatgpt
- claude
- deepseek
- gemini
- perplexity
- qwen

## api_responses (ok:false)
- kimi: echoes the instruction text (token appears inside prose, not as exact reply) - false positive in naive substring match
- mistral: returns a wall-clock timestamp (e.g. 11:51) instead of the requested token - not a valid reply
- zai: returns empty stream (0 deltas) consistently across attempts - no content emitted

## Anmerkung
- claude intermittierend: gelegentlich 150s-Stall oder Warmup-Prosa, liefert auf Retry den sauberen Token.
- kimi/mistral/zai fehlgeschlagen: echo / Timestamp / leer (kein sauberer Token), als echtes Nichtbestehen gewertet.

## Counts
- streaming: 6/9 passed
- api_responses: 6/9 passed
- DoD: **NICHT done** - kimi/mistral/zai blockieren beide Flaeche (kein sauberer Token via Bridge)
