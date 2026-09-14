<!-- **Referenz: Beleg der T-931-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-931 Handoff - Catalog verdrahten

- Task: T-931
- Owner: local/opencode
- Branch: `refactor/T-931-wire-catalog` (gepusht, Claim-Voraussetzung erfuellt)
- Claim: `origin/master` `0b26212` (Status `claimed`, owner `local/opencode`)

## Diff-Scope

- `src/api_bridge.rs`: `mod catalog;` ergaenzt; `pub use catalog::{...}`
  Re-Export fuer Katalog/Auto-Router/Modellaufloesung; Root-Kopien
  (`available_brains`, `AutoPurpose`, `AutoRoute`, `classify_auto_route`,
  `first_available_auto_brain[_in]`, `select_auto_brain[_with_default]`,
  `select_auto_brain_for_cli`, `resolve_model`, `model_id`,
  `advertised_{input,output}_modalities`, `model_metadata`) entfernt.
- `src/api_bridge/catalog.rs`: Sichtbarkeit der verdrahteten Symbole von
  `pub(crate)`/`pub(super)` auf `pub` angehoben; Modul-Doc zeigt T-931.

Produktion laeuft ueber das Kindmodul. Kein neues Modell, kein HTTP-Routing.

## Gates

Siehe `docs/proofs/T-931/gates.txt`.

## Nachzug (offen, Nicht-Scope von T-931)

- `docs/API_BRIDGE_ARCHITECTURE.md` Zeile 28 (`catalog.rs` "nein -> T-931")
  nach dem Muster von T-922/T-923 auf "ja (T-931)" setzen (folgt als
  separater Docs-/Map-Sync, wie bei store/content gehandhabt).