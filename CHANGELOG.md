# Changelog
> **Referenz:** Versionshistorie. Der aktuelle Produktzustand steht in `docs/OVERVIEW.md`, der operative Arbeitsstand in `docs/CURRENT_WORK.md`.

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
