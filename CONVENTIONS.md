# WebAgent Rust — Design- & Doku-Konventionen

**Historischer Kontext (nicht mehr aktueller Auftrag):** dieses Dokument
begann als Anleitung für Aider, den Python-Agenten (`../src/webagent/`)
plattformunabhängig nach Rust zu portieren. Der Port ist inzwischen
weitgehend abgeschlossen (8/8 Provider, siehe `START_HERE.md`) — die
Portierungsregeln unten gelten weiter für Code, der noch aus der
Python-Referenz übernommen wird, sind aber nicht mehr der Haupt-Auftrag.
Aktueller Stand/Fokus: `START_HERE.md`, `MISSION.md`.

⚠️ **Korrektur:** Ein früherer Satz hier sagte „Default-Impl später via
CDP/WebSocket" — das ist überholt. Die Browser-Anbindung ist **Embedded
WebView** (`wry`/`tao`), kein CDP. Siehe `PROVIDER_STATUS.md`.

## Ziel & Scope

- **Plattformen:** Windows, Linux, Android. Kein Code, der eine Plattform
  hart voraussetzt. Plattform-Spezifisches (PID-Liveness, Shell-Binary,
  Profilpfade) hinter `#[cfg(...)]` oder eine kleine Trait-Abstraktion legen.
- **Kern zuerst, Browser später:** Die reine Logik (Protokoll-Parser, Timeouts,
  Run-Store, Transcript, Loop-Guard, Observer-Textheuristik, Doctor, Prompts,
  Controller-Zustandsmaschine) ist plattformrein und wird zuerst portiert und
  getestet. Die Browser-Anbindung ist eine austauschbare Trait-Grenze
  (`brain::BrainBackend`), Impl via Embedded WebView (`wry`/`tao`).

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
`regex`, `fancy-regex`, `clap` (bereits in `Cargo.toml`).

**Regex-Lookaround:** Die `regex`-Crate unterstützt KEIN Lookahead/Lookbehind
(`(?=...)`, `(?<=...)`) — solche Muster panics beim Kompilieren. Für jedes aus
Python übernommene Pattern mit Lookaround `fancy_regex::Regex` verwenden (dessen
`.find`/`.captures` geben `Result` zurück, entsprechend behandeln). Muster ohne
Lookaround weiter mit der schnelleren `regex`-Crate. **Nicht** hinzufügen: `chrono`,
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

- Jedes Modul hat `#[cfg(test)] mod tests`. Bei Portierung aus Python: Testfälle
  direkt übernehmen. Bei neuen (Rust-nativen) Features: Unit-Tests pro reiner
  Funktion/Entscheidungslogik, plus mind. ein End-to-End-Test gegen echte
  Brains vor dem ersten Commit (siehe „Design-Prinzipien" unten).
- Kein echter Browser, kein Netz, keine echten Logins in Unit-Tests.
- `cargo test` + `cargo clippy --all-targets -- -D warnings` müssen grün sein,
  bevor ein Modul als fertig gilt. Bekannte Ausnahme: `executor::tests::*`
  kann unter Voll-Parallel-Läufen vereinzelt flaken (Ursache: Prozess-Spawn-
  Kontention, siehe Commit-Historie von `executor.rs`) — bei Zweifel isoliert
  erneut laufen lassen (`cargo test --lib executor::`).

## Design-Prinzipien (gilt für neue, Rust-native Features — nicht nur Portierung)

- **Kein Allowlist-only, kein Sandbox-Anspruch.** Die Shell ist by Design offen
  (Single-User-Local-Agent). Sicherheitsmaßnahmen (`shell_policy.rs`) sind ein
  Netz gegen versehentlich Destruktives/Prompt-Injection, keine Sandbox.
- **Extern Blockiertes flaggen, nicht als Fehler werten.** Tageslimit/Login/
  Cloudflare sind externe Zustände, kein Tool-Defekt — sichtbar machen
  (`backend_status="blocked"`), Lauf fortsetzen statt abbrechen.
- **So testen, wie das Produkt benutzt wird.** Ein Kaltstart-Relay-Loop
  (viele Sessions im Sekundentakt) erzeugt selbst künstliche Rate-Limits —
  Stabilität/Reliability immer über eine gehaltene REPL-Session messen, nicht
  über Kaltstart-Hämmern.
- **Unsicherheit ehrlich darstellen statt raten.** Ein Wert ohne Daten ist
  „unbekannt", nicht „schlecht" oder „gut" (siehe `brain_score.rs`s
  Wilson-Score: 0 Ereignisse → 0.5, nicht 0.0).
- **Vor größeren Refactors: echten Zustand nachmessen, nicht Doku-Zahlen
  glauben.** Reviews/Pläne können stale sein (siehe `executor::tests`-Beispiel
  oben) — vor „das ist kaputt, das muss ich fixen" erst selbst reproduzieren.

## Was NICHT tun

- Keine `async`-Runtime einführen, solange der synchrone Kontrollfluss reicht.
  Der Agent-Loop ist bewusst sequentiell; parallele Runs sind kein aktuelles
  Ziel. Erst bei explizitem Multi-Run-Bedarf neu bewerten.
- Keine spekulativen Features/Abstraktionen ohne konkreten Bedarf. Drei
  ähnliche Zeilen sind besser als eine verfrühte Abstraktion.
- Profil-/Sicherheitslogik nicht auf Verdacht „verbessern" — Bedarf erst
  belegen (Test, Repro, Review-Fund), dann ändern.

## Doku-Richtlinien

- **`START_HERE.md`** — einziger Einstiegspunkt, Status + Architektur +
  Build/Test + offene Punkte. Wird bei jeder strukturellen Änderung
  aktualisiert (siehe Pflegepflicht dort).
- **`MISSION.md`** — aktueller Arbeitsfokus/Auftrag, ändert sich häufiger als
  `START_HERE.md`. Bei Themenwechsel aktualisieren, nicht anhäufen.
- **`CONVENTIONS.md`** (diese Datei) — Design-Prinzipien + wie Dokumentation
  organisiert wird. Ändert sich selten.
- **`README.md`** — öffentliche/GitHub-Oberfläche, kurz, verkaufsorientierter
  als `START_HERE.md`.
- **`PROVIDER_STATUS.md`** — Provider-Messwerte mit Historie (append, nicht
  überschreiben — alte Messungen sind Kontext für „warum haben wir das
  geglaubt").
- **`docs/*.md`** — abgeschlossene Konzept-/Planungsdokumente (z. B.
  `AUTORESEARCH_PLAN.md`, `GENIUS_COUNCIL_CONCEPT.md`). Jedes trägt einen
  Status-Banner oben (GEPLANT / DEFERRED / UMGESETZT).
- **Kein neues Root-`.md` ohne Grund.** Bevor eine neue Datei entsteht: passt
  der Inhalt in eine der obigen? Sprawl (siehe `bot2bot`s Review: 10+
  Root-Docs, teils widersprüchlich) ist genau das Problem, das diese Struktur
  vermeiden soll. Externe Reviews (`CODE_REVIEW.md`/`CLAUDE_PROPOSALS.md`)
  sind die Ausnahme — die kommen von außen, nicht von uns benannt.
