# T-1013 proof: API/relay modules extracted into `webagent-api`

> **Referenz.** Task proof for T-1013. This slice moves the local bridge, relay
> and session modules into the workspace member created by T-1010. It does not
> merge a branch or publish a release.

**Claim.** Branch `refactor/T-1013-api-extraction`, claim commit `4fc4e1e`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. It stays `claimed` until its branch is merged.

## Bestand zuerst geprüft

- T-1012 had left three library members with core and transport populated, and
  `webagent-api` still empty. The nine candidate modules named in
  `docs/arch/CRATE_SPLIT.md` §4.3 were enumerated: 23 source files
  (`api_bridge.rs` + 12 children, `session/` with 3 files, plus
  `browser_inference`, `comms`, `free_cloud_chat`, `https_client`, `oobe`,
  `relay` and `source_scope`).
- Outgoing `crate::` references from the candidates pointed only at core
  modules (`brain`, `config`, `protocol`, `timeouts`, `bench_events`,
  `brain_score`, `capability`, `capability_proof`, `circuit_breaker`,
  `observer`, `inference_types`, `now_rfc3339`, `now_run_stamp`). No candidate
  needed a root-only module.
- One real blocker existed: `relay.rs` called the concrete `WebBackendFactory`
  from transport. Since the architecture guard rejects `api → transport`, this
  could not be carried over by a root re-export. The fix is a core-side
  provider installed by the binary, not a new crate dependency or exception.
- No second proof store was created; `capability_proof` stays the one gate.

## Delivered

- 23 source files moved with `git mv` into `crates/webagent-api/src/`. The crate
  declares the nine top-level modules in its `lib.rs`; root `src/lib.rs` no
  longer declares them and re-exports `webagent_api::*` alongside core and
  transport, so existing `crate::<module>` paths in the binary and tests still
  resolve.
- `crates/webagent-api/Cargo.toml` declares the dependencies the moved code
  already used: `webagent-core`, `serde`, `serde_json`, `time`, `tokio`,
  `tokio-rustls`, `rustls` and `webpki-roots`. No GUI crate, no `clap`, no
  version change; all crate versions stay `0.11.8`. `Cargo.lock` was
  regenerated offline for the new member.
- Compile-driven rewrite changed `crate::<core>` to `webagent_core::<core>` in
  14 api files. Items previously `pub(crate)` in root `api_bridge.rs` had to be
  widened to `pub` because the root UI still calls them through the re-export:
  `api_error`, `read_http_request`, `write_http_response`, `ConnectionLimiter`,
  `try_acquire`, `ConnectionPermit`, `ApiFlavor`, the `method`/`path`/`query`/
  `body` fields of `HttpRequest`, `validate_bridge_config`, `HttpResponse`,
  `overload_response` and `route_request`. No logic changed.
- The `t404_sdk_blackbox_official_sdks_and_two_clients` test failed after the
  move with `python nicht startbar: Der Verzeichnisname ist ungültig. (os
  error 267)`: `CARGO_MANIFEST_DIR/scripts/t404` is crate-local, but the SDK
  scripts live in the repository root. `t404_script_dir()` now joins
  `../../scripts/t404` and the test passes again.
- New `crates/webagent-core/src/backend_factory.rs` holds a process-wide
  `OnceLock` of three constructor function pointers (`brain`, `relay`, `image`).
  `BackendFactory` has associated functions and is therefore not dyn-compatible;
  storing `fn` pointers keeps the trait unchanged and avoids a `dyn` object.
  `install::<F>()` is single-assignment, accessors return an explicit error
  before installation. The binary installs
  `webagent::backend_factory::install::<webagent::WebBackendFactory>()` once in
  `run()`, so `relay` uses the real browser factory without naming transport.
  Two unit tests cover single-assignment and the error path.
- `docs/arch/layers.toml` places `backend_factory` at L0 rank 1 next to `brain`.
  Rank 0 was measured and rejected by the guard:
  `new upward edge backend_factory (rank 0) -> brain (rank 1)`. No exception
  line was added or removed for T-1013.

## Gates measured

- Claim-time baseline (before the move): workspace lib tests 647 root + 623
  core + 277 transport = **1547** passed / 0 failed / 1 ignored; guard
  `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`.
- Final `cargo test --workspace --lib --quiet`: 466 root + 181 api + 625 core +
  277 transport = **1549** passed / 0 failed / 1 ignored. The two additional
  tests are the new provider tests; all other tests moved with their modules.
- `cargo fmt --all -- --check` — 0 after formatting the rewritten files.
- `cargo clippy --workspace --features webview --all-targets -- -D warnings` — 0.
- `cargo test --test arch_layers -- --nocapture` — 5 passed; guard printed
  `modules=92, edges=280, cycles=3, upward_edges=26, crate_violations=0`.
  `ratchet_rejects_api_to_transport_edge` still passes.
- `cargo test --workspace --all-targets --quiet` — 466 root lib + 7 bin-unit +
  5 integration + 181 api lib + 625 core lib + 277 transport lib passed,
  0 failed.
- `cargo test --workspace --locked --no-default-features --quiet` and
  `cargo check --workspace --all-targets --no-default-features` — 0.
- `cargo check --workspace --offline` — 0.
- Debug smoke: `cargo build --locked` 0, `WebView2Loader.dll` present,
  `--version` printed `webagent 0.11.8 (4fc4e1ed4+dirty)` and `--help` printed
  usage; both exit 0.
- Release smoke: `cargo build --release --locked` 0 in 2m 51s,
  `WebView2Loader.dll` present, `--version` printed
  `webagent 0.11.8 (4fc4e1ed4+dirty)` and `--help` printed usage; both exit 0.
  The `+dirty` suffix is expected: the binaries were built from the uncommitted
  extraction tree before this slice was committed.
- Provider smoke, debug and release:
  `webagent relay --brain nonexistent_brain_xyz --message hi --headless --json`
  printed `[relay] Unbekanntes Brain 'nonexistent_brain_xyz' — verfuegbar:
  chatgpt, claude, deepseek, gemini, kimi, mistral, qwen, zai`, exit 2. The
  relay reached the installed real factory and rejected the unknown brain
  before any browser work; without the provider the error would have been
  `backend factory is not installed`. No live provider call was made.
- `git diff --check` — 0 (LF→CRLF working-copy warnings only).

## Grenze / offen

No merge, PR, tag, release or deployment made by this slice. No credentials
were read or used, no provider call, browser login or live brain/image session
was started. The task remains `claimed` until its branch is merged. After this
slice the extraction chain is complete; final integration into `master`, task
completion and release/deployment reconciliation are a separate approved step.
