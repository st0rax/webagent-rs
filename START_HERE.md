# START HERE — webagent-rs

**Stand:** 2026-07-17 · Lies diese Datei zuerst, komplett, bevor du andere
Dokumente öffnest. Sie ist in sich geschlossen — du brauchst kein anderes
Repo und kein Vorwissen, um hier weiterzuarbeiten.

> 🔧 **Pflegepflicht:** Wer hier strukturell etwas ändert (neue Features,
> geänderter Provider-/Test-Status, neue Abhängigkeiten, Versionssprung)
> aktualisiert diese Datei **als Teil derselben Änderung**, nicht als
> Nachtrag. Ein veralteter Stand hier ist schlimmer als keiner — er kostet
> die nächste Session Zeit, ihn erst zu widerlegen. Diese Regel gilt
> unabhängig davon, mit welchem Tool/Agenten gearbeitet wird (Claude Code,
> Codex, Grok-CLI, ein Mensch, egal).

---

## 0. Was ist das

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant,
lokale Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare
Web-Chats (ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) —
kein API-Key, sondern die im Browser angemeldete Session. Rust-Port eines
ursprünglichen Python-Projekts; Embedded WebView (`wry`/`tao`) statt
CDP/Playwright.

## 1. ⚠️ Zwei verschachtelte Git-Repos — nicht eins

Dieser Ordner (`webagent-rs/`) ist ein **eigenständiges Repo**
(`github.com/st0rax/webagent-rs`). Er liegt aber typischerweise verschachtelt
in einem **anderen, separaten** Repo (`Desktop\webagent\`, Remote
`github.com/st0rax/webagent`, anderer Account) — der alten
Python-Referenzimplementierung. `webagent-rs` ist dort als Gitlink (Mode
`160000`) eingetragen, aber **ohne `.gitmodules`** — ein behelfsmäßiger, kein
echter Submodule-Verweis. Falls du im Elternordner landest und dich wunderst,
warum `git log` dort etwas völlig anderes zeigt: das ist erwartbar, es ist
ein anderes Repo. Details, falls relevant: `CLEANUP_PLAN.md` (in diesem
Repo) und `docs/struktur.md` im Elternordner. Ein unfertiger Branch
`origin/docs/deprecate-in-favor-of-rust` im Elternrepo zielt darauf ab, diese
Verschachtelung aufzulösen — nicht gemerged, vor eigener Aufräumaktion dort
erst prüfen.

## 2. Architektur

Ein Brain plant im `webagent/1`-Protokoll (JSON), der Controller führt die
Actions strikt seriell aus, Beobachtungen fließen zurück ins Brain:

```
Brain (Web-Chat)  ──plan──▶  Controller  ──shell──▶  Executor (PowerShell/sh)
      ▲                          │
      └──────── Observation ◀────┘
```

| Modul | Verantwortung |
|---|---|
| `protocol` | `webagent/1`-Parser inkl. `WEBAGENT/1 SHELL`-Rohformat |
| `controller` | Plan/Act/Observe-Zustandsmaschine, Resume, Loop-/Budget-Schutz |
| `brain` | Trait `BrainBackend` (Browser-neutral) |
| `browser` + `webview_runtime` + `page_driver` | Embedded WebView-Backend (`wry`/`tao`) |
| `browser_pool` | Shared-Profil: ein Runtime, ein Tab pro Brain |
| `executor` | Shell-Ausführung (Windows: PowerShell, Unix: sh/bash) |
| `shell_policy` | Denylist + Audit vor jeder Shell-Ausführung (Sicherheitsnetz, kein Sandbox) |
| `circuit_breaker` | Pro-Brain Failure-Tracking, überspringt dauerblockierte Brains statt jedes Mal neu zu warten |
| `brain_score` | Leistungsindex (Wilson-Score-Reliability aus echten Aufrufen), `/score`-Befehl |
| `run_store` · `transcript` · `memory` | Persistenz (JSON-Lines) |
| `comms` | Internes Agent-zu-Agent-Messaging (nichts mit `bot2bot` zu tun — getrenntes System) |
| `doctor` · `watchdog` · `brains_health` | Diagnose & Pre-flight |
| `relay` · `oobe` · `repl` | Single-turn Relay, Ersteinrichtungs-Wizard, interaktive REPL |
| `timeouts` · `loop_guard` · `observer` · `prompts` · `config` | Politik & Heuristiken |

Selektoren pro Provider liegen in `selectors/`; Portierungsregeln in
`CONVENTIONS.md`.

## 3. Build/Test

Voraussetzung: eine Rust-Toolchain. Auf Windows **ohne** Visual Studio genügt
die GNU-Toolchain:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu   # im Projektordner
cargo build --release
cargo test --no-default-features   # CI-Parität (ohne WebView/GTK)
cargo test                         # mit WebView-Feature (lokal)
cargo clippy --all-targets -- -D warnings
```

Der Kern baut rein-Rust (`serde`, `serde_json`, `regex`, `fancy-regex`,
`clap`, `time`). WebView-Deps (`wry`, `tao`, `webview2-com`, `windows`) sind
optional (`--no-default-features` für headless CI).

**`WebView2Loader.dll`** muss neben `webagent.exe` liegen. Bei `cargo build`
kopiert `webview2-com-sys`s eigenes `build.rs` das automatisch nach
`target/release/`. Der **GitHub-Release-Workflow tut das aktuell nicht**
(`.github/workflows/release.yml` lädt nur die lose `.exe` hoch) — wer sich
nur die Release-`.exe` von GitHub runterlädt, bekommt vermutlich einen
Absturz. Ungefixt, siehe §6.

## 4. Wo was liegt

- `src/` — siehe Architektur-Tabelle oben
- `selectors/*.json` — pro-Provider CSS/Playwright-Text-Selektoren
- `data/` — Runtime-State (Runs, Memory, Circuit-Breaker, Brain-Score, Audit-Log); gitignored
- `docs/` — Konzept-/Planungsdokumente (siehe §5)
- `.github/workflows/` — CI (`ci.yml`, `android.yml`) + Release (`release.yml`)
- Root-`.md`-Dateien: `README.md` (öffentliche Übersicht), `CONVENTIONS.md`
  (Code-Konventionen), `docs/PROVIDER_STATUS.md` (Provider-Messwerte, mit
  Historie); externe Reviews + Roadmap in `docs/` (`CODE_REVIEW.md`/
  `CLAUDE_PROPOSALS.md`, siehe §6)

## 5. Konzept-/Planungsdokumente

- `docs/AUTORESEARCH_PLAN.md` — vollständiger, **noch nicht umgesetzter**
  Implementierungsplan für einen autonomen Verbesserungs-Loop (Karpathys
  `autoresearch`-Muster: Modify→Verify→Keep/Discard→Repeat). Bewusst so
  geschrieben, dass jemand ohne Vorwissen direkt einsteigen kann.
- `docs/GENIUS_COUNCIL_CONCEPT.md` — Multi-Brain-Council-Idee, **bewusst
  zurückgestellt** (Status im Dokument), teilweise durch `/swarm` (§6) ersetzt.

## 6. Aktueller Stand (2026-07-17, nachgemessen)

v0.8.1. **8/8 Provider antworten headless mit echten Antworten** (chatgpt,
deepseek, kimi, gemini, qwen, claude, mistral, zai) — gemessen per REPL mit
gehaltener Session (siehe Methodik-Hinweis unten), nicht per Kaltstart-Loop.
`cargo test --lib`: 186+ grün, `clippy --all-targets -D warnings` clean.

**REPL-Befehle:** `/model <brain>` (= `/switch`), `/chat <text>`, `/goal
<text>` (stehendes Ziel, fließt in autonome Aufgaben ein), `/swarm [n]
<text>` (alle Brains antworten, ein Orchestrator — fest gewählt oder per
Konsens — führt zusammen), `/score` (Leistungsindex-Tabelle), `/whoami`,
`/brains`, `/new`, `/memory`, `/login`.

**Sicherheit:** `shell_policy.rs` prüft jeden Shell-Befehl vor Ausführung
gegen eine Denylist (rekursives Löschen, Formatieren, Fork-Bombs,
Download-Cradles) + Audit-Log (`data/audit/shell.jsonl`). Kein
Allowlist-only — die Shell ist by Design offen (Single-User-Local-Agent),
das ist ein Sicherheitsnetz, keine Sandbox.

**Externer Review vorhanden:** `docs/CODE_REVIEW.md` + `docs/CLAUDE_PROPOSALS.md`
(Qwen/Grok, 2026-07-16) mit priorisierter Roadmap. ⚠️ Der darin behauptete
P0-Blocker („7 rote Executor-Tests") war zum Zeitpunkt der Review nicht
reproduzierbar (mehrfach 186/186 grün gemessen) — die eigentliche Ursache
war Prozess-Spawn-Kontention unter Voll-Parallel-Läufen, inzwischen per
Mutex-Serialisierung behoben. Zahlen in Review-Dokumenten vor Gebrauch selbst
nachmessen, nicht blind übernehmen.

**Methodik-Lektion (wichtig für jede künftige Stabilitätsmessung):** ein
Kaltstart-Relay-Loop (viele Sessions im Sekundentakt starten/stoppen)
erzeugt selbst die Rate-Limits, die er messen soll. Immer so testen, wie das
Produkt benutzt wird (REPL, gehaltene Session) — nicht per Kaltstart-Hämmern.

**Offene Punkte:**
- Release-Workflow bündelt `WebView2Loader.dll` nicht mit der `.exe` (§3)
- Canary (periodischer 8-Brain-Health-Check), Protocol-Repair-Loop,
  Controller-Split (`controller.rs` ~1150 Zeilen) — in `docs/CLAUDE_PROPOSALS.md`
  skizziert, nicht begonnen
- Fähigkeitsprofil-Teil des Leistungsindex (`/benchmark`, Stärken/Schwächen
  je Kategorie, maximale Prompt-Länge) — bewusst nicht mitgebaut, siehe
  Commit-Historie von `brain_score.rs`
- `docs/AUTORESEARCH_PLAN.md` — geplant, nicht implementiert

## 7. Nicht verwechseln

`Desktop\webagent\` (der Elternordner, Python) ist die **Alt-Referenz**,
kein Pflegeziel — nur als Verhaltensvorlage beim Rust-Port relevant, siehe §1.
Zwei weitere, komplett unabhängige Projekte existieren daneben:
`bot2bot` (`github.com/st0rax/bot2bot`, dateibasiertes Agent-Messaging —
hat nichts mit diesem Repos `comms.rs` zu tun) und `presence-monitor`
(`github.com/st0rax/presence-monitor`). Keine Schnittmenge, keine
Abhängigkeit — jedes Projekt hat seine eigene `START_HERE.md`.
