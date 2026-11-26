# T-999 proof — break config → worker_pool edge

> **Referenz.** Task proof for T-999. The live architecture ratchet remains
> `tests/arch_layers.rs`, `docs/arch/layers.toml` and
> `docs/arch/layer_exceptions.txt`.

**Date:** 2026-11-26
**Owner:** `local/opencode`
**Branch:** `refactor/T-999-config-worker-pool-edge` (claim `928ea11`, base `refactor/T-998-protocol-browser-edge` at `744a175`)
**Scope used:** `src/config/brains.rs`, `docs/arch/layer_exceptions.txt`, this proof file

## Existing util checked first

`src/fsutil.rs` does not exist in the worktree or in `src/lib.rs`; no module
declaration was added and no parallel utility was created. The repository
already has the dependency-free leaf in `src/pool_state.rs`:

- `atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()>`
- `RETRY_UNAVAILABLE_AFTER_SECS: u64 = 120`

`src/worker_pool.rs` already re-exports both. `src/config/brains.rs` now calls
`crate::pool_state::atomic_write` and
`crate::pool_state::RETRY_UNAVAILABLE_AFTER_SECS` directly. The comments that
named the old re-export path were updated to the canonical module. No helper
body, constant value, signature or call semantics changed.

`rg` is not installed in this PowerShell shell; the equivalent ripgrep-backed
search confirms `crate::worker_pool` has no match under `src/config`.

## Guard delta

Measured with `cargo test --test arch_layers -- --nocapture`:

| Measure | Before | After |
|---|---:|---:|
| Top-level modules | 87 | 87 |
| Production `crate::<module>` edges | 274 | 274 |
| Strongly connected components (cycles) | 6 | 6 |
| Upward edges outside excepted cycles | 32 | 32 |

The cycle `brain_limits,circuit_breaker,config,pool_failover,worker_pool`
shrank to `brain_limits,config`. `docs/arch/layer_exceptions.txt` records the
shorter cycle instead of the old one. No `edge:` line was added or removed: all
five modules are in the `cross` layer (rank 0), so the change affects only the
cycle exception. The two `config -> worker_pool` references are gone; the
remaining `config -> brain_limits` edge is now explicitly visible in the
two-module cycle.

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

Claim-time baseline `cargo test --lib --quiet` was already green at
1521 passed / 0 failed / 1 ignored. The test count is unchanged.

The binary-smoke failure is pre-existing, not introduced here: it was already
measured at the T-998 claim commit `103967b` in a clean detached worktree, with
the same exit code before `main`. `Cargo.toml` and every `[profile.*]` setting
are untouched. No live brain call, login or provider request was made.

## Open questions

- None blocking. The remaining `brain_limits,config` cycle is a much smaller
  piece of debt and is outside this task's scope (T-1000 covers config →
  brain_limits).

## Deviations with reasons

- The task scope listed `src/fsutil.rs`, `src/worker_pool.rs` and
  `src/pool_state.rs`. None of them needed an edit: `fsutil.rs` does not exist
  and `pool_state.rs` already hosts both helpers. Creating a new module would
  have required `src/lib.rs` and `docs/arch/layers.toml` changes outside the
  scope and would have duplicated the existing leaf. AGENTS.md rule 1
  ("Bestand zuerst pruefen") took precedence.
- `docs/arch/layer_exceptions.txt` and this proof file are outside the narrow
  source scope but are required by the task DOD and by repo bookkeeping.

## Grenze

No behavior change to CLI, WEBAGENT/1 protocol, API responses or shell-policy
decisions. No credentials, profile, cookie or token was read. No release, tag,
deployment, PR or merge was performed.
