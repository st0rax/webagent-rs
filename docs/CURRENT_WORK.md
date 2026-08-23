# Aktueller Arbeitsstand

**Aktualisiert:** 2026-08-23
**Zweck:** kurze operative Übergabe. Nach jeder fertigen oder unterbrochenen
Entwicklungsscheibe aktualisieren. Git-Angaben vor Verwendung lokal prüfen.

## Zuletzt verifizierte Basis

- Produktbasis: `9f46ec4` auf `codex/project-takeover`, aufgebaut auf
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
| `cargo test --no-default-features` | 1.123 Bibliotheks- + 7 Binärtests bestanden |
| `cargo test` | 1.189 Bibliotheks- + 7 Binärtests bestanden |
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

## Benchmark-/Harvest-Systemabnahme — erbracht

Die Scheibe aus `OVERVIEW.md` ist providerfrei belegt. `src/benchmark/e2e_tests.rs`
faehrt die volle Pipeline auf einem echten temporaeren Git-Repo mit echten
Eval-Kommandos; nur die Brain-Ausfuehrung ist gedoppelt (Phase A ueber `query`,
Phase B ueber `PhaseBRunner`).

| Gefordert | Beleg |
|---|---|
| Phase B injizierbar | `3b7d935` (`PhaseBRunner`, `run_benchmark_with`) |
| Scope bis in den Harvest | `de1959c`, `0b24ad0`, `ab32795` |
| temporaeres Mini-Git-Repo | `e2e_tests::repo` |
| Ernte eines gueltigen Patches | `e2e_lauf_in_scope_wird_geerntet` |
| Fail-closed ausserhalb `allowed_paths` | `e2e_patch_ausserhalb_scope_wird_fail_closed_verworfen` |
| unveraenderter HEAD, sauberer Worktree | ebenda, beide zugesichert |
| Stall und begrenztes Cross-Brain-Handoff | `e2e_stall_fuehrt_zu_cross_brain_handoff` |
| Fresh → Continuation mit gleicher Run-ID | `e2e_zweite_iteration_ist_continuation_derselben_run_id` |

Zwei Dinge, die dabei gelernt wurden und die kuenftige Nachweise betreffen:

**Ein Ablehnungsnachweis braucht seinen Gegenprobe-Test.** Der erste
Out-of-Scope-Patch fuegte eine oeffentliche Funktion hinzu und wurde schon von
`validate_task_scope` am Aufgaben-Freitext verworfen — der Test war gruen und
belegte ueber den Datei-Scope nichts. Er aendert jetzt nur einen bestehenden
Funktionsrumpf, und `ohne_typisierten_auftrag_passiert_derselbe_patch` faehrt
denselben Patch ohne Scope und verlangt, dass er durchgeht. Faellt diese
Gegenprobe, ist die Abnahme wertlos, nicht die Gegenprobe kaputt.

**Der Handoff ist reihenfolgeabhaengig.** `assign_tasks` rotiert um die
Rundennummer; laeuft das bauende Brain zuerst, ist die Aufgabe geloest, bevor
ein Stall eintreten kann, und es gibt nichts weiterzureichen. Der Test setzt
die Brainliste deshalb bewusst so, dass das stille Brain zuerst zieht.

**Die Laeufe sind serialisiert.** Sie teilen sich prozessweiten Zustand —
Statistik-Store, Ablage der Erntekandidaten, Ereignisbus. Parallel gefahren war
`e2e_lauf_in_scope_wird_geerntet` sporadisch rot und allein immer gruen. Ein
`static SERIELL` in `e2e_tests.rs` haelt sie auseinander; wer dort einen Test
ergaenzt, muss die Sperre mitnehmen.

## Aktiver Edit

Diesen Abschnitt vor Änderungen an gemeinsamen Dateien erneut verifizieren:

- **Entwickler:** Claude (vom Eigentümer als aktueller Produktintegrator benannt)
- **Branch und Commit:** `codex/project-takeover` bei `9f46ec4`, gepusht
- **Ergebnis:** Die Benchmark-/Harvest-Systemabnahme (Roadmap 3) ist erbracht.
  Der Datei-Scope reicht vom typisierten Auftrag bis ans Ernte-Tor
  (`de1959c`, `0b24ad0`, `ab32795`), belegt auf echtem Git (`2557eee`) und im
  vollen Lauf (`5ed8839`, `7e0357d`). `51d196f` hat die seit dem 22.08. rote CI
  repariert.
- **Geänderte Dateien:** `src/benchmark/{pipeline,types,harvest,mod,e2e_tests}.rs`,
  `src/{lib,brain_wall}.rs`, `src/commands/research.rs`, `src/tui.rs`,
  `docs/{CURRENT_WORK,OVERVIEW}.md`
- **Ausgeführte Befehle:** `cargo fmt --all -- --check`, striktes Clippy mit und
  ohne Defaultfeatures, `cargo test --no-default-features` (1.123 + 7),
  `cargo test` (1.189 + 7) — alle grün. Vier aufeinanderfolgende
  Headless-Gesamtläufe grün (Flakiness-Prüfung).
- **Schmutzige Pfade:** sauber
- **Bekannte Grenzen:** Roadmap 1 (Live-Rezertifizierung) und der Rest von
  Roadmap 2 (Poolstart, Heartbeat, Profil-Lease/Write-back, Shutdown im
  gemeinsamen Szenariolauf) sind NICHT erbracht. Beide brauchen entweder den
  anwesenden Eigentümer oder eine Injektionsschicht für den `BrowserPool` —
  ein Eingriff in genau den Profil-/Browserpfad, der am 22.08. das Master-Profil
  zerstört hat, und ohne Livelauf nicht verifizierbar. Deshalb bewusst nicht
  autonom begonnen.
- **Genau eine sicherste nächste Aktion:** Die grüne Scheibe als PR nach
  `master` integrieren — `master` ist aus demselben Lint-Grund noch rot, den
  `51d196f` behebt.
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
