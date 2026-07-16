# Autoresearch — Implementierungsplan

> **STATUS: GEPLANT, NICHT IMPLEMENTIERT.**
> Dieses Dokument ist eine vollständige Spezifikation für ein Feature, das noch
> nicht existiert. Es ist bewusst so geschrieben, dass jemand anderes (Mensch
> oder ein anderer Claude/Brain) direkt damit anfangen kann, ohne die
> Ursprungs-Konversation zu kennen. Nichts hier ist bereits in `src/`
> umgesetzt — vor dem Start `git grep -n autoresearch src/` laufen lassen, um
> zu prüfen, ob sich das inzwischen geändert hat.

---

## 1. Idee in einem Satz

Andrej Karpathys [`autoresearch`](https://github.com/karpathy/autoresearch)-Muster
(eine `program.md`, die einen Coding-Agenten anweist: **Modify → Verify →
Keep/Discard → Repeat**, unbeaufsichtigt über Nacht) auf webagent-rs selbst
anwendbar machen — ein Modus, der eine messbare Metrik (Testquote,
Flakiness-Rate, `/score`-Reliability, was auch immer) autonom über mehrere
Iterationen verbessert, mit Git als Sicherheitsnetz (jede Iteration ist ein
Commit oder ein vollständiger Revert) und den bereits vorhandenen
Schutzmechanismen des Projekts (`shell_policy`, `loop_guard`-Muster).

**Warum das zu webagent-rs passt:** Der Controller-Loop (Plan/Act/Observe),
die Shell-Policy und ein Brain-Backend existieren schon — das "Modify" aus
Karpathys Muster ist im Grunde ein normaler `controller.run()`-Aufruf mit
einem engen Task. Neu ist nur die **äußere** Schleife (Metrik messen, behalten
oder verwerfen, wiederholen) plus die Git-Mechanik dafür.

## 2. Quelle / Herkunft

- [`github.com/karpathy/autoresearch`](https://github.com/karpathy/autoresearch) — Original, 2026-03-07, einzelne `program.md`.
- Mehrere Community-Ports als installierbare Claude-Code-Skills (z.B.
  `github.com/uditgoenka/autoresearch`) — verallgemeinern das Muster über
  ML-Training hinaus auf "alles mit einer messbaren Zahl".
- Kernmechanik dort: Agent editiert **eine** Datei, lässt einen fixen
  Zeitraum laufen, prüft ob die Metrik sich verbessert hat, behält die
  Änderung oder verwirft sie, wiederholt — protokolliert in einem eigenen
  `log.md`.

## 3. Mapping auf vorhandene webagent-rs-Bausteine

| Karpathy-Konzept | webagent-rs-Baustein | Status |
|---|---|---|
| Coding-Agent, der editiert | `controller::AgentController` + ein `BrainBackend` | existiert schon, wiederverwendbar |
| Shell-Ausführung der Änderung | `executor::ShellExecutor` | existiert schon |
| Schutz vor destruktiven Edits | `shell_policy::evaluate` | existiert schon, greift automatisch |
| "5 Minuten laufen lassen, Metrik prüfen" | — | **neu**: `eval_metric()` |
| "behalten oder verwerfen" | — | **neu**: Git-Commit/Revert-Mechanik |
| Wiederholen mit Abbruch-Bedingung | Muster wie `circuit_breaker`s `consecutive_failures` | **neu**, aber gleiche Form wie Bestehendes |
| `log.md`-Protokoll | Muster wie `memory.rs`/`brain_score.rs` (JSON-Lines + lesbares Log) | **neu**, gleiche Konvention |

## 4. Kernschleife (Pseudocode)

```text
fn run(config: AutoResearchConfig) -> AutoResearchReport:
    require git_status_clean(config.workdir)          # sonst: harter Abbruch
    branch = create_and_checkout(f"autoresearch/{timestamp}")
    baseline = eval_metric(config.eval_cmd)?           # muss beim Start funktionieren
    history: Vec<IterationLog> = []
    no_improve_streak = 0

    for i in 1..=config.max_iterations:
        start_sha = git_head_sha()
        task = build_modify_prompt(config.goal, baseline, history)  # siehe §10
        run_result = controller.run(task, config.brain_id, resume=None, config.headless)
        # run_result kann fehlschlagen (Brain-Fehler, Timeout) -> zaehlt als "nicht verbessert"

        new_metric = eval_metric(config.eval_cmd)          # Fehler/Nicht-Parsebar -> wie "nicht verbessert"
        improved = compare(new_metric, baseline, config.direction)

        if improved:
            sha = git_commit_all(f"autoresearch: iteration {i}, {baseline} -> {new_metric}")
            baseline = new_metric
            no_improve_streak = 0
            history.push(IterationLog{i, ..., kept: true, commit_sha: Some(sha)})
        else:
            git_reset_hard(start_sha)                      # kompletter Revert dieser Iteration
            no_improve_streak += 1
            history.push(IterationLog{i, ..., kept: false, commit_sha: None})

        append_log_md(config.log_path, history.last())
        if no_improve_streak >= config.no_improve_abort:
            break                                           # frueher Abbruch, kein Endlos-Nutzlos-Loop

    return AutoResearchReport{branch, baseline, history}
```

Wichtig: **jede Iteration startet von einem garantiert sauberen Zustand**
(entweder dem ursprünglichen `start_sha` oder dem zuletzt behaltenen Commit).
`git reset --hard start_sha` ist hier bewusst gewählt statt `git checkout --
.` + `git clean -fd`, weil es unabhängig davon korrekt ist, ob der Brain neue
Dateien angelegt, bestehende geändert oder welche gelöscht hat — ein
einzelner, robuster Mechanismus statt mehrerer Sonderfälle.

## 5. Neues Modul: `src/autoresearch.rs`

Vorschlag für die öffentliche API — so geschrieben, dass direkt gegen diese
Signaturen implementiert werden kann:

```rust
pub enum Direction { HigherIsBetter, LowerIsBetter }

pub struct AutoResearchConfig {
    pub brain_id: String,
    pub goal: String,
    pub eval_cmd: String,
    pub direction: Direction,
    pub max_iterations: usize,      // Default-Vorschlag: 10
    pub no_improve_abort: usize,    // Default-Vorschlag: 3 (siehe circuit_breaker-Analogie)
    pub headless: bool,
    pub workdir: PathBuf,           // Git-Repo-Root, in dem gearbeitet wird
}

pub struct IterationLog {
    pub n: usize,
    pub metric_before: f64,
    pub metric_after: Option<f64>,  // None wenn eval_cmd fehlschlug
    pub kept: bool,
    pub commit_sha: Option<String>,
    pub brain_summary: String,      // letzte Message/finish-Text aus dem Run
    pub ts: String,                 // now_rfc3339()
}

pub struct AutoResearchReport {
    pub branch: String,
    pub final_metric: f64,
    pub iterations: Vec<IterationLog>,
    pub stopped_reason: String,     // "max_iterations" | "no_improve_abort" | "eval_failed_at_start"
}

pub fn run(config: AutoResearchConfig) -> Result<AutoResearchReport, String>;

// Innere Bausteine, einzeln testbar:
fn eval_metric(cmd: &str) -> Result<f64, String>;
fn git_status_clean(workdir: &Path) -> Result<bool, String>;
fn git_head_sha(workdir: &Path) -> Result<String, String>;
fn git_create_branch(workdir: &Path, name: &str) -> Result<(), String>;
fn git_commit_all(workdir: &Path, message: &str) -> Result<String, String>; // -> commit sha
fn git_reset_hard(workdir: &Path, sha: &str) -> Result<(), String>;
fn build_modify_prompt(goal: &str, current_metric: f64, history: &[IterationLog]) -> String;
fn append_log_md(path: &Path, entry: &IterationLog) -> Result<(), String>;
```

Alle `git_*`-Funktionen laufen über `std::process::Command::new("git")` direkt
— **nicht** über `executor::ShellExecutor`/`shell_policy`, weil das
Sicherheits-Commits des Tools selbst sind, keine Brain-Aktionen. Der Brain
bekommt seine Shell-Zugriffe weiterhin ausschließlich über den normalen
Controller-Pfad (inkl. `shell_policy`-Gate).

## 6. CLI- und REPL-Oberfläche

**CLI-Subcommand** (in `main.rs`, gleiches Muster wie `Run`/`Relay`):

```
webagent autoresearch --brain <id> --goal "<text>" --eval "<shell-cmd>"
                       [--direction higher|lower]      (Default: higher)
                       [--max-iterations N]             (Default: 10)
                       [--no-improve-abort N]            (Default: 3)
                       [--headless]
                       [--workdir <pfad>]                (Default: aktuelles Repo-Root)
```

Das ist der primäre Anwendungsfall ("über Nacht laufen lassen, morgens
Ergebnis prüfen") — läuft als eigener Prozess, lässt sich vom Nutzer selbst
im Hintergrund starten/planen.

**REPL-Befehl** `/autoresearch <goal>` — nutzt das bereits aktive Brain der
Session (wie `/swarm`/`/chat`), mit kleineren Defaults für interaktives
Ausprobieren (z.B. `max_iterations=3`), druckt Fortschritt live wie
`run_swarm` es tut (`[autoresearch i/N] metrik: X -> Y (behalten|verworfen)`).

## 7. Eval-Vertrag

Der `--eval`-Befehl **muss**:
- exit code 0 bei Erfolg liefern,
- als **letzte Zeile** von stdout eine reine Zahl ausgeben (f64-parsebar).

Alles andere (Nicht-Null-Exit, keine parsebare letzte Zeile) zählt als
fehlgeschlagene Iteration — kein Absturz der Schleife, sondern wie ein
"nicht verbessert"-Ergebnis (führt zu Revert + `no_improve_streak += 1`).

Beispiele für sinnvolle `--eval`-Befehle in diesem Repo:
- `cargo test --lib 2>&1 | grep -oP '\d+(?= passed)'` (höher = besser)
- `cargo build 2>&1 | grep -c warning` (niedriger = besser)
- ein kleines Skript, das `crate::brain_score::leaderboard()` abfragt und den
  Durchschnitt der Reliability-Werte ausgibt (höher = besser) — würde ein
  eigenes kleines Binary/Beispiel brauchen, das die Lib-Funktion aufruft.

## 8. Sicherheitsmodell

- **Kein Start auf schmutzigem Working Tree.** `git_status_clean()` prüft
  zuerst; bei uncommitted changes sofortiger Abbruch mit klarer Fehlermeldung
  — nie den Nutzer-Zustand stashen oder überschreiben.
- **Immer auf einem eigenen Branch** (`autoresearch/<timestamp>`), nie direkt
  auf `master`/`main`. Der Nutzer muss das Ergebnis explizit selbst mergen —
  kein automatisches Push, kein automatischer Merge, keine PR-Erstellung.
  (Konsistent mit den generellen Regeln dieses Environments: destruktive/
  weitreichende Git-Operationen sind nie automatisch.)
- **Bounded**: `max_iterations` als harte Obergrenze, `no_improve_abort` als
  Circuit-Breaker-artiger früher Ausstieg (gleiche Form wie
  `circuit_breaker.rs`s `consecutive_failures`).
- **Shell-Policy greift automatisch**: Da "Modify" über den normalen
  Controller-Pfad läuft, prüft `shell_policy::evaluate` jeden vom Brain
  vorgeschlagenen Shell-Befehl wie sonst auch — keine neue Angriffsfläche.
- **Revert ist ein Full-Reset**, kein partielles Aufräumen (siehe §4-Begründung).

## 9. Logging / Datenmodell

- `data/autoresearch/<run_id>/log.md` — menschenlesbar, ein Eintrag pro
  Iteration (Nummer, Zeitstempel, Metrik vorher/nachher, behalten/verworfen,
  Commit-SHA falls behalten, eine Zeile Zusammenfassung was der Brain
  versucht hat). Spiegelt Karpathys eigenes `log.md`-Muster.
- `data/autoresearch/<run_id>/iterations.jsonl` — maschinenlesbar, ein
  `IterationLog` pro Zeile, gleiche JSON-Lines-Konvention wie
  `memory.rs`/`brain_score.rs`/`circuit_breaker.rs`.

## 10. Offene Design-Fragen (bewusst nicht vorentschieden)

- **Prompt-Template für "Modify"**: braucht Tuning/Iteration, nicht hier
  festgelegt. Muss mindestens enthalten: das Ziel (`goal`), den aktuellen
  Metrikwert, und (empfohlen) eine Kurzfassung der letzten K Iterationen
  (damit der Brain nicht wiederholt dieselbe erfolglose Änderung versucht).
- **Wie viele Controller-Actions pro Iteration?** Eine Iteration = ein
  `controller.run()`-Aufruf mit `max_cycles` klein (z.B. 3-5) hält den Brain
  auf "eine fokussierte Änderung" statt einer offenen Aufgabe. Exakter Wert
  noch offen.
- **Eval-Befehl-Timeout**: `eval_cmd` selbst braucht ein Timeout (z.B. via
  denselben Mechanismus wie `run_command_with_timeout` in `main.rs`), sonst
  kann ein hängender Eval-Befehl die ganze Schleife blockieren.
- **Anbindung an `brain_score.rs`**: naheliegend, jede Iteration zusätzlich
  als Erfolg/Fehlschlag-Event zu loggen (`kept` = Erfolg, `verworfen` =
  Fehlschlag) — günstige Synergie, aber nicht Teil des MVP, weil es die
  Reliability-Semantik von brain_score (Session-übergreifende
  Brain-Zuverlässigkeit) mit einer anderen Bedeutung (Erfolg *dieser
  Änderung*) vermischen würde. Erst entscheiden, wenn brain_score in der
  Praxis genutzt wird.

## 11. Teststrategie

- Reine Unit-Tests für `eval_metric()` (Parsing-Edge-Cases: leere Ausgabe,
  keine Zahl, negative Zahl, Exit-Code != 0).
- Git-Helfer gegen ein `tempdir` + `git init` in Tests (kein Live-Repo nötig)
  — Muster wie `unique_data_dir()` in `controller.rs`s Tests.
- Kernschleife mit einem **gemockten** "Modify"-Schritt (Trait/Closure statt
  echtem `AgentController`) und gemocktem `eval_metric`, um Keep/Discard/
  Abbruch-Logik ohne echten Browser/Brain zu testen — Muster wie
  `MockExecutor`/`MockBrain` in `controller.rs`.
- Manueller Livetest zuletzt: gegen webagent-rs selbst, trivialem Ziel
  ("reduziere Compiler-Warnings"), auf einem Wegwerf-Branch, um Commit-/
  Revert-Verhalten *wirklich* zu sehen, bevor man es für etwas Ernsthaftes
  nutzt.

## 12. Phasenweise Umsetzung (Reihenfolge für Einstieg)

1. `autoresearch.rs`: `eval_metric()` + Parsing, unit-getestet. Noch kein Git,
   noch kein Brain.
2. Git-Helfer (`git_status_clean`, `git_head_sha`, `git_commit_all`,
   `git_reset_hard`, `git_create_branch`), unit-getestet gegen ein
   Tempdir-Repo.
3. Kernschleife gegen einen **gemockten** Modify-Schritt verdrahten — Keep/
   Discard/Abbruch-Kontrollfluss vollständig testbar, ohne echten Brain.
4. Echten Modify-Schritt über `controller::AgentController` anbinden
   (wiederverwendet `run_with_options` wie `repl.rs`s `run_autonomous`).
5. CLI-Subcommand `webagent autoresearch`.
6. REPL-Befehl `/autoresearch <goal>`.
7. Livetest gegen webagent-rs selbst (trivialer Fall, Wegwerf-Branch).
8. README/Doku ergänzen.

## 13. Explizit außerhalb des Scopes für v1

- Mehrere Brains parallel experimentieren lassen (Swarm-artiges
  Autoresearch) — später, falls v1 sich bewährt.
- Automatisches Push/PR/Merge — bleibt manuell.
- Verteilte/Mehrmaschinen-Läufe — v1 ist single-machine.
- Anbindung an `brain_score.rs` — siehe §10, bewusst zurückgestellt.
