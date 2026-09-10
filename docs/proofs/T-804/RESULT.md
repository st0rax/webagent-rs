> **Archiv** — T-804 (Scheibe 4) als Code umgesetzt und belegt (2026-09-10).
> Messprotokoll bleibt fuer Gegenprobe und Regression; lebender Stand:
> `docs/CURRENT_WORK.md`, `docs/TASKBOARD.md`.

# T-804 — Gemeinsame Profil-Lease und Blocker

Messung: 2026-09-10 · Owner: local/opencode · Branch: `feature/T-804-profile-lease`
Bezug: `docs/BRAIN_UNIFICATION_PLAN.md` Scheibe 4 (T-804), `docs/TASKBOARD.json`.

## Kernentscheidungen

- **Lease v2 mit Prozess- und Lebenszyklus-Metadaten.** `SwarmProfileOwner`
  traegt jetzt `pid`, `process_started_at`, `generation` und einen erneuerbaren
  `heartbeat` (`HEARTBEAT_FRESH_SECS = 120s`). Die Reservation bleibt weiterhin
  atomar (`create_dir` + Owner-Datei via pending+rename) und verbindlich bis
  nach dem Browser-Abbau — `release` raeumt erst nach der Freigabe.
- **Busy ist kein Providerlimit.** `circuit_breaker::record_failure` prueft vor
  jedem Zaehler-/Breaker-Schritt, ob das Profil gerade geleast ist
  (`is_profile_leased`); in diesem Fall wird der Fehlschlag protokolliert, aber
  der Breaker bleibt unberuehrt. Ein gelegentlicher Parallel-Lauf kann ein
  gesundes Brain also nie fuer Minuten ausser Kraft setzen.
- **Keine Probes waehrend belegter Sperre.** `welcome::probe_with_shot` und
  `commands::ops::cmd_measure_limits` ueberspringen ein Brain, dessen Profil
  frisch geleast ist — statt in die Sperre zu laufen.
- **Kontrolliert warten ODER konsistent vorbereitete Isolation.**
  `wait_for_profile_free_in` realisiert das „kontrolliert warten“; die atomare
  `create_dir`-Reservation liefert die konsistente Isolation. Neu dazu:
  `reclaim_swarm_profile_in` erlaubt die **Wiederaufnahme nach Prozessabsturz**,
  aber nur wenn (1) der Heartbeat abgekaltet ist und (2) der alte Owner-Marker
  lesbar, format-konform und exakt zum Scope passend ist. Ein frischer Lease
  wird nie wiedergeklaut, ein fremdes/unlesbares Profil nie blind recycelt.
- **Resetzeit mit Herkunft.** `BrainState` traegt `reset_at` + `reset_origin`
  (message/login/manual/probe/unknown); `record_reset`/`reset_status` pflegen
  sie. „Unbekannter Reset bleibt unbekannt“: Ohne beobachtete Zeit wird nichts
  geraten (`reset_status` = `None`); ein explizit als `unknown` markierter
  Reset bleibt unterscheidbar von „kein Reset“.
- **Kein neuer Store.** Die Resetzeit mit Herkunft lebt im bestehenden
  Circuit-Breaker-Zustand (`state.json`); Eingabelaengen bleiben in
  `brain_limits.json`.

## Abnahme

| Abnahmepunkt (Plan Z.32) | Beleg |
|---|---|
| Zwei Prozesse pruefen Profilbelegung VOR Kopieren/Start | `config::profiles::lease_tests::scope_konkurrenz_ist_fail_closed` (2 Threads, atomare Reservation, 1 Gewinner + 1 `AlreadyExists`) |
| os error 32 (Sharing-Violation) | `os_error_32_ist_profilkonkurrenz_nicht_providerlimit` (CreateFileW `share=0` wie WebView2-SingletonLock -> Second Open = `ERROR_SHARING_VIOLATION` 32); `circuit_breaker::tests::belegtes_profil_ist_kein_providerlimit` (selbe Fehlerklasse oeffnet den Breaker nie) |
| Prozessabsturz + Wiederaufnahme | `stale_heartbeat_ermöglicht_wiederaufnahme` (frischer Heartbeat -> reclaim scheitert fail-closed; abgekalteter Heartbeat -> Reclaim ersetzt altes Verzeichnis) |
| Alter Worker darf neuen Lease nicht loeschen | `release_is_idempotent_and_refuses_foreign_owner` + `release()` verweigert fremden/ueberschriebenen Owner |
| Bekannter/unbekannter Reset | `reset_mit_herkunft_wird_gespeichert`, `unbekannter_reset_bleibt_unbekannt`, `reset_mit_explizit_unbekannter_herkunft_bleibt_unterscheidbar` |
| Navigationstimeout | `navigationstimeout_laesst_lease_intakt` (Heartbeat ueberlebt, Lease bleibt, Verlierer wartet kontrolliert) |
| Keine Probes waehrend belegter Sperre | Gates in `welcome::probe_with_shot` + `commands::ops::cmd_measure_limits`; verifiziert ueber `is_profile_leased` |
| Busy ist kein Providerlimit | `circuit_breaker::tests::belegtes_profil_ist_kein_providerlimit` (Zaehler unveraendert, `check_at` = geschlossen; Kontrolle oeffnet normal) |

## Lieferumfang

- **`src/config/profiles.rs`**: `SWARM_OWNER_VERSION=2`, `SwarmProfileOwner` um
  pid/process_started_at/generation/heartbeat erweitert, `SwarmProfileLease`:
  `heartbeat_now`, `generation`, `pid`; `release` verweigert fremde Owner
  (liest erst, loescht nie blind); `is_profile_leased(_in)`,
  `wait_for_profile_free(_in)`, `reclaim_swarm_profile_in`, `now_secs`.
- **`src/config/mod.rs`**: Re-Exporte `is_profile_leased`, `wait_for_profile_free`,
  `reclaim_swarm_profile_in`.
- **`src/circuit_breaker.rs`**: `reset_at`/`reset_origin` in `BrainState`,
  `record_reset(_at)`/`reset_status(_at)`, Busy-Guard in `record_failure_at`
  (testbar ueber `record_failure_at_with`).
- **`src/welcome.rs`** / **`src/commands/ops.rs`** (Bin): Lease-Gates fuer Probes.

## Pflichtgates (2026-09-10)

- `cargo test` (lib + bins): **1394 + 8 passed, 0 failed, 1 ignored**.
- `cargo test --features tui` (lib + bins): **1427 + 8 passed, 0 failed, 1 ignored**.
- `cargo check --features tui`: **gruen**.
- `cargo check --no-default-features`: **gruen**.
- Aktive Profil-Freigabe im Live-Betrieb und Rezertifizierung der Live-Matrix:
  gehoert zur Endabnahme **T-807** (auto-RED-Entscheidung bleibt offen;
  zai GLM-5.3 Live-Flap ungeloest).

Commit: `9c31c64` (feat(T-804)) auf `feature/T-804-profile-lease`, Basis
`feature/T-803-answer-stream`.