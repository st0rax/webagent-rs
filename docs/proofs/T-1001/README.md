# T-1001 proof: capability <-> capability_proof cycle removed

> **Referenz.** Task proof for T-1001. The live architecture ratchet remains
> `tests/arch_layers.rs`, `docs/arch/layers.toml` and
> `docs/arch/layer_exceptions.txt`.

**Claim.** Branch `refactor/T-1001-capability-cycle`, claim commit `2de15c5`, owner `local/opencode`, 2026-11-26. The owner's standing "autonom weiterarbeiten" overrode the formal `depends_on: T-997` gate; T-997 stays `claimed` because it is not merged into `master`.

## Bestand zuerst geprüft

`src/capability_types.rs` did not exist before this task, so the leaf named in the board scope was created. `capability_proof` already contained the only proof store (`capability/proofs.jsonl`, `record_measurement`, `record_proof`); no second gate or store was introduced. The previous cycle was `capability -> capability_proof` (`selector_hash_for`, `proof_state`, `ProofState`, `ExpiryReason`) and `capability_proof -> capability` (`Capability`, `capability`, `capability_for_route`).

## Changes

- `src/capability_types.rs` (new leaf): `ProofKind`, `Capability`, `CATALOG`, `capability()` and `capability_for_route()` moved verbatim from `capability.rs`. The module has no `crate::` dependency.
- `src/capability.rs`: re-exports the moved items, so all existing public paths (`webagent::capability::CATALOG`, `capability()`, `Capability`, `ProofKind`) and every caller are unchanged. It still uses the proof gate in one direction only.
- `src/capability_proof.rs`: imports `Capability`, `capability()` and `capability_for_route()` from `capability_types`; no reference to `crate::capability` remains. The store, record format, hash and TTL logic are untouched.
- `src/lib.rs`: declares `pub mod capability_types;` (see deviation).
- `docs/arch/layers.toml`: registers the new module in L4 alongside the two existing capability modules (see deviation).
- `docs/arch/layer_exceptions.txt`: removed `cycle:capability,capability_proof`; header and cycle comment updated to four remaining cycles.

`git grep -n 'crate::capability' -- src/capability_proof.rs` shows only `capability_types`. `git grep -n 'crate::capability_proof' -- src/capability_types.rs` is empty. No CLI, WEBAGENT/1 protocol, API or shell-policy behavior changed; `Cargo.toml` and every `[profile.*]` setting are untouched.

## Guard delta (measured)

| Metric | Claim commit | After |
|---|---:|---:|
| modules | 87 | 88 |
| edges | 273 | 274 |
| cycles | 5 | 4 |
| upward edges | 32 | 32 |

Removed exception: `cycle:capability,capability_proof`. No `edge:` line changed: the former `capability_proof -> capability` edge became `capability_proof -> capability_types`, and `capability -> capability_types` was added; all three modules share L4, so no upward edge appeared. The ratchet initially rejected the stale cycle line, which confirmed the removal before the line was deleted.

## Test counts

| Run | Before | After |
|---|---:|---:|
| `cargo test --lib --quiet` | 1523 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |

No test was added or removed; this is a pure move with re-exports. The claim-time baseline first failed on `startup::tests::betriebs_markdown_hat_eine_wahrheit` because the published T-1000 proof lacked its required reference banner. That was fixed on the T-1000 branch (`27b38af`, pushed) and cherry-picked here (`52b41f8`) before the baseline was re-measured.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib --quiet`: 1523 passed / 0 failed / 1 ignored
- `cargo test --test arch_layers -- --nocapture`: 4 passed; `architecture guard: modules=88, edges=274, cycles=4, upward_edges=32`
- `cargo test --all-targets --quiet`: 1523 lib + 7 binary-unit + 4 integration passed
- `cargo test --locked --no-default-features --test arch_layers --quiet`: 4 passed
- `git diff --check`: 0

## Deviation

`src/lib.rs` and `docs/arch/layers.toml` are outside the board's three-file scope. A new top-level module must be declared in `lib.rs` to exist, and the ratchet test requires every declared module to carry a layer rank. Both edits are one line each and change no behavior. The proof file format (`proofs.jsonl`) is unchanged.

## Binary smoke (blocked, not a regression)

After `cargo build`, `target\\debug\\webagent.exe --help` and `diagnose` both exit `-1073741515` (`0xC0000005`) before `main`, as on the earlier phase-16 branches. This crash was present at the claim-time baseline and is not caused by this refactor. No `[profile.*]` setting was touched.

## Boundaries

No login, live provider call, credentials, capability-proof record, release, tag, deployment, PR or merge. T-1001 stays `claimed` until its branch is merged into `master`. T-1004 (crate-split decision) still needs explicit owner approval and was not touched.
