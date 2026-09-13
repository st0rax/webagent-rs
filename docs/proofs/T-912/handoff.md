<!-- **Referenz: Beleg der T-912-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-912 Handoff — API-Bridge-Architektur

- Task: T-912
- Owner: grok-agent
- Branch: `docs/T-912-api-bridge-architecture`
- Claim: `origin/master` `fc4daf9`

## Dokumentationsdiff

| Datei | Aenderung |
|---|---|
| `docs/API_BRIDGE_ARCHITECTURE.md` | neu: Modulkarte, Abhaengigkeitsrichtung, Nicht-Ziele, Gates, Risiken, Agenten-Einstieg |
| `START_HERE.md` | Pflicht-Lese 4b, Phase-10-Stand T-907–T-911 done, kein starrer HEAD-Hash |

## Quellen

- `src/api_bridge.rs` und `src/api_bridge/*.rs` (Stand nach T-911)
- `docs/API_BRIDGE.md` (Betriebsvertrag, unveraendert)
- `docs/TASKBOARD.json` Phase 10

## Offene Unsicherheiten

- Kindmodule sind vor T-913 nicht kompiliert (`mod` fehlt in der Root-Datei).
- Image/Audio/Lifecycle bleiben in der Root-Datei.

## Gates

`git diff --check`; Dokument gegen `src/api_bridge/` abgeglichen.
