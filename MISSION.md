# MISSION / ÜBERGABE — webagent-rs

**Wahrheitsquelle für Architektur:** `START_HERE.md`. Diese Datei = aktueller
Arbeitsfokus + Übergabestand. Stand: **2026-07-17**.

> Handoff von **qwen** zu Ende geschrieben (Claude beim Update abgeschmiert).
> `worker_pool` ist geschrieben **und verifiziert** (`cargo test --lib worker_pool::`: 6 passed /
> 0 failed; `cargo clippy --all-targets -- -D warnings`: exit 0),
> uncommitted. `DONE worker-pool` an Claude gemeldet.

---

## STAND 2026-07-17 (Rollenübergabe Claude→qwen, Claude am Sitzungslimit)

**qwen ist jetzt Orchestrator.** In-flight-Antworten landen in `agents/claude/inbox`.

**Erledigt + committed/gepusht:** login-persist (`6a45410`, profiles/data an stabilem
`%LOCALAPPDATA%\webagent` statt Build-Pfad → überlebt Release/Update, idempotente
Migration; 228/228, clippy clean, per Subagent verifiziert). Docs `0cb2c7c`
(BRAIN_ANALYZE_ADD, TUI_DESIGN, research/browser_pool_files).

**In flight:**
- **grok** fixt **Phantom-Resume-Bug** (`controller.rs`): `run`/`bot2bot-worker`
  meldet `status=done` OHNE Ausführung (2 ms `finish`, `conversation_ref=example.test/chat/old`
  Mock leakt in echten Run). Evidenz `data/runs/20260717_093153_b4bee1f4/transcript.jsonl`.
  PLAN-first, prüfen vor Build. Lane `controller.rs`+`bot2bot_worker` = nur grok.
- **Flotte** (deepseek/chatgpt/gemini) läuft als `bot2bot-worker` aus
  `runtime-workers/webagent.exe`-Kopie. deepseek ok; chatgpt/gemini vom Bug betroffen.
  Bis Fix: verlässliche Antworten via `relay --brain X` (nicht `run`/worker).

**Queued (qwen entscheidet):** browser-pool Hybrid PLAN (Input
`docs/research/browser_pool_files.md`; Windows-Link-Sub-Frage per relay holen; Lane
`browser_pool.rs`/`config` disjunkt von grok) · worker-failover T104813 **HALT** bis
groks Fix · TUI-Redesign (`docs/TUI_DESIGN.md`, auf `tui.rs` aufbauen).

**Offene Storax-Entscheidung:** grok hängt, weil `registry.json` ihn als
`poll_mode: safemode` (externer Watcher) führt → auf `self` umstellen + 4 stale
Watcher-Dateien wegräumen (nur mit Storax-OK, persistente Config). grok-Einstieg =
`Desktop/bot2bot/START_HERE.md`.

---

## Nordstern (ERREICHT + bewiesen)

webagent = Pool von **Claude-anrufbaren autonomen Worker-Brains**. Claude
orchestriert; die Web-Chat-Brains (chatgpt, deepseek, kimi, gemini, qwen, claude,
mistral, zai) arbeiten als vollwertige Agenten — kostenlos (Browser-Login, kein
API-Key). Neben 2 CLI-Agenten (qwen-code, grok-cli) also bis zu ~8 weitere Worker.

**Drei Aufruf-Wege (alle live, committed):**
- `webagent relay --brain X --message "..." --json` → Einzelantwort
  `{brain,ok,answer,latency_ms,reason}`.
- `webagent run --brain X --task "..." --headless` → autonomer Plan/Act/Observe
  (Shell + Dateien, shell_policy-geschützt).
- `webagent bot2bot-worker --brain X` → **dauerhafter autonomer bot2bot-Worker**
  (grok-Äquivalent): pollt `agents/X/inbox`, arbeitet Task via Controller ab,
  schreibt Ergebnis an Absender zurück. Isoliert sein Profil selbst (Q5-copy) →
  **N Worker laufen parallel** (bewiesen: deepseek+chatgpt gleichzeitig, 43 s).

**Der entscheidende Fix** war die Protokoll-Härtung (`prompts.rs` Few-Shot):
davor brachen Brains bei nicht-trivialen `run`-Tasks am webagent/1-JSON.

## Fertiggestellt — Worker-Pool (Teil 1) + TUI (Teil 2) + Heartbeat v2

**qwen-code hat gebaut + verifiziert** (2026-07-17): Worker-Pool-Manager
(`src/worker_pool.rs` + `pub mod worker_pool;` in `lib.rs`) **und** TUI-Default
(`src/tui.rs` — `webagent` ohne Subcommand startet die TUI) **und** Heartbeat-
Hang-Erkennung v2 (`bot2bot_worker.rs` schreibt `heartbeat_<brain>.json` pro Poll;
Supervisor killt stale > 300 s und re-promotet). Alle drei verifiziert:
`cargo test --lib worker_pool::` (8 passed), `cargo clippy --all-targets -- -D warnings`
(exit 0), Pool live mit 8 Workern + Heartbeats. End-to-End geprüft: künstlich
veraltetes `heartbeat_zai.json` → zai-Worker gekillt (PID-Wechsel) + frisch
respawned, Heartbeat erneuert, Status `active`.

**Status (KOMPLETT):**
- `main.rs`-Wiring: `Workers`-Subcommand (`--active` default 2, `--brains`,
  `--poll-secs` default 10, `--headless`) → `run_worker_pool(...)`. Zusätzlich
  `Tui`-Subcommand (`--active` default 8, `--poll-secs` 5, `--headless`);
  `webagent` ohne Subcommand startet die TUI (Default). clippy exit 0.
- Pool **live** (sichtbares Terminal-Fenster "wtui3", headless): 8 active
  (alle Brains) + 0 reserve; `pool_state.json`/`pool_control.json` unter
  `bot2bot/workers/`. Start zwingend mit
  `WEBAGENT_BOT2BOT_ROOT=C:\Users\storax\Desktop\bot2bot`.

**Zwei behobene Fehler (qwen, 2026-07-17):**
1. **Worker-Hang (v2):** hängende Worker (Browser eingefroren / Login-Idle, nie
   Exit) wurden in v1 nicht erkannt. v2: Worker schreibt `heartbeat_<brain>.json`
   bei JEDEM Poll; Supervisor prüft mtime, killt > 300 s alte Heartbeats, setzt
   Brain auf `available`, re-promotet frischen Worker. Bewiesen (s.o.).
2. **Orphan-`active` blockierte Restart:** nach `taskkill` des Pools blieb
   `pool_state.json` mit allen Brains `active` stehen → `select_to_promote` fand
   keine `available` → Pool startete nach Restart **leer** (0 Worker). Fix:
   `reset_orphaned_active()` setzt pro Tick jedes `active` ohne laufenden
   Kindprozess auf `available`. Plus `WEBAGENT_SPARSE_COPY=1` für schnelle
   Profil-Kopie (statt Vollkopie) → Worker starten in Sekunden, nicht Minuten.

**Betrieb (sauber, keine liegengebliebenen Terminals):**
- Start: `cmd /c start "wtui3" cmd /c "SET WEBAGENT_BOT2BOT_ROOT=... && SET
  WEBAGENT_SPARSE_COPY=1 && cd /d <repo> && target\debug\webagent.exe tui --active 8
  --headless"`. `cmd /c` (nicht `/k`!) schließt das Fenster automatisch beim
  Beenden — sonst bleibt bei jedem Restart ein offenes Terminal zurück.
- Stop: in der TUI `q` (schreibt `pool_control.json` stop:true) oder
  `taskkill /F /IM webagent.exe` (killt Supervisor + alle Worker; Fenster
  schließt sich via `cmd /c`). Vor Restart: `taskkill /F /IM webagent.exe`,
  kurz warten, ggf. erneut killen (verwaiste Worker werden neu geparentet).
- TUI-Commands: `+`/`-` (target_active), `r` (reflag_all),
  `send <brain> <text>` (Task routen), `q` (stop).

**Erledigt (committet in diesem Stand):**
- `worker_pool.rs`, `lib.rs`, `main.rs`, `tui.rs`, `bot2bot_worker.rs` gezielt
  gestaged + committet. `scripts/start-workers.ps1` gelöscht (ersetzt durch
  Parameter `--active`/`--headless` am `tui`/`workers`-Subcommand).

## Wie man das Team betreibt (Orchestrierung)

- **Kommunikation:** bot2bot (`Desktop\bot2bot`). Senden:
  `send.ps1 -To <agent> -From claude -Subject "..." -Message "..."`.
  Antworten der Agenten landen in `agents/claude/inbox/`.
- **Worker liest `WEBAGENT_BOT2BOT_ROOT`** (nicht `BOT2BOT_ROOT` — das ist nur für
  send.ps1). Profil-Logins liegen in `webagent-rs/profiles/<brain>` (nicht
  `data/profiles`). zai hat (noch) kein Profil.
- **grok-cli** ist dormant/Reserve (~$2 Credits). Nur wecken mit Subject
  `GROK-FALLBACK: <task>`, z.B. wenn qwen stecken bleibt (grok kennt
  protocol/controller/prompts am besten).
- **Anti-Schleife:** Agenten pflegen `state.json` schlecht → Inbox drainen
  (`inbox/_read/`) + Watermark setzen, wenn sie loopen. Ein Auftrag = eine
  klare Nachricht, disjunkte Datei-Lanes, kein ACK-Pingpong.

## Merkregeln (Claude als Orchestrator)

- **Delegieren, nicht selbst machen.** Builds/Tests → Claude-Code-Subagents.
  Coding → qwen/grok in disjunkten Lanes. qwen nie idle lassen. (Mehrfach von
  Storax eingefordert.)
- **„Fertig"-Meldungen selbst/per Subagent nachprüfen**, nie blind glauben.
- **Gezielt stagen** (nur die Track-Dateien), nie `git add -A` im geteilten
  Worktree — grok/qwen editieren parallel.

## Geparkt (auf Storax-Signal)

- **Recherche→Evaluation über den Pool**: Muster bewiesen (deepseek recherchiert,
  chatgpt+claude evaluieren). Ergebnis der 1. Runde: eigene Agenten-Sprache
  **jetzt nicht bauen** (3-Brain-Konsens: Debugbarkeit > Token-Ersparnis bei
  kleiner Größe).
- Fähigkeitsprofil/`/benchmark` (brain_score-Erweiterung), Controller-Split,
  Integration-Tests — Backlog in `CLAUDE_PROPOSALS.md`.

## Offene Kleinigkeiten

- Release-Workflow bündelt `WebView2Loader.dll` jetzt mit (grok, erledigt).
- `executor::tests::*` flaken vereinzelt unter CPU-Last (bekannt, per Mutex
  entschärft; bei Zweifel isoliert laufen lassen).
