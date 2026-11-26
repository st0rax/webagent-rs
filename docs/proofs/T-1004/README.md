# T-1004 proof: crate split decision (design only)

> **Referenz.** Task proof for T-1004. The decision document is
> `docs/arch/CRATE_SPLIT.md`; the live build remains the single `webagent`
> package with unchanged `Cargo.toml` and `[profile.*]` sections.

**Claim.** Branch `docs/T-1004-crate-split`, claim commit `f27928b`, owner
`local/opencode`, 2026-11-26. The owner phrase `mach fertig den senf` approved
completing this design document only; crate extraction still needs a new,
explicit approval. Formal `depends_on` (T-998, T-999, T-1000, T-1001, T-1003)
is satisfied in substance but those branches are still `claimed` because none
was merged, so T-1004 also stays `claimed`.

## Bestand zuerst geprüft

- No `[workspace]` section and no `crates/` directory existed; `Cargo.toml`
  defines exactly one package (`webagent`, lib + bin + `build.rs`).
- No `webagent-core`, `webagent-transport` or `webagent-api` manifest existed.
- `[profile.release]`, `[profile.dev]` and `[profile.test]` are unchanged;
  `codegen-units = 16` is the load-bearing loader fix.
- `tests/arch_layers.rs` already provides the production-code graph sanitizer
  and the ratchet. It measured 88 modules / 274 edges / 3 cycles /
  32 upward edges at the claim commit.
- `src/relay.rs` already has a private `RelayBackend` trait and two direct
  `WebBrainBackend::from_config` calls. `ClientPresence` is defined in
  `src/api_bridge/transport.rs` but consumed by `relay.rs` and
  `browser_inference.rs`. No factory trait existed.
- `selectors` is not a top-level module: `src/browser/selectors.rs` belongs to
  transport, `src/config/selectors.rs` to core.

## Delivered

- `docs/arch/CRATE_SPLIT.md`: re-measured baseline, naive and recommended
  group graphs, per-crate module/Cargo trees, seven trait boundaries, the
  workspace-root profile policy, an ordered nine-step follow-up proposal and
  an explicit no-go list.
- This proof. No `src/`, test, `Cargo.toml`, `Cargo.lock` or profile file was
  touched; no task ID was created and no board task was set to `done`.

## Measurements (re-measured at claim commit)

| Measure | Before | After |
|---|---:|---:|
| `cargo test --lib --quiet` | 1523 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |
| Clean lib test build+link | 65.98 s, 162,341,370-byte test exe | not re-run; docs-only change |
| Clean default binary build+link | 40.47 s, 150,514,926-byte exe | not re-run; docs-only change |
| Architecture guard | 88 modules / 274 edges / 3 cycles / 32 upward edges | unchanged |
| Direct Cargo dependencies | 21 | unchanged |

The clean-build numbers use `cargo clean -p webagent` before each timed build,
then a PowerShell `Stopwatch` around `cargo test --lib --no-run
--message-format=json` and `cargo build --message-format=json`, with the
executable path and size read from the compiler artifact message. They are a
baseline for the later extraction, not a claimed improvement.

The module graph was re-derived outside the repository with the same sanitizer
as `tests/arch_layers.rs`; the scratch program lived under `%TEMP%\opencode\`.
The recommended cut has 34 core, 9 transport, 9 api and 36 root modules, with
six cross-crate blockers listed in `CRATE_SPLIT.md` §3.2.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib --quiet`: 1523 passed / 0 failed / 1 ignored
- `cargo test --test arch_layers -- --nocapture`: 4 passed / 0 failed, guard
  `modules=88, edges=274, cycles=3, upward_edges=32`
- `cargo test --all-targets --quiet`: 1523 lib + 7 binary-unit + 4 integration passed
- `cargo test --lib startup::tests::betriebs_markdown_hat_eine_wahrheit -- --exact`: 1 passed
- `git diff --check`: 0

## Binary smoke

`target\debug\webagent.exe --help` exits `-1073741515` (`0xC0000005`) with an
empty output at the claim commit, exactly as in T-1000..T-1003. This is a
pre-existing host/loader problem, not a regression; no binary-affecting change
was made here. The browser-free `--help` path remains verified by T-1003.

## Boundaries

No login, credentials, profile/token read, live provider call, crate move,
profile change, release, tag, deployment, PR, force-push or merge. The owner
must approve extraction separately; until then the single package is the
product and every phase-16 branch stays `claimed` until merged.
