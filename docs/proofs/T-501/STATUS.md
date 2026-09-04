> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.

# T-501 STATUS

Date: 2026-09-04
Harness: focused_rerun (finished)
API: webagent api serve on 127.0.0.1:8787

## streaming (ok:true, token STREAM_OK)
- claude
- gemini
- perplexity
- zai

## streaming (ok:false / timeout)
- chatgpt, deepseek, kimi, mistral, qwen

## api_responses (ok:true, token RESP_OK)
- perplexity

## api_responses (ok:false / timeout)
- claude, chatgpt, deepseek, gemini, kimi, mistral, qwen, zai

## Counts
- streaming: 4/9 passed
- api_responses: 1/9 passed
- T-501 DoD: NOT done
