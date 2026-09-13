<!-- **Referenz: Beleg der T-919-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-919 Handoff - Inference verdrahten

- Task: T-919
- Owner: grok-agent
- Branch: `feature/T-919-wire-inference`
- Claim: `origin/master` `3737b5f`
- Basis: `origin/master` nach #61 (`8aa43f8` content wired)

## Diff-Scope

- `src/api_bridge.rs`: `mod inference;` plus `use inference::{browser_run_lock, run_task_blocking, run_task_streaming}`;
  Root-Duplikate (`BROWSER_RUN_LOCKS`, `fake_inference_response`, `emit_fake_stream`,
  `run_task_blocking`/`streaming`, Auto-Attach-Helfer, `annotate_auto_routed_inference_error`) entfernt.
- `src/api_bridge/inference.rs`: Laufvertrag unveraendert; Lock-Hilfsfunktion `browser_run_lock`
  fuer Medienpfad in Root.
- Board: T-923 auf `done` (nach #61-Merge), T-919 bleibt claimed bis PR-Merge.

Produktion laeuft ueber das Kindmodul. Kein media/catalog/response_protocol/store-Umzug.

## Gates

Siehe `docs/proofs/T-919/gates.txt`.
