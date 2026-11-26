# T-1011 proof: core modules extracted into `webagent-core`

> **Referenz.** Task proof for T-1011. This slice moves platform-neutral code
> into the workspace member created by T-1010. It does not merge a branch or
> publish a release.

**Claim.** Branch `refactor/T-1011-core-extraction`, claim commit `6e421e6`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. It stays `claimed` until its branch is merged.

## Bestand zuerst geprüft

- The T-1010 workspace skeleton had three empty library members; no production
  module had moved. The architecture guard discovered modules from all
  workspace `lib.rs` files and enforced `crate_violations=0`.
- The 35 candidate modules were checked for outgoing `crate::` references to
  non-core modules. Three hits (`brain_probe`, `session`, `tui`) were doc
  comments only; they were rewritten as plain code spans so core rustdoc has no
  broken intra-doc links.
- `counting.rs` was the only real blocker: it calls
  `WebBackendFactory::create_brain_backend`, and the `BackendFactory` impl is
  in root `browser/backend.rs`. Moving it would introduce a forbidden core→root
  dependency. It therefore stays in the root crate.
- Selector JSON files are compile-time assets of `config/selectors.rs`; they
  moved with their consumer. No second proof store was created; `capability_proof`
  remains the single gate.

## Delivered

- 34 modules moved with `git mv` into `crates/webagent-core/src/`: acceptance,
  bench_events, brain, brain_limits, brain_score, capability,
  capability_proof, capability_types, circuit_breaker, code_score, config,
  contract, doctor, executor, file_actions, goal_plan, inference_types,
  loop_guard, network_observation, observer, pool_failover, pool_state,
  prompts, protocol, run_store, scoring, shell_policy, taskboard, timeouts,
  tools, transcript, watchdog, wiki_memory, worker_pool.
- The shared root `lib.rs` helper block moved to
  `crates/webagent-core/src/support.rs` (new top-level module at layer rank 0).
  It is re-exported from the core crate with `pub use support::*;`, so existing
  `crate::<helper>` paths keep resolving; `timestamp()` became `pub` because
  root modules call it.
- Root `src/lib.rs` keeps only the non-core modules and re-exports the core
  crate with `pub use webagent_core::*;`. `counting` stays in root with an
  explanatory comment; `WebBackendFactory` is re-exported at the root so
  callers need no transport-layer path (which would have added a new upward
  `counting → browser` edge to the ratchet).
- Orphan-rule fix: the concrete `WebBackendFactory` marker moved from core
  `brain.rs` to root `browser/backend.rs`, next to the `BackendFactory` impl.
  The trait stays in core. `counting.rs` and `relay.rs` import the marker from
  the root re-export.
- `selectors/*.json` moved to `crates/webagent-core/selectors/` together with
  `config/selectors.rs`; `include_str!` paths are unchanged.
- Compile-driven visibility widenings across the new crate boundary:
  `inference_types` (`TOOL_ENVELOPE`, `model_output_error`,
  `DSML_TOOL_FORMAT_ERROR`, `DSML_MARKERS`, `MAX_TOOL_SCHEMA_BYTES`,
  `TOOL_DESCRIPTION_LIMITS`), `circuit_breaker::contains_at_word_boundary`,
  `protocol` (`has_protocol_payload`, `BLOCK_PHRASES`, `BLOCK_BANNER_MAX_CHARS`,
  `block_phrase_in_text`), `capability_proof::record_from_measurement`,
  `config::prepare_shared_profile_for_clone` + `WriteBackLock`, and
  `worker_pool::atomic_write`.
- Core manifest declares the existing dependency versions (`serde`,
  `serde_json`, `sha2`, `regex`, `fancy-regex`, `lazy_static`, `fs2`, `time`,
  `windows-sys`, optional `windows`) and `webview`, `tui`, `test-support`
  features. Root forwards `webview`/`tui` and enables `test-support` as a
  dev-dependency for `bench_events::test_bus_mutex`. All three crate versions
  are aligned to the released `0.11.8`.
- Three root tests that read source/data files by path were updated to the
  new locations; one core test that reads `docs/TASKBOARD.json` now resolves
  the workspace root via `../..`. A fourth test
  (`benchmark::tests::gliederung_ersetzt_das_scheibchenweise_lesen`) had an
  early return when its old `src/protocol.rs` path vanished; it now points at
  `crates/webagent-core/src/protocol/mod.rs` and actually asserts again.
- Path semantics were preserved across the crate boundary: `config::root_dir()`
  resolves two levels above the core manifest (still the workspace root), while
  `config::selectors_dir()` is now crate-local
  (`crates/webagent-core/selectors/`). Without this split, moving `config`
  would have silently changed the dev fallback for `bot2bot_root()`, `src_dir()`
  and `webagent_root_stable()`.
- `docs/arch/layers.toml` gained `support` at rank 0;
  `docs/arch/layer_exceptions.txt` and `docs/arch/CRATE_SPLIT.md` record the
  measured state and the `counting` deviation. No exception line was added or
  removed.

## Gates measured

- `cargo metadata --no-deps --format-version 1` lists all four packages.
- `cargo fmt --all -- --check` — 0 after formatting the changed imports.
- `cargo clippy --workspace --features webview --all-targets -- -D warnings` — 0.
- Unit tests: claim-time (before the move, root only) 1547 passed / 0 failed /
  1 ignored. Final `cargo test --workspace --lib --quiet`: 924 root + 623 core
  = 1547 passed / 0 failed / 1 ignored. The split is expected: unit tests
  moved with their modules into the core crate; the total is unchanged.
- `cargo test --test arch_layers -- --nocapture` — 5 passed; guard printed
  `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`.
  The one new edge is `support → bench_events` (both rank 0).
- `cargo test --workspace --all-targets --quiet` — 924 root lib + 7 bin-unit +
  5 integration + 623 core lib passed, 0 failed.
- `cargo test --workspace --locked --no-default-features --quiet` — 918 root
  lib + 7 bin-unit + 5 integration + 623 core lib passed, 0 failed / 1 ignored.
  `Cargo.lock` was regenerated offline for the new member dependencies.
- Debug smoke: `cargo build --locked` 0, `WebView2Loader.dll` present,
  `--version` and `--help` exit 0.
- Release smoke on the final tree (after the path-semantics fix):
  `cargo build --release --locked` 0 in 3m 07s, `WebView2Loader.dll` present,
  `target/release/webagent.exe --version` printed
  `webagent 0.11.8 (6e421e612+dirty)` and exited 0, `--help` printed usage and
  exited 0. The `+dirty` suffix is expected: the binary was built from the
  uncommitted extraction tree before this slice was committed.
- `git diff --check` — 0 (LF→CRLF working-copy warnings only).

## Grenze / offen

No transport or api module moved, no dependency version changed, no merge, PR,
tag, release or deployment made by this slice. No credentials were read or
used, no provider/login call was made and no live browser session was started.
The task remains `claimed` until its branch is merged. Next slice: T-1012
moves the transport/browser modules into `webagent-transport`.
