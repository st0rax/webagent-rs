> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.

# T-501 STATUS

Date: 2026-09-04
Harness: definitive verifier (capable-on-retry, max 3 Versuche, sauberer Token), Bridge 127.0.0.1:8790
API: webagent api serve /v1/chat/completions (stream) + /v1/responses

## streaming (ok:true, sauberer STREAM_OK)
- chatgpt
- claude
- deepseek
- gemini
- perplexity
- qwen

## streaming (ok:false)
- kimi: FAIL streaming chat: emits reasoning/instruction echo as delta stream (token appears only inside prose) - clean via non-stream + /v1/responses
- mistral: FAIL: emits wall-clock timestamp + keep-alive as delta stream; later circuit-open/capacity
- zai: FAIL: circuit_open (provider capacity, ~6h cooldown); no content emitted; passed 09-03

## api_responses (ok:true, sauberer RESP_OK)
- chatgpt
- claude
- deepseek
- gemini
- kimi
- perplexity
- qwen

## api_responses (ok:false)
- mistral: FAIL (flaky): clean RESP_OK earlier, then http200-empty or circuit-open/capacity - provider-side
- zai: FAIL: circuit_open (provider capacity, ~6h cooldown)

## Anmerkung (Root-Cause)
- Ausfallursache ist Provider-seitig, NICHT Bridge-Code: circuit_open/Capacity-Cooldowns (zai, mistral),
  Streaming-Chat-Transport-Quirks (kimi Reasoning-Echo, mistral Timestamp, chatgpt Truncation, claude Thinking).
- Bridge liefert korrektes SSE/Response-Objekt (200, sequence_number, [DONE]), relayt Token sobald Provider ihn liefert.
- Stabil sauber: deepseek/gemini/perplexity/qwen. kimi besteht /v1/responses (sauber), nur Streaming-Chat bricht ab.

## Counts
- streaming: 6/9 passed (chatgpt/claude flaky, kimi/mistral/zai blocken Streaming-Chat)
- api_responses: 7/9 passed (mistral/zai capacity-blockiert)
- DoD: **NICHT done** - Provider-Instabilitaet (Capacity/Cooldown/Transport) blockiert vollstaendige 9/9
