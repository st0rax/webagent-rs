# **Referenz.** Play-Mode v0.3 — Recon des echten Repo-Stands

Stand: 2026-11-26, Branch `fix/T-976-api-turn-reveal`, Basis `66b9045`.
Methode: `rg`/`read` im Workspace plus 14 reale `cargo test --locked --lib
<filter>`-Läufe. Dies ist Bestandsaufnahme, **keine** Umsetzung und kein
Beleg für die PLAY-Tasks. Das Ziel ist `docs/PLAY_MODE_SPEC.md`; die
maschinenlesbare Tafel ist `docs/TASKBOARD.json` (Schema v2, T-978..T-996).

## 1. Fundorte (alle Typen sind `privat` im selben Modul)

| Typ / Symbol | Fundort | Aktuelle Definition (kurz) | Konsequenz für Tasks |
|---|---|---|---|
| `Room` | `src/repl/play.rs:21` | `enum { Prompt, Bridge, Breaker, Werkbank, Arena }`, `#[default] = Prompt`; `title()` :31, `scene()` :41, `exits()` :61 | Ticket-, Szenen- und Ausgangslogik liegt in `src/repl/play.rs`, nicht in `src/play/`. |
| `BrainState` | `src/repl/play.rs:73` | `struct { xp: u32, level: u32, correct: u32, wrong: u32 }`, `Default` | Wird von `Save.brains: BTreeMap<String, BrainState>` gehalten. |
| `Save` | `src/repl/play.rs:81` | `struct { version: 1, room, core, awake, ghosts: u32, brains, inventory, badges, heal_pending, cable_pending, finished }`; `Default` :97; `path()` :134 | Pfad `data_dir()/play/save.json`; Version ist **1**. `diary`/`ghost_log` fehlen. |
| `Save::write` | `src/repl/play.rs:145` | `create_dir_all` + `serde_json::to_vec_pretty` + **`std::fs::write` direkt** | **Nicht atomar.** T-983 muss tmp-Datei + `sync_all` + `rename` nachziehen; es gibt gerade keinen Schutz gegen halb geschriebene Dateien. |
| `Mission` | `src/repl/play.rs:222` | `enum { Tutorial, Scan, Repair, Work, Champion }`; `needs_marker()` :230, `badge()` :235, `task()` :245 | `Scan`/`Repair`-Kriterien und Badges werden in T-992 geändert; `Work`/`Champion` bleiben vorerst unberührt. |
| `Ledger` | `src/repl/play.rs:186` | `struct { proofs: u32, final_text: String }`; `observe()` :191 zählt nur erfolgreiche `Shell`/`File`, `Message` Setzt `final_text`; `install()` :203 | Kein `Vec<Proof>`, keine `ProofId`. T-981 muss `proofs_count()` als Kompatibilitäts-API ergänzen. |
| `ObservationHook`-Nutzung | Typ `src/controller/types.rs:31`; Feld `RunOptions.observation_hook` :42; `notify_observation()` `src/controller.rs:235`; Aufrufer `Message` :914, `Shell` :967, `File` :1058; Play-Install `src/repl/play.rs:203..218` | Hook ist ein `Arc<dyn Fn(ObservationEvent) + Send + Sync>`; `RunOptions` trägt ihn optional; Controller meldet echte Aktionen | Genau **ein** Beleg-Gate: kein zweiter Proof-Store, kein Eingriff in `capability_proof`. |
| `Ghost`-Zustand | `src/repl/play.rs:88`, `ghost_step()` :281, `print_map()` :486 | `ghosts: u32`, `saturating_sub(1)` bei Erfolg, `min(3)` bei Fehler, Anzeige als `●`-Kette | **`ghosts` ist ein Zähler, keine Liste.** T-984/T-989/T-011 müssen `ghost_log` einführen und die Zahl daraus ableiten. |
| Brain-Wechsel | `src/repl/play.rs:658..697` (`"load" | "wake"`) | Aktiver Kern → `IDENTITÄTSKONFLIKT`, `ghost_step(save,false)`, `wrong += 1`; sonst `ReplSession::new`, `eject` :646 | T-989 erzeugt hier einen `BrainDrift`-Geist mit Re-Verify-Bindung. |
| XP-Regel | `award()` `src/repl/play.rs:269`, `run_mission()` :340 | `10 * proofs`, `+20` nur bei `status=done` **und** Marker bei markerpflichtigen Missionen; Items :376..385 | XP bleibt an echte Observations gebunden; `sonderbonbon` gibt nur Level. |

Sichtbarkeit: alle genannten Typen sind ohne `pub`. `src/repl/mod.rs:7`
deklariert `mod play;`, `:13` exportiert nur `run_play`. `src/lib.rs` hat
**kein** `pub mod play;`. Ein neues `src/play/`-Modul muss daher in T-979
einmal in `src/lib.rs` verdrahtet werden; kein anderer PLAY-Task berührt
diese Datei.

## 2. Verzeichnisse und Abhängigkeiten (real geprüft)

- `src/play/` existiert **nicht**; die komplette Spiel-Logik liegt in der
  einzelnen Datei `src/repl/play.rs` (32 KiB).
- `docs/PLAY_MODE_SPEC.md`, `docs/PLAY_MODE.md`, `play/fixtures/`, `tests/`
  existierten vor diesem Update nicht. `docs/PLAY_MODE_SPEC.md` und
  `docs/PLAY_MODE.md` sind jetzt als Referenz/Soll-Zustand angelegt; die
  Fixtures und Integrationstests bleiben Aufgaben (T-986, T-993).
- `Cargo.toml:3` nennt `version = "0.11.7"`. Es gibt kein `reqwest`;
  `tokio-rustls` ist vorhanden. Ein `play-live`-Feature ist noch nicht
  definiert (T-994).
- `docs/TASKBOARD.json` hatte vor dem Update 102 Tasks, `schema_version=1`,
  maximale ID `T-977`. Die 19 PLAY-Tasks sind jetzt `T-978..T-996` bei
  `schema_version=2`; `docs/TASKBOARD.md` ist der geprüfte Spiegel.

## 3. Testfilter — gemessen, nicht angenommen

Alle Filter wurden mit `cargo test --locked --lib <filter>` ausgeführt:

| Filter | `running` | Ergebnis |
|---|---:|---|
| `play` | 9 | 9 passed, 0 failed, 1 ignored (Gesamtlauf) |
| `play::proof` | 0 | 0 passed, 1536 filtered out |
| `play::claim` | 0 | 0 passed, 1536 filtered out |
| `play::save` | 0 | 0 passed, 1536 filtered out |
| `play::diary` | 0 | 0 passed, 1536 filtered out |
| `play::ghost` | 0 | 0 passed, 1536 filtered out |
| `play::source` | 0 | 0 passed, 1536 filtered out |
| `play::jetstream` | 0 | 0 passed, 1536 filtered out |
| `play::tick` | 0 | 0 passed, 1536 filtered out |
| `play::brain_switch` | 0 | 0 passed, 1536 filtered out |
| `play::failsafe` | 0 | 0 passed, 1536 filtered out |
| `play::noise` | 0 | 0 passed, 1536 filtered out |
| `play::mission` | 0 | 0 passed, 1536 filtered out |
| `play::live` | 0 | 0 passed, 1536 filtered out |

**Folge:** Die im Fragment vorgeschlagenen engen Filter sind Ziel-Gates,
keine aktuellen Belege. Ein Task gilt erst als prüfbar, wenn sein Filter
mindestens einen echten Test findet. Bis dahin ist `cargo test --lib play`
der gemeinsame, reale Baseline-Gate.

## 4. Korrekturen, die in die Tafel eingeflossen sind

- `PLAY-000`/T-978: `docs/TASKBOARD.md` in den Scope aufgenommen, weil der
  Sync-Test `taskboard::tests::taskboard_invarianten_halten` sonst rot wird.
- `PLAY-001`/T-979: `src/lib.rs` in den Scope aufgenommen (`pub mod play;`).
- `PLAY-003`, `PLAY-005`, `PLAY-010`, `PLAY-011`, `PLAY-014`: Platzhalter
  `<… laut RECON>` durch die echte Datei `src/repl/play.rs` ersetzt und als
  ausdrückliche Ausnahme von der `src/play/`-Regel markiert.
- `PLAY-005`/T-983: Die Annahme „Save::write bleibt atomar" war falsch;
  tmp+rename ist jetzt explizite Abnahme.
- `PLAY-005`: `play/fixtures/v1_save.json` für die Migrations-Fixture und
  `PLAY-007`: `play/fixtures/source_mini/describe_repo.json` für die
  Mini-Fixture in den Scope aufgenommen, damit die Abnahme nicht auf
  Dateien außerhalb des Scopes angewiesen ist.
- `PLAY-011`/T-989: Kante zu T-988 ergänzt (beide schreiben
  `src/repl/play.rs`).
- `PLAY-014`/T-992: Kante zu T-989 ergänzt (Mission nach Brain-Wechsel).
- `PLAY-015`/T-993: Kante zu T-987/PLAY-009 (Jetstream-Parser) ergänzt;
  außerdem fehlte der Golden-File-Pfad im Scope.
- `PLAY-017`/T-995 und `PLAY-018`/T-996: `test -f` ist unter Windows
  PowerShell kein verlässlicher Gate; ersetzt durch `Test-Path`.
- `acquire_claim` in `src/taskboard.rs` erzwingt jetzt `depends_on`: Ein
  Claim wird abgelehnt, solange ein Vorgänger nicht `done` ist.

## 5. Lokaler Spielstand (Beobachtung, kein Repo-Inhalt)

`%LOCALAPPDATA%\webagent\data\play\save.json` beim Stand 2026-11-26:
`version=1`, `room=Bridge`, `core=deepseek`, `ghosts=0`,
`chatgpt xp=0/correct=0/wrong=1`, `claude xp=0/correct=0/wrong=1`,
`deepseek xp=60/correct=2/wrong=0`, Abzeichen `startschein,brueckenstein`.
Kein Tokenwert in dieser Datei; `auth.json` blieb unberührt.
