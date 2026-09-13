<!-- **Referenz: Beleg der T-913-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-913 Handoff — API-Bridge-Verdrahtung

- Task: T-913
- Owner: grok-agent
- Branch: `refactor/T-913-api-bridge-integrate`
- Claim: `origin/master` `d6fa01d`

## Gepruefte Vorgaenger

T-907 routing, T-908 provider_handlers, T-909 transport/wire, T-910 boundary,
T-911 tests.rs, T-912 Modulkarte — alle `done` auf master.

## Diff-Scope

- `src/api_bridge.rs`: `mod` + `use` der Kindmodule, `route_request` ueber
  `classify`/`BridgeRoute`, entfernte Duplikate (Handler, Auth, HTTP/SSE,
  Inline-Tests).
- `src/api_bridge/tests.rs`: `use std::io::{Read, Write}` (noetig nach
  Auslagerung, Trait-Scope).

Keine neue Fachlogik. Image/Audio/Lifecycle bleiben in der Root-Datei.

## Gates

Siehe `docs/proofs/T-913/gates.txt`.
