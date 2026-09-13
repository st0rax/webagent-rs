<!-- **Referenz: Beleg der T-925-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-925 Handoff — Modulkarte + START_HERE Sync

- Task: T-925
- Owner: pflege
- Branch: `docs/T-925-modulkarte-sync`

## Änderungen

- `docs/API_BRIDGE_ARCHITECTURE.md`: Dateitabelle inkl. store/content/catalog/response_protocol;
  **unwired ≠ done**; Phase-11 mit DEFERRED T-914/T-919; Phase-12-Nachzug;
  keine Fake-Live-Claims.
- `START_HERE.md`: Arbeitsstand Phase-11/12; „immer grün“ relativiert mit Linux-Flake / T-929;
  keine Fake-Live-Claims.
- Taskboard Claim/Done für T-925.

## Nicht angefasst

- Kein `src/api_bridge` Rust-Logik.

## Verifikation

Siehe `docs/proofs/T-925/gates.txt`.

## Columbo-Nachzug

- START_HERE: eingefrorenen Pass-Count 1388/0 entfernt.
- .env.example: WEBAGENT_API_KEY für CLI//v1/* dokumentiert; /api/* =
  trust-localhost (kein Bearer, kein Fake-Auth).

## Columbo QM follow-up

- Typo esponse_protocol -> response_protocol.
- v1.0/Verify-Zahl entschärft.
- T-919 freigegeben im Board (kein DEFERRED/USER-SKIP).

