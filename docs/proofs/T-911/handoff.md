<!-- **Referenz: Beleg der T-911-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-911 Handoff — API-Bridge-Tests auslagern

- Task: T-911
- Owner: grok-agent
- Branch: `refactor/T-911-api-bridge-tests`
- Claim: `origin/master` `c041ed7` (2026-09-13)

## Testanzahl

| Stand | `#[test]` in API-Bridge |
|---|---|
| Vorher (`src/api_bridge.rs` `mod tests`) | 50 |
| Nachher (`src/api_bridge/tests.rs`) | 50 |
| Geloscht/abgeschwaecht | 0 |

Die Root-Datei bleibt in diesem Slot unveraendert (Phase-10-Regel). T-913 ersetzt
`#[cfg(test)] mod tests { ... }` durch `#[cfg(test)] mod tests;` (Datei
`src/api_bridge/tests.rs`).

## Modulgliederung (Abschnitte in einer Datei, Scope nur tests.rs)

- Prompt / Content
- Katalog / Auto-Router
- Tools / Protokoll
- Routing / Streaming / Store
- Auth / Fehler / Transport
- SDK-Blackbox

## Gates

Siehe `docs/proofs/T-911/gates.txt`.
