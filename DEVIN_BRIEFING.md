> **Archiv.** Kein Soll-Zustand. Aktuell: docs/OVERVIEW.md, TUI-Betrieb: AGENTS.md §6.

# Einstieg fuer neue Entwickler

## Projekt
**webagent-rs** — autonomer Coding-Agent, der in einem Benchmark-Schwarm aus
9 LLM-Brains Code vorschlaegt, baut, testet und automatisch ins Repo erntet.

Branch: `dogfood/gemini-telemetry-fenced`

## Architektur (Schluesseldateien)

| Datei | Zweck |
|---|---|
| `src/benchmark/pipeline.rs` | Herzschlag: Brain-Loop, Pass-Gate (build+test+lint), Ernte-Auswahl |
| `src/benchmark/tasks.rs` | Aufgabenverfeinerung, Phantom-Gate, Fehlweisungs-Gate, Redundanz-Check |
| `src/target_check.rs` | Prueft ob eine Aufgabe auf existierenden Code zeigt (Symbol-extraktion) |
| `src/benchmark/harvest.rs` | Ernte: Patch-Validierung, Scope-Pruefung, git-commit |
| `src/bench_harvest.rs` | Ernte-Schnittstelle (validate_harvest_patch, has_substantive_change) |
| `src/code_score.rs` | Scoring/Events (CodeEvent, Wilson-Score, is_pass) |
| `src/bench_scoring.rs` | is_pass, Progress, outcome_label |
| `src/benchmark/mod.rs` | BenchmarkConfig, Baseline-Testzaehlung |
| `src/benchmark/types.rs` | BenchmarkConfig, HarvestCandidate, BenchmarkReport |
| `src/benchmark/git.rs` | run_eval_detail, capture_patch, reset_repo |
| `src/brain.rs` | Brain-Schnittstelle (LLM-Calls) |
| `STATUS_LIVE.md` | Laufender Stand, was passiert ist |

## Pass-Gate (Brain-Loop)

Ein Brain-Pass verlangt: `did_change && compiled && tests_passed && lint_ok`

Der Loop in `pipeline.rs` (~Zeile 693):
1. Brain-Run (Executor)
2. `cargo build --lib` — wenn rot => Reparier-Prompt
3. `cargo test --lib` — wenn rot => Reparier-Prompt
4. `cargo clippy --all-targets -- -D warnings` — wenn rot => Reparier-Prompt mit Lint-Output
5. Erst wenn alles gruen => PASS => Patch als Ernte-Kandidat sichern

## Ernte-Pipeline

Nach jeder Runde:
1. `pick_harvest` waehlt den besten Kandidaten (Score-Rang)
2. `harvest_commit` wendet den Patch auf den sauberen Tree, verifiziert build+test+lint erneut
3. Bei Erfolg: `git commit` mit Brain als Autor
4. Bei Fehler: Tree wird zurueckgesetzt

## Pre-Flight-Gates (pipeline.rs refine_one)

Vor dem Brain-Bauen prueft der Harness:
1. `task_is_redundant` — Aufgabe fuegt nichts Neues hinzu
2. `task_targets_missing_file` — Zieldatei existiert nicht
3. `task_is_misdirected` — Symbol steht in anderer Datei (nur Koerper-Text)
4. `task_has_phantom_anchors` — Lokale Belege nennen nicht-existierende Symbole
5. `refinement_has_evidence` — mindestens 2 lokale Belege + Abschlussbeleg

## Symbol-Extraktion (target_check.rs)

`wie_bezeichner(s, streng)` erkennt Rust-Bezeichner:
- Snake_case (mit `_`) => immer ok (>=4 Zeichen)
- PascalCase/camelCase => mind. 1 Gross+1 Klein, kein Bindestrich/Schraegstrich
- Akronym-Plurale (`APIs`, `IDs`) => abgelehnt (deutsche Fliesstext-Woerter)
- Deutsche Komposita (`Backend-Implementierung`) => abgelehnt

## Befehle

```bash
# Benchmarks (CLI)
target\debug\webagent.exe benchmark --brains chatgpt,claude,deepseek,gemini,kimi,mistral,perplexity,qwen,zai --rounds 6 --suggestions 3 --parallel 2 --headless

# TUI mit Benchmark (aus opencode oder Terminal)
target\debug\webagent.exe tui --force-tui --benchmark="--rounds 6" --view=bench

# Nur Tests
cargo test --lib

# Lint
cargo clippy --all-targets -- -D warnings

# Build
cargo build
```

## Monitoring

```bash
# Live-Log (letzte 10 Events)
powershell -Command "Get-Content 'C:\Users\storax\AppData\Local\webagent\data\brain_score\events.jsonl' -Tail 10"

# Circuit-Breaker (gesperrte Brains)
powershell -Command "Get-Content 'C:\Users\storax\AppData\Local\webagent\data\circuit_breaker\state.json' | ConvertFrom-Json | ConvertTo-Json -Depth 3"

# Alles auf einmal
powershell -File bench-monitor.ps1
```

## Wichtige Lektionen

- **Stale Binary:** Nach Code-Aenderungen IMMER `cargo build` ausfuehren, bevor ein
  Run gestartet wird. Der 08:29-Run lief mit einer uralten exe und konnte den
  Redundanz-Fix nicht anwenden.
- **Guard Clean Tree:** Benchmark-Runs nur bei sauberem Git-Tree starten.
- **Clippy als Gate:** Lint gehoert zum Pass-Gate, nicht nur zur Ernte-Nachkontrolle.
  Ohne Lint-Gate gingen Patch-Kandidaten durch, die bei der Ernte an Clippy scheiterten.
- **Deutsche Kommentare:** Der Code ist deutsch kommentiert. Der Symbol-Extraktor
  muss deutsche Nominalphrasen von Rust-Bezeichnern unterscheiden koennen.

## Aktueller Stand (16.08.2026)

4 Ernten im Repo (autonom vom Harness erzeugt):
- `0894c94` deepseek: is_availability_outage Guard (bench_scoring.rs)
- `bf89c09` perplexity: Navigation-Timeout Diagnose (browser/backend.rs)
- `35e05f8` chatgpt: Regressionstest leere Webview (brain_probe.rs)
- `db6c75e` deepseek: Test-Refactoring (brain_probe.rs)

Laufender Run: 21:34 gestartet, 6 Runden, 7 Brains (mistral+perplexity gesperrt).

## Wichtige Lektionen (ergaenzt)

- **Working Tree dirty durch untracked Files:** `git status --short` pruefen —
  auch `??`-Eintraege brechen den Benchmark (`autoresearch.rs:550`).
- **Kacheln brauchen Konsolenfenster:** Bei Start aus opencode via
  `Start-Process cmd.exe` gibt es kein `GetConsoleWindow()` => Kacheln
  schlagen fehl. Kosmetisch, Benchmark laeuft trotzdem.
- **OCR funktioniert nur mit Windows PowerShell** (nicht pwsh) wegen
  WinRT-Bridge (`System.Runtime.WindowsRuntime`).
