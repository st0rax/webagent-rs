# T-998 proof — break protocol/brain → browser edges

> **Referenz.** Task proof for T-998. The live architecture ratchet remains
> `tests/arch_layers.rs`, `docs/arch/layers.toml` and
> `docs/arch/layer_exceptions.txt`.

**Date:** 2026-11-26
**Owner:** `local/opencode`
**Branch:** `refactor/T-998-protocol-browser-edge` (claim `103967b`, base `feature/T-997-arch-layers`)
**Scope:** `src/protocol/parser.rs`, `src/protocol/mod.rs`, `src/brain.rs`, `src/browser/mod.rs`, `src/browser/blocking.rs`

## What changed

Two pure text helpers were moved into the protocol layer:

- `has_protocol_payload` now lives in `src/protocol/mod.rs`. `src/brain.rs` calls
  `crate::protocol::has_protocol_payload`; `src/browser/mod.rs` re-exports it so
  existing `crate::browser::has_protocol_payload` call sites keep compiling.
- `block_phrase_in_text`, its shared `BLOCK_PHRASES` list and
  `BLOCK_BANNER_MAX_CHARS` now live in `src/protocol/mod.rs`.
  `src/protocol/parser.rs` calls `super::block_phrase_in_text`; the browser
  backend continues to use the re-export from `src/browser/mod.rs`.
- `src/browser/blocking.rs` imports the shared phrase list from `crate::protocol`
  and no longer defines either helper.

No signature, phrase, threshold, CLI output, protocol envelope, API response or
shell-policy decision changed. No `[profile.*]` entry in `Cargo.toml` was touched.

## Guard delta

Measured with `cargo test --test arch_layers -- --nocapture`:

| Measure | Before | After |
|---|---:|---:|
| Top-level modules | 87 | 87 |
| Production `crate::<module>` edges | 274 | 274 |
| Strongly connected components (cycles) | 6 | 6 |
| Upward edges outside excepted cycles | 30 | 32 |

The former eight-module cycle
`api_bridge, brain, brain_probe, browser, browser_inference, browser_pool, protocol, relay`
shrank to the six-module cycle
`api_bridge, brain_probe, browser, browser_inference, browser_pool, relay`.
`brain` and `protocol` dropped out. `docs/arch/layer_exceptions.txt` now records
the shorter cycle instead of the old one.

Removing that cycle exposed two upward edges that the old cycle line had hidden:
`browser -> protocol` (already present through `is_possibly_truncated` and
`parse`) and the new `brain -> protocol` call site for the moved helper. Both are
listed explicitly as `edge:...` exceptions. The exception count therefore did
not shrink overall, but the browser cycle is smaller and no behavior changed.

## Verification

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo clippy --features webview --all-targets -- -D warnings` | exit 0 |
| `cargo test --lib --quiet` | 1521 passed / 0 failed / 1 ignored |
| `cargo test --test arch_layers -- --nocapture` | 4 passed / 0 failed |
| `cargo test --all-targets --quiet` | 1521 lib + 7 binary-unit + 4 integration passed / 0 failed |
| `cargo test --locked --no-default-features --test arch_layers --quiet` | 4 passed / 0 failed |
| `git diff --check` | exit 0 |
| `target/debug/webagent.exe --help` | blocked: exit `-1073741515` (`0xC0000005`) |
| `target/debug/webagent.exe diagnose` | blocked: same loader crash before `main` |

Test count is unchanged from the T-997 baseline: 1521 passed / 0 failed /
1 ignored in `cargo test --lib`.

The two binary-smoke checks are red, but not caused by this task. A clean
detached worktree at the claim commit `103967b` builds a binary of
150,498,625 bytes and crashes on `--help` with the same `-1073741515`
(`0xC0000005`) before `main`; this branch's binary is 150,498,536 bytes.
`Cargo.toml` and all `[profile.*]` settings are untouched, so the known loader
regression is pre-existing and out of scope here. No live brain, login or
provider call was made.

The claim-time baseline run was red: 1520 passed / 1 failed / 1 ignored because
`docs/arch/LAYERS.md` lacked the `**Referenz.**` banner required by
`startup::tests::betriebs_markdown_hat_eine_wahrheit`. The same failure then
named `docs/proofs/T-997/README.md`. Both are T-997 documentation artifacts;
see Deviations below.

`rg` is not installed in this PowerShell shell. The equivalent ripgrep-backed
search was used: `crate::browser` has no match in `src/protocol/*.rs` or
`src/brain.rs`.

## Open questions

- None blocking. The two new upward-edge exceptions are deliberate debt; a
  later task could move `has_protocol_payload` to a lower-ranked leaf module if
  the layer model should be tightened further.

## Deviations with reasons

- `docs/arch/LAYERS.md` and `docs/proofs/T-997/README.md` were edited although
  they are outside the T-998 `scope`. Both needed the repository-mandated
  `**Referenz.**` banner before any library gate could be green. The edits are
  documentation-only, reversible, and repair T-997 artifacts on which this
  branch is based.
- The ratchet exception list gained two `edge:` lines while losing one cycle
  member set. This is a deviation from a strict "list only shrinks" reading, but
  the task explicitly requires the helpers in the protocol layer while `brain`
  is ranked lower; no in-scope placement avoids the new upward edge. The cycle
  itself is reduced as required by the task.

## Grenze

No login, browser session, profile, cookie, token or credential was read. No
live provider call, release, tag, deployment, PR or merge was performed.
