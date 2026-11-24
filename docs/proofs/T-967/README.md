# T-967 — Release gate (work in progress)

**Reference:** claim and proof scaffold. This is not acceptance evidence yet.
T-967 is `claimed` on `chore/T-967-release-gate`; no tag or release is created by
this task.

## Problem

The `v0.11.4` tag was published from a commit whose `Cargo.toml` still said
`0.11.3`. The release workflow builds and publishes on any `v*` tag without
checking the manifest version and without running tests. A rebuild of that tag
therefore reports the wrong version, and a future mismatched tag would publish
silently.

## Planned changes

- `scripts/verify-release-version.ps1`: local, fail-closed check that
  `vX.Y.Z` matches the `[package] version` in `Cargo.toml` and the `webagent`
  entry in `Cargo.lock`.
- `.github/workflows/release.yml`: a `verify` job that runs the version check
  for tag refs and `cargo test --lib --locked` before any build job; all build
  and publish jobs depend on it.
- `Cargo.toml` / `Cargo.lock`: move to the next patch version (`0.11.5`) so
  `master` no longer claims to be the already-published `0.11.3`.
- `.github/workflows/ci.yml`: stop running the full matrix twice for the same
  pull-request head (push only on `master`, pull requests unchanged).
- `docs/RELEASE.md`: document the bump, merge, tag, and publish sequence.
- `CHANGELOG.md` and `docs/CURRENT_WORK.md`: record the change and its limits.

## Verification plan

- `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` → exit 0.
- `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.4` → exit 1 with a
  mismatch message.
- `pwsh -File scripts/verify-release-version.ps1 -Tag 0.11.5` → exit 2 with a
  format message.
- Parse both workflow YAML files.
- `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib`, `git diff --check`.

## Historical note

The existing `v0.11.4` tag is not moved, deleted, or force-pushed. Re-running
the new release workflow on that tag will fail the version check by design.
The next release must be tagged from a commit whose manifest matches the tag.
