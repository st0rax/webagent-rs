<!-- **Referenz: Beleg der T-917-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-917 Handoff — Katalog und Auto-Router

- Task: T-917
- Owner: grok-agent
- Branch: `refactor/T-917-api-bridge-catalog`
- Claim: `origin/master` `1db7636`

## Katalogfelder

`id` = `webagent/{brain}`, `owned_by=webagent`, `context_window=128000`,
`max_tokens=16384`, `modalities.input/output` nur laut bestaetigten Smokes.
Auto: `virtual=true` plus Routing-Hinweise, keine Live-Verfuegbarkeit.

## Auto-Regeln

Zweck/Attachments/Tools zuerst, dann Coding- bzw. Recherche-Marker, sonst
Default-Kette. Circuit-Breaker ueberspringt gesperrte Brains. Kein HTTP-Routing.

## Datei

`src/api_bridge/catalog.rs` — T-921 verdrahtet `mod catalog`. Root unveraendert.

## Gates

Siehe `docs/proofs/T-917/gates.txt`.
