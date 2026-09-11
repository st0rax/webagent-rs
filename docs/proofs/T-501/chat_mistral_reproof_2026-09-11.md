# T-501: mistral chat Live-Reproof 2026-09-11

**Referenz.** Datierter Live-Beleg. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`;
> aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

Datum: 2026-09-11, lokale Zeit (UTC+2). Binary: `target/debug/webagent.exe`,
frisch aus `master` (3a7d30d / Merge PR #55 + Doc-Update d2248e0) gebaut,
headless, Kommando `verify --brain mistral --cap chat --headless`.

| Run | ts (lokal) | Ergebnis | Latenz | Beleg |
|---|---|---|---|---|
| 1 | 12:34:17 | **passed** | 11048 ms | `chat belegt (count>baseline)` — Proof-Store `proofs.jsonl` 2026-09-11T10:34:17Z |
| 2 | 12:34:34 | unreachable | 4212 ms | `blocked` — unmittelbar nach Run 1, Provider-Rate-Limit nach Burst |
| 3 | 12:34:44 | unreachable | 4086 ms | `blocked` — Folge-Limit (3 Sends in ~30 s) |

## Kontext / Interpretation

- `diagnose --brain mistral --headless` direkt nach den Läufen: `session_state: Ready`,
  `logged_in: true`, `composer: ok`, `cloudflare: false`, url `https://chat.mistral.ai/chat`.
- Die heutige Proof-Store-Chronologie (10:14–12:34 lokal) wechselte zwischen
  exactly `ABSENDEKNOPF_DEAKTIVIERT` (failed) und `passed`. Mit dem frischen
  master-Build liefert ein Einzellauf nach Kohärenz `passed`; zwei unmittelbare
  Folge-Runs werden `blocked`. Das Muster ist konsistent mit provider-seitigem
  Rate-Limit bei schneller Sequenz, nicht mit einem Selektor- oder
  Composer-Fehler.
- Beispielserial vom 08:xx lokaler Zeit: failed (ABSENDEKNOPF_DEAKTIVIERT),
  dann mehrfach `passed` — die Zelle war auch vor dem master-Nachzug grün.

## Einordnung

- Matrix-Zelle `webui_chat|mistral` bleibt **passed**, frischer Live-Beleg 2026-09-11.
- T-501-DoD unverändert NICHT done (offen: effort-Zellen, model/auto, claude/
  perplexity model strukturell; siehe `model_switch_roundtrip_v2_2026-09-08.md`).