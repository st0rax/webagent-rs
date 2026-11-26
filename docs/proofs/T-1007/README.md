# T-1007 proof: PanelSink boundary

> **Referenz.** Task proof for T-1007. This is a refactor inside the single
> `webagent` package: no crate extraction, no merge, and no change to
> `Cargo.toml`, `[profile.*]` or dependencies.

**Claim.** Branch `refactor/T-1007-panel-sink`, claim commit `6fec681`, owner
`local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. T-1007 stays `claimed`; no `done`, no PR and no merge.

## Bestand zuerst geprüft

- Top-level module count and graph: 89 modules / 275 `crate::<module>` edges /
  3 cycles / 29 upward edges (measured at the claim commit, not quoted).
- The three transport hooks were `browser_pool.rs` (`fitting_tile_count`,
  `grid_layout`, `Rect` in `arrange_brain_grid`), `webview_reveal.rs`
  (`Rect` import plus `primary_work_area()` in `reveal_rect()`) and
  `webview_runtime.rs` (`Rect` in three runtime messages plus
  `terminal_window_hwnd()` in `focus_terminal_window()`).
- `Rect` and all pure grid geometry lived in `src/brain_grid.rs` (rank 8), so
  every transport module had to reach into the application layer. Old public
  paths such as `webagent::brain_grid::Rect` are used by `brain_wall` and
  external callers, so they had to keep resolving.
- No `src/panel_sink.rs` and no desktop-fact trait existed. `main.rs` had a
  single `run()` entry point before any command dispatch, which is the natural
  installation point for an application-provided sink.

## Delivered

- New dependency-free `src/panel_sink.rs` (L0, rank 1) owns `Rect`, the pure
  layout helpers (`grid_dimensions`, `grid_layout`, `split_areas`,
  `fitting_tile_count`, `reveal_rect_for_work_area`), the `PanelSink` trait
  (`work_area`, `terminal_window_hwnd`), `NoPanelSink`, and a one-shot
  `install_panel_sink`/`panel_sink` accessor pair.
- `brain_grid` re-exports the moved geometry so `brain_wall`, its own tests and
  external users keep the old paths. It now defines `DesktopPanelSink`, which
  implements `PanelSink` using the unchanged Win32 work-area and terminal-window
  searches; `main.rs` installs it before dispatch.
- `browser_pool`, `webview_reveal` and `webview_runtime` import geometry and
  desktop facts from `panel_sink`. No `crate::brain_grid` reference remains in
  any of the three modules. Parking, offscreen positions, docking, TOPMOST
  handling and focus calls are unchanged; only the source of two read-only
  facts moved behind the trait.
- Without installation (library users and unit tests) `panel_sink()` returns
  `NoPanelSink`, so `reveal_rect()` uses the historical 80/80/1280×900 fallback
  even when a work area exists. The `webagent` binary installs the desktop sink
  at startup, so production reveal geometry is unchanged. This is the explicit
  trade-off for keeping Win32 out of the transport boundary.
- `docs/arch/CRATE_SPLIT.md` marks the `PanelSink`/`WindowLayout` follow-up as
  done in the current crate. No file was moved to another crate.

## Guard delta and ratchet edits

- Deleted `edge:browser_pool->brain_grid`, `edge:webview_reveal->brain_grid`
  and `edge:webview_runtime->brain_grid` only after the guard reported them
  stale. No exception was added.
- Added `panel_sink` to L0 in `docs/arch/layers.toml`; all new edges point
  downward (transport/rank 2–3 and root/rank 8 into `panel_sink`/rank 1).
  The three-module cycle `brain_probe,browser,browser_pool` is unchanged.
- Measured after: 90 modules, 276 edges, 3 cycles, **26 upward edges**
  (claim: 89 / 275 / 3 / 29).

## Measurements (re-measured, not quoted)

| Measurement | Claim | Final |
|---|---:|---:|
| `cargo test --lib --quiet` total | 1528 | 1531 |
| passed / failed / ignored | 1527 / 0 / 1 | 1530 / 0 / 1 |
| lib test time | 50.71 s | 52.40 s |
| arch guard modules / edges / cycles / upward | 89 / 275 / 3 / 29 | 90 / 276 / 3 / 26 |
| `cargo test --all-targets --quiet` | not run at claim | 1530 lib + 7 binary-unit + 4 integration, 0 failed |

Three lib tests were added in `panel_sink` (fallback facts, centered/clamped
reveal rectangle, no-work-area fallback). No existing test was weakened or
removed.

## Gates measured

- `cargo fmt --all -- --check` — 0.
- `cargo clippy --features webview --all-targets -- -D warnings` — 0.
- `cargo test --lib --quiet` — 1530 passed / 0 failed / 1 ignored.
- `cargo test --test arch_layers -- --nocapture` — 4 passed; guard printed
  `modules=90, edges=276, cycles=3, upward_edges=26`.
- `cargo test --all-targets --quiet` — 1530 lib + 7 binary-unit + 4 integration
  passed, 0 failed.
- `cargo test --locked --no-default-features --quiet` — final run 1524 lib
  (1 ignored) + 7 binary-unit + 4 integration passed, 0 failed. See the flake
  note below.
- `git diff --check` — 0 (only LF→CRLF working-copy warnings on edited files).

### Flaky browser-free HTTP test (honest limit)

Two parallel `--no-default-features` runs on this branch failed once each with
`ConnectionReset` at `src/web_ui.rs:303`, in two different one-shot tests
(`health_mit_api_rolle_liefert_bridge_health`,
`api_rolle_zugriff_mit_token_liefert_modellliste`). The failing helper is in
`web_ui.rs`, which T-1007 did not touch. The isolated retry passed, a full
serial run (`-- --test-threads=1`) passed, and a third parallel run passed with
1524 / 0 / 1. A clean detached worktree at claim commit `6fec681` also passed
in parallel (1521 / 0 / 1). The failure is therefore intermittent and not
reproducible as a regression; it is documented rather than hidden.

## Binary smoke (blocked, not a regression)

After the binary-affecting `main.rs` change, `cargo build` succeeded, but
`target/debug/webagent.exe --help` and `diagnose --brain deepseek --headless`
both exit `-1073741515` (`0xC0000005`) before `main`, exactly as in the T-1004
and T-1006 proofs. No `[profile.*]` line and no dependency was changed, so the
pre-existing loader issue remains the limit on live binary smoke.

## Grenze / offen

No workspace or crate was created, no file was moved to another crate, no
branch was merged, and no PR, tag, release or deployment was made. No
credentials were read or used, no provider/login call was made, and no live
browser session was started. Crate extraction still requires a fresh, explicit
owner approval; this task stops at the prepared transport boundary.
