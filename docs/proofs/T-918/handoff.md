<!-- **Referenz: Beleg der T-918-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-918 Handoff — JSON/SSE-Antwortkoerper

- Task: T-918
- Owner: grok-agent
- Branch: `refactor/T-918-api-bridge-protocol`
- Claim: `origin/master` `0701f6f`

## Renderer

`anthropic_response`, `openai_message`, `openai_sse`, `anthropic_sse`,
`response_object`, `response_object_from_answer`, `response_with_state`,
`responses_sse`, `responses_sse_with_object`.

`sequence_number` kommt weiter aus `wire::sse_data`. Keine Header-Aenderung.

## Datei

`src/api_bridge/response_protocol.rs` — T-921 verdrahtet `mod response_protocol`.

## Gates

Siehe `docs/proofs/T-918/gates.txt`.
