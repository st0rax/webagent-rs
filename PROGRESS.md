# PROGRESS — webagent-rs

**Stand:** 2026-07-20 (Autoresearch + Wiki-Memory delegiert gebaut)

## 2026-07-20 (5) — Autoresearch + Wiki-Memory (delegierte Umsetzung)

Beide Karpathy-Features sind drin, Umsetzung delegiert (Claude-Subagents für
den Rust-Kern, webagent-Flotte für Vorarbeiten), vom Orchestrator verifiziert:

- **Autoresearch** (`autoresearch.rs`, Commit 1148547): Modify→Verify→
  Keep/Discard-Schleife nach docs/AUTORESEARCH_PLAN.md. CLI `webagent
  autoresearch --brain --goal --eval …`, REPL `/autoresearch <eval> :: <goal>`.
  Livetest: zai, Wegwerf-Repo, Metrik 10→11→12 via edit-Actions, Commits auf
  autoresearch/-Branch.
- **Wiki-Memory** (`wiki_memory.rs`): Karpathy-LLM-Wiki-Kern nach
  docs/WIKI_MEMORY_PLAN.md — Markdown-Seiten + [[links]] + index.md unter
  data/memory/wiki/, REPL `/wiki [suche|lint]`, Index fließt als Kontextblock
  in autonome Runs (verifiziert im Transcript). slugify/extract_links kamen
  von der webagent-Flotte (qwen, 16/16 Tests).
- **Zwei dabei gefundene Bugs gefixt:** (1) `-`/`_` steckten in der Such-Token-
  Klasse — "deploy" fand "Deploy-Regeln" nicht (fatal bei Kebab-Slugs); Tokens
  werden jetzt zusätzlich an -/_ gesplittet. (2) `AgentController::new()` war
  CWD-abhängig (./data) statt stabiler OS-Ort — run/bot2bot-worker und REPL
  nutzten dadurch VERSCHIEDENE data/-Verzeichnisse (Runs, Memory, Wiki).
- **Offene Flotten-Funde** (Dogfooding, noch zu fixen): kimi-Run hing 30+ min
  ohne Start-Timeout; gemini lieferte False-Done (cycles=1, kein Artefakt,
  fehlendes Run-Verzeichnis) — Pfad (c) "finish ohne Aktionen = verdächtig"
  aus dem Phantom-Resume-Komplex bleibt offen. Einmalig beobachtet: leere
  index.md nach Seeding (nicht reproduzierbar, beobachten).
- Tests: 267 → **321**, clippy -D warnings clean.

## 2026-07-20 (4) — Coding-Agent Phase 1–3

Ziel (Storax): webagent soll als Coding-Agent durchgehen. Drei Lücken geschlossen:

**Phase 1 — edit/write-Actions im webagent/1-Protokoll** (`protocol.rs`,
`file_actions.rs` neu, `controller.rs`, `prompts.rs`):
- `edit`: path + old_string/new_string; Anker muss exakt einmal matchen.
  Fehler-Observations sind brain-lesbar (nicht gefunden / mehrdeutig mit
  Trefferzahl); CRLF↔LF-Toleranz (Anker+Ersatz werden konsistent umkodiert);
  new_string="" = Löschung. Ausführung nativ in Rust, nicht über PowerShell.
- `write`: path + content, legt NEUE Dateien an (inkl. Parent-Dirs); existierende
  Datei → Fehler mit Verweis auf edit (kein stilles Überschreiben).
- Parse-Validierung: edit braucht path+old_string, old!=new; write braucht
  path+content-Key. edit/write dürfen mit shell in einer Antwort gebatcht werden.
- Prompt-Doku + Regel "für Dateiänderungen IMMER edit/write statt Set-Content".

**Phase 2 — Repo-Kontext:** Initial-Prompt bekommt einen begrenzten Dateibaum
des Arbeitsverzeichnisses (Tiefe<=3, max 120 Einträge, .git/target/venv/profiles
u.ä. gefiltert; Kill-Switch WEBAGENT_NO_TREE=1) — spart Struktur-Erkundungs-
Roundtrips über den Browser.

**Phase 3 — `/diff` im REPL:** git status --short + git diff --stat des
Arbeitsverzeichnisses nach Aufgaben.

**Dogfooding:** Testfall-Ideen für edit kamen von webagent selbst (zai, run
20260720_150236, 3 Zyklen) — 2 davon fehlten in der Suite (Umlaute,
Anfang/Ende-Ersetzung) und wurden übernommen. Dabei gefundene Bugs/Findings:
- webagent schreibt normale Fortschritts-Zeilen auf stderr → PowerShell-Wrapper
  rendern sie als rote NativeCommandError-Blöcke (sieht nach Crash aus). TODO:
  Fortschritt auf stdout, stderr nur für echte Fehler.
- zai baute für eine simple Markdown-Datei erneut eine fragile einzeilige
  Set-Content-Array-Konstruktion — bestätigt den Bedarf der write-Action.

## 2026-07-20 (2) — /pool + Session-Zusammenfassung beim Beenden

- **`/pool [n]`** (Aliase `/tui`, `/workers`) startet die Worker-Pool-TUI aus dem
  Chat heraus (Default 8 aktiv, headless); `q` in der TUI kehrt in den Chat
  zurück (REPL-Brain wird vorher gestoppt, danach neu gestartet).
- **Session-Zusammenfassung** bei `/exit`/EOF (qwen-code-Vorbild): Dauer,
  Anfragen (Aufgaben ok/Fehler, Chats, Swarms), Plan/Act/Observe-Zyklen,
  benutzte Brains, Token-Schätzung (Zeichen/4 — Web-Chats liefern keine echten
  Token-Zahlen).
- Verifiziert: 251/251 Tests (neue Parser-/Format-Tests), clippy clean, e2e via
  `repl --brain zai --headless`: Task→Datei real erstellt, /chat ok, Summary
  korrekt (2 Anfragen, 2 Zyklen, ≈22/≈38 Tokens).
- ~~Known Issue: `/chat` nach autonomer Aufgabe antwortet im JSON-Format~~ —
  **behoben (2026-07-20, 3):** `display_chat_text()` in `repl.rs` erkennt
  webagent/1-Envelopes (via `protocol::parse` + serde-Fallback für Varianten
  ohne `protocol`-Feld) und zeigt den Klartext der message-Actions. e2e
  verifiziert: Task → `/chat` → Antwort "pong" statt JSON-Envelope.

## 2026-07-20 — `webagent` ohne Parameter startet jetzt die Chat-REPL

Storax-Wunsch: `webagent` ohne Subcommand soll einen Chat öffnen, der auch
Aufgaben entgegennimmt (wie andere Coding-Agenten) — vorher startete die
Worker-Pool-TUI, was beim Testen als "funktioniert nicht" ankam.

- `main.rs`: Default-Dispatch `Tui` → `Repl { brain: "chatgpt", headless: false }`.
  Die REPL konnte beides schon (Plain-Input = autonomer Task via Controller,
  `/chat` = reiner Chat, `/model` zum Brain-Wechsel); es fehlte nur der Default.
- TUI/Worker-Pool unverändert erreichbar: `webagent tui` bzw. `webagent workers`.
- Verifiziert: 250/250 Tests, clippy `--all-targets -D warnings` clean; e2e-Smoke
  via `repl --brain zai --headless` (gleicher Codepfad): Banner ok, `/whoami` ok,
  Task "erstelle repl_smoke.txt" → Datei real erstellt, cycles=2, kein Phantom.

## 2026-07-19 — Testsuite wieder voll lauffähig (beide Blocker von 2026-07-15 weg)

Beide unten dokumentierten "Offene Baustellen an der Testsuite" sind nicht mehr
reproduzierbar und damit erledigt:

1. **`cargo test` linkt wieder.** Die aktive Default-Toolchain ist inzwischen
   `stable-x86_64-pc-windows-msvc` (nicht mehr `-gnu`); das `-lgcc`-Linkproblem
   entfällt damit komplett. Verifiziert: `cargo test --lib` → **250 passed, 0 failed**.
2. **`--all-targets` compiliert.** Der E0451-Fehler (private Felder in
   `WebViewPageDriver`/`PageMessage` aus `browser_pool.rs`-Tests) tritt nicht mehr
   auf. Verifiziert: `cargo test --all-targets --no-run` → baut alle Targets;
   `cargo clippy --all-targets -- -D warnings` → exit 0.

Außerdem: `runtime-workers/webagent.exe` (Flotten-Kopie) war vom 2026-07-17 11:14
und damit **älter als der Phantom-Resume-Fix e62f188 (12:14)** — die Flotte hätte
noch mit dem Bug gearbeitet. Release neu gebaut von HEAD (719e6cc) und nach
`runtime-workers/` kopiert (2026-07-19).

**Stabilisierung dagegen (2026-07-19):**
- `build.rs` bettet Git-Hash+Dirty-Flag ein; `webagent --version` →
  `0.8.1 (719e6cc53+dirty)`. Damit ist jede deployte Kopie ihrem Commit zuordenbar.
- Neuer Deploy-Flow im äußeren Repo: `delivery/deploy_webagent_rs.ps1`
  (Build → Copy nach `runtime-workers/` → Check; verweigert bei laufender Flotte)
  + `delivery/post_deploy_check.ps1` (Binary-Parität, Version-vs-HEAD,
  Python-CLI-Import, Relay-Ping; externe Blocks = WARN). Beide Fehlerklassen
  (stale Binary, CLI-Import-Bruch) per Negativtest verifiziert → FAIL/exit 1.
- **Regel: `runtime-workers/` nie mehr von Hand bekopieren — immer über
  `deploy_webagent_rs.ps1`.**

## DoD Status
- [!] "cargo test --no-default-features (x2) + clippy -D warnings green (160+ tests, 0 fail)"
      — auf dieser Maschine nicht reproduzierbar, siehe Korrektur-Abschnitt unten.
      `cargo clippy --lib` ist grün; `cargo test` linkt nicht.
- [x] comms.rs wired into CLI/main entry + Controller struct (CommsStore constructed and send exercised from dispatch/startup; not dead code)
- [x] bot2bot_root coupling documented as legacy/compat only (internal uses comms; fn kept for watchdog/bridge)
- [x] Providers: 5/8 headless PASS without login (deepseek/kimi/gemini/qwen/zai); chatgpt/claude/mistral honestly "needs manual login" (Cloudflare) with repro in PROVIDER_STATUS.md — no false green
- [x] README/CONVENTIONS/PROVIDER_STATUS current
- [x] Release tag set (after commits)
- [x] PROGRESS.md maintained

## Notes
- comms is webagent-internal (data/comms/), independent of bot2bot repo.
- Pre-existing dead_code in webview_runtime allowed to keep -D warnings green.
- No manual logins performed; status reflects technical headless feasibility.

Next in reihenfolge: bot2bot

## 2026-07-15 — Korrektur einer als "fertig" gemeldeten Runde

Gemeldet war "comms used inside run logic; clean clippy". Nachgeprüft:

- `src/controller.rs` enthielt im Working Tree eine Zeile **Python** in der Rust-Datei:
  `logger_if_any = getattr(globals(), "print", lambda x: None); ...`
  Die Crate compilierte nicht. Ersetzt durch ein `eprintln!` auf `m.id`, womit der
  comms-Send weiterhin echt gelesen wird (kein toter `allow`).
- Commit 22f849b ist **leer** (0 Dateien, 0 Zeilen), obwohl seine Message drei Fixes
  behauptet ("make comms field read", "add comment on CREATE_NO_WINDOW",
  "produce source diff"). Der echte Code lag unkommittet daneben.

Verifiziert nach Fix: `cargo clippy --lib` → clean (2x).

## Offene Baustellen an der Testsuite (vorbestehend, nicht aus dieser Runde)

1. **`cargo test` linkt nicht.** Aktive Toolchain ist `stable-x86_64-pc-windows-gnu`;
   der Linker findet `-lgcc` / `-lgcc_eh` nicht (MinGW-Runtime fehlt). Betrifft jedes
   Test-Target unabhängig von Features. `cargo clippy --lib` läuft, weil nur geprüft
   und nicht gelinkt wird. Die DoD-Zeile oben ("160+ tests, 0 fail") kann hier also
   nicht entstanden sein.

2. **`--all-targets` compiliert nicht (E0451).** Die Unit-Tests in `browser_pool.rs`
   konstruieren `WebViewPageDriver { view_id, page_tx }` mit Feldern, die privat in
   `webview_runtime.rs` liegen, und `PageMessage` ist privat. Nur unter dem
   (default-aktiven) `webview`-Feature sichtbar — mit `--no-default-features` fällt
   der Code weg. Beide Dateien seit 8fc33de unangetastet.
