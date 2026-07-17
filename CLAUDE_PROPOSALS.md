# Vorschläge – WebAgent-RS Verbesserungen
*Erstellt: 2026-07-16 | Von: Qwen Code Review*  
*Revidiert & ergänzt: 2026-07-16 | Grok (xAI) — siehe auch `CODE_REVIEW.md` Gegenprüfung*

Konkrete, umsetzbare Vorschläge. Original-Ideen von Qwen bleiben sichtbar;  
**Entscheidung / angepasste Umsetzung** stehen jeweils unter „Grok“.

**Voraussetzung für größere Refactors:** `cargo test --lib` grün
(Stand Gegenprüfung 2026-07-16: **159 pass / 7 fail**, alles `executor::tests::*`).

**Stand 2026-07-17:** `cargo test --no-default-features` → **194 pass / 0 fail** (lib). Die ehemals roten `executor::tests::*`
sind grün; Baseline somit freigegeben für P0/P1-Refactors.

---

## Status-Update 2026-07-17 — Swarm-Profil-Isolation & einheitliches `login-all` (DONE)

Umgesetzt durch Qwen (Track A–E, AUTONOMIE-MANDAT) + Grok-Review. Build + Tests grün (s. o.).

- **Profil-Isolation im Swarm:** `config::prepare_swarm_profile` legt pro Teilnehmer eine isolierte Laufzeit-Kopie an
  (`profiles/swarm/<run>_<brain>/`), bevorzugt aus `profiles/reference/<brain>`, sonst `profiles/<brain>`, sonst leer.
  `copy_dir_all` überspringt Lock-/Cookie-Artefakte (SingletonLock, SingletonCookie, SingletonSocket, `*.lock`, `lockfile`)
  → kein `SingletonLock`-Konflikt bei 8 parallelen Brains. `cleanup_swarm_profiles` räumt nach dem Lauf auf (auch Fehlerpfad).
- **Eigene Runtime bei Override:** `browser::start()` nutzt den Shared-Pool **nur** ohne `profile_override`; mit Override
  (Swarm-Kopie) startet eine eigene `WebViewRuntime` → Isolation wirksam (Grok MUST-FIX, in `browser.rs` integriert).
- **Einheitliches Login:** `webagent login-all [--timeout] [--force] [--parallel N]` (N auf 3 gedeckelt) + REPL `/login-all`
  loggen alle Brains **sequenziell** ein und schreiben canonical nach `profiles/<brain>`. Bereits eingeloggte Brains werden
  via `is_logged_in_quick` übersprungen (außer `--force`).
- **Tests:** `config::tests::test_prepare_swarm_profile_fallback_and_cleanup`, `test_swarm_and_reference_paths` (grün).
- **Docs:** `README.md` Profil-Tabelle + login-all + swarm aktualisiert.

Erledigt (Q1–Q3, Grok REVIEW Approve 2026-07-17): login-all optional `reference/` via Env `WEBAGENT_LOGIN_TO_REFERENCE=1`
(`maybe_copy_to_reference` nach erfolgreichem Login); `is_logged_in_quick` headless (`probe.start(true)`);
`copy_dir_all` loggt WARN, wenn 0 von N Dateien kopiert wurden. Build/Test grün (195/0).

---

## P0 – Blocker: Executor-Unit-Tests wieder grün

**Status:** Neu in der revidierten Roadmap (Original-Review kannte das nicht).

**Problem:** Sieben Shell-Session-Tests failen (Marker, Exit-Code, CWD-Persistenz, stderr, Timeout-Isolation). Ohne grüne Baseline sind Controller-Split und Security-Hooks riskant.

**Betroffen:** `src/executor.rs` (persistente PowerShell/sh-Session, Nonce-Marker, Wrapping).

**Vorgehen:**
1. `cargo test --lib executor::` isoliert laufen lassen, Failures einzeln fixen
2. Fokus: Marker-Parsing, `LASTEXITCODE`/exit-Code-Propagation, Timeout killt nicht die Session für den nächsten Command
3. Keine Feature-Arbeit parallel in `executor.rs` bis grün

**Aufwand:** 2–6 h (je nach Root Cause: Env/Shell-Version vs. Logik-Regression)  
**Impact:** Regressionsschutz, Freigabe für P0/P1 Refactors

---

## P0 – Security: Shell Policy (statt Allowlist-only)

### Original (Qwen)

**Risiko:** `execute_actions_serial` führt `action.command` direkt aus. Brain-JSON → Shell.

**Optionen Qwen:**
1. Prefix-Allowlist (`Get-`, `ls`, `cat`, …)
2. Dispatcher `webagent:…` (bevorzugt)

### Grok — Entscheidung

| Option | Entscheidung |
|--------|--------------|
| Prefix-Allowlist als Default | **No-Go** — Agent wird unbrauchbar; Shell ist by Design |
| Typed Dispatcher `webagent:read/write/…` | **Go als Ergänzung** (häufige sichere Ops) |
| Policy-Layer | **Go als Default-Schutz** |

**Risikomodell:** Single-User-Local-Agent. Kein Multi-Tenant. Schutz vor Prompt-Injection und versehentlich destruktiven Commands, nicht „kein Shell“.

**Umsetzung (empfohlen):**

```text
controller (ActionType::Shell)
  → shell_policy::evaluate(command) -> Allow | Deny(reason) | Confirm(reason)
  → executor.execute  (nur Allow / Confirm-ok)
  → audit log (optional file/event)
```

1. **Denylist / Heuristik** (case-insensitive, grobe Muster), z. B.:
   - `Remove-Item -Recurse`, `rm -rf /`, `format `, `mkfs`, Registry-Wipe-Pfade
   - Fork-Bombs / massives Download+Exec (heuristisch)
2. **Env-Schalter:**
   - `WEBAGENT_SHELL_STRICT=1` — nur Denylist + optional nur `webagent:`-Tools + whitelisted Prefixes
   - Default: Denylist + Audit, sonst freier Shell (heutiges Verhalten abzgl. Deny)
3. **Optional typed tools** (Dispatcher):
   - `webagent:read path`, `webagent:write path`, `webagent:list path`
   - Shell bleibt `type: shell` für den Rest
4. **Hook-Punkt:** vor `self.executor.execute` in `controller.rs` (~Shell-Zweig), Logik in neuem `shell_policy.rs` (testbar)

**Nicht tun:** Harte Allowlist `["Get-", "ls", "echo"]` als einziges Modell.

**Aufwand:** ~3–5 h Policy + Tests; Dispatcher-Tools extra 0,5–1 Tag  
**Impact:** Security ohne Feature-Kastration

---

## P0/P1 – Architecture: `controller.rs` aufteilen

### Original (Qwen) — P0, 4 Module

| Modul | Verantwortung |
|-------|---------------|
| `run_loop.rs` | Kern-Loop, `handle_response`, Phase 1/2 |
| `resume.rs` | Resume/Recovery, Conversation-Restore |
| `action_executor.rs` | `execute_actions_serial`, Loop-Guard, Completion |
| `observation.rs` | `bounded_observation`, `track_observation_bytes` |

### Grok — Entscheidung

- **Go later** (Priorität nach Executor-grün + Shell-Policy)
- Größe eher **~1025 LOC**, nicht 1150; `loop_guard.rs` existiert bereits
- **Größere God-Files zusätzlich:** `doctor.rs` (~1308), `browser.rs` (~1270) — nicht ignorieren
- Pragmatischer Schnitt:
  1. `action_executor.rs` (+ Observation eng gekoppelt oder `observation.rs`)
  2. `resume.rs`
  3. Loop bleibt dünn in `controller.rs`
- Vor Split: uncommitted REPL-Arbeit (`/model`, `/goal`, `/swarm`) committen oder parken

**Aufwand:** 1–2 Tage  
**Impact:** Maintainability, testbarere Action-Pfade

---

## P3 (war P1) – Concurrency: Blocking → Full Async

### Original (Qwen) — P1, ~4 h

`tokio::time::sleep`, `spawn_blocking` um `evaluate`/`navigate`.

### Grok — Entscheidung

**No-Go now / P3 später.**

- Agent-Loop ist **sequentiell**; parallele Runs sind kein aktuelles Kernziel
- Full-async + WebView ist **Tage** + Regressionen, nicht 4 h
- Quick Win stattdessen: Sleeps konfigurierbar / dokumentiert lassen; kein Runtime-Umbau

**Wann doch:** Multi-Run, UI-Non-Blocking, oder explizites Parallel-Ziel.

---

## P1 – Robustheit: Selector-Hardening (nicht Magic-Normalize)

### Original (Qwen)

Rust-`normalize_selector` → CSS an `querySelectorAll`; Beispiel mit `[data-text*=…]` für `text=`.

### Grok — Entscheidung

**Problem real, Sketch No-Go.** `[data-text*=…]` ersetzt kein Playwright-`text=`.

**Go — Selector-Hardening:**

1. Inventory: welche `text=` / `:has-text` in `selectors/*.json` (und ggf. eingebettet) noch greifen
2. Wo möglich: stabile CSS / roles / testids in den JSON-Dateien
3. `JS_SEL_PRELUDE` nur gezielt erweitern oder mit Unit-Tests absichern (pro Form: `text=`, `text=/re/i`, `:has-text`)
4. Keine Big-Bang-Ersetzung ohne Provider-Smoke

**Aufwand:** 1–2 Tage  
**Impact:** Weniger flaky Provider-UI

---

## P1 – Testing: Integration gestaffelt

### Original (Qwen)

`tests/integration_run.rs`, headless, 1 Brain „Sag Hallo“.

### Grok — Entscheidung

**Go, gestaffelt:**

| Stufe | Inhalt | CI |
|-------|--------|-----|
| A | Bestehende Unit + Executor grün | immer |
| B | Offline-Integration mit `MockPageDriver` / DummyBrain (Loop, Resume, Policy) | immer |
| C | Headless 1-Provider-Smoke | **gated** (`WEBAGENT_E2E=1`, Login/Secrets) |

Nicht sofort „echtes Brain in CI“ ohne Auth-Story.

**Aufwand:** B = 0,5–1 Tag; C = 1+ Tag + Infra  
**Impact:** Catcht Controller/Protocol-Regressionen (B) und Selektor-Drift (C)

---

## P2 – Race: `RunStore::save` TOCTOU

### Original (Qwen)

`fs2` exclusive lock um save.

### Grok — Entscheidung

**Go optional.** Atomic rename ist schon da; relevant erst bei Multi-Process.

Wenn implementieren: Lock-Datei pro `run_id` oder `fs2` auf meta; Tests mit zwei parallelen Saves.

**Aufwand:** ~2 h  
**Impact:** Korrektheit bei parallelen Prozessen

---

## P2 – Config: restliche Constants + Env

### Original (Qwen)

`LOOP_GUARD_*`, `STABILITY_SECONDS`, `RESUME_TRANSCRIPT_CHAR_BUDGET` → config + Env.

### Grok — Entscheidung

**Partial done:**

- ✅ `LOOP_GUARD_WARN_COUNT` / `LOOP_GUARD_ABORT_COUNT` bereits in `config.rs`
- ⬜ Env-Override z. B. `WEBAGENT_LOOP_GUARD_WARN` / `_ABORT` noch nachrüsten (optional)
- ⬜ `RESUME_TRANSCRIPT_CHAR_BUDGET`, `MEMORY_CONTEXT_LIMIT`, Heartbeat-Interval → `config.rs`
- ⬜ `STABILITY_SECONDS` liegt in **`browser.rs`**, nicht controller → dort + Env

**Aufwand:** 1–2 h  
**Impact:** Runtime-Tuning ohne Recompile

---

## P3 – Browser-Pool / WebView straffen (kein Big-Bang-Merge)

### Original (Qwen)

Zwei Architekturen konsolidieren; Feature-Flag; Pool als Wrapper.

### Grok — Entscheidung

**Rewrite des Findings:** Pool **ist bereits** Wrapper um `WebViewRuntime` (`browser_pool.rs` hält `Option<WebViewRuntime>`).

**Go (klein):**
- Doppelte Start/Stop-Pfade in `browser.rs` (shared vs own) dokumentieren/straffen
- Öffentliche API vereinheitlichen wo Call-Sites divergieren
- Kein 2–3-Tage-Merge-Epic

**Aufwand:** 0,5–1 Tag  
**Impact:** Weniger Verwirrung, kleinere Diffs später

---

## P3 – `unwrap`/`expect` opportunistisch

### Original (Qwen)

~15 Stellen auditieren.

### Grok — Entscheidung

**Go opportunistic** — bei Dateien, die sowieso angefasst werden (`?` / `map_err` / Defaults). Kein reiner mechanischer Audit-PR. Filter Tests sauber ausnehmen.

---

## Zusätzlich (Grok): große Module jenseits Controller

| Datei | ~LOC | Vorschlag |
|-------|------|-----------|
| `doctor.rs` | ~1308 | Später splitten (checks vs. reporting) wenn angefasst |
| `browser.rs` | ~1270 | Selector-Hardening + ggf. Provider-Sends extrahieren |
| `repl.rs` | ~734 | Erst committen (uncommitted Features), dann ruhig lassen |

---

## Zusammenfassung — revidierte Roadmap (Code-Review-Strang)

| # | Prio | Task | Aufwand | Entscheidung |
|---|------|------|---------|--------------|
| 0 | **P0** | Executor-Tests grün | 2–6 h | **Go now** |
| 1 | **P0** | Shell-Policy (Deny + Audit + optional Strict) | 3–5 h | **Go** |
| 1b | **P0** | Optional `webagent:` read/write/list | 0,5–1 d | **Go optional** |
| 2 | **P1** | Selector-Hardening + Tests | 1–2 d | **Go** |
| 3 | **P1** | Controller dünner schneiden | 1–2 d | **Go after 0–1** |
| 4 | **P1** | Integration Stufe B (Mock) | 0,5–1 d | **Go** |
| 4b | **P1** | E2E headless gated | 1+ d | **Go later** |
| 5 | **P2** | Config-Env Rest | 1–2 h | **Go** |
| 6 | **P2** | RunStore Lock | ~2 h | **Go optional** |
| 7 | **P3** | Pool/WebView Pfade straffen | 0,5–1 d | **Go small** |
| 8 | **P3** | unwrap opportunistisch | laufend | **Go** |
| 9 | **P3** | Full async | Tage | **No-Go now** |
| — | — | Prefix-Allowlist-only | — | **No-Go** |
| — | — | Magic `normalize_selector` wie Qwen-Sketch | — | **No-Go** |

**Empfohlene Reihenfolge (Review-Strang):**  
`Executor grün` → `Shell-Policy` → `Selector-Hardening` → `Controller-Split` → `Integration B` → `Config/Lock` → Rest.

Weitere Produkt-/Council-Ideen: Abschnitt **„Ideen-Backlog (Repo-weit)“** unten.

---

## Uncommitted Work (Hinweis bleibt gültig)

REPL-Änderungen (`/model`, `/goal`, `/whoami`, `/swarm` u. a.) und offene Diffs in `webview_runtime.rs` / `Cargo.toml` **erst committen oder parken**, bevor am Controller geschnitten wird — sonst Merge-Konflikte.

---

# Ideen-Backlog (Repo-weit)

*Ergänzung 2026-07-16 | Grok — Synthese aus Genius Council, Provider-Status,
Paritätsplan, Archive. Gefiltert auf das, was für **webagent-rs heute** noch
relevant ist. Kein Implementierungsauftrag; Entscheidungsgrundlage.*

## Quellen

| Quelle | Inhalt |
|--------|--------|
| Genius Council 2026-07-06 (`data/council/final_top10_*.json`, Evaluation) | 70 Ideen → Top-10 Safety/Robustheit |
| `GENIUS_COUNCIL_EVALUATION.txt` (Codex-Gegenreview) | Pragmatischere Umsetzungsreihenfolge |
| `PROVIDER_STATUS.md` | Live 8/8, Quoten, tote Config-Keys, DLL-Fußangel |
| `MERGE_AND_PARITY.md` | Rust-only-Ziel, fehlende Multi-Agent-Schicht, REPL |
| `docs/GENIUS_COUNCIL_CONCEPT.md` | Council bewusst **DEFERRED** |
| Abschnitte oben (Qwen/Grok Review) | Security, Tests, Architektur |

## Schon weitgehend erledigt (nicht nochmal erfinden)

- Loop-Guard / wiederholte Read-Fingerprints (`loop_guard.rs`)
- Resume + `completed_actions` (Idempotenz-Ansatz)
- Shared Browser / Tab-Pool
- Relay mit echten Antworten, Send-Retry, bestätigtes Composer-Fill
- Playwright-`text=`/`has-text` im JS-Prelude (nicht mehr stumm tot)
- 8/8 Provider headless (mit Quota-Einschränkungen)
- Doctor / Watchdog / brains-health / oobe

---

## A — Sofort / Stabilität

| ID | Idee | Herkunft | Warum | Entscheidung |
|----|------|----------|-------|--------------|
| **A1** | Executor-Unit-Tests wieder grün | Review-Gegenprüfung | Ohne Baseline kein sicherer Refactor | **Go now** (siehe P0 Blocker oben) |
| **A2** | Shell-Policy (Deny + Audit + optional Strict), kein Allowlist-only | Council #2/#8 + Review | Shell by Design; Prompt-Injection real | **Go** (siehe P0 Security oben) |
| **A3** | Release-Binary: `WebView2Loader.dll` neben `webagent.exe` kopieren (Post-Build / Install-Doku) | `PROVIDER_STATUS` | Sonst stiller Crash `0xC0000135` nach `cargo clean` | **Go** |
| **A4** | Smoke-Script strikt: nur echte Antwort, nie `exit 0` allein | `PROVIDER_STATUS` | Früher falsche „5/8 PASS“-Illusion | **Go** (`delivery/provider_webview_smoke.ps1` prüfen) |
| **A5** | Tote Config-Keys verdrahten oder entfernen | `PROVIDER_STATUS` | `response_preference_prompt` (gemini), `dialog_dismiss_button` (mistral) ungelesen | **Go** |
| **A6** | Selector-Hardening + Drift-Probes pro Provider | Council #8, Gemini-Story | UI driftet; Prelude hilft, stabile CSS besser | **Go** (siehe P1 Selector oben) |

### A3 — Packaging-Skizze

```text
cargo build --release
→ copy WebView2Loader.dll next to target/release/webagent.exe
→ document in README; optional build.rs / install script
```

### A4 — Smoke-Kriterium

- PASS nur wenn stdout/stderr die erwartete Antwort enthält (z. B. `OK`)
- `exit 0` allein zählt **nicht**
- Optional: pro Brain Latenz + Fail-Reason persistieren

### A5 — Tote Keys

- Entweder in `browser`/`relay` einlesen (Popup/Preference dismiss) **oder**
- aus `selectors/*.json` entfernen + in PROVIDER_STATUS als „removed dead config“ notieren

---

## B — Robustheit Autonomie

| ID | Idee | Herkunft | Status heute | Entscheidung |
|----|------|----------|--------------|--------------|
| **B1** | Circuit Breaker pro Provider (Zeitbudget, Retries, Isolate) | Council #1 (stärkster Konsens) | Loop-Guard nur Read-Loops; **kein Brain-Breaker** | **Go** (P1 nach A1–A2) |
| **B2** | Live-Canary: Login → Composer → Send → Stream-Ende → Extract + Latenzen | Council #9 / Codex | `relay` ist Messung; **kein periodischer Canary + Historie** | **Go** |
| **B3** | Protocol-Repair-Schleife bei kaputtem JSON | Council #10 / gemini-09 | Parser streng; **ein Repair-Turn fehlt** | **Go** |
| **B4** | Explizite Brain-Zustandsmaschine | Codex #1 | Verstreut in diagnose/relay | **Go later** (P2) |
| **B5** | Act-Ledger feiner (geplant → committed, „darf ich wiederholen?“) | Council #5/#7 | Resume + completed_actions teilweise | **Go later** |
| **B6** | Rate-Limit / Quota-Awareness (qwen Tageslimit, claude session) | Live-Messung | Teilweise claude-spezifisch; qwen ehrlich als Text | **Go** (mit B1) |

### B1 — Circuit Breaker (Skizze)

```text
pro Brain:
  consecutive_failures, open_until, budget_seconds
  on timeout/rate_limit/send_fail → failure++
  on success → reset
  if open → skip brain, degrade message, don't block whole chain forever
```

Env-Ideen: `WEBAGENT_BRAIN_BUDGET_S`, `WEBAGENT_BRAIN_MAX_RETRIES`, `WEBAGENT_BREAKER_COOLDOWN_S`.

### B2 — Canary

- CLI oder Script: alle 8 Brains `relay "OK"` headless, JSONL unter `data/canary/`
- Optional Task-Scheduler / watchdog-Hook
- Metriken: latency, pass/fail, reason (`rate_limit`, `timeout_no_text`, …)

### B3 — Protocol Repair

- Bei Parse-Fail: **ein** Follow-up-Prompt „nur gültiges webagent/1 JSON“
- Zweiter Fail → run status `protocol_error`, kein Endlos-Retry
- Unit-Tests mit absichtlich kaputten Brain-Antworten

### B6 — Quota

- Provider-Antworten mit Limit-Text → Status `rate_limited` / `quota_exhausted` (brain-spezifisch, nicht alles claude)
- Adaptive Loadouts: Brain auslassen statt endlos retryen
- Anbindung an `data/brains_performance.json` (existiert, wenig Rust-Verdrahtung)

---

## C — Produkt / Parität

| ID | Idee | Herkunft | Entscheidung |
|----|------|----------|--------------|
| **C1** | REPL: Session über Turns offen halten — Doku vs. Code klären/festziehen | `MERGE_AND_PARITY` F5 | **Go** (klären + ggf. fix) |
| **C2** | Multi-Agent **Advisory-Council** (nur Text, kein Shell aus Council-Output) | Genius-Konzept | **Deferred** bis Policy+Breaker; dann MVP advisory-only |
| **C3** | Leistungsindex → Loadout-Gewichtung | Council / `brains_performance.json` | **Go later** (wenn Multi-Brain-Runs) |
| **C4** | Semantische Deduplizierung bei Multi-Ideen | Council #10 | Nur mit C2 |
| **C5** | Python-Baum archivieren / ein Repo | Merge Phase M | **Go later** nach stabiler Rust-Nutzung |
| **C6** | Linux-CI + Unix-Executor hart grün | Merge P1 | **Go** (CI pflegen) |

### C2 — Advisory-Council (wenn reaktiviert)

- Schicht **über** Controller, ersetzt ihn nicht
- Phasen: Vorschläge → (optional) Review → Moderator-Synthese → **Text-Output only**
- Executive-Modus (Council → Shell) **erst** nach A2 + B1
- Konzept: `docs/GENIUS_COUNCIL_CONCEPT.md` (STATUS: DEFERRED)

---

## D — Security tiefer (später, teuer)

| ID | Idee | Herkunft | Entscheidung |
|----|------|----------|--------------|
| **D1** | Risikoklassen read/write/network/destructive + User-Confirm | Council Policy-Cluster | **Go later** (Ausbau von A2) |
| **D2** | PowerShell-AST-Prüfung | gemini-01 | **Optional later** (Windows-spezifisch) |
| **D3** | Windows-Sandbox / Docker für Shell | kimi-01 | **No-Go v1** (Infra) |
| **D4** | DPAPI / abgestufte Profilablage | Council #9 | **Go later** (geteilte Maschinen) |
| **D5** | Typed tools `webagent:read/write/list` | Review | **Go optional** (siehe 1b oben) |

---

## E — Architektur / Maintainability

| ID | Idee | Herkunft | Entscheidung |
|----|------|----------|--------------|
| **E1** | Controller dünner schneiden | Review | **Go after** A1–A2 |
| **E2** | `browser.rs` / `doctor.rs` entflechten | Gegenprüfung | **Go opportunistic** |
| **E3** | Integration-Tests gestaffelt | Review | **Go** (siehe P1 Testing) |
| **E4** | Full-async Runtime | Review | **No-Go now** |

---

## Genius-Council Top-10 → Status (2026-07-16)

| Rank | Idee | Status | Nächste Aktion |
|------|------|--------|----------------|
| 1 | Watchdog / Circuit Breaker | Teilweise (watchdog CLI, loop_guard); **kein Provider-Breaker** | **B1** |
| 2–4 | Sandbox / Policy / Isolation | Offen | **A2**, später D1–D3 |
| 5+7 | Transaktionslog / idempotentes Resume | Teilweise (`completed_actions`, resume) | **B5** feiner |
| 6 | Least-Privilege-Profile | Shared vs. per-brain; kein DPAPI | **D4** later |
| 8 | Action-Policy-Engine | Offen | **A2** / **D1** |
| 9 | Health / Self-Test | relay/diagnose da; kein Canary-Scheduler | **B2** |
| 10 | JSON-Schema + Repair | Parser streng; Repair-Loop fehlt | **B3** |

---

## Gesamtreihenfolge (Review + Backlog)

```text
A1 Executor grün
 → A2 Shell-Policy
 → A3 Packaging DLL
 → A4 Smoke strikt
 → B1+B6 Circuit Breaker + Quota
 → A5+A6 Selektor-Hygiene / tote Keys
 → B2 Canary
 → B3 Protocol-Repair
 → C1 REPL-Klarheit
 → E1 Controller-Split + E3 Integration B
 → C2 Advisory-Council nur wenn gewünscht
 → D*/C3–C5 later
```

**Nicht priorisieren (jetzt):** Docker/Sandbox-first, Full Genius-Executive, Full-async, neue Provider ohne Quota-Ops.

---

## IDs für Claude (kurze Spec-Anker)

Wenn ein Item umgesetzt wird, im Commit/PR die ID nennen (`A2`, `B1`, …) und gegen diese Datei abhaken (Status-Spalte oder Checkbox im PR).

| Priorität | IDs |
|-----------|-----|
| Diese Woche | A1, A2, A3, A4 |
| Nächster Sprint | A5, A6, B1, B6, B2, B3 |
| Danach | C1, E1, E3, B4, B5 |
| Deferred / later | C2–C5, D1–D5, E2, E4 |

---

*Originalvorschläge: Qwen. Revision + Ideen-Backlog: Grok (Code-Pfade, Testlauf, Genius Council, PROVIDER_STATUS, MERGE_AND_PARITY).*
