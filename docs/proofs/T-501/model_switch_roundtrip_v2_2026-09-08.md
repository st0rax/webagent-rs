> **Referenz.** Datierter Live-Reproof-Beleg. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`; aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

# T-501: Modellwahl Live-Reproof (roundtrip-v2) 2026-09-08

Datum: 2026-09-08. Arbeitsbranch `fix/T-501-model-proof` (rekonsolidiert auf
`origin/master` `a3036db` / Merge PR #54). Messbinary: `target/debug/webagent.exe`
(roundtrip-v2-Verifier aus `capability_proof.rs`).

## Messung

Seriell, headless, je Brain `webagent verify --cap model_switch --brain <id> --headless`:

| Brain | Ergebnis | Latenz | Beleg (Proof-Store `%LOCALAPPDATA%\webagent\data\capability\proofs.jsonl`) |
|---|---|---|---|
| zai | failed | 1310 ms | `model_switch` 2026-09-08T06:37:20Z, `button[aria-label='Select a model']` |
| claude | failed | 1163 ms | 2026-09-08T06:37:38Z, `button[data-testid='model-selector-dropdown']` |
| qwen | failed | 2283 ms | 2026-09-08T06:37:42Z, `[aria-label='Select Model']` |
| perplexity | failed | 2030 ms | 2026-09-08T06:37:48Z, `button[aria-label*='Modell' i]` |
| gemini | failed | 2485 ms | 2026-09-08T06:37:53Z, `button[aria-label*='Modusauswahl' i]` |
| kimi | failed | 1636 ms | 2026-09-08T06:37:57Z, `div.current-model` |

Einheitlicher Grund: **"Aktuelles Modell nicht eindeutig in der Laufzeitliste
erkennbar; kein Wechsel versucht"** — der roundtrip-v2-Verifier verlangt
tatsaechlichen Laufzeit-Wechsel + unabhaengiges Nachlesen + Restore in derselben
Sitzung; bei fehlender Eindeutigkeit bricht er fail-closed ab, ohne in die UI
einzugreifen. Keine Chatnachricht wurde gesendet, kein Profil veraendert, kein
Wechsel versucht.

## Bewertung

- Deterministisches, konsistentes Fehlverhalten ueber alle sechs Brains — kein
  Provider-Ausfall, kein flaky-Netz, kein Login-Problem (alle `diagnose` headless:
  `Ready`, `logged_in: true`, `composer: ok`, `cloudflare: false`).
- Ursache liegt in der Selektoren-Ebene: das aktive Modell wird nicht
  zuverlaessig aus der Laufzeitliste identifiziert (Menue-Knopf gefunden, aber
  aktuelle Selektion/Listen-Eindeutigkeit fehlt).
- Sechs Matrix-Zellen wechseln von `not_run` auf `failed` (gemessen, nicht
  bestanden). Gesamtstand: **103 passed, 16 failed, 4 unreachable, 7 not_run**.
- T-501 DoD ("Pro beworbenes Brain gruen") bleibt NICHT done. Folge-Arbeit:
  Selektoren/Listen-Lesen je Brain nachziehen und Roundtrip erneut messen.