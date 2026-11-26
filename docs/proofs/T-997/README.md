# T-997 proof — architecture layer ratchet

> **Referenz.** Historical proof for T-997. The live ratchet is
> `tests/arch_layers.rs` with `docs/arch/layers.toml`.

**Date:** 2026-11-26
**Owner:** `local/opencode`
**Branch:** `feature/T-997-arch-layers` (base `master` `93d33a3`)
**Scope:** `tests/arch_layers.rs`, `docs/arch/layers.toml`,
`docs/arch/layer_exceptions.txt`, `docs/arch/LAYERS.md`, this proof file.

## What was added

`tests/arch_layers.rs` builds the top-level module graph from source and
enforces two rules:

1. Every strongly connected component with more than one module (or a
   self-loop) must be listed in `docs/arch/layer_exceptions.txt`.
2. An edge from a lower-ranked layer to a higher-ranked layer must be listed
   there too. Edges inside an excepted cycle are covered by the cycle line.

Comments, doc comments, string literals, byte/char literals and `#[cfg(test)]`
items are removed before scanning, so documentation links and test-only imports
do not create fake edges. Every exception must still exist; a stale exception
fails the test. That makes the list a ratchet that can only shrink.

Three extra tests in the same file use synthetic graphs to prove that a new
cycle, a new upward edge and a stale exception are rejected.

## Measured state (not quoted from the prompt)

The guard reports, from the real tree:

```text
architecture guard: modules=87, edges=274, cycles=6, upward_edges=30
```

| Prompt statement | Measured on `93d33a3` | Difference |
|---|---|---|
| `master 93d33a3` | confirmed | none |
| 89 top-level modules | 87 `mod` declarations in `src/lib.rs` | 2 fewer |
| about 94k production lines | 92,649 raw `.rs` lines in those module files; 64,130 after removing `#[cfg(test)]` items | raw figure close; production-only figure much lower |
| five cycles | six strongly connected components | one additional cycle |
| `T-975`: 1521 passed / 0 failed / 1 ignored | 1521 passed / 0 failed / 1 ignored on the same base | none |

The six measured cycles are:

| Cycle | Note |
|---|---|
| `api_bridge, brain, brain_probe, browser, browser_inference, browser_pool, protocol, relay` | prompt listed the same set **without `browser_pool`** |
| `autoresearch, bench_harvest, bench_scoring, benchmark, controller, runs_report` | **not in the prompt's five** |
| `bin_hooks, repl, tui, tui_ansi` | prompt listed the same set **without `bin_hooks`** |
| `brain_limits, circuit_breaker, config, pool_failover, worker_pool` | matches prompt |
| `capability, capability_proof` | matches prompt |
| `tui_load, tui_state` | matches prompt |

The exception file therefore seeds **6 cycle lines and 30 legacy upward-edge
lines**. The upward edges are grandfathered, not denied: the conceptual layer
model keeps modules such as `taskboard`, `timeouts` and `file_actions` in
`cross`/utility layers even where the current tree violates the direction.
Reclassifying them would hide the debt; the explicit lines make each edge
removable by a later task.

## Gates (real runs)

| Gate | Before (base) | After | Exit |
|---|---|---|---:|
| `cargo fmt --all -- --check` | 0 | 0 | 0 |
| `cargo clippy --features webview --all-targets -- -D warnings` | 0 | 0 | 0 |
| `cargo test --lib --quiet` | 1521 passed / 0 failed / 1 ignored (57.96 s) | 1521 passed / 0 failed / 1 ignored (50.66 s) | 0 |
| `cargo test --test arch_layers -- --nocapture` | no test file | 4 passed / 0 failed / 0 ignored | 0 |
| `cargo test --all-targets --quiet` | not run as one command before the change | lib 1521, binary-unit 7, integration 4 — all passed | 0 |
| `cargo test --locked --no-default-features --test arch_layers --quiet` | no test file | 4 passed / 0 failed / 0 ignored | 0 |
| `git diff --check` | 0 | 0 | 0 |

`cargo test --lib` is unchanged because the guard is an integration test; the
task's DoD explicitly allows either a lib test or an integration test. CI's
`cargo test --locked --no-default-features --verbose` runs the new file, and
`cargo test --all-targets` was verified locally.

## Deviations and reasons

- **Branch name:** the working branch was renamed from `feat/T-997-arch-layers`
  to `feature/T-997-arch-layers` before the first commit.
  `scripts/commit-as-agent.ps1` and `taskboard::tests::taskboard_invarianten_halten`
  only allow `feature|fix|docs|chore|refactor|test` prefixes for new claims.
- **Six cycles instead of five:** measured, not assumed; the extra cycle and
  the two larger cycle sets are seeded as exceptions with the differences
  spelled out above.
- **30 edge exceptions:** the prompt's five cycles alone were not sufficient
  to make the conceptual layer model pass. They are explicit grandfathered
  debt; a new upward edge still fails the guard.
- **No binary-affecting change:** no source module, CLI, protocol, API or
  `Cargo.toml` change. `webagent --help` / `webagent diagnose` are therefore
  not part of this proof. No `[profile.*]` entry was touched.
- **Board bookkeeping is a separate docs commit** (`fdff65c`): phase-16 tasks
  `T-997..T-1005` were added, `T-997` claimed, and `docs/CURRENT_WORK.md`
  updated. No next task is claimed.

## Open questions

- Whether the sixth cycle (`benchmark`/`controller` cluster) should become a
  permanent documented exception or a later refactor task. It is recorded, not
  silently accepted as harmless.
- The 30 upward edges are outside the phase-16 cycle tasks. Removing them is
  not scheduled here; the exception lines are the inventory.
- No decision has been taken about a workspace split. That is T-1004 and needs
  owner approval before any crate extraction.

## Boundaries

No browser, network, login, credentials, profile data, capability-proof record,
shell-policy behavior, release, tag, deployment, PR, merge or force-push.
`capability_proof` remains the single proof gate; no second proof store was
created. Only the five files in the T-997 scope plus the mandatory
`docs/CURRENT_WORK.md` handover were changed.
