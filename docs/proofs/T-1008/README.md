# T-1008 proof: local runnable WebView2 build

> **Referenz.** Task proof for T-1008. This is a build-only fix inside the
> single `webagent` package: no crate extraction, no merge, and no change to
> `Cargo.toml`, `[profile.*]` or dependencies.

**Claim.** Branch `fix/T-1008-webview-loader-smoke`, claim commit `2d64df9`,
owner `local/opencode`, 2026-11-26. The task was created and claimed in
`docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md` before any source
file was edited. T-1008 stays `claimed`; no `done`, no PR and no merge.

## Bestand zuerst geprüft

- The old smoke notes in T-1004, T-1006 and T-1007 report exit
  `-1073741515` as `0xC0000005`. That hex code is wrong: `-1073741515` as
  `u32` is `0xC0000135` (`STATUS_DLL_NOT_FOUND`), while `0xC0000005` would be
  `-1073741819` (`STATUS_ACCESS_VIOLATION`).
- `gdb -q -batch -ex run -ex "bt 30" --args target/debug/webagent.exe --help`
  ended with `During startup program exited with code 0xc0000135.` and no
  stack, so the failure is in the loader before `main`, not a code fault.
- `scripts/copy-webview2-loader.ps1` already knew both Cargo layouts
  (`build/webview2-com-sys-<hash>/out/x64` and
  `build/webview2-com-sys/<hash>/out/x64`). Running it with `-Profile both`
  made debug and release `--version`/`--help` exit 0. The missing piece was
  that a plain `cargo build` did not call it, so local smoke stayed blocked.
- The root `build.rs` already embeds `WEBAGENT_GIT_HASH` and compiles the
  Windows version resource with std + `winres` only; it is the right place for
  an automatic, dependency-free copy. The CI release workflow and the manual
  script remain unchanged.

## Delivered

- `build.rs` now calls `copy_webview2_loader(&target)` after the resource
  step. For a Windows target with `CARGO_FEATURE_WEBVIEW` enabled it locates
  the profile directory by looking for a sibling `build/webview2-com-sys[-*]`
  (robust against Cargo 1.100's `build/webagent/<hash>/out` layout as well as
  the older `build/webagent-<hash>/out`), finds
  `out/x64/WebView2Loader.dll` in both sys-crate layouts, and copies it to
  `target/<profile>/WebView2Loader.dll`. A missing source or failed copy is a
  `cargo:warning`, not a hard build failure; browser-free builds return early
  and stay quiet.
- `scripts/copy-webview2-loader.ps1` documents that it is now a manual
  fallback / CI helper and names the correct `0xC0000135` code.
- The misleading `0xC0000005` diagnosis is corrected in the proofs for
  T-1004, T-1006 and T-1007, in `docs/arch/CRATE_SPLIT.md` (including the old
  `codegen-units` claim) and in `docs/arch/CLI_GROUPS.md`. The new
  `docs/CURRENT_WORK.md` entry states that all earlier phase-16 smoke notes
  share the same misread.

## Gates measured

- `cargo fmt --all -- --check` — 0 after formatting `build.rs`.
- `cargo clippy --features webview --all-targets -- -D warnings` — 0.
- `cargo test --lib --quiet` — claim-time baseline (board-only commit
  `2d64df9`, implementation stashed): 1530 passed / 0 failed / 1 ignored
  (55.72 s); final: 1530 passed / 0 failed / 1 ignored (49.43 s), unchanged.
- `cargo test --test arch_layers -- --nocapture` — 4 passed; guard printed
  `modules=90, edges=276, cycles=3, upward_edges=26` (unchanged from T-1007).
- `cargo test --all-targets --quiet` — 1530 lib + 7 bin-unit + 4 integration
  passed, 0 failed.
- `cargo build --no-default-features --locked` — 0, no missing-DLL warning.
- `git diff --check` — 0 (only LF→CRLF working-copy warnings on edited files).

## Binary smoke (after deleting the copied DLL)

Each profile was tested with `WebView2Loader.dll` removed first, so the copy
had to come from the build script:

| Command | Result |
|---|---|
| `cargo build --locked` | 0; `target/debug/WebView2Loader.dll` present |
| `./target/debug/webagent.exe --version` | `webagent 0.11.7 (2d64df93f+dirty)`, exit 0 |
| `./target/debug/webagent.exe --help` | banner + command list, exit 0 |
| `cargo build --release --locked` | 0; `target/release/WebView2Loader.dll` present |
| `./target/release/webagent.exe --version` | `webagent 0.11.7 (2d64df93f+dirty)`, exit 0 |
| `./target/release/webagent.exe --help` | banner + command list, exit 0 |

The `+dirty` suffix is expected because the implementation was uncommitted
when measured; it is produced by the pre-existing `WEBAGENT_GIT_HASH` build
stamp, not by this fix.

## Grenze / offen

No crate extraction, merge, PR, tag, release or deployment was made. No
credentials were read or used, no provider/login call was made, and no live
browser session was started. `Cargo.toml`, `[profile.*]`, dependencies and the
CI workflow are unchanged. The task remains `claimed` until its branch is
merged.
