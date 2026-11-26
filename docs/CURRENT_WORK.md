## 2026-11-26 - Phase 16 crate-extraction chain merged into master; board closed

- **Integration:** local `master` was fast-forwarded from `93d33a3` to released `origin/master` `fb7b886`, then the four reviewed branches were merged in order with explicit merge commits: `9227572` (T-1010), `83f6516` (T-1011), `7bed096` (T-1012), `c1771dbf4` (T-1013). All merges were conflict-free; working tree clean before the board update. No force-push, no history rewrite, no branch deleted.
- **Board:** T-1010..T-1013 are `done` with `done_at=2026-11-26` in `docs/TASKBOARD.json` and the `docs/TASKBOARD.md` mirror. Each keeps its claim, branch and proof path; no proof file was removed. Phase 16 now has no `claimed` task.
- **Gates on merged master:** fmt 0; clippy 0; arch 5 passed with `modules=92, edges=280, cycles=3, upward_edges=26, crate_violations=0` and `api → transport` still rejected; workspace lib 466 root + 181 api + 625 core + 277 transport = 1549 passed / 0 failed / 1 ignored; all-targets adds 7 bin-unit + 5 integration; no-default 466 + 181 + 625 + 271 passed / 1 ignored; offline check 0; debug and release builds 0 with `WebView2Loader.dll`, `--version`/`--help` 0 and a clean `webagent 0.11.8 (c1771dbf4)`; relay bad-brain smoke exit 2 with the real provider installed; `git diff --check` 0.
- **Release reconciliation:** annotated tag `v0.11.8` points at the released commit `fb7b886`, the exact base of this chain. GitHub release `v0.11.8` is published (not draft/prerelease, 2026-11-26T13:33:49Z) with seven assets (Windows EXE + DLL, Linux and Android binaries and three `.sha256` files). It was not moved, deleted or replaced; the merged extraction is a post-release refactor and all crate versions remain `0.11.8`.
- **Deployment reconciliation (facts):** `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\v0.11.8\webagent.exe` reports `webagent 0.11.8 (fb7b88607)`; the active `...\webagent-deploy-target\release\webagent.exe` still reports `webagent 0.11.7 (93d33a3f2+dirty)`. Rollback folders `v0.11.5-backup`, `v0.11.6-backup` and `v0.11.7-backup` exist. No deployment file was changed by the integration. Whether to promote the staged `v0.11.8` assets into the active folder and/or publish a new version for the extracted code remains an explicit owner decision.
- **Grenze / offen:** no tag, GitHub release or deployment file was changed by this integration; no credentials, provider call, browser login or live brain/image session was used. The extracted code is merged but not published as a new release.

## 2026-11-26 - T-1013 implemented: api modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1013-api-extraction`, claim commit `4fc4e1e`, owner `local/opencode`. T-1013 was claimed in board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1013/README.md`.
- **Delivered:** 23 source files (nine top-level modules) moved with `git mv` into `crates/webagent-api/src/`: `api_bridge` (+12 children), `browser_inference`, `comms`, `free_cloud_chat`, `https_client`, `oobe`, `relay`, `session` (+2 children), `source_scope`. Root re-exports `webagent_api::*`, so all existing paths stay valid. `crate::<core>` was rewritten to explicit `webagent_core::` paths in 14 files; compile-driven `pub(crate)` → `pub` widenings in `api_bridge` only. The t404 SDK test now resolves `../../scripts/t404` because its manifest moved. New core module `backend_factory` stores `fn` pointers (the trait is not dyn-compatible) and is installed once by the binary as `install::<webagent::WebBackendFactory>()`, removing the last `api → transport` blocker without a new dependency or exception.
- **Guard/docs:** `layers.toml` places `backend_factory` at L0 rank 1; rank 0 was rejected by the guard (`new upward edge backend_factory (rank 0) -> brain (rank 1)`). `CRATE_SPLIT.md` §4.3 and `layer_exceptions.txt` record the measured state; no exception line was added or removed. Guard `modules=92, edges=280, cycles=3, upward_edges=26, crate_violations=0`; `api → transport` still rejected.
- **Gates:** fmt 0; clippy 0; arch 5 passed; workspace lib tests 466 root + 181 api + 625 core + 277 transport = 1549 passed / 0 failed / 1 ignored (claim-time 1547, +2 new provider tests); all-targets adds 7 bin-unit + 5 integration; no-default 0; offline check 0; debug/release builds 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; relay bad-brain smoke exit 2 in both profiles, proving the provider is installed without a live call; `git diff --check` 0.
- **Grenze / offen:** no merge, tag, release or deployment, no credentials, provider call, browser login or live brain/image session. Extraction chain complete; final integration into `master`, task completion and release reconciliation are a separate approved step.

## 2026-11-26 - T-1013 claimed: api extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror carry T-1013 as `claimed`, owner `local/opencode`, branch `refactor/T-1013-api-extraction`, 2026-11-26. It branches from the pushed T-1012 implementation commit `1910fec` and stays `claimed` until merged. No source file was moved before this claim.
- **Claim-time baseline:** workspace lib tests 647 root + 623 core + 277 transport = 1547 passed / 0 failed / 1 ignored; guard `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0; debug and release smoke 0. Known blocker: `relay.rs` calls the concrete `WebBackendFactory` (transport), which api must not reference. It needs a core-side factory provider installed by the binary.

## 2026-11-26 - T-1012 implemented: transport modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1012-transport-extraction`, claim commit `3c4388d`, owner `local/opencode`. T-1012 was claimed in board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1012/README.md`.
- **Delivered:** 22 source files (ten top-level modules) moved with `git mv` into `crates/webagent-transport/src/`: `browser` (11 files), `brain_probe`, `browser_pool`, `login`, `mock_page`, `page_driver`, `panel_sink`, `upload_spa_js`, `webview_reveal`, `webview_runtime` (plus its two child files). Root re-exports `webagent_transport::*`, so all existing paths stay valid. `crate::<core>` was rewritten to explicit `webagent_core::` paths in 13 files; `WebBackendFactory` and its `BackendFactory` impl moved together to satisfy the orphan rule. `panel_sink` geometry helpers are exposed under a new `test-support` feature used by root dev-dependencies. No runtime behavior, dependency version or `[profile.*]` change.
- **Guard/docs:** `CRATE_SPLIT.md` §4.2 and `layer_exceptions.txt` record the measured state; `layers.toml` and the exception lines needed no edit. Guard still `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; `api → transport` still rejected.
- **Gates:** fmt 0; clippy 0; arch 5 passed; workspace lib tests 647 root + 623 core + 277 transport = 1547 passed / 0 failed / 1 ignored (identical to claim-time total); all-targets adds 7 bin-unit + 5 integration; no-default 647 + 623 + 271 passed / 1 ignored; offline check 0; debug/release builds 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; `git diff --check` 0.
- **Grenze / offen:** no api module moved, no merge, tag, release or deployment, no credentials, provider call or browser login. Next: T-1013 api extraction from this branch.

## 2026-11-26 - T-1012 claimed: transport extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror carry T-1012 as `claimed`, owner `local/opencode`, branch `refactor/T-1012-transport-extraction`, 2026-11-26. It branches from the pushed T-1011 implementation commit `2635336` and stays `claimed` until merged. No source file was moved before this claim.
- **Claim-time baseline:** workspace lib tests 924 root + 623 core = 1547 passed / 0 failed / 1 ignored; guard `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0; debug and release smoke 0. Known blocker to resolve first: `panel_sink` is a root module used by transport candidates, and the concrete `WebBackendFactory` marker must move with its `BackendFactory` impl.

## 2026-11-26 - T-1011 implemented: core modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1011-core-extraction`, claim commit `6e421e6`, owner `local/opencode`. T-1011 was claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1011/README.md`.
- **Delivered:** 34 platform-neutral modules and the root `lib.rs` helper block moved into `crates/webagent-core` (helpers in new `support` module). Root re-exports the crate with `pub use webagent_core::*`; no public path changed. `counting` deliberately stays in root because it calls `WebBackendFactory::create_brain_backend` (impl in root browser); the factory marker moved to root `browser/backend.rs` to satisfy the orphan rule. Selector JSON assets moved with `config/selectors.rs`. Compile-driven `pub(crate)` widenings and three source-path test fixes only; no runtime behavior change.
- **Guard/docs:** `layers.toml` gained `support` at rank 0; `layer_exceptions.txt` records `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0` with no exception line changed; `CRATE_SPLIT.md` records the actual extraction and the `counting` deviation.
- **Gates:** fmt 0; `cargo clippy --workspace --features webview --all-targets -- -D warnings` 0; arch 5 passed; unit tests 924 root + 623 core = 1547 passed / 0 failed / 1 ignored (identical to the claim-time total); all-targets 924 lib + 7 bin-unit + 5 integration + 623 core lib; no-default 918 + 7 + 5 + 623 passed / 1 ignored; debug build and `--version`/`--help` exit 0; release build 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; `git diff --check` 0. A late path-semantics fix (`root_dir()`/`selectors_dir()` and a benchmark test plan path) was applied during final verification; all gates were then re-run green on the final tree, including the release rebuild.
- **Grenze / offen:** no merge into master, no new tag/release/deployment, no credentials, live provider call or browser login. Next: T-1012 transport extraction from this branch.

## 2026-11-26 - T-1011 claimed: core extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror now carry T-1011 as `claimed`, owner `local/opencode`, branch `refactor/T-1011-core-extraction`, 2026-11-26. It builds directly on the caught-up T-1010 branch (`cb1e51b`) and stays `claimed` until merged. No module was moved before this claim.
- **Claim-time baseline:** lib 1547 passed / 0 failed / 1 ignored; guard `modules=90, edges=277, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0. The extraction must preserve these counts (modulo moved-module test relocation) and re-export the existing public API from the root crate.

## 2026-11-26 - T-1010 branch caught up with released v0.11.8 master

- **Lagebild:** Another session merged phase 16 through T-1009 into `origin/master` and published tag `v0.11.8` (`fb7b886`), explicitly leaving crate extraction to this session. `refactor/T-1010-workspace-guard` was based on the pre-release `92011a5`; `origin/master` was merged into it as a normal merge commit (no force-push, no history rewrite).
- **Konfliktauflösung:** Board conflicts were resolved in favour of the released board: T-997..T-1009 stay `done`, T-976/T-977 remain present, T-1010 stays `claimed`. `Cargo.toml`/`Cargo.lock` kept released version `0.11.8` and gained the workspace skeleton. `docs/TASKBOARD.json` parses with 116 tasks; no conflict markers remain.
- **Re-gates:** fmt 0; clippy 0; arch 5 passed with `modules=90, edges=277, cycles=3, upward_edges=26, crate_violations=0` (two new edges from released Play/T-976/T-977 code, no new exception); lib 1547 passed / 0 failed / 1 ignored (was 1531 before the master merge); all-targets 1547 lib + 7 bin-unit + 5 integration; no-default 1541 lib + 7 + 5; `git diff --check` 0. The T-1010 proof records both measurement points.
- **Grenze / offen:** no crate extracted yet; this branch is still not merged into master. No new tag, release, deployment, credentials, live provider call or browser login. Next: T-1011 core extraction from this caught-up branch.

## 2026-11-26 - Produktstand ausgeliefert: Play-Code, Phase 16, Release-Vorbereitung

- **Auftrag:** fertig machen. Das heisst den vorhandenen Produktstand auf `master` bringen und ausliefern, nicht die naechste Architekturbaustelle oeffnen.
- **Enthalten:** Phase 16 bis T-1009, Play-Kommando (`src/repl/play.rs`), sichtbare API-Turns (T-976), zentrale Modellgrenzen und Timeouts (T-977). `relay` deckt geparkte WebViews vor dem Turn auf (`relay_reveal_for_turn`).
- **Nicht geoeffnet:** Play-v0.3-Spec-Tasks T-978..T-996 (kein Code, 0 Tests). Crate-Extraktion T-1010 bleibt in der anderen Session und ist nicht Teil dieser Auslieferung.
- **Release:** `Cargo.toml` und `Cargo.lock` stehen auf `0.11.8`. Tag `v0.11.8` wird nach dem Versionsgate auf diesen Commit gesetzt. Kein Produktiv-Deployment.

## 2026-11-26 - Phase 16 landed on master through T-1008

- **Freigabe:** Der Eigentuemer hat die offene Arbeit aus der Session-Auswertung
  bestaetigt: Release-Absturz schliessen und die ungemergte Phase-16-Kette nach
  `master` legen. Kein Tag, kein GitHub-Release, kein Deployment, keine
  Crate-Extraktion, kein Live-Lauf.
- **Merge:** Fast-Forward `origin/master` `93d33a3` → `fa7c37d`
  (`fix/T-1008-webview-loader-smoke`). 29 Commits, T-997 bis T-1008.
- **Absturz, selbst geprueft:** Exit `-1073741515` ist `0xC0000135`
  (`STATUS_DLL_NOT_FOUND`), nicht `0xC0000005`. Frischer Worktree auf `fa7c37d`,
  `cargo build --release --locked` Exit 0 in 11m 12s, `WebView2Loader.dll`
  (165336 Bytes) lag ohne manuelles Skript neben der EXE.
  `target/release/webagent.exe --version` → `webagent 0.11.7 (fa7c37dda)`, Exit 0.
  `--help` Exit 0, 54 Zeilen. Kein Login, kein Browserfenster.
- **Board:** T-997..T-1008 nach dem Merge auf `done` gesetzt (`done_at`
  2026-11-26).

## 2026-11-26 - T-1010 implemented: workspace skeleton and multi-crate guard

- **Branch/claim:** `refactor/T-1010-workspace-guard`, claim commit `142f964`, owner `local/opencode`. T-1010 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This slice adds only the workspace skeleton and guard coverage; no production module moved.
- **Delivered:** root `Cargo.toml` gains `[workspace]` with `members = [".", "crates/webagent-core", "crates/webagent-transport", "crates/webagent-api"]`, `resolver = "2"`, `default-members = ["."]`; one shared `Cargo.lock`/`target/`. Three empty library members exist with their own manifests and no `[profile.*]`. All `[profile.*]` values and dependency versions are unchanged; two stale `0xC0000005` comments were corrected to the measured `0xC0000135` (T-1008). Proof: `docs/proofs/T-1010/README.md`.
- **Guard upgrade:** `tests/arch_layers.rs` now discovers modules from every workspace `lib.rs`, records each module's owning crate, counts `crate::` plus explicit `webagent_core::`/`webagent_transport::`/`webagent_api::` paths, and enforces an allowed crate-pair matrix (core has no library outgoing edge; transport and api are siblings reaching only core; only root reaches all three). The conceptual layer/cycle/upward/stale-exception checks are unchanged. A new negative test rejects `api → transport`. `docs/arch/layer_exceptions.txt` records `crate_violations=0`; no exception line changed.
- **Gates measured:** `cargo metadata --no-deps` lists all four packages. Claim lib 1531 passed / 0 failed / 1 ignored; final lib 1531 / 0 / 1 (52.06 s), unchanged. `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 5 passed with `modules=90, edges=275, cycles=3, upward_edges=26, crate_violations=0`; `cargo test --all-targets --quiet` 1531 lib + 7 bin-unit + 5 integration; `cargo test --locked --no-default-features --quiet` 1525 lib + 7 bin-unit + 5 integration; debug/release build and `--version`/`--help` smoke exit 0; `git diff --check` 0. The root path deps are intentionally not added until T-1011 so clippy does not report unused dependencies.
- **Grenze / offen:** no module moved, no dependency version changed, no merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`. Next slice is T-1011: move the core modules into `webagent-core` with root re-exports, add the path dep, and keep the guard and all gates green.

## 2026-11-26 - T-1009 implemented: ImageBackend boundary for relay

- **Branch/claim:** `refactor/T-1009-image-backend`, claim commit `0ede5dc`, owner `local/opencode`. Trait-Grenze, keine Crate-Extraktion.
- **Delivered:** `ImageGenerationBackend` plus `BackendFactory::create_image_backend` in `brain.rs`; `WebBrainBackend` implementiert das Trait; `relay_image_generation` geht ueber die Factory und `relay_image_turn`. Lifecycle, Traces, Timeouts und Fehlerstrings unveraendert. Proof: `docs/proofs/T-1009/README.md`.
- **Nachgemessen auf dem Merge nach `6969da3`:** `rg crate::browser src/relay.rs` leer. `cargo test --lib image_turn_uses_the_core_trait_lifecycle` 1 passed. `cargo test --test arch_layers` 4 passed, `modules=90, edges=275, cycles=3, upward_edges=26`. `taskboard_invarianten_halten` 1 passed. `git diff --check` 0.
- **Merge:** `origin/refactor/T-1009-image-backend` (`92011a5`) nach `master`. T-1009 danach `done`. Kein Tag, kein Release, keine Crate-Extraktion. T-1010 bleibt bei der anderen Session. Play-Mode-Branch `fix/T-976-api-turn-reveal` ist nicht enthalten.

## 2026-11-26 - T-1008 implemented: local runnable WebView2 build

- **Branch/claim:** `fix/T-1008-webview-loader-smoke`, claim commit `2d64df9`, owner `local/opencode`. T-1008 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This is a local build fix, not crate extraction.
- **Diagnosis correction:** every phase-16 note that reads exit `-1073741515` as `0xC0000005` was wrong. `-1073741515` is `0xC0000135` (`STATUS_DLL_NOT_FOUND`); `gdb` showed `During startup program exited with code 0xc0000135.` with no stack, i.e. the loader failed before `main` because `WebView2Loader.dll` was missing beside the EXE. The correction is recorded in the proofs for T-1004/T-1006/T-1007, `docs/arch/CRATE_SPLIT.md`, `docs/arch/CLI_GROUPS.md` and `docs/proofs/T-1008/README.md`.
- **Delivered:** the existing root `build.rs` now copies `WebView2Loader.dll` from `target/<profile>/build/webview2-com-sys[-<hash>]/out/x64` to `target/<profile>/` for Windows + `webview` builds. It handles both the Cargo 1.100 `build/webagent/<hash>/out` layout and the older `build/webagent-<hash>/out` layout by locating the profile directory through the sys-crate build folder. `scripts/copy-webview2-loader.ps1` stays as a documented manual fallback and for CI. No `Cargo.toml`, `[profile.*]`, dependency or workflow change.
- **Smoke proven:** with the DLL deleted first, `cargo build --locked` and `cargo build --release --locked` each recreated it; debug and release `--version` and `--help` all exit 0. Browser-free `cargo build --no-default-features --locked` also exits 0 and emits no missing-DLL warning.
- **Gates measured:** claim-time lib 1530 passed / 0 failed / 1 ignored (55.72 s); final lib 1530 / 0 / 1 (49.43 s), unchanged. `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed with `modules=90, edges=276, cycles=3, upward_edges=26`; `cargo test --all-targets --quiet` 1530 lib + 7 bin-unit + 4 integration; `git diff --check` 0.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`. The owner has approved the remaining work; next slice is the image-backend trait for `relay → WebBrainBackend`, then crate extraction and merges. Proof: `docs/proofs/T-1008/README.md`.

## 2026-11-26 - T-1007 implemented: PanelSink boundary for transport

- **Branch/claim:** `refactor/T-1007-panel-sink`, claim commit `6fec681`, owner `local/opencode`. T-1007 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. It is follow-up 3 from `docs/arch/CRATE_SPLIT.md`, not crate extraction.
- **Delivered:** new dependency-free `src/panel_sink.rs` owns `Rect`, the pure grid/reveal geometry and the read-only `PanelSink` trait. `brain_grid` keeps the old re-exports and implements `DesktopPanelSink`; `main.rs` installs it before dispatch. `browser_pool`, `webview_reveal` and `webview_runtime` now reference `panel_sink` only; no `crate::brain_grid` remains in those three modules. Parking, docking and focus behavior is unchanged. Proof: `docs/proofs/T-1007/README.md`.
- **Documented limit:** without installation (library users and unit tests), `panel_sink()` returns `NoPanelSink` and `reveal_rect()` uses the 80/80/1280×900 fallback. The binary installs the desktop sink, so production geometry is unchanged.
- **Guard delta:** deleted `browser_pool→brain_grid`, `webview_reveal→brain_grid` and `webview_runtime→brain_grid` from the ratchet after the guard proved them stale; added `panel_sink` to L0. Measured 90 modules / 276 edges / 3 cycles / 26 upward edges (was 89 / 275 / 3 / 29). The `brain_probe,browser,browser_pool` cycle is unchanged; no exception was added.
- **Tests re-measured:** claim 1528 total / 1527 passed / 0 failed / 1 ignored (50.71 s); final 1531 total / 1530 passed / 0 failed / 1 ignored (52.40 s). Three new unit tests in `panel_sink`. Gates green: `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1530 lib + 7 bin-unit + 4 integration; `cargo test --locked --no-default-features --quiet` final 1524 lib + 7 bin-unit + 4 integration; `git diff --check` 0.
- **Flake, not hidden:** two parallel browser-free runs failed intermittently with `ConnectionReset` in the untouched `web_ui.rs` one-shot helper (two different tests). Isolated retry, full serial run and a third parallel run passed; the claim commit passed in parallel in a clean worktree. No regression from the changed modules was reproducible.
- **Binary smoke blocked, not a regression:** `cargo build` succeeded, but default `webview` `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time. `Cargo.toml`, `[profile.*]` and dependencies were not touched.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`; crate extraction still needs fresh explicit owner approval. Next owner choice: review/push this branch, or approve the image-backend follow-up explicitly.

## 2026-11-26 - T-1006 implemented: public core traits and shared inference types

- **Branch/claim:** `refactor/T-1006-core-traits`, claim commit `9bf1841`, implementation commit `7f87e5c`, owner `local/opencode`. T-1006 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This is F1 from the T-1004 follow-up list, not crate extraction.
- **Delivered:** new dependency-free `src/inference_types.rs` owns the shared inference/tool types, `ClientPresence`/`AlwaysConnected` and `WebSearchObservation`; old paths keep working via re-exports. `brain.rs` publishes `RelayBackend`, `BackendFactory`, `PageProbe` and `WebBackendFactory`; the browser module implements them. `relay_single_turn_checked`, `counting` and `contract` now use the new boundaries. Proof: `docs/proofs/T-1006/README.md`.
- **Correction to T-1004:** `PageProbe` alone does not remove `contract → browser` or `counting → browser`. `contract` used protocol helpers merely re-exported by `browser`; `counting` needed `BackendFactory`. `docs/arch/CRATE_SPLIT.md` now says so. The image-generation relay path still constructs `WebBrainBackend` directly and needs a separate image-backend trait before extraction.
- **Guard delta:** removed three stale `edge:` lines and replaced the six-module browser cycle with `brain_probe,browser,browser_pool`; no exception was added. `browser_inference`/`free_cloud_chat` moved to L3 and `contract` to L6 so remaining edges are downward. Measured 89 modules / 275 edges / 3 cycles / 29 upward edges (was 88 / 274 / 3 / 32).
- **Tests re-measured:** claim 1524 total / 1523 passed / 0 failed / 1 ignored (50.53 s); final 1528 total / 1527 passed / 0 failed / 1 ignored (50.20 s). Four new unit tests (three in `inference_types`, one `PageProbe` test in `brain`). All gates green: `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1527 lib + 7 bin-unit + 4 integration passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** default `webview` `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, identical to the T-1004 baseline. `Cargo.toml`, `[profile.*]` and dependencies were not touched.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`; crate extraction still needs a fresh explicit owner approval. Next owner choice: review/push this branch, or approve the next follow-up (PanelSink/image backend) explicitly.

## 2026-11-26 - T-1004 delivered: crate split decision (design only)

- **Branch/claim:** `docs/T-1004-crate-split`, claim commit `f27928b`, owner `local/opencode`. T-1004 stays `claimed`; no merge, no `done`, no PR. The owner phrase `mach fertig den senf` approved finishing the design document only, not crate extraction. Formal `depends_on` is satisfied in substance by T-998..T-1003, but those branches are also still unmerged.
- **Delivered:** `docs/arch/CRATE_SPLIT.md` with a re-measured baseline, the naive and recommended group graphs, per-crate module/Cargo trees, seven required trait boundaries, the workspace-root `[profile.*]` policy, nine ordered follow-up proposals and an explicit no-go list. Proof: `docs/proofs/T-1004/README.md`. No `src/`, Cargo, profile or test file was touched; no follow-up task ID was created.
- **Decision:** three libraries (`webagent-core`, `webagent-transport`, `webagent-api`) plus the existing root binary crate. Recommended grouping measured at 34 core / 9 transport / 9 api / 36 root modules. Six cross-crate blockers remain before extraction: `contract`/`counting → browser`, `browser → browser_inference`, three `brain_grid` hooks, and `relay → WebBrainBackend`. They need `PageProbe`, shared inference types, `PanelSink` and `BackendFactory`; `RelayBackend` already exists privately in `relay.rs` and should become a public core trait.
- **Baseline re-measured, not quoted:** `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (53.09 s); guard 88 modules / 274 edges / 3 cycles / 32 upward edges; clean lib test build+link 65.98 s with a 162,341,370-byte test executable; clean default binary build+link 40.47 s with a 150,514,926-byte `webagent.exe`; 21 direct Cargo dependencies. A scratch copy of the guard sanitizer under `%TEMP%\opencode\` produced the group matrices without touching repo files.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; startup markdown-banner test 1 passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** default `webview` `--help` still exits `-1073741515` (`0xC0000005`) before `main` at the claim commit. No binary-affecting change was made, and `[profile.*]` (`codegen-units = 16`) was not touched.
- **Grenze / offen:** no crate extraction, code change, credentials, live call, release, tag, deployment, PR or merge. Next extraction step requires a fresh, explicit owner approval; stop here.

## 2026-11-26 - T-1005 delivered: CLI grouping design (no code changes)

- **Branch/claim:** `docs/T-1005-cli-groups`, claim commit `afd11c9`, owner `local/opencode`. T-1005 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Delivered:** `docs/arch/CLI_GROUPS.md` with the verbatim before-help snapshot, 43-command inventory, proposal for `brain`/`run`/`check`/`api`/`dev` groups, illustrative after-help, default-help policy and an alias table keeping every old name callable. Proof: `docs/proofs/T-1005/README.md`. No `src/`, Cargo or test file was touched.
- **Snapshot source:** browser-free binary at `938bbf1` (`--no-default-features`), because the default `webview` binary crashes before `main` with `0xC0000005`; the command enum is not feature-gated, so help text is identical.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `git diff --check` 0; startup markdown-banner test 1 passed.
- **Grenze / offen:** design only, no command removed, no behavior change, no credentials, release, tag, deployment, PR or merge. Open for implementation: whether `serve` maps to `api` or `ui --api`, and whether hidden root aliases get a `help --all`. T-1004 remains free and still needs explicit owner approval; no further task was claimed.

## 2026-11-26 - T-1003 implemented: Windows GUI dependencies behind `webview`

- **Branch/claim:** `chore/T-1003-feature-hygiene`, claim commit `a76d79d`, owner `local/opencode`. The initial `build/` name was renamed before the claim because the board invariant and commit script only allow `feature|fix|docs|chore|refactor|test`. T-1003 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten".
- **Bestand zuerst geprüft:** `windows` 0.58 is used only behind `cfg(all(windows, feature = "webview"))` (JobObjects in `worker_pool.rs`, ToolHelp in `brain_grid.rs`/`brain_wall.rs`, console handle in `lib.rs`/`main.rs`), so the browser-free build does not need it. `windows-sys` 0.59 is used by non-GUI code (`config/profiles.rs`, `config/writeback.rs`) and stays non-optional. Proof: `docs/proofs/T-1003/README.md`.
- **Changes:** `Cargo.toml` only — `webview2-com`, `windows`, `arboard`, `clipboard-win`, `image` are optional and wired into the `webview` feature via `dep:`; `windows-sys` moved into the Windows target table. No `[profile.*]` line changed; `src/lib.rs` and `Cargo.lock` needed no edit.
- **Measured:** Windows `cargo tree --no-default-features -e normal --depth 1` went from five GUI crates plus `windows-sys` to core crates plus `windows-sys`; Linux tree has no `windows-sys`. After `cargo clean -p webagent`, lib test binary 95,086,256 → 94,481,201 bytes (−605,055); wall-clock build+link 46.69 s → 47.66 s (no improvement, within noise). Default binary size unchanged at 150,514,926 bytes; browser-free binary 82,625,882 bytes.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --all-targets --quiet` 1523 lib + 7 bin-unit + 4 integration passed; `cargo test --locked --no-default-features --quiet` 1517 lib + 7 bin-unit + 4 integration passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Binary smoke:** default `webview` binary still crashes before `main` with `0xC0000005` on `--help` and `diagnose --brain deepseek --headless` (pre-existing, blocked DoD item). Browser-free binary: `--help` exit 0; `diagnose --brain deepseek --headless` exit 1 with the intended `page-unavailable: WebView-Feature nicht aktiviert` message. Linux cross-check blocked: `x86_64-linux-gnu-gcc` is not installed; the Linux dependency tree was checked instead.
- **Grenze / offen:** no credentials, live provider call, release, tag, deployment, PR or merge. T-1004..T-1005 remain free; T-1004 still needs explicit owner approval — stop here.

## 2026-11-26 - T-1002 implemented: tui_load/tui_state cycle removed, legacy TUI cycle documented

- **Branch/claim:** `refactor/T-1002-tui-cycles`, claim commit `06f5f2d`, owner `local/opencode`. T-1002 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Bestand zuerst geprüft:** the only `tui_state -> tui_load` production edge was the `pub use crate::tui_load::load_state;` re-export, consumed solely by `tui.rs`. Removed it and imported `load_state` directly in `tui.rs`. The `bin_hooks,repl,tui,tui_ansi` cycle spans four modules and slash-command/`PersistentQueryPool`/`run_tui`/ANSI helpers; per the task's low-priority rule it stays with a written reason in `docs/arch/layer_exceptions.txt` (legacy TUI, no behavior gain, no feature work). Proof: `docs/proofs/T-1002/README.md`.
- **Guard delta:** 88 modules / 274 edges (unchanged: one edge moved from `tui_state` to `tui`) / 3 cycles (was 4) / 32 upward edges (unchanged). Removed exception `cycle:tui_load,tui_state`; the remaining cycle line now carries its justification comment.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time. `Cargo.toml` and `[profile.*]` untouched.
- **Grenze / offen:** no login, credentials, live provider call, TUI feature work, release, tag, deployment, PR or merge. T-1003..T-1005 remain free; T-1004 still needs explicit owner approval. Per the standing instruction, the next free slice (T-1003) may be claimed; stop before T-1004.

## 2026-11-26 - T-1001 implemented: capability <-> capability_proof cycle removed

- **Branch/claim:** `refactor/T-1001-capability-cycle`, claim commit `2de15c5`, owner `local/opencode`. T-1001 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Bestand zuerst geprüft:** `src/capability_types.rs` did not exist; the leaf named in the board scope was created. `capability_proof` remains the single proof gate and store — no second gate. `ProofKind`, `Capability`, `CATALOG`, `capability()` and `capability_for_route()` moved verbatim into the dependency-free leaf; `capability.rs` re-exports them, so all public paths and callers stay identical. `capability_proof.rs` imports from `capability_types`; no `crate::capability` reference remains there. Proof: `docs/proofs/T-1001/README.md`.
- **Guard delta:** the `capability,capability_proof` cycle is gone. Measured graph: 88 modules (new leaf) / 274 edges (was 273) / 4 cycles (was 5) / 32 upward edges (unchanged). `docs/arch/layer_exceptions.txt` lost the cycle line; no `edge:` line changed because the three modules share L4.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged from claim baseline, no tests added); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Deviation:** `src/lib.rs` (module declaration) and `docs/arch/layers.toml` (one L4 entry) are outside the three-file scope but required for a new top-level module to exist and be ranked. No behavior change to CLI, protocol, API, shell policy or proof-file format. `Cargo.toml` and `[profile.*]` untouched.
- **Follow-up already published:** the claim-time baseline was red because the T-1000 proof lacked the mandatory reference banner. Fixed on `refactor/T-1000-config-brain-limits-edge` at `27b38af` (pushed) and cherry-picked here at `52b41f8`; baseline then measured green.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time.
- **Grenze / offen:** no login, credentials, live provider call, release, tag, deployment, PR or merge. T-1002..T-1005 remain free; T-1004 still needs explicit owner approval. Per the standing instruction, the next free slice (T-1002) may be claimed; stop before T-1004.

## 2026-11-26 - T-1000 implemented: config no longer reaches brain_limits

- **Branch/claim:** `refactor/T-1000-config-brain-limits-edge`, claim commit `3d038ea`, owner `local/opencode`. T-1000 stays `claimed`; no merge, no `done`, no PR. Formally `depends_on: T-999`, but the owner's "autonom weiterarbeiten" explicitly overrode that gate just as the earlier "mach" did; T-999 stays `claimed` because it is not merged.
- **Bestand zuerst geprüft:** `src/config/limits.rs:47` called `crate::brain_limits::accepted_chars` while `brain_limits` reads `config::data_dir`. The inversion uses a small `AcceptedChars` trait: `config::limits` takes the resolved value by parameter (`Option<usize>` implements it) and `brain_limits` implements it for `&str`, delegating to the existing measurement-store lookup. The unchanged caller in `controller.rs` keeps compiling. `git grep -n "crate::brain_limits" -- src/config` is empty. Proof: `docs/proofs/T-1000/README.md`.
- **Guard delta:** the `brain_limits,config` cycle is gone. Measured graph: 87 modules / 273 edges (was 274) / 5 cycles (was 6) / 32 upward edges (unchanged). `docs/arch/layer_exceptions.txt` lost the `cycle:brain_limits,config` line; no `edge:` line changed. The ratchet initially failed on the stale entry, which confirmed the removal before the exception was deleted.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (claim-time baseline 1521 / 0 / 1, two tests added); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Deviation:** `src/config/mod.rs` is outside the board's two-file scope. The one-line `AcceptedChars` re-export is required because `mod limits;` is private; duplicating store parsing in `config` would create a second reader of the measurement file. No behavior change to CLI, WEBAGENT/1 protocol, API or shell policy. `Cargo.toml` and all `[profile.*]` settings are untouched.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as measured at the claim-time baseline on the earlier phase-16 branches.
- **Grenze / offen:** no login, credentials, live provider call, release, tag, deployment, PR or merge. T-1001..T-1005 remain free; T-1004 still needs explicit owner approval and was not touched. Per the owner's standing "autonom weiterarbeiten", the next free slice (T-1001) may be claimed; stop before T-1004.

## 2026-11-26 - T-999 implemented: config no longer reaches worker_pool

- **Branch/claim:** `refactor/T-999-config-worker-pool-edge`, claim commit `928ea11`, implementation commit `d6a4b37`, owner `local/opencode`. T-999 stays `claimed`; no merge, no `done`, no PR. Both commits are pushed and the worktree is clean. T-997 and T-998 also stay `claimed`; the owner's "mach" again overrode the formal dependency.
- **Bestand zuerst geprüft:** `src/fsutil.rs` does not exist and is not declared in `src/lib.rs`. The required leaf already exists in `src/pool_state.rs` (`atomic_write`, `RETRY_UNAVAILABLE_AFTER_SECS`) and `worker_pool.rs` already re-exports it. No new module was created; `src/lib.rs` and `docs/arch/layers.toml` were not touched. `src/config/brains.rs` now calls `crate::pool_state::…` directly, so `crate::worker_pool` has no match under `src/config`.
- **Guard delta:** the cycle `brain_limits,circuit_breaker,config,pool_failover,worker_pool` shrank to `brain_limits,config`. Measured graph is unchanged at 87 modules / 274 edges / 6 cycles / 32 upward edges. `docs/arch/layer_exceptions.txt` holds the shorter cycle; no `edge:` line was added or removed. Proof: `docs/proofs/T-999/README.md`.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1521 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. Claim-time baseline was already 1521 / 0 / 1, so the test count is unchanged.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`; binary size 150,498,536 bytes, identical to the T-998 branch head. The same crash was measured at claim commit `103967b` in a clean worktree. `Cargo.toml` and all `[profile.*]` settings remain untouched.
- **Grenze / offen:** no behavior change to CLI, protocol, API or shell policy; no login, credentials, live call, release, tag, deployment, PR or merge. T-1000..T-1005 remain free; T-1004 still needs owner approval. Stop here and wait; do not claim T-1000 implicitly.

## 2026-11-26 - T-998 implemented: protocol/brain no longer reach browser

- **Branch/claim:** `refactor/T-998-protocol-browser-edge`, claim commit `103967b`, implementation commit `5a24ddb`, owner `local/opencode`. T-998 stays `claimed` in `docs/TASKBOARD.json` — no merge, no `done`, no PR. T-997 also stays `claimed`; the owner's explicit "mach" overrode the formal dependency. Both commits are pushed and the worktree is clean.
- **Scope delivered:** `has_protocol_payload`, `block_phrase_in_text`, `BLOCK_PHRASES` and `BLOCK_BANNER_MAX_CHARS` now live in `src/protocol/mod.rs`; `src/brain.rs` and `src/protocol/parser.rs` call the protocol module directly; `src/browser/mod.rs` re-exports both helpers so `controller.rs` and `contract.rs` keep compiling unchanged. `rg` has no `crate::browser` hit in `src/protocol` or `src/brain.rs` (ripgrep-backed search: the `rg` executable is absent from this PowerShell). Proof: `docs/proofs/T-998/README.md`.
- **Guard delta:** the former eight-module cycle `api_bridge, brain, brain_probe, browser, browser_inference, browser_pool, protocol, relay` shrank to six modules (`brain` and `protocol` dropped out). Measured graph stayed at 87 modules / 274 edges / 6 cycles; upward edges rose from 30 to 32 because removing the cycle exposed `browser->protocol` and added `brain->protocol`. Both are explicit `edge:` exceptions in `docs/arch/layer_exceptions.txt`; the cycle line was shortened, not deleted. No behavior change.
- **Deviations:** `docs/arch/LAYERS.md` and `docs/proofs/T-997/README.md` received the `**Referenz.**` banner required by `startup::tests::betriebs_markdown_hat_eine_wahrheit`; both are T-997 documentation artifacts outside the T-998 scope and were red at claim time. The two new upward-exception lines are documented debt: the task requires the helper in the protocol layer while `brain` is ranked lower. No strict shrink of the total exception list.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored; `cargo test --test arch_layers -- --nocapture` 4 passed (guard: modules=87, edges=274, cycles=6, upward_edges=32); `cargo test --all-targets --quiet` 1521 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. Test count is unchanged from the T-997 baseline; the initial claim-time lib run was 1520 passed / 1 failed / 1 ignored solely because of the two missing banners.
- **Binary smoke blocked, not a regression:** `target/debug/webagent.exe --help` and `diagnose` exit `-1073741515` (`0xC0000005`) before `main`. A clean detached worktree at claim commit `103967b` builds a 150,498,625-byte binary with the same crash; this branch's binary is 150,498,536 bytes. `Cargo.toml` and every `[profile.*]` setting are untouched. No login, live brain call or provider request was made.
- **Grenze / offen:** no release, tag, deployment, PR or merge. T-999..T-1005 remain free; the crate-split decision (T-1004) still needs owner approval. Stop here and wait for review; do not claim T-999 implicitly.

## 2026-11-26 - T-997 implemented: module layer and cycle ratchet

- **Branch/claim:** `feature/T-997-arch-layers`, implementation commit `2b875b5`, owner `local/opencode`. T-997 remains `claimed` in `docs/TASKBOARD.json` — no merge, no `done`. The board/mirror/claim commit is `fdff65c`. No next task claimed.
- **Scope delivered:** `tests/arch_layers.rs` (source-based graph plus four tests), `docs/arch/layers.toml` (87 modules over `cross` + `L0..L7`), `docs/arch/layer_exceptions.txt` (6 cycles + 30 grandfathered upward edges), `docs/arch/LAYERS.md`, `docs/proofs/T-997/README.md`. Only the proof file and this handover are additional; no source module, CLI, protocol, API, `Cargo.toml` or `[profile.*]` change.
- **Measurement:** the guard derives 87 top-level modules, 274 `crate::<module>` edges, 6 cycles and 30 upward edges from `master 93d33a3`. The prompt's 89 modules / five cycles did not reproduce: `browser_pool`, `bin_hooks` and a sixth `benchmark`/`controller` cycle were missing from its list; raw lines were 92,649 (64,130 after removing `#[cfg(test)]`). Differences and reasons are in the proof.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored (unchanged from base); `cargo test --test arch_layers -- --nocapture` 4 passed / 0 failed; `cargo test --all-targets --quiet` lib 1521 + binary-unit 7 + integration 4 passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. The three extra integration tests prove rejection of a new cycle, a new upward edge and a stale exception.
- **Ratchet behavior:** every exception must still exist, so a removed edge/cycle makes the test fail until its line is deleted. Edges inside an excepted cycle are covered by the cycle line; the 30 remaining upward edges are explicit inventory, not hidden debt.
- **Grenze / offen:** no browser, network, login, secrets, capability-proof record, behavior change, release, tag, deployment, PR or merge. T-998..T-1005 remain free; the crate-split decision (T-1004) still needs owner approval before any extraction.

## 2026-11-26 - Phase 16 board prepared, T-997 claimed (bookkeeping only)

- **Branch:** `feature/T-997-arch-layers`, created from clean `master` `93d33a3` in worktree `C:\Users\storax\Documents\Standardprojekt\webagent-rs-phase16`. The initial `feat/…` name was renamed before any commit: `scripts/commit-as-agent.ps1` and `taskboard::tests::taskboard_invarianten_halten` only allow `feature|fix|docs|chore|refactor|test` prefixes for new claims. The Play branch `fix/T-976-api-turn-reveal` stays untouched at `3779d47` (pushed; no merge/PR/tag).
- **Board:** Added the nine phase-16 tasks `T-997..T-1005` to `docs/TASKBOARD.json` and to the machine-readable mirror plus a human-readable phase section in `docs/TASKBOARD.md`. Source: the JSON fragment pasted by the owner; `TASKBOARD.refactor.json` was absent from workspace and all git history. Owner-approved renumbering from `T-976..T-984` to `T-997..T-1005` covers `depends_on`, proof paths and verification strings; the historical `T-975` baseline reference stays unchanged. Board now has 109 tasks.
- **Claim:** `T-997` (`Layer ratchet: guard test for module dependency direction and cycles`) is `claimed` by `local/opencode` on `feature/T-997-arch-layers` with `claimed_at=2026-11-26`. `T-998..T-1005` remain `free`.
- **Baseline (measured, not quoted):** `cargo test --lib --quiet` on the unmodified branch base: **1521 passed / 0 failed / 1 ignored** in 57.96 s. This matches the prompt's `T-975` number; the prompt's other figures (89 top-level modules, about 94k lines, five cycles) still need re-measurement inside T-997.
- **Gate for this bookkeeping commit:** `cargo test --lib taskboard::tests::taskboard_invarianten_halten -- --exact` 1 passed; `git diff --check` 0. No source code, Cargo.toml, CLI or protocol change in this commit.
- **Next:** implement only the T-997 scope (`tests/arch_layers.rs`, `docs/arch/layers.toml`, `docs/arch/layer_exceptions.txt`, `docs/arch/LAYERS.md`, `docs/proofs/T-997/README.md`); then run the full four gates, record before/after test counts, push and stop for review. No next task is claimed.

## 2026-11-26 - T-975: Pi-Abdeckung geschlossen

- **Branch:** `chore/T-975-pi-full-coverage`, Basis `master` `c9050f3`. T-975 ist `done` mit `done_at=2026-11-26`; Board und Markdown-Spiegel sind nachgezogen. Direkte Nutzeranforderung: „ja bitte auf 100% hinarbeiten". Keine Runtime-Änderung, kein Portwechsel, kein Eingriff in fremde Pi-Provider.
- **Alle Brains registriert:** Die dynamische Extension `examples/pi/extensions/webagent-models.ts` wurde nach `%USERPROFILE%\.pi\agent\extensions\webagent-models.ts` installiert (Pfad aus der Pi-Dokumentation für auto-discovery/hot-reload). `pi --list-models webagent` liefert exit 0 mit neun Modellen: `auto`, `chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`, `qwen`, `zai`; die letzten beiden ohne Bildunterstützung.
- **Neun echte Turns:** Jedes registrierte Modell wurde einzeln mit `pi --offline --provider webagent --model webagent/<brain> --no-tools --no-session --no-context-files --no-skills --no-prompt-templates --mode json` gegen die dauerhaft laufende Bridge gefahren. Alle neun: exit 0, Marker `WEBAGENT-<BRAIN>-OK-20261126` exakt gefunden.
- **Dialog über die echte Verknüpfung:** `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk` real per `Start-Process` gestartet. Sichtbares Fenster `WebAgent Bridge Start` mit `hwnd=23791118`, `visible=True`, `rect=122,130,1251,765` (Win32 `GetWindowRect`/`IsWindowVisible`). `n{ENTER}` → `optin=False`, `port=False`, `prompt.log` `Antwort=aus`, Fenster nach 15 s automatisch zu. `j{ENTER}` → `optin=True`, `port=True`, `Antwort=start`, `supervisor.log` `Bridge gestartet PID=7000` und gesund.
- **Beleg:** `docs/proofs/T-975/pi-volle-abdeckung.md` mit Katalogtabelle, allen neun Turn-Ergebnissen und den Dialog-Messwerten. Kein Tokenwert, kein `Bearer `, keine Cookies in Datei oder Ausgabe; `auth.json` blieb unberührt, es wurde keine Anmeldung durchgeführt.
- **Gates:** `cargo fmt --all -- --check` 0; `cargo clippy --all-targets --locked -- -D warnings` 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Grenze:** Der Anmelde-Dialog wurde über die echte Verknüpfung und das echte Skript geprüft, nicht durch einen echten Reboot/Re-Login – dafür ist eine gesonderte ausdrückliche Freigabe nötig. Kein Last-, Dauerlast- oder Parallelitätstest, keine neue Browser-Anmeldung, kein Release, kein Tag, kein Deployment.

## 2026-11-26 - T-974: Bridge-Dauerbetrieb mit Anmelde-Abfrage

- **Branch:** `chore/T-974-bridge-autostart`, Basis `master` `d824b10`. T-974 ist `done` mit `done_at=2026-11-26`. Direkte Nutzerentscheidung: "bitte bei windows start terminalfenster mit abfrage ob bridge gestartet werden soll".
- **Bausteine:** `scripts/bridge-start-prompt.ps1` zeigt beim Anmelden ein sichtbares Terminalfenster mit `Bridge jetzt starten? [J/N]`. `J` setzt das Opt-in und startet den Supervisor einmal; `N` entfernt das Opt-in und stoppt eine noch laufende eigene Bridge. `scripts/supervise-bridge.ps1` ist ein idempotenter Health-Check auf `127.0.0.1:8788/health` und startet die Bridge nur bei Bedarf neu; mit `-RequireOptIn` tut er ohne Zustimmung nichts. `scripts/setup-bridge-autostart.ps1` installiert und entfernt Verknuepfung sowie zyklische Aufgabe. Bedienung und Grenzen: `docs/LOCAL_BRIDGE_DAEMON.md`.
- **Installierter Endzustand:** Verknuepfung `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk`, Aufgabe `WebAgent Bridge Health (5min)` (alle 5 Minuten, Interactive/Limited, nur aktueller Benutzer). Opt-in gesetzt, Bridge laeuft, Task-Lauf `LastTaskResult=0`. Kein Admin, kein Fernzugriff, kein Session-0-Dienst.
- **Real geprueft:** Supervisor erster Start und Zweitlauf (`keine Aenderung`) exit 0; Antwort `e` (Nein) stoppte die eigene Bridge und die zyklische Pruefung blieb no-op bei freiem Port; Antwort `j` (Ja) setzte das Opt-in und startete gesund; nach Stopp der eigenen PID startete die zyklische Pruefung die Bridge neu; `setup -Remove` entfernte Verknuepfung, Aufgabe, Opt-in und Bridge und wurde danach neu installiert. Ein echter Pi-Turn gegen die Daemon-Bridge lieferte exit 0 und exakt `WEBAGENT-DAEMON-OK-20261126`. Beleg `docs/proofs/T-974/daemon-abnahme.md`; Secret-Scan: kein Tokenwert, kein `Bearer `.
- **Rueckweg:** `pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/setup-bridge-autostart.ps1 -Remove`. Logs unter `%LOCALAPPDATA%\webagent\bridge\` bleiben als Beleg erhalten und koennen separat geloescht werden. Der Supervisor stoppt ausschliesslich die eigene, per `bridge.pid` und Prozessname verifizierte PID; fremde Prozesse auf Port 8788 fuehren zu Exit 3.
- **Gates erneut gruen:** `cargo fmt --all -- --check` 0; `cargo clippy --all-targets --locked -- -D warnings` 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Grenze:** Der Anmeldedialog wurde anhand von Verknuepfung und Skript geprueft, nicht durch einen echten Reboot/Re-Login. Kein Release, kein Tag, kein Deployment, kein Last-/Paralleltest und kein Monitoring ausser dem 5-Minuten-Health-Check. `auth.json` blieb unberuehrt, es wurde kein bestehendes Secret gelesen oder ausgegeben.
## 2026-11-26 - T-973: lokaler Pi-Provider nutzbar

- **Branch:** `fix/T-973-pi-provider-port-8788`, Basis `master` `4df9d3d`. T-973 ist `done` mit `done_at=2026-11-26`; Board und Markdown-Spiegel sind nachgezogen. Kein Runtime-Code, kein Portwechsel der Bridge, kein neues Protokoll.
- **Zwei echte Fehler behoben:** Die aktiven Pi-Dateien nutzten noch den historisch toten Port `8787` (`examples/pi/models.json`, `examples/pi/extensions/webagent-models.ts`, `scripts/test-pi-provider.ps1`); jetzt überall `8788`. Das Modell-Schema enthielt `"input": ["text","image","audio"]`, das Pi 1.5.2 ablehnt (`("text" | "image")[]`). Korrigiert in der Vorlage und in beiden Snippets in `docs/API_BRIDGE.md`; die Extension filtert Katalog-Modalitys jetzt auf `text`/`image` und fällt auf `["text"]` zurück, damit `auto`/`gemini` nicht registrierungsunfähig werden.
- **Isolierter Smoke:** Mit deploytem `v0.11.7` auf `127.0.0.1:8788` und frischem, nur prozessbezogenem Token: `pwsh -File scripts/test-pi-provider.ps1` → `PASS: Textturn und mindestens fünf Tool-Aufrufe in einem Pi-Auftrag sind grün.` (`PI_BRIDGE_OK`, fünf sequenzielle `read`-Calls). Rohtranskript `docs/proofs/T-973/pi-tool-loop.json`.
- **Nutzerkonfiguration:** `%USERPROFILE%\.pi\agent\models.json` wurde vorher nach `models.json.bak-T973-20261126` gesichert und um den Provider `webagent` ergänzt (`baseUrl=http://127.0.0.1:8788/v1`, `api=openai-completions`, `apiKey=$WEBAGENT_API_KEY`, ein Modell `webagent/chatgpt`). Ein frischer lokaler Loopback-Token wurde als Benutzer-Umgebungsvariable `WEBAGENT_API_KEY` gesetzt, damit Pi und Bridge ihn teilen; der Wert wurde weder ausgegeben noch in einer Datei gespeichert. `auth.json` wurde nicht geöffnet, kein bestehendes Secret gelesen oder kopiert, keine Anmeldung durchgeführt.
- **Echter Nutzer-Turn:** `pi --list-models webagent` zeigt `webagent/chatgpt`. Mit derselben Konfiguration lieferte ein echter Textturn gegen die Bridge exit 0 und exakt `WEBAGENT-PI-USER-OK-20261126`; Beleg `docs/proofs/T-973/pi-user-config-turn.md`. Secret-Scan der beiden Belegdateien: kein Tokenwert, kein `Bearer `, keine Cookies; der Name `WEBAGENT_API_KEY` erscheint nur als Variablenname.
- **Gates erneut grün:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets --locked -- -D warnings` exit 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` exit 0. Beide Bridges wurden gestoppt, Port `8788` ist frei.
- **Grenze / offen:** Kein Release, kein Tag, kein Deployment aus dieser Scheibe, kein Dauerbetrieb/Autostart und kein Fernzugriff. Der Provider ist manuell startbar; der Befehl `webagent api serve --brain chatgpt --headless --port 8788` benötigt die gesetzte `WEBAGENT_API_KEY`. Für alle acht Brains bleibt die dynamische Extension `examples/pi/extensions/webagent-models.ts` der dokumentierte Weg; sie wurde nicht in die Nutzerinstallation kopiert.
## 2026-11-26 - Release v0.11.7, lokales Deployment und Live-Turn-Abnahme

- **Freigabe:** Der Eigentümer hat PR/Merge/Deployment und einen zusätzlichen Live-Turn freigegeben; die Anmeldung lag bereits vor. Der Agent hat keine Zugangsdaten gelesen, keine Anmeldung durchgeführt und keine Profil-/Token-Dateien geöffnet.
- **Release:** Branch `release/v0.11.7-hardening` mit Versionsbump `0.11.6` → `0.11.7` in `Cargo.toml`/`Cargo.lock`, CHANGELOG und `done_at`-Korrektur für T-969..T-972 (der Board-Invariantentest war nach dem `done`-Setzen kurz rot). PR [#112](https://github.com/st0rax/webagent-rs/pull/112) mit vier grünen Checks squash-gemergt (`d54d347`). `verify-release-version.ps1 -Tag v0.11.7` → Exit 0, `-Tag v0.11.6` → Exit 1. Tag `v0.11.7` gepusht; Workflow [`45725669798`](https://github.com/st0rax/webagent-rs/actions/runs/45725669798) vollständig grün inkl. `github-release`, sieben Assets.
- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --lib --locked` (1521 passed / 0 failed / 1 ignored) und `git diff --check` grün.
- **Deployment:** Exakt die Release-Assets aus `v0.11.7` nach `webagent-deploy-target\release\` kopiert; EXE-SHA-256 `0f45d96377857220ab7d25eda44bfe3f6e8886e6bb01136136d5d2dcfd973e84`, DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`, alle sieben Asset-Digests geprüft. Die `v0.11.6`-Dateien liegen unter `v0.11.6-backup\` als Rückweg (`v0.11.5-backup\` bleibt erhalten). `--version` → `webagent 0.11.7 (d54d34755)`, `canary` 8/8 `ok`, `brains-health` 8/8 `selectors=ok` (`bot2bot=MISSING`, informativ).
- **Provider-Rolle:** `api serve --brain chatgpt --headless --port 8788` mit prozessonly Token: `GET /health` 200, `GET /v1/models` mit Bearer 9 Modelle (`webagent/auto` zuerst), ohne Bearer 401. Ein echter `POST /v1/chat/completions` lieferte `PROVIDER-OK-20261126` (`finish_reason=stop`, `chatcmpl-18eb7029b20cca24`); Beleg in `docs/proofs/live-turn-2026-11-26/provider-chat-turn.md`. Prozess gestoppt, Port 8788 frei.
- **Live-Turn-Abnahme:** Deploytes `v0.11.7`, sichtbares WebView2-Fenster, `cloud web-view-stream --brain chatgpt --prompt … --net-trace` → Exit 0, Token exakt `LIVE-OK-20261126-V0.11.7`, `oauth_session=true`, `api_key=false`, 200-SSE-`provider_response` (602 ms). Rohbeleg plus Secret-Scan und Grenzen unter `docs/proofs/live-turn-2026-11-26/`. Ein gleicher Preflight mit `v0.11.6` und `--headless` lieferte `LIVE-OK-20261126`.
- **Bewertung:** `docs/PRODUCTION_READINESS.md` auf `v0.11.7` und Zielbetrieb umgeschrieben: Go für den lokalen Rechner mit Loopback-Provider, No-Go für Fern-/Produktivbetrieb ohne Monitoring/Last/Risikoakzeptanz. Restrisiken sind fünf Punkte, darunter ein Turn/ein Brain und die interne `Buffer()`-Deprecation im Publish-Job.
## 2026-11-26 - Vier Härtungsscheiben auf `master` zusammengeführt

- **Freigabe:** Der Eigentümer hat am 2026-11-25 „absolute freigabe für alle sinnvollen vorgänge wie PR und merge etc" erteilt. Deployment und Live-Turn wurden später separat freigegeben.
- **PRs und Checks:** [#108](https://github.com/st0rax/webagent-rs/pull/108) (T-969) wurde als Squash `d60a6c0` über GitHub gemergt. Für [#109](https://github.com/st0rax/webagent-rs/pull/109), [#110](https://github.com/st0rax/webagent-rs/pull/110) und [#111](https://github.com/st0rax/webagent-rs/pull/111) verweigerte GitHub den direkten Squash-Merge wegen Doku-Konflikten. Alle acht Check-Jobs auf den Retry-SHAs `17c0adb`, `e3ddb12`, `1c71e87`, `b3b23da7` waren zuvor `completed/success`.
- **Lokale Squash-Commits:** `a161c8b` (T-970), `9d2692d` (T-971), `c2ff017` (T-972), jeweils mit PR-Referenz in der Commit-Message, auf `master` gepusht. GitHub erkennt manuell gepushte Squashes nicht als PR-Merge; die drei PRs wurden mit einem nachvollziehbaren Kommentar geschlossen. Kein Force-Push, keine History-Umschreibung.
- **Konfliktauflösung:** Nur `docs/CURRENT_WORK.md` und `docs/TASKBOARD.json` waren betroffen. Alle vier Doku-Abschnitte bleiben erhalten; die Tafel behält die Master-Version und übernimmt pro Task nur den branchspezifischen Diff. Kein Code-Konflikt, kein Logikverlust.
- **Gates auf dem zusammengeführten Stand:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1521 passed / 0 failed / 1 ignored; `git diff --check` exit 0. `master` und `origin/master` stehen auf `c2ff017`.
- **Tafel:** T-969 bis T-972 sind jetzt `done`, der Markdown-Spiegel ist nachgezogen. Offen bleiben die Release-/Deploy-Scheibe und die Live-Turn-Abnahme mit vorhandener Nutzer-Sitzung.
## 2026-11-25 - T-972: Controller und WebView-Runtime aufgeteilt

- **Branch:** `refactor/T-972-controller-webview-split`, Basis `23c10e9` (Claim-Commit). Nur lokale Änderungen; kein Merge, kein `done`.
- **Ausgangslage:** Entgegen der geclaimten Schätzwerte hatte `src/controller.rs` 3349 und `src/webview_runtime.rs` 3315 Zeilen. Beide liegen jetzt bei 1909 bzw. 2229 Zeilen.
- **Änderung (reine Bewegung):** `controller::tests` → `src/controller/tests.rs` (1439 Zeilen); `webview_runtime::tests` → `src/webview_runtime/tests.rs` (95 Zeilen); die kohärenten WebView2-Datei-Upload-Helfer (Chooser, FileList-Fallback, Remount-Watch, CDP-Setzen, MIME) → `src/webview_runtime/file_input.rs` (996 Zeilen). Nur `set_file_input_files_cdp` wurde für den bestehenden Aufrufer `pub(super)`; alle anderen Signaturen, `cfg(windows)`/`cfg(not(windows))`-Pfade und das Verhalten bleiben unverändert. Kein neues Modul mit eigener Verantwortung außer der Aufteilung.
- **Gleichheit geprüft:** Testzahl vorher/nachher Controller 44 / Controller 44, WebView 6 / WebView 6; `cargo test --lib` 1517 passed / 0 failed / 1 ignored — identisch zur Basis.
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1517 passed / 0 failed / 1 ignored; `git diff --check` exit 0.
- **Grenze:** Kein Login, kein WebView2-Fensterstart, kein Live-Provider-Call, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - T-971: VerifiedFree ehrlich als Registry-Beleg gekennzeichnet

- **Branch:** `fix/T-971-verifiedfree-proof`, Basis `23c10e9` (Claim-Commit). Nur lokale Änderungen; kein Merge, kein `done`, kein Live-Provider-Call.
- **Befund:** `AccessMode::VerifiedFree` ist der einzige automatisch routbare Free-Status, hatte aber keinen Bezug zum einen Beleg-Gate. Der Live-Nachweis liegt als datiertes Markdown unter `docs/proofs/T-926/webview-oauth-live-result.md` (2026-11-21), nicht als Record in `capability/proofs.jsonl` (dort 0 Treffer auf `free|cloud|webview|stream|oauth`).
- **Entscheidung:** Variante A — ehrliche Kennzeichnung ohne Store-Schreibzugriff und ohne Verhaltensänderung. Neu: `VerificationBasis` und `verification_basis()` in `src/free_cloud_chat.rs` (`kind = registry_only_dated_doc`, `at = 2026-11-21`, `proof_path = docs/proofs/T-926/...`, immer `capability_gate = false`); lokale Mock-/Hub-Verträge werden als `local_contract` ausgewiesen. `cloud list` (indexgleich zu `models`) und `cloud decide` geben das Feld `verification` aus. `decide_route`, `ProofState`, TTL und Store-Schema bleiben unangetastet; `src/capability_proof.rs` wurde bewusst nicht geändert. Kein nachträgliches Einfügen eines 2026-11-21-Records.
- **Beleg:** Zwei neue Unit-Tests prüfen die Herkunftsangabe und die unveränderte Routing-Invariante sowie `ProofState::Never` auf einem frischen, unberührten Store-Pfad.
- **Doku:** `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` nennt den Unterschied zwischen Registry-Beleg und Capability-Gate ausdrücklich.
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1519 passed / 0 failed / 1 ignored; `git diff --check` exit 0. Lokaler CLI-Smoke `cloud decide --model-id webagent/webview-oauth-session` exit 0 mit `capability_gate=false`, `kind=registry_only_dated_doc` und datiertem `proof_path`.
- **Grenze:** Kein Login, keine Profil-/Token-Lektüre, kein zweiter Belegspeicher, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - T-970: Turn-Beobachtungen thread-lokal isoliert

- **Branch:** `fix/T-970-observation-scope`, Basis `23c10e9` (Claim-Commit). Nur lokale Änderungen; kein Merge, kein `done`, kein Live-Lauf.
- **Ursache:** `src/brain_score.rs` hielt `PENDING_TURN`, `PENDING_WEB_SEARCH` und `PENDING_SOURCE_HINTS` als prozessglobale Mutexe. `record_event_at` konsumierte alle drei bei jedem Event, auch bei Early Returns ohne eigenen Turn. Parallele Läufe (Bridge pro Verbindung, Swarm, REPL-Pool, Benchmark) konnten sich gegenseitig überschreiben; `brain_id` allein wäre als Schlüssel nicht ausreichend.
- **Änderung:** Die drei Mutexe sind durch ein `thread_local!`-`RefCell<PendingObservations>` ersetzt. Ein `PendingScope` (RAII) nimmt beim Laufbeginn vorhandene Reste beiseite und stellt sie beim Verlassen wieder her; verschachtelte Scopes sind sicher. Guards an den Laufgrenzen: `src/relay.rs` (`relay_image_generation`, `relay_single_turn_checked`, `run_turn_loop`) und `src/repl/pool.rs` (`isolated_query`, `PersistentBrain::query`). Alle öffentlichen `set_pending_*`/`update_pending_turn`/`pending_turn_snapshot`-Signaturen und das Event-Format bleiben unverändert; `WRITE_LOCK` schützt weiterhin nur das Datei-Append.
- **Beleg:** Zwei neue Unit-Tests: `t970_parallelaeufe_teilen_keine_beobachtungen` (zwei Threads, getrennte Websuch-/Quellenhinweise) und `t970_pending_scope_verhindert_reste_aus_frueheren_laeufen` (Reste aus früherem Lauf werden nicht in den frischen Scope gezogen und nach Ende wiederhergestellt).
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1519 passed / 0 failed / 1 ignored; `git diff --check` exit 0.
- **Grenze:** Kein Umbau von Proof-Gate, Brain-Score-Formel oder Event-Schema; kein Login, keine Profil-/Token-Lektüre, kein Live-Provider-Call, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - Greenfield-Abgleich: vier lokale Härtungsscheiben geclaimt

- **Auftrag:** Der Eigentümer hat nach dem Prompt-Abgleich "optimier das projekt soweit möglich" freigegeben. Der ursprünglich zu breit gefasste T-969 wurde auf ausdrücklichen Wunsch in vier Tasks mit je eigenem Branch und Typ zerlegt: **T-969** `docs/T-969-protocol-security` (PROTOCOL.md + SECURITY.md), **T-970** `fix/T-970-observation-scope` (globale Turn-Puffer), **T-971** `fix/T-971-verifiedfree-proof` (Belegpfad), **T-972** `refactor/T-972-controller-webview-split` (God-Files).
- **Claims:** Alle vier stehen in `docs/TASKBOARD.json` auf `claimed`, Eigentümer `local/opencode`, Basis `master` = `1c93cfe`; der Markdown-Spiegel ist nachgezogen. `done` wird erst nach Merge gesetzt. Die Claim-Zeilen werden in einem gemeinsamen Basis-Commit festgehalten, damit jeder der vier Branches dieselbe Tafel trägt.
- **Arbeitsstand:** Aktuell läuft T-969 auf `docs/T-969-protocol-security`. T-970 bis T-972 sind geclaimt, aber noch nicht begonnen; die Hintergrund-Recherche zu Puffern und Beleg-Gate läuft separat.
- **T-969 (Doku, auf Branch):** `docs/PROTOCOL.md` und `docs/SECURITY.md` neu, aus Parser, Shell-Policy, Datei-Grenzen, Bridge-Auth, Redaktion, Run-Store und dem einen Proof-Gate abgeleitet. Beide tragen ein `**Referenz**`-Banner, damit der Betriebs-Dokumentationstest in `src/startup.rs` grün bleibt. Gates real gemessen: `cargo fmt --all -- --check` exit 0, `cargo clippy --all-targets -- -D warnings` exit 0, `cargo test --lib` 1517 passed / 0 failed / 1 ignored, `git diff --check` exit 0. Kein Merge, kein `done`.
- **Grenzen unverändert:** Kein Login, keine CAPTCHA-/2FA-Bedienung, keine Lektüre von Profilen, Cookies oder Tokens, kein Live-Provider-Call, kein Tag, kein GitHub-Release, kein Deployment. Die vier Scheiben enden mit lokalen Gates und einem Push auf ihre Arbeitsbranches; ein Merge in `master` braucht weiterhin eine eigene Freigabe.

## 2026-11-25 - Release v0.11.6 mit Canary-Fix ausgeliefert

- **Freigabe:** Der Eigentümer hat mit „kümmer dich drum" die eigenständige Auslieferung des bereits gemergten Canary-Fix per Patch-Release übernommen. Ein Produktiv-Deployment blieb nicht freigegeben; gesucht wurde dennoch in Diensten, geplanten Tasks und allen Laufwerken – kein Ziel gefunden.
- **PR [#107](https://github.com/st0rax/webagent-rs/pull/107):** Branch `release/v0.11.6-canary`, Versionsbump `0.11.5` → `0.11.6` in `Cargo.toml`/`Cargo.lock`, CHANGELOG und `docs/RELEASE.md` nachgezogen. Alle vier PR-Checks grün; Squash-Merge `1149dff` auf `master`.
- **Lokale Gates:** `cargo fmt --all -- --check`, `cargo build --locked`, `cargo test --lib --locked`, `cargo clippy --all-targets -- -D warnings`, `git diff --check` grün. `verify-release-version.ps1 -Tag v0.11.6` → Exit 0, `-Tag v0.11.5` → Exit 1 (Mismatch erwartet).
- **Tag und Publish:** Tag `v0.11.6` auf `1149dff` gesetzt und gepusht. Release-Workflow [`45702611313`](https://github.com/st0rax/webagent-rs/actions/runs/45702611313) vollständig grün inkl. `github-release`; sieben Assets, veröffentlicht 21:21:55 UTC, nicht Draft/Prerelease. Alle Hash-Dateien und Asset-Digests geprüft.
- **Publish-Pfad real belegt:** `download-artifact@v8` und `action-gh-release@v3` im Publish-Job ausgeführt und grün; damit ist der zuvor nur statisch geprüfte Pfad nachgewiesen. Eine interne `[DEP0005] Buffer() is deprecated`-Meldung des Node-Runtime wurde dokumentiert; keine `Node.js 20`-Action-Warnung.
- **Lokales Ziel nachgezogen:** `webagent-deploy-target\release\` enthält jetzt die `v0.11.6`-Assets (EXE-SHA256 `568c5001941ef30df7dbe98f7e9c3a0f14ed8e147bff8458e84d1a3d47f8d5d3`, DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`). Die `v0.11.5`-Dateien liegen unter `v0.11.5-backup\` als Rückweg. `--version` → `0.11.6 (1149dff2f)`; `canary` → Exit 0, 8/8 `ok` (vorher 7/8 Fehlalarm); `brains-health` → Exit 0, 8/8 `selectors=ok`, `bot2bot=MISSING`; UI-Smoke `GET /` und `GET /health` → 200, Prozess gestoppt, Port 8788 frei.
- **Bewertung:** `docs/PRODUCTION_READINESS.md` auf `v0.11.6` umgeschrieben. Zwei frühere Risiken (Canary-Fehlalarm, ungetesteter Publish-Pfad) sind geschlossen; fünf bleiben. Release-Reife ja, Produktivreife im Zielbetrieb weiter nicht belegt – kein Produktivziel, kein echter WebView2-Fensterstart/Live-Turn, keine bewusste Restrisiko-Akzeptanz.

## 2026-11-25 - Produktreife-Bewertung und Canary-Korrektur

- **Bewertung:** `docs/PRODUCTION_READINESS.md` trennt Release-Reife (ja) von Produktivreife im Zielbetrieb (nicht vollständig belegt) und nennt sechs konkrete Restrisiken sowie Go/No-Go-Kriterien.
- **Runtime-Smoke mit dem deployten Binary:** `webagent.exe ui --no-open --skip-startup-reconcile --port 8788` → Port nach 1 s im Listen-Zustand, `GET /` 200 (38358 Byte), `GET /health` 200 (31 Byte); Prozess gestoppt, Port danach frei. `webagent.exe brains-health` → Exit 0, `shared_profile=ok`, 8/8 `selectors=ok`, `bot2bot=MISSING`.
- **Canary-Befund:** Dasselbe Binary meldete `canary` 7/8 `missing_selectors`, obwohl `load_selectors` eingebettete Selektoren korrekt lädt und `brains-health` sie mitzählt. Ursache: die leichte Probe in `src/canary.rs` prüfte nur die Datei am Build-Pfad. Korrektur plus Unit-Test (`selectors_available`, eingebettete Kopie gilt als vorhanden) sind mit PR [#106](https://github.com/st0rax/webagent-rs/pull/106) (`57584a8`) nach `master` gemergt; alle lokalen Gates und die vier PR-Checks grün. Die Korrektur ist mit `v0.11.6` ausgeliefert (siehe Abschnitt oben).
- **Grenze:** Publish-Pfad mit den neuen Actions wurde nicht erneut ausgeführt (taggebunden), kein echter WebView2-Fensterstart, kein Produktivziel. Deployment weiterhin nur lokal.

## 2026-11-25 - Lokales Deployment von v0.11.5

- **Freigabe und Ziel:** Der Eigentümer hat Deployment freigegeben und um einen Zielvorschlag gebeten. Im Workspace gibt es kein `delivery/deploy_webagent_rs.ps1`, kein `runtime-workers/` und kein `webagent-deploy-target/`; `webagent` ist weder im PATH noch als Prozess vorhanden. Gewählt wurde daher der lokale, klar benannte Zielordner `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\`. **Kein Fern- oder Produktiv-Deployment**, keine PATH-Änderung, kein Prozessstart, keine Zugangsdaten.
- **Quelle und Prüfung:** Exakt die GitHub-Release-Assets `v0.11.5` (kein lokaler Rebuild). EXE-SHA256 `a1baa187bd435e4902beb1b519df7398029212e3b68f33e12940ca6dad4d562c` und DLL-SHA256 `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f` stimmen mit den Release-Digests überein; nach dem Kopieren erneut geprüft.
- **Post-Deploy-Check:** `webagent.exe --version` → `webagent 0.11.5 (9b2ad7e6e)`, Exit 0; `WebView2Loader.dll` liegt neben der EXE. Die `.sha256`-Datei des Release liegt ebenfalls im Zielordner.
- **Rückbau:** Zielordner löschen; sonst keine Systemänderung. Ein Produktiv-Deployment bleibt weiterhin eine gesonderte Entscheidung mit eigenem Ziel.

## 2026-11-25 - CI-Warnungen nach dem Release bereinigt

- **Anlass:** `actions/checkout@v4`, `actions/cache@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4` und `softprops/action-gh-release@v2` erzeugten Node-20-Deprecation-Warnungen. Der veröffentlichte Release blieb unverändert.
- **Änderung:** PR [#105](https://github.com/st0rax/webagent-rs/pull/105) (`c1f01f0`) hebt auf checkout v7, cache v6, upload-artifact v7, download-artifact v8 und action-gh-release v3. Alle vier PR-Checks grün; YAML lokal geparst, lokale Gates grün. Zwei veraltete T-961-Belegsätze wurden auf den veröffentlichten Release nachgezogen.
- **Verifiziert:** Release-Workflow per `workflow_dispatch` auf `master` ([Lauf `45690875482`](https://github.com/st0rax/webagent-rs/actions/runs/45690875482)) vollständig grün; `Copy WebView2Loader.dll` weiterhin ok, `github-release` wie erwartet übersprungen (kein Tag). Im Log des Laufs: 0 Zeilen mit `Node.js 20` oder `deprecated`. Tag `v0.11.5` zeigt weiter auf `9b2ad7e`; GitHub-Release unverändert mit 7 Assets.
- **Grenze:** Der Publish-Job mit den neuen Action-Versionen wurde nicht ausgeführt — dafür wäre ein neuer Tag nötig, der nicht freigegeben ist. Die verwendeten Inputs (`generate_release_notes`, `files`, `merge-multiple`, `path`) wurden statisch gegen die `action.yml` der neuen Versionen geprüft. Zu diesem Zeitpunkt kein Deployment; siehe Abschnitt zum lokalen Deployment oben.

## 2026-11-25 - Release v0.11.5: Windows-DLL-Pfad korrigiert

- **Fehler:** Der Tag `v0.11.5` auf `8d7eae3` startete den Release-Workflow [`45680524253`](https://github.com/st0rax/webagent-rs/actions/runs/45680524253). `verify`, Android und Linux waren grün, der Windows-Build lief durch; `Copy WebView2Loader.dll` scheiterte mit `release: WebView2Loader.dll nicht unter build/webview2-com-sys-*/out/x64 gefunden` (`scripts/copy-webview2-loader.ps1:47`). Der `github-release`-Job wurde nicht ausgeführt: kein GitHub-Release, keine Assets.
- **Ursache:** Mit Rust/Cargo 1.100 liegt `OUT_DIR` unter `target/<profile>/build/webview2-com-sys/<hash>/out`, nicht mehr wie früher unter `build/webview2-com-sys-<hash>/out`. Das Skript kannte nur das alte Layout. Belegt durch die lokale Datei `target/debug/build/webview2-com-sys/dc268415efa03ec9/out/x64/WebView2Loader.dll` und den CI-Log.
- **Fix:** `Find-Loader` in `scripts/copy-webview2-loader.ps1` prüft jetzt beide Layouts; die Fehlermeldung nennt beide Pfade. Lokal geprüft: `-Profile debug` gegen das neue Layout → `[ok]`, simuliertes altes Layout mit `-Profile release` → `[ok]`. Keine Code-, Versions- oder Workflow-Änderung.
- **Freigabe:** Der Eigentümer hat am 2026-11-25 ausdrücklich erlaubt, den bereits gepushten Tag `v0.11.5` nach dem Fix auf den neuen Commit zu legen. Deployment bleibt ausgeschlossen.
- **Abschluss:** Der Fix wurde als `9b2ad7e` nach `master` gepusht. Der Remote-Tag `v0.11.5` wurde mit ausdrücklicher Freigabe gelöscht und auf `9b2ad7e` neu gesetzt (alter Tag: `8d7eae3`). Der neue Release-Workflow [`45684483569`](https://github.com/st0rax/webagent-rs/actions/runs/45684483569) ist vollständig grün: `verify`, Android, Linux, Windows (`Copy WebView2Loader.dll` jetzt grün) und `github-release`. Das [GitHub-Release](https://github.com/st0rax/webagent-rs/releases/tag/v0.11.5) ist am 25.11.2026 19:28:56 UTC veröffentlicht, nicht Draft/Prerelease, mit sieben Assets: Windows-EXE, `WebView2Loader.dll`, Linux-, Android-Binary und drei `.sha256`-Dateien. Die drei Hash-Dateien wurden heruntergeladen und stimmen mit den Asset-Digests überein. Kein Deployment.

## 2026-11-25 - T-961/G-002: Endabnahme des engen Frisch-Chat-Retries

- **Freigabe:** Der Eigentümer hat am 2026-11-25 zunächst die autonome Fortführung „bis einschließlich endabnahme" freigegeben. Die vorherige Entscheidung „noch nicht abnehmen" und die vom 2026-11-21 („Keine Änderung") bezogen sich auf frühere konkrete Strategien und sind damit ersetzt. Anschließend hat er separat den vollen Ablauf **PR + Merge + Release `v0.11.5`** freigegeben; Deployment ist weiterhin nicht enthalten.
- **Branch und Eigentümerschaft:** `fix/T-961-semantic-denial-retry`, abgezweigt von `master` bei `7481b02`. Code-Commits `c6d8493`, `0e00682`, `b7d8ba4`, später `ee33b33` und `08f05b6`, gepusht und mit `origin` synchron. `master` stand zu diesem Zeitpunkt bei `7481b02`; dieser Scheiben-Commit führte die finalen Belege und Statusdokumente auf dem Branch nach. Eigentümerschaft `local/opencode`.
- **Code:** `src/api_bridge/content.rs`, `src/api_bridge/inference.rs`, `src/api_bridge/provider_handlers.rs`, `src/browser_inference.rs`. Erkennung nur bei „Werkzeugbegriff + Verneinung + Verfügbarkeits-/Zugriffsbegriff"; genau **ein** Retry im frischen Chat mit dem unveränderten ursprünglichen Auftrag plus den bisherigen Tool-Ergebnissen als Daten. Kein Korrektur-Meta, keine `required`-Eskalation. Erneute Verweigerung wird ehrlich gemeldet (`Retry erschoepft`), kein stilles PASS. Fixtures in `src/api_bridge/tests.rs` und `src/browser_inference.rs`. Doku: `docs/API_BRIDGE.md`.
- **Gates real gemessen:** `cargo fmt --check`, `cargo build`, `cargo test --lib` (1516 passed / 0 failed / 1 ignored), `cargo clippy --all-targets -- -D warnings`, `git diff --check` grün. Ein einmaliges `web_ui::tests::api_rolle_zugriff_mit_token_liefert_modellliste` scheiterte bei laufendem Bridge-Prozess an der Port-Kollision und lief isoliert sowie im Rerun grün.
- **Kausale Live-Belege (ohne neues Login, nur bestehende chatgpt-Sitzung, prozess-only Token, ohne `--auto-tool-choice-required`):** 14 strict-clean Pi-Läufe mit je 5/5 Tool-Calls und Nonces (5–10, 12–17, 19–20), `stopReason_error 0`, Fehlermarker 0, Relay-Korrekturen 0. In **Lauf 10** und **Lauf 12** wurde die semantische Verweigerung live ausgelöst; `t961_fresh_chat_retries=1`, danach jeweils bis `call_5` weitergearbeitet, `STRICT_RESULT=PASS`. Transkript-SHA256: `0F022DF4EB9689FB30BC52E7E5C3D625B0B035EAB48F5419248F511DA18288E5` (10) und `8CC2BDB4C6955DD2CFAF839248043AA0233857BFF4BF8EE31D6C83F03DF9E198` (12). `t961_retry_exhausted=0` in allen Läufen; rohe Transkripte, Bridge-Logs, Strict-Ausgaben und Negativläufe unter `docs/proofs/T-961/semantic-denial-retry/`, Secret-Scan sauber.
- **Ehrliche Grenze:** Die Ursache des Modellverhaltens bleibt nicht isoliert; keine Kontoeinstellung wurde geprüft oder geändert. Negativlauf 11 (4/5) und 18 (1/5) sind vorzeitiger Textabschluss ohne Verweigerung, Lauf 21 (0/5) ein `ABSENDEKNOPF_DEAKTIVIERT`-Sendefehler mit automatischem Circuit-Breaker (`open_until 1795630874`, nicht manuell zurückgesetzt). Diese Klassen werden vom Retry-Pfad nicht adressiert; der Mehrschritt-Loop bleibt nicht deterministisch.
- **Abnahme:** T-961 ist in `docs/TASKBOARD.json` und `docs/TASKBOARD.md` auf `done` gesetzt, G-002 in `GOALS.md` geschlossen. Die Detailbegründung steht in `docs/proofs/T-961/semantic-denial-retry/README.md`. Merge, Tag und GitHub-Release `v0.11.5` sind freigegeben; Deployment bleibt ausgenommen.

## 2026-11-25 - T-961 via PR #104 gemergt, Release v0.11.5 vorbereitet

- **Freigabe:** Der Eigentümer hat nach der Endabnahme ausdrücklich den vollen Ablauf **PR + Merge + Release** freigegeben; Deployment ist weiterhin ausgeschlossen.
- **PR/CI:** PR [#104](https://github.com/st0rax/webagent-rs/pull/104) wurde gegen `master` geöffnet. Alle vier Checks waren grün (`aarch64`, `build & test`, Windows volle Features, Linux-WebView). Der Squash-Merge ist `ea88203`.
- **Versionsgate lokal:** `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` → Exit 0; `v0.11.4` → Exit 1 wie erwartet. `Cargo.toml` und `Cargo.lock` stehen auf `0.11.5`.
- **Arbeitsstand zu diesem Zeitpunkt:** `master` war bei `ea88203`, Working Tree sauber. Tag und GitHub-Release `v0.11.5` wurden danach nach grüner Master-CI gesetzt; `release.yml` erzeugte die Artefakte und das GitHub-Release selbst. Kein Deployment.

## 2026-11-25 - T-968 via PR #103 nach `master` gemergt

- **Freigabe und Merge:** Der Eigentuemer hat den Merge ausdruecklich freigegeben.
  PR #103 (https://github.com/st0rax/webagent-rs/pull/103) wurde mit
  `a2a144a` als Squash nach `master` gemergt; alle vier PR-Checks waren vorher
  gruen. `master` lokal und remote steht auf `a2a144a`, kein Tag, kein
  GitHub-Release, kein Deployment. Board zu diesem Zeitpunkt: 93 Tasks /
  92 `done` / 1 `free` (T-961) / 0 `claimed`; `GOALS.md`,
  `docs/OVERVIEW.md` und `CHANGELOG.md` waren auf diesen Stand nachgezogen.
- **Damalige Ausnahme:** T-961/G-002 war zu diesem Zeitpunkt `free`; die
  damaligen rohen Pi-Transkripte waren negativ (0/5 Tool-Turns,
  `session_state=Unbestimmt`). Kein erfundenes PASS und keine erneute
  Anmeldung durch einen Agenten. Die spätere Endabnahme steht im Abschnitt
  oben; `master` ist dadurch unverändert geblieben.

## 2026-11-25 - T-968: kimi `reasoning_effort` live belegt (umgesetzt im Worktree)

- **Fortsetzung von DeepSeek's Claim:** auf `b073d26` im Worktree
  `webagent-rs-t968-kimi-effort`, Branch `fix/T-968-kimi-effort-roundtrip`.
  Dessen uncommittete Arbeit (`click_key_chain`, `--open`-Ketten-Doku, Surveys)
  wurde uebernommen und weitergefuehrt, nicht verworfen.
- **Ursache geklaert:** die beiden alten Surveys waren byte-gleich — das
  Untermenue wurde nie geoeffnet. `reasoning_effort_menu` las nur den Modellnamen
  `Schnell`; `reasoning_effort_option` traf mit `[role='menuitem']` /
  `#v-3-menu-trigger` erneut den Trigger. Der rohe Hover-Survey zeigt
  `aria-expanded=true` und die echten Eintraege
  `[data-testid='model-effort-option'][role='menuitemradio']` = `Standard`,
  `Hoch`. Zusaetzlich lag ein Nutzer-Override ausserhalb des Repos obenauf, der
  den Trigger als Option definierte; nur dieser Key wurde korrigiert, das
  Original liegt als `kimi.json.bak-T968-20261125` im selben Verzeichnis.
- **Live-Beleg (kein neues Login):** `webagent --skip-startup-reconcile verify
  --brain kimi --cap reasoning_effort --headless` → **Passed, 10054 ms**:
  `Schnell Hoch` → `Standard`, unabhaengig gelesen `Schnell Standard`,
  Rueckweg=true. Proof-Store `ts 2026-11-25T02:18:35Z`, `outcome=passed`,
  `selector_hash=2705021080`. Der ehrliche Fehlversuch mit altem Override ist
  ebenfalls gespeichert. Alles roh unter `docs/proofs/T-968/README.md`.
- **Code:** `src/browser/{js,operations,ui,verify}.rs`, `src/brain_probe.rs`,
  `src/cli.rs`, `selectors/kimi.json` — Hover-/Trusted-Click-Pfad, echte
  Optionsliste aus dem Live-DOM, Roundtrip mit unabhaengiger Nacherfassung und
  Wiederherstellung; kein zweiter Beleg-Gate.
- **Gates real gemessen:** `cargo fmt`, `cargo build`, `cargo test --lib`
  (1512 passed / 0 failed / 1 ignored), `cargo clippy --all-targets --
  -D warnings`, `git diff --check` alle gruen. Matrix `effort/kimi` nur aus dem
  rohen Lauf auf `passed` gesetzt; Board zu diesem Zeitpunkt 93 Tasks /
  92 `done` / 1 `free` (T-961) / 0 `claimed`, MD-Spiegel synchron.
- **Grenzen:** PR #103 wurde mit ausdruecklicher Freigabe geoeffnet und nach
  gruenen Checks (aarch64, build & test, Linux-WebView, Windows) als Squash
  `a2a144a` nach `master` gemergt; seither kein Tag, kein GitHub-Release, kein
  Deployment. Keine Zugangsdaten, Profile, Cookies oder Tokens gelesen, keine
  Kontoeinstellung geaendert. T-961 war zu diesem Zeitpunkt `free`; die
  spaetere Endabnahme steht im Abschnitt oben. Der Branch wurde als
  `88918df` (Umsetzung), `141736b` und `4da55d9` (Handoff) committet und
  gepusht; die Uebergabe enthaelt Pfad, Zweck und Eigentuemerschaft
  (`local/opencode`).

## 2026-11-25 - Projektabschluss im erreichbaren Umfang

- **Entscheidung:** Der Eigentuemer hat klargestellt, dass er sich nicht erneut
  anmelden wird. Es wird deshalb **kein** weiterer Live-Lauf und keine
  Anmeldungs-, Einmalcode- oder CAPTCHA-Bedienung durch einen Agenten verlangt.
- **Stand:** T-967 ist via PR #102 (`c0890d5`) auf `master` gemergt; alle
  PR- und Master-CI-Laeufe sind grün; `cargo test --all-targets --locked`
  zusaetzlich grün (1505 + 7 passed). Board zu diesem Zeitpunkt: 92 Tasks /
  91 `done` / 1 `free` (T-961), kein `claimed`.
- **Damalige Ausnahme:** G-002/T-961 war zu diesem Zeitpunkt `free`. Die
  damaligen rohen Pi-Transkripte sind negativ (0/5 Tool-Turns,
  `session_state=Unbestimmt`); daraus wurde kein PASS erfunden. Die spaetere
  Endabnahme mit kausalem Retry-Beleg steht im Abschnitt oben.
- **Grenzen:** kein neuer Tag, kein GitHub-Release, kein Deployment. Latest
  Release bleibt `v0.11.4`; der historische Tag ist unangetastet.
- **DeepSeek-Review (01:33 Uhr):** der Child-Agent war wieder verfuegbar und
  hat den gemergten Stand read-only mit `review ok` bewertet. Zwei minimale
  Befunde eingearbeitet: Taskboard-Branch/Handoff auf den gemergten Stand
  korrigiert und `--locked` auch in die drei Release-Builds sowie den
  Android-Build ergaenzt. Der Android-Target ist lokal ohne NDK nicht
  nachgefahren; CI prueft ihn. Kein Logikfehler am Gate gefunden.

## 2026-11-24 — T-967 unabhaengig im getrennten Worktree umgesetzt

- **Parallele Arbeit:** DeepSeek arbeitet auf `chore/T-967-release-gate`; zur
  Vermeidung von Dateikonflikten liegt diese Umsetzung im Worktree
  `webagent-rs-t967-opencode` auf `chore/T-967-release-gate-opencode` (ab
  `b7d75fd`). Der Versuch, einen neuen DeepSeek-Child-Agenten zu starten,
  scheiterte mit `Free usage limit reached` (Reset laut Eigentuemer 1:31); von
  diesem Child wurde nichts ausgefuehrt. Eine laufende DeepSeek-Session kann
  nicht von hier gesteuert werden.
- **Implementierung:** `scripts/verify-release-version.ps1` (fail-closed,
  Exit 0/1/2, nur `Cargo.toml` + `Cargo.lock`); `release.yml` mit `verify`-Job
  (Versionspruefung nur fuer Tag-Refs plus `cargo test --lib --locked`) und
  `needs: [verify]` an allen Build-/Publish-Jobs, `contents: read` fuer Verify
  und Builds; `ci.yml` mit Push nur auf `master`, `contents: read` für alle
  Jobs und `--locked` in jedem Cargo-Schritt; Version `0.11.5` in Manifest
  und Lockfile; `docs/RELEASE.md`,
  `CHANGELOG.md`, Beleg und Board nachgezogen.
- **Gates (real gemessen):** Skript `v0.11.5` → 0, `v0.11.4` → 1,
  `0.11.5` → 2; vor der Hebung `v0.11.3` → 0 und `v0.11.4` → 1; PyYAML
  `safe_load` und `actionlint 1.7.12` auf beiden Workflows OK; `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1505 passed /
  0 failed / 1 ignored), `git diff --check` gruen. Nachtrag zum `verify`-Job:
  Ubuntu-Runner haben `pwsh`, aber `default = ["webview"]` braucht WebKitGTK;
  die gebundene apt-Installation aus `ci.yml` wurde ergänzt und erneut geprüft.
- **Board:** T-967 ist nach PR #102 auf `master` `done` mit `done_at` und
  echtem Pruefprotokoll; T-961 bleibt `free` (Userentscheidung „Keine
  Aenderung"). Der Spiegelblock in `docs/TASKBOARD.md` ist synchron.
- **Grenzen:** PR #102 wurde nach ausdruecklicher Freigabe per Squash nach
  `master` gemergt (`c0890d5`); **kein** Tag, kein GitHub-Release, kein
  Deployment. Der historische Tag `v0.11.4` ist unangetastet; ein Workflow-Lauf
  dagegen scheitert absichtlich. Die GitHub-Action wurde nicht live auf einem
  Tag ausgefuehrt. Keine Live-Provider-Laeufe, keine Zugangsdaten, keine
  Profile/Cookies/Tokens, keine Kontoeinstellung angefasst.
- **Dokumentenabgleich (25.11.2026):** `GOALS.md` und `docs/OVERVIEW.md` zeigten
  noch veraltete Zaehlungen (91 Tasks / 14 frei) und einen angeblichen
  `pending`-PR #69. Nach PR #102 stehen auf `master` 92/91/1; PR #69 ist
  `CLOSED` ohne Merge; die rohen T-961-Transkripte sind vorhanden, aber
  negativ (0/5 Tool-Turns, Anmeldestatus unbestaetigt).
- **PR #102:** https://github.com/st0rax/webagent-rs/pull/102 ist `MERGED`
  (`chore/T-967-release-gate-opencode` → `master`, Squash `c0890d5`). Alle vier
  CI-Checks des PRs waren grün (Linux-WebView 1m42s, aarch64 2m37s,
  build & test 2m16s, Windows 3m38s). Kein Tag, kein Release.
- **Offen:** die parallele DeepSeek-Umsetzung auf `chore/T-967-release-gate`
  ist nicht bewertet. Weitere Merge-, Tag- oder Release-Schritte brauchen
  erneut eine ausdrueckliche Freigabe.

## 2026-11-24 — PR-Kette nach Freigabe vollstaendig nach `master` gemergt

- **Freigabe:** Der Eigentuemer hat auf die Frage „Soll ich die PR-Kette jetzt in
  der Reihenfolge 97 bis 101 nach master mergen?" ausdruecklich „Ja, alle fuenf"
  geantwortet. Damit war der Merge freigegeben; **kein Tag, kein
  GitHub-Release**, keine Live-Laeufe, keine Zugangsdaten/Profile/Cookies/
  Tokens, kein Eingriff in FreeRouter (8787) oder Supervisor (4096).
- **Reihenfolge und Squash-Merges:** #97 T-926 `a3f27ec`, #98 T-940 `df9e4b5`,
  #99 T-942 `ae595a5`, #100 T-944 `4d206cc`, #101 T-950 `e0b1ea8`. Alle fuenf
  PRs `MERGED`; `master` lokal und remote `e0b1ea8`.
- **Retarget und Sync:** Weil die PRs auf ihre Vorgaenger-Branches basierten,
  wurden #98–#101 vor dem Merge auf `master` retargetet. Je Branch ein Sync-Merge
  von `origin/master` (neue Commits, kein Rebase, kein Force-Push); Konflikte in
  `docs/CURRENT_WORK.md`, `src/cli.rs`, `src/main.rs`, `selectors/deepseek.json`
  und `src/browser/mod.rs` zugunsten des weiter fortgeschrittenen Chain-HEADs
  geloest — die T-926/T-940/T-942-Abschnitte und -Code waren dort bereits
  enthalten. Sync-Commits: T-940 `438e8bb`, T-942 `2780c6d`, T-944 `34efaec`,
  T-950 `e62acc5`.
- **CI und Gates vor jedem Merge:** alle Jobs des jeweiligen Head gruen; lokal
  nach jedem Sync `cargo test --lib` (1498 / 1499 / 1501 / 1505 passed, 0 failed,
  1 ignored), `cargo check --all-targets` und `git diff --check` gruen.
- **Verifikation auf echtem `master`:** `WebViewSessionAdapter`
  (`src/free_cloud_chat.rs`), `answer_markdown` (`src/browser/mod.rs`,
  `selectors/{deepseek,gemini}.json`), `reasoning_exclude`
  (`selectors/deepseek.json`), `source_hints` (`src/observer.rs`,
  `src/brain_score.rs`, `src/relay.rs`) und `web_search_isolation`
  (`src/api_bridge/catalog.rs`, `docs/API_BRIDGE.md`) sind vorhanden. Board
  unveraendert 91 Tasks / 90 `done` / 1 `free` (T-961, Userentscheidung „Keine
  Aenderung") / 0 `claimed`; die fuenf `done`-Zellen sind jetzt durch den echten
  Merge gedeckt und nicht mehr nur eine dokumentierte Diskrepanz zu `master`.
- Der waehrend der Merge-Arbeit frisch erschienene Remote-Branch
  `chore/T-967-release-gate` wurde nicht angefasst.

## 2026-11-24 — PR-Kette zur Abnahme vorbereitet, CI-Blocker behoben

- **Freigabeumfang:** Der Eigentuemer hat den Audit-Vorschlag bestaetigt und die
  Projektverantwortung uebertragen. Umgesetzt wurden PR-Eroeffnung und
  CI-Reparaturen. **Kein Merge, kein Tag, kein GitHub-Release**; T-961 bleibt
  `free` („Keine Aenderung"), kein Live-Lauf, keine Zugangsdaten/Profile/
  Cookies/Tokens, kein Eingriff in FreeRouter (8787) oder Supervisor (4096).
- **Fuenf PRs, kaskadierend** (je ein Task, ein Branch):
  [#97](https://github.com/st0rax/webagent-rs/pull/97) T-926 → `master`,
  [#98](https://github.com/st0rax/webagent-rs/pull/98) T-940 → T-926,
  [#99](https://github.com/st0rax/webagent-rs/pull/99) T-942 → T-940,
  [#100](https://github.com/st0rax/webagent-rs/pull/100) T-944 → T-942,
  [#101](https://github.com/st0rax/webagent-rs/pull/101) T-950 → T-944.
  Alle fünf sind `OPEN`, `MERGEABLE`, mit Scope, Livebelegen (Nonces/Hashes),
  Gates und ehrlichen Grenzen beschrieben. Merge-Reihenfolge nur 1 → 2 → 3 →
  4 → 5; die Branches bauen linear aufeinander auf.
- **Zwei CI-Blocker aus aelteren Pushes behoben** (neue Commits, kein
  Force-Push): `attach_network_observation` war nur unter `#[cfg(windows)]`
  definiert, aber unbedingt aufgerufen → Nicht-Windows-Stub `Ok(None)` auf
  T-940/T-942/T-944/T-950 (`dd833c3`, `223e2ca`, `1c6173d`, `4a863d9`). Im
  T-944-Livebeleg fehlte das `**Referenz**`-Banner →
  `betriebs_markdown_hat_eine_wahrheit` rot; mit `5b34bde` ergaenzt, auf T-950
  war es bereits vorhanden.
- **CI-Stand nach den Fixes:** alle fuenf Prueflaeufe gruen (T-926 `success`,
  T-940 `success`, T-942 `success`, T-950 `success`, T-944 nach einem
  einmaligen Flake `executor::tests::test_stderr_capture` — leere Streams bei
  Exit 0, Push-Lauf desselben SHA gruen — im Rerun ebenfalls `success`).
  Lokal auf T-950-Head: `cargo test --lib` 1505 passed / 0 failed / 1 ignored,
  fmt/clippy/build/`git diff --check` gruen.
- **Abnahmeuebersicht (nicht im Repo):** `%TEMP%\opencode\pr-acceptance-overview.md`
  mit PR-Tabelle, Branch-SHAs, Gates, Reparaturen und Grenzen. Der Board-Status
  `done` bleibt gegenueber `master` (`9017249`) inkonsistent, bis die PRs
  tatsaechlich gemergt sind; das war zu diesem Zeitpunkt noch nicht erledigt
  und ist jetzt im Abschnitt oben nachgeholt.

## 2026-11-24 — Audit: Board-Status gegenueber `master`, Gates nachgemessen

- **Auftrag:** Stand/Audit prüfen. Der Eigentuemer hat danach die
  Projektverantwortung an diesen Agenten delegiert. Daraus wurde **keine**
  Freigabe fuer Merge, Tag, Release oder Live-/Kontozugriff abgeleitet.
- **Gepruefter Baum:** `webagent-rs`, `feature/T-950-websearch-isolation`,
  HEAD `956425f`, identisch mit `origin/...`; Working Tree vor und nach dem
  Audit sauber. Remote-`master` per `git ls-remote`:
  `9017249157a3182de900fa927ae6f4d8ee33f906` (lokal gleich).
- **Board:** 91 Tasks, 90 `done`, 1 `free` (T-961), 0 `claimed`. Der
  maschinenlesbare Spiegelblock in `docs/TASKBOARD.md` ist synchron;
  `taskboard::tests::taskboard_invarianten_halten` gruen (1 passed, 1505
  gefiltert). Aeltere Prosotabellen im selben File sind historisch und nicht
  die Quelle.
- **Gates am 24.11. auf HEAD nachgemessen:** `cargo test --lib` 1505 passed /
  0 failed / 1 ignored; `cargo fmt --all -- --check` Exit 0;
  `cargo clippy --all-targets -- -D warnings` Exit 0; `git diff --check`
  Exit 0. Diese Zahlen gelten fuer den Feature-Branch, **nicht** fuer
  `master`.
- **Befund (Diskrepanz):** `START_HERE.md` Abschnitt 4 und `AGENTS.md`
  Regel 5 definieren `done` als „Branch in `master` gemergt". Fuer `T-926`,
  `T-940`, `T-942`, `T-944`, `T-950` steht das Board auf `done`, der Code
  liegt aber nur auf den gleichnamigen, gepushten Feature-Branches.
  `git branch --merged master` fuehrt keinen davon; `git grep` in `master`
  findet `WebViewSessionAdapter`, `answer_markdown`, `reasoning_exclude`,
  `source_hints`, `web_search_isolation` nicht, auf HEAD dagegen schon.
  `docs/CURRENT_WORK.md` verbietet Merge ohne neue ausdrueckliche Freigabe —
  der Board-Status `done` behauptet hier mehr, als `master` ausliefert.
- **Entscheidung dieses Agenten:** Befund dokumentieren, **keinen** Merge,
  keinen Tag, kein Release, keine Statuskorrektur im Board ohne Freigabe.
  T-961 bleibt `free` („Keine Aenderung" des Eigentuemers), kein `PASS`, kein
  `done`. Empfehlung: fuer die fuenf Branches zuerst PRs oeffnen und pruefen
  und `done`/`done_at` erst nach tatsaechlichem Merge bestaetigen; alternativ
  den Board-Status bewusst korrigieren — beides braucht eine ausdrueckliche
  Entscheidung des Eigentuemers.
- **Keine Live-Laeufe**, keine Zugangsdaten, keine Profil-/Cookie-/Token-
  Dateien, kein Eingriff in FreeRouter (8787) oder Opencode-Supervisor (4096).

## 2026-11-21 — Entscheidung zu T-961: keine Änderung

- Der Eigentümer hat den Konto-Check mit **„Angemeldet / OK"** beantwortet.
- Ein konkreter neuer Strategievorschlag („Auftrag statt Werkzeug-Meta":
  Tool-Schema als Preamble nach vorn, Choice-/Verfügbarkeitssätze entfernt,
  auftragsförmiger User-Turn ohne Werkzeug-Meta, ≥5 aufeinanderfolgende saubere
  Raw-Läufe) wurde mit Ziel, Umfang, erwarteter Wirkung und Messplan vorgelegt.
- Der Eigentümer hat ausdrücklich **„Keine Änderung"** gewählt. Damit gilt:
  T-961 bleibt bewusst `free`, **kein Code, kein Live-Lauf, kein Tag, kein
  GitHub-Release, kein Merge**. Die bereits widerlegten Richtungen (0/7 und
  0/6) bleiben die letzte Aussage; es gibt kein `PASS` und kein `done`.
- Endstand des Boards: **90 `done` / 1 `free` (T-961) / 0 `claimed` / 91**.
  Damit ist „100 %" ohne neue, ausdrücklich freigegebene Strategie nicht
  erreichbar — ehrliche Grenze statt decayed Status.

## 2026-11-21 — G-010: T-950 Websuche-Isolation und Quellenhinweise live belegt

- **Branch:** `feature/T-950-websearch-isolation`, ausgehend von
  `feature/T-944-reasoning-exclude` (`584f071`). Kein Tag, kein Release, kein
  Merge ohne neue ausdrückliche Freigabe.
- **Surveys (Binary `64ADA47A…C90`):** je Brain DOM-Survey mit frischer
  WebView-OAuth-Session. Nur DeepSeek hat einen sichtbaren Websuche-Schalter
  (`Search`, `ds-toggle-button--selected`); chatgpt/gemini zeigen nur die
  Chatverlaufssuche, kimi/mistral/qwen keinen Schalter, zai trifft einen
  Sidebar-Menüeintrag ohne Zustandswechsel, claude ist nicht angemeldet,
  perplexity unbekannt.
- **Toggle-Beleg:** deepseek `aria-pressed=true|…--selected →
  aria-pressed=false|…` (Hash `FC4869A0…4DB8AF3F`). Folge-Survey wieder
  `selected` — nur Konversationszustand, keine dauerhafte Kontoänderung.
- **Umsetzung:** `WebSearchObservation` + Puffer und Event-Felder `web_search` /
  `source_hints` in `src/brain_score.rs`; `source_hints(text)` in
  `src/observer.rs` (nur explizit markierte Hinweise, kein Fließtext-Raten,
  Text unverändert); `ensure_web_search_off()` mit bounded Wait nach
  `new_chat` in `src/browser/ui.rs`; Turn-Verdrahtung in `src/relay.rs`;
  `web_search_isolation(brain)` + `/v1/models`-Feld in
  `src/api_bridge/catalog.rs` und Test in `src/api_bridge/tests.rs`.
- **Bridge-Livebeleg (Port 8788, frischer prozess-only Token, Final-Binary
  `20DC7552…82FB8`):** `/v1/models` markiert nur deepseek als
  `switchable_disabled_for_api`, alle übrigen als `not_isolatable`. Turn
  `T950F-CDD54DDB` (chatgpt, URL auf öffentliche Rust-README): Antwort endet
  `Quelle: Offizielle Rust-README.md auf GitHub.` → Event mit
  `source_hints=["Offizielles Rust-README.md auf GitHub"]` und
  `web_search=not_isolatable`. Turn `T950G-C9645851` (deepseek): Event mit
  `web_search=off_before_send` und belegtem Zustandswechsel. Beide
  `success=true`, `sends=1`. Während der Läufe keine Repo-Änderung, Rohdaten nur
  unter `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T950-websearch-20261121\\`.
- **Ehrlicher Zwischenbefund:** ein erster Lauf mit Zwischen-Binary
  `2D67A22…29078` meldete deepseek `not_isolatable` („nicht im DOM"), weil die
  SPA den Schalter direkt nach `new_chat` noch nicht gerendert hatte; bounded
  Wait (20 × 150 ms) behebt das, Final-Lauf ist `off_before_send`. Grenze
  bleiben stille Provider-Abrufe ohne Quellenhinweis: sie sind für die Bridge
  nicht erkennbar und werden nicht behauptet.
- **Gates:** `cargo test --lib` 1505 passed / 0 failed / 1 ignored; fmt,
  clippy (`-D warnings`), build, `git diff --check`, Board-Invariante grün.
- **Beleg:** `docs/proofs/T-950/websearch-isolation-live-result.md` und
  `docs/API_BRIDGE.md` (neuer Abschnitt „Anbieterseitige Websuche"). Board:
  T-950 `done`, 90 `done` / 1 `free` / 91.
- **Offen:** nur noch T-961 `free` (beide Richtungen live widerlegt: 0/7 und
  0/6; kein `PASS`, neue Strategie nur nach neuer ausdrücklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigentümers weiter offen). Kein Tag/Release.

## 2026-11-21 — G-010: T-944 Reasoning-Ausschluss live belegt

- **Branch:** `feature/T-944-reasoning-exclude`, ausgehend von
  `feature/T-942-markdown-structure` (`19e8c6f`). Die abgelehnte T-961-Scheibe
  wurde **nicht** mitgezogen.
- **DOM-Beleg:** DeepSeek rendert den Denkbereich als Geschwister
  `.ds-think-content` (Textbeginn `We need answer...`) neben
  `div.ds-markdown.ds-assistant-message-main-content`; Dump
  `A62C877F…830C2A`. Gemini zeigte im gemessenen Turn **keinen** separaten
  Denkbereich (`3DD89CC0…353D`), deshalb dort auch kein Ausschluss erfunden.
  ZAI-Toggle nicht anklickbar, ChatGPT ohne Reasoning-Key, Claude nicht
  angemeldet — für diese wird keine Ausschluss-Behauptung aufgestellt.
- **Fix:** deklarierter Ausschluss `reasoning_exclude` nur in
  `selectors/deepseek.json`; beide Auslesepfade (`answer_read_body` und
  `probe_generation_js` in `src/browser/mod.rs`) entfernen die Knoten aus einer
  DOM-Kopie vor `TX`/`TXMD`, mit Rückfall auf den Originaltext. Ohne
  Deklaration bytegenau unverändert, kein Text-/Formulierungsfilter
  (non_goals eingehalten). `src/browser/verify.rs`: Signatur mit `"[]"`.
  Temporärer DOM-Dump (`WEBAGENT_T944_DOMDUMP`) vor dem Final-Build entfernt.
- **Zustand:** DeepThink war eingeschaltet; erster Toggle `true → false`,
  sofortiger Restore `false → true`, Abschluss-Survey weiterhin
  `ds-toggle-button--selected`. Kein dauerhafter Kontoeingriff. (DeepSeek
  `Search` war ebenfalls selected — T-950, nicht verändert.)
- **Finaler Livebeleg (Binary `64ADA47A…C90`, 1501 Tests grün):** deepseek
  `T944Z-9FDB9E20`, gemini `T944Z-B2AC9CFF`, qwen `T944Z-A651188A`, zai
  `T944Z-FD92CE43`, chatgpt `T944Z-E7002AD0`; alle Exit 0, `webview_oauth`,
  `api_key: false`, `oauth_session: true`, keine Reasoning-Marker in den
  Antworten. Ehrliche Grenze: chatgpt lieferte die Nonce nicht mit (Antwort
  nur `391`) — dieser Lauf belegt nur „kein Reasoning im Text". kimi/mistral
  Exit 2 mit leerer stdout, claude ohne Login → keine Aussage.
- **Gates:** `cargo test --lib` 1501 passed / 0 failed / 1 ignored (zwei neue
  T-944-Tests); `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build`, `git diff --check` grün.
- **Beleg:** `docs/proofs/T-944/reasoning-exclusion-live-result.md` mit
  DOM-Beleg, Rohhashes und Einschränkungen. Rohdaten unter
  `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T944-reasoning-20261121\\`
  (nicht ins Repo kopiert). Board: T-944 `done`, 89 `done` / 2 `free` / 91.
- **Offen (Stand der Uebergabe):** T-950 brauchte damals noch echte
  DOM-/Antwortmessungen; T-950 ist inzwischen oben dokumentiert `done`.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdrücklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigentümers weiter offen). Kein Tag/Release.

## 2026-11-21 — G-010: T-942 Markdown-Struktur live zurueckgewonnen

- **Branch:** `feature/T-942-markdown-structure`, ausgehend von
  `feature/T-940-network-observation` (`2241eeb`). Die abgelehnte T-961-Scheibe
  wurde **nicht** mitgezogen.
- **Vorher-Negativbeleg:** deepseek `T942-05223CD7`, gemini `T942-F228D3C9` —
  Tabellen als Tab-Text, Fences weg, UI-Labels `Copy`/`Download` im Antworttext,
  Listenmarker weg. Binary `A0B5D0D1…3306CD`, Exit 0.
- **Fix:** neues `TXMD` im gemeinsamen JS-Prelude (`src/browser/js.rs`)
  rekonstruiert Markdown aus echten DOM-Elementen (`table`/`pre`/`ul`/`ol`/`h*`)
  und verwirft UI-Chrome; Opt-in `answer_markdown` in `src/browser/mod.rs`,
  gesetzt in `selectors/deepseek.json` und `selectors/gemini.json`. Ohne Flag
  bleibt `TX` unverändert. Kein Raten aus Tab-Text, keine Zwischenablage.
- **Nachher-Livebeleg (finales Binary `9EE80CE4…B760`):** deepseek
  `T942H-23A8FEF4` (11,3 s), gemini `T942H-C9CE5F14` (9,0 s), beide Exit 0,
  leeres stderr. Ausgelieferter Text ist **STRUKTURGLEICH** mit dem bekannten
  Quell-Markdown (115/115 Zeichen nach CRLF→LF): Tabelle mit Pipes und
  Separatorzeile, rust-Code-Fence, Listen mit `-`. Kein Copy/Download/`?`-Icon.
- **Gates:** `cargo test --lib` 1499 passed / 0 failed / 1 ignored (neuer Test
  `t942_prelude_fuehrt_markdown_strukturell_zurueck`); `cargo fmt --all`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build`, `git diff --check`
  grün.
- **Beleg:** `docs/proofs/T-942/markdown-structure-live-result.md` mit
  Vorher/Nachher, Rohhashes und Einschränkungen (nur deepseek/gemini gemessen,
  andere Brains unverändert). Rohdaten unter
  `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T942-markdown-structure-20261121\\`
  und `…\\T942-markdown-fix-20261121\\` (nicht ins Repo kopieren). Board: T-942
  `done`, 88 `done` / 3 `free` / 91.
- **Offen (Stand der Uebergabe):** T-944 und T-950 brauchten damals noch echte
  DOM-/Antwortmessungen; T-944 ist inzwischen oben dokumentiert `done`.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdrücklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigentümers weiter offen). Kein Tag/Release.

## 2026-11-21 — G-010: T-940 lesende Netzwerkbeobachtung live belegt

- **Branch:** `feature/T-940-network-observation`, ausgehend von
  `feature/T-926-webview-oauth-adapter` (`9af4033`, nur
  Branch-Präfixkorrektur). Die abgelehnte T-961-Scheibe wurde **nicht**
  mitgezogen.
- **Implementierung:** neues reines Modul `src/network_observation.rs`
  (Redaktion auf `scheme://host`, 512-Eintrags-Grenze mit `dropped_events`,
  gewichtete Auswahlheuristik mit `selection_reason`); drei **lesende**
  CDP-Receiver in `src/webview_runtime.rs` (`Network.enable` sowie
  `requestWillBeSent`/`responseReceived`/`loadingFinished`, kein Intercept,
  kein Port/WebSocket); opt-in `--net-trace` in `src/cli.rs`/`src/main.rs`.
- **Negativbefund zuerst:** Lauf 1 (`T940-EB0F8B8395`, Exit 0) lieferte die
  richtige Antwort, aber die **falsche Klassifikation** — die Heuristik „jüngster
  Request gewinnt" wählte einen 4-ms-Static-Fetch von
  `fe-static.deepseek.com` statt des SSE-Provider-Calls. Daraufhin wurde die
  Auswahl gewichtet (SSE +8, POST +3, Site-Origin +2, 2xx +1) und das Urteil
  um `request_id`, `mime_type` und `selection_reason` erweitert.
- **Live-Nachweis Lauf 2:** deepseek, Nonce `T940-A58B1F5DC0`, Exit 0, 14,9 s,
  Token exakt. `verdict`: `request_id 7812.198`, Origin
  `https://chat.deepseek.com`, `text/event-stream`, Status 200, 1101 ms,
  `finished: true`, `selection_reason` „SSE-Antwort … POST, Origin der
  Dokumentseite, 2xx". Rohereignisse 3198/3539/4299 ms ergeben exakt 1101 ms.
  Zähler 74 Requests / 54 Responses / 54 Finished / 0 verworfen. Binary
  `A0B5D0D1…3306CD`. Keine Repo-Änderung während der Läufe, kein HTTP-Port.
- **Gates:** `cargo test --lib` 1498 passed / 0 failed / 1 ignored;
  `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo build`,
  `git diff --check` grün; sechs neue `t940_`-Tests grün.
- **Beleg:** `docs/proofs/T-940/network-observation-live-result.md` mit beiden
  Läufen, Korrektur, Rohereignissen und ehrlichen Einschränkungen (nur 2 Läufe,
  nur deepseek, Heuristik statt Beweis). Rohdaten unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T940-network-observation-20261121\`
  (nicht ins Repo kopieren). Board: T-940 `done`, 87 `done` / 4 `free` / 91.
- **Offen:** T-942, T-944, T-950 brauchen weiterhin echte DOM-/Antwortmessungen.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdrücklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigentümers weiter offen). Kein Tag/Release.

## 2026-11-21 — G-010: T-926 VerifiedFree-Adapter live belegt (WebView-OAuth, kein API-Key)

- **Branch:** `feature/T-926-webview-oauth-adapter`, ausgehend von `master`
  (`9017249`) — die abgelehnte T-961-Scheibe wurde **nicht** mitgezogen.
- **Freigabe:** Der Eigentümer hat „keine api keys. nur oauth." vorgegeben und
  die WebView-Session als Basis bestätigt. Es gibt bewusst **kein** Key-Argument
  in Code oder CLI; Groq/Gemini-Key/Cloudflare/NVIDIA/OpenRouter wurden nicht
  angebunden.
- **Implementierung:** `WebViewSessionTransport` (ohne Key/Token-Parameter),
  `BrowserInferenceWebViewTransport` über `browser_inference::complete()`,
  `WebViewSessionAdapter<T>` als `TextStreamAdapter`, und die einzige
  `VerifiedFree`-Registry-Kennung `webagent/webview-oauth-session`. Der Adapter
  prüft Free-only-Route → Modellbindung → Sitzung; verweigerte Routen rühren die
  Sitzung nicht an.
- **Red/Green:** sechs Tests rot gegen den leeren Rumpf (5 `FAILED`), danach
  sieben grün; `cargo test --lib` 1491 passed / 0 failed / 1 ignored; `cargo
  fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`,
  `git diff --check` alle grün.
- **Live-Nachweis:** drei echte Turne auf frisch angemeldeten WebViews — 2×
  gemini (Nonces `T926-0A8327B448`, `T926-38A1C3E14F`) und 1× deepseek
  (`T926-AD4B110890`), alle Exit 0, `oauth_session: true`, `api_key: false`,
  `started → token → completed`. Binary `97E4B4DB…B611FB`. Lauf 1 enthielt
  zusätzlich UI-Text, Lauf 3 einen harmlosen WebView2-Teardown-Hinweis; das ist
  im Beleg dokumentiert. Kein HTTP-Port, keine Repo-Änderung während der Läufe.
- **Belege:** `docs/proofs/T-926/webview-oauth-live-result.md` und
  `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` aktualisiert. Rohdaten unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T926-webview-oauth-20261121\`
  (nicht ins Repo kopieren).
- **Offen (Stand dieses Abschnitts):** T-961 bleibt `free` (beide Richtungen
  live widerlegt: 0/7 und 0/6; die zugehörigen Belege liegen auf
  `fix/T-961-self-mirror`). T-940, T-942, T-944, T-950 brauchten zu diesem
  Zeitpunkt echte Messungen; **T-940 ist inzwischen `done`** (siehe Abschnitt
  oben). Kein Tag/Release.

## 2026-11-21 — G-010: T-927/T-928 — Matrix ehrlich bereinigt, beide `done`

- **Methode:** read-only DOM-Surveys (`webagent survey --brain <brain> --dump
  --headless`) mit dem Binary aus `cd76926` (SHA256 `61DC1281…`). Kein `--write`,
  keine Zugangsdaten eingegeben, keine Kontoeinstellung geändert, keine
  Profil-/Cookie-/Token-Datei gelesen.
- **T-927 (`done`):** `model/auto` war `not_run`, obwohl T-902 den Routing-Pfad
  schon live belegt hatte (HTTP 200, `AUTO-ROUTER-OK`, `selected=chatgpt`).
  Frischer Survey: `Unbekanntes Brain: auto` → auto ist ein virtueller Router
  ohne eigene Oberfläche. Zelle jetzt `passed` **nur für den Routing-Pfad**, mit
  explizitem Hinweis, dass keine Browser-Modellwahl behauptet wird. Beleg:
  `docs/proofs/T-927/model-auto-evidence.json` + roher Survey-Dump.
- **T-928 (`done`):** alle offenen `effort`-Zellen haben jetzt Datum, Status und
  Belegpfad: chatgpt/deepseek/gemini/mistral/perplexity/zai/auto → `unreachable`
  (fehlende Oberfläche, abgelaufene Sitzung oder Brain nicht registriert), kimi
  bleibt `failed` mit dokumentiertem Strukturbefund. claude/qwen (`passed`)
  unverändert. **Kein `passed` erfunden.** Beleg:
  `docs/proofs/T-928/effort-survey-evidence.json` + 7 rohe Survey-Dumps.
- **Matrix:** `as_of` 2026-09-12 → 2026-11-21; `live_verify` (alter
  Gesamt-Live-Lauf vom 12.09.) bewusst unangetastet. `.gitattributes` sichert die
  Rohdumps bytegenau. Board-Spiegel: 85 `done`, 6 `free`, 91 gesamt. T-961 bleibt
  `free` (0/7 sauber), kein Tag/Release.

## 2026-11-21 — G-010: T-961 frische Live-Matrix — 0/7 sauber, weiter `free`

- **Binary:** `target/debug/webagent.exe` aus `0d7d802` (Code identisch zum
  gemergten `f85010a`), SHA256 `61DC1281EA96EAC5B8B788C3D3CBE1418F2F4BB1E83FFAE1456949ABCDFDB69E`.
- **Harness:** ausschließlich Port 8788, pro Lauf frischer GUID-Token nur in der
  Prozessumgebung, keine Repo-Datei während eines Laufs geändert, keine
  Zugangsdaten/Profile/Cookies gelesen, keine Kontoeinstellung geändert. Der
  Textturn bestätigte die bestehende Anmeldung (`PI_BRIDGE_OK`). Temporäre
  Harness-Kopie unter `%TEMP%\opencode\T961-live-20261121\`; dortige
  `models.json` nur für Pi 1.5.2 auf `input: [text, image]` korrigiert.
- **Ergebnis:** sieben Läufe (2 Pi, 5 direkte OpenAI-Client-Läufe), **0 sauber**.
  Direct 3/4/5: HTTP 502 `model_output_envelope_invalid` in Turn 3/4/1. Pi 2:
  0/5 Tools, Navigationstimeout. Pi 3: 18 Tool-Starts, aber 15
  Relay-Korrekturen, 20 `stopReason=error`, 6 Auto-Retries, kein `stopReason=stop`
  — der Pi-Harness meldete hier fälschlich „PASS"; der neue strikte Verifier
  weist ihn zurück. Direct 6: fünf Tool-Turns clientseitig sauber, aber 3
  Relay-Korrekturen und 180-s-Timeout im `none`-Schlussturn.
- **Beleg:** `docs/proofs/T-961/auto-required-live-result.md` mit Methodik,
  Ergebnistabelle, Hash-Tabelle und Rohdateien (`auto-required-*.jsonl` /
  `auto-required-*-bridge-*.log`). `README.md`, `analysis.md` und
  `API_BRIDGE.md` sind aktualisiert. Secret-Scan: 0 Treffer.
- **Entscheidung:** T-961 bleibt `free`. Kein PASS, kein `done`, kein Tag/Release.
  Die Eskalation ist funktional verdrahtet, aber nicht stabil; eine Änderung der
  Strategie braucht eine neue ausdrückliche Freigabe. Die übrigen sieben
  `free`-Tasks brauchen weiterhin echte Live-Belege.

## 2026-11-21 — G-010: T-961 Auto→Required-Eskalation — Code gemergt, Live-Abnahme offen

- **PR #96 gemergt** (Squash `f85010a`, alle CI-Checks grün), Branch
  `fix/T-961-auto-required-escalation` gelöscht. `master` lokal und remote bei
  `f85010a`, Working Tree vor dieser Statusfortschreibung sauber.
- **Freigabe umgesetzt:** `effective_tool_choice` eskaliert `Auto` → `Required`,
  wenn der Betreiber `--auto-tool-choice-required` bzw.
  `WEBAGENT_AUTO_TOOL_CHOICE_REQUIRED=1` setzt. Opt-in, default aus: `required`
  verhindert eine Textabschlussantwort im Werkzeugloop. Explizites `none`,
  `required`, Function und Tool-freie Requests bleiben unverändert. Alle fünf
  Parser-Aufrufstellen sind verdrahtet.
- **Zusatzfix:** Der Prompt bot bei `Required`/`Function` widersprüchlich eine
  Textantwort an; der Hinweis steht jetzt nur noch bei `Auto`.
- **Belege:** `docs/proofs/T-961/auto-required-escalation.md` mit echter
  Red-/Green-Demonstration (ohne Eskalation `left: Auto, right: Required`, Exit
  101; danach grün) und Prompt-Test. `docs/API_BRIDGE.md` dokumentiert den
  Schalter und die Folge für Aufrufer (Schlussturn `none` oder ohne Tools).
- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test --lib` (1485 grün, 1 ignored),
  `taskboard_invarianten_halten`, `git diff --check`.
- **T-961 bleibt `free`:** Der PR liefert nur die freigegebene Code-Semantik,
  keinen Live-Nachweis. Die frische Live-Matrix auf Port 8788 mit prozess-only
  Tokens und gesetztem Schalter steht noch aus; jeder Negativlauf blockiert die
  Abnahme. Kein Tag/Release. Die bisherige Messreihe `required-mode-*`
  (clientseitiges `required`, 1/8 grün) bleibt ein Negativbefund.

## 2026-11-21 — Eigentümer-Entscheidungen zu den Rest-Tasks

Der Nutzer hat am 2026-11-21 ausdrücklich freigegeben:

- **T-914:** USER-SKIP aufgehoben — Medienhandler-Refactor darf geclaimt und
  umgesetzt werden.
- **T-961:** „Auto→Required freigeben" — der Auto-Modus darf Tool-Calls
  erzwingen. Das ist eine bewusste Semantikänderung und verlangt eine neue
  Abnahme mit frischer Live-Matrix; der bisherige Negativbefund bleibt als
  Historie unter `docs/proofs/T-961/` erhalten.
- **Live-Läufe:** erlaubt. Harness ausschließlich auf Port `8788`, frische
  prozess-only Tokens, keine Repo-Datei während eines Laufs ändern. Logins,
  CAPTCHAs und Einmalcodes bedient der Nutzer im Fenster; ich lese keine
  Profil-/Cookie-/Token-Dateien und gebe keine Zugangsdaten ein.

Abarbeitung: T-914 zuerst (lokal), dann T-961 mit Live-Matrix, danach die
übrigen Live-Belege. Kein Tag/Release ohne vollständige Abnahme und neue
ausdrückliche Freigabe.

## 2026-11-21 — G-010: T-914 abgeschlossen (Medienmodul)

- **PR #95 gemergt** (Squash `a6d6909`, alle CI-Checks grün), Branch
  `refactor/T-914-media-module` gelöscht. `master` lokal und remote bei
  `a6d6909`, Working Tree vor dieser Statusfortschreibung sauber.
- **Refactor:** Zehn Medienblöcke (Bildgenerierung, Audio-Transkription,
  -Absageerkennung, -Speech, Multipart-Parser, `audio_mime`, Request-Typen)
  unverändert nach `src/api_bridge/media.rs` übernommen; nur Sichtbarkeit
  `pub(super)` für Root und Tests. Root hat keine Multipart-Schleife mehr.
- **Beleg:** `docs/proofs/T-914/measurement.md` mit Block-für-Block-Vergleich
  gegen `HEAD` (10/10 identisch nach Normalisierung von Sichtbarkeit und
  `rustfmt`-Whitespace, SHA256 `338C4B57…`), Schleifenzähler 1 → 0 und den
  bestehenden Audio/Bild-Tests. Kein Live-Aufruf der Endpunkte.
- **Gates:** `cargo fmt --all -- --check`,
  `cargo clippy --features webview --all-targets -- -D warnings`,
  `cargo test --features webview --lib api_bridge::tests::` (54 grün),
  `cargo test --lib` (1483 grün, 1 ignored), `git diff --check`.
- **Board:** T-914 auf `done`, Stand 83/91 `done`, 8 `free`.

## 2026-11-21 — G-010: T-948 abgeschlossen (Preflight vor dem Senden)

- **PR #94 gemergt** (Squash `23879aa`, alle CI-Checks grün), Branch
  `fix/T-948-consent-preflight` gelöscht. `master` lokal und remote bei
  `23879aa`, Working Tree vor dieser Statusfortschreibung sauber.
- **Ursache (Messbefund 2026-09-14):** Sechs mistral-Anfragen endeten mit
  `timeout_no_message`, obwohl das Senden Erfolg meldete. `webagent shot`
  belegte eine abgemeldete Seite mit „Anmelden"/„Registrieren" und einem
  modalen „Vibe Nutzungsbedingungen"-Dialog. `login_indicator` zeigte auf den
  Composer (auch abgemeldet im DOM), `login_button` kannte „Anmelden" nicht.
- **Fix (rein lesend, nur für deklarierte Brains):** `send_generic` ruft
  `operations::pre_send_block_reason` vor `wake_renderer`/Fill. Sichtbarer
  Zustimmungsdialog → `terms_consent_required`, sichtbarer Anmeldeknopf →
  `login_required`; Meldung `send_phase=preflight: …`, HTTP 502 mit Marker im
  Text. Kein Zustimmungsknopf, keine Anmeldung, kein Prompt — beides bleibt beim
  Menschen. `selectors/mistral.json` korrigiert `login_indicator` auf
  Profil-/Konto-Knöpfe, ergänzt „Anmelden" und `send_preflight_guard`.
- **Belege:** `docs/proofs/T-948/measurement.md` mit Mock-DOM-Tests und echter
  Red-/Green-Demonstration (beide Guard-Tests ohne Guard:
  `send_phase=insert: Composer-Feld nicht gefunden (Timeout)`, Exit 101; danach
  grün). `docs/API_BRIDGE.md` nennt die Marker. **Kein Live-Anspruch:** die
  neuen Profil-/Anmelde-Selektoren sind nicht live vermessen; der `handoff`
  „Fehlermeldung nachher gegen den abgemeldeten Zustand" bleibt offen.
- **Gates lokal grün:** `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1483 grün,
  0 failed, 1 ignored), `git diff --check`.
- **Board:** T-948 im JSON und Spiegel auf `done`, Stand 82/91 `done`,
  9 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigentümer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdrückliche Freigabe.

## 2026-11-21 — G-010: T-946 abgeschlossen (verwaiste Anfrage / Brain-Sperre)

- **PR #93 gemergt** (Squash `ae9cb2c`, alle CI-Checks grün), Branch
  `fix/T-946-client-disconnect-lock` gelöscht. `master` lokal und remote bei
  `ae9cb2c`, Working Tree sauber.
- **Ursache:** Eine abgebrochene mistral-Anfrage lief am 2026-09-14 dreimal
  (~867/867/861 s) im Browser weiter, obwohl der Client längst weg war. Die
  per-Brain-Sperre hielt den verwaisten Turn; Folgeanfragen an dasselbe Brain
  warteten. Andere Brains liefen parallel — das war korrekt, aber
  `docs/API_BRIDGE.md` behauptete fälschlich keine parallelen Browserruns.
- **Fix (drei Ebenen, ohne globale Serialisierung):**
  1. `TcpClientPresence` in `api_bridge/transport.rs` beobachtet den Client über
     einen zweiten Socket-Handle (20 ms Peek, Lese-Timeout sofort restore).
  2. `with_browser_lock` in `api_bridge/inference.rs` prüft vor und nach dem
     Sperrerwerb; verwaisste Anfragen starten keinen Turn.
  3. `run_turn_loop` in `relay.rs` prüft zu Beginn jedes Turns und übergibt
     einen `cancelled`-Closure an `wait_response_streaming_with_cancel`
     (`browser/backend.rs`, Checks in beiden Poll-Phasen). Abbruch →
     `backend_status=client_disconnected`, `relay_stop`, brain_score-Grund
     `client_disconnected`, Sperre frei. Kein erneutes Senden eines bereits
     gesendeten Prompts.
- **Belege:** `docs/proofs/T-946/measurement.md` mit echtem TCP-Loopback-Test,
  vier Red-/Green-Tests, echten brain_score-Zeilen und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1479 grün, 1 ignored), `git diff --check`. Ein einmaliger
  `ConnectionReset` im unabhängigen Web-UI-One-Shot-Test war nicht
  reproduzierbar (5 Einzel- + 2 Gesamtwiederholungen grün). `docs/API_BRIDGE.md`
  beschreibt jetzt Serialisierung je Brain und Abbruchverhalten korrekt.
- **Board:** T-946 im JSON und Spiegel auf `done`, Stand 81/91 `done`,
  10 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigentümer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdrückliche Freigabe. G-010 bleibt in_progress.

## 2026-11-21 — G-010: T-953 abgeschlossen (Modellausgabe-Fehler)

- **PR #92 gemergt** (Squash `02b6987`, alle CI-Checks grün), Branch
  `fix/T-953-model-output-error` gelöscht. `master` lokal und remote bei
  `02b6987`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **DoD 1 — Fehlertyp:** Jeder `parse_response`-Fehler trägt jetzt den stabilen
  Marker `MODEL_OUTPUT_ERROR:`. `inference_failure_response()` in
  `provider_handlers.rs` meldet 502 mit OpenAI-`type: server_error` und
  `code: model_output_envelope_invalid` (Anthropic entsprechend) statt
  `invalid_request_error`. Andere Fehler behalten ihre alte Übersetzung; der
  DSML-Fehler aus T-957 ist nun ebenfalls als Modellausgabe gekennzeichnet.
- **DoD 2 — brain_score:** `relay::run_turn_loop` prüft die Ausgabe über
  `OutputCheck::validate`, bevor `answer` gesetzt wird. Ein ungueltiger
  Umschlag ist ein Fehlschlag mit dem eigenen Grund
  `model_output_envelope_invalid`; der Circuit-Breaker bleibt bewusst unberührt
  (Brain war erreichbar). Vorher wurde der Turn als Erfolg verbucht.
- **DoD 3 — Korrekturgrenze:** Höchstens eine Korrektur im selben Chat (kein
  `new_chat`) mit der Parserfehlermeldung; danach fail-closed. Das Zeitbudget
  gilt für die gesamte Anfrage inklusive Korrektur. Mock-Backend-Tests prüfen
  `new_chats=1`, `sends=2` und den brain_score-Eintrag.
- **Beleg:** `docs/proofs/T-953/measurement.md` mit Rot-/Green-Tests, echten
  Event-Zeilen aus dem Test-Data-Verzeichnis und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1474 grün), `git diff --check`. `docs/API_BRIDGE.md`
  beschreibt Fehlerklasse, Grund und Korrekturgrenze.
- **Board:** T-953 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  80/91 `done`, 11 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigentümer-Entscheidung nicht abschließbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 — G-010: T-957 abgeschlossen (DSML fail-closed)

- **PR #91 gemergt** (Squash `f2a5aac`, alle CI-Checks grün), Branch
  `fix/T-957-dsml-fail-closed` gelöscht. `master` lokal und remote bei
  `f2a5aac`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **Entscheidung fail-closed statt Übersetzung:** `parse_response` erkennt den
  DSML-Marker aus dem realen deepseek-Beispiel (Vollbreiten-Form und die im
  Taskboard gespeicherte verformte Form) und meldet
  `DSML_TOOL_FORMAT_UNSUPPORTED`. Keine Übersetzung in `tool_calls`, weil die
  Argumente aus derselben verformenden Lesestrecke stammen, die T-956 nur für
  den JSON-Umschlag abgesichert hat. Gültige `WEBAGENT_INFERENCE/1`-Umschläge
  bleiben unberührt. `docs/API_BRIDGE.md` beschreibt die Grenze.
- **Beleg:** `docs/proofs/T-957/measurement.md` mit Red-Test (vorher
  `Ok(Textantwort)`, `tool_calls: []`), Green-Tests und Gates. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1468 grün), `git diff --check`. Der erste CI-Lauf war
  rot, weil ein neues Beleg-Markdown das Referenz-Banner brauchte; mit Banner
  sind alle Checks grün. MinGW liegt lokal in einem WinGet-Paket und wurde nur
  für die Cargo-Läufe an `PATH` gehängt (keine Repo-Änderung).
- **Board:** T-957 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  79/91 `done`, 12 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigentümer-Entscheidung nicht abschließbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 — T-961: auch erzwungener Tool-Call widerlegt — weiter NICHT abgenommen

- **Dritte Richtung geprüft (strukturell, ohne Code-Änderung):** ein direkter
  OpenAI-Client auf dem unveränderten `master` `bf0a65b` (frisch gebaut,
  SHA256 `3532478D...F710`) sendete auf fünf aufeinanderfolgenden Turns
  `tool_choice: "required"` — bereits vom Adapter unterstützt, also keine
  API-Semantik-Änderung und keine Freigabe nötig. Ergebnis aus 8 Läufen:
  **1/8 streng grün**. Sechs Läufe lieferten trotz Verpflichtung nur Text
  (fail-closed 502), zwei scheiterten an `ABSENDEKNOPF_DEAKTIVIERT`. Ein
  zusätzlicher Validierungslauf mit fünf sauberen Tool-Turns hatte einen
  abschliessenden Renderer-Timeout und ist nicht Teil der Vergleichsreihe.
  Rohbelege: `docs/proofs/T-961/required-mode-*`, Auswertung
  `required-mode-result.md`, 0 Secret-Treffer.
- **Einordnung:** `required` macht die Absage für Aufrufer sichtbar (502 statt
  Text), beseitigt aber die Fehlerklasse nicht. Damit sind drei Richtungen
  gemessen: Verfügbarkeitsregel 2/8, `[FUNKTIONEN]` 6/8, `required` 1/8.
  Eine serverseitige Auto→Required-Eskalation oder ein versteckter Retry
  würde API-Semantik ändern und braucht eine ausdrückliche Eigentümer-Freigabe
  samt eigener Abnahme; beides ist **nicht** implementiert.
- **Eigentümer-Entscheidung vom 21.11.2026:** Negativbefund dokumentieren,
  **T-961 offen lassen**. Kein PASS, kein `done`, kein Code-Merge, kein
  Tag/Release. G-009 bleibt offen. Die bekannte Grenze steht jetzt in
  `docs/API_BRIDGE.md` inklusive des `required`-Befunds.

## 2026-11-21 — T-961: zwei Prompt-Kandidaten widerlegt — weiter NICHT abgenommen

- **PR #88 gemergt** (`8693b99`, Squash, alle CI-Checks gruen): Kontobefund und
  der ehrliche 2/8-Negativbeleg zum Verfügbarkeits-Kandidaten liegen jetzt in
  `master`.
- **Zweiter Kandidat ebenfalls widerlegt:** `f899ed3` nannte den Schema-Block
  `[FUNKTIONEN]` statt `[Client-Werkzeuge]` und vermied jedes Meta-Wort zur
  Verfügbarkeit. Alle lokalen Gates gruen (fmt, clippy, 1467 Tests), aber
  8 echte Live-Laeufe lieferten **6/8 streng gruen** und zwei Laeufe mit exakt
  der T-961-Fehlerklasse („Das read-Werkzeug ist in dieser Sitzung nicht
  verfügbar"). Das ist nicht besser als die 3/4-Baseline von `master`. Die
  Code-Aenderung wurde auf dem Branch mit `a7eb7ef` zurueckgenommen; Rohtrans-
  kripte und Auswertung liegen unter `docs/proofs/T-961/function-*`.
- **Ergebnis der Prompt-Versuche:** ausdrückliche Verfügbarkeitsregel 2/8,
  reine Umbenennung 6/8 (Baseline 3/4). Reine Prompt-Wortwahl ist als Richtung
  widerlegt; der Kontozustand scheidet als Ursache aus. Die Ursache bleibt
  **ungeklärt** (modellseitige Stochastik ist eine Vermutung, kein Beleg).
- **Entscheidung:** kein PASS, kein `done`, kein Merge eines Kandidaten,
  kein Tag/Release. **T-961 bleibt `free`**, G-009 bleibt offen. Die Bridge
  dokumentiert die bekannte Grenze jetzt in `docs/API_BRIDGE.md`, damit
  Aufrufer unvollständige Werkzeugläufe anhand der Marker pruefen.

## 2026-11-21 — T-961: Kontobefund belegt, Kandidat widerlegt — weiter NICHT abgenommen

- **PR #87 gemergt** (`dfebc47`, Squash, CI komplett gruen), `master` lokal und remote
  bei `e4386f0`. Er dokumentiert ehrlich, dass der Rahmungsfix die Intermittenz
  nicht beseitigt hat.
- **Kontozustand autonom geprueft** (DoD-Punkt, bisher als Eigentuemer-Auftrag
  markiert): read-only DOM der angemeldeten ChatGPT-WebView auf
  `/settings/personalisation` und im geoeffneten Dialog
  „Erinnerungszusammenfassung". Erinnerung aktiv, Abschnitt
  „Benutzerdefinierte Anweisungen" vorhanden, Zusammenfassung aktuell
  („Gerade aktualisiert"/„Überblick"), **kein Eintrag, der Werkzeugzugriff
  verneint**. Keine Einstellung geaendert, keine Zugangsdaten/Profile/Cookies
  gelesen, temporaere Diagnose-Patches vollstaendig zurueckgerollt. Persönliche
  Inhalte bewusst nicht committet. Beleg: `docs/proofs/T-961/account-state.md`.
- **Modellwechsel nicht moeglich:** `webagent model --brain chatgpt` meldet
  `aktuell ''` und `'model_menu' nicht anklickbar`; sichtbarer Modus lief in eine
  Navigation timeout. Kein Menue- und kein Konto-Settings-Eingriff vorgenommen.
- **Kandidat widerlegt:** `0039cd4` ergaenzte den Browser-Prompt um eine neutral
  formulierte Regel (Werkzeuge seien aktiv und ausfuehrbar, ein Ergebnis
  verbrauche das Werkzeug nicht). Alle lokalen Gates waren gruen (1467 Tests),
  aber 8 echte Live-Laeufe mit diesem Binary (Port 8788, frische Tokens,
  identische Prompts) lieferten nur **2/8 streng gruen**. In fuenf Laeufen
  verneinte das Modell den Werkzeugzugriff — teils schon vor dem ersten Aufruf
  („ist in dieser Sitzung nicht ausführbar"), teils nach drei Reads; ein Lauf
  endete in `ABSENDEKNOPF_DEAKTIVIERT` + `circuit_open`. Die Formulierung hat
  das Meta-Thema „Verfuegbarkeit" also eher provoziert. Die Code-Aenderung
  wurde auf dem Branch mit `63a6f7a` zurueckgenommen; der Branch enthaelt nur
  noch die Belegdokumentation (`docs/proofs/T-961/availability-*`).
- **Entscheidung jetzt:** kein PASS, kein `done`, kein Merge des Kandidaten,
  kein Tag/Release. G-009 bleibt offen. **T-961 bleibt `free`.** Die
  Intermittenz ist nicht erklaert; der Kontozustand scheidet als Ursache aus.
  Ein naechster Versuch muss ohne jedes Verfügbarkeits-Meta im Prompt auskommen
  und braucht erneut eine belastbare Raw-Transkript-Reihe.

## 2026-11-20 — T-961: Rahmungsfix gemergt, Intermittenz bleibt — weiter NICHT abgenommen

- **PR #86 gemergt** (`eaed3d1`, Squash, CI gruen, Branch geloescht). Ursache:
  `conversation_prompt` labelte Tool-Ergebnisse im **Verlauf** mit
  `[Client-Werkzeug-Ergebnis]` + `[tool_call_id]`, rendering aber die **letzte**
  Nachricht (genau Pi's `toolResult`) als nackten Text unter „Aktuelle Nachricht
  an dich". Red-Test zuerst (`current_tool_result_gets_explicit_label_and_call_id`),
  danach Fix in `src/api_bridge/content.rs`; Einzelnachricht-Pfad unveraendert.
  `docs/API_BRIDGE.md` dokumentiert die Rahmenung jetzt ausdruecklich.
- **Gates gruen:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1466 passed), davon 54 `api_bridge`-Tests. PR #85
  (Belegdoku) ist ebenfalls gemergt (`4b8a80c`). Lokal `master` = `eaed3d1`,
  Working Tree bis auf die hier dokumentierte Belegergaenzung sauber.
- **Neue echte Live-Laeufe mit finalem Binary `eaed3d1`** (SHA256
  `A2913A3D...904C7`, 4 aufeinanderfolgende Laeufe, gleiche Prompts, frische
  Tokens, Port 8788): 3 gruen (jeweils 5/5 Nonces `isError=false`, Finalanswer
  mit allen Markern, 0 Session-/Circuit-Fehler), aber **Lauf 1 negativ**:
  call_1..call_4 erfolgreich, dann *„Das fünfte read-Werkzeug ist in dieser
  Sitzung nicht verfügbar."* (4/5, fehlender Marker 5). Zwei Vorab-Laeufe mit
  dem Fix-Branch-Binary `37fe0fc` waren ebenfalls gruen.
- **Entscheidung: kein PASS, kein `done`.** T-961 bleibt `free`. Der Rahmungsfix
  ist korrekt und unit-getestet, aber die Fehlerklasse (Werkzeugzugriff nach
  eigenem Erfolg verneinen) ist mit dem finalen Binary reproduzierbar. Ein
  gruener Lauf, auch drei von vieren, beweist keine geheilte Ursache.
- **Naechster Schritt (nur Eigentuemer):** ChatGPT-Konto-Speicher /
  Personalisierung auf Erinnerungen pruefen, die Werkzeuge verneinen (DoD nennt
  das ausdruecklich). Kein Agent liest Profile, Cookies oder Zugangsdaten. Danach
  erneut mehrere stabile gruene Laeufe fordern. G-008 und G-009 bleiben offen.
- **Belege:** `docs/proofs/T-961/analysis.md`, `environment.txt`, `README.md`,
  `after-framing-negative.jsonl`, `after-framing-green-{1,2,3}.jsonl`,
  `after-premerge-green-{1,2}.jsonl` (jeweils mit `-summary.txt`). Neue
  `.gitattributes`-Regel haelt die Rohtranskripte bytegenau, damit die
  dokumentierten SHA256-Werte stimmen.

## 2026-11-20 — T-961: drei Blocker behoben, gruener Lauf, aber intermittierend — NICHT abgenommen

- **Branch/PRs (alle gemergt):** PR #82 `901e8e1` (Selektor-Drift `selectors/chatgpt.json`),
  PR #83 `5355e18` (Inference-Envelope toleriert Text vor `WEBAGENT_INFERENCE/1`),
  PR #84 `976abd3` (Login-Vorabpruefe weich: nur `Error`/`Cloudflare` hart,
  `Unbestimmt`/`LoginRequired` entscheidet die echte Antwort). Lokal `master`
  = `976abd3`, fast-forward mit `origin/master`. CI aller drei PRs gruen; lokal
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1465 passed) gruen.
- **Live-Diagnose (Port 8788, frischer Prozess-Token, keine Zugangsdaten/Profile gelesen):**
  drei deterministische Blocker gefunden und behoben:
  1. chatgpt-DOM hatte `data-message-author-role` entfernt -> Antwort im DOM
     sichtbar, aber `probe_generation` sah 0 Antworten -> `timeout_no_message`,
     Bridge 502. Neue stabile Anker (`data-dil-response`, `data-response-body`,
     `data-markdown-text-style='assistant-message'`, ...).
  2. chatgpt gab einen gueltigen Tool-Call mit `Nur Text\n` davor aus ->
     `strip_prefix` verwarf ihn, Pi sah nur Text, 0 Tool-Starts.
  3. `session_state=Unbestimmt` (False Positive) blockierte den Prompt trotz
     funktionierender Sitzung.
- **Echter gruener Lauf** (`docs/proofs/T-961/after.jsonl`, 51.529 B, SHA256
  `71FD0429...975C8977`): Textturn `PI_BRIDGE_OK` (`assistant_ok=True`), 6
  Tool-Starts, alle fuenf Nonces `isError=false`, Schlussantwort mit allen
  fuenf Markern, 0 Session-/Circuit-Fehler, 0 Suspect-Patterns. Harness-Gates
  (`toolStarts -lt 5`, fehlende Marker) waren unveraendert aktiv; Secret-Scan 0.
- **ABER nicht abgenommen:** `after-negative-intermittent.jsonl` (27.075 B),
  Minuten vorher mit **exakt demselben Binary** `976abd3`, reproduziert das
  Ursprungssymptom: call_1 erfolgreich (`isError=false`), dann *„Der
  Rückgabewert von nonce-1.txt fehlt..."*, 1/5 Tool-Starts. Ein einzelner
  gruener Lauf beweist bei nachweislich intermittierendem Verhalten keine
  geheilte Ursache. **T-961 bleibt `free`**, kein PASS, kein `done`.
- **Offen:** Intermittenz eingrenzen (Tool-Ergebnis-Rahmung in
  `conversation_prompt` variieren; Konto-Memory nur durch Eigentuemer
  einsehbar), dann mehrere stabile gruene Laeufe fordern. G-008 bleibt offen;
  G-009 (Tag/Release) bleibt offen. Belege: `docs/proofs/T-961/analysis.md`,
  `environment.txt`, `README.md`, `after-summary.txt`,
  `after-negative-intermittent-summary.txt`.

## 2026-11-20 — G-006 abgeschlossen: echte T-961-Belege (negativ)

- **Rohe Transkripte jetzt vorhanden:** `docs/proofs/T-961/before.jsonl` (29.021 B)
  und `after.jsonl` (29.305 B) sind echte Pi-1.5.2-JSON-Modus-Streams, secret-gescannt
  (0 Treffer). Commit `1049369` auf `master`. Vorher lag nur ein Platzhalter vor.
- **Ergebnis ehrlich negativ:** Beide Laeufe scheitern mit
  `Browser-Inference fehlgeschlagen: session_state=Unbestimmt`; `assistant_ok=False`,
  `tool_execution_start = 0/5`, Circuit Breaker oeffnet erneut (900 s). Der
  Nach-Fix-Lauf wurde nach einem Harness-StrictMode-Fehler korrigiert wiederholt;
  das ausgewertete `after.jsonl` stammt aus diesem echten zweiten Lauf.
- **Kontozustand geprueft** (nicht `not accessible`): `GET /api/health/brains` meldet
  fuer chatgpt `login_state="unknown (old)"`, `profile_exists=true`, keinen
  erfolgreichen Run — Anmeldung **nicht bestaetigt**. Kein Agent hat Zugangsdaten
  oder Profildateien gelesen.
- **Konsequenz:** T-961 bleibt `free`. Das ist ein Negativbeleg, keine Abnahme.
  Rezertifizierung erst nach Anmeldung durch den Eigentuemer
  (`webagent login --brain chatgpt`) und erneutem Lauf. G-006 ist als
  "Belege beschafft" done; G-008/G-009 bleiben offen.
- **PR #81 (T-949) gemergt:** Squash `0f80973`, alle CI-Checks gruen; lokaler
  `master` auf `origin/master`, danach dieser Handover-Stand committed.

## 2026-11-20 — T-949 (Branch `fix/T-949-consent-guard`)

- **G-008, lokal abgeschlossen:** `dismiss_consent` prueft vor dem Klick auf
  `dialog_dismiss_button` die Beschriftung des Treffers. Zustimmungstexte
  (`Accept and Continue`, `OK, I agree`, `Akzeptieren`, `Zustimmen`,
  `einverstanden`) werden nicht geklickt; fail-closed bei nicht lesbarer
  Beschriftung. Cookie-Ablehnung und Nur-notwendige-Cookies bleiben bewusst
  unangetastet (non-goal, ueber `NON_AGREEMENT_MARKERS` explizit erlaubt).
- **Beleg:** `docs/proofs/T-949/measurement.md` (Mock-DOM-Tests + Gate-Ergebnisse,
  kein Live-Anspruch). `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test --lib` (1460 gruen) und
  `taskboard_invarianten_halten` gruen. T-949 im Board auf `done`, Spiegel
  synchron (13 freie Tasks).
- **Nicht erledigt:** kein Live-Zustimmungsdialog gefahren (gehoert zu T-948);
  Board-Status liegt auf dem Branch und wird mit dem Merge wirksam.

## 2026-11-20 — G-008 Triage der restlichen freien Tasks

Einordnung der freien Tasks nach dem, was **ohne** Zugangsdaten/Profile und ohne
Live-Anbieter-Sitzung ehrlich abgeschlossen werden kann. Diese Triage ist keine
Abnahme und kein Defer-Beschluss; sie bereitet die Entscheidungsvorlage fuer den
Eigentuemer vor (G-008 bleibt offen, G-009 bleibt offen).

| Kategorie | Tasks | Begruendung |
|---|---|---|
| A — lokal umgesetzt + geprueft | T-949 | Code-/Mock-DOM-Beleg, PR #81 offen; nach CI-Fix (Referenz-Banner im Proof) erneut in CI. |
| B — lokal umsetzbar, noch offen | T-926, T-940, T-946, T-953, T-957 | DoD mit `cargo test --lib` und/oder statischem Beleg erfuellbar; keine Live-Sitzung noetig. Kandidaten fuer weitere Scheiben. |
| C — braucht echte Live-Sitzung / Anmeldung | T-927, T-928, T-948, T-961 | DoD verlangt frische Live-Matrix-/Effort-Belege, einen echten mistral-Zustimmungsdialog bzw. rohe Pi-Transkripte vor/nach Fix. Integrator liest keine Zugangsdaten und keine Profile. T-961-Lauf siehe G-006. |
| D — teils Live-Messung noetig | T-942, T-944, T-950 | DoD verlangt zusaetzlich eine Messung je Anbieter (Quell-Markdown gegen ausgelieferten Text, Denkmodus-Turn, Websuche-Zustand vor dem Senden); nicht rein lokal belegbar. |
| E — explizit deferred | T-914 | Im Taskboard bereits `[DEFERRED]` markiert; kein aktiver Auftrag. |

Konsequenz: autonom weiter bearbeitbar sind Kategorie B; Kategorie C/D benoetigen
entweder eine Anmeldung durch den Eigentuemer oder einen ausdruecklichen
Defer-Beschluss. Ein solcher Beschluss wird nicht stellvertretend erfunden.

## 2026-11-20 — Zielstack G-005..G-007 (autonomer Fortschritt)

- **G-005 done:** PR [#80](https://github.com/st0rax/webagent-rs/pull/80) wurde als Squash-Commit `493e9a613d72dcf319151b813ed7ffc84662c657` in `master` gemergt; lokaler `master` ist auf `origin/master` fast-forwarded und war bei Beginn dieser Scheibe sauber.
- **G-006 in_progress:** Echte T-961-Live-Messung wurde vorbereitet und der Vor-Fix-Lauf ausgefuehrt. Frisch gebaute Binaries: `master` (`493e9a6`, `webagent 0.11.3`) und isolierter Worktree `dd78128` (`webagent 0.11.3`). Pi ist installiert als **1.5.2** (nicht mehr die im alten Plan genannte 0.84.4). Die Bridge startet auf `127.0.0.1:8788` mit frischem, nicht gespeichertem Prozess-Token; Port 8787 bleibt bewusst unberuehrt (dort laeuft ein fremder Free-Router-Prozess).
  - Vor-Fix-Rohtranscript: `C:\Users\storax\AppData\Local\Temp\webagent-t961-harness\before.jsonl` (29.021 Bytes, frei von Token-/Cookie-/Key-Mustern). Beide Pi-Turns endeten mit `502: Browser-Inference fehlgeschlagen: session_state=Unbestimmt`; `tool_execution_start` = 0/5. Der Circuit Breaker fuer `chatgpt` waren daraufhin 900 s offen.
  - Wichtiger Befund: Das Repo-Skript `scripts/test-pi-provider.ps1` erkennt `PI_BRIDGE_OK` im echoenden User-Prompt und meldet den Textturn daher falsch als erfolgreich. Die temporaere Harness-Kopie prueft jetzt nur noch die Assistant-Nachricht. Live-Eingaben (Prompts, Modell, Port) blieben unveraendert.
  - Der Nach-Fix-Lauf wurde nach Ablauf des Breakers ausgefuehrt; echte rohe Belege und der Kontozustand sind inzwischen committed (siehe Abschnitt G-006 oben). Ergebnis negativ, T-961 bleibt `free`.
- **G-007 done:** PR [#69](https://github.com/st0rax/webagent-rs/pull/69) wurde am 20.11.2026 19:32 UTC bewusst als **ueberholt geschlossen**, nicht gemergt. Sein Code (Systemkontext, `content:null` + `tool_calls`, Tool-Fortsetzung) liegt bereits via PR #78/`864e346` und PR #80/`493e9a6` in `master`; gegenueber `master` verblieben nur T-934-Referenzbelege. Die offen gebliebenen Live-Gates auf dem veralteten Head-SHA `f361402` wurden nicht fabriziert.
- **Offen:** G-006 Nach-Fix-Ergebnis, G-008 Triagierung der restlichen 14 freien Tasks, G-009 Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-11-20 — T-966: Acceptance integrity (Arbeitsstand)

- **Status (historisch zum Zeitpunkt der Arbeit):** Branch `fix/T-966-acceptance-integrity` (abgezweigt von `master` bei `eaac398`) wurde gepusht; PR [#80](https://github.com/st0rax/webagent-rs/pull/80) war zu diesem Zeitpunkt offen. Zwei Commits: `cac7c29` (Taskboard/Gates), `b0628a2` (Belege/Doku). **Inzwischen gemergt** als `493e9a6`, siehe Abschnitt oben.
- **Ausgangslage:** `GOALS.md` behauptete „Projekt abgeschlossen“ und G-002 done, obwohl `docs/TASKBOARD.json` auf einen 1-Task-Stub reduziert war, `docs/proofs/T-961/environment.txt` `BLOCKED_BY_MISSING_EVIDENCE` meldet und PR #69 offene Live-Belege hat.
- **Korrigiert:**
  - Vollständiges Taskboard aus `dd78128` wiederhergestellt (91 Tasks, 77 done, 14 free); T-961 bleibt `free`. Kein History-Rewrite, kein Force-Push.
  - `docs/TASKBOARD.md` um einen maschinenlesbaren Spiegelblock ergänzt; `src/taskboard.rs` prüft Taskzahl, IDs und Spiegelabgleich im normalen `cargo test` und damit auch in CI.
  - T-961-Belege bereinigt: keine PASS-Behauptung, keine nicht existierenden SHAs, `before.jsonl`/`after.jsonl` als ausdrückliche `not_run`-Platzhalter.
  - Statusdokumente auf eine ehrliche Version gebracht; `README.md` nennt den realen Tag `v0.11.4` mit Hinweis auf die offene Abnahme.
- **Gates (20.11.2026, Windows/GNU, Rust 1.100):**
  - `cargo fmt --all -- --check` ✅
  - `git diff --check` ✅
  - `cargo clippy --locked --no-default-features --all-targets -- -D warnings` ✅ (nur zwei Cargo-Manifest-Hinweise zu ungenutzten optionalen Deps ohne `webview`; keine Rust-Warnungen)
  - `cargo test --locked --no-default-features` ✅ — Lib 1448 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
  - Negativprobe: `docs/TASKBOARD.md` wurde absichtlich auf `| T-001 | free |` geändert; `taskboard_invarianten_halten` schlug mit „nicht mit docs/TASKBOARD.json synchron“ fehl und die Datei wurde sofort wiederhergestellt.
  - Ein erster Testlauf scheiterte an einer veralteten lokalen Linker-Umgebungsvariable sowie einem fehlenden C-Compiler; beides war Werkbank-Setup, kein Repo-Code. Die Defensive gegen Ersteres (`is_test_run`) ist jetzt Teil des Fixes.
  - `cargo clippy --locked --all-targets -- -D warnings` (Default-Features/WebView) ✅
  - `cargo test --locked` (Default-Features/WebView) ✅ — Lib 1454 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
- **GitHub-CI auf PR #80:** alle Checks grün in beiden Läufen (Push + PR-Event): `build & test`, `build & test (Windows, volle Features)`, `build (Linux, WebView via WebKitGTK)`, `aarch64`.
- **PR #69** ([Kommentar](https://github.com/st0rax/webagent-rs/pull/69#issuecomment-7048051133)): bewusst nicht gemergt, weil der PR selbst Live-Pi-Re-run und generisches curl/SDK Chat+Tools als `pending` führt.
- **Offen / nicht übernommen:** Echte Live-Rezertifizierung (Eigentümer muss anwesend sein; ein Agent liest keine Zugangsdaten oder Profile), PR #69 mit `pending`-Live-Belegen, kein neuer Tag/Release ohne Freigabe.
- **Nächste Aktion (damals):** Integrationsentscheidung zu PR #80 durch den Eigentümer. PR #80 ist inzwischen gemergt; T-961 bleibt `free`, bis echte rohe Pi-Transkripte vor/nach dem Fix und eine Kontozustandsprüfung vorliegen. Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-10-15 — T-965: English-first maintained documentation

- **Status:** `done`; PR [#76](https://github.com/st0rax/webagent-rs/pull/76) merged.
- **Merge commit:** `3778f31a83535690e3746501cd642c46b873c70e`.
- **Branch:** `docs/T-965-english-first-docs`; task status is `done` in `docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md`.
- **Policy:** English is the default for maintained documentation, task titles, agent instructions, issues, and pull requests. Preserve dated historical reports/evidence when rewriting would damage traceability; add English summaries where useful. Product-facing output language remains a separate runtime decision.
- **Changed scope:** `README.md` and `docs/NEWCOMER_GUIDE.md` are now in English; the README retains the quick start, architecture/module map, commands, profile and environment-variable guidance, release/platform boundaries, and historical provider context. The policy and T-965 status are recorded in the taskboard and handover.
- **Non-goals:** no Rust code, runtime behavior, release, tag, login-profile, or provider configuration changes. This does not claim that every maintained or historical document in the repository has already been translated; remaining maintained docs should be migrated in follow-up tasks.
- **Verification:** PR #76 merged; `docs/TASKBOARD.json` parsed successfully; README and newcomer-guide section headings are English; links to existing Markdown files and the README quick-start anchor were checked through GitHub. Local `git diff --check`, automated link-check tooling, and Rust tests were not run in this remote editing session.
---


## 2026-10-15 — T-964: Kontextfreier Repository-Einstieg abgeschlossen

- **Status:** `done`; PR [#73](https://github.com/st0rax/webagent-rs/pull/73) gemergt.
- **Merge-Commit:** `76ba3b303d78135e8927b2bd7f1cd382732fdc8e`.
- **Branch:** `docs/T-964-newcomer-onboarding`; Taskboard-Abschluss in [PR #74](https://github.com/st0rax/webagent-rs/pull/74) dokumentiert.
- **Einstieg:** [`docs/NEWCOMER_GUIDE.md`](NEWCOMER_GUIDE.md) ist die zentrale Landkarte für neue Menschen und Coding-Agenten; die README verlinkt sie sichtbar und trennt Benutzung von Entwicklung.
- **Quellenklarheit:** Guide erklärt Projektmodell, Dokumentenrollen, Architekturbegriffe, historische vs. aktuelle Informationen, Bazaar-Claim und Sicherheitsgrenzen.
- **Scope:** nur README, Einstiegsdokument und Taskboard/Handover; keine Rust-Code-, Laufzeit-, Login-Profil-, Tag- oder Release-Änderungen.
- **Verifikation:** Taskboard-JSON geparst; relevante Guide-Zielpfade auf GitHub geprüft; Taskboard-Spiegel enthält T-964 und den Guide-Link. `git diff --check` und Rust-Gates wurden nicht lokal ausgeführt; für reine Dokumentation wurden keine Rust-Tests gestartet.


## 2026-10-15 — T-963: Bazaar-konforme Integration abgeschlossen

- **Status:** `done`; PR [#72](https://github.com/st0rax/webagent-rs/pull/72) gemergt.
- **Merge-Commit:** `23ccca28dfd856d2bc90bcc9744aae136d4866b9`.
- **Branch:** `docs/T-934-bazaar-agent-prompt`; Taskboard-Claim und Handover sind dokumentiert.
- **Prompt:** [`docs/PI_INSPIRED_CODING_AGENT.md`](https://github.com/st0rax/webagent-rs/blob/master/docs/PI_INSPIRED_CODING_AGENT.md).
- **Repo-Workflow:** T-963 steht in `docs/TASKBOARD.json` (Quelle der Wahrheit) und `docs/TASKBOARD.md` (Spiegel). Der vorhandene Bazaar-Arbeitsprozess bleibt erhalten.
- **Scope eingehalten:** nur Dokumentation und Taskboard; keine Rust-Code-, Laufzeit-, Browserprofil-, Login-, Tag- oder Release-Änderungen.
- **Verifikation:** Taskboard-JSON erfolgreich geparst; Taskboard-Spiegel und Handover geprüft; keine abschliessenden Leerzeichen in den vier geprüften Dateien. Lokales `git diff --check` und Rust-Tests wurden in dieser Remote-Umgebung nicht ausgeführt.

---

> **Aktualisiert 2026-09-12:** **G-001 abgeschlossen** — alle Zeilen der
> v1.0-Definition-of-Done verifiziert (`docs/proofs/G-001/g001-abschluss-2026-09-12.json`).
> - Voll-Gates am Abschluss-Stand (`2589b0a`) erneut grün: `cargo fmt --all -- --check`,
>   Clippy `-D warnings` headless **und** full-features, `cargo test --locked`
>   **1388 passed / 0 failed / 1 ignored + 7 bin**, `cargo test --locked
>   --no-default-features` 1382 + 7 bin, `git diff --check` sauber.
> - Toolchain-Norm auf rust 1.97 (`2589b0a`): rustfmt-Drift über 13 Dateien
>   bereinigt, sechs neue Clippy-Lints behoben (u. a. `items_after_test_module`,
>   `single_match`, tote Konstante `MEMORY_CONTEXT_LIMIT`, nie gelesenes `wiki`-Feld).
> - **Release `v0.11.3` veröffentlicht** —
> https://github.com/st0rax/webagent-rs/releases/tag/v0.11.3 (Tag `7021959`,
> CI `34696249277` success, Assets: Windows exe + WebView2Loader.dll + sha256,
> Linux, Android aarch64):
> - **Web-UI vom T-203-Fake-Prototyp auf die echte `/api/*`-Anbindung gehoben**
>   (Phase-9 T-901–T-906 done):
> - T-905 (`664f4e8`): Brain-Status, Sessions anlegen/wechseln, Chat-Stream
>   (`events?since=`), Stop, Quellen, Gruppen, Brain-Detail, Upload, Brain-
>   Fenster show/hide; Layout im T-203-Grok-Schema, Einzeldatei < 48 KiB,
>   Tests erzwingen `fetch(`-Anbindung.
> - T-906 (`5c42656`): T-905-Restlücken geschlossen — Gruppenlauf öffnet die
>   erzeugte Session (`run_id`) und rendert die Events live (Runden-Stati,
>   TextDeltas, Synthese, Done), Datei-Upload im Composer
>   (`POST /sessions/{id}/upload`, 202 `accepted`), Brain-Fenster show/hide im
>   Detail; Beweis `docs/proofs/T-906/`.
> - **Code-Review (`44f7a1e`):** reviewer-Bot REQUEST_CHANGES → alle 11 Befunde
>   umgesetzt: XSS-Klasse behoben (echtes `escRow`, DOM-Selects, CSP-Meta),
>   `TextComplete`-String-Handling (live belegt: Backend sendet JSON-String),
>   Poll-Race-Guard, Gap-/404-Recovery, 409-`session_done`-Text, Backoff,
>   ehrlicher Upload-Text. Beweis `docs/proofs/T-906/webui-review-2026-09-12.json`.
> - Gates für 0.11.3: **1388 passed / 0 failed / 1 ignored**; `cargo check` grün;
>   Changelog `[0.11.3]` finalisiert; Beweise T-901..T-906 in `docs/proofs/`.
> - T-805/T-806/T-807 abgenommen: 46 frische Live-Verify-Belege (alle 9 Brains)
>   in `docs/proofs/T-501/live_verify_2026-09-12.json`, Matrix `as_of 2026-09-12`.
> - T-806 Receipt-E2E live belegt (negativ fail-closed + positiv done,
>   `docs/proofs/T-806/`); T-807 Windows-Prozessprobe + Release-Binary-E2E
>   (`docs/proofs/T-807/`), alle 26 Tasks im TASKBOARD `done`.
> - Release-Fix im CI: `build.rs` linkt `resource.o` nur noch unter GNU/mingw
>   (MSVC-CI nutzt winres `resource.lib`) — behebt `LNK1181` im Release-Workflow.
> - Gates: `cargo test --lib` 1388 passed / 0 failed / 1 ignored;
>   `--features tui` und `--no-default-features` grün.
> - perplexity als Custom-Brain registriert (`custom_brains.json`, `%LOCALAPPDATA%\\webagent\\data`) — diagnose/verify erreichbar
> - deepseek model/mode_switch removed by-design (Modus-Bar vom Anbieter entfernt)
> - effort-Spalte komplett vermessen: claude/qwen passed, alle anderen not_run/failed
>   (kimi: Pfad `['Denkaufwand Hoch','Hoch']` klickt, verify-Failed strukturell)
> - gemini/kimi/zai model_switch Frischbelege, chatgpt model_switch failed (Sofort-Pill)
> - chatgpt.json reasoning_effort_menu entfernt (komplett deckungsgleich mit model_menu)
> - Matrix-Stand: **130 Zellen, 10 Brains × 13 Areas** vollständig, `as_of` 2026-09-11
> - restliche Restposten: streaming/attachment bereits 10/10 passed (PR #44/#45/#46);
>   managed_tools 10/10 failed by-design; nur `model/auto` und zai-Webchat-Flaky offen

## Fortsetzung G-001 / T-501 am 2026-09-07

Claim: `chatgpt-codex`, Branch `fix/T-501-model-proof`, Basis `28e9f1c`.
Der Nutzer hat die Fortsetzung von G-001 beauftragt. Erste Scheibe: den
Modellwechsel-Beleg korrigieren; `verify model_switch` prueft bisher nur den
generischen Zustand des Menueknopfs. Der Gemini-Beleg vom 2026-09-06 zeigt
`aria-expanded=false -> true` bei unveraendertem Modell `Flash`.
Das ist kein Modellwechsel. Matrix vor Audit: 109 passed, 10 failed,
4 unreachable, 7 not_run; keine aktuelle Live-Rezertifizierung.

Isolation: eigener Worktree. Der vorhandene Arbeitsbaum
`C:/Users/storax/projects/GitHub/webagent-rs` auf `feature/T-501-effort-rest`
enthaelt fremde, uncommittierte ChatGPT-Selektoren und Survey-Dateien; diese
bleiben erhalten. Die dort laufende WebAgent-Instanz wird nicht ersetzt.
Die Code-Scheibe ist jetzt lokal abgenommen: Modellwahl verlangt tatsaechlichen
Laufzeitwechsel plus Restore; alte Trigger-PASS sind per Hash-Version ungueltig.
Alle vier Gates gruen: `cargo test --locked --lib` (1331 passed, 1 ignored),
striktes Clippy, no-default-features und TUI-Check. Die Tests legten eine
unabhaengige Race in `benchmark::harvest` offen; die vier Tests teilen einen
Testdatenordner und sind nun per Test-Mutex isoliert. Offene Folge: sechs
Modellzellen erneut live messen, T-501 insgesamt bleibt offen.

> **Checkout-Hinweis (HEAD, 2026-09-07):** dieser Arbeitsbaum lief vor dem
> Rebase auf `feat/durable-transaction-log` (Basis `505d416`). Run-Events
> SHA-256-verknuepft + fsync; `cargo test --lib --no-default-features` 1278
> passed / 1 ignored. Release-Artefakt
> `webagent-0.11.1-transaction-log-repl-fix-x86_64-pc-windows-gnu.exe`.

**Aktualisiert:** 2026-09-08
**Zweck:** verbindlicher Wiedereinstieg und operative Wahrheit. Historische Befunde stehen in `docs/OVERVIEW.md` sowie in den datierten Übergaben; diese Datei ersetzt sie nicht, sondern hält nur den aktuellen Abschlusspfad fest.

**Stand 2026-09-05:** `master` auf `9793623` (PR #33 gemergt: Port-Vereinheitlichung „Ein Listener, zwei Rollen“ — Web-UI + API-Bridge gemeinsam auf `8788`, `api serve` als Alias; `8787` ist historische Dead-Zahl; Details [`CLI_UI_REDESIGN.md`](CLI_UI_REDESIGN.md) §8; `a934db2`/PR #32 `ask` war der Vorgaenger-Commit). **T-501-Nachzieh-Runde 2026-09-05 auf Port 8788** (Breaker-Cooldowns abgelaufen, Bridge mit echtem Login-Profil-Root): `api_responses` jetzt **9/9 passed** (mistral+zai nachgeholt; mistral-Deltas Provider-Timestamps, finale `.done`-Antwort sauber RESP_OK), `streaming` bleibt **6/9** (kimi Reasoning-Echo, mistral Timestamp, zai „Thinking...“), `attachment` 5 Brains frisch gemessen. **Nachtrag `attachment kimi` (2026-09-05, Fokus laut Eigentuemerentscheidung):** Der fruehere 502 `ABSENDEKNOPF_DEAKTIVIERT` lag am entarteten 1x1/70-Byte-Testbild, nicht an der Bridge - mit echtem 256x256-Bild (PIL, ~1,1 kB) laeuft der komplette Upload->Send->Antwort-Pfad durch: **kimi attachment jetzt `passed` (200 "RED", 36,6 s, frische Session)**. qwen/zai bleiben `failed` (auch mit echtem Bild "0 von 1 Dateien": echter SPA-Upload-Gap, Re-Run-Belege `attachment_{qwen,zai}_real_image_2026-09-05.json`), mistral Timestamp, auto Timeout 240s. `managed_tools` by-design 400 erneut bestaetigt. Matrix-Gesamtstand **98/130 passed, 17 failed, 6 unreachable, 9 not_run**; T-501 im TASKBOARD auf `claimed` zurueckgestellt (DoD weiterhin NICHT done: streaming 3, attachment 4, model/effort login-gebunden). **Web-UI-Feature (2026-09-05):** `web/index.html`-Composer zeigt bei leerer Eingabemaske einen Beispiel-Hinweis (`/quelle list`, `/quelle ... --save`, `@brain`) wie die opencode-Promptleiste; verschwindet beim Tippen (`syncHint()`), wirkt nach Neubau (eingebettet via `include_str!`). Belege `docs/proofs/T-501/*2026-09-05.*`, Harness `docs/proofs/T-501/rerun_2026-09-05.py`. Alle aelteren `127.0.0.1:8787`-Erwaehnungen in `docs/proofs/*` bleiben historische Belege und werden nicht angefasst. **Login-Realitaet (Messung 2026-09-05, DeepSeek zuerst):** isolierter No-Login-Smoke via `WEBAGENT_ROOT`-Temp-Root (frisches, leeres Profil; Breaker-Zustand im Temp — echter Betrieb unberuehrt): `webagent ask --task "Antworte exakt und nur: DS_NOLOGIN_OK" --brain deepseek --chat --json --headless --no-memory` → `reason=session_state=LoginRequired`, `latency_ms=47561`, deterministische Breaker-Sperre 21600s (nur im Temp-Root), Exit 1. **Befund: deepseek funktioniert ohne Login NICHT** — bestaetigt den frueheren Eintrag in `CLI_UI_REDESIGN.md` §2 P2; der README-Login-Claim („angemeldeter Browser bei dem Dienst, den du nutzen willst“) bleibt damit korrekt neutral formuliert. Protokoll: `%LOCALAPPDATA%\Temp\opencode\wa-nologin\deepseek-smoke.log`.

**Historischer Kopf (2026-09-02):** Phase 4 komplett inkl. T-404 SDK-Blackbox (Python/JS-SDK + urllib/fetch, Dumps `docs/proofs/T-404/`). Live T-301/T-302/T-501 ohne Freigabe nicht gegen echte Brains. Naechste Code-Zelle war T-601.

## Verbindlicher Produkt-Neuschnitt vom 01.09.2026

Die bisherigen Teilziele rund um TUI, Browser-Inference-Bridge und einzelne
OpenAI-foermige Routen werden durch einen neuen, messbaren Produktvertrag
geordnet. Der vollstaendige Erkenntnisstand, die Zielarchitektur, Phasen und
Abnahmematrix stehen in [`WEB_UI_API_TOOL_RESET.md`](WEB_UI_API_TOOL_RESET.md).

Kurzfassung:

1. `webagent` erhaelt eine lokale, menschenfreundliche Web-UI als primaere
   Oberflaeche. Die rund 4.400 Zeilen TUI-Code werden vor dem Umbau durch einen
   GitHub-Archivbranch und ein Tag erhalten und bleiben bis zur UI-Abnahme als
   Legacy-Pfad baubar.
2. Claude ist der strengste Referenz-Brain fuer Chat, Modellwahl, Aufwand,
   Attachment und Streaming. Dieselbe Funktionsmatrix muss danach jeder
   beworbene Brain ueber Web-UI und API bestehen.
3. Der OpenAI-Endpunkt wird nicht mehr ueber einzelne HTTP-200-Smokes bewertet.
   Er bekommt versionierte Models-, Chat-Completions- und Responses-Profile,
   offizielle SDK-Black-box-Tests und explizite Fehler fuer nicht umgesetzte
   Semantik. Client-spezifische Sonderfaelle sind ausgeschlossen.
4. WebAgent implementiert selbst einen kleinen Managed-Tool-Kern nach dem
   Umfang von Pi 0.84.4: `read`, `bash`, `edit`, `write`. Pi bleibt reine
   Referenz und wird weder Abhaengigkeit noch Executor. Web-UI und API verwenden
   dieselbe Registry, Policy, Execution und Ereignisstrecke.
5. Der bisherige Prompt, der Systemkontext, Tool-Monster und Verlauf als
   vermeintliche Browseranweisung vermischt, wird verworfen. Reiner Chat zeigt
   nur einen gekennzeichneten Gespraechsverlauf; Managed Agent erklaert dem
   Brain die reale lokale Vermittlung offen und kompakt.
6. Streaming ist Pflicht: sofortiger sichtbarer Zustand, inkrementelle
   Textdeltas, Werkzeugereignisse, Stop und reconnectbarer SSE-Strom. Ein erst
   am Ende sichtbarer Text gilt als `buffered` und besteht das Streaming-Gate
   nicht.

Die folgenden Abschnitte bleiben als historische Teilbelege erhalten. Sie
duerfen insbesondere nicht als Nachweis fuer ein fertiges Web-UI, vollstaendige
OpenAI-Kompatibilitaet oder All-Brain-Toolparitaet gelesen werden.

## Virtuelles AutoRouter-Modell

**Aktualisiert:** 2026-09-01
**Branch:** `feat/browser-inference-provider`

Der OpenAI-Modellkatalog enthaelt nun an erster Stelle das virtuelle Modell
`webagent/auto`. Es wird nicht als statischer Alias behandelt: Der zentrale
Browser-Inference-Pfad klassifiziert Bildgenerierung, Audio- und Bild-Input,
Function-Tools, Coding, aktuelle Recherche sowie allgemeinen Text und waehlt
dafuer ein geeignetes reales Brain. Offene Circuit Breaker werden bei der
Auswahl uebersprungen; fuer zwingende Klassen wie Audio scheitert der Router
ehrlich, wenn kein geeignetes Brain verfuegbar ist. Die Entscheidung erscheint
als strukturierte Zeile im Serverlog.

Die Regeln und ihre Reihenfolge sind in `docs/API_BRIDGE.md` dokumentiert. Die
gleichen Regeln greifen fuer Chat Completions, Responses, Anthropic Messages,
OpenAI Audio und OpenAI Images, weil die Materialisierung direkt vor dem
gemeinsamen Browserturn erfolgt.
Zusätzlich akzeptiert die Modellauflösung neben den kanonischen
`webagent/...`-IDs auch die sichtbaren ZCode-Aliase `wa/...`; die Antwort des
Modell-Endpunkts bleibt kanonisch und liefert weiterhin `webagent/...`.

| Gate | Ergebnis |
|---|---|
| fokussierte API-/Router-Tests | 39 bestanden |
| Vollfeature-Clippy `-D warnings` | bestanden |
| No-Default-Features-Clippy `-D warnings` | bestanden |
| vollstaendige No-Default-Features-Suite | 1.198 bestanden, 1 ignoriert; 7 Binaertests bestanden |
| Windows-GNU-Debug-Build | bestanden |
| Headless Katalog-Smoke | `webagent/auto` als erstes Modell mit Routingmetadaten |
| Headless Text-Smoke | zu ChatGPT geroutet, exakt `AUTO_ROUTE_OK`, 18,40 s |
| Headless WAV-Smoke | zu Gemini geroutet, `Die Pruefziffer lautet 789`, 22,33 s |

Nach dem Live-Befund, dass ChatGPT und Claude bei Audio teils HTTP 200 mit einer
reinen Absage liefern, klassifiziert die Bridge eindeutige
Transkriptionsabsagen jetzt als HTTP 502. Damit werden diese Provider nicht
mehr als scheinbar erfolgreiche Audio-Backends ausgegeben; die beworbene
Audio-Eingabe bleibt auf `gemini` und `webagent/auto` begrenzt.

### Frische headless Text-Rezertifizierung (2026-09-01)

Ein serieller Smoke mit eindeutigen Antwortmarkern lief gegen den aktualisierten
Endpoint auf `127.0.0.1:8787`; parallele Browserruns wurden vermieden.
ChatGPT (25,7 s), Claude (29,4 s), DeepSeek (14,3 s), Gemini (34,6 s), Kimi
(22,8 s), Mistral (48,8 s), Qwen (31,1 s) und Perplexity (16,7 s) antworteten
jeweils mit dem angeforderten exakten Marker und HTTP 200. Nach Ablauf des
providerbezogenen Circuit Breakers antwortete Z.ai im Recovery-Smoke ebenfalls
mit HTTP 200 in 20,7 s. Die Oberfläche stellte dem Marker den UI-Titel
`Thought Process` voran; die API-Schicht entfernt diesen Titel jetzt
konservativ nur für Z.ai und lässt den eigentlichen Antworttext unverändert.

Ein ZCode-kompatibler Streaming-Smoke gegen `webagent/chatgpt` lieferte HTTP
200, Keep-Alive-Frames, inkrementelle `delta.content`-Chunks und einen
abschließenden `finish_reason: stop` (18,4 s).

## Aktueller Fix: multimodaler Upload-Fallback für dynamische Browser-UIs

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`

Die bisherige Bild-/Audio-Strecke setzte ausschließlich ein bereits vorhandenes
`input[type=file]` voraus. Genau das führte bei DeepSeek, Gemini, Kimi und Mistral
zu `no_file_input`, sobald eine alte Bildnachricht im ZCode-Verlauf mitgesendet
wurde. `browser::send::attach_files` öffnet jetzt zunächst vorsichtig die
konfigurierte Attach-Oberfläche (untrusted DOM-Klick), wartet auf ein dynamisch
gerendertes Datei-Input und fällt danach auf Paste/Drop zurück. Der Fallback gilt
nur bei einer sichtbaren neuen Attachment-Vorschau als erfolgreich; andernfalls
kommt weiterhin ein erklärender 502. Native Dateidialoge werden nicht geöffnet.

### Korrektur zum Providerbefund vom 31.08.2026

Die frühere Formulierung „kein Datei-Upload“ war für DeepSeek, Gemini, Kimi und
Mistral zu stark: Eine frische DOM-/Screenshot-Inventarisierung zeigt bei allen
vier eine Upload-Oberfläche. DeepSeek rendert ein verstecktes
`input[type=file]` und eine Büroklammer; Gemini erzeugt nach „Uploads & Tools“
versteckte Inputs; Mistral rendert `input[name=file-upload]`; Kimi zeigt die
Toolkit-/+-Schaltfläche und Attachment-Zustände. `no_file_input` war dort ein
Entdeckungsfehler eines vollständigen Verlaufs, kein Beleg für fehlende
Providerfunktion.

Der Windows-Treiber versucht deshalb neben dem bestehenden Paste/Drop-Weg einen
nativen WebView2-CDP-Upload (`DOM.setFileInputFiles`) und akzeptiert sowohl die
direkte WebView2-Antwortform als auch den üblichen `result`-Wrapper. Dieser Weg
wurde live mit DeepSeek (23,33 s), Gemini (26,06 s), Kimi und Mistral als Bild-Request bis
zur Antwort `IMAGE_INPUT_OK` verifiziert. Kimi verwendet dafür eine echte
Windows-Dateiübergabe (`CF_HDROP`) mit abgefangenem WebView2-Dateichooser und
den realen Pfeil-Button seines Lexical-Composers. Ein Submit gilt erst als
bewiesen, wenn Kimi den Composer tatsächlich konsumiert hat. Audio wurde mit
Gemini anhand einer synthetisierten WAV-Sprachphrase inhaltlich verifiziert.

Die generische Selektormaske enthält dafür providerneutrale Attach-Signale
(`aria-label`, `data-testid`, `data-tooltip`, Datei-/Upload-Beschriftungen). Die
Text-Circuit-Breaker-Behandlung klassifiziert sowohl `no_file_input` als auch
`no_file_input_and_paste_not_confirmed` als request-spezifische Fähigkeitsgrenze;
Textturns eines betroffenen Brains bleiben davon unberührt.

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| fokussierte Upload-/Relay-Tests (`cargo test --locked --no-default-features send --lib`) | 12 bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo check --locked` (WebView2-Feature) | bestanden |
| Live-Bild-Input-Smoke `webagent/chatgpt` (1x1 PNG, exakte Antwort `IMAGE_INPUT_OK.`) | bestanden |
| Live-Bild-Input-Smoke `webagent/deepseek` (30 KB PNG, `IMAGE_INPUT_OK`) | bestanden (23,33 s) |
| Live-Bild-Input-Smoke `webagent/gemini` (30 KB PNG, `IMAGE_INPUT_OK`) | bestanden (26,06 s) |
| Live-Bild-Input-Smoke `webagent/kimi` (512x512 PNG, `IMAGE_INPUT_OK`) | bestanden (HTTP 200) |
| Live-Bild-Input-Smoke `webagent/mistral` (512x512 PNG, `IMAGE_INPUT_OK`) | bestanden (HTTP 200) |
| Live-Audio-Input-Smoke `webagent/gemini` (WAV-Sprachphrase, `AUDIO INPUT OK`) | bestanden (HTTP 200) |

Die Kimi-Korrekturen sind in `e668ff0` und `8f4e074` enthalten. Alte rote
Fehlerkarten werden entfernt bzw. der verunreinigte Draft wird einmalig
zurueckgesetzt. Der Upload faellt nicht mehr auf den rote Karten erzeugenden
synthetischen `DataTransfer`-Pfad zurueck. Der finale Smoke lief mit einem
512x512-PNG ueber `/v1/chat/completions` und lieferte HTTP 200 sowie exakt
`IMAGE_INPUT_OK`; Kimi-Bildinput ist damit freigegeben.

Der identische WAV-Request gegen ChatGPT Web wurde transportiert und mit HTTP
200 beantwortet, das dort ausgewaehlte Web-Modell verweigerte jedoch selbst die
Transkription. Das ist eine providerseitige Modellgrenze; der positive
Gemini-Smoke belegt den gemeinsamen API- und Uploadpfad.

ChatGPT-Bildausgabe ist inzwischen als eigener Headless-Pfad über
`POST /v1/images/generations` implementiert. Ein extrahiertes Resultat wurde als
808.492-Byte-PNG mit korrekter Signatur und 1254x1254 Pixeln dekodiert und
visuell geprüft. Weitere Gegenproben deckten zwei False-Positive-Quellen auf und
schlossen sie: das 104x104-Toolicon wird durch die nach Toolaktivierung
aufgenommene Baseline ausgeschlossen; alte Estuary-Bilder bleiben anhand ihrer
stabilen `file_...`-ID ausgeschlossen, auch wenn ChatGPT `ts` und `sig` ändert.
Der aktuell folgende atomare Neu-Generierungs-Smoke scheitert ehrlich am
providerseitigen Free-Plan-Kontingent (Reset laut UI am 01.09.2026 um 16:57)
und wird nicht als Erfolg gewertet. Für Gemini wurde der zuvor fehlende
provider-spezifische Pfad ergänzt: „Uploads & Tools“ wird nach dem asynchronen
`new_chat()`-Remount per trusted CDP-Pointer geöffnet und „Bild erstellen“
aktiviert. Headless waren Moduswechsel („Bilder“), Submit und alternierende
Renderer-Wakeups belegt; Gemini blieb trotzdem bis zum absoluten 180- bzw.
300-Sekunden-Budget bei „Creating your image“, ohne neues Bild im DOM. Gemini
wird daher noch nicht mit Bildausgabe beworben. Alle anderen Brains melden
weiterhin nur Textausgabe. Eine Behauptung „100 % API-Kompatibilitaet fuer alle
Brains“ ist mit diesem Stand ausdrücklich nicht zulässig.

Die anschließende headless Werkzeugmenü-Vermessung aller übrigen Profile fand
keinen weiteren Bildgenerator: Claude und Qwen bieten Datei-Upload, Mistral
einen Medien-Upload einschließlich verbreiteter Audioformate; DeepSeek, Kimi,
Perplexity und Z.ai zeigten keinen Bild-/Audio-Generator. Damit sind
`/v1/images/generations`-Backends aktuell nur für ChatGPT und Gemini sachlich
begründet; Claude bestätigt offiziell, dass der Chat nur textbasierten Output
liefert und Bilder ausschließlich als Input verarbeitet.

Die bislang fehlenden eigenständigen OpenAI-Audiorouten sind anschließend
ergänzt worden. `/v1/audio/transcriptions` und `/v1/audio/translations` parsen
den offiziellen binären Multipart-Upload, routen ihn als Audio-Anhang an ein
Web-Brain und liefern `json`, `text` oder ein konservatives `verbose_json`.
Binärbytes, Dateiname, MIME-Typ, Modell und Format sind durch Regressionstests
abgedeckt. `/v1/audio/speech` existiert als ehrliche fail-closed Grenze: Keines
der aktuell vermessenen Web-Brains liefert einen belegten, extrahierbaren
TTS-Audiostream, daher werden dort keine synthetischen Audioantworten behauptet.
Ein echter headless Multipart-Smoke gegen `webagent/gemini` transkribierte die
neu synthetisierte Kontrollaufnahme „Die Prüfziffer lautet sieben acht neun“ in
23,8 Sekunden korrekt zu `{"text":"Die Prüfziffer lautet 789"}`. Ein erster
englischer Satz aus einer deutschen Windows-Stimme war phonetisch ungeeignet;
dieser Quellenfehler wird nicht als Provider- oder Endpoint-Erfolg gewertet.

## Aktueller Fix: große ZCode-Toolrequests im Browser-Provider

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`
**Commit:** `7bd1881` (`fix: compact large browser tool prompts`)

Ein ZCode-Request mit 53 Werkzeugen und rund 124 KiB JSON reproduzierte bei
ChatGPT die Web-UI-Antwort „Something went wrong“. Die folgenden Claude- und
DeepSeek-Versuche liefen dadurch im Client in `ECONNREFUSED`, weil der lokale
Endpoint während des vorherigen Neustarts nicht erreichbar war; ein frischer
Textturn gegen beide Brains ist danach erfolgreich durchgelaufen.

`browser_inference::prompt_with_tools` kompaktisiert den Toolblock nun
stufenweise auf höchstens 64 KiB. Werkzeugnamen und Parameterschemata bleiben
vollständig erhalten, Beschreibungstexte werden nur bei Bedarf gekürzt.

| Gate | Ergebnis |
|---|---|
| Kompakter Unit-Test | bestanden |
| ZCode-Request mit 53 Tools, normal | HTTP 200, kein Web-UI-Fehler |
| Derselbe Request über SSE | HTTP 200, gültiges `data: [DONE]` |
| Claude nach dem ChatGPT-Request | HTTP 200, `CLAUDE_CHECK_42` |
| DeepSeek nach dem ChatGPT-Request | HTTP 200, `deepseek_OK` |
| `cargo clippy --locked --all-targets --no-default-features -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.186 bestanden, 1 ignoriert |

Der laufende Release-Endpoint verwendet die neue Binary auf `127.0.0.1:8787`
und antwortet auf `/health` mit `status=ok`.

Die Browser-Einleitung wurde anschließend neutralisiert: Sie beschreibt den
Inhalt als Gesprächsverlauf eines API-Clients und vermeidet sichtbare
`Provider-Bridge`-/`WEBAGENT_INFERENCE/1`-Identitätsdiskussionen. Der
Tool-Umschlag bleibt für echte Client-Werkzeuge strikt und fail-closed, ist aber
kurz als Client-Werkzeugliste formuliert. Das verhindert unnötige Claude-
Metaantworten, ohne providerseitige Sicherheitsablehnungen zu verändern oder
zu umgehen.

Die Fähigkeitsgrenze bleibt bewusst dokumentiert: Bild-/Audio-Eingaben werden
hochgeladen, die Ausgabe wird derzeit ausschließlich als Text extrahiert.
Generierte Bildartefakte aus der ChatGPT-Weboberfläche sind noch kein
API-`image`-Output.

Ein konkreter Fehlerbefund aus dem ZCode-Test wurde getrennt: DeepSeek, Gemini
und Kimi hatten wegen eines mitgeführten Bildes `no_file_input` geliefert. Dieser
Fähigkeitsfehler wird in `relay` jetzt nicht mehr als allgemeiner Brain-Ausfall
gezählt, nicht wiederholt und nicht in den Text-Circuit-Breaker übernommen. Ein
späterer text-only Turn bleibt dadurch möglich; die API meldet die
Multimodalitätsgrenze weiterhin ehrlich mit 502.

## Browser-Inference-Provider-Scheibe vom 29.08.2026

Die aktuelle Arbeit liegt isoliert auf `feat/browser-inference-provider`, ausgehend von `master` bei `a0cd00b7`. `api_bridge` startet nicht mehr den vollständigen `AgentController`, sondern ruft die neue harnessfreie Grenze `browser_inference::complete()` auf. Dadurch sind WEBAGENT/1, Shell-/Dateiaktionen, Controller-Memory und der Plan-Act-Observe-Loop nicht mehr Teil eines Providerrequests. Der bestehende Rust-Harness bleibt unverändert als separater Consumer erhalten.

Die Providertrennung liegt in `83bc90d`; die abgeschlossene Pi-0.84.4-Integration samt reproduzierbarem Tool-Smoke liegt in `c8b98ed`. Beide Commits gehoeren zum Branch `feat/browser-inference-provider`; diese Uebergabe wird als nachfolgender reiner Dokumentationscommit abgeschlossen.

Der OpenAI-Adapter normalisiert Function-Tools, `tool_choice`, Assistant-`tool_calls` und `role=tool`-Ergebnisse. Browsermodelle geben Tool Calls über einen strikten `WEBAGENT_INFERENCE/1`-Umschlag zurück; unbekannte Tools, doppelte IDs und verletzte erzwungene Toolwahl scheitern fail-closed. Die Bridge führt Tools ausdrücklich nicht selbst aus. Anthropic Messages bleibt in dieser Scheibe textbasiert; SSE bleibt gepuffert statt tokeninkrementell.

| Gate | Ergebnis dieser Scheibe |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.170 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden |
| `cargo test --locked` | 1.209 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo build --locked --release` | bestanden; `webagent.exe` 7.005.184 Bytes (6,68 MiB) |
| Lokaler API-Smoke | `/health=ok`, geschütztes `/v1/models=webagent/chatgpt`, kontrollierter Shutdown bestanden |
| Live-Textturn ChatGPT | bestanden am 30.08.2026: `BRIDGE_OK` in 20,8 Sekunden |
| Pi 0.84.4 Textturn | bestanden: Provider entdeckt, `PI_BRIDGE_OK` ueber SSE, `stopReason=stop` |
| Live-Tool-Call | bestanden: Pi 0.84.4 fuehrte `read` auf einer Zufallsdatei aus, `role=tool` wurde zurueckgesendet und der exakte Nonce-Inhalt final ausgegeben |

`webagent relay` lieferte live `BRIDGE_OK` in 20,8 Sekunden; Pi 0.84.4 entdeckte den Beispielprovider und erhielt ueber gepuffertes SSE `PI_BRIDGE_OK` mit `stopReason=stop`. Der erste Pi-Toolversuch zeigte zusaetzlich, dass eine weiche `auto`-Instruktion das Browsermodell nicht sicher zum explizit verlangten Tool zwingt und externe Client-Tools mit eingebauten Browserwerkzeugen verwechselt. Die Browser-Inference-Anweisung trennt beides jetzt explizit und verlangt Tools fuer ausdrueckliche Toolauftraege sowie lokale/aktuelle/unbekannte Daten. Der erneute zufallsbasierte Pi-read-Smoke bestand den kompletten Loop: Tool Call, lokale Pi-Ausfuehrung, Tool-Ergebnis-Rueckgabe und finale Nonce-Antwort.

Die reproduzierbare Nutzerstrecke liegt in `examples/pi/models.json` und `scripts/test-pi-provider.ps1`. Sie nutzt die aktuelle Pi-Paketlinie `@earendil-works/pi-coding-agent`, isoliert `PI_CODING_AGENT_DIR`, prueft erst Text und dann einen echten Tool-Loop und veraendert weder die regulaere Pi-Konfiguration noch das Repository.

## Dynamischer Multi-Brain-Katalog vom 30.08.2026

Die Bridge ist nicht mehr auf das beim Serverstart angegebene Brain beschraenkt. `/v1/models` wird aus `config::brains()` erzeugt und enthaelt damit alle eingebauten sowie registrierten Custom-Brains. Jede OpenAI- oder Anthropic-Anfrage loest `webagent/<brain>` vor dem Browserstart auf und uebergibt dieses Brain an die harnessfreie Inference-Schicht; `webagent` allein bleibt Alias fuer `--brain`.

`examples/pi/extensions/webagent-models.ts` fragt den Katalog beim Pi-Start dynamisch ab und registriert den Provider. Der neue Pi-Befehl `/models` aktualisiert den Endpoint-Katalog und oeffnet die Brain-Auswahl; `/models <brain>` wechselt direkt. Die lokale Pi-0.84.4-Installation listete live `chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`, `qwen`, `zai` und das Custom-Brain `perplexity`. Die statische globale Ein-Modell-Konfiguration wurde auf diesem Rechner durch die dynamische Erweiterung ersetzt.

Die Release-Binary mit diesem Stand lieferte den dynamischen Katalog live aus. Pi lud die Repository-Erweiterung, listete alle neun Brains und bestand einen echten ChatGPT-Request mit der exakten Antwort `DYNAMIC_MODELS_OK`. Das direkte Kommando `/models claude` wurde ebenfalls ohne Fehler verarbeitet; ein produktiver Turn mit jedem weiteren Brain setzt dessen gueltiges Browser-Login voraus.

Die Implementierung ist als `1015586` (`feat: add dynamic pi brain switching`) auf `origin/feat/browser-inference-provider` gepusht.

## Latenz- und Isolation-Optimierung vom 30.08.2026

`api_bridge` serialisiert Browserruns jetzt nur noch pro Brain; unterschiedliche Brains koennen parallel arbeiten. In `relay` werden deterministische Sendefehler (fehlender Absende-Beweis, deaktivierter Knopf, erkannte Blockade, Login-/Cloudflare-/Limit-Hinweis) nicht mehr dreimal vollstaendig wiederholt; transiente CDP- und Navigationsfehler bleiben retry-faehig. Der identische Gemini-Fehler sank live von 95,351 s auf 33,408 s. Vollfeature-Clippy, fokussierte Relay-Tests und Release-Build bestanden.

## Aktueller Repositoryzustand

> **Historisch (Stand vor 2026-09-04).** Die nachfolgenden Branch-, Commit- und
> Gate-Angaben betreffen den damaligen Stand `task/v1-release-baseline` und sind
> mit dem aktuellen `master` (`505d416`) überholt. Aktueller Einstieg: der Kopf
> dieser Datei und `feature/docs-cli-ui-fixes`.

Die Arbeit läuft auf `task/v1-release-baseline`, ausgehend von `origin/master` bei `1d214e8`. Die erste abgeschlossene Baseline-Scheibe ist als `2659baf` (`fix: restore headless release baseline`) committet; der Scope-Freeze und die Definition of Done folgen in `4fdb068` (`docs: define v1 release completion`). Beide Commits sind auf `origin/task/v1-release-baseline` gepusht. Die Baseline-Scheibe stellte den browserfreien Releasezustand wieder her: fehlende Bibliotheksmodule wurden registriert, der Root-Eintrag in `Cargo.lock` wurde auf `0.11.1` korrigiert, reine Kernmodule wurden von unnötigen TUI-Gates entkoppelt, strikte Lint-/Testdrift wurde behoben und die alte Wilson-Dublette wird auf die gemeinsame Implementierung zurückgeführt.

Besonders wichtig ist die Korrektur von `capture_patch`: Eine globale Git-Einstellung wie `color.ui=always` lieferte ANSI-Sequenzen in `git diff` und machte die Scope- sowie Harvest-Prüfung blind. Der maschinell verarbeitete Patch wird nun explizit ohne Farbe erzeugt; reale Gegenproben auf einem temporären Git-Repository belegen In-Scope-Harvest und fail-closed Out-of-Scope-Ablehnung.

| Gate | Ergebnis auf dem aktuellen Arbeitsstand |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | **1.157 Bibliotheks-, 7 Binärtests bestanden; 1 Test bewusst ignoriert** |
| `git diff --check` | bestanden |
| Reale Scope-/Harvest-Gegenprobe | bestanden; ANSI-farbige Git-Diffs werden sicher verarbeitet |
| `cargo clippy --locked --all-targets -- -D warnings` auf Linux/WebKitGTK | bestanden |
| `cargo test --locked` auf Linux/WebKitGTK | **1.196 Bibliotheks-, 7 Binärtests bestanden; 1 Test bewusst ignoriert** |
| Windows-CI `33001615724` auf `e1e5a74` | Formatcheck, Vollfeature-Clippy, Vollfeature-Build und Vollfeature-Tests vollständig bestanden |

## v1.0-Ziel und Definition of Done

> **v1.0 bedeutet nicht, dass jede Idee umgesetzt ist.** Webagent ist fertig, wenn der dokumentierte lokale Browser-Agent verlässlich gebaut, getestet, betrieben und anhand aktueller Evidenz freigegeben werden kann. Ein grüner Unit-Test oder ein abgeschlossener Teil-Goal reicht dafür nicht aus.

| Abschlussbereich | Verbindliches Abnahmekriterium |
|---|---|
| **Reproduzierbarer Kern** | Formatcheck, striktes Headless-Clippy und vollständige browserfreie Test-Suite sind auf dem Release-Candidate grün; der Lockfile-Stand ist konsistent. |
| **Plattformgrenze** | Die Vollfeature-Gates (`cargo clippy --all-targets -- -D warnings`, `cargo test`) sind auf Windows mit WebView2 oder durch nachvollziehbare Windows-CI-Evidenz grün. Linux bleibt ein unterstützter browserfreier Kern-/TUI-Pfad, ersetzt aber keine Windows-WebView-Abnahme. |
| **Providerfähigkeit** | Jeder in der Releasekonfiguration aktivierte Provider besitzt eine frische, nachvollziehbare Send-/Antwort-Evidenz oder ist bewusst deaktiviert und mit Ursache dokumentiert. Keine historische Erfolgsmeldung gilt als Live-Beleg. |
| **Mehr-Brain-Betrieb** | Poolstart, Heartbeat, Recovery, Profil-Lease, Write-back, bereinigbarer Shutdown sowie Continuation und Cross-Brain-Handoff sind systemisch belegt. Ein abgebrochener Lauf darf weder das Master-Profil beschädigen noch ungemeldete Klonreste hinterlassen. |
| **Harvest-Sicherheit** | Ein gültiger Patch wird nach Wiederanwendung erneut gebaut und getestet; Scope-Verstöße, Löschungen und nicht nachvollziehbare Patches bleiben fail-closed ausgeschlossen. |
| **Betriebsdokumentation** | `docs/OVERVIEW.md`, diese Datei, `CHANGELOG.md` und die Release-Checkliste stimmen mit Code, Tests und bekannten Grenzen überein. |
| **Releaseentscheidung** | Der Release Candidate ist auf einem sauberen Tree und über GitHub erreichbar; erst nach dokumentierter Review- und Eigentümerfreigabe folgen Merge, Tag und GitHub-Release. |

## Scope-Freeze

Der v1.0-Kritische Pfad umfasst ausschließlich die nachfolgenden Arbeiten. Jede Erweiterung, die keine Definition-of-Done-Zeile erfüllt, bleibt außerhalb des Releasepfads.

| Im v1.0-Scope | Bewusst nach v1.0 verschoben |
|---|---|
| Browserfreie Releasequalität, Windows-Vollfeature-Evidenz und CI-Reproduzierbarkeit | Terminal-Renderer und ARIA-zentrierte Vermittlungsschicht |
| Frische Provider-Rezertifizierung und klarer Umgang mit Drift bzw. Ausfällen | CDP-/AX-Spike über die heutige Browseranbindung hinaus |
| Shutdown-, Profil-, Lease- und Klonbereinigungshärtung | Reale Free-Cloud-Transportadapter oder kostenpflichtige Fallbacks |
| End-to-End-Abnahme für Pool, Continuation, Cross-Brain-Handoff und Harvest | Genius Council, zusätzliche Agentenrollen und größere UX-Erweiterungen |
| Dokumentierte Release-Candidate- und Freigabevorbereitung | Neue Provider, neue Persistenz- oder Protokollmechanismen ohne Abschlussbezug |

## Providerbereitschaft und Desktop-Grenze

Die aktiven Standardprovider sind aktuell `chatgpt`, `deepseek`, `kimi`, `gemini`, `qwen`, `claude`, `mistral` und `zai`. Die ausgelieferten Selektoren für alle acht Anbieter werden eingebettet, sind valides JSON und bestehen die beiden providerfreien Verträge `test_embedded_selectors_cover_all_brains_and_parse` sowie `all_configured_brains_have_selectors`. `perplexity` besitzt eine ausgelieferte Selektordatei, ist jedoch nicht Teil des aktuellen statischen `BRAIN_TABLE` und damit kein v1.0-Standardprovider.

Eine manuelle Browser-Gegenprobe auf ChatGPT im verbundenen Browser lieferte den erwarteten Antworttext `BEREIT.`. Sie ist **keine Webagent-WebView-Evidenz**, weil sie nicht aus dem vom Nutzer getesteten Desktop-Arbeitsordner stammt. Der anschließende DeepSeek-Aufruf leitete in diesem separaten Browserprofil auf `/sign_in` um; auch das ist kein Befund über die projektseitigen Profile.

Der vom Nutzer getestete Windows-Desktop-Arbeitsordner ist in dieser Sitzung nicht erreichbar: Der Sandbox-Desktop enthält keinen Webagent-Ordner, die Browserverbindung blockiert lokale `file:///`-Pfade, der Remote enthält keinen neueren Desktop-Commit und es ist keine Desktop-/Computer-Verbindung konfiguriert. Das ist ein technischer Zugriffsblocker, keine fehlende Bereitschaft des Nutzers und keine Providerbewertung. Bis zu einem erreichbaren Desktop-Stand bleiben echte WebView-, Profil- und Providerbelege offen.

## Linux-WebView-Abnahme

Die Vollfeature-Abnahme auf Linux legte eine echte Konfigurationsregression offen: Die vorherige Headless-Bereinigung hatte `CloneGuard::path` entfernt, obwohl der WebView-Pfad sie bei der Runtime-Übergabe benötigt. Zusätzlich war `browser_args` unter WebKitGTK ungenutzt und zwei Linux-Stubs lagen hinter dem Testmodul. Commit `d32b8d0` grenzt die Windows-Argumente sauber ein, stellt den WebView-gateten Zugriff wieder her und ordnet die Plattformstubs lintkonform vor den Tests an. Nach Installation der notwendigen lokalen GTK-/WebKit-Entwicklungspakete sind die strikten Headless- und Vollfeature-Lints sowie beide vollständigen Test-Suiten grün.

Diese Evidenz bestätigt den Linux-WebKitGTK-Build und seine Tests. Ergänzend hat die Windows-CI `33001615724` auf dem aktuellen Branch die Vollfeature-Gates vollständig bestanden. Beide Belegketten ersetzen jedoch keine Live-Sitzungs- oder Providerabnahme auf dem tatsächlichen Desktop-Arbeitsstand.

## Betriebshärtung: aktueller Stand

Die beiden historischen Betriebsbefunde sind im gegenwärtigen Quellstand bereits strukturell adressiert. `main::run` ruft vor jedem regulären Ablauf `sweep_stale_runtime_profiles()` auf; die Bereinigung betrachtet ausschließlich die Wegwerf-Wurzeln `swarm` und `encapsulated`, verlangt eine lesbare Altersinformation und lässt kanonische Login-Profile bewusst unangetastet. Der gezielte Test `test_sweep_stale_runtime_profiles_spares_fresh_and_logins` belegt genau diese Grenze.

Für den kontrollierten TUI-Exit existiert `--run-secs`: Die Deadline führt in denselben Cleanup-Pfad wie die Taste `q`. Dieser schreibt `PoolControl { stop: true }`, joint den Worker-Thread und ruft anschließend `BrowserPool::shutdown_with_result()` auf, damit der geordnete Tab-Teardown und ein möglicher Write-back stattfinden. Der Worker-Pool belegt mit `kill_all_children_is_idempotent_and_empties_the_pool`, dass der Stopvorgang wiederholbar und vollständig ist. Eine echte Windows-WebView-/Write-back-Gegenprobe bleibt jedoch Teil der späteren Systemabnahme; der browserfreie Test beweist nicht die vorhandenen Desktop-Sitzungen.

## Kritischer Pfad und Blocker

Die nächste technische Scheibe ist die **Provider-Rezertifizierung**. Vor dem Live-Lauf werden die bestehenden Selektoren, Capability-Proofs und Fehlerbilder lokal inventarisiert. Die eigentliche Messung darf nur stattfinden, wenn der Eigentümer anwesend ist: Browserfenster können sich öffnen, aber Anmeldungen, Einmalcodes, CAPTCHAs und Zugangsdaten verbleiben vollständig beim Eigentümer. Der Integrator protokolliert ausschließlich die beobachteten Zustände und nutzt keine Geheimnisse.

Der aktuell bekannte spezifische Providerrest ist `zai`: Die Antwort kann einen `Thought Process` voranstellen; ein früherer Überlastungszustand verhinderte die belastbare Gegenprobe. Ein Provider mit frischer, nicht klassifizierbarer Antwort wird nicht still als releasebereit bewertet. Für alle Provider gilt dieselbe Regel: frische Evidenz oder dokumentierte, bewusst deaktivierte Grenze.

| Abhängigkeit | Status | Umgang |
|---|---|---|
| Eigentümer für Live-Browser/Anmeldung | erforderlich | Vor der tatsächlichen Live-Rezertifizierung gezielt um Freigabe bitten; bis dahin nur lokale Vorbereitung. |
| Windows-WebView2-Gates | CI bestanden, Live-Abnahme offen | Windows-CI `33002148599` ist vollständig grün; reale WebView2-Sitzungen und Profile bleiben separat zu prüfen. |
| Merge | abgeschlossen | Die geprüften Baseline- und Plattformkorrekturen wurden per Fast-Forward nach `master` integriert. |
| Tag/GitHub-Release | offen | Erst nach vollständiger Live-Evidenz und Eigentümerfreigabe vorbereiten; keine Veröffentlichung vorab. |

## Nächste sichere Aktion

Die nächste sichere Aktion ist die **Live-Abnahme am erreichbaren Desktop-Arbeitsstand**: je aktivem Provider denselben harmlosen Sende-/Antwortpfad prüfen, Zustände klassifizieren und die Windows-WebView2-/Write-back-Gegenprobe dokumentieren. Der aktuelle Windows-Desktop-Arbeitsordner ist aus dieser Sitzung nicht erreichbar; bis eine erreichbare Verbindung besteht, werden weder unbefugte Logins noch Ersatzsitzungen als Projektevidenz verwendet.

## Übergabe

- **Branch:** `master`; G-001-Abschluss-Stand `2589b0a`, Release `v0.11.3` (Tag `7021959`) veröffentlicht.
- **Abgeschlossene Scheiben:** `2659baf` (Headless-Baseline), `4fdb068` (v1.0-Definition), `d32b8d0` (Linux-WebView-Feature-Gates), `f51acd6` (Linux-/Windows-Abnahmestände), Phase-9 `T-901..T-906` inkl. Web-UI-Review-Fixes (`44f7a1e`), Release `v0.11.3`, Toolchain-Norm `2589b0a`.
- **Eigentümerschaft:** aktueller Integrator hat den v1.0-Abschluss bearbeitet; Live-Anmeldungen bleiben beim Eigentümer.
- **Externe Freigaben:** für Release `v0.11.3` erteilt (Eigentümerwahl „Erst README/Bild, dann Release"); Tag und GitHub-Release sind erfolgt.
- **Arbeitsbaum:** nach diesem Übergabecommit erneut prüfen; `master` direkt pushen, keine History umschreiben.

> **Hinweis zur Wahrheitspflege:** Historische Live-Befunde aus August 2026 sind nützlich für die Diagnose, aber kein aktueller Verfügbarkeitsbeleg. Maßgeblich für v1.0 sind frische, reproduzierbare Belege am Release Candidate — für `v0.11.3` in `docs/proofs/G-001/g001-abschluss-2026-09-12.json` zusammengeführt.
