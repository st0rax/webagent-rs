# WebAgent — Architektur

> **Archiv.** Kein Soll-Zustand. Aktuell: docs/OVERVIEW.md, TUI-Betrieb: AGENTS.md §6.
> **Zeilenzahlen nachmessen** — `controller.rs` ist wieder ~2500 Z.

> **Detaillierte Momentaufnahme.** Für Zielbild, Vertrauensleiter, Reifegrade
> und aktuellen Funktionsstand siehe [`OVERVIEW.md`](OVERVIEW.md). Dateizahlen
> und Refactoring-Befunde in diesem Dokument sind zeitgebunden und vor einer
> Entscheidung am aktuellen Quellbaum nachzumessen.

> **Zweck:** Dieses Dokument ist die Karte. Sie erklärt in 10 Minuten, welche
> Schichten existieren, was wo liegt, und wo die dokumentierten
> Struktur-Schwächen sind. Grundlage ist der statische Wissensgraph
> (`.graphify/`, siehe Abschnitt [Graph](#graph)) — die Community-Grenzen dort
> **sind** die Modul-Grenzen.

**Stand:** v0.8.1 · 94 Quelldateien · ~52.000 Zeilen · Kern plattformrein,
Browser via Embedded WebView (`wry`/`tao`), TUI via `ratatui`/`crossterm`.

## Schichten

```
┌─────────────────────────────────────────────────────────────┐
│  ui/          CLI + TUI + REPL            (Einstieg, dünn)  │
│   main, cli, commands/, repl/, tui*, brain_grid, welcome    │
├─────────────────────────────────────────────────────────────┤
│  agent/       Steuerung & Agent-Schleife (Plan→Act→Observe) │
│   controller, prompts, executor, file_actions, capability,  │
│   capability_proof, relay, knockout, canary                 │
├─────────────────────────────────────────────────────────────┤
│  brain/       Gehirn-Abstraktion + Browser-Anbindung        │
│   brain (Trait+SessionState), browser/{mod,ui,backend,…},   │
│   browser_pool, page_driver, mock_page, login,              │
│   webview_runtime, brain_probe, brain_grid, brain_limits    │
├─────────────────────────────────────────────────────────────┤
│  bench/       Messung & Selbst-Verbesserung                 │
│   benchmark, bench_*, round_tally, runs_report,             │
│   code_score, brain_score, design_vote,                     │
│   autoresearch, self_research, wiki_memory, brains_health   │
├─────────────────────────────────────────────────────────────┤
│  workers/     Parallelität & Gesundheit                     │
│   worker_pool, bot2bot_worker, watchdog, doctor             │
├─────────────────────────────────────────────────────────────┤
│  core/        plattformreiner Kern (keine UI/Browser-Deps)  │
│   lib (zeit/pid/text), config, protocol, timeouts,          │
│   run_store, transcript, observer, comms, memory,           │
│   shell_policy, loop_guard, circuit_breaker, oobe           │
└─────────────────────────────────────────────────────────────┘
```

**Abhängigkeitsregel (Ist + Soll):** `core` ← `brain` ← `agent` ← `bench`/`workers` ← `ui`.
Keine Schicht darf auf eine höhere zugreifen. Aktuell ist das eine *Konvention*,
keine Compiler-Erzwingung — die Module liegen (mit Ausnahme von `config/`,
`protocol/`, `benchmark/`, `browser/`, `controller/`, `repl/`, `commands/`)
flach in `src/`.

## Der Datenfluss eines Runs

```
ui: webagent run <brain> <task>
  └─ controller: AgentController          Plan→Act→Observe-Loop
       ├─ protocol: webagent/1-Parser     Brain-Antwort → Action
       ├─ brain::BrainBackend (Trait)     Chat-Session, austauschbar
       │    └─ browser::WebBrainBackend   Embedded WebView (wry)
       │         ├─ backend.rs            Session (ensure_ready, send, new_chat)
       │         ├─ ui.rs                 Menü-/Toggle-Steuerung
       │         ├─ operations.rs         DOM-Operationen
       │         ├─ blocking.rs           Block-Banner-/Warte-Erkennung
       │         ├─ send.rs               Sende-Pfad (send_generic/gemini/qwen)
       │         ├─ surface.rs            DOM-Inspektion, Login, Live-Diagnose
       │         └─ verify.rs             Verifikation der UI-Aktionen
       ├─ executor::ShellExecutor         lokale Shell (PowerShell/sh)
       ├─ shell_policy                    Deny-/Audit-Regeln
       ├─ file_actions                    Working-Tree-Änderungen
       ├─ run_store / transcript          Persistenz & Verlauf
       └─ memory / loop_guard             Kontext & Abbruchschutz
```

## Modul-Karte (vollständig)

| Schicht | Module | Rolle |
|---|---|---|
| **core** | `lib.rs` | Zeitstempel (Python-kompatibel), PID-Liveness, `StageTimer`, Text-Helfer |
| | `config/` | Pfade (`paths`), Write-Back (`writeback`), Limits, Brains, Profile-Layout, Selektor-Auflösung, Kopierplanung (`clone`) |
| | `protocol/` | `webagent/1`-Protokoll (Parser, Action-Typen, Text-Formatierung) |
| | `timeouts.rs`, `loop_guard.rs`, `circuit_breaker.rs` | Schutz-Mechanismen |
| | `run_store.rs`, `transcript.rs`, `memory.rs`, `observer.rs`, `comms.rs` | Persistenz & Verlauf |
| | `shell_policy.rs`, `executor.rs`, `file_actions.rs`, `oobe.rs` | Werkzeuge |
| **brain** | `brain.rs` | `BrainBackend`-Trait, `SessionState`, `BrainResponse` |
| | `browser/{mod,ui,backend,composer,js,operations,selectors,verify,blocking,send,surface}.rs` | WebBrainBackend-Impl (WebView) |
| | `browser_pool.rs`, `page_driver.rs`, `mock_page.rs`, `login.rs` | Browser-Lebenszyklus |
| | `webview_runtime.rs` | wry/tao-Runtime (eigene Event-Loop) |
| | `brain_probe.rs`, `brain_grid.rs`, `brain_limits.rs` | Diagnose-UI & Grenzen |
| **agent** | `controller/{mod,types,resume,plan}.rs` | AgentController — Plan/Act/Observe-Zustandsmaschine (+ Turn-/Options-Typen, Resume-Logik, Aktionsplan-Validierung) |
| | `prompts.rs` | Prompt-Bau (autonomous, resume, recovery) |
| | `capability.rs`, `capability_proof.rs` | Fähigkeits-Nachweise |
| | `relay.rs`, `knockout.rs`, `canary.rs`, `welcome.rs` | CLI-Helfer |
| **bench** | `benchmark/` | Fertigungsstraße (`pipeline`), Aufgaben-Bau (`tasks`), Git/Eval-Helfer (`git`), Ernte (`harvest`), Handoff-Warteschlange (`handoff`), Ausgabe (`report`), Shared-Typen (`types`) |
| | `bench_events.rs`, `bench_harvest.rs`, `bench_scoring.rs`, `round_tally.rs`, `runs_report.rs` | Benchmark-Teil-Pipelines |
| | `code_score.rs`, `brain_score.rs`, `brains_health.rs` | Wilson-Scores |
| | `design_vote.rs` | Design-Voting |
| | `autoresearch.rs`, `self_research.rs`, `wiki_memory.rs` | Selbst-Verbesserungs-Schleifen |
| **workers** | `worker_pool.rs`, `bot2bot_worker.rs` | Parallel-Betrieb |
| | `doctor.rs`, `watchdog.rs` | Diagnose & Aufräumen |
| **ui** | `main.rs`, `cli.rs`, `commands/{ops,research,ui,mod}.rs` | CLI-Einstieg & Dispatch |
| | `repl/{mod,commands}.rs` | Chat-REPL |
| | `tui.rs`, `tui_render.rs`, `tui_state.rs`, `tui_keys.rs`, `tui_mouse.rs`, `tui_config.rs`, `brain_grid.rs` | TUI (Feature `tui`) |

## God-Files — dokumentierte Schnitte

Der Graph zeigt für die großen Dateien interne Community-Split-Tendenzen.
Diese Nähte sind die Kandidaten für spätere Aufteilung (siehe GRAPH_REPORT.md):

| Datei | Zeilen | Communities | Innere Nähte |
|---|---|---|---|
| `capability.rs` | ~1080 | **1** | Fähigkeits-Level / Nachweis-Logik / Brain-Probe-Anbindung |
| `brain.rs` | ~660 | **1** | Trait / SessionState / Mock-Tests |

> **Erledigt (2026-08-09):** Die drei größten Einzel-Dateien sind entlang der
> Community-Nähte aufgeteilt:
> - `config.rs` (2802 Z.) → `config/{mod,paths,writeback,limits,brains,profiles,selectors,clone}.rs`
> - `benchmark.rs` (2996 Z.) → `benchmark/{mod,types,tasks,git,harvest,handoff,report,pipeline}.rs`
> - `protocol.rs` (1719 Z.) → `protocol/{mod,types,parser,text}.rs`
> - `browser/mod.rs` (1848 Z.) → `browser/{mod,blocking,send,surface}.rs`
> - `controller.rs` (2419 Z.) → `controller/{mod,types,resume,plan}.rs`
>
> Öffentliche API unverändert: alle `pub`-Items werden in den jeweiligen
> `mod.rs`/`controller.rs` explizit re-exportiert (kein Glob). Sichtbarkeits-Bumps
> nur dort, wo Sibling-Module aufeinander zugreifen müssen (z. B.
> `send_generic`/`send_gemini`/`send_qwen` für `backend`/`verify`,
> `recover_from_incomplete`/`resume_initial_turn` als `pub(crate)`-impl-Methoden).
> `pub(crate)`-Helfer (z. B. `fnv1a`, `patch_touched_paths`, `HandoffQueue`,
> `bench_say!`) wurden nur intern verschoben.

## Bekannte Redundanzen

1. **Wilson-Score doppelt** — `code_score.rs` (objektiver Benchmark-Outcome) und
   `brain_score.rs` (Nutzungs-Reliability) implementieren denselben
   Wilson-Lower-Bound. Der gemeinsame Kern fehlt.
2. **Drei Karpathy-Schleifen** — `autoresearch.rs`, `self_research.rs`,
   `wiki_memory.rs` sind drei Varianten desselben „Verifizieren-Verwerfen-Loop".
3. **Shell-Policy doppelt dokumentiert** — Text steht sowohl in
   `docs/CLAUDE_PROPOSALS.md` als auch in `CONVENTIONS.md`.

## API-Fläche & tote Items (P1-Befund)

`lib.rs` deklariert 55 Module, alle `pub`. Ein Experiment (alle Module auf
`pub(crate)`, dann `cargo check --all-targets`) zeigt: **nur ~31 Module nutzt
das Binary direkt** (via `main.rs` und `commands/*`) — der Rest ist
Implementierungsdetail und könnte `pub(crate)` sein.

Der Versuch deckt dabei **~25 tote public Items** auf, die bisher von `pub`
versteckt wurden (clippy meldet sie erst, wenn das Modul `pub(crate)` ist):

- `MockPageState` / `MockStateInner` / `MockPageDriver` (`mock_page.rs`) — nie konstruiert
- `detach_brain`, `has_tab`, `tab_ref_count` (`browser_pool.rs`)
- `select_with_fallback` (`browser/selectors.rs`)
- `rounds_remaining` (`round_tally.rs`)
- `terminal_window_rect`, `right`, `bottom`, `overlaps`, `contained_in` (`brain_grid.rs`)
- `last_response`, `bench_scroll`, `ConfirmQuit` (`tui_state.rs`)
- `error_code`, `format_audit_line`, `validate_path_allowlist_recursive`,
  `evaluate_command_policy`, `ScriptMap` (u. a.)

**P1-Schritt:** Diese Items löschen oder verdrahten, dann die ~24 nicht vom
Binary genutzten Module auf `pub(crate)` stellen. Erzwingt sofort, dass jede
neue öffentliche Funktion auch wirklich benutzt wird.

## Feature-Gating

- `tui` → `ratatui`, `crossterm`. In `lib.rs` sind `tui_render/state/keys/mouse`
  hinter dem Feature; `tui.rs` gated seine ratatui-Teile intern. `tui_config.rs`
  und `brain_grid.rs` sind ratatui-frei (Windows-API / Pure-Logik) und stehen
  deshalb bewusst ohne `#[cfg]`.
- `webview` → `wry`, `tao`, `webview2-com`, `windows` (`webview_runtime.rs`).

**Verifiziert:** `cargo check --no-default-features` baut den Kern ohne UI.
Default ist `webview + tui`.

## Graph

Der statische Wissensgraph unter `.graphify/` (3164 Knoten, 5825 Kanten,
188 Communities) ist der maschinenlesbare Spiegel dieses Dokuments. Er wird
über `webagent`-interne Tooling regeneriert; `studio/index.html` ist eine
offline navigierbare Ontologie-Studio-Ansicht des Graphen.

- `graph.json` — Rohdaten (Knoten mit `community`-Attribut)
- `GRAPH_REPORT.md` — Audit: God-Nodes, überraschende Kanten, offene Fragen
- `memory/` — beantwortete Graph-Fragen
