# T-1010 proof: workspace skeleton and multi-crate architecture guard

> **Referenz.** Task proof for T-1010. This slice adds only empty library
> members and guard coverage; no production module is moved and no merge is
> made. `[profile.*]` values and dependency versions are unchanged.

**Claim.** Branch `refactor/T-1010-workspace-guard`, claim commit `142f964`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. T-1010 stays `claimed`; no `done`, no PR and no merge.

## Bestand zuerst geprüft

- The single-root guard at the claim commit measured 90 modules / 275
  `crate::<module>` edges / 3 cycles / 26 upward edges and had no concept of
  crate ownership. `tests/arch_layers.rs` used only `CARGO_MANIFEST_DIR`, read
  only the root `src/lib.rs`, scanned only `src/<module>.rs` and
  `src/<module>/**`, and counted only `crate::<module>` paths. Its coverage
  assertion (`modules.len() >= 80`) would have gone silently satisfied if
  modules were moved out of the root without extending it.
- `Cargo.toml` was a single package with no `[workspace]`. All 90 modules lived
  in the root `src/`, so the conceptual layer ratchet and the crate-direction
  invariant are best introduced before any file moves.
- The one wrong-direction edge from the original crate-split plan
  (`relay → browser`) had already been removed by T-1009, so the upgraded guard
  could start from `crate_violations=0` without adding an exception.

## Delivered

- `Cargo.toml` gains a workspace root with `members = [".",
  "crates/webagent-core", "crates/webagent-transport", "crates/webagent-api"]`,
  `resolver = "2"` and `default-members = ["."]`. One shared `Cargo.lock` and
  one `target/`; the root keeps `[lib]`, `[[bin]]`, `build.rs`, the optional TUI
  and GUI dependencies and all `[profile.*]` values.
- Three empty library members with their own `Cargo.toml` and `src/lib.rs`.
  They are declared in the workspace but not yet added as root dependencies,
  so `cargo clippy -D warnings` does not report unused path dependencies.
  Their manifests contain no `[profile.*]` section (member profiles are ignored
  by Cargo and would silently drop the root loader fix).
- The two load-bearing `[profile.*]` comment blocks that still claimed a
  `0xC0000005` loader threshold were corrected to the measured `0xC0000135`
  (`STATUS_DLL_NOT_FOUND`, fixed by the root `build.rs` in T-1008); values were
  not changed.
- `tests/arch_layers.rs` now:
  1. discovers top-level modules from every workspace `lib.rs`
     (`discover_modules` / `crate_roots`) and records each module's owning crate;
  2. counts `crate::<module>` inside the owning crate plus explicit
     `webagent_core::`, `webagent_transport::` and `webagent_api::` paths
     (`module_edges`), so cross-crate calls remain visible after a move;
  3. enforces an explicit crate-pair matrix (`allowed_crate_pair` /
     `check_crate_direction`): core has no library outgoing edge, transport and
     api are siblings that may only reach core, and only root may reach all
     three;
  4. keeps the existing conceptual layer, cycle, upward-edge and stale-exception
     checks unchanged.
- A new negative test `ratchet_rejects_api_to_transport_edge` proves an
  `api → transport` edge is rejected and can never be excepted. The baseline
  comment in `docs/arch/layer_exceptions.txt` records the new invariant; no
  exception line was added or removed.

## Gates measured

- `cargo metadata --no-deps --format-version 1` — 0; lists `webagent`,
  `webagent-core`, `webagent-transport`, `webagent-api`.
- `cargo fmt --all -- --check` — 0 after formatting the upgraded guard.
- `cargo clippy --features webview --all-targets -- -D warnings` — 0 (after
  dropping the not-yet-used root path dependencies; they are re-added with the
  first real module move in T-1011).
- `cargo test --lib --quiet` — claim commit `142f964`: 1531 passed / 0 failed /
  1 ignored; final: 1531 passed / 0 failed / 1 ignored (52.06 s), unchanged.
  No production module moved in this slice.
- `cargo test --test arch_layers -- --nocapture` — 5 passed; guard printed
  `modules=90, edges=275, cycles=3, upward_edges=26, crate_violations=0`.
- `cargo test --all-targets --quiet` — 1531 lib + 7 bin-unit + 5 integration
  passed, 0 failed. Integration rose from 4 to 5 with the new negative test.
- `cargo test --locked --no-default-features --quiet` — 1525 lib + 7 bin-unit +
  5 integration passed, 0 failed / 1 ignored.
- Debug and release smoke with the copied DLL deleted first: `cargo build
  --locked` and `cargo build --release --locked` each recreated
  `WebView2Loader.dll`; `--version` and `--help` exited 0 in both profiles.
- `git diff --check` — 0 (only LF→CRLF working-copy warnings on edited files).

## Post-merge re-measurement (integration with released v0.11.8)

While this slice was being prepared, another session merged phase 16 through
T-1009 into `origin/master` and released v0.11.8 (`fb7b886`), explicitly leaving
crate extraction to this session. `origin/master` was merged into
`refactor/T-1010-workspace-guard` as a normal merge commit (no force-push, no
history rewrite). The merge kept version `0.11.8`, the integrator's `done`
statuses for T-997..T-1009 and its T-976/T-977 board entries, plus this
slice's workspace skeleton, guard upgrade, T-1010 claim and proof. Board
conflicts were resolved in favour of the released board; `docs/TASKBOARD.json`
parses with 116 tasks.

Re-measured on the merged tree: `cargo fmt --all -- --check` 0; `cargo clippy
--features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet`
1547 passed / 0 failed / 1 ignored (was 1531 before the master merge — the
increase comes from the released Play/T-976/T-977 tests, not from this slice);
`cargo test --test arch_layers -- --nocapture` 5 passed with `modules=90,
edges=277, cycles=3, upward_edges=26, crate_violations=0` (two new edges came
in with the released code, no new exception); `cargo test --all-targets --quiet`
1547 lib + 7 bin-unit + 5 integration; `cargo test --locked
--no-default-features --quiet` 1541 lib + 7 bin-unit + 5 integration;
`git diff --check` 0.

## Grenze / offen

No production module was moved and no dependency version changed by this
slice. `origin/master` was merged *into* this branch to catch up with the
v0.11.8 release; this branch was not merged into master, and no PR, new tag,
new release or deployment was made by this slice. No credentials were read or used, no
provider/login call was made and no live browser session was started. The task
remains `claimed` until its branch is merged. Next slice: T-1011 moves the core
modules into `webagent-core` with root re-exports, keeping the guard green.
