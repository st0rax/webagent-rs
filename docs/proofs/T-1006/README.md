# T-1006 proof: public core traits and shared inference types

> **Referenz.** Task proof for T-1006. This is a refactor inside the single
> `webagent` package: no crate extraction, no merge, and no change to
> `Cargo.toml`, `[profile.*]` or dependencies.

**Claim.** Branch `refactor/T-1006-core-traits`, claim commit `9bf1841`,
implementation commit `7f87e5c`, owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. T-1006 stays `claimed`; no `done`, no PR and no merge.

## Bestand zuerst geprüft

- `src/inference_types.rs` did not exist. The shared value types lived in
  `src/browser_inference.rs`, `ClientPresence`/`AlwaysConnected` in
  `src/api_bridge/transport.rs`, and `WebSearchObservation` in
  `src/brain_score.rs`.
- `src/relay.rs` had a private `RelayBackend` trait and two direct
  `WebBrainBackend::from_config` calls. No factory or page-probe trait existed.
- `src/contract.rs` used `crate::browser::has_protocol_payload` and
  `crate::browser::block_phrase_in_text`, which are only re-exports of
  `crate::protocol` helpers; `src/counting.rs` directly constructed
  `WebBrainBackend` and used two concrete-only methods.
- `src/browser/send.rs` imported only value types from `browser_inference`, so
  the `browser → browser_inference` edge was removable by re-routing imports.
- The guard at claim time measured 88 modules / 274 edges / 3 cycles /
  32 upward edges. The phase-16 ratchet already grandfathered
  `contract → browser`, `counting → browser`, `contract → brain`,
  `brain_probe → page_driver`, `browser_inference → observer` and a six-module
  browser cycle.

## Delivered

- New dependency-free `src/inference_types.rs` (L0, rank 1) owns
  `BrowserTool`, `BrowserToolChoice`, `BrowserAttachment`,
  `BrowserAttachmentKind`, `BrowserInferenceRequest`, `BrowserToolCall`,
  `BrowserInferenceResponse`, `MODEL_OUTPUT_ERROR_MARKER`, the tool envelope
  constants/helper, `ClientPresence`, `AlwaysConnected` and
  `WebSearchObservation`.
- Old paths remain source-compatible: `browser_inference` re-exports the
  inference/tool types, `api_bridge::transport` re-exports the presence types,
  and `brain_score` re-exports the observation. No caller outside the refactored
  boundaries had to change.
- `src/brain.rs` now publishes `RelayBackend`, `BackendFactory`, `PageProbe`
  and the `WebBackendFactory` marker. `BrainBackend` gained defaulted
  `disable_toggle` and `diagnostic_state` methods so `counting` can hold
  `Box<dyn BrainBackend>` without losing behaviour.
- `src/browser/backend.rs` implements the three public traits for
  `WebBrainBackend` and `BackendFactory` for `WebBackendFactory`; the browser
  module supplies the concrete backend at link time.
- `relay_single_turn_checked` builds its backend through `BackendFactory`;
  `counting` uses the same factory; `contract` calls `crate::protocol`
  directly; `browser_inference` and relay production paths use
  `inference_types::ClientPresence`/`AlwaysConnected`.
- `docs/arch/CRATE_SPLIT.md` now records the T-1006 correction: `PageProbe`
  alone does not fix `contract → browser` or `counting → browser`. The former
  was a re-exported protocol helper, the latter needed the factory. The image
  generation relay path still constructs `WebBrainBackend` directly and needs
  a dedicated image-backend trait before extraction.

## Guard delta and ratchet edits

- Removed `edge:contract→browser`, `edge:contract→brain` and
  `edge:counting→browser` from `docs/arch/layer_exceptions.txt`; the guard
  confirmed each as stale before deletion.
- The former six-module cycle
  `api_bridge,brain_probe,browser,browser_inference,browser_pool,relay` shrank
  to `brain_probe,browser,browser_pool`. `browser_inference` and
  `free_cloud_chat` moved to L3 (rank 4) so their remaining downward edges stay
  legal; `contract` moved to L6 (rank 7) so its protocol/brain calls are
  downward. No new exception line was added.
- Measured after: 89 modules (new `inference_types`), 275 edges, 3 cycles,
  **29 upward edges** (was 32).

## Measurements (re-measured, not quoted)

| Measurement | Claim | Final |
|---|---:|---:|
| `cargo test --lib --quiet` total | 1524 | 1528 |
| passed / failed / ignored | 1523 / 0 / 1 | 1527 / 0 / 1 |
| lib wall time | 50.53 s | 50.20 s |
| arch guard modules / edges / cycles / upward | 88 / 274 / 3 / 32 | 89 / 275 / 3 / 29 |
| `cargo test --all-targets --quiet` | not run at claim | 1527 lib + 7 bin-unit + 4 integration, 0 failed |

Four lib tests were added: three in `inference_types` and one `PageProbe`
contract test in `brain`. No existing test was weakened or removed.

## Gates measured

- `cargo fmt --all -- --check` — 0.
- `cargo clippy --features webview --all-targets -- -D warnings` — 0.
- `cargo test --lib --quiet` — 1527 passed / 0 failed / 1 ignored.
- `cargo test --test arch_layers -- --nocapture` — 4 passed; guard printed
  `modules=89, edges=275, cycles=3, upward_edges=29`.
- `cargo test --all-targets --quiet` — 1527 lib + 7 bin-unit + 4 integration
  passed, 0 failed.
- `git diff --check` — 0 (only LF→CRLF working-copy warnings on edited files).

## Binary smoke (blocked, not a regression)

After the binary-affecting source changes, `target/debug/webagent.exe --help`
and `target/debug/webagent.exe diagnose` both exit `-1073741515`
(`0xC0000005`) before `main`, exactly as measured in the T-1004 proof. No
`[profile.*]` line and no dependency was changed, so this remains a
pre-existing loader issue rather than a T-1006 regression.

## Grenze / offen

No workspace or crate was created, no file was moved to another crate, no
branch was merged, and no PR, tag, release or deployment was made. No
credentials were read or used, no provider/login call was made, and no live
browser session was started. Crate extraction still requires a fresh, explicit
owner approval; this task stops at the prepared trait boundaries.
