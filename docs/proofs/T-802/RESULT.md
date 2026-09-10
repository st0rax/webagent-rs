# T-802 — Einheitliches Fill/Verify/Submit ohne Doppelversand

Messung: 2026-09-10 · Owner: local/opencode · Branch: `feature/T-801-brain-contract`
Bezug: `docs/BRAIN_UNIFICATION_PLAN.md` Scheibe 2 (T-802), `docs/TASKBOARD.json`.

## Lieferumfang

- **`src/contract.rs`** — pure, browserfreie Send-State-Machine:
  - `classify_send_surface(text, composer_text, composer_contains, disabled)` mit
    `SendSurface { Empty, Complete, Consumed, Truncated, Missing, Disabled }`.
    Vergleicht den **vollstaendigen** Editor-Inhalt statt eines 8-Zeichen-Praefixes.
  - `normalize_editor_content` = `split_whitespace().join(" ")`: normalisiert **nur**
    Editor-Leerraum und laesst Codezeichen/Unicode unveraendert (`editor_matches`,
    `editor_is_prefix` in `src/browser/composer.rs` delegieren hierher).
  - `run_send_flow` mit begrenztem `SendBudget` (**default 5 Submit, 3 Refill**) und
    der Doppelversand-Regel: **nach konsumiertem Composer oder unklarem Submit wird
    nur noch beobachtet, nie blind nachgefuellt;** `Complete` bestaetigt nur bei
    Volltext-Konflikt pruefbar neu. Retries sind gezaehlt und im Fehlermapping
    dokumentiert (`NoProof`/`Truncated`/`Missing` -> `capture_submit_failure_trace`).
- **`src/browser/send.rs`** — `send_common` als einzige Gemeinsam-Schleife
  (Baseline erfassen, Wake, Consent-Dismiss, Fill/Verify/Submit bis Budget,
  `verify_submitted`), `SendFlowProfile` kapselt Editor-Faehigkeit + Submit-Gesten:
  kimi = `RichMultilineVerified/ButtonOnly`, generic = `FillContains/EnterThenButton`,
  gemini = `Gemini/AlternateButtonEnter`, qwen = `Qwen/AlternateButtonEnter`.
  `send_generic`/`send_gemini`/`send_qwen` sind nur noch dünne Wrapper auf
  `send_common`; die alten getrennten Schleifen und `wait_fill_composer` sind
  entfernt. Neue Composer-Evals (`composer_text`/`composer_contains`/
  `send_button_disabled`) sind gemeistert und in `src/browser/verify.rs` gemockt.

## Abnahmekriterien (Scheibe 2)

| Abnahmepunkt | Beleg |
|---|---|
| Multiline-Prompt | `multiline_editor_content_matches_after_whitespace_normalization` (contract.rs) |
| Unicode unveraendert | `normalization_never_alters_code_or_unicode_characters` (contract.rs) |
| Abgeschnittener Prompt → nichts senden | `truncated_prompt_is_never_sent` (contract.rs), real-gestuetzte Fehlermeldung „NICHTS abgesendet“ |
| Deaktivierter Button | `disabled_button_aborts_before_any_gesture` + `classify_disabled_only_when_text_is_complete` (contract.rs) |
| Verspaetete Bestaetigung | `delayed_confirmation_is_waited_and_sends_exactly_once` (contract.rs) |
| Doppelversand-Gegenprobe | obiger Beweis bei `!sent`/„nur beobachten“ + reale Logs (unten) |
| Fehlender Composer | `missing_composer_gives_up_without_sending` (contract.rs), Refill-Budget 3 |

## Pflichtgates (BRAIN_UNIFICATION_PLAN, Zeile 40)

| Gate | Ergebnis |
|---|---|
| `cargo test --lib` | **1372 passed, 0 failed, 1 ignored** (Kommando-Teil) |
| `cargo check --features tui` | bestanden |
| `cargo check --no-default-features` | bestanden |

## Gegenprobe gegen bestehende reale Logs

Auswertung der realen T-501-Belege unter `docs/proofs/T-501/` (u.a.
`chat_live_2026-09-03.jsonl`, `attachment_kimi_2026-09-05.json`,
`zai_send_fix_2026-09-03.jsonl`, `streaming_*.json`, `api_responses_*.json`):

1. `chat_live_2026-09-03.jsonl:8` (zai, 15:22:58): **„5 Versuche“**, Button deaktiviert,
   Composer voll, 56,5 s — der alte Blind-Refill-Lauf.
2. `attachment_kimi_2026-09-05.json:10` (kimi, attempts:1): dito „5 Versuche“,
   `ABSENDEKNOPF_DEAKTIVIERT`, 52 s.
3. `zai_send_fix_2026-09-03.jsonl`: Fix wirkt live (`passed`, 11,2 s) — Gegenstueck zur
   neuen Schleife.
4. `effort_survey_chatgpt_2026-09-06.txt`: „Reply ROUTED“ 3× als separate neue Chats —
   **kein** Doppelversand, jeweils eigene Test-Runs.

Bewertung: **Kein dokumentierter Einzelfall von Doppelversand**, aber die Risiko-
konstellation ist durch die beiden realen „5 Versuche“-Befunde direkt belegt. Das
neue Verhalten (nach unklarem Submit nur beobachten, Retries begrenzen) ist
konsistent mit dem, was die alten Logs als naechste Aktion getan haetten (blind
nachfuellen = potentieller Doppelversand).

## Ehrliche Grenze

Eine gruene Testsuite ersetzt keine Live-Matrix: Echte Provider-Lueftungen gegen
frische Browser-Sitzungen bleiben T-807 (Endabnahme) vorbehalten.