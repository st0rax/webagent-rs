> **Referenz.** Beleg für T-967 (Release-Gate). Kein Tag, kein GitHub-Release.
> Operativer Stand: `docs/CURRENT_WORK.md`.

# T-967 — Release-Gate gegen Versionsdrift und doppelte CI-Läufe

**Datum:** 2026-11-24
**Branch:** `chore/T-967-release-gate-opencode` (getrennter Worktree). Die
 parallele DeepSeek-Arbeit lief auf `chore/T-967-release-gate`; diese Datei
 belegt ausschließlich die hier geprüfte Umsetzung.

## Befund und Maßnahmen

`v0.11.4` wurde von einem Commit mit `Cargo.toml` `0.11.3` getaggt. Der
Release-Workflow baute und veröffentlichte bei jedem `v*` ohne Versionsprüfung
und ohne Tests. Umgesetzt:

- `scripts/verify-release-version.ps1` — fail-closed, nur lokal, liest
  `[package].version` aus `Cargo.toml` und den `webagent`-Eintrag aus
  `Cargo.lock`; Exit 0 bei Match, 1 bei Mismatch/unlesbarem Baum, 2 bei
  ungültigem `vX.Y.Z`-Format.
- `.github/workflows/release.yml` — `verify`-Job mit Versionsprüfung für
  Tag-Refs und `cargo test --lib --locked`; `windows`, `linux`, `android` und
  `publish` hängen davon ab. Bei `workflow_dispatch` auf einem Branch läuft
  nur der Testteil. Der Job installiert WebKitGTK mit gebundenen/apt-Retries,
  weil `default = ["webview"]` sonst schon beim Kompilieren auf Ubuntu scheitert.
  `verify` und die drei Build-Jobs haben nur `contents: read`; Schreibrechte
  bleiben ausschließlich beim `publish`-Job.
- `.github/workflows/ci.yml` — Push nur noch auf `master`; PR-Heads werden
  nicht doppelt geprüft. Alle Jobs laufen mit `contents: read`, und jeder
  Cargo-Schritt nutzt `--locked`, damit Lockfile-Drift schon im PR auffällt.
- `Cargo.toml`/`Cargo.lock` — `0.11.5`.
- `docs/RELEASE.md`, `CHANGELOG.md`, `docs/CURRENT_WORK.md`,
  `docs/TASKBOARD.json`/`.md` — Verfahren, Grenzen und Status.

## Gemessene Prüfungen

| Prüfung | Ergebnis |
|---|---|
| `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` | Exit 0, `Cargo.toml 0.11.5 = Cargo.lock 0.11.5` |
| `-Tag v0.11.4` | Exit 1, Mismatch-Meldung mit `0.11.4` vs. `0.11.5` |
| `-Tag 0.11.5` | Exit 2, Formatmeldung (erwartet `vX.Y.Z`) |
| isolierter Temp-Test ohne `Cargo.lock` | Exit 1, `Versionsdateien nicht lesbar: Datei fehlt` |
| PyYAML `safe_load` beider Workflows | OK (`release.yml` 5 Jobs, `ci.yml` 3 Jobs) |
| `actionlint 1.7.12` auf beiden Workflows | Exit 0, keine Befunde |
| `cargo fmt --all -- --check` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo test --lib` | 1505 passed / 0 failed / 1 ignored |
| `cargo test --lib --locked` | 1505 passed / 0 failed / 1 ignored (Lockfile unverändert) |
| `cargo test --no-default-features` | 1499 + 7 passed / 0 failed / 1 ignored |
| `git diff --check` | grün |

Vor der Versionshebung wurde das Skript zusätzlich gegen den alten Stand
gemessen: `v0.11.3` → Exit 0, `v0.11.4` → Exit 1. Damit ist der historische
Mismatch direkt nachgestellt.

## Ehrliche Grenzen

- Kein Tag, kein GitHub-Release, kein Deployment, kein Merge nach `master`.
  Der bestehende Tag `v0.11.4` wurde nicht verschoben, gelöscht oder
  force-gepusht. Ein erneuter Workflow-Lauf gegen ihn scheitert absichtlich.
- Die GitHub-Action selbst wurde **nicht** live auf einem Tag ausgeführt;
  geprüft wurden Skript, YAML-Struktur und lokale Rust-Gates.
- Das Skript prüft keine Git-Historie, keine Signatur und keine Binary-Version.
- Keine Live-Provider-Läufe, keine Zugangsdaten, keine Profile/Cookies/Tokens,
  keine Kontoeinstellungen angefasst.
- Die parallele DeepSeek-Umsetzung auf `chore/T-967-release-gate` ist hier
  **nicht** bewertet; dieser Beleg gilt nur für den oben genannten Branch.
