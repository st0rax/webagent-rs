> **Archiv** — T-803 (Scheibe 3) als Code umgesetzt und belegt (2026-09-10).
> Messprotokoll bleibt fuer Gegenprobe und Regression; lebender Stand:
> `docs/CURRENT_WORK.md`, `docs/TASKBOARD.md`.

# T-803 — Antwortstream durch Controller, REPL, Swarm, UI und API

Messung: 2026-09-10 · Owner: local/opencode · Branch: `feature/T-803-answer-stream`
Bezug: `docs/BRAIN_UNIFICATION_PLAN.md` Scheibe 3 (T-803), `docs/TASKBOARD.json`.

## Kernentscheidungen

- **Ein gemeinsamer Snapshot->Edit-Strom fuer alle Einstiegspunkte.**
  `contract::StreamJournal` (zustandsbehafteter Beobachter) konsumiert die
  rohen DOM-Snapshots und uebersetzt sie mit `contract::classify_edit` in
  `StreamEdit::Append`/`Replace`. Controller, Relay, Web-UI und API laufen
  ueber denselben Klassifikator; Appends erscheinen als `TextDelta`, Revisionen
  als `SessionEvent::TextReplace`.
- **Transportvertrag: kumulative Snapshots.** `on_update` traegt stets das
  vollstaendige Antwort-Abbild (wie der echte Browser-Poll), nie einzelne
  Fragmente. FakeBrain `Text`/`Stream`-Szenarien wurden darauf umgestellt;
  ein nicht-Praefix-Wachstum ist eine Revision (Replace), nie Konkatenation.
- **Diagnose ist nie eine Antwort.** Nach der Antwort wird `classify_surface`
  angewendet; nur `SurfaceKind::Content` zaehlt als Erfolg/`TextComplete`.
  HTML-Rohbeleg, Login-Wand, Captcha, Limit und leere Antworten werden als
  Fehler gemeldet (502/Retry), nie als fertige Antwort ausgegeben.
- **Additiver Draht kann keine Revision.** Der OpenAI-kompatible SSE-Draht
  (chat.completions/Responses) drueckt Replace nicht aus: Revisionen laufen als
  `TextReplace` in den Session-Stream, am Draht bleibt Stille und `last_sent`
  wird auf den revidierten Stand gesetzt (naechster Zuwachs misst relativ).
- **Swarm-Kontext sichtbar pro Brain.** Gruppen-Sessions erhalten einen
  `[SWARM-KONTEXT]`-Block (Gruppe, Run-ID, Leader, Mitstreiter, Runde); der
  CLI-Swarm (`cmd_swarm`) ergaenzt Ziel, Repo, Commit, Branch. „Fehlend bleibt
  fehlend“: nicht verfuegbare Werte werden weggelassen, nie erfunden.

## Abnahme

| Abnahmepunkt (Plan Z.31) | Beleg |
|---|---|
| Erstes Delta erscheint VOR Ende | `web_ui_api::tests::chat_erstes_delta_erscheint_vor_abschluss` (Seq-Delta < Seq-TextComplete) |
| Keine Duplikate | `StreamJournal`-Dedupe (identischer Snapshot -> `None`) in `contract::tests` + `stream_ingest_drops_identical_consecutive_and_status` |
| Unicode bleibt heil | `contract::tests::unicode_suffix_grenze_bleibt_zeichensicher`; Testtext „früh\nmittig\nEnde“ |
| Revision ohne Textverlust | `web_ui_api::tests::chat_revision_emits_text_replace_ohne_textverlust` (Replace mit vollem Text, nie „einsAntwort“); `revision_wirkt_nicht_als_doppeltes_delta` |
| Leere Antwort | Relay-Retry + Oberflaechen-Gate (`Transient`), Web-UI 502 statt `TextComplete` |
| Stop | `chat_stop_reconnect_mid_turn` |
| Reconnect | `chat_stop_reconnect_mid_turn` (zweiter Turn nach Stop funktioniert) |
| Diagnose nie als Antwort/Repair | `web_ui_api::tests::chat_diagnose_wand_ist_keine_fertige_antwort` (502, Error, kein TextComplete); Relay-Gate; `contract::tests::{diag…}` (UiDiagnosis bleibt Rohbeleg) |
| Vollstaendige Endantwort | FakeBrain liefert kumulative Snapshots + `final_text`; Replace-Test endet mit vollem Text |
| Gepuffert != Streaming | `chat_erstes_delta_erscheint_vor_abschluss`; Controller/API konsumieren `wait_response_streaming` |
| Swarm-Kontext pro Brain sichtbar | `group_run::tests::swarm_kontext_sichtbar_pro_brain_und_synthese`; `commands::ops::tests::swarm_kontext_fehlend_bleibt_fehlend` |

## Lieferumfang

- **`src/contract.rs`**: `StreamEdit`, `classify_edit` (byte-sicher,
  `starts_with`-Praefix), `StreamJournal` (Snapshot, `appends`/`replaces`,
  roher Beweis-Cap 192 Snapshots), `SurfaceKind::UiDiagnosis`-Vertrag.
- **`src/session/events.rs`**: `SessionEvent::TextReplace { text }` + Serde-Roundtrip.
- **`src/web_ui_api.rs`**: `drive_chat_turn` emittiert `TextDelta`/`TextReplace`
  aus `StreamJournal`; Oberflaechen-Gate vor `TextComplete` (502 bei Diagnose).
- **`src/api_bridge.rs`**: beide SSE-Handler (chat.completions + Responses) auf
  `classify_edit`; Replace -> `TextReplace` (responses) bzw. Stille + `last_sent`
  (chat) — additive Draht-Semantik dokumentiert.
- **`src/controller.rs`**: `run_once` konsumiert `wait_response_streaming`;
  Transkript-Evidenz `brain_stream_snapshot` mit rohen Snapshots + Zaehlern
  (Test `stream_rohbeweis_transkript_behaelt_snapshots`).
- **`src/fakebrain.rs`**: kumulative Snapshots in `Text`/`Stream`-Szenarien.
- **`src/relay.rs`**: Oberflaechen-Gate nach jeder Antwort (nie Diagnose als Antwort).
- **`src/group_run.rs`**: `[SWARM-KONTEXT]`-Block in Turn- und Synthese-Prompts.
- **`src/commands/ops.rs`** (Bin): `cmd_swarm` sendet `[SWARM-KONTEXT]` (Ziel/
  Repo/Commit/Branch) an jedes Brain und die Synthese.

## Pflichtgates (2026-09-10)

- `cargo test` (lib + bins): **1386 + 8 passed, 0 failed, 1 ignored**.
- `cargo test --features tui` (lib + bins): **1419 + 8 passed, 0 failed, 1 ignored**.
- `cargo check --features tui`: **gruen** (2 vorbestehende Warnings in `controller.rs`).
- `cargo check --no-default-features`: **gruen**.
- Gegenprobe gegen reale Logs/Live-Matrix: gehoert zur Live-Matrix **T-807**
  (auto-RED-Entscheidung bleibt offen; zai GLM-5.3 Live-Flap ungeloest).

Commit: siehe `git log` auf `feature/T-803-answer-stream` (ft. T-806).