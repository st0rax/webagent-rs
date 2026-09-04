> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.

# T-501 STATUS

Date: 2026-09-04
Harness: run_stream_responses.py (finished)
API: webagent api serve healthy on 127.0.0.1:8787

## streaming (ok:true, token STREAM_OK)
- perplexity
- zai

## streaming (false positive, UI chrome)
- chatgpt (delta was 'Erneut versuchen', not STREAM_OK)

## streaming (ok:false / timeout)
- claude, deepseek, gemini, kimi, mistral, qwen

## api_responses (ok:true, token RESP_OK)
- perplexity

## api_responses (ok:false / timeout)
- chatgpt, claude, deepseek, gemini, kimi, mistral, qwen, zai

## Counts
- streaming: 2/9 passed
- api_responses: 1/9 passed
- T-501 DoD: NOT done
