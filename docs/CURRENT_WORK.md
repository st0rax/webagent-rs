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

## Live-Rezertifizierung 2026-08-23 (Roadmap 1)

Mit anwesendem Eigentümer gemessen, sichtbarer Browser, kanonische
Per-Brain-Profile (`shared_browser=false`).

### Diagnose — 9/9

Alle neun Brains: `logged_in=true`, `session_state=Ready`, `composer=ok`, keine
Cloudflare-Wand. **Keine einzige Anmeldung war nötig.**

Damit ist auch belegt, dass die Wiederherstellung des am 22.08. beschädigten
Master-Profils trägt. `brains-health` konnte das nicht zeigen — es prüft
Selektordateien und Profilverzeichnis, nie einen Login.

### Relay (send+wait gegen echten Browser) — 8/9

| Brain | Ergebnis | Latenz |
|---|---|---|
| chatgpt | BEREIT | 20,4 s |
| claude | BEREIT | 17,8 s |
| deepseek | BEREIT | 15,5 s |
| gemini | BEREIT | 17,4 s |
| kimi | BEREIT | 25,6 s |
| mistral | BEREIT (mit UI-Beiwerk) | 24,2 s |
| perplexity | **Fehlschlag** | Timeout |
| qwen | BEREIT | 30,6 s |
| zai | BEREIT (mit UI-Beiwerk) | 24,4 s im 2. Anlauf |

`claude` und `mistral` hatten vorher **nie** einen Relay-Beleg; jetzt haben ihn
acht von neun. Die Belege stehen in `data/capability/proofs.jsonl`.

`zai` scheiterte im ersten Lauf nach 193 s und bestand den zweiten in 24 s —
transient, kein Defekt, aber als Flackern notiert statt geglättet.

### Defekt: perplexity liefert, wir sehen es nicht

Der Eigentümer beobachtete am Bildschirm, dass perplexity längst geantwortet
hatte, während der Relay-Lauf weiter wartete. Der Fehler liegt also bei uns,
nicht beim Anbieter.

`selectors/perplexity.json` enthält ausschliesslich `model_menu` und
`model_option` — keinen `assistant_message`-Selektor. Die generische Maske
(sieben Kandidaten von `[data-message-author-role='assistant']` bis
`div.markdown`) greift bei der heutigen Oberfläche nicht. `probe --brain
perplexity` findet Composer, Modellmenü, Projekte und Anhang-Knopf, aber
**weder `assistant_message` noch `stop_button`** — auch nicht mit
`--generating`, also während eine Antwort läuft.

Folge: Der Lauf wartet auf eine Antwort, die er nicht sehen kann, bis der
Timeout greift. Die Reparatur braucht manuelles DOM-Studium der
Perplexity-Oberfläche und ist eine eigene Scheibe.

### Run-Nachweis — bestanden

Der volle Controller-Loop (Plan → Act → Observe) gegen ein echtes Brain,
in einem isolierten Workspace ausserhalb des Repos:

    cd %TEMP%/webagent-run-nachweis
    webagent.exe run --brain deepseek --max-cycles 8 --no-memory       --task "Lege nachweis.txt mit genau dem Wort BEREIT an und pruefe danach
              per Shell, dass Datei und Inhalt stimmen."

Ergebnis: `status=done`, 3 Zyklen. Das Brain schickte eine `write`-Action, dann
eine `shell`-Action zur Selbstprüfung (`EXISTS` + `CONTENT_OK`), dann die
Abschlussmeldung.

Die Behauptung des Brains wurde NICHT geglaubt, sondern nachgemessen: die Datei
liegt mit exakt 6 Byte und dem Inhalt `BEREIT` auf Platte, ohne Zeilenumbruch
oder verborgene Zeichen (`od -c`). Ein Agent, der Erfolg meldet, ist kein
Beleg — die Platte ist einer.

Der Workspace lag bewusst unter `%TEMP%`: `cmd_run` startet die Shell im
aktuellen Verzeichnis, ein Lauf aus dem Repo heraus hätte das Brain im
Projektbaum arbeiten lassen.

### Befund: mistral und zai liefern UI-Beiwerk mit

`mistral` antwortete `BEREIT

4:42
War das hilfreich?
Überspringen`,
`zai` mit vorangestelltem `Thought Process`. Beide Läufe gelten als bestanden,
der extrahierte Text stammt aber nicht mehr allein vom Modell. Bei `mistral`
ist die Ursache belegt: `selectors/mistral.json` endet unter
`assistant_message` auf `div.prose`, einen sehr breiten Fallback, der
Nachbarelemente einsammelt, weil die spezifischen Selektoren davor nicht mehr
greifen. Für den bot2bot-Betrieb ist das relevant.

## Mehr-Brain-Abnahme (Roadmap 2) — Teilnachweis 2026-08-23

Zwei reale Läufe mit anwesendem Eigentümer, Master-Profil vorher gesichert
(`shared.bak-vor-pool-20260823-051522`, 531 MB).

**Belegt:**

- Poolstart und Auto-Recovery: `workers --active 2 --brains deepseek,qwen`
  startete den Supervisor, ein als unavailable geführtes Brain wurde nach
  Ablauf der Sperre selbsttätig wieder aufgenommen.
- Profil-Lease über neun Brains: `autoresearch-self` zog für jedes Brain einen
  Klon (`profiles/swarm/<runstamp>_<brain>_<hash>`) und fuhr sie im
  Parallelbetrieb, vier gleichzeitig.
- Acht der neun Brains antworteten — unabhängige Bestätigung des
  perplexity-Defekts aus der Relay-Matrix.
- **Der reparierte Write-back hält.** Das Master-Profil war nach BEIDEN Läufen
  unverändert: 23 Hosts, jede Cookie-Zahl identisch, byteweise gegen die
  vorher gezogene Referenz verglichen. Das ist die erste Live-Erprobung des
  Mechanismus seit dem Schaden vom 22.08.

**Nach diesen zwei Läufen noch offen — inzwischen alles nachgeholt, siehe
unten:** geordneter Shutdown, Write-back und Heartbeat unter Last. Beide Läufe
endeten am Timeout; dass das Master dabei heil blieb, belegte die Schutzlogik,
nicht den Rückweg. Same-Brain-Continuation und Cross-Brain-Handoff sind im
Benchmarkpfad belegt (`e2e_tests.rs`), nicht im Pool-Kontext.

## Der Write-back ist belegt (2026-08-23, 07:25 UTC)

Der Rückweg ins Master-Profil funktioniert. Gemessen an einem REPL-Lauf im
Shared-Modus mit geschlossenem stdin:

| | vorher | nachher |
|---|---|---|
| Hosts im Master | 23 | 33 |
| Cookies | 102 | 144 |
| `COMPASS` (gemini-Sitzungsnachweis) | fehlt | vorhanden |
| Zeitstempel der Cookie-DB | — | 07:25:51, also während des Laufs |

Das Master wurde **erweitert, nicht ersetzt**: Kein Host verschwand, keine
Cookie-Zahl sank. Genau das ist das Verhalten, das der Schaden vom 22.08.
vermissen liess, als ein `login-all` die Datenbank neunmal überschrieb und der
letzte Brain gewann.

**Der Weg dorthin, weil er nicht offensichtlich ist.** `stop_brain` löst beim
letzten Tab `shutdown_shared_runtime_with_result` aus — den Write-back. Er
läuft aber nur, wenn `persist_browser_tabs()` false ist, und die Funktion
liefert im Shared-Modus per Default **true**. Ohne `WEBAGENT_PERSIST_TABS=0`
bleibt der Tab offen, der Teardown unterbleibt und der Rückweg wird nie
gegangen. Ein Lauf, der nur den Supervisor fährt oder gar keine Anfrage stellt,
öffnet ausserdem keinen Tab — dann gibt es nichts zu stoppen.

## Worker-Heartbeat unter Last (2026-08-23, 05:35 UTC)

Der letzte offene Punkt aus Roadmap 2. Eine echte Aufgabe wurde in
`agents/qwen/inbox/` abgelegt und von einem laufenden `workers`-Supervisor
abgeholt:

- `heartbeat_qwen.json` wird bei JEDEM Poll neu geschrieben und tickte
  durchgehend (05:33:02 → 05:35:10), auch während der Worker arbeitete;
- die Nachricht wanderte aus der Inbox nach `_read/`;
- `state.json` führt sie unter `processed`, mit `registered`, `last_seen` und
  `last_lineage: "claude -> qwen"`.

Eine Randnotiz, die Zeit gekostet hat: Der erste Versuch blieb liegen, weil das
Nachrichtenformat nicht stimmte. `Msg::parse` verlangt die Header `From`, `To`
und `Time` in genau dieser Schreibweise, gefolgt von einer Leerzeile. Eine
Datei mit abweichenden Headern wird stillschweigend übersprungen — sie bleibt
in der Inbox liegen, ohne Meldung, und sieht von aussen aus wie ein hängender
Worker.

Damit sind für Roadmap 2 belegt: Poolstart, Auto-Recovery, Profil-Lease über
neun Brains, Parallelbetrieb, geordneter Shutdown, der Write-back ins Master
und der Worker-Heartbeat unter Last. Same-Brain-Continuation und
Cross-Brain-Handoff sind im Benchmarkpfad belegt (`e2e_tests.rs`).

**Weiterhin gültig: die TUI lässt sich nicht fernsteuernd beenden.** Der
Write-back oben lief über die REPL, nicht über die TUI. Für die TUI bleibt es
dabei:

**Befund: die TUI lässt sich nicht fernsteuernd beenden.** Ein dritter Lauf
(`tui --active 1 --brains deepseek` mit `WEBAGENT_USE_SHARED_BROWSER=1`) sollte
den geordneten Shutdown auslösen. Die Datei-IPC `pool_control.json` kennt dafür
`stop` — "Supervisor sauber beenden". Die Datei wurde nachweislich abgeholt
(atomares Rename, sie verschwand), der Prozess lief aber weiter: `stop` beendet
den Worker-Supervisor, nicht die TUI-Hauptschleife. Die wartet auf `q` von der
Tastatur, und `taskkill` ohne `/F` weist sie ab.

Damit ist der Write-back-Pfad nicht automatisiert erreichbar. Wer den
Systemnachweis ohne Hand am Terminal führen will, braucht entweder ein
`stop`, das auch die TUI beendet, oder einen anderen Auslöser für
`BrowserPool::shutdown_with_result` — heute ruft nur die TUI ihn auf.

Immerhin: Das Master-Profil blieb auch bei diesem dritten Lauf unverändert,
diesmal sogar nach einem erzwungenen `taskkill /F` im Shared-Browser-Modus.


**Befund: abgebrochene Läufe lassen ihre Profil-Klone stehen.** Die beiden
Läufe hinterließen 780 MB in `profiles/swarm/`, ein weiterer Rest stammt vom
22.08. Bei sauberem Ende räumt der Lease auf, beim Abbruch nicht. Auf einer
chronisch vollen Platte ist das kein Schönheitsfehler.

Nebenbefund zur Kopierstrategie: Die Sparse-Klone lagen bei 24–51 MB
(perplexity 147 MB), die Klone des Pool-Laufs ohne `WEBAGENT_SPARSE_COPY` bei
107–166 MB. Der Lease-Lauf lief bewusst sparse, weil neun volle Klone à 531 MB
die Platte gesprengt hätten; der Standardpfad ist damit nicht mitgemessen.

## Korrektur 2026-08-23: das Master-Profil ist NICHT vollständig

Beim TUI-Lauf meldete der Harness von sich aus:

> Master kennt claude, perplexity, gemini nicht, obwohl die kanonischen Profile
> eingeloggt sind. Der Pool wuerde Login nötig melden trotz gueltiger Session.
> Abhilfe: login-all

Unabhängig nachgemessen (Rohbyte-Scan der Cookie-Datenbanken auf die
Nachweis-Cookies aus `SESSION_PROOF_COOKIES`):

| Brain | Nachweis-Cookie | kanonisch | Master |
|---|---|---|---|
| kimi | `kimi-auth` | ja | ja |
| chatgpt | `__Secure-next-auth.session-token` | ja | ja |
| mistral | `ory_session` | ja | ja |
| claude | `sessionKey` | ja | **nein** |
| perplexity | `__Secure-pplx.session` | ja | **nein** |
| gemini | `COMPASS` | ja | **nein** |

**Was das für die Wiederherstellung vom 23.08. heisst.** Der Klon aus
`profiles/qwen` hob das Master von "nur z.ai" auf sechs von neun Brains — aber
nicht auf neun. Ein kanonisches Profil trägt auch Cookies der Nachbarn aus
gemeinsamen Login-Runden; die *Domain* ist deshalb vorhanden, der eigene
Sitzungsnachweis aber nicht. Genau diese Unterscheidung macht
`master_missing_sessions_from_canonical`, und genau sie habe ich beim Zählen
der Cookie-Domains übersehen.

Damit gilt beides nebeneinander, was vorher wie ein Widerspruch aussah:

- Die **kanonischen Profile** sind vollständig und live belegt — Diagnose 9/9,
  Relay 8/9. `verify`, `login`, `probe`, `relay` und `diagnose` arbeiten auf
  ihnen und funktionieren.
- Das **Master** (`profiles/shared`) ist für den Shared-Betrieb (Pool, TUI,
  Benchmark) unvollständig. Dort würden claude, perplexity und gemini als
  "Login nötig" gemeldet, obwohl gültige Sitzungen existieren.

**Abhilfe, wenn der Shared-Betrieb gebraucht wird:** `login-all` im
Shared-Modus für diese drei Brains — Anmeldung durch den Eigentümer. Nicht neun
Anmeldungen, wie ich am 23.08. zuerst schätzte, und auch nicht null, wie ich
danach behauptete: drei.

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
- **Integriert:** PR #6 ist am 2026-08-23 nach `master` gemerged (`77c45d9`).
  Damit traegt `master` auch die CI-Reparatur aus `51d196f`.
- **Genau eine sicherste nächste Aktion:** Roadmap 1 — die Live-Rezertifizierung
  gemeinsam mit dem Eigentümer. Sie ist der einzige verbleibende Schritt, der
  ohne neuen Produktionscode auskommt, und blockiert Roadmap 4.
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
