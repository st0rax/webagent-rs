# WebAgent-RS Code Review
*Erstellt: 2026-07-16 | Reviewer: Qwen Code*  
*Gegenprüfung & Ergänzung: 2026-07-16 | Grok (xAI)*

---

## Executive Summary

**Status:** Production-tauglich für Single-User-Local-Agent  
**Architektur:** Solide, modular, gut durchdacht  
**Code-Qualität:** Hoch im Kern; **Test-Baseline aktuell nicht grün** (siehe Gegenprüfung)  
**Haupt-Risiken (revidiert):**

1. **Shell ohne Policy** – Brain-JSON → `executor.execute` (by Design, aber ungeschützt)
2. **Rote Executor-Unit-Tests** – 7 Failures blockieren verlässliche Refactors
3. **Große Module** – `doctor.rs` / `browser.rs` größer als `controller.rs`
4. **Selektor-Drift** – Playwright-Text-Syntax im JS-Prelude

---

## 🎯 Stärken

| Bereich | Details |
|---------|---------|
| **Modulare Trennung** | `brain.rs` (Trait), `browser.rs` (WebView-Impl), `controller.rs` (Loop), `protocol.rs` (Parsing), `run_store.rs` (Persistenz) |
| **Pluggable Brains** | `BrainBackend` Trait + `WebBrainBackend`; Provider-Logik in `send_*()` gekapselt |
| **Recovery/Resume** | `RunMeta` mit `conversation_ref`, `completed_actions` für Idempotenz, `reconcile_stale_runs` |
| **Loop-Guard** | Fingerprint-basiert (Command-Muster), Warn/Abort-Schwellen, in `RunMeta.extra` persistiert; **schon in `loop_guard.rs` ausgelagert** |
| **Testbarkeit** | Viele Unit-Tests, Mock-Traits (`MockPageDriver`, `DummyBrain`), isolierte Temp-Dirs |
| **Konfiguration** | `config.rs` mit `BRAIN_TABLE` + JSON-Selektoren + Env-Overrides; `LOOP_GUARD_*` bereits dort |
| **Atomic Saves** | `RunStore::save_internal` schreibt `meta.json.tmp` → rename |

---

## ⚠️ Kritische Risiken & Redundanzen (original Qwen, priorisiert)

### P0 – Security: Shell Command Injection
**Datei:** `src/executor.rs` / Aufruf in `src/controller.rs` (`execute_actions_serial`)  
**Problem:** Action-`command` kommt vom Brain (JSON), wird direkt via persistenter PowerShell/sh-Session ausgeführt. Kein Escaping/Allowlist.  
**Fix (Qwen):** Allowlist für Commands oder Prefix-basierter Dispatcher (`webagent:`).  
**→ Grok:** Finding bestätigt; **Allowlist-only ablehnen** — siehe Gegenprüfung & `CLAUDE_PROPOSALS.md`.

### P0 – Architecture: `controller.rs` God-Module (~1.150 Zeilen)
**Datei:** `src/controller.rs`  
**Problem:** Run-Lifecycle, Resume, Brain-Turn, Action-Execution, Observation-Bounding, Loop-Guard, Comms, Transcript – vieles in einer Datei.  
**Refactoring (Qwen):** Split in 4 Module (`run_loop`, `resume`, `action_executor`, `observation`).  
**→ Grok:** Größe eher **~1025 LOC**; Split sinnvoll, aber **nicht die größte Datei** und **nach** Test-Grün.

### P1 – Concurrency: Blocking I/O im Sync-Kontext
**Dateien:** `src/browser.rs`, `src/run_store.rs`, `src/controller.rs`  
**Problem:** `std::thread::sleep()` blockiert gesamten Agent – keine parallelen Runs.  
**Fix (Qwen):** `tokio::time::sleep` + `spawn_blocking`.  
**→ Grok:** Technisch korrekt, **strategisch verfrüht** (sequentieller Agent-Loop). → P3/später.

### P1 – Robustheit: Selektor-Parser (Playwright-Syntax in JS)
**Datei:** `src/browser.rs` (`JS_SEL_PRELUDE`)  
**Problem:** Viele Selektoren nutzen Playwright-Text-Syntax (`text=...`, `:has-text(...)`). Handgeschriebener JS-Interpreter ist brüchig.  
**Fix (Qwen):** Normalisierung in Rust → reines CSS.  
**→ Grok:** Problem real; naive `[data-text*=…]`-Normalisierung **nicht brauchbar**. Selector-Hardening statt Magic-Normalize.

### P1 – Testing: Keine Integration/E2E-Tests
**Fehlt:** Top-Level-`tests/` Verzeichnis.  
**Risiko:** Selektor-Drift, Browser-Updates, Timing-Änderungen werden nicht gecatcht.  
**→ Grok:** Bestätigt; zuerst Unit-Executor grün, dann gestaffelte Integration (Mock → gated headless).

### P2 – Race Condition: `RunStore::save` TOCTOU
**Datei:** `src/run_store.rs`  
**Problem:** `load_existing_meta()` → `validate_transition()` → `save_internal()` – Fenster bei Multi-Process.  
**→ Grok:** Real, Impact gering solange Single-Process. Optional `fs2` / etag.

### P2 – Config: Hardcoded Constants
**Datei (Qwen):** `src/controller.rs`  
**Constants:** `STABILITY_SECONDS`, `LOOP_GUARD_*`, `RESUME_TRANSCRIPT_CHAR_BUDGET`  
**→ Grok:** `LOOP_GUARD_WARN_COUNT` / `LOOP_GUARD_ABORT_COUNT` **bereits in `config.rs`**. Rest (`RESUME_*`, `STABILITY_SECONDS` in `browser.rs`) noch nachziehen + Env-Override.

### P3 – Redundanz: Browser-Pool + WebView-Runtime
**Dateien:** `src/browser_pool.rs`, `src/webview_runtime.rs`, `src/page_driver.rs`  
**Problem (Qwen):** Zwei parallele Browser-Architekturen, Logik dupliziert.  
**→ Grok:** **Überzeichnet.** `BrowserPool` ist bereits Wrapper um `WebViewRuntime` (Shared = 1 Runtime, N Tabs). Kein Big-Bang-Merge; eher Oberflächen/Pfade straffen.

### P3 – Stabilität: `unwrap`/`expect` in Non-Test-Code
**Befund (Qwen):** ~15 Stellen.  
**→ Grok:** Zahl hängt stark vom Filter ab; **opportunistisch** bei angefassten Dateien ersetzen, kein reiner Audit-PR.

---

## 🔍 Gegenprüfung (Grok) — Fakten-Check

| Claim (Qwen) | Reality (2026-07-16) | Urteil |
|--------------|----------------------|--------|
| Shell: Brain → `execute` ohne Filter | Bestätigt: `controller.rs` `ActionType::Shell` → `self.executor.execute(&action.command, …)` | **korrekt** |
| `controller.rs` ~1150 Zeilen God-Module | ~**1025** LOC; `loop_guard.rs` schon extrahiert | Richtung ok, Größe leicht übertrieben |
| Größtes Architektur-Risiko = controller | **`doctor.rs` ~1308**, **`browser.rs` ~1270** LOC größer | **untergewichtet** |
| `LOOP_GUARD_*` hardcoded in controller | Import aus **`config.rs`** | Proposal teilweise **veraltet** |
| Kein Integration-`tests/` | Kein Top-Level-`tests/` | **korrekt** |
| 168 Unit-Tests grün | `cargo test --lib`: **166 gelistet, 159 pass / 7 fail** (alle `executor::tests::*`) | **Baseline falsch/veraltet** |
| Browser-Pool vs WebView redundant | Pool **nutzt** `WebViewRuntime` | **überzeichnet** |
| TOCTOU `RunStore::save` | load → validate → tmp+rename; kein Lock | **real, niedrig priorisieren** |
| Allowlist als Default-Fix | Agent braucht generische Shell (by Design, Single-User) | **Allowlist-only = No-Go** |

### Aktuelle Test-Baseline (Grok-Nachlauf)

```
cargo test --lib
→ 159 passed; 7 failed (executor::tests::*)
```

Bekannte rote Tests (Executor/Shell-Session):

- `test_simple_command`
- `test_nonzero_exit`
- `test_cwd_persists_across_commands`
- `test_fake_marker_does_not_complete_early`
- `test_stale_lastexitcode_not_inherited`
- `test_stderr_capture`
- `test_timeout_no_leak_to_next_action`

**Implikation:** Keine großen Refactors (Controller-Split, async) bevor Executor-Suite wieder grün ist.

### Was der Original-Review nicht abdeckt

1. Rote Executor-Tests als **aktueller Blocker**
2. Größe von `doctor.rs` / `browser.rs`
3. Produktkontext: Shell ist **gewollt** (Local Agent) — Security = Policy/Audit, nicht „kein Shell“
4. Provider-/Relay-Stabilität (`PROVIDER_STATUS.md`) oft höherer User-Impact als Architektur-Purity
5. Uncommitted Work (`repl.rs` /model /goal /swarm, `webview_runtime.rs`, Cargo.toml) vor Controller-Refactor committen/parken

---

## 🔒 Security Detail

| Check | Status | Details |
|-------|--------|---------|
| Command Injection | ⚠️ **Medium–High** (Kontext: Local Agent) | Brain-Output → persistente Shell. Kein Remote-RCE-Modell, aber Prompt-Injection über Seite/Tool-Output möglich. **Policy-Layer nötig, kein reines Allowlist.** |
| Path Traversal | ✅ Ok | `run_id` kontrolliert, `safe_id` alphanum-only |
| Profile Isolation | ⚠️ Medium | Shared-Browser teilt Cookies/Storage zwischen Brains |
| Secret Leakage | ✅ Ok | Keine API-Keys im Code; Profile user-scoped |
| Input Parsing | ✅ Stark | `protocol.rs`: strikte Validierung, Windows-Path-Repair nur für `message` type |
| CDP Exposure | ℹ️ Info | Port 9222 lokal erreichbar (`WEBAGENT_SHARED_DEBUG_PORT` überschreibbar) |

**Risikomodell (Grok):** Single-User-Desktop. „Shell offen“ ist Feature. Schutzziele: (1) versehentlich destruktive Commands, (2) Prompt-Injection, (3) Auditierbarkeit. Nicht: Multi-Tenant-Sandbox.

---

## 📊 Metriken

| Metrik | Qwen (Review-Zeit) | Grok (Nachprüfung) |
|--------|--------------------|--------------------|
| LOC (src/) | ~12.000 | ~12.000 (plausibel) |
| Unit-Tests gelistet | 168 | **166** (`cargo test --lib -- --list`) |
| Unit-Tests grün | 168/168 | **159 pass / 7 fail** |
| Integration-Tests | 0 | 0 |
| Test-Dauer | ~20s | ~51s (bei Failures inkl. Timeouts) |
| Unsafe Blocks | 0 | 0 (unverändert angenommen) |
| Größte Module (LOC) | controller ~1150 | doctor ~1308, browser ~1270, controller ~1025, repl ~734 |

---

## 📁 Dateien für Folgearbeit

| Datei | Relevanz | Prio (revidiert) |
|-------|----------|------------------|
| `src/executor.rs` | Shell-Session + **rote Tests** + Policy-Hook | **P0** |
| `src/controller.rs` | Shell-Aufruf, Split-Kandidat | P0 Policy-Hook / P1 Split |
| `src/browser.rs` | Selektor-Prelude, Größe, Provider | **P1** |
| `src/doctor.rs` | Größtes Modul, wenig im Original-Review | P2 Refactor-Kandidat |
| `src/run_store.rs` | TOCTOU (optional Lock) | P2 |
| `src/protocol.rs` | Parsing-Sicherheit, gut getestet | Referenz |
| `src/config.rs` | Env-Overrides, restliche Constants | P2 Quick Wins |
| `src/browser_pool.rs` + `webview_runtime.rs` | Bereits gekoppelt; kein Merge-Epic | P3 straffen |
| `CLAUDE_PROPOSALS.md` | Umsetzungsvorschläge (revidiert) | Roadmap |
| `CODE_REVIEW.md` | Diese Datei | — |

---

## 🎯 Empfohlene nächste Schritte (revidiert, Grok)

1. **P0 Blocker:** Executor-Unit-Tests wieder grün (`executor::tests::*`)
2. **P0 Security:** Shell-**Policy** (Denylist + Audit + optional `WEBAGENT_SHELL_STRICT`) + optionale typed `webagent:`-Tools — **kein** Prefix-Allowlist-Default
3. **P1 Robustheit:** Selector-Hardening (JSON + Prelude + Tests pro Provider)
4. **P1 Architecture:** `controller.rs` dünner (`action_executor` / `resume` / observation) — **nach** 1–2
5. **P1 Testing:** gestaffelte Integration (Mock offline; headless smoke gated)
6. **P2:** restliche Config-Env, optional RunStore-Lock, unwrap opportunistisch
7. **P3/später:** Full-async, große Browser-Konsolidierung nur bei Multi-Run-Bedarf

Details und Go/No-Go: siehe **`CLAUDE_PROPOSALS.md`**.

---

## Go / No-Go (Kurz)

| Thema | Entscheidung |
|-------|--------------|
| Shell Allowlist-only | **No-Go** |
| Shell Policy + optional Dispatcher | **Go** |
| Controller-Split | **Go later** (nach Tests + Policy) |
| Full async / tokio-Umbau | **No-Go now** |
| Selektor Magic-Normalize wie skizziert | **No-Go**; Hardening **Go** |
| Integration tests | **Go** (gestaffelt) |
| RunStore file lock | **Go optional** |
| Config-Constants Rest | **Go** (klein; LOOP_GUARD schon da) |
| Pool/WebView Big-Bang-Merge | **No-Go**; straffen **Go** |
| unwrap-Audit-PR | **Go opportunistic** |

---

*Original-Review: Qwen Code (Code-Analyse + damaliges `cargo test`).  
Gegenprüfung: Grok — Code-Pfad-Verifikation, Modulgrößen, `cargo test --lib` (159/166), Architektur-Abgleich Pool/Runtime/config. Keine vollständige Laufzeit-E2E der Provider.*
