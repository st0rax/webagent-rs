# T-961 Live Acceptance Analysis — PR #78

## Before (parent: dd78128)
- CI: success, but tool-context framing errors observed in logs
- Tool execution: context markers missing or inconsistent
- Issue: tool results not properly labeled as `client-tool` history

## After (fix: 1e2d5de2 / PR #78)
- CI: green (Android + Windows, 27.10.2026)
- Live evidence: 6 consecutive successful tool turns (after.jsonl)
- All turns show `context_framed: true`
- Tool results properly labeled as structured client-tool history
- No false tool-access denial after successful results

## Root Cause Evidence
The fix clarifies that tool results are execution data (not model output) and that the model should continue the original task. This prevents the false continuation denials observed in T-961.

## Acceptance Result
PASS — 5+ consecutive tool turns verified, account state checked, no secrets in evidence.
