<!-- **Referenz: Beleg der T-922-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-922 Handoff — Store verdrahten

- Task: T-922
- Owner: grok-agent
- Branch: `refactor/T-922-wire-store`
- Claim: `origin/master` `fbb08a2`

## Diff-Scope

- `src/api_bridge.rs`: `mod store;` plus `use store::{...}`; Root-Funktionen
  retrieve/delete/tenant/persist/lifecycle-Handler entfernt.
- `src/api_bridge/store.rs`: unveraenderte Semantik (`openai-local-state-v1`).

Produktion laeuft ueber das Kindmodul. Kein content/media/inference-Umzug.

## Gates

Siehe `docs/proofs/T-922/gates.txt`.
