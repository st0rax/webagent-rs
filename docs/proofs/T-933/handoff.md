<!-- **Referenz: Beleg der T-933-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-933 Handoff - response_protocol verdrahten

- Task: T-933
- Owner: local/opencode
- Branch: `refactor/T-933-wire-response-protocol` (gepusht, Claim-Voraussetzung erfuellt)
- Claim: `origin/master` `20f1d37` (Status `claimed`, owner `local/opencode`)

## Diff-Scope

- `src/api_bridge.rs`: `mod response_protocol;` ergaenzt; `pub(crate) use
  response_protocol::{...}` Re-Export der JSON-/SSE-Renderer
  (`anthropic_response`, `anthropic_sse`, `openai_message`, `openai_sse`,
  `response_object`, `response_object_from_answer`, `response_with_state`,
  `responses_sse_with_object`) plus `#[cfg(test)]` Re-Export von
  `responses_sse`; Root-Duplikate der neun Renderer entfernt (357 Zeilen).
  `sse_data`-Import aus dem Root-Wire-`use` entfernt (nur noch vom
  Kindmodul genutzt; ab jetzt `use super::wire::sse_data`).
- `src/api_bridge/response_protocol.rs`: Sichtbarkeit der Renderer von
  `pub(super)` auf `pub(crate)` angehoben (Geschwistermodul
  `provider_handlers` braucht sie); Modul-Doc zeigt T-933.

Die neun Renderer-Bodys sind byte-identisch zur bisherigen Root-Fassung
(geprueft gegen `origin/master` und den T-918-Extraktionsstand `191632e`) --
reiner Move, kein Verhaltenswechsel. Header und `sequence_number` bleiben in
`wire.rs`; keine Handler-Orchestrierung angefasst (Non-Goals eingehalten).

## Gates

Siehe `docs/proofs/T-933/gates.txt`. Clippy mit `-D warnings` gruen,
api_bridge::tests 51/51, voller lib-Lauf 1426 passed / 0 failed / 1 ignored.

## Nachzug (offen, Nicht-Scope von T-933)

- `docs/API_BRIDGE_ARCHITECTURE.md` Zeile 29 (`response_protocol.rs`
  "nein -> T-933") nach dem Muster von store/content/catalog auf
  "ja (T-933)" setzen (folgt als separater Docs-/Map-Sync).