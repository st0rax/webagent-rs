# Changelog
> **Referenz:** Versionshistorie. Der aktuelle Produktzustand steht in `docs/OVERVIEW.md`, der operative Arbeitsstand in `docs/CURRENT_WORK.md`.

## [Unreleased]

## [0.11.7] - 2026-11-26

- Vier lokale Härtungsscheiben nach `master` zusammengeführt (PRs #108–#111; #109–#111 wurden wegen Doku-Konflikten als lokal gesquashte Commits mit PR-Referenz gepusht und anschließend geschlossen). T-969 liefert `docs/PROTOCOL.md` und `docs/SECURITY.md` aus dem Code; T-970 isoliert Turn-Beobachtungen thread-lokal (`PendingScope`), T-971 kennzeichnet `VerifiedFree` ehrlich als Registry-Beleg ohne Capability-Gate, T-972 teilt `controller.rs`/`webview_runtime.rs` in Untermodule (3349→1909 bzw. 3315→2229 Zeilen). Alle PR-Checks und die Gates auf dem zusammengeführten Stand sind grün (1521 passed / 0 failed / 1 ignored).
- Der Boardstand T-969..T-972 ist `done`; `docs/CURRENT_WORK.md` hält den Merge- und Konfliktauflösungsweg fest. Kein Force-Push, keine History-Umschreibung.

## [0.11.6] - 2026-11-25

- Release `v0.11.6`: published 2026-11-25 21:21:55 UTC via green workflow `45702611313` (PR #107, tag `1149dff`, seven SHA-256-verified assets). The publish job with `download-artifact@v8` and `action-gh-release@v3` was exercised for the first time; one internal `Buffer()` deprecation was logged, no Node-20 action warning. The local deploy target was updated to these assets with the `v0.11.5` files kept as rollback; `canary` is 8/8 `ok` there. No production deployment.
- Diagnostics: `canary` now accepts embedded selector copies as present, matching `load_selectors` and `brains-health`; a deployed binary without the source tree no longer reports seven false `missing_selectors` failures. A unit test covers the embedded fallback. A production-readiness assessment with verified evidence, six residual risks, rollback and Go/No-Go criteria is in `docs/PRODUCTION_READINESS.md`. The canary fix is not in the already published `v0.11.5`; it is included in `v0.11.6`.
- CI maintenance: checkout v7, cache v6, upload-artifact v7, download-artifact v8 and action-gh-release v3 replace the Node-20-targeting actions. PR #105 (`c1f01f0`) is merged; all four PR checks and a `workflow_dispatch` run of `release.yml` on master (`45690875482`) are green with zero Node-20 warnings. The tag-only publish job was not re-exercised; its inputs were checked statically against the new `action.yml` files. Release `v0.11.5` and its tag are unchanged.
- Release `v0.11.5`: `scripts/copy-webview2-loader.ps1` now finds `WebView2Loader.dll` in both Cargo `OUT_DIR` layouts (`build/<crate>/<hash>/out` and `build/<crate>-<hash>/out`). The first release run failed at that step before publishing anything; the owner approved re-tagging the same version, and workflow `45684483569` then published seven assets (Windows EXE + DLL, Linux, Android, three SHA-256 files). Published 2026-11-25. The published Windows assets were staged to a local deploy target with SHA-256 and `--version` verification (`0.11.5` / `9b2ad7e6e`); no production deployment.
- T-961/G-002: accepted 2026-11-25. A narrowly scoped one-shot fresh-chat retry handles chatgpt's semantic tool denial with the unchanged original task and prior tool results as data; no correction meta, no `required` escalation, and `Retry erschoepft` instead of a silent PASS. Two causal live runs (`t961_fresh_chat_retries=1`, 5/5 tool turns, strict PASS) and 14 strict-clean runs total are in `docs/proofs/T-961/semantic-denial-retry/`; raw transcripts, bridge logs, strict verifier output, and negative runs are included. The root cause of the provider behavior is not isolated, no account settings were changed, and other failure classes remain. Board now 93 `done` / 0 `free`; merged to `master` via PR #104 (`ea88203`).
- T-968: kimi `reasoning_effort` is now live-verifiable. The verifier reads the full trigger label (`Schnell Hoch`), opens the hover-gated submenu, selects a real option from the live DOM, independently re-reads the change and restores the original value in the same session. Live proof: `Schnell Hoch` → `Standard` → restored (10054 ms, `outcome=passed`, `selector_hash=2705021080`); raw DOM surveys and both verify transcripts are in `docs/proofs/T-968/`. The `effort/kimi` matrix cell is derived only from that proof. Merged to `master` via PR #103 (`a2a144a`).
- T-967: the release gate now fails closed when a `vX.Y.Z` tag does not match `Cargo.toml` and the `webagent` entry in `Cargo.lock` (`scripts/verify-release-version.ps1`, exit 0/1/2). `release.yml` runs that check plus `cargo test --lib --locked` in a `verify` job before any build or publish, with `contents: read` for verify and build jobs; `ci.yml` no longer runs the full matrix twice for the same PR head, declares read-only `contents` permissions, and uses `--locked` in every cargo step. Manifest and lockfile moved to `0.11.5`; the procedure is documented in `docs/RELEASE.md`. Merged to `master` via PR #102 (`c0890d5`). A read-only DeepSeek review confirmed the gate (`review ok`); the release and Android builds were subsequently switched to `--locked` as well.
- Added a machine-readable mirror block in `docs/TASKBOARD.md` and a fail-closed test that compares it with the JSON (task count, IDs, statuses).
- Fixed `is_test_run()` so unit tests can no longer write into the production data dir when Cargo places the test binary outside `target/debug/deps`.
- Resolved a `clippy::needless_bool` warning in `src/controller/resume.rs` surfaced by newer toolchains (no behavior change).
- T-949 (#81): the configured `dialog_dismiss_button` is now checked for consent labels before clicking; agreement text is skipped and unreadable labels fail closed. Cookie-reject choices stay untouched. Mock-DOM tests and gates are green.

### Historische Zwischenstände (durch die heutige Abnahme ersetzt)

- Das vollständige 91-Task-Board aus `dd78128` war zunächst wiederhergestellt; T-961 war zu jenem Zeitpunkt `free`.
- Die Korrektur von `docs/proofs/T-961/` entfernte unbelegte PASS-Aussagen und nonexistente Commit-SHAs. Die damaligen rohen Transkripte waren negativ (0/5 Tool-Turns, `session_state=Unbestimmt`); das war kein Abnahmebeleg.
- `GOALS.md`, `README.md`, `docs/OVERVIEW.md` und `docs/CURRENT_WORK.md` beschrieben den Stand mit offenem G-002/T-961 trotz `v0.11.4`; diese Aussage wurde am 2026-11-25 durch die Endabnahme oben ersetzt.
- `v0.11.4` wurde am 28.10.2026 vor Abschluss der Live-Abnahme getaggt und bleibt als historischer Tag unangetastet; der neue Tag `v0.11.5` wird nach PR #104 und grüner Master-CI gesetzt.

## [0.11.4] - 2026-10-28

### Added
- **login-all --parallel N** (echte Parallelitaet, max 3): pro Brain ein eigener `login-worker`-Kindprozess (re-exec), JSON-Ergebnis per Tempdatei (`WEBAGENT_LOGIN_WORKER_RESULT`), Live-Fortschritt via stdout-Inheritance; automatisch sequenziell bei `WEBAGENT_USE_SHARED_BROWSER=1` (SingletonLock-Race)
- **REPL `/login-all parallel=N`**: Paritaet zur CLI (vorher nur sequenziell)
- **run-\*.bat: WebView2Loader-Guard** — Kopiert `WebView2Loader.dll` aus `webview2-com-sys` nach `target/debug`, falls die Datei fehlt (behebt `0xC0000135`-Crash nach frischem `cargo build`)

## [0.11.3] - 2026-09-12

### Added
- **Web-UI echte API-Anbindung** (T-905): `web/index.html` komplett neu — echte `fetch`-Aufrufe zu `/api/health/brains`, `/api/sessions` (anlegen, Chat-Stream `events?since=`, Stop), `/api/quelle`, `/api/groups`, `/api/sources`; Brain-Detail, Aktivitätsleiste; Einzeldatei 32,7 KB (< 48 KiB), Layout im T-203-Grok-Schema (a11y, `prefers-reduced-motion`, Skip-Link)
- **Web-UI Restlücken geschlossen** (T-906): Gruppenlauf (`POST /api/groups/{id}/run`) öffnet erzeugte Session (`run_id`) und rendert Events live (rundenbezogene Stati, TextDeltas, Synthese, Done; 409-busy-Retry); Datei-Upload im Composer (`POST /sessions/{id}/upload`, 202 `accepted`); Brain-Fenster show/hide im Detail (`POST /brains/{id}/show|hide`, 404-Hinweis ohne offenes Fenster)
- **zai stabil 4/4** (T-901): reasoning-toggle-Fix (DOM-klick + Readback), effort-Toggle geprüft; Live-Beweis `docs/proofs/T-901/`
- **AutoRouter-Deckung** (T-902): Live-Beleg `docs/proofs/T-902/`
- **chatgpt Slideover-Drift** (T-903): Driftvermessung dokumentiert, Selektoren aktualisiert; Live-Beweis `docs/proofs/T-903/`
- **VerifiedFree-Providergrenze** (T-904): Grenze für paid-only-Features dokumentiert; Live-Beweis `docs/proofs/T-904/`

### Changed
- **Tests** erzwingen jetzt echte API-Anbindung: `ui_ist_echt_an_die_api_angebunden_und_kompakt` assertiert `fetch(`, `/api/health/brains`, `/api/sessions`, `events?since=` statt „kein fetch"

## [0.11.2] - 2026-09-12

### Added
- **Phase-8-Abnahme abgeschlossen** (Brain-Vereinheitlichung T-801–T-808): alle 26 TASKBOARD-Tasks done
- Live-Verify-Suite (46 Belege je Brain) + CAPABILITY_MATRIX `as_of 2026-09-12`
- Verifizierter Taskabschluss (Acceptance-Receipt): E2E live belegt, negativ fail-closed, danach `done` nur mit gültigem Receipt
- Windows-Prozessprobe: keine Prozess-/Profil-Leaks nach Headless-Läufen dokumentiert

### Fixed
- `build.rs`: Windows-Version-Resource wird nur noch unter GNU/mingw als `resource.o` direkt gelinkt; unter MSVC (CI) genügt das von winres vermeldete `resource.lib` — behebt `LNK1181: cannot open input file 'resource.o'` im Release-Workflow
- E2E-Resume-Pfad: Brains können UI-Artefakte (`text\nKopieren` etc.) vor der WEBAGENT/1-Antwort senden, die der Parser fail-closed ablehnt (dokumentiert, keine stille Reparatur)

### Release
- GitHub-Release `v0.11.2`: Windows (`webagent-windows-x86_64.exe` 10,4 MB + `WebView2Loader.dll`), Linux, Android (CI green)

## [0.11.1] - 2026-08-25

### Refactored
- Extract `wilson_lower_bound` to shared `scoring` module (DRY: brain_score + code_score)
- `canary` module: `pub` for binary access, `all_ok` test-only
- PowerShell wrapper: `$PSStyle.OutputRendering = 'PlainText'` to prevent ANSI escapes

## [0.11.0] - 2026-08-23

### Added
- Linux product release (feat/linux-webview)
- TUI tile worker brains across processes
- Clean-room tasks without memory
- ChatGPT German usage limit detection

### Fixed
- Protocol: preserve literal code through webchat markdown
- Protocol: preserve payloads containing limit text
- Controller: re-anchor task during protocol repair
- Browser: track reused response containers
- Browser: require ChatGPT user echo after send
- Browser: repair stable truncated responses promptly
- Controller: stop on unverified browser sends
- Shell: reject nested PowerShell encoding traps
- Protocol: ignore capacity terms in technical prose

## [0.10.1] - 2026-08-XX

### Fixed
- Two defects from v0.10.0

## [0.10.0] - 2026-08-XX

### Added
- Three acceptance proofs
- Capability proof system
- Design vote mechanism
- Benchmark pipeline
