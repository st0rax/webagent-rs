# webagent (Rust)

> **Required before contributing:** Read [`AGENTS.md`](AGENTS.md) in full and follow it. Project-specific rules apply as well; where rules conflict, follow the stricter safety requirement.

webagent is a **local, browser-driven agent harness**. A logged-in web chat (“Brain”) plans; local tools such as PowerShell or a shell execute actions. Brains are interchangeable web chats (ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai). The browser session is used instead of a provider API key.

This is the Rust port of the original Python project. It provides a session web UI, REPL, and CLI on Windows, Linux, and Android/Termux. Embedded WebView brains use WebView2 on Windows and WebKitGTK on Linux.

## New here?

The [Newcomer Guide](docs/NEWCOMER_GUIDE.md) explains the project without requiring previous chat context and links to the right entry point for users, contributors, and agents.

- **Use the program:** follow the [Quick start](#quick-start-use-not-develop).
- **Understand the design:** read [the system overview](docs/OVERVIEW.md) and [architecture map](docs/ARCHITECTURE.md).
- **Contribute:** read [`START_HERE.md`](START_HERE.md), [`AGENTS.md`](AGENTS.md), and [`CONTRIBUTING.md`](CONTRIBUTING.md).
- **Current work and claims:** [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md) and [`docs/TASKBOARD.json`](docs/TASKBOARD.json). The JSON file is the sole source of truth for task status and claims.

## Quick start (use, not develop)

You do **not** need Rust or the source code to use a release. Download the appropriate binary from [GitHub Releases](https://github.com/st0rax/webagent-rs/releases/latest) and have an active browser session with the chat service you intend to use. No provider API key is required.

### 1. Download and prepare

On Linux, make the binary executable:

```
chmod +x webagent-linux-x86_64
```

On Windows, place `webagent-windows-x86_64.exe` and `WebView2Loader.dll` in the same directory.

### 2. Install Linux system dependencies

The embedded browser uses WebKitGTK. On Debian or Ubuntu:

```
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-0
```

WebView2 is already available on current Windows systems.

### 3. Initialize and sign in

```
./webagent-linux-x86_64 oobe
./webagent-linux-x86_64 login --brain chatgpt
```

The `login` command opens a browser window. **You sign in yourself**; the program does not ask for or store your credentials. The session remains in its own profile, so one sign-in per service is normally sufficient.

### 4. Run a first task

```
./webagent-linux-x86_64 run --brain chatgpt --task "List the files here and summarize what they contain"
```

The chat plans; your computer executes. To check whether the interface is usable, run `diagnose --brain chatgpt`.

> **Linux status:** The Linux WebView build has been built but has not yet been exercised on a Linux desktop. The Windows version is used regularly. If Linux does not work as expected, open an issue.

> **Android/Termux:** The aarch64 binary includes the core, REPL, and CLI, but **does not include an embedded browser**. An APK would be required for that; browser-based brains cannot currently run there.

## Release status and provider evidence

**Released version:** [v0.11.3](https://github.com/st0rax/webagent-rs/releases/tag/v0.11.3). The local web UI is the primary interface and is connected to the real `/api/*` endpoints. It supports Brain health, creating and switching sessions, live chat events, source selection (`/quelle`), multi-Brain group runs, file uploads, Brain-window visibility, a capability matrix, and a doctor report. See `docs/CURRENT_WORK.md` for the current handover and commit-specific verification.

**Historical provider evidence:** On 2026-07-16, all eight tested Brains returned headless relay responses. This is historical evidence, not a promise of current availability; web UIs, sessions, and quotas change. Measurements and later capability history are in [`docs/PROVIDER_STATUS.md`](docs/PROVIDER_STATUS.md).

An earlier claim that only five of eight Brains worked headlessly because ChatGPT, Claude, and Mistral were blocked by Cloudflare was incorrect: the recorded `cloudflare` result was false for all eight. Three bugs had masked the results (a tao event-loop panic on a non-main thread, a JavaScript wrapper that returned a Promise and caused `evaluate` to yield `{}`, and a “headless” window without focus so Enter was not delivered). See `docs/PROVIDER_STATUS.md` for the detailed history.

## Architecture

A Brain plans using the `webagent/1` JSON protocol. The controller executes actions serially and sends observations back to the Brain. Browser control, the agent loop, tool execution, the API bridge, and the web UI are separate layers.

For the current conceptual design, see [docs/OVERVIEW.md](docs/OVERVIEW.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Verify implementation details against the current code and tests.

### Module map

| Module | Responsibility |
|---|---|
| `protocol` | `webagent/1` parser, including the raw `WEBAGENT/1 SHELL` format |
| `controller` | Plan/Act/Observe state machine, resume, loop and budget guards |
| `brain` | Browser-neutral `BrainBackend` trait |
| `browser`, `webview_runtime`, `page_driver` | Embedded WebView backend (`wry`/`tao`) |
| `browser_pool` | Shared profile: one runtime and one tab per Brain |
| `executor` | Shell execution (PowerShell on Windows; sh/bash on Unix) |
| `run_store`, `transcript`, `memory` | JSON-Lines persistence |
| `comms` | Internal agent-to-agent messaging and per-agent inboxes |
| `doctor`, `watchdog`, `brains_health` | Diagnostics and pre-flight checks |
| `relay`, `oobe` | Single-turn relay and first-run setup wizard |
| `timeouts`, `loop_guard`, `observer`, `prompts`, `config` | Policies and heuristics |

Provider selectors live in [`selectors/`](selectors/); porting rules are in [`CONVENTIONS.md`](CONVENTIONS.md).

## Build and test

A Rust toolchain is required for development. Common checks include:

```
cargo fmt --all -- --check
cargo test --locked
```

For the browser-free build and tests:

```
cargo test --locked --no-default-features
```

Use the exact verification gates required by your task in [docs/WORK_CONTRACT.md](docs/WORK_CONTRACT.md) and [CONTRIBUTING.md](CONTRIBUTING.md). Do not report a check as passing unless it was actually run.

### Windows release build

The Windows release includes `WebView2Loader.dll`. The repository script copies it into the release output:

```
cargo build --release
pwsh -File scripts/copy-webview2-loader.ps1 -Profile release
```

For manual or platform-specific release steps, follow the current instructions in the repository's release workflow and contribution docs rather than relying on historical status notes.

## Usage

The local **web UI** is the default interface: running `webagent` without arguments starts it and opens `http://127.0.0.1:8788/`. It is connected to the real API and provides Brain health, local sessions, live chat events, source selection, multi-Brain group runs, file upload, Brain window visibility, a capability matrix, and a doctor report.

The line-oriented **REPL** is `webagent repl`; the pool/wall/benchmark view is `webagent tui`. From a pipe, use `webagent tui --force-tui --view=session`. On Windows, Brain windows are arranged above the TUI.

**REPL questions versus autonomous tasks:** normal REPL input runs an autonomous task (the Brain can modify files and execute local shell commands). `/chat <text>` is conversation without tools. Common slash commands include `/new`, `/resume`, `/status`, `/model`, `/dashboard`, and `/quit`.

The local browser-inference bridge for Pi is a separate integration: `webagent api serve` starts a token-protected loopback endpoint. See [`docs/API_BRIDGE.md`](docs/API_BRIDGE.md) for setup and examples.

### Common commands

```
webagent login             --brain <id> [--timeout <seconds>] [--force] [--auto]
webagent show              --brain <id> [--port 8788]
webagent hide              --brain <id> [--port 8788]
webagent login-all         [--timeout <seconds>] [--force] [--parallel N]
webagent ask                --task "<task>" [--brain <id>|auto] [--auto|--chat] [--resume <run_id>] [--headless] [--max-cycles N] [--no-memory] [--json]
webagent run                --task "<task>" [--brain <id>|auto] [--headless] [--max-cycles N] [--resume <run_id>] [--no-memory]
webagent repl               [--brain <id>|auto] [--headless]
webagent relay              --message "<text>" [--brain <id>|auto] [--headless] [--timeout <seconds>] [--json]
webagent diagnose           --brain <id> [--headless]
webagent doctor             [--brain <id>]... [--json]
webagent watchdog           [--repair] [--json]
webagent brains-health      [--allow-empty-profile]
webagent autoresearch       --brain <id> --goal "<text>" --eval "<command>" [--direction higher|lower]
webagent oobe               [--brains <csv>] [--skip-login] [--yes]
webagent maintenance-check  [--json]
```

`ask` is the unified input command: by default it starts an autonomous run; `--chat` performs a single conversational turn without the controller/shell loop. `run` remains a compatible alias for an autonomous run, and `relay` is a conversational single-turn alias. The auto-router can choose a Brain for `ask`, `run`, `repl`, and `relay` when no Brain is specified or `--brain auto` is used.

`relay --json` and `ask --chat --json` return one send-and-wait turn as machine-readable JSON. In the protocol, file changes use native `edit` (exactly one matching anchor) and `write` actions rather than shell-escaped text.

**Autoresearch:** the `--eval` command must print a number as its final stdout line. Improved iterations are committed on an `autoresearch/` branch; regressions are reverted. Merging remains manual. Logs are stored under `data/autoresearch/<run_id>/log.md`.

**Wiki memory:** Markdown pages with `[[links]]` and `index.md` live under `data/memory/wiki/`. The index can be included as context in autonomous runs. REPL commands include `/wiki`, `/wiki <search>`, and `/wiki lint`.

Available built-in Brains: `chatgpt, deepseek, kimi, gemini, qwen, claude, mistral, zai`.

### Login

Use `webagent login --brain <brain>` to open the browser for a provider and sign in interactively. Never place credentials, cookies, or private browser profiles in the repository.

### Profiles and multi-Brain operation

| Path | Role |
|---|---|
| `profiles/<brain>/` | Canonical login profile written by `login` / `login-all` |
| `profiles/reference/<brain>/` | Optional golden template; preferred Swarm source when present |
| `profiles/swarm/<run>_<brain>/` | Per-participant runtime copy, cleaned up after the run |
| Shared profile | Default for normal `/chat` and `run` when no override is set |

A Swarm run gives each Brain an isolated profile copy. The orchestrator is fixed when explicitly selected; otherwise it uses the reliability of responding Brains. Live voting is optional and can be enabled with `WEBAGENT_SWARM_VOTE=1`. Swarm profiles are cleaned up afterward.

### Configuration

| Variable | Effect |
|---|---|
| `WEBAGENT_TIMEOUT_MULT` / `WEBAGENT_TIMEOUT_MIN` / `WEBAGENT_TIMEOUT_MAX` | Scale dynamic timeouts |
| `WEBAGENT_SHARED_BROWSER` | Enable a shared profile and `BrowserPool` |
| `WEBAGENT_PERSIST_TABS` | Keep tabs open after relay/run in pool mode |
| `WEBAGENT_SHELL_STRICT` | Set to `1` to restrict shell execution to lower-risk prefixes plus a denylist |
| `WEBAGENT_LOGIN_TO_REFERENCE` | Set to `1` to copy profiles to `profiles/reference/<brain>` after `login-all` |
| `WEBAGENT_SWARM_VOTE` | Set to `1` to enable phase-two live voting in `/swarm`; otherwise use reliability scores |
| `WEBAGENT_PROFILE_DIR` | Override the profile root (default: `profiles`) |

Verify names and defaults against the current code before relying on them; historical examples may be stale.

## Data and security

Runs are stored under `data/runs/<run_id>/` (`meta.json`, `transcript.jsonl`, `events.jsonl`); long-term memory is stored in `data/memory.jsonl`. Browser profiles live under `profiles/`. Both runtime data and profiles are excluded from version control.

Browser profiles contain cookies. Never commit them, copy credentials into the repository, or replace an authenticated profile as a troubleshooting shortcut.

The Brain can request shell commands in the logged-in user's context. The shell policy blocks known dangerous patterns and records executions, but it is **not an operating-system sandbox**. Use webagent only in a trusted environment and with appropriately limited user permissions.

Never commit credentials, tokens, cookies, private transcripts, `.env` files, or runtime data.

## Development and collaboration

- [Newcomer Guide](docs/NEWCOMER_GUIDE.md) — context-free orientation.
- [START_HERE.md](START_HERE.md) — required developer/agent entry point.
- [CONTRIBUTING.md](CONTRIBUTING.md) — workflow and acceptance checks.
- [docs/CURRENT_WORK.md](docs/CURRENT_WORK.md) — current handover state.
- [docs/COLLABORATION.md](docs/COLLABORATION.md) — GitHub collaboration.
- [CONVENTIONS.md](CONVENTIONS.md) — porting conventions.
- [docs/MERGE_AND_PARITY.md](docs/MERGE_AND_PARITY.md) — parity with the Python project.
- [docs/TASKBOARD.json](docs/TASKBOARD.json) — authoritative task status and claims; [`docs/TASKBOARD.md`](docs/TASKBOARD.md) is only a mirror.

**Documentation language:** English is the default for maintained documentation, task titles, agent instructions, issues, and pull requests. Preserve dated historical reports and evidence when rewriting them would damage traceability; add an English summary when useful. The repository language does not dictate the language used in the product's user-facing output.

## Platforms and release binaries

| Platform | Artifact | Notes |
|---|---|---|
| Windows x86_64 | `webagent-windows-x86_64.exe` + `WebView2Loader.dll` | Web UI by default, REPL, CLI, embedded WebView2 |
| Linux x86_64 | `webagent-linux-x86_64` | Web UI by default, REPL, CLI, embedded WebView via WebKitGTK; requires `libwebkit2gtk-4.1-0` |
| Android aarch64 | `webagent-aarch64-linux-android` | Core, REPL, and CLI for Termux; no Play Store APK or embedded WebView |

GitHub Releases are created from `v*` tags by [the release workflow](.github/workflows/release.yml). Embedded-WebView brains run on Windows (WebView2) and Linux (WebKitGTK). Android/Termux uses the same core without an embedded browser.

## License

MIT.
