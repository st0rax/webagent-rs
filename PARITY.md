# PARITY.md — webagent (Python) → webagent-rs (Rust) parity audit

Authoritative, code-verified comparison of the Python reference implementation
(`../webagent/src/webagent/`) against the Rust port (`webagent-rs/src/`).

- **Reference (Python):** `C:\Users\storax\Desktop\webagent\src\webagent\`
- **Target (Rust):** this repository, `src/*.rs`
- **Method:** every row below was checked by reading both sources; no guessing.
  Where a feature exists in Rust but behavioral parity could not be confirmed from
  source alone, it is marked **Partial** with the exact thing to verify.
- **Scope:** documentation only. No Rust code was changed.

Companion docs already in this repo: [`MERGE_AND_PARITY.md`](MERGE_AND_PARITY.md)
(high-level CLI table + platform plan) and [`PROVIDER_STATUS.md`](PROVIDER_STATUS.md)
(live per-provider automation status). This file is the detailed per-feature ledger.

---

## 1. Rust modules (`src/*.rs`) and what they cover

| Rust module | Lines | Responsibility | Python counterpart |
|---|---:|---|---|
| `main.rs` | 567 | clap CLI entry, subcommand dispatch | `cli.py` (partial) |
| `repl.rs` | ~400 | interactive REPL loop + slash router | `cli.py::cmd_repl` (core subset) |
| `browser_pool.rs` | ~180 | shared-browser singleton pool | `browser_pool.py` |
| `lib.rs` | 162 | crate root, UTC/时间 helpers, `pid_alive`, char-safe slicing | (spread across Python stdlib) |
| `protocol.rs` | 873 | `webagent/1` parser + `WEBAGENT/1 SHELL` raw format | `protocol.py` |
| `controller.rs` | 1037 | Plan/Act/Observe state machine, resume, loop/budget guard | `controller_new.py` |
| `brain.rs` | 193 | `BrainBackend` trait + `SessionState`/`BrainResponse` | `brains/base.py` |
| `browser.rs` | 911 | concrete `WebBrainBackend` driving Chromium via CDP | `brains/playwright_base.py` + `brains/*.py` |
| `cdp.rs` | 595 | minimal Chrome DevTools Protocol client (replaces Playwright) | `browser_launch.py` + Playwright |
| `executor.rs` | 263 | shell execution (PowerShell / sh) | `executor/powershell.py` + `executor/base.py` |
| `observer.rs` | 152 | transient-label + rate-limit text heuristics | `observer.py` (pure parts) |
| `memory.rs` | 475 | brain-independent long-term memory | `memory.py` |
| `run_store.rs` | 541 | run metadata persistence + stale reconcile | `persistence/run_store.py` |
| `transcript.rs` | 227 | JSONL transcript append + recovery tail | `persistence/transcript.py` |
| `timeouts.rs` | 127 | dynamic timeout policy | `timeouts.py` |
| `loop_guard.rs` | 98 | repeated-read fingerprinting | `loop_guard.py` |
| `prompts.rs` | 218 | autonomous/resume system prompts | `prompts.py` |
| `doctor.rs` | 1379 | per-brain diagnosis report | `doctor.py` |
| `watchdog.rs` | 778 | orphaned runs / stale lock scan + repair | `watchdog.py` |
| `config.rs` | 210 | paths, brain table, env flags | `config.py` |
| `comms.rs` | 216 | **new** webagent-internal agent-to-agent messaging store | (no direct Python equivalent; loosely relates to `relay.py`/bot2bot) |

### CLI subcommands exposed

**Rust (`main.rs`):** `run`, `login`, `diagnose`, `repl`, `doctor`, `watchdog`,
`brains-health`, `relay`, `oobe`, `maintenance-check` — **10 commands.**

**Python (`cli.py::build_parser`):** `run`, `repl`, `brains-health`, `doctor`,
`watchdog`, `maintenance-check`, `diagnose`, `login`, `oobe`, `relay`,
`genius` (alias `consensus`) — **11 commands.**

**Missing from the Rust CLI:** `genius`/`consensus` (Deferred).

### REPL slash commands

- **Python** (`cli.py::cmd_repl`): `/exit`, `/update`, `/oobe`, `/memory`,
  `/remember`, `/forget`, `/switch`, `/genius` (`/consensus`), `/vote`,
  `/score` (`/perf`), `/new`, `/login`, `/waitlogin`, `/chat`, plus bare input =
  autonomous run.
- **Rust** (`repl.rs`): `/exit`, `/quit`, `/new`, `/memory`, `/remember`, `/forget`,
  `/switch`, `/login`, `/chat`, plus bare input = autonomous run (persistent session).
- **Gap:** `/update`, `/oobe`, `/genius`, `/vote`, `/score`, `/waitlogin` (council/update
  surfaces deferred).

---

## 2. Per-feature parity table

Status legend: **Present** = ported with matching behavior; **Partial** = present
but with a behavioral or coverage gap (see note); **Missing** = not in Rust;
**Deferred** = intentionally postponed upstream.

| Feature | Python location | Rust location | Status | Notes |
|---|---|---|---|---|
| Plan/Act/Observe controller | `controller_new.py` | `controller.rs` | **Present** | Faithful port: serial action execution, per-action dedup by run-wide id, incomplete-response recovery (max 5 retries), protocol-error streak abort (3), heartbeat (30s), observation byte tracking, resume via `conversation_ref` restore + transcript-tail fallback. Tests mirror Python. |
| Protocol `webagent/1` (JSON) | `protocol.py` | `protocol.rs` | **Present** | Near byte-identical: same regexes (rendered-JSON labels, UI controls, leading-prose strip, protocol extraction), Windows-path repair for message-only, duplicate-id/finish-alone/message-alone rules, timeout range (0<t≤3600), truncation detection. Uses `fancy_regex` for the lookahead the `regex` crate lacks. Large shared test suite. |
| `WEBAGENT/1 SHELL` raw format | `protocol.py::_SCRIPT_ENVELOPE` | `protocol.rs::script_envelope_regex` | **Present** | Identical envelope grammar, nested-script unwrap with id match, byte-for-byte script preservation. |
| Autonomous / resume prompts | `prompts.py` | `prompts.rs` | **Present** | Prompt text is effectively identical (autonomous prefix, memory-context wrapper, resume-continue, resume-recovery). |
| Dynamic timeout policy | `timeouts.py` | `timeouts.rs` | **Present** | Identical operation bases, per-brain multipliers, message-size extension, env overrides (`WEBAGENT_TIMEOUT_MULT/MIN/MAX`), override-as-minimum semantics. |
| Loop guard (repeated reads) | `loop_guard.py` | `loop_guard.rs` | **Present** | Same 5 fingerprint patterns and warning message. NOTE: controller abort threshold differs — see config row. |
| Response observer heuristics | `observer.py` | `observer.rs` + `browser.rs` | **Partial** | Pure text helpers (`is_transient_response_text`, `is_claude_limit_response_text`) are ported verbatim. The stateful DOM `ResponseObserver.wait_for_response` is **re-implemented inside `browser.rs::wait_response`** (stop-button authority, stability window, interruption handling). Verify: stream-completion edge cases match, and that the thinking-prefix strip (`observer.py::_THINKING_PREFIX`) is equivalently handled (Rust leans on `protocol.rs` prose-stripping instead). |
| Shell executor | `executor/powershell.py` | `executor.rs` | **Present** | Persistent `pwsh`/`powershell` (Windows) or `bash`/`sh` (Unix) session: base64-wrapped commands, nonce completion marker, `$LASTEXITCODE` capture, `start`/`stop` wired in controller. `send_interrupt` sends Ctrl+C via stdin (Python uses `GenerateConsoleCtrlEvent` on Windows — functionally equivalent for timeout recovery). |
| CDP / browser driver | `brains/playwright_base.py`, `browser_launch.py` | `browser.rs`, `cdp.rs` | **Partial** | Custom CDP client replaces Playwright (launch, navigate, `Runtime.evaluate`, real `Input` mouse/key/insertText). Selector scanning is per-selector try/catch. Live status (`PROVIDER_STATUS.md`): 6/8 providers work end-to-end; **`gemini` (submit block)** and **`qwen` (automation hard-block)** do not. Verify per provider against Playwright behavior. |
| Brain backends (8 providers) | `brains/{chatgpt,claude,deepseek,gemini,kimi,mistral,qwen,zai}.py` | `browser.rs` (one generic backend) | **Partial** | Python has per-provider subclasses with provider-specific overrides; Rust uses a single `WebBrainBackend` fully driven by `selectors/<id>.json` + generic interruption handling. All 8 selector files present in `selectors/`. Verify provider quirks encoded in the Python subclasses are all covered by selectors/interruptions (esp. gemini/qwen). |
| Run persistence | `persistence/run_store.py` | `run_store.rs` | **Present** | Create/load/save run meta, terminal-status guard, `reconcile_stale_runs`, `completed_actions`, `conversation_ref`, `extra` bag. |
| Transcript | `persistence/transcript.py` | `transcript.rs` | **Present** | JSONL append with role/content/extra; recovery-tail by char budget. |
| Long-term memory | `memory.py` | `memory.rs` | **Present** | API parity (add/delete/list/search/record_run). Search uses same token overlap + recency scoring; `ORDER BY id DESC` before score (2026-07-14). Storage: SQLite (Python) vs JSON-Lines (Rust) — ranking aligned via fixture tests. |
| Doctor | `doctor.py` | `doctor.rs` | **Present** | Per-brain selectors/profile-lock/last-done-run/login-state/recovery-hint; human + `--json` output wired in `main.rs`. |
| Watchdog | `watchdog.py` | `watchdog.rs` | **Present** | Orphaned runs, stale bridge locks, stale profile locks; dry-run vs repair; `--json`. NOTE: Rust CLI default is **dry-run** (`--repair` to fix); Python default **repairs** (`--dry-run` to scan). Interval/daemon loop present in Python CLI, not in Rust CLI. |
| maintenance-check | `cli.py::cmd_maintenance_check` | `main.rs::cmd_maintenance_check` | **Partial** | Different checks. Python: git-clean, VERSION/pyproject/runtime version agreement, selectors+bot2bot presence, optional `pytest`. Rust: doctor-ok + watchdog dry-run-clean + optional `cargo test`. Both are read-only gates returning exit 0/2, but they assert different invariants. |
| brains-health | `cli.py::cmd_brains_health` | `brains_health.rs` + `main.rs` | **Present** | Pre-flight ohne Browser; `--allow-empty-profile`. |
| login | `cli.py::cmd_login` + `oobe.perform_brain_login` | `main.rs::cmd_login` + `browser.rs::interactive_login` | **Present** | Opens headed browser, polls login state, no credential entry, flushes session to profile. Rust adds `--timeout`; Python has `--yes`/non-tty auto-confirm. |
| diagnose | `cli.py::cmd_diagnose` | `main.rs::cmd_diagnose` + `browser.rs::live_diagnose` | **Present** | Rust returns a structured `LiveDiagnosis` (session_state, logged_in, composer, assistant count, cloudflare, url). Python additionally dumps per-selector counts and a screenshot; Rust exposes richer DOM via `browser.rs::dom_report` but the `diagnose` command prints the summary form. |
| relay / bot2bot single-turn | `relay.py` (`relay_single_turn`) | `relay.rs` + `main.rs` | **Present** | `webagent relay --brain --message`; `comms.rs` bleibt separater Inbox-Store. |
| oobe first-run wizard | `oobe.py`, `cli.py::cmd_oobe` | `oobe.rs` + `main.rs` | **Partial** | CLI-Subset: Brain-Auswahl + State; interaktiver Login-Hinweis; REPL `/oobe` fehlt noch. |
| Agent / AgentManager | `agent.py` | (folded into `controller.rs` + `browser.rs`) | **Present (by design)** | High-level Agent abstraction; Rust achieves the same lifecycle via controller+backend. Functional equivalent for single-agent flows; sub-agent management used by council is absent (see Genius-Council). |
| browser_pool / shared-browser | `browser_pool.py`, `browser_launch.py`, `config.use_shared_browser` | `browser_pool.rs`, `config.rs` | **Present** | One Chromium + `profiles/shared`, one CDP tab per brain, refcount + `persist_browser_tabs()` (2026-07-14). Activate via `WEBAGENT_USE_SHARED_BROWSER=1`. |
| terminal_status (REPL banner/summary) | `terminal_status.py` | inline `println!` in `main.rs`/`repl.rs` | **Partial** | Rust prints plain summaries; the styled banner/`print_run_summary` UX is not reproduced. |
| Genius-Council: council policy | `council.py` | — | **Deferred** | Loadout, moderator election (ranked/instant-runoff), selection parsing. |
| Genius-Council: consensus runner | `consensus.py`, `cli.py::cmd_genius` | — | **Deferred** | Workspace-driven multi-brain cycles + moderator synthesis. |
| Genius-Council: vote | `cli.py::conduct_vote` + `council.py` | — | **Deferred** | Ballot collection with DOM ranking parse + cache, instant-runoff. |
| Genius-Council: performance index | `performance.py` | — | **Deferred** | Auditable cumulative scoring, `award_task`/`penalize`, `/score`. |
| Genius-Council: leader calibration | `leader_calibration.py` | — | **Deferred** | Blind moderator calibration case-suite + inter-rater metrics. |
| Genius-Council: battleroyale | `battleroyale.py` | — | **Deferred** | `/br` methodology-suggestion collection across chained brains. |
| Genius-Council: countup eval | `countup_eval.py` | — | **Deferred** | Count-up chain ledger + methodology consensus. |

> **Genius-Council is DEFERRED as a stack.** Upstream `MERGE_AND_PARITY.md` §2 marks
> the entire council/consensus/vote/performance/calibration/battleroyale/countup
> layer as not-yet-ported. This audit reflects that: rows above are **Deferred**,
> not accidental omissions.

---

## 3. Configuration divergences (verified, worth reconciling)

These are real value/name mismatches between `config.py` / `controller_new.py` and
`config.rs` / `controller.rs`:

| Setting | Python | Rust | Impact |
|---|---|---|---|
| `MAX_OBSERVATION_CHARS` | `12000` | `12000` (`config.rs`) | **Reconciled** (2026-07-14). |
| `LOOP_GUARD_ABORT_COUNT` | `8` | `8` (`config.rs`) | **Reconciled** (2026-07-14). |
| Shared-browser env var | `WEBAGENT_USE_SHARED_BROWSER` | `WEBAGENT_USE_SHARED_BROWSER` (+ legacy alias) | **Reconciled** (2026-07-14). |
| Profiles dir | `DATA_DIR/profiles` and all brains share `SHARED_PROFILE_DIR` | `ROOT/profiles`, per-brain `profiles/<id>` (override via `WEBAGENT_PROFILE_DIR`) | Different profile layout and default sharing model. |
| bot2bot root | `ROOT/../bot2bot` (sibling) + install pointer file | sibling + `WEBAGENT_BOT2BOT_ROOT` / install pointer | **Reconciled** (2026-07-14). |
| consensus workspace | `~/Desktop/consensus` | `bot2bot_root/consensus_<stamp>` | Different default (moot until council is ported). |
| Startup reconcile | `cli.main` runs `RunStore.reconcile_stale_runs()` on every command (except maintenance-check) | `main.rs::startup_reconcile_runs()` | **Reconciled** (2026-07-14). |

Also confirmed **matching**: protocol version string, timeout policy numbers,
loop-guard fingerprints, prompt text, resume char budget (`8000`), memory context
limit (`5`), heartbeat interval (`30s`), brain table (all 8 URLs identical).

---

## 4. Prioritized gap checklist (Wave 2 seed)

Ordered by user value. Size: **S** ≈ <½ day, **M** ≈ 1–2 days, **L** ≈ 3+ days.
Genius-Council items are intentionally last (Deferred upstream).

- [x] **Persistent shell session in `executor.rs`** — done (2026-07-14): long-lived
  shell, base64+nonce marker, `$LASTEXITCODE` capture, controller `start`/`stop`.
  **Size: L**
- [x] **Finish the `gemini` and `qwen` provider integrations** — 8/8 live (2026-07-14). **Size: L**
- [x] **Reconcile config divergences** — done (2026-07-14). **Size: S**
- [x] **Port `brains-health`** — `brains_health.rs` + CLI (2026-07-14). **Size: S**
- [x] **Port `relay` as a CLI command** — `relay.rs` + CLI (2026-07-14). **Size: S**
- [x] **Expand the REPL** — done (2026-07-14): persistent session, `/memory`,
  `/remember`, `/forget`, `/switch`, `/login`, `/chat`. **Size: M**
- [x] **Port `oobe` first-run wizard** — `oobe.rs` + CLI subset (2026-07-14); REPL `/oobe` still optional. **Size: M**
- [ ] **Align `maintenance-check` semantics** — decide whether the Rust gate should
  also assert version agreement / git-clean, or document the intentional divergence.
  **Size: S**
- [x] **Shared browser pool** — `browser_pool.rs` + flag wiring (2026-07-14). **Size: M**
- [x] **Verify memory search parity** — `ORDER BY id DESC` + fixture tests (2026-07-14). **Size: M**
- [ ] **Genius-Council stack (DEFERRED)** — council policy, consensus runner, vote,
  performance index, leader calibration, battleroyale, countup eval, and the
  `genius`/`/vote`/`/score` surfaces. Port only after single-agent parity is
  locked. **Size: L (multiple units)**

---

_Audit generated by reading `webagent-rs/src/*.rs` and
`webagent/src/webagent/**` in full. Doc-only unit; no Rust code changed._
