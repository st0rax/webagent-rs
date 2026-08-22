# Aktueller Arbeitsstand

**Aktualisiert:** 2026-08-22
**Zweck:** kurze operative Übergabe. Nach jeder fertigen oder unterbrochenen
Entwicklungsscheibe aktualisieren. Git-Angaben vor Verwendung lokal prüfen.

## Zuletzt verifizierte Basis

- Produktbasis: `8e6ea50` auf `codex/project-takeover`, aufgebaut auf
  `origin/master` bei `9bf57f5`.
- Seit der Übergabe kamen hinzu: `3b7d935` (Phase-B-Ausführung injizierbar),
  `f51c31b` (Entwickler-Onboarding, von `codex/developer-onboarding`
  übernommen), `8e6ea50` (Vertrag der lebenden Dokumente nachgezogen) sowie
  dieser Dokumentationscommit.
- **Der Übergabecommit `ef23170` war selbst rot.** Er fügte sein eigenes
  Handover ohne Archivbanner hinzu und brach damit
  `startup::tests::betriebs_markdown_hat_eine_wahrheit`. Die dort notierte
  Evidenz wurde auf `2cf952e` erhoben und galt für `ef23170` nie. `f51c31b`
  reparierte das Dokument, verletzte den Vertrag aber an fünf weiteren Stellen;
  `8e6ea50` führt ihn vollständig nach. Lehre: fremde Evidenz nachmessen, und
  eine Evidenztabelle nie in denselben Commit schreiben, den sie belegt.
- Der Arbeitsbranch wird nach GitHub gepusht, damit der aktuelle Stand jederzeit
  ziehbar ist. Grüne Scheiben merged der Integrator selbst über einen PR; Tag,
  Release und Deployment führt er nach den Abnahmebelegen ebenfalls aus. Nur die
  Anmeldung im Live-Lauf nimmt der Eigentümer selbst vor.

Die datierten Details bleiben in
`HANDOVER_FROM_CODEX_TO_CLAUDE_2026-08-22.md` erhalten. Für künftige operative
Updates ersetzt diese Datei die alte Übergabe.

## Offene Baustelle: beschädigtes Master-Profil

**Stand 2026-08-22 23:00:** `profiles/shared` trägt nur noch `z.ai`. Ein
`login-all` über neun Brains hat die Cookie-Datenbank neunmal überschrieben —
der letzte Brain gewinnt. Ursache und Reparatur des Mechanismus stehen in
`db8e357`; **der Datenschaden selbst ist nicht behoben.**

Nicht betroffen: die neun kanonischen Profile unter `profiles/<brain>` sind
vollständig und aktuell. `verify`, `login` und `probe` arbeiten auf ihnen und
funktionieren. Betroffen sind nur Pool, TUI und Benchmark, die aus dem Master
klonen.

Drei Wege, bewusst noch nicht gegangen:

| Weg | Ergebnis | Kosten |
|---|---|---|
| Sicherung `shared.bak-20260807-153635` zurückspielen | alle neun Brains, aber Sitzungen vom 07.08. | keine; Sitzungen wahrscheinlich rotiert und tot |
| `WEBAGENT_USE_SHARED_BROWSER=1 login-all` | korrektes, aktuelles Master | neun Anmeldungen von Hand |
| nichts tun | Master bleibt unbrauchbar | keine, solange Pool/TUI/Bench ruhen |

Der Shared-Betrieb ist der einzige Weg, der ein vollständiges Master erzeugt:
Nur dort laufen alle Anmeldungen von vornherein in dieselbe Cookie-Datenbank.
Eine Spiegelung einzelner Profile kann das prinzipiell nicht leisten, weil
Chromium die Cookies mit einem Schlüssel aus dem `Local State` desselben
Profils verschlüsselt.

## Kimi: Umfirmierung auf kimi.ai

`chat` und `new_chat` bestehen wieder live. Drei Ursachen hatten sich
überlagert: die Domain (`BRAIN_TABLE` zeigte auf `kimi.com`), Selektoren, bei
denen Anmeldewand und Anmeldenachweis auf dasselbe Element zeigten, und eine
fehlende Gnadenfrist beim Schließen des Browsers.

Offen und **nicht** anmeldebezogen: `model_switch` findet keinen Selektor,
`stop_generation` ist über `--stop-diff` nicht auffindbar (55 Elemente vor und
nach der Generierung identisch — vermutlich zu spät gescannt, Kimi antwortet
in Sekunden), `projects` klappt nur eine Sidebar-Sektion auf statt zu
navigieren. Letzteres ist womöglich gar kein Defekt, sondern eine entfallene
Fähigkeit.

Beim Vermessen aufgefallen, nicht angefasst: Die kanonischen Profile sind
durchmischt — `claude`, `gemini`, `mistral` und `qwen` tragen ChatGPT- und
Kimi-Cookies bei identischen Byte-Längen, was auf Kopien statt Anmeldungen
hindeutet.

## Evidenz dieser Basis

| Gate | Ergebnis |
|---|---|
| fokussierte Write-back-Durability-Tests | 9 bestanden |
| `cargo test --no-default-features` | 1.104 Bibliotheks- + 7 Binärtests bestanden |
| `cargo test` | 1.170 Bibliotheks- + 7 Binärtests bestanden |
| striktes Headless-Clippy | bestanden |
| striktes Vollfeature-Clippy | bestanden |
| Format und `git diff --check` | bestanden |

Diese Ergebnisse belegen genau diesen Commit, nicht einen späteren Tree.

## Produktabnahme

Das Gesamtprojekt ist nicht fertig abgenommen. Der lokale Rust-Kern ist stark
getestet; folgende Systembelege fehlen:

1. providerfreier End-to-End-Lauf für Benchmark, Repair, Handoff und Harvest in
   einem echten temporären Git-Repo;
2. getrennte integrierte Abnahme für Pool, Heartbeat, Continuation,
   Cross-Brain-Handoff, Profil-Lease/Write-back und Shutdown;
3. aktuelle Live-Rezertifizierung von Provider/WebView, nur mit ausdrücklicher
   Eigentümerfreigabe;
4. bewusste Upstream-Integration und Releaseentscheidung.

Die vollständige Reifegradmatrix und Roadmap stehen in `OVERVIEW.md`.

## Sicherste nächste Produktscheibe

Die providerfreie Benchmark-/Harvest-Systemabnahme aus `OVERVIEW.md` umsetzen:

- ~~Phase-B-Brainausführung hinter einen crate-privaten injizierbaren Runner
  legen~~ — erledigt in `3b7d935` (`PhaseBRunner`, `BenchRunRequest`,
  `run_benchmark_with`); der Produktionspfad reicht unverändert an `bench_run`
  durch;
- ein geparstes, `Ready` bewertetes Work-Package in den echten Git-/Eval-/
  Harvestpfad führen — **Befund 2026-08-22:** `allowed_paths` erreicht den
  Harvest heute gar nicht. `validate_harvest_patch` prüft nur eine generische
  Policy (bestehende `.rs` unter `src/`, höchstens vier Dateien, keine
  Löschungen), und `validate_task_scope` leitet aus dem **Freitext** der
  Aufgabe einen einzigen erwarteten Funktionsnamen ab. Der typisierte
  `WorkPackage`-Scope wird nie konsultiert. Der geforderte Nachweis „Patch
  außerhalb `allowed_paths` wird fail-closed verworfen" kann deshalb noch gar
  nicht gelingen; die Scheibe muss den Scope zuerst bis in den Harvest
  durchreichen;
- ein temporäres Mini-Git-Repo verwenden;
- Fresh -> Continuation mit gleicher Run-ID -> Stall -> begrenztes Cross-Brain-
  Handoff belegen;
- Brain B einen technisch grünen Patch außerhalb `allowed_paths` erzeugen
  lassen;
- Fail-closed-Verwerfung, unveränderten HEAD, sauberen Worktree und erwartete
  Handoff-/Reject-Events belegen.

BrowserPool, echte Provider, Liveprofile und Release gehören nicht in diese
Scheibe.

## Aktiver Edit

Diesen Abschnitt vor Änderungen an gemeinsamen Dateien erneut verifizieren:

- **Entwickler:** Claude (vom Eigentümer als aktueller Produktintegrator benannt)
- **Branch/Worktree:** `codex/project-takeover` im Worktree
  `webagent-codex-takeover`
- **Aufgabe:** offenbar die priorisierte Benchmark-/Harvest-Scheibe; vor der
  Übernahme dieses Dokuments von Claude bestätigen
- **Eigene Dateien:** `src/benchmark/pipeline.rs`
- **Schmutzige Dateien:** `src/benchmark/pipeline.rs` war beim letzten
  read-only Check am 2026-08-22 modifiziert
- **Blocker:** keiner bekannt
- **Nächster Schritt:** Claude beendet oder checkpointet seine Scheibe und
  ersetzt diesen Abschnitt mit seinem exakten Commit- und Gatestand

Bei einem aktiven zweiten Entwickler einen eigenen Worktree und disjunkte
Dateien verwenden oder die Zuständigkeit vorher ausdrücklich abstimmen.

## Vorlage für den nächsten Checkpoint

Den vorherigen Checkpoint ersetzen, nicht ein unbegrenztes Log anhängen:

```text
Aktualisiert:
Entwickler:
Branch und Commit:
Ergebnis:
Geänderte Dateien:
Abnahmekriterien:
Ausgeführte Befehle und exaktes Ergebnis:
Bekannte Grenzen/Blocker:
Schmutzige Pfade und Eigentümer (oder "sauber"):
Genau eine sicherste nächste Aktion:
Weiterhin erforderliche externe Freigaben:
```

Diese Datei kurz halten. Lange Untersuchungshistorie gehört in ein datiertes
Dokument oder eine Commitnachricht; dauerhafte Produktwahrheit in `OVERVIEW.md`.
