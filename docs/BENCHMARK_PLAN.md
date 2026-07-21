# benchmark — objektiver Code-Kompetenz-Score (Spec)

> STATUS: SPEZIFIZIERT 2026-07-21. Die fehlende **code-Dimension** des im
> `brain_score.rs`-Header geplanten Fähigkeitsprofils. Anders als der
> Reliability-Score (Wilson über "hat geantwortet") ist dies **objektiv**:
> der Compiler + die Tests sind der Schiedsrichter (umgeht den Phantom-Done-
> Komplex vollständig — kein Selbst-Report zählt, nur `cargo test`).

## 1. Idee

Ein Benchmark ist der VOLLE Selbst-Verbesserungs-Loop, nicht eine fixe Aufgabe:
**der Schwarm sammelt Vorschläge, stimmt ab, und wird dann am autonomen Bauen
des Siegers gemessen.** Die Aufgabe ist also jedes Mal dynamisch der
top-gevotete Vorschlag. Gemessen wird hart und objektiv (kein Selbst-Report):
- **did_change** — hat das Brain überhaupt etwas geändert (git diff nicht leer)?
- **compiled** — baut das Ergebnis (`cargo build --lib` exit 0)?
- **tests_passed** — bleibt die Suite grün (`cargo test --lib` exit 0)?
- pass = did_change && compiled && tests_passed.
Daraus eine Code-Kompetenz-Quote pro Brain (Wilson-Lower-Bound wie Reliability).

## 2. Ablauf eines Benchmark-Zyklus

**Phase A — Sammeln + Abstimmen (immer zuerst):** `self_research::run_self_research`
aufrufen → Rangliste. Der Platz-1-Vorschlag wird zur Benchmark-Aufgabe. Daraus
ein Task-Prompt bauen: "Implementiere folgenden Verbesserungsvorschlag im
Rust-Projekt webagent-rs (aktuelles Verzeichnis) mit dem Rohformat (WEBAGENT/1
EDIT/WRITE). Ergänze Tests. `cargo test --lib` muss grün bleiben. <winner>".

**Phase B — Implementieren + Messen, pro Brain (sequenziell):**
1. **Git-Sicherheit:** Working Tree muss sauber sein (sonst Abbruch). Baseline-
   SHA merken (`autoresearch::git_head_sha`).
2. **Run:** Task via Controller-`run` (headless, kleiner max_cycles) — das Brain
   baut den Sieger. Rohformat (Commit 897a032) macht das erst praktikabel.
3. **Eval** über einen Kommando-Runner mit Timeout (wie `autoresearch::eval_metric`):
   `did_change` (git diff --quiet ? nein=Änderung), dann `cargo build --lib`,
   dann `cargo test --lib`. Jeweils exit-Code als bool.
4. **Event speichern** (JSONL wie brain_score): brain, task_id (Winner-Hash),
   did_change, compiled, tests_passed, cycles, latency_ms, ts.
5. **Reset:** `git reset --hard <baseline>` + `git clean -fd` — jedes Brain
   startet identisch, der Benchmark hinterlässt KEINE Änderung. So misst jedes
   Brain denselben Sieger unabhängig.

`--rounds N` wiederholt den ganzen Zyklus N-mal (N Abstimmungen → N Sieger →
N×Brains Datenpunkte). Der Score aggregiert über alle Events.

## 3. Neues Modul `src/code_score.rs`

Spiegelt `brain_score.rs` (JSONL + Wilson), aber auf dem objektiven Outcome:

```rust
pub struct CodeEvent { brain_id, task_id, compiled: bool, tests_passed: bool,
                       cycles: u32, latency_ms: u64, ts: String }
pub fn record(event: &CodeEvent);                 // -> data/code_score/events.jsonl
pub struct CodeStats { brain_id, attempts, compile_rate, pass_rate,
                       wilson_pass: f64 }
pub fn leaderboard() -> Vec<CodeStats>;           // sortiert nach wilson_pass
```

Reine Helfer (Wilson, Aggregation) unit-getestet gegen ein Tempdir-JSONL.

## 4. Harness `src/benchmark.rs` + CLI

- `webagent benchmark [--brains <csv>] [--rounds N] [--suggestions K]
  [--build-eval <cmd>] [--test-eval <cmd>] [--workdir <pfad>] [--headless]`
  Default: alle verfügbaren Brains, `cargo build --lib` / `cargo test --lib`,
  rounds=1, suggestions=10 (für die Abstimmungsphase), workdir=Repo-Root.
- KEIN `--task`: die Aufgabe kommt IMMER aus der Abstimmung (Phase A). Optional
  ließe sich per Flag ein fester Task erzwingen, aber Default ist vote-driven.
- Live-Fortschritt: `[benchmark] runde 1/N — abstimmen…`, dann
  `[benchmark] Sieger: <winner>`, dann pro Brain
  `[benchmark] deepseek: run… did_change=ja build=ok test=ok -> PASS`.
- Ausgabe am Ende: Code-Rangliste (brain | attempts | change% | compile% |
  pass% | wilson_pass).
- Ergebnis zusätzlich als Wiki-Seite `code-benchmark-<stamp>` (inkl. der
  gevoteten Sieger je Runde, damit nachvollziehbar ist, WAS gebaut werden sollte).

## 5. Sicherheit / Grenzen v1

- Läuft NUR auf sauberem Git-Tree; resettet nach jedem Lauf hart. Nie auf
  master ohne Wegwerf-Branch (der Aufrufer wählt den Branch).
- Sequenziell (Profile/Locks). Parallel später.
- Eine Aufgabe/Dimension (code). Weitere Dimensionen (reasoning/kreativ) später.
- Kein Merge/Commit der Brain-Änderung — reiner Messlauf, Reset ist Pflicht.

## 6. Tests

Reine Funktionen (Wilson, Aggregation, Event-Parsing, outcome-Klassifikation)
voll unit-getestet. Der Live-Teil (echtes Brain + cargo) wird vom Orchestrator
end-to-end geprüft, nicht im Unit-Test.
