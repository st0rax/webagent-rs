# T-1002 proof: remaining TUI-side cycles

> **Referenz.** Task proof for T-1002. The live architecture ratchet remains
> `tests/arch_layers.rs`, `docs/arch/layers.toml` and
> `docs/arch/layer_exceptions.txt`.

**Claim.** Branch `refactor/T-1002-tui-cycles`, claim commit `06f5f2d`, owner `local/opencode`, 2026-11-26. The owner's standing "autonom weiterarbeiten" overrode the formal `depends_on: T-997` gate; T-997 stays `claimed` because it is not merged into `master`.

## Bestand zuerst geprüft

`git grep` found exactly one production edge from `tui_state` to `tui_load`: the `pub use crate::tui_load::load_state;` re-export at the top of `tui_state.rs`. Its only consumer was `tui.rs`. The other cycle (`bin_hooks, repl, tui, tui_ansi`) spans slash-command parsing, `PersistentQueryPool`, `run_tui`/`run_evolve` and the ANSI control helpers; no small move would break it.

## Changes

- `src/tui_state.rs`: removed the `load_state` re-export; the module doc now states that callers import it from `tui_load` directly. No state struct or behavior changed.
- `src/tui.rs`: imports `load_state` from `crate::tui_load` instead of `crate::tui_state`.
- `docs/arch/layer_exceptions.txt`: removed `cycle:tui_load,tui_state`; added a written justification for keeping `cycle:bin_hooks,repl,tui,tui_ansi` permanently (legacy TUI, `docs/WEB_UI_API_TOOL_RESET.md`, no behavior gain from a four-module extraction, no TUI feature work requested); header and cycle comment updated to three remaining cycles.

No other file was touched. No CLI, WEBAGENT/1 protocol, API or shell-policy behavior changed; `Cargo.toml` and every `[profile.*]` setting are untouched.

## Guard delta (measured)

| Metric | Claim commit | After |
|---|---:|---:|
| modules | 88 | 88 |
| edges | 274 | 274 |
| cycles | 4 | 3 |
| upward edges | 32 | 32 |

Removed exception: `cycle:tui_load,tui_state`. The `tui_state -> tui_load` edge was replaced by `tui -> tui_load`, so the edge count is unchanged. The remaining `bin_hooks, repl, tui, tui_ansi` exception is unchanged but now carries a written reason, as the task's DoD explicitly allows.

## Test counts

| Run | Before | After |
|---|---:|---:|
| `cargo test --lib --quiet` | 1523 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |

No tests were added or removed.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib --quiet`: 1523 passed / 0 failed / 1 ignored
- `cargo test --test arch_layers -- --nocapture`: 4 passed; `architecture guard: modules=88, edges=274, cycles=3, upward_edges=32`
- `cargo test --all-targets --quiet`: 1523 lib + 7 binary-unit + 4 integration passed
- `cargo test --locked --no-default-features --test arch_layers --quiet`: 4 passed
- `git diff --check`: 0

## Binary smoke (blocked, not a regression)

After `cargo build`, `target\\debug\\webagent.exe --help` and `diagnose` both exit `-1073741515` (`0xC0000005`) before `main`, as at the claim-time baseline and on the earlier phase-16 branches. No `[profile.*]` setting was touched.

## Boundaries

No login, live provider call, credentials, release, tag, deployment, PR or merge, and no TUI feature work. T-1002 stays `claimed` until its branch is merged into `master`. T-1004 (crate-split decision) still needs explicit owner approval and was not touched.
