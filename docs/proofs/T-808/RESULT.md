# T-808 — Dauerhaftes Run-Ledger, Prozesssicherheit und Crash-Recovery

**Archiv:** Nachweis für Scheibe 8 der BRAIN-UNIFICATION-ROADMAP (Plan-Z.36). Das Ticket ist mit diesem Dokument **abgeschlossen**.

**Commit:** `b5088c7` (feat(T-808)) auf `feature/T-808-run-ledger`, Basis `feature/T-805-probe-proof` (gepusht nach https://github.com/st0rax/webagent-rs).

## Soll (Plan-Z.36) → Ist

| Abnahmepunkt | Zustand |
|---|---|
| Run-Meta, Eventkette und Transcript mit eindeutiger Run-/Attempt-/Lease-Bindung versöhnen | events.jsonl trägt bereits run_id + seq + prev_hash-basierte SHA-256-Kette; Ketten-Verifikation jetzt zentral in `run_ledger::verify_event_chain`, append läuft unter demselben Vertrag. Attempt-/Lease-Bindung bleibt Seed (`contract.rs::Operation` ist persistierbar) — Scheibe 9/10. |
| Prozessübergreifend sequenzieren und sperren | `LedgerLock` (atomares `create_dir` + Owner-Datei `{pid, started_at}`); stale = PID tot oder Alter > 120 s; frisches Lock ohne Owner wird NICHT gestohlen (Race-fest: erster Schreiber ist geschützt). 4-Thread-Test schreibt 100 Events lückenlos. |
| Keine gemeinsam verwendeten Tempnamen | `atomic_write` erzeugt `meta.json.<pid>.<nanos>.tmp` via `create_new`; Legacy-Reste (`meta.json.tmp`) werden beim Speichern bereinigt. |
| Windows-sicher atomar ersetzen und flushen | Temp-Datei wird mit `sync_all` geflusht, dann `rename`; Windows-Fallback schreibt direkt aufs Ziel mit `sync_all` des Ziels und entfernt Temp best effort. Kein halbes `meta.json` mehr möglich. |
| Torn Tails erkennen, unverändert quarantänieren, Recovery-Receipt statt stiller Reparatur | `verify_event_chain` unterscheidet abgerissenen Tail (letzte Zeile ohne Terminal-Newline) von mittiger Korruption (fail-closed, nie repariert). Torn-Bytes werden byteidentisch nach `quarantine/events.jsonl.<stamp>.torn` gelegt; `recovery.json`-Receipt (run_id, SHA-256, Torn-Länge, gültige seq) wird geschrieben. |
| Resume behandelt vor Crash möglicherweise gesendete Aktionen als mehrdeutig und beobachtet zuerst | Bei vorhandenem `recovery.json` wird im Resume-Pfad (`controller.rs`) die `pending_response` NICHT recycelt; `resume_initial_turn` (beobachten/restore) startet zuerst. |
| Zwei Prozesse schreiben eine lückenlose lineare Kette | `zwei_threads_schreiben_lueckenlose_kette`: 4 parallele Writer mit Lock schreiben 100 Events, `valid_count == 100`, `next_seq == 101`. |
| Kaputter Tail lässt gültigen Präfix lesbar | Nach Quarantäne: `verify_event_chain` bestätigt `valid_count == 3`, Präfix-Bytes bleiben unverändert (Neuschreibung mit fsync). |
| Fehler bei Rename/Write erzeugen nie halbes Meta | `atomic_write` schreibt erst vollständig in eindeutige Temp-Datei, fsync, dann ersetzt; Windows-Fallback mit Ziel-fsync. |
| Jede Persistenzunterbrechung ergibt fail-closed `interrupted` oder `recovery_required`, nie unbelegtes `done` | Neuer Status `recovery_required`: `save()` verweigert terminale Status bei offenem Receipt (vor Validierung), Reconcile setzt `recovery_required` bei Receipt statt blinder Reparatur; `activate_continuation` schließt die Recovery explizit ab (Receipt-Entfernung). Ende-zu-Ende-Test `torn_tail_verhindert_done_und_setzt_recovery_required`. |

## Grenzen (bewusst)

- **Mittige Ketten-Korruption** ist `ChainError::Corrupt` (fail-closed) — wird nie repariert; nur der abgerissene Tail ist per Quarantäne erholbar. Das entspricht dem Plan (Torn-Tails), nicht einer Voll-Reparatur beliebiger Schäden.
- **Transcript-Bindung** von `run_id`/seq wird hier nicht neu beschrieben; geerbt bleibt die bestehende `meta_saved`/`status_changed`-/`created`-Event-Kette.
- **Attempt/Lease-Bindung** des `Operation`-Vertrags (Scheibe 9) ist nicht Teil dieses Tickets.

## Gates (Z.40)

| Gate | Ergebnis |
|---|---|
| `cargo check --features tui` | grün |
| `cargo check --no-default-features` | grün |
| `cargo test --lib` | **1405 passed**, 0 failed, 1 ignored |
| `cargo test --features tui --lib` | **1438 passed**, 0 failed, 1 ignored |

## Neue Tests (7)

- `run_ledger::leeres_journal_liefert_genesis`
- `run_ledger::intakte_kette_wird_verifiziert`
- `run_ledger::abgerissener_tail_ist_torn`
- `run_ledger::torn_tail_quarantaene_erhaelt_praefix_und_receipt`
- `run_ledger::korrupt_mitten_in_der_kette_ist_nie_torn`
- `run_ledger::zwei_threads_schreiben_lueckenlose_kette` (100 Events, 4 Writer)
- `run_store::torn_tail_verhindert_done_und_setzt_recovery_required` (Ende-zu-Ende: Crash → done blockiert → recovery_required → Fortsetzung schließt Recovery ab)

## Dateien

- **neu:** `src/run_ledger.rs` (Kette, Lock, Quarantäne, Receipt, atomic_write)
- **geändert:** `src/run_store.rs` (save/append/reconcile/activate), `src/controller.rs` (Resume-Zweig), `src/lib.rs` (Modul-Deklaration)