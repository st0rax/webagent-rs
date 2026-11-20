**Referenzstatus:** laufendes Abnahmeprotokoll; diese Datei ist ein Testplan, kein Ergebnisbeleg.

# T-961 live acceptance — pending (intermittent)

This directory holds the live evidence required by T-961. **This README is a test plan, not evidence.** PRs #82/#83/#84/#86 are merged (`901e8e1`/`5355e18`/`976abd3`/`eaed3d1`); those merges are code changes, not live evidence. Do not mark T-961 done until the evidence below is stable. Status as of 20.11.2026: `NICHT ABGENOMMEN / INTERMITTIEREND` — several raw green runs exist (text turn `PI_BRIDGE_OK`, 5/5 nonces read, 0 errors), but a negative run with the **final binary** `eaed3d1` (`after-framing-negative.jsonl`) still denies the fifth tool after four successful calls. Three of four consecutive runs with that binary were green. See `analysis.md` and `environment.txt`.

## Reproducible before/after run

Use the same ChatGPT account, selected model/effort, Pi version, bridge configuration, task prompt, and workspace layout for both runs. The script creates a fresh isolated temporary directory each time; do not reuse state between runs.

1. Record the account's relevant personalization/memory state without changing settings. Record only the relevant state; redact unrelated personal data. If it cannot be inspected, write `not accessible` rather than assuming it is clean. A logged-in WebView DOM inspection is sufficient (see `account-state.md`); no profile, cookie, or credential file may be read.
2. Start a bridge built from the parent revision before the fix (`dd78128e5d7ace05a7589e466d8c98206b663bcc`). With the current checkout, run `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl` (PowerShell: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/before.jsonl`). This saves Pi's raw JSON-mode event stream; it is a baseline only if the failure reproduces or the result is explicitly analyzed.
3. Start a bridge built from the fix revision and repeat with a fresh temporary workspace: `./scripts/test-pi-provider.ps1 -TranscriptPath docs/proofs/T-961/after.jsonl`.
4. Review each transcript manually. For the fixed run, identify at least five consecutive tool cycles with assistant tool call, successful result, and continuation of the same task. The smoke script checks for at least five tool-start events and all five unique file markers, but that check alone is not the final acceptance proof.
5. Record the exact consecutive range in `analysis.md`. Repeat the run several times with the same binary; a single green run does not prove an intermittently reproducing bug is fixed.
6. Add `environment.txt` with commit SHAs, Pi version, bridge version/config (secrets redacted), model/effort, timestamps, and pass/fail result.

## Required files

- `before.jsonl` — unmodified baseline Pi JSON-mode event stream.
- `after.jsonl` — green run with binary `976abd3` (5+ successful tool turns).
- `after-negative-intermittent.jsonl` — same binary `976abd3`, negative run reproducing the original symptom.
- `after-premerge-green-{1,2}.jsonl` — green runs with the fix-branch binary `37fe0fc`.
- `after-framing-green-{1,2,3}.jsonl` — green runs with the final `master` binary `eaed3d1`.
- `after-framing-negative.jsonl` — same final binary, negative run (4/5, tool denied); decisive against acceptance.
- `analysis.md` — concise comparison, root-cause evidence, turn range, and account-state check.
- `account-state.md` — read-only WebView DOM finding: memory enabled, custom-instructions section present, no tool-denying entry; personal content redacted.
- `availability-negative-{1..8}.jsonl` — raw runs with candidate `0039cd4` (neutral availability rule); only 2/8 strict green.
- `availability-negative-verify.txt` — unmodified strict verifier output for those eight runs.
- `availability-candidate-result.md` — honest evaluation: the rule triggered tool-availability meta replies; code reverted.
- `environment.txt` — reproducibility metadata; no API keys, cookies, or other secrets.

All `*.jsonl` files are byte-exact raw Pi streams (`.gitattributes` marks them `-text`); their SHA256 values are listed in `environment.txt`.

## Acceptance checklist

- [x] Raw before/after Pi JSON-mode transcripts exist and are secret-scan clean.
- [x] Account state checked via the bridge API / WebView DOM (not read from profile files): login confirmed via successful completions; no credentials or profiles read.
- [x] Same conditions used for before/after comparison (Pi 1.5.2, port 8788, model `webagent/chatgpt`, fresh process token).
- [x] Four blockers diagnosed and fixed (selector drift, envelope prefix, login pre-check, current tool-result framing; PRs #82/#83/#84/#86).
- [x] Fixed runs show at least five consecutive successful tool turns — **achieved repeatedly** (`after.jsonl`, `after-premerge-green-*`, `after-framing-green-*`).
- [ ] **Stable / reproducible:** `after-framing-negative.jsonl` with the final binary `eaed3d1` still denies a tool after four successful calls. The availability-rule candidate was even worse (2/8 green). **Open**; root cause of the intermittency unclear.
- [x] **Account memory checked:** read-only WebView DOM inspection of personalization/memory; no entry denying tool access (`account-state.md`). Personal content redacted, no settings changed, no credentials/profiles read.
- [ ] Evidence linked from the T-961 taskboard entry with a passing, stable result — **pending**: T-961 stays `free`, no tag/release.

Do not fabricate transcripts or infer a live result from unit tests/CI.
