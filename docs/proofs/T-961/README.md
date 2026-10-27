**Referenzstatus:** laufendes Abnahmeprotokoll; diese Datei ist ein Testplan, kein Ergebnisbeleg.

# T-961 live acceptance — pending

This directory is reserved for the live evidence required by T-961. **This README is a test plan, not evidence.** Do not mark T-961 done or merge PR #78 until the evidence below exists.

## Reproducible before/after run

Use the same ChatGPT account, selected model/effort, Pi version, bridge configuration, task prompt, and workspace layout for both runs. The script creates a fresh isolated temporary directory each time; do not reuse state between runs.

1. Record the account's relevant personalization/memory state without changing settings. Record only the relevant state; redact unrelated personal data. If it cannot be inspected, write `not accessible` rather than assuming it is clean.
2. Start a bridge built from the parent revision before the fix (`dd78128e5d7ace05a7589e466d8c98206b663bcc`). With the current checkout, run `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl` (PowerShell: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl`). This saves Pi's raw JSON-mode event stream; it is a baseline only if the failure reproduces or the result is explicitly analyzed.
3. Start a bridge built from the fix revision (`c7e8a5a6a26db1320ee56a880b8a0ba05311f864`) and repeat with a fresh temporary workspace: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/after.jsonl`.
4. Review each transcript manually. For the fixed run, identify at least five consecutive tool cycles with assistant tool call, successful result, and continuation of the same task. The smoke script checks for at least five tool-start events and all five unique file markers, but that check alone is not the final acceptance proof.
5. Record the exact consecutive range in `analysis.md`.
6. Add `environment.txt` with commit SHAs, Pi version, bridge version/config (secrets redacted), model/effort, timestamps, and pass/fail result.

## Required files

- `before.jsonl` — unmodified baseline Pi JSON-mode event stream.
- `after.jsonl` — fixed Pi JSON-mode event stream containing at least five consecutive successful tool turns.
- `analysis.md` — concise comparison, root-cause evidence, turn range, and account-state check.
- `environment.txt` — reproducibility metadata; no API keys, cookies, or other secrets.

## Acceptance checklist

- [ ] Baseline reproduces the false tool-access denial after a successful tool result, or documents precisely why it does not reproduce.
- [ ] Fixed run completes at least five consecutive successful tool turns without falsely denying tool availability.
- [ ] Same conditions used for before/after comparison; relevant account state explicitly checked or marked not accessible.
- [ ] Transcripts and environment metadata reviewed for secrets and unrelated personal data before committing.
- [ ] Evidence is committed here and linked from the T-961 taskboard entry.

Do not fabricate transcripts or infer a live result from unit tests/CI.