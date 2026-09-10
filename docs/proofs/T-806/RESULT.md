# T-806 — Taskabschluss mit verifizierten Run-Belegen und Crashschutz

**Archiv:** Nachweis für Scheibe 6 der BRAIN-UNIFICATION-ROADMAP (Plan-Z.34). Das Ticket ist mit diesem Dokument **abgeschlossen**.

**Commit:** `d30b55c` (feat(T-806)) auf `feature/T-806-run-proof`, Basis `feature/T-808-run-ledger` (gepusht nach https://github.com/st0rax/webagent-rs).

## Soll (Plan-Z.34) → Ist

| Abnahmepunkt | Zustand |
|---|---|
| Schwachen Datei-Existenzabschluss ersetzen | `complete_claim` (prüfte nur `proof_path.is_file()`) ersetzt durch `complete_claim_verified`: Beleg muss Datei sein, nicht leer, im Scope `docs/proofs/<task_id>/` (Geschwisterordner des Boards) liegen und frisch sein (mtime >= `claimed_at`). SHA-256 wird controllerseitig über den unangetasteten Inhalt gebildet. |
| Runstart mit expliziter Task-/Owner-Bindung, keine fest codierte chatgpt-codex-Identität | Owner kommt aus CLI-Flag `--owner`, Fallback Env `WEBAGENT_AGENT_ID`, letzter Fallback `local/opencode` (`DEFAULT_OWNER`); der hart codierte `"chatgpt-codex"` in `ops.rs` ist entfernt. |
| Abnahmeanforderungen vor Runende einfrieren | `acquire_claim` friert `requirements_snapshot` (verification/dod/depends_on) ins Board; beim Abschluss wird eingefroren == aktuell verglichen — eine nachträgliche Verschärfung/Aufweichung wird als Manipulation verweigert. |
| Controllerseitig erfasste Prüfungen gegen Run-ID, Commit, Artefakthashes und Geltungsbereich | Abschluss bindet `proof_sha256`, `proof_commit` (git HEAD, `git rev-parse HEAD`), `run_id` und den Belegpfad in den Board-Task; alle Prüfungen laufen im Controller (`taskboard.rs`), nicht im Brain. Brain-Manifest ist Antrag, kein eigener Beweis. |
| Board unter Prozesslock erneut lesen, Claim/Dependencies vergleichen | Der Abschluss re-liest das Board unter `LedgerLock` (30 s, atomare `create_dir`-Reservation + Owner) und prüft owner/branch-Match sowie, dass alle eingefrorenen `depends_on` `done` sind; eine parallele Boardänderung (gestohlener Claim) blockiert den Abschluss. |
| Eindeutige Tempdatei + flush/sync + Windows-sicher ersetzen | Wiederverwendet `run_ledger::atomic_write` (T-808): Tempnamen mit PID+Nanos via `create_new`, fsync vor Rename, Windows-Fallback mit Ziel-fsync. Kein geteilter Tempname. |
| Crash-Recovery und idempotenter Replay getestet | Replay mit identischem Beleg+Hash wird als `replayed: true` erkannt und schreibt nichts erneut; manipulierter Beleg nach `done` blockiert. Eventlog bleibt unter der Ledger-Sperre (Claim-Ablehnungen neuerdings als `claim_rejected` persistiert). |
| Ablehnung im Run-Eventlog persistieren | `RunStore::append_claim_rejected(meta, task_id, reason)` schreibt `claim_rejected` mit task_id + Grund ins gesperrte `events.jsonl` — nie still geschluckt (ops.rs-Rückgabepfad exit 1). |
| Alte/leere/fremde/manipulierte Belege und parallele Boardänderungen dürfen nie `done` ergeben | Tests: leere/externe Verzeichnis-Belege, alter Beleg (claimed_at in der Zukunft), fremder Owner/Branch, geänderte Anforderungen, offene Dependency, parallele Claim-Entführung — alle bleiben `claimed`. |

## Grenzen (bewusst)

- **Live-Abnahme der Capability-Proofs** (T-807) ersetzt diese Scheibe nicht: kontrollerseitige Prüfungen sind hier die Run-/Commit-/Beleg-Bindung aus `taskboard.rs`; die Live-Matrix mit echten Providern bleibt offen.
- **Attempt/Lease-Bindung** der `Operation`-Ebene (Scheibe 9) ist nicht Teil dieses Tickets.
- **Antikollision auf Board-Ebene** ist fail-closed: Bei parallelem Entführen des Claims wird der Abschluss verweigert; eine automatische Wiederauflösung/Konflikt-Logik ist nicht enthalten.

## Gates (Z.40)

| Gate | Ergebnis |
|---|---|
| `cargo check --features tui` | grün |
| `cargo check --no-default-features` | grün |
| `cargo test --lib` | **1411 passed**, 0 failed, 1 ignored |
| `cargo test --features tui --lib` | **1444 passed**, 0 failed, 1 ignored |

## Neue Tests (13)

- `taskboard::acquire_friert_abnahmeanforderungen_ein`
- `taskboard::abschluss_erfordert_verifizierten_scope_und_hash`
- `taskboard::leerer_oder_externer_beleg_ergibt_nie_done`
- `taskboard::offene_dependency_verhindert_done`
- `taskboard::geaenderte_anforderungen_sind_manipulation`
- `taskboard::fremder_owner_und_branch_werden_abgelehnt`
- `taskboard::alter_beleg_vor_claim_ergibt_nie_done`
- `taskboard::replay_ist_idempotent_und_fremder_hash_nach_done_blockiert`
- `taskboard::parallele_boardaenderung_nach_claim_aendert_owner_und_blockiert`
- `taskboard::acquire_bleibt_fail_closed`
- `taskboard::sha256_hex_ist_stabil`
- `taskboard::task_status_liest_unter_lock`
- `run_store::claim_ablehnung_landet_im_run_eventlog`

## Dateien

- **geändert:** `src/taskboard.rs` (Kern: Claim-Freeze + verifizierter Abschluss), `src/commands/ops.rs` (Owner-Auflösung, Ablehnung → Eventlog), `src/cli.rs` (`--owner`-Flag), `src/main.rs` (Durchreichen), `src/run_store.rs` (`append_claim_rejected`)