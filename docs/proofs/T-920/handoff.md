<!-- **Referenz: Beleg der T-920-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-920 Handoff — Modulkarte Phase 11

- Task: T-920
- Owner: grok-agent
- Branch: `docs/T-920-api-bridge-architecture`
- Claim: `origin/master` `4799dc1`

## Dokumentationsdiff

| Datei | Aenderung |
|---|---|
| `docs/API_BRIDGE_ARCHITECTURE.md` | Dateitabelle mit `mod`-Spalte; unwired≠done; T-914/T-919 deferred; T-921-Kanten |
| `START_HERE.md` | aktueller Stand, keine freien T-914–T-920-pauschal; Orphan-Hinweis |

## Quellen

`src/api_bridge.rs` (`mod` nur boundary/provider_handlers/routing/tests/transport/wire)
plus Dateien `store.rs`, `content.rs`, `catalog.rs`, `response_protocol.rs`.
Board: T-921 `depends_on` T-915/916/917/918/920/922/923/931/933.

## Unsicherheiten

- T-925 überlappt thematisch (Phase-12-Liste, TASKBOARD.md). T-920 bleibt auf
  Architektur + START_HERE; Phase-12-Details stehen im Board.
- T-933 steht im Board noch `free`; die Karte nennt ihn als Verdrahtungs-Nachzug.

## Gates

`git diff --check`; Banner-Test `betriebs_markdown_hat_eine_wahrheit`.
