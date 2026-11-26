# T-1005 proof: CLI grouping design

> **Referenz.** Task proof for T-1005. The design snapshot lives in
> `docs/arch/CLI_GROUPS.md`; the live CLI contract is `src/cli.rs` and the
> binary's `--help`.

**Claim.** Branch `docs/T-1005-cli-groups`, claim commit `afd11c9`, owner `local/opencode`, 2026-11-26. The owner's standing "autonom weiterarbeiten" overrode the formal `depends_on: T-997` gate; T-997 stays `claimed` because it is not merged into `master`. T-1004 was skipped because it needs explicit owner approval; T-1005 depends only on T-997.

## Bestand zuerst geprüft

`src/cli.rs` holds one flat `Commands` enum with 43 subcommands (clap's own `help` excluded). `ask` is already documented as the successor of `run`/`relay`; `api` and `ui --api` both expose the server role; `serve` does not exist yet. No grouping mechanism was present. No code was changed.

## Delivered

- `docs/arch/CLI_GROUPS.md`: verbatim *before* top-level help, per-command snapshot method, findings, five proposed groups (`brain`, `run`, `check`, `api`, `dev`), illustrative *after* help text, default-help policy and a 43-row alias table that keeps every existing command name callable at the root.
- This proof. No `src/`, `Cargo.toml` or test file was touched.

The snapshot was taken from the browser-free binary built at `938bbf1` (`cargo build --no-default-features`) because the default `webview` binary crashes before `main` with `0xC0000005` on this host. The command enum is not feature-gated, so the help text is the same; only runtime availability of browser-backed commands differs.

## Test counts

| Run | Before | After |
|---|---:|---:|
| `cargo test --lib --quiet` | 1523 passed / 0 failed / 1 ignored | 1523 passed / 0 failed / 1 ignored |

No tests were added or removed.

## Gates measured

- `cargo fmt --all -- --check`: 0
- `cargo clippy --features webview --all-targets -- -D warnings`: 0
- `cargo test --lib --quiet`: 1523 passed / 0 failed / 1 ignored
- `git diff --check`: 0
- `cargo test --lib startup::tests::betriebs_markdown_hat_eine_wahrheit -- --exact`: 1 passed (verifies the new documentation banner)

## Boundaries and open questions

No command name was removed, no CLI behavior changed, no credentials, live provider call, release, tag, deployment, PR or merge. T-1005 stays `claimed` until its branch is merged into `master`.

Open for a future implementation task: whether `serve` maps to today's `api` or to `ui --api` (the design picks the non-invasive `api` mapping), and whether hidden root aliases should be listed by a `help --all` mode. T-1004 (crate split) still needs explicit owner approval and was not started.
