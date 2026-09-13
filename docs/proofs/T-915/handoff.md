<!-- **Referenz: Beleg der T-915-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-915 Handoff — Response-Store und Lifecycle

- Task: T-915
- Owner: grok-agent
- Branch: `refactor/T-915-api-bridge-store`
- Claim: `origin/master` `9595e35`

## Store-Invarianten

- Format `openai-local-state-v1`, Pfad `{data_dir}/openai-local-state-v1/{tenant}/store.json`
- Tenant-ID: FNV-1a des API-Keys
- Eviction nach Anzahl und Byte-Budget (Konstanten bleiben in der Root-Datei)
- retrieve/delete/input_items und previous_response_id sind tenant-isoliert
- Kein neues Speicherformat

## Dateiliste

| Datei | Rolle |
|---|---|
| `src/api_bridge/store.rs` | Handler retrieve/delete/input_items, Persistenz, `responses_context`, `append_response_message` |
| `src/api_bridge.rs` | unveraendert in diesem Slot; T-921 verdrahtet `mod store` |

## Gates

Siehe `docs/proofs/T-915/gates.txt`.
