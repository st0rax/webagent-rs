# benchmark — objektiver Code-Kompetenz-Score (Spec)

> STATUS: SPEZIFIZIERT 2026-07-21. Die fehlende **code-Dimension** des im
> `brain_score.rs`-Header geplanten Fähigkeitsprofils. Anders als der
> Reliability-Score (Wilson über "hat geantwortet") ist dies **objektiv**:
> der Compiler + die Tests sind der Schiedsrichter (umgeht den Phantom-Done-
> Komplex vollständig — kein Selbst-Report zählt, nur `cargo test`).

## 1. Idee

Jedes Brain bekommt DIESELBE, self-contained Coding-Aufgabe und muss sie
autonom (via `run` + Rohformat) umsetzen. Gemessen wird hart:
- **compiled** — baut das Ergebnis?
- **tests_passed** — ist das Eval-Kommando (z.B. `cargo test --lib`) grün?
Daraus eine Code-Kompetenz-Quote pro Brain (Wilson-Lower-Bound wie Reliability).

## 2. Ablauf pro Brain × Runde

1. **Git-Sicherheit:** Working Tree muss sauber sein (sonst Abbruch). Baseline-
   SHA merken (`autoresearch::git_head_sha`).
2. **Aufgabe** via Controller-`run` (headless, kleiner max_cycles) abarbeiten
   lassen — mit dem Rohformat (EDIT/WRITE), das autonome Code-Edits erst
   praktikabel macht (Commit 897a032).
3. **Eval** über `autoresearch::eval_metric(cmd, workdir, timeout)`:
   - Build-Check: `cargo build --lib` exit 0 → `compiled=true`.
   - Test-Check: `cargo test --lib` exit 0 → `tests_passed=true`.
   (Zwei getrennte Evals, damit "kompiliert aber Tests rot" unterscheidbar ist.)
4. **Event speichern** (JSONL wie brain_score): brain, task_id, compiled,
   tests_passed, cycles, latency_ms, ts.
5. **Reset:** `git reset --hard <baseline>` + `git clean -fd` — jedes Brain
   startet identisch, der Benchmark hinterlässt KEINE Änderung.

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

- `webagent benchmark [--brains <csv>] [--task <datei>] [--build-eval <cmd>]
  [--test-eval <cmd>] [--rounds N] [--workdir <pfad>] [--headless]`
  Default: alle verfügbaren Brains, `cargo build --lib` / `cargo test --lib`,
  rounds=1, workdir=Repo-Root.
- Aufgabe: aus `--task <datei>` (Freitext-Prompt) ODER eine eingebaute
  Default-Aufgabe (klein, deterministisch verifizierbar).
- Live-Fortschritt: `[benchmark] qwen runde 1/N: run… build… test… -> pass`.
- Ausgabe am Ende: Code-Rangliste (brain | attempts | compile% | pass% | wilson).
- Ergebnis zusätzlich als Wiki-Seite `code-benchmark-<stamp>`.

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
