# Top-level module layers

> **Reference.** This is the layer model enforced by `tests/arch_layers.rs`.
> The machine-readable assignment lives in [`layers.toml`](layers.toml), the
> grandfathered debt in [`layer_exceptions.txt`](layer_exceptions.txt). This
> document describes the model; it is not a second source of truth for either
> list.

## Direction of an edge

An edge `A -> B` exists when a production source file belonging to top-level
module `A` contains a `crate::B::…` path. `tests/arch_layers.rs` removes
comments (including doc comments), string literals, byte/char literals and
`#[cfg(test)]` items before scanning, so documentation links such as
[`crate::tui`] and test-only imports do not create edges.

Higher rank means a higher-level application. The guard accepts an edge only
when `rank(source) >= rank(target)`: applications may depend on lower layers,
not the other way round. An edge from a lower to a higher rank must be listed
as `edge:<lower>-><higher>` in the exception file. A cycle (strongly connected
component with more than one module, or a self-loop) must be listed as
`cycle:<sorted,names>`.

Every listed exception must still exist. A stale exception is a test failure,
so the list can only shrink when somebody actually removes an edge or breaks a
cycle. Edges inside an excepted cycle are covered by that cycle exception;
they do not need a second edge line.

## Measured state on `master` `93d33a3`

| Measure | Value |
|---|---:|
| Top-level modules in `src/lib.rs` | 87 |
| Production `crate::<module>` edges | 274 |
| Strongly connected components (cycles) | 6 |
| Upward edges outside cycles | 30 |

The prompt for phase 16 quoted 89 modules, about 94k production lines and five
cycles. Re-measurement found 87 modules, 92,649 raw `.rs` lines in those module
files (64,130 after removing `#[cfg(test)]` items) and six cycles. The three
differences are documented in [`../proofs/T-997/README.md`](../proofs/T-997/README.md).

## Ranks and layers

| Rank | Layer | Responsibility | Examples |
|---:|---|---|---|
| 0 | `cross` | Cross-cutting utilities and resilience primitives; depended on by nearly everything | `config`, `timeouts`, `executor`, `shell_policy`, worker-pool primitives (`worker_pool`, `pool_state`, `pool_failover`), `brain_limits`, `circuit_breaker` |
| 1 | `L0` | Remote chat abstraction and provider-facing clients | `brain`, `brain_probe`, `browser_inference`, `login`, `oobe`, `https_client`, `comms` |
| 2 | `L1` | Browser runtime, WebView bridge and page interaction | `browser`, `webview_runtime`, `webview_reveal`, `page_driver`, `mock_page`, `upload_spa_js` |
| 3 | `L2` | Routing, provider selection and pool workers | `browser_pool`, `free_cloud_chat`, `source_scope`, `bot2bot_worker` |
| 4 | `L3` | Transport reliability, health and watchdogs | `relay`, `network_observation`, `doctor`, `watchdog` |
| 5 | `L4` | Session, persistence and proof state | `session`, `run_store`, `transcript`, `memory`, `capability`, `capability_proof`, `wiki_memory`, `goal_plan` |
| 6 | `L5` | Protocol, presentation and scoring helpers | `protocol`, `observer`, `prompts`, `target_check`, `design_vote`, `round_tally`, `brain_score` |
| 7 | `L6` | Composition helpers above the protocol layer | `group_run`, `knockout`, `welcome`, `canary`, `fakebrain` |
| 8 | `L7` | Applications: local UI, provider API, controller, CLI, benchmarks | `controller`, `repl`, `tui*`, `web_ui`, `web_ui_api`, `api_bridge`, `startup`, `benchmark`, `autoresearch`, `self_research`, `brain_grid`, `brain_wall`, `brains_health` |

The full, authoritative module list per rank is in `layers.toml`. Ranks are
deliberately coarse: eight conceptual layers plus `cross` are enough to catch
new upward dependencies without pretending that the current tree is already
clean.

## Ambiguous assignments and reasons

- `api_bridge` is product-facing (`L7`) but sits in the same cycle as `brain`,
  `browser`, `protocol` and `relay`. The cycle exception covers its internal
  edges until T-998 breaks that cycle.
- `protocol` is presentation/protocol (`L5`) but belongs to the same cycle as
  the remote-chat and browser modules. T-998 targets two of its edges.
- `browser_pool` is a pool/routing module (`L2`) and is part of the measured
  seven-plus-one-module cycle; the prompt's five-cycle list omitted it.
- `config`, `brain_limits`, `circuit_breaker`, `pool_failover` and `worker_pool`
  are cross-cutting (`cross`) but mutually cyclic. T-999 and T-1000 target
  those edges.
- `capability` and `capability_proof` live in `L4` because proof state is
  persistent state. They form one cycle; `capability_proof` remains the single
  proof gate. T-1001 targets this cycle.
- `bin_hooks`, `repl`, `tui`, `tui_ansi`, `tui_load` and `tui_state` are legacy
  UI (`L7`) with two cycles. `bin_hooks` was missing from the prompt's TUI
  cycle. T-1002 may remove or keep these with a written reason.
- `autoresearch`, `bench_harvest`, `bench_scoring`, `benchmark`, `controller`
  and `runs_report` are applications (`L7`). Their cycle is a sixth cycle that
  the prompt did not list.
- `contract`, `counting`, `file_actions`, `taskboard`, `timeouts`, `tools` and
  the WebView modules keep their conceptual layer despite measured upward
  edges. Those 30 edges are grandfathered individually; reclassifying the
  modules would hide the debt instead of making it removable.

## Running the guard

```bash
cargo test --test arch_layers
cargo test --all-targets          # includes this integration test
cargo test --no-default-features --test arch_layers
```

The guard needs no browser, network, credentials or feature flag. It is an
integration test: `cargo test --lib` alone does not execute it, but CI's
`cargo test --locked --no-default-features --verbose` and `cargo test
--all-targets` do. Three additional tests in the same file prove that a new
cycle, a new upward edge and a stale exception are each rejected.
