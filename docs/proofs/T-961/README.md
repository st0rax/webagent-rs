**Referenzstatus:** laufendes Abnahmeprotokoll; diese Datei ist ein Testplan, kein Ergebnisbeleg.

# T-961 live acceptance — pending (intermittent)

This directory holds the live evidence required by T-961. **This README is a test plan, not evidence.** PRs #82/#83/#84 are merged (`901e8e1`/`5355e18`/`976abd3`); those merges are code changes, not live evidence. Do not mark T-961 done until the evidence below is stable. Status as of 20.11.2026: `NICHT ABGENOMMEN / INTERMITTIEREND` — a raw green run (`after.jsonl`) now exists (text turn `PI_BRIDGE_OK`, 5/5 nonces read, 0 errors), but a run minutes earlier with the **same binary** (`after-negative-intermittent.jsonl`) reproduced the original symptom (tool result denied after a successful call). See `analysis.md` and `environment.txt`.

## Reproducible before/after run

Use the same ChatGPT account, selected model/effort, Pi version, bridge configuration, task prompt, and workspace layout for both runs. The script creates a fresh isolated temporary directory each time; do not reuse state between runs.

1. Record the account's relevant personalization/memory state without changing settings. Record only the relevant state; redact unrelated personal data. If it cannot be inspected, write `not accessible` rather than assuming it is clean. Only the owner can inspect their account memory.
2. Start a bridge built from the parent revision before the fix (`dd78128e5d7ace05a7589e466d8c98206b663bcc`). With the current checkout, run `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl` (PowerShell: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl`). This saves Pi's raw JSON-mode event stream; it is a baseline only if the failure reproduces or the result is explicitly analyzed.
3. Start a bridge built from the fix revision and repeat with a fresh temporary workspace: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/after.jsonl`.
4. Review each transcript manually. For the fixed run, identify at least five consecutive tool cycles with assistant tool call, successful result, and continuation of the same task. The smoke script checks for at least five tool-start events and all five unique file markers, but that check alone is not the final acceptance proof.
5. Record the exact consecutive range in `analysis.md`. Repeat the run several times with the same binary; a single green run does not prove an intermittently reproducing bug is fixed.
6. Add `environment.txt` with commit SHAs, Pi version, bridge version/config (secrets redacted), model/effort, timestamps, and pass/fail result.

## Required files

- `before.jsonl` — unmodified baseline Pi JSON-mode event stream.
- `after.jsonl` — fixed Pi JSON-mode event stream containing at least five consecutive successful tool turns (currently green, but one green run is not acceptance).
- `after-negative-intermittent.jsonl` — same binary, negative run reproducing the original symptom; documents why T-961 is not accepted.
- `analysis.md` — concise comparison, root-cause evidence, turn range, and account-state check.
- `environment.txt` — reproducibility metadata; no API keys, cookies, or other secrets.

## Acceptance checklist

- [x] Raw before/after Pi JSON-mode transcripts exist and are secret-scan clean.
- [x] Account state checked via the bridge API / WebView DOM (not read from profile files): login confirmed via successful completions; no credentials or profiles read.
- [x] Same conditions used for before/after comparison (Pi 1.5.2, port 8788, model `webagent/chatgpt`, fresh process token).
- [x] Three deterministic blockers diagnosed and fixed (selector drift, envelope prefix, login pre-check; PRs #82/#83/#84).
- [x] Fixed run shows at least five consecutive successful tool turns — **achieved in `after.jsonl`** (6 tool starts, all 5 nonces, final answer with all markers).
- [ ] **Stable / reproducible:** a negative run with the same binary reproduces the original symptom — **open**, root cause of the intermittency unclear.
- [ ] Evidence linked from the T-961 taskboard entry with a passing, stable result — **pending**: repeated green runs and owner release required.

Do not fabricate transcripts or infer a live result from unit tests/CI.
