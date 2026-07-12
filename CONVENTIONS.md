# WebAgent Rust-Port — Konventionen für Aider

Du portierst den bestehenden Python-Agenten (`../src/webagent/`) sauber und
plattformunabhängig nach Rust. Nicht 1:1 übersetzen, sondern idiomatisch.

## Ziel & Scope

- **Plattformen:** Windows, Linux, Android. Kein Code, der eine Plattform
  hart voraussetzt. Plattform-Spezifisches (PID-Liveness, Shell-Binary,
  Profilpfade) hinter `#[cfg(...)]` oder eine kleine Trait-Abstraktion legen.
- **Kern zuerst, Browser später:** Die reine Logik (Protokoll-Parser, Timeouts,
  Run-Store, Transcript, Loop-Guard, Observer-Textheuristik, Doctor, Prompts,
  Controller-Zustandsmaschine) ist plattformrein und wird zuerst portiert und
  getestet. Die Browser-Anbindung ist eine austauschbare Trait-Grenze
  (`brain::BrainBackend`), Default-Impl später via CDP/WebSocket.

## Architektur-Grenzen (Traits)

- `BrainBackend` — Web-Chat-Gehirn (start/stop/ensure_ready/send/wait_response/…).
  Genau ein Trait; konkrete Backends (CDP) sind separate Structs.
- `ShellExecutor` — lokale Shell. Windows → PowerShell, Unix → `sh`/`bash`.
  Auswahl über `#[cfg(...)]`, nicht über Laufzeit-Stringvergleich.
- Persistenz (`run_store`, `transcript`) arbeitet nur mit `std::fs` + `serde_json`,
  keine plattformspezifischen Pfadannahmen außer über `config`.

## Bereitgestellte Helfer (in `lib.rs`, NICHT neu bauen)

- `webagent::now_rfc3339()` → UTC-Zeitstempel exakt wie Pythons
  `datetime.now(timezone.utc).isoformat()` (`...T...+00:00`, mit Mikrosekunden).
- `webagent::now_run_stamp()` → `YYYYMMDD_HHMMSS` (UTC) für Run-IDs.
- `webagent::pid_alive(pid: i64) -> bool` → plattformübergreifende PID-Liveness.
- `webagent::char_prefix(s, n)` / `char_suffix(s, n)` → zeichen-sicheres Slicing
  wie Pythons `s[:n]` / `s[-n:]` (Transcript-Kürzung, Observation-Budget).

## Dependencies — bewusst minimal

Erlaubt sind nur rein-Rust-Crates ohne C-Toolchain-Bedarf: `serde`, `serde_json`,
`regex`, `clap` (bereits in `Cargo.toml`). **Nicht** hinzufügen: `chrono`,
`windows-sys`, `libc`, `time` mit lokaler Zeitzone — sie ziehen auf Windows-GNU
`dlltool`/MSVC nach und brechen den Build. Zeit/PID über die Helfer oben lösen.

## Rust-Stil

- Edition 2021. `#![forbid(unsafe_code)]` überall außer im PID-Liveness-Modul
  (Windows braucht `windows-sys`, Unix `libc`), dort eng gekapselt.
- Fehlerbehandlung: eigene `enum`-Fehler pro Modul mit `std::error::Error`,
  keine externe Fehler-Crate nötig. Öffentliche APIs geben `Result<_, E>` zurück.
- `serde` mit `#[derive(Serialize, Deserialize)]` für alle persistierten Structs
  (`RunMeta`, Transcript-Zeilen, Protokoll). Feldnamen exakt wie im Python-JSON,
  damit bestehende `data/`-Artefakte lesbar bleiben.
- Keine `unwrap()`/`expect()` in Bibliothekscode außer bei nachweislich
  unmöglichen Fällen (dann mit Kommentar). Tests dürfen `unwrap`.
- Öffentliche Items mit `///`-Doc auf Deutsch (wie das Original), knapp.

## Protokoll — exakt erhalten

Das `webagent/1`-Protokoll und das `WEBAGENT/1 SHELL`-Rohskript-Format müssen
**byte-genau** dieselbe Semantik haben wie `../src/webagent/protocol.py`:
- Regex-Verhalten für Thought-Process-/UI-Control-Prefixes.
- `timeout_seconds` endlich und `0 < t <= 3600`.
- `finish`/`message` müssen alleinige Action sein.
- `is_possibly_truncated`-Heuristik identisch.
Die vorhandenen Python-Tests (`../tests/test_protocol.py`) sind die Spezifikation.
Jeder dortige Testfall bekommt ein Rust-Äquivalent in `#[cfg(test)]`.

## Tests

- Jedes Modul hat `#[cfg(test)] mod tests`. Portiere die Python-Testfälle direkt.
- Kein echter Browser, kein Netz, keine echten Logins in Tests — wie im Original.
- `cargo test` muss grün sein, bevor ein Modul als fertig gilt.

## Was NICHT tun

- Keine `async`-Runtime einführen, solange der synchrone Kontrollfluss reicht
  (das Python-Original ist synchron/blockierend). Erst bei der CDP-Anbindung
  neu bewerten.
- Keine spekulativen Features. Nur portieren, was in `../src/webagent/` existiert.
- Profil-/Sicherheitslogik nicht „verbessern" — 1:1-Verhalten, dann später.
