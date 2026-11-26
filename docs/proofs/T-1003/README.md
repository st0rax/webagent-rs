# T-1003 proof: Windows GUI dependencies behind `webview`

> **Referenz.** Task proof for T-1003. The live configuration is `Cargo.toml`
> and `Cargo.lock`; the architecture ratchet is unchanged.

**Claim.** Branch `chore/T-1003-feature-hygiene`, claim commit `a76d79d`, owner `local/opencode`, 2026-11-26. The branch was renamed from `build/…` before the claim commit because `taskboard::tests::taskboard_invarianten_halten` and `scripts/commit-as-agent.ps1` only allow `feature|fix|docs|chore|refactor|test`. The owner's standing "autonom weiterarbeiten" overrode the formal `depends_on: T-997` gate.

## Bestand zuerst geprüft

`git grep` found `windows::` (the `windows` 0.58 crate) only behind `cfg(all(windows, feature = "webview"))`: `brain_grid.rs`, `brain_wall.rs`, `worker_pool.rs` (JobObjects), `lib.rs` (console handle), `main.rs`, `tui.rs` and `webview_runtime*`. JobObjects and ToolHelp are therefore *not* needed by the browser-free build, despite the board's caution. `windows-sys` 0.59 is used by non-GUI code (`config/profiles.rs` PID liveness, `config/writeback.rs`), so it stays non-optional; it moved from `[dependencies]` into `[target.'cfg(windows)'.dependencies]` because its callers are Windows-only.

## Changes

- `Cargo.toml`: `webview2-com`, `windows`, `arboard`, `clipboard-win` and `image` are now `optional = true` in the Windows target table and listed as `dep:` entries of the `webview` feature. `windows-sys` moved to the Windows target table, still non-optional. Comments updated.
- No `[profile.*]` line was changed: `codegen-units = 16` in `release`, `dev` and `test` remains exactly as before.
- `src/lib.rs` needed no change: `webview_runtime` was already gated by `#[cfg(feature = "webview")]`.
- `Cargo.lock` needed no change and is untouched.

## `cargo tree --no-default-features -e normal --depth 1` (Windows host)

Before: `arboard`, `clipboard-win`, `image`, `webview2-com`, `windows`, `windows-sys` were all present.

After: only core crates plus `windows-sys v0.59.0` remain; none of `arboard`, `clipboard-win`, `image`, `webview2-com`, `windows`, `wry`, `tao` is listed. With `--target x86_64-unknown-linux-gnu` the no-default tree has no `windows-sys` either.

## Link time / binary size (measured)

`cargo clean -p webagent` before each build, then `cargo test --lib --no-default-features --no-run --message-format=json`; wall clock from a PowerShell `Stopwatch`, size from the reported executable.

| Metric | Before | After |
|---|---:|---:|
| wall clock, clean lib test build+link | 46.69 s | 47.66 s |
| lib test executable | 95,086,256 bytes | 94,481,201 bytes |

The binary shrank by 605,055 bytes (~0.6 MB). Link time did not improve measurably; the ~1 s difference is within run-to-run noise and is reported as such, not claimed as a win. `cargo clean -p webagent` removed 340 files / 1.7 GiB before and 43 files / 332.8 MiB after, showing that the GUI dependency artifacts are no longer rebuilt.

## Test counts

| Run | Before | After |
|---|---:|---:|
| `cargo test --lib` (default features) | 1523 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |
| `cargo test --locked --no-default-features --quiet` | not measured before | 1517 lib + 7 bin-unit + 4 integration + 0 doc passed / 0 failed / 1 ignored |

The lower no-default count is the expected set of `cfg(feature = "webview")`-gated tests; no test was deleted.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib`: 1523 passed / 0 failed / 1 ignored
- `cargo test --all-targets --quiet`: 1523 lib + 7 binary-unit + 4 integration passed
- `cargo test --locked --no-default-features --quiet`: 1517 lib + 7 binary-unit + 4 integration + 0 doc passed
- `git diff --check`: 0
- `cargo build` (default): 0, binary 150,514,926 bytes (unchanged size)
- `cargo build --no-default-features`: 0, binary 82,625,882 bytes

## Binary smoke

- Default (`webview`) binary: `--help` and `diagnose --brain deepseek --headless` both exit `-1073741515` (`0xC0000005`) before `main`, exactly as on the claim-time baseline. The compiler cannot catch this and the `[profile.*]` setting that avoids it was not touched. This part of the DoD is therefore **blocked, not a regression**.
- Browser-free binary: `--help` exits **0**; `diagnose --brain deepseek --headless` exits **1** with `[diagnose] deepseek: Fehler: page-unavailable: WebView-Feature nicht aktiviert — mit --features webview bauen`. That is the intended behavior without a browser backend, not a crash.

## Linux build limit

`cargo check --target x86_64-unknown-linux-gnu --no-default-features` fails in a dependency's build script with `failed to find tool "x86_64-linux-gnu-gcc"`. The target std is installed, but no cross C compiler is present on this Windows host, so a Linux compile cannot be verified here. The Linux dependency tree was verified instead (see above); no Linux-specific source was changed.

## Boundaries

No login, live provider call, credentials, release, tag, deployment, PR or merge. T-1003 stays `claimed` until its branch is merged into `master`. T-1004 (crate-split decision) still needs explicit owner approval and was not touched.
