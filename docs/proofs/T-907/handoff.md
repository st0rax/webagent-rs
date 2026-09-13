<!-- **Referenz: Beleg der T-907-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-907 Handoff — API-Bridge-Routing isolieren

- Task: T-907
- Owner: grok-agent
- Branch: `refactor/T-907-api-bridge-routing`
- Claim: `origin/master` `b29d69f` (2026-09-13)

## Vorher / Nachher

| Vorher (`src/api_bridge.rs`) | Nachher (`src/api_bridge/routing.rs`) |
|---|---|
| `route_request` entscheidet Methode/Pfad/Streaming inline | `classify(method, path, body) -> BridgeRoute` |
| `is_incremental_text_request` / `is_incremental_chat_request` lokal | dieselben Namen, gleiche Policy, im Routing-Modul |
| Handler-Aufrufe gemischt mit Dispatch | Handler bleiben in der Root-Datei; Varianten mapen 1:1 |

`src/api_bridge.rs` ist in diesem Slot **unveraendert** (Phase-10-Regel). T-913 verdrahtet `mod routing` und ersetzt den Inline-Dispatch durch `classify`.

## Geaenderte Dateien

- `src/api_bridge/routing.rs` (neu)
- `docs/proofs/T-907/` (dieser Beleg)
- Claim-Spiegel: `docs/TASKBOARD.json`, `docs/TASKBOARD.md` (bereits auf master)

## Gates

Siehe `docs/proofs/T-907/gates.txt` nach dem Lauf.
