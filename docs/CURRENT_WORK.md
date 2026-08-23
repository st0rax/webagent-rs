# Aktueller Arbeitsstand

**Aktualisiert:** 2026-08-23
**Zweck:** kurze operative Übergabe. Nach jeder fertigen oder unterbrochenen
Entwicklungsscheibe aktualisieren. Git-Angaben vor Verwendung lokal prüfen.

## Zuletzt verifizierte Basis

- Produktbasis: `51d196f` auf `codex/project-takeover`, aufgebaut auf
  `origin/master` bei `9bf57f5`. **CI grün** (beide Jobs, Run `32610085285`) —
  erstmals seit dem 22.08.
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

**Behoben am 2026-08-23.** `profiles/shared` wurde aus dem vollständigen
Klon `profiles/qwen` wiederhergestellt — mitsamt `Local State`, ohne den die
verschlüsselten Cookies unlesbar wären. Die Cookie-Datenbank trägt jetzt alle
neun Dienste mit Ablauf 2027; der alte Stand liegt als
`shared.bak-20260823-024140`. **Nicht belegt ist, dass die Sitzungen live
tragen**: `brains-health` prüft Selektoren und Profilverzeichnis, keinen Login.
Der Nachweis braucht einen Live-Lauf mit anwesendem Eigentümer.

Die Tabelle unten ist damit historisch. Sie hielt eine Spiegelung für
prinzipiell unmöglich — das galt für das Kopieren einzelner Cookie-Dateien,
nicht für die Übernahme des ganzen Profilverzeichnisses samt Schlüssel.

**Ursprünglicher Stand 2026-08-22 23:00:** `profiles/shared` trug nur noch
`z.ai`. Ein
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
- **Branch und Commit:** `codex/project-takeover` bei `51d196f`, gepusht
- **Ergebnis:** Der Datei-Scope reicht jetzt vom typisierten Auftrag bis ans
  Ernte-Tor. `de1959c` baute die Prüfung, `0b24ad0` verdrahtete sie in die
  Pipeline, `2557eee` belegte sie auf echtem Git, `ab32795` gibt Phase A.5 einen
  typisierten Auftrag — vorher war `allowed_paths` in Produktion immer leer.
  `51d196f` macht den Headless-Lint wieder grün.
- **Geänderte Dateien:** `src/benchmark/{pipeline,types,harvest,mod}.rs`,
  `src/{lib,brain_wall}.rs`, `src/commands/research.rs`, `src/tui.rs`
- **Ausgeführte Befehle:** `cargo fmt --all -- --check`, `cargo clippy
  --no-default-features --all-targets -- -D warnings`, `cargo clippy
  --all-targets -- -D warnings`, `cargo test --no-default-features`,
  `cargo test` — alle grün; 1.118 headless, 1.184 volle Tests. CI-Run
  `32610085285` grün.
- **Schmutzige Pfade:** sauber
- **Bekannte Grenzen:** Der Auftrag-Repair-Handoff-Harvest-Lauf über
  `run_benchmark_with` ist noch nicht als Ganzes belegt — die Brain-Choreografie
  (Vorschlag, Turnier, Treueprobe) fehlt im Test. Belegt ist das Scope-Tor
  einzeln, auf echtem Git.
- **Genau eine sicherste nächste Aktion:** einen `PhaseBRunner`-Doppelgänger
  bauen und den vollen Lauf durch `run_benchmark_with` fahren, mit einem Patch
  außerhalb `allowed_paths` als Abnahme.
- **Externe Freigaben:** Live-Browser, Login und kostenpflichtige Provider
  weiterhin nur mit anwesendem Eigentümer.

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
