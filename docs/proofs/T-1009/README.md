# T-1009 proof: ImageBackend boundary

> **Referenz.** Task proof for T-1009. This is a refactor inside the single
> `webagent` package: no crate extraction, no merge, and no change to
> `Cargo.toml`, `[profile.*]` or dependencies.

**Claim.** Branch `refactor/T-1009-image-backend`, claim commit `0ede5dc`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited; the branch was created from that claim commit. T-1009 stays
`claimed`; no `done`, no PR and no merge.

## Bestand zuerst geprüft

- The architecture guard at the claim commit measured 90 modules / 276
  `crate::<module>` edges / 3 cycles / 26 upward edges. The only wrong-direction
  crate edge left after T-1006/T-1007 was `relay → browser`, from
  `src/relay.rs:8` (`use crate::browser::WebBrainBackend;`) and
  `src/relay.rs:94` (`WebBrainBackend::from_config`).
- The image turn called eleven inherent methods on that concrete backend:
  `start`, `ensure_ready`, `stop`, `new_chat`, `enable_image_generation_mode`,
  `mark_image_generation_baseline`, `send`, `wake_renderer`,
  `latest_generated_image`, `image_generation_provider_error` and
  `image_generation_report`. The text path already used `BackendFactory` since
  T-1006; there was no `ImageGenerationBackend` trait and no exception line
  for the image edge in `layer_exceptions.txt`.

## Delivered

- New public, dependency-free `ImageGenerationBackend` trait in `src/brain.rs`,
  plus `BackendFactory::create_image_backend`. The trait returns a tuple
  `(mime_type, base64)` instead of `relay::GeneratedImage`, so core does not
  depend on the api crate.
- `WebBrainBackend` implements the trait in `src/browser/backend.rs` by
  delegating to the unchanged inherent methods; `WebBackendFactory` supplies
  the boxed backend.
- `relay_image_generation` now obtains the backend through the factory and
  delegates the turn loop to a new `relay_image_turn(&mut dyn
  ImageGenerationBackend, …)` helper. Lifecycle order, trace messages, timeout
  budget, circuit-breaker check, `PendingScope` and all error strings are
  unchanged. `relay_image_generation` and `GeneratedImage` keep their public
  paths; no `crate::browser` reference remains in `src/relay.rs`.
- One mock-backed contract test in `src/relay.rs` proves the lifecycle order
  (`start → ensure_ready → new_chat → enable_generation_mode → mark_baseline →
  send → wake_renderer → latest → stop`) without a browser.
- `docs/arch/CRATE_SPLIT.md` now marks all four original cross-crate blockers
  as prepared; `docs/arch/layer_exceptions.txt` records the new edge count in
  its baseline comment. No exception line was added or deleted because the
  removed edge never had one.

## Gates measured

- `cargo fmt --all -- --check` — 0.
- `cargo clippy --features webview --all-targets -- -D warnings` — 0.
- `cargo test --lib --quiet` — claim commit `0ede5dc`: 1530 passed / 0 failed /
  1 ignored (51.80 s); final: 1531 passed / 0 failed / 1 ignored (77.82 s).
  The single new test is the image lifecycle contract test; nothing was
  weakened or removed.
- `cargo test --test arch_layers -- --nocapture` — 4 passed; guard printed
  `modules=90, edges=275, cycles=3, upward_edges=26`, proving `relay → browser`
  is gone (was 276 edges).
- `cargo test --all-targets --quiet` — 1531 lib + 7 bin-unit + 4 integration
  passed, 0 failed.
- `cargo test --locked --no-default-features --quiet` — 1525 lib + 7 bin-unit +
  4 integration passed, 0 failed / 1 ignored.
- `git diff --check` — 0 (LF→CRLF working-copy warnings on edited files only).

## Grenze / offen

No crate extraction, merge, PR, tag, release or deployment was made. No
credentials were read or used, no provider/login call was made, and no live
browser or image generation was started. The browser-backed implementation was
only compile-checked and exercised by existing mocks; the generated-image
artifact path itself was not run live. The task remains `claimed` until its
branch is merged.
