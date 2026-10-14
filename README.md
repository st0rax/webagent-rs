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

## Architecture

A Brain plans using the `webagent/1` JSON protocol. The controller executes actions serially and sends observations back to the Brain. Browser control, the agent loop, tool execution, the API bridge, and the web UI are separate layers.

For the current conceptual design, see [docs/OVERVIEW.md](docs/OVERVIEW.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Verify implementation details against the current code and tests.

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

### Login

Use `webagent login --brain <brain>` to open the browser for a provider and sign in interactively. Never place credentials, cookies, or private browser profiles in the repository.

### Profiles and multi-Brain operation

Profile and pool behavior is documented in the architecture and operational docs. Runtime profile data belongs in the user's data directory, not in Git. Do not delete or replace an authenticated profile as a troubleshooting shortcut.

### Configuration

Configuration options and environment variables are documented alongside the relevant implementation and operational guides. Verify names and defaults against the current code; historical examples may be stale.

## Data and security

Runtime state and browser profiles are stored outside the source tree. Browser profiles contain cookies and are excluded from version control.

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
