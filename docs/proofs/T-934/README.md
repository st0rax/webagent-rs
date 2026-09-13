# T-934: OpenAI Chat+Tools+Fortsetzung (surgical port)

Owner: spock. Branch: feature/T-934-pi-bridge-roundtrip.
Cherry-pick of cb51e1f onto fresh origin/master (no blind-merge of old fix branch / no T-923 junk).

## Scope

- System/developer context, assistant content:null + tool_calls, tool results/continuations
- Buffered OpenAI Chat Completions forwards client tools to browser_inference
- Bridge does not execute client tools locally
- T-914 deferred; no wire-rollback of store/content/inference

## Harness

- Covered: unit tests (incl. pi_system_null… and OpenAI-shaped prompt/tool replay)
- Pending: Live Pi re-run on this SHA; generic curl/SDK chat+tools blackbox
- Not Done: /v1/models-only

See handoff.md (Referenz-Banner) and live-status.md.
