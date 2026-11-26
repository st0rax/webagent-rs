# T-1000 proof: config no longer reaches brain_limits

> **Referenz.** Task proof for T-1000. The live architecture ratchet remains
> `tests/arch_layers.rs`, `docs/arch/layers.toml` and
> `docs/arch/layer_exceptions.txt`.

**Claim.** Branch `refactor/T-1000-config-brain-limits-edge`, claim commit `3d038ea`, owner `local/opencode`, 2026-11-26. The owner's "autonom weiterarbeiten" overrode the formal `depends_on: T-999` gate; T-999 stays `claimed` because it is not merged into `master`.

## Changes

- `src/config/limits.rs`: added the `AcceptedChars` trait with an `Option<usize>` implementation. `max_observation_chars_for` now takes `L: AcceptedChars` as a parameter instead of calling `crate::brain_limits::accepted_chars(brain_id)`.
- `src/brain_limits.rs`: implements `AcceptedChars for &str`, delegating to the existing measured-store lookup, so the unchanged caller in `controller.rs` keeps working. Added two tests and a test-only mutex because two tests write the same store path.
- `src/config/mod.rs`: one-line re-export of `AcceptedChars` (see deviation).
- `docs/arch/layer_exceptions.txt`: removed `cycle:brain_limits,config`; header and cycle comment updated to five remaining cycles.

`git grep -n "crate::brain_limits" -- src/config` is empty. No CLI, protocol, API or shell-policy behavior was changed; `Cargo.toml` and every `[profile.*]` setting are untouched.

## Guard delta (measured)

| Metric | Claim commit | After |
|---|---:|---:|
| modules | 87 | 87 |
| edges | 274 | 273 |
| cycles | 6 | 5 |
| upward edges | 32 | 32 |

Removed exception: `cycle:brain_limits,config`. No `edge:` line was added or removed.

## Test counts

| Run | Before | After |
|---|---:|---:|
| `cargo test --lib --quiet` | 1521 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |

The two added tests are `config::limits::tests::parameter_wird_genutzt_und_fallback_bleibt` and `brain_limits::tests::brain_id_wird_zum_config_parameter_aufgeloest`.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib --quiet`: 1523 passed / 0 failed / 1 ignored
- `cargo test --test arch_layers -- --nocapture`: 4 passed; `architecture guard: modules=87, edges=273, cycles=5, upward_edges=32`
- `cargo test --all-targets --quiet`: 1523 lib + 7 binary-unit + 4 integration passed
- `cargo test --locked --no-default-features --test arch_layers --quiet`: 4 passed
- `git diff --check`: 0

## Deviation

`src/config/mod.rs` is outside the board's two-file scope. The single added `AcceptedChars` re-export is required because `mod limits;` is private: the in-scope `impl AcceptedChars for &str` in `brain_limits.rs` must be able to name a trait from `config`. The alternative of duplicating the measurement-store parsing inside `config/limits.rs` would create a second reader of the same file and contradict the objective ("config::limits taking the value by parameter"); changing `controller.rs` would add a third out-of-scope file. This one-line re-export is the minimal deviation and changes no behavior.

## Binary smoke (blocked, not a regression)

`target\debug\webagent.exe --help` and `diagnose` both exit `-1073741515` (`0xC0000005`) before `main`, as on the earlier phase-16 branches. This was already present at the claim-time baseline; it is not caused by this refactor. No `[profile.*]` setting was touched.

## Boundaries

No login, live provider call, credentials, release, tag, deployment, PR or merge. T-1000 stays `claimed` until its branch is merged into `master`. T-1004 (crate-split decision) still needs explicit owner approval and was not touched.
