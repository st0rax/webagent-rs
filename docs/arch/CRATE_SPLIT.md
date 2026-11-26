# Crate split decision: core / transport / api

> **Referenz.** Design decision for T-1004, not an extraction order. The live
> tree is still one package; `Cargo.toml` and every `[profile.*]` section stay
> unchanged. Any crate move needs a new, explicit owner approval.

**Status:** decided on paper, blocked in reality. This document recommends a cut
and lists the traits that must exist first. It does not create crates, tasks or
commits beyond its own proof. The owner phrase `mach fertig den senf` approved
finishing this design document only; it did not approve crate extraction.

## 1. Measured baseline

All numbers below were re-measured on this branch's claim commit `f27928b`
(`docs/T-1004-crate-split`, 2026-11-26), not copied from an older proof.

| Measure | Command | Measured value |
|---|---|---|
| Default lib tests | `cargo test --lib --quiet` | 1523 passed / 0 failed / 1 ignored (53.09 s) |
| Architecture guard | `cargo test --test arch_layers -- --nocapture` | 4 passed / 0 failed; `modules=88, edges=274, cycles=3, upward_edges=32` |
| Clean lib test build+link | `cargo clean -p webagent`; timed `cargo test --lib --no-run --message-format=json` | 65.98 s; test executable 162,341,370 bytes (382 files / 2.0 GiB removed by the clean) |
| Clean default binary build+link | `cargo clean -p webagent`; timed `cargo build --message-format=json` | 40.47 s; `target/debug/webagent.exe` 150,514,926 bytes (43 files / 422.7 MiB removed by the clean) |
| Direct Cargo dependencies | `cargo tree --depth 1 -e normal` | 21 direct dependencies (list in §5) |
| Default binary smoke | `target\debug\webagent.exe --help` | exit `-1073741515` (`0xC0000135`, misread then as `0xC0000005`), zero-byte stdout, before `main` |

The default `webview` binary startup failure is pre-existing and reproduced at
the claim commit. The hex value was misread as `0xC0000005` in the phase-16
notes; `-1073741515` is `0xC0000135` (`STATUS_DLL_NOT_FOUND`) because
`WebView2Loader.dll` was missing beside the EXE. T-1008 now copies it in the
root build script and verifies debug/release `--version` and `--help`; see
`docs/proofs/T-1008/README.md`. The browser-free binary remains usable: T-1003
measured `--help` exit 0 and the intended `page-unavailable` message for
`diagnose`.

The graph was derived with the exact production-code sanitizer from
`tests/arch_layers.rs` (comments, doc comments, string/char literals and
`#[cfg(test)]` items removed, then every `crate::<module>` path counted). The
scratch copy lived under `%TEMP%\opencode\`; no repository source file was
touched by the measurement.

## 2. Decision

Create **three library crates plus the existing root binary crate**, in this
order of dependency:

```text
webagent (root: CLI, controller, TUI, web UI, benchmarks, apps)
├── webagent-api (HTTP bridge, inference, relay, session, sources)
│   └── webagent-core
├── webagent-transport (WebView browser, page driver, pool, selectors)
│   └── webagent-core
└── webagent-core (protocol, brain trait, shell, runs, transcripts, events)
```

Allowed edges are downward only: root → any library, api → core, transport →
core, core → nothing outside itself. Four measured edges violate that direction
today and must be broken by traits before any module is moved (§4).

This is a **three-library cut**, not a fourth business crate. The remaining 36
top-level modules stay in the root crate; the ratchet already keeps their
legacy TUI and benchmark cycles visible, and extracting them would be a
separate, larger decision.

## 3. Why the cut is shaped this way

### 3.1 Naive three-group grouping (candidate names only)

Grouping only the modules named in the task (`core` = protocol, executor,
shell_policy, brain, run_store, transcript, timeouts, bench_events;
`transport` = browser, webview_runtime, page_driver, browser_pool plus mock /
reveal / upload helpers; `api` = api_bridge, browser_inference, session,
source_scope; everything else `rest`) gives these measured cross-group edges:

| From \ to | core | transport | api | rest |
|---|---:|---:|---:|---:|
| core | 6 internal | 0 | 0 | 4 |
| transport | 7 | 12 internal | 1 | 12 |
| api | 2 | 0 | 4 internal | 8 |
| rest | 59 | 17 | 9 | 133 internal |

The naive core is a good leaf (59 inbound, only 4 outbound), but its four
outbound edges are real blockers: `protocol → circuit_breaker`,
`shell_policy → config`, `timeouts → brain_score`, `transcript → config`.
`config`, `circuit_breaker` and `brain_score` therefore belong in the core
group, not behind it. The api group also still reaches `relay`, `observer`,
`config` and `circuit_breaker`, so it cannot claim to depend on the brain
abstraction alone yet.

### 3.2 Recommended grouping

Moving the cross-cutting `cross` layer and the persistence/proof/protocol
helpers into core, and moving `relay` plus the HTTP/TLS clients into api,
gives the following measured graph:

| From \ to | core | transport | api | rest |
|---|---:|---:|---:|---:|
| core | 50 internal | **2** | 0 | 0 |
| transport | 19 | 16 internal | **1** | **3** |
| api | 17 | **1** | 10 internal | 0 |
| rest | 77 | 12 | 7 | 59 internal |

Every `rest → library` edge is an allowed application-to-library dependency.
The four bold cells were the only edges that pointed across a crate boundary
in the wrong direction or into another library; all are now prepared for
extraction by T-1006, T-1007 and T-1009:

| Blocker | Measured edge | Fix |
|---|---|---|
| core → transport | `contract → browser`, `counting → browser` | T-1006: `contract` imports protocol helpers directly; `counting` uses `BackendFactory`. `PageProbe` is prepared for later panel work, not for these two edges. |
| transport → api | `browser → browser_inference` | T-1006: shared inference/tool value types moved to `core::inference_types` with old re-exports. |
| transport → rest | `browser_pool → brain_grid`, `webview_reveal → brain_grid`, `webview_runtime → brain_grid` | T-1007: low-rank `panel_sink` owns `Rect`, pure layout and the desktop-fact `PanelSink`; root `brain_grid` implements and installs it. No file was moved. |
| api → transport | `relay → browser` | T-1009: core `ImageGenerationBackend` trait plus `BackendFactory::create_image_backend`; the text path already used `BackendFactory` since T-1006. |

T-1006 correction to the original wording: `PageProbe` alone does not fix
`contract → browser` or `counting → browser`. `contract.rs:436/442` called
protocol helpers that `browser` merely re-exported, and `counting.rs:90`
constructed a concrete `WebBrainBackend`. The shared page probe is still useful
as a small core-facing capability, but the two measured edges are removed by a
protocol import and the factory respectively.

After those four fixes the library graph is acyclic by construction: core has
no outgoing crate edge, api and transport depend only on core, and the root
crate depends on all three. The remaining three module cycles from the ratchet
(`api_bridge,brain_probe,browser,browser_inference,browser_pool,relay`;
`autoresearch,bench_harvest,bench_scoring,benchmark,controller,runs_report`;
`bin_hooks,repl,tui,tui_ansi`) are handled in §6.

## 4. Per-crate dependency trees

Module names are the top-level modules counted by the guard. The two selector
files are submodules and follow their parent module: `browser/selectors.rs`
goes with transport, `config/selectors.rs` with core.

### 4.1 `webagent-core` (34 modules moved in T-1011; `counting` stays in root)

```text
webagent-core
├── cross utilities (19)
│   acceptance, circuit_breaker, code_score, config (+ selectors), counting,
│   executor, file_actions, loop_guard, scoring, shell_policy, taskboard,
│   timeouts, tools, bench_events, brain_limits, pool_failover, pool_state,
│   worker_pool, contract
├── brain + protocol + persistence (15)
│   brain (BrainBackend), capability_types, capability, capability_proof,
│   wiki_memory, goal_plan, run_store, transcript, protocol, brain_score,
│   observer, prompts, network_observation, doctor, watchdog
└── trait providers to add before extraction
    RelayBackend, BackendFactory, ClientPresence, PageProbe, InferenceTypes
```

**T-1011 status (2026-11-26).** The crate now owns the modules above except
`counting`, plus `inference_types` from T-1006 and a new `support` module that
holds the shared helpers previously in the root `lib.rs` (`StageTimer`,
`StageNote`, `civil_utc`, `now_rfc3339`, `now_run_stamp`, `timestamp`,
`char_prefix`, `char_suffix`, `ProcessSnapshot`, `pid_alive`). The root crate
re-exports everything with `pub use webagent_core::*;`, so no public path
changed. `counting` stays in the root crate because it calls
`WebBackendFactory::create_brain_backend`; the `BackendFactory` impl lives in
`browser/backend.rs`. For the same orphan-rule reason the concrete
`WebBackendFactory` marker moved from core `brain.rs` to root
`browser/backend.rs` and is re-exported at the root. The selector JSON files
(compile-time assets of `config/selectors.rs`) moved to
`crates/webagent-core/selectors/`. Core has `webview`, `tui` and
`test-support` features; the root forwards its `webview`/`tui` features and
enables `test-support` as a dev-dependency for the shared bus test lock.
Measured guard state: `modules=91, edges=278, cycles=3, upward_edges=26,
crate_violations=0`.

Measured: 50 internal edges; 19 inbound from transport, 17 from api and 77
from the root crate; only the two `contract`/`counting → browser` edges leave
the crate. Cargo dependencies: `serde`, `serde_json`, `sha2`, `regex`,
`fancy-regex`, `lazy_static`, `fs2`, `time`, and `windows-sys` on Windows for
`config`. No `tokio`, no TLS, no `clap`, no GUI crate. `capability_proof`
remains **the one** proof gate and store; this crate must not grow a second
one.

### 4.2 `webagent-transport` (9 modules)

```text
webagent-transport
├── browser (+ selectors), mock_page, page_driver
├── webview_runtime, webview_reveal, upload_spa_js
├── browser_pool, login, brain_probe
└── implements: BrainBackend (WebBrainBackend), RelayBackend, BackendFactory,
                PageProbe; emits PanelSink layout events
```

Measured: 16 internal edges; 19 outbound and 3 inbound to/from core, 1 outbound
to api (`browser → browser_inference`) and 3 outbound to the root app (the
`brain_grid` hooks). Cargo dependencies: `serde_json`, `time`, and, behind the
crate's own `webview` feature, `wry`, `tao`, `webview2-com`, `windows`,
`arboard`, `clipboard-win`, `image`. The root `webview` feature forwards to
`webagent-transport/webview`; `--no-default-features` must still produce a
tree without those GUI crates, as T-1003 proved for the single package.

### 4.3 `webagent-api` (9 modules)

```text
webagent-api
├── api_bridge (+ boundary, catalog, content, inference, media, provider_handlers,
│              response_protocol, routing, store, transport, wire, tests)
├── browser_inference, relay
├── session, source_scope, free_cloud_chat
└── https_client, comms, oobe
```

Baseline measurement: 10 internal edges; 17 outbound to core, 1 to transport
(`relay → browser`) and 7 inbound from the root app. T-1009 removed that one
outbound edge, so the api crate now names no `WebBrainBackend`, no `wry`/`tao`
type and no concrete page: brain access is `dyn BrainBackend`, `dyn
RelayBackend`, `dyn ImageGenerationBackend` and `BackendFactory`.
Cargo dependencies: `serde`, `serde_json`, `time`, `tokio`, `tokio-rustls`,
`rustls`, `webpki-roots` (all via `https_client`). No GUI crate, no `clap`.

### 4.4 root `webagent` (36 modules + binary/build script)

```text
webagent (root package; keeps Cargo.lock, build.rs, [profile.*])
├── apps: controller, repl, startup, web_ui, web_ui_api, benchmark*,
│   autoresearch, self_research, runs_report, brains_health, ...
├── local UI: tui*, bin_hooks, brain_grid, brain_wall, welcome, canary, ...
├── domain apps: group_run, knockout, design_vote, round_tally, target_check,
│   fakebrain, bot2bot_worker, ...
└── depends on: webagent-api, webagent-transport, webagent-core
```

Measured: 59 internal edges; 77 → core, 12 → transport and 7 → api, all
downward. Cargo dependencies: `clap`, `tokio`, `ratatui`/`crossterm` behind
`tui`, plus the three library crates. The two remaining legacy cycles stay
here; extracting them is not part of this cut.

### 4.5 Current external tree (for comparison)

`cargo tree --depth 1 -e normal` at the claim commit lists exactly:
`arboard`, `clap`, `clipboard-win`, `fancy-regex`, `fs2`, `image`,
`lazy_static`, `regex`, `rustls`, `serde`, `serde_json`, `sha2`, `tao`, `time`,
`tokio`, `tokio-rustls`, `webpki-roots`, `webview2-com`, `windows`,
`windows-sys`, `wry`. After the split these are distributed as in §4.1–4.4;
no dependency is added or removed by the design.

## 5. Trait boundaries required before extraction

1. **`RelayBackend` → public core trait.** `relay.rs` already has a private
   `RelayBackend` with `relay_new_chat`, `relay_send_with_attachments`,
   `relay_wait_response_streaming`, `relay_stop` and
   `relay_web_search_state` (the last returns
   `brain_score::WebSearchObservation`). Make it public in core and keep
   `WebBrainBackend`'s impl in transport. This removes the private-trait
   coupling to `browser_inference::BrowserAttachment` by moving that value
   type to core as well.
2. **`BackendFactory` in core.** `relay.rs` used to call
   `WebBrainBackend::from_config` directly for both text and image turns.
   T-1006 routed the text path through `create_relay_backend`; T-1009 added
   `create_image_backend` and the `ImageGenerationBackend` trait for the image
   path. This removed the measured `api → transport` blocker; api only sees
   core traits.
3. **`ClientPresence` in core.** `api_bridge/transport.rs` defines
   `ClientPresence`/`AlwaysConnected`, but `relay.rs:5` and
   `browser_inference.rs` consume them. Move the trait and the harmless
   `AlwaysConnected` implementation to core so no library depends on another
   library's HTTP presence concept.
4. **Shared inference/tool types in core.** `BrowserAttachment`,
   `BrowserAttachmentKind`, `BrowserTool`, `BrowserToolChoice`,
   `BrowserInferenceResponse`, `MODEL_OUTPUT_ERROR_MARKER` and the tool
   envelope parser are used by both `browser` and `api_bridge`. Put them in a
   core `inference_types` module (or extend `protocol`) so
   `browser → browser_inference` disappears and api_bridge's many
   `crate::browser_inference::…` references become internal to api only.
5. **`PageProbe` in core.** Published in T-1006 as a small read-only page
   capability implemented by transport; do not move the browser crate into
   core. It is not the fix for `contract → browser` or `counting → browser`
   (see the correction in §3): the former was a re-exported protocol helper,
   the latter needs `BackendFactory`.
6. **`PanelSink`/`WindowLayout` in transport.** Published in T-1007:
   `browser_pool`, `webview_reveal` and `webview_runtime` no longer reach
   `brain_grid`. The low-rank `panel_sink` module owns `Rect`, pure layout and
   the read-only desktop-fact trait; root `brain_grid` supplies
   `DesktopPanelSink` and the binary installs it. Parking and focus logic stay
   in the root/runtime modules, so this removed all three `transport → rest`
   edges without leaking window control into a library.
7. **`BrainBackend::page(&dyn Any)` stays an escape hatch.** The trait method
   exists for diagnose tools. No downcast to a WebView type may appear in api
   or core; transport keeps the concrete page access.

The one proof gate, `capability_proof::record_measurement` /
`record_proof` read via `level_from_selectors_with`, stays in core. No crate
gets its own proof store, its own selector hash or its own TTL logic.

## 6. `[profile.*]` stays at the workspace root

`Cargo.toml` currently has `[profile.release]`, `[profile.dev]` and an empty
`[profile.test]` section. The settings are load-bearing:

- `codegen-units = 16` in `release` and `dev` (test inherits dev) stays as-is,
  but the old claim that it prevents a `0xC0000005` loader threshold is wrong.
  The measured `0xC0000135` at task-claim time was `STATUS_DLL_NOT_FOUND` from
  the missing `WebView2Loader.dll`, fixed in T-1008; the 2026-09-03 note used
  the same wrong hex diagnosis. Keep these settings until a separate, measured
  reason to change them exists.
- `[profile.dev] debug = 1`, `incremental = true` keeps test links affordable
  while retaining panic line numbers; `debug-assertions` stays on.

When a `[workspace]` root is introduced, these sections belong **only** in the
root manifest. Member crates must not declare `[profile.*]`: Cargo ignores
profile settings in non-root manifests and warns, which would silently remove
the loader fix. The workspace gets one `Cargo.lock`; member crates do not get
their own locks or targets. T-1004 changed no profile line and no dependency
line.

## 7. Ordered follow-up tasks (proposed, not created)

These are proposals only. No ID was assigned and nothing was written to
`docs/TASKBOARD.json`. Each item expects the usual proof, four gates and a
before/after test count; none may merge without separate approval.

1. **Publish the core traits.** Done as T-1006 in the current crate: public
   `RelayBackend`, `BackendFactory`, `ClientPresence`, `PageProbe` and shared
   inference types exist in `src/`, old paths still re-export, call sites
   switched, ratchet green. No crate move.
2. **Break `browser → browser_inference`.** Move the shared value types to
   their new home, delete the exception only after the guard proves the edge
   is gone.
3. **Break the three `brain_grid` hooks.** Done as T-1007 in the current crate:
   `panel_sink` owns `Rect`, `WindowLayout` helpers and `PanelSink`; `brain_grid`
   implements `DesktopPanelSink`, the binary installs it, and the three
   transport modules no longer reference `brain_grid`. Parking, docking and
   focus are unchanged; the guard dropped from 29 to 26 upward edges.
4. **Break `contract`/`counting → browser`.** Done as T-1006: `contract` uses
   `crate::protocol` helpers and `counting` uses `BackendFactory`; both
   grandfathered `edge:` lines were removed only after the guard confirmed the
   edges were gone. `PageProbe` was published separately for later panel work.
5. **Owner gate: approve extraction.** This is the explicit permission needed
   before any file is moved. Without it, stop here.
6. **Create the workspace and move core.** Add member manifests, move the 34
   core modules, preserve all public paths through re-exports where possible,
   re-measure build/link and the guard.
7. **Move transport.** Put the GUI dependencies behind the member crate's
   `webview` feature, forward the root feature, re-run the no-default tree.
8. **Move api.** Keep relay/inference/session on core traits only; re-run
   `cargo test --all-targets` and re-measure clean build/link against §1.
9. **Close the ratchet.** Update `layers.toml`/`layer_exceptions.txt` only for
   edges actually removed, refresh proofs and handover. `done` is set only
   after a branch is merged into `master`.

## 8. Explicit no-go

- No crate, workspace manifest, `Cargo.toml`, `Cargo.lock`, `[profile.*]` or
  source file was changed by T-1004.
- No crate extraction without the owner's new, explicit approval.
- No second proof gate, no second capability store, no selector-hash change.
- No removal of a CLI command, feature, legacy TUI path or grandfathered
  exception that still exists.
- No credentials, profile/token read, live provider call, browser login or
  network benchmark.
- No release, tag, deployment, PR, force-push or merge into `master`.
- No task was marked `done`; all phase-16 branches remain `claimed` until
  merged.

## 9. Decision gate

The recommendation is **yes, split — after the four trait boundaries exist and
after a fresh owner approval**. Until then the single package is the product.
This document records the measured cut so that the approval, when it comes, is
about a concrete shape rather than a vague refactor.
