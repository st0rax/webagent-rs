<!-- **Referenz: Beleg der T-916-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-916 Handoff — Prompt- und Tool-Normalizer

- Task: T-916
- Owner: grok-agent
- Branch: `refactor/T-916-api-bridge-content`
- Claim: `origin/master` `af5fbd3`

## Kopplung

- Parent-Typen: `OpenAiRequest`, `AnthropicRequest`, `ResponsesRequest`, `ConversationMessage`, `PromptBundle`, `OpenAiTool`, `HttpResponse`
- `api_error_code` (Boundary)
- `browser_inference::{BrowserTool, BrowserToolChoice, BrowserAttachmentKind}`

## Nicht verschoben

- SSE/JSON-Antwortkörper (`openai_sse`, `anthropic_sse`, `openai_message`) → T-918
- `audio_mime` → T-914
- Store/`responses_context` → T-915
- `run_task_*` → T-919

Unsupported-Felder bleiben hart (`seed`, `n>1`, `logprobs`).

## Gates

Siehe `docs/proofs/T-916/gates.txt`.
