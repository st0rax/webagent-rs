# WebAgent Rust — Design-Konventionen

> Einstieg: [`START_HERE.md`](START_HERE.md), aktueller Arbeitsstand:
> [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md), Produktwahrheit:
> [`docs/OVERVIEW.md`](docs/OVERVIEW.md), TUI: [`AGENTS.md`](AGENTS.md) §6.
> Regeln, die der Code heute braucht. Kein Port-Auftrag, kein zweites
> Betriebsdokument.

## Geltende Regeln

- **Plattform:** Windows, Linux, Android. Plattform-Spezifisches hinter
  `#[cfg(...)]` oder einer kleinen Trait-Grenze. Session-Einstieg, Transcript
  und Slash-Parser bleiben ohne Win32.
- **Ein Beweis-Gate:** nur `capability_proof`. Kein zweiter Beleg-Speicher.
- **Ein Brain-Trait:** `BrainBackend`. Embedded WebView (`wry`/`tao`) ist die
  produktive Windows-Impl. CDP ist kein Betriebsdefault.
- **Shell:** offen (lokaler Single-User-Agent). `shell_policy` ist ein Netz,
  keine Sandbox. `PFLICHT_DENY` und Harvest-Lock darf der Bench nicht
  wegernten.
- **Regex:** `regex` kennt kein Lookahead/Lookbehind. Muster mit Lookaround
  nur über `fancy_regex`. Kein `chrono`, kein `windows-sys`/`libc` außer in
  der gekapselten PID-Liveness.
- **Wilson:** `n == 0` → `0.5` (unbekannt, nicht schlecht). Gleiches
  Verhältnis, mehr Evidenz → höher. Eine Rechnung, zwei Aufrufer
  (`code_score`, `brain_score`).
- **Fehler:** `unwrap`/`expect` im Bibliothekscode nur bei nachweislich
  unmöglichen Fällen (Kommentar). Öffentliche APIs: `Result`.
- **Tests:** kein echter Browser, kein Netz, keine Logins in Unit-Tests.
  `cargo test --lib` und `cargo clippy --all-targets -- -D warnings` vor
  fertiger Scheibe. `pub` ist Absicht, nicht Default.

## Was nicht gilt

- Python-Tests unter `../tests/` sind nicht die Spezifikation.
- `docs/MISSION.md` ist Archiv, kein Einstieg. `START_HERE.md` ist der
  verbindliche, stabile Einstieg.
- Keine 30. lebende Spezifikation. Betrieb: `README.md`, `AGENTS.md` §6,
  `docs/OVERVIEW.md`, `docs/PROTOCOL_SCHEMA.md`.
- Keine `async`-Runtime, solange der synchrone Loop reicht.
- Die drei Karpathy-Schleifen nicht zusammenlegen, nur weil sie ähnlich
  aussehen.

## Doku-Rollen

| Rolle | Dateien |
|---|---|
| Einstieg/Arbeit | `START_HERE.md`, `CONTRIBUTING.md`, `docs/CURRENT_WORK.md` |
| Betrieb | `README.md`, `AGENTS.md`, `docs/OVERVIEW.md`, `docs/PROTOCOL_SCHEMA.md` |
| Regeln | diese Datei |
| Referenz | `docs/ARCHITECTURE.md`, `docs/PROVIDER_STATUS.md` (Zahlen nachmessen) |
| Archiv | jede andere Repo-`.md` — Banner oben, nicht betreiben |
