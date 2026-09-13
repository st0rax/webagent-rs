# T-908 Handoff — Provider-Handler trennen

- Task: T-908
- Owner: grok-agent
- Branch: `refactor/T-908-api-bridge-handlers`
- Claim: `origin/master` `f5e2149` (2026-09-13)

## Modulkarte

| Datei | Inhalt |
|---|---|
| `src/api_bridge/provider_handlers.rs` | `handle_openai`, `handle_openai_incremental`, `handle_anthropic`, `handle_responses`, `handle_responses_incremental` |
| `src/api_bridge.rs` | unveraendert in diesem Slot (Phase-10-Regel); T-913 verdrahtet `mod provider_handlers` |

Nicht verschoben (bewusst andere Slots / Root): Image/Audio, Lifecycle retrieve/delete/input_items, Auth, Transport/SSE-Helfer, Routing.

## Kopplungsliste (nur Parent)

`authorize`, `api_error`, `decode_json`, `reject_unsupported_openai_body`, `resolve_model`, Prompt-/Tool-Normalizer (`openai_*`, `anthropic_*`, `responses_*`), `run_task_blocking`, `run_task_streaming`, SSE-Schreibhelfer, `store_response` / `tenant_id` / `responses_context`, `session_service`. Keine Browser-Selektoren, keine Providerprofile.

## Gates

Siehe `docs/proofs/T-908/gates.txt`.
