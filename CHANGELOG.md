# Changelog
> **Referenz:** Versionshistorie. Der aktuelle Produktzustand steht in `docs/OVERVIEW.md`, der operative Arbeitsstand in `docs/CURRENT_WORK.md`.

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
