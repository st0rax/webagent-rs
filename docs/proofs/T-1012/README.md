# T-1012 proof: browser/transport modules extracted into `webagent-transport`

> **Referenz.** Task proof for T-1012. This slice moves the browser, page and
> panel modules into the workspace member created by T-1010. It does not merge
> a branch or publish a release.

**Claim.** Branch `refactor/T-1012-transport-extraction`, claim commit `3c4388d`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. It stays `claimed` until its branch is merged.

## Bestand zuerst geprüft

- T-1011 had moved 34 platform-neutral modules into `webagent-core` and left
  three empty workspace members. Transport candidates were enumerated with
  `rg`: `browser` (11 files), `mock_page`, `page_driver`, `webview_runtime`
  (plus two child files), `webview_reveal`, `upload_spa_js`, `browser_pool`,
  `login`, `brain_probe` and `panel_sink`.
- Non-candidate root users were inventoried before the move: `browser` is used
  by `autoresearch`, `benchmark/pipeline`, `bot2bot_worker`, `controller`,
  `repl`, `tui`, `web_ui_api`, `welcome` and `lib.rs`; `browser_pool` by `tui`,
  `tui_ansi` and `web_ui_api`; `login` by `repl`; `panel_sink` by `brain_grid`.
  All were preserved by root re-exports, so no caller path changed.
- Outgoing `crate::` references from the candidates pointed only at core
  modules (`brain`, `config`, `protocol`, `capability`, `timeouts`,
  `bench_events`, `brain_score`, `circuit_breaker`, `observer`, helpers, ...).
  They were rewritten to explicit `webagent_core::` paths; no candidate needed
  a root-only module. The concrete `WebBackendFactory` marker moved with its
  `BackendFactory` impl to keep the orphan rule satisfied.
- No second proof store was created; `capability_proof` stays the one gate.

## Delivered

- 22 source files moved with `git mv`: the `src/browser/` directory (11 files),
  nine top-level modules (`brain_probe`, `browser_pool`, `login`, `mock_page`,
  `page_driver`, `panel_sink`, `upload_spa_js`, `webview_reveal`,
  `webview_runtime`) and the two `src/webview_runtime/` child files
  (`file_input.rs`, `tests.rs`).
- `crates/webagent-transport/src/lib.rs` declares the ten top-level modules with
  the original `cfg` gates (`upload_spa_js` behind `any(feature = "webview",
  test)`, `webview_runtime` behind `feature = "webview"`) and re-exports
  `WebBackendFactory` from `browser::backend.rs`. Root `src/lib.rs` no longer
  declares those modules and re-exports the crate with
  `pub use webagent_transport::*;`, so existing `crate::<module>` paths in the
  binary and tests still resolve.
- A compile-driven rewrite changed `crate::<core>` to `webagent_core::<core>`
  in 13 transport files. `browser::has_protocol_payload` was widened from
  `pub(crate)` to `pub` because root `controller.rs` still calls it through the
  re-export. The `brain_probe` selector-file test now asks
  `webagent_core::config::selectors_dir()` for the asset path instead of
  deriving it from the transport manifest directory.
- `panel_sink` geometry helpers (`right`, `bottom`, `overlaps`, `contained_in`)
  moved from `cfg(test)` to `cfg(any(test, feature = "test-support"))`. The root
  enables that feature as a dev-dependency; production builds gain no code.
- Transport manifest declares the existing dependency versions (`serde`,
  `serde_json`, `time`, `regex`, `fancy-regex`, `lazy_static` and the optional
  GUI crates) and `webview` plus `test-support` features. The root `webview`
  feature forwards to `webagent-transport/webview`; `--no-default-features`
  still excludes the GUI crates. All crate versions remain `0.11.8`.
- `Cargo.lock` was regenerated offline for the new member dependencies.
  `docs/arch/CRATE_SPLIT.md` and `docs/arch/layer_exceptions.txt` record the
  measured state; `docs/arch/layers.toml` and the exception lines did not need
  an edit.

## Gates measured

- `cargo metadata --no-deps --format-version 1` lists all four packages.
- `cargo fmt --all -- --check` — 0 after formatting the rewritten imports and
  call sites.
- `cargo clippy --workspace --features webview --all-targets -- -D warnings` — 0.
- Unit tests: claim-time (before the move) 924 root + 623 core = 1547 passed /
  0 failed / 1 ignored. Final `cargo test --workspace --lib --quiet`: 647 root
  + 623 core + 277 transport = 1547 passed / 0 failed / 1 ignored. The 277
  tests moved with their modules into the transport crate; the total is
  unchanged.
- `cargo test --test arch_layers -- --nocapture` — 5 passed; guard printed
  `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`,
  identical to the claim baseline. No exception line was added or removed;
  `ratchet_rejects_api_to_transport_edge` still passes.
- `cargo test --workspace --all-targets --quiet` — 647 root lib + 7 bin-unit +
  5 integration + 623 core lib + 277 transport lib passed, 0 failed.
- `cargo test --workspace --locked --no-default-features --quiet` — 647 root
  lib + 7 bin-unit + 5 integration + 623 core lib + 271 transport lib passed,
  0 failed / 1 ignored. The six GUI-gated transport tests drop out as intended.
- `cargo check --workspace --offline` — 0.
- Debug smoke: `cargo build --locked` 0, `WebView2Loader.dll` present,
  `--version` printed `webagent 0.11.8 (3c4388d0f+dirty)` and `--help` printed
  usage; both exit 0.
- Release smoke: `cargo build --release --locked` 0 in 2m 45s,
  `WebView2Loader.dll` present, `--version` printed
  `webagent 0.11.8 (3c4388d0f+dirty)` and `--help` printed usage; both exit 0.
  The `+dirty` suffix is expected: the binaries were built from the uncommitted
  extraction tree before this slice was committed.
- `git diff --check` — 0 (LF→CRLF working-copy warnings only).

## Grenze / offen

No api module moved, no dependency version changed, no merge, PR, tag, release
or deployment made by this slice. No credentials were read or used, no provider
/login call was made and no live browser session was started. The task remains
`claimed` until its branch is merged. Next slice: T-1013 moves the api modules
into `webagent-api`.
