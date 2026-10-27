# T-961 live acceptance — pending

This directory is reserved for the live evidence required by T-961. **This README is a test plan, not evidence.** Do not mark T-961 done or merge PR #78 until the evidence below exists.

## Reproducible before/after run

Use the same ChatGPT account, selected model/effort, Pi version, bridge configuration, task prompt, and temporary workspace for both runs.

1. Record the account's relevant personalization/memory state without changing settings. Record only the relevant state; redact unrelated personal data. If it cannot be inspected, write `not accessible` rather than assuming it is clean.
2. Check out the parent revision before the fix (`dd78128e5d7ace05a7589e466d8c98206b663bcc`) and run the task once. Save the complete Pi JSON transcript, including assistant tool calls, tool execution results, and final response, as `before.json`.
3. Check out the fix revision (`c7e8a5a6a26db1320ee56a880b8a0ba05311f864`) and repeat the same task in a fresh workspace. Save the transcript as `after.json`.
4. In the fixed run, require at least five consecutive successful tool executions in the same conversation. A tool turn counts only when the transcript shows an assistant tool call, a successful execution result, and the assistant continuing the original task afterward. Record the exact consecutive range in `analysis.md`.
5. Add `environment.txt` with commit SHAs, Pi version, bridge version/config (secrets redacted), model/effort, timestamps, and pass/fail result.

## Required files

- `before.json` — unmodified baseline transcript.
- `after.json` — fixed transcript containing at least five consecutive successful tool turns.
- `analysis.md` — concise comparison, root-cause evidence, turn range, and account-state check.
- `environment.txt` — reproducibility metadata; no API keys, cookies, or other secrets.

## Acceptance checklist

- [ ] Baseline reproduces the false tool-access denial after a successful tool result, or documents precisely why it does not reproduce.
- [ ] Fixed run completes at least five consecutive successful tool turns without falsely denying tool availability.
- [ ] Same conditions used for before/after comparison; relevant account state explicitly checked or marked not accessible.
- [ ] Transcripts and environment metadata reviewed for secrets and unrelated personal data before committing.
- [ ] Evidence is committed here and linked from the T-961 taskboard entry.

Do not fabricate transcripts or infer a live result from unit tests/CI.