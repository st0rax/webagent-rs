> **Archiv** — T-805 (Scheibe 5) als Code umgesetzt und belegt (2026-09-10).
> Messprotokoll bleibt fuer Gegenprobe und Regression; lebender Stand:
> `docs/CURRENT_WORK.md`, `docs/TASKBOARD.md`.

# T-805 — Generische Probe mit bestehendem Capability-Proof-Gate

Messung: 2026-09-10 · Owner: local/opencode · Branch: `feature/T-805-probe-proof`
Bezug: `docs/BRAIN_UNIFICATION_PLAN.md` Scheibe 5 (T-805), `docs/TASKBOARD.json`.

## Kernentscheidungen

- **Gemeinsamer Snapshot->Edit-Vertrag fuer den Generation-Probe.**
  `generation_sequence` (`browser/verify.rs`) konsumiert dichten XMLHttpRequest-
  Text via dasselbe `StreamJournal` wie der Produktivpfad (`controller.rs`).
  Praefix-Wachstum wird als `Append` klassifiziert, Revisionen als `Replace`;
  der Appendix-Zaehler (`streaming: N appends, M replaces`) erscheint als Note
  im `chat`-Beleg — die Gegeprobe zur Produktivmessung, die denselben Vertrag
  ueber `wait_response_streaming` konsumiert.

- **`BrainVerification` als reiner Ausweis.** `capability_proof.rs` fuehrt
  `BrainVerification::Verified`/`Unverified` ein. Die Funktion
  `brain_verification_at` prueft alle Kernfaehigkeiten (`driveable + attainable`)
  auf `proof_state_at == Proven`; mindestens ein frischer, hash-konformer Beleg
  genuegt fuer `Verified`, alles andere bleibt `Unverified`. Die Funktion ist
  komplett pur: kein Dateizugriff, nur der uebergebene Store-Pfad — Tests
  schreiben in `unique_path()`-Wegwerf-Dateien. Eine neue Brain-URL aus der
  generischen Discovery (`parse_custom_brains`) startet ohne Belege und wird
  damit ehrlich als `Unverified` ausgewiesen, statt still als „geprobt" zu
  gelten.

- **Sichtbare Ausweisung in `brains_health`.** Die Befehlsausgabe fuegt
  `verification=Verified|Unverified` pro Brain ein, berechnet aus den
  aktuellen Selektoren und dem `chat`-Hash. Das schliesst die Luecke, dass
  neue Entdeckungen unmarkiert bleiben.

- **Toggle-Pfade sind vereinheitlicht.** Die Gap-Analyse widerlegt die
  fruehere Vermutung zweier getrennter Pfade: `operations::verify_surface`
  (operations.rs:603) und `brain_probe::verify` (Z.1000-1001) teilen dieselben
  JS-Bausteine (`toggle_state_expr_for`, `click_toggle_expr_for`).

## Abnahme

| Abnahmepunkt (Plan Z.33) | Beleg |
|---|---|
| Brain nutzt denselben Send-/Observe-Vertrag | `generation_sequence` (verify.rs) initialisiert `StreamJournal`, pusht `probe_generation`-Text, prueft appends/replaces; Test `streaming_wird_ueber_denselben_journalvertrag_belegt` (5 Polls, 3 Appends, Note geprueft) |
| Kandidat -> Live-Zustandswechsel -> Measurement -> capability_proof | bestehende Kette (`verify_records` -> `record_measurement` -> `proof_state_at`) bleibt unveraendert; `generation_sequence` fuegt T-805-Note hinzu |
| Composer/Submit/Output/Streaming/Stop separat | bestehende Moeglichkeit (verify_capabilities Z.180ff) unveraendert; T-805 erweitert nur den chat-Beleg um Streaming-Note |
| Cache an Selektor-/Adapterversion + TTL binden | bereits implementiert (`ttl_days`, `selector_hash_for`, `proof_state_at` in capability_proof.rs:275-297, Beleg T-803/T-804) |
| Neue Brain-URL ohne feste Liste | `parse_custom_brains`/`available_brain_ids()` (config/brains.rs) — generische Discovery vorhanden |
| Unbekannte Mechanik als unverified ausweisen | `BrainVerification`-Enum + Tests (`brain_ohne_beleg_ist_unverified`, `frischer_hashkonformer_beleg_macht_brain_verified`, `entwerteter_beleg_bleibt_unverified`); Anzeige in `brains_health` |

## Lieferumfang

- **`src/browser/verify.rs`**: `StreamJournal` in `generation_sequence` (vor
  Poll-Schleife initialisiert, `push` nach jedem `probe_generation`, Note in
  chat-Measurement mit Appends/Replaces).
- **`src/capability_proof.rs`**: `BrainVerification` (enum), `brain_verification_at`
  (pur, testbar ueber Wegwerf-Pfad), `brain_verification` (Wrapper fuer
  Echtzeit-Store), drei Tests.
- **`src/brains_health.rs`**: `verification=Verified|Unverified` pro Brain-
  Zeile, berechnet via `brain_verification` + aktueller `chat`-Hash.

## Pflichtgates (2026-09-10)

- `cargo test --lib`: **1398 passed, 0 failed, 1 ignored**.
- `cargo test --features tui --lib`: **1431 passed, 0 failed, 1 ignored**.
- `cargo check --features tui`: **gruen**.
- `cargo check --no-default-features`: **gruen**.
- Live-Matrix/Rezertifizierung gehoert zur Endabnahme **T-807**.

Commit: `951572d` (chore(T-805)) auf `feature/T-805-probe-proof`, Basis
`feature/T-804-profile-lease`.
