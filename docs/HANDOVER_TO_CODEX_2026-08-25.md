# Projektübergabe an Codex: `webagent-rs`

> **Dokumentstatus:** Laufend gepflegte, dateibasierte Übergabe. Diese Fassung wurde am **21. August 2026** gegen den tatsächlich abgefragten Git- und Worktree-Zustand erstellt. Sie ist kein Ersatz für `AGENTS.md`, `STATUS_LIVE.md` oder den Quelltext; sie ist der Einstieg, der Codex ohne Chatverlauf handlungsfähig machen soll.
>
> **Wichtig:** Dieser Stand enthält eine **uncommittete Sicherheitsreparatur** im aktiven Worktree. Weder sie noch diese Übergabedatei dürfen als fertig, geprüft oder zur Veröffentlichung freigegeben gelten, bevor die unten genannten Restgates tatsächlich bestanden und dokumentiert sind.

## 1. Projektziel und Produktvision

`webagent-rs` ist ein lokaler, Windows-GNU-fähiger Rust-Harness für nachvollziehbare Browser- und Agentenautomatisierung. Er verbindet mehrere Browser-„Brains“ mit einer Plan–Act–Observe-Steuerung, reproduzierbaren Nachweisen, Profile-/Session-Schutz, Worker- und Benchmark-Topologie sowie einer CLI/TUI. Die zentrale Produktidee ist kein unkontrollierter Autopilot, sondern ein **messbarer, sicherer und auditierbarer lokaler Agentenbetrieb**.

Maßgebliche Szenarien sind die lokale Führung mehrerer Browser-Brains, die Ausführung und Beobachtung kleiner Agentenaufgaben, eine belastbare Session- und Profilbehandlung sowie Benchmark-/Evidenzarbeit. Die Kernabläufe sollen auch ohne WebView/TUI als Headless-CLI testbar bleiben. Die Free-Cloud-Erweiterung ist nur insofern Teil des Zielbilds, als sie kostenbewusste, erklärbare und zunächst lokale Adapterverträge bereitstellt.

Nicht zum gegenwärtigen Ziel gehören automatische kostenpflichtige Providerzugriffe, das Speichern oder Weitergeben von Credentials, eine unautorisierte Browser- oder Account-Automation, eine Cloud-SaaS-Bereitstellung oder die Behauptung einer produktiven Providerintegration ohne reale, freigegebene Tests.

> **„Produktionsreif“** bedeutet hier: Die lokal unterstützten Verträge sind funktional, testbar, dokumentiert, fail-closed an Datenverlustgrenzen und mit klaren Betriebsgrenzen versehen. **„Fertig“** bedeutet erst: alle erforderlichen Tests, Lints, Diffprüfungen und unabhängigen Reviews liegen als echte Evidenz vor; der Arbeitsbaum ist sauber; bekannte Restgrenzen sind sichtbar und kein externer Pfad wird fälschlich als implementiert behauptet.

| Akzeptanzdimension | Erforderlicher Nachweis |
|---|---|
| Funktionsvertrag | Reproduzierbare Tests und CLI-/Codepfade |
| Sicherheitsvertrag | Fail-closed Verhalten, Fehlerpfade und unabhängiges Review |
| Qualität | `cargo fmt --all -- --check`, `git diff --check`, passende Test- und Clippy-Läufe |
| Lieferung | Sauberer Worktree nach Commit; Remote- und Branchregeln erfüllt |
| Externe Grenzen | Provider-, Login-, Kosten- und Browserabhängigkeiten klar als solche markiert |

## 2. Aktueller Gesamtstand des 12-Punkte-Plans

Die folgenden Einträge unterscheiden bewusst zwischen **erledigt**, **teilweise erledigt**, **offen** und **blockiert**. Belege sind Commit-Hashes, Befehle und Dateien, nicht der frühere Chatverlauf.

| Nr. | Arbeitspaket | Status | Beleg / Erläuterung |
|---:|---|---|---|
| 1 | Zielvertrag, Baseline, Risiken und Zeitplan | **Erledigt** | Ziel- und Architekturartefakte; `goal_plan.rs` sowie `/home/ubuntu/webagent_goal_plan.md` in der damaligen Arbeitsumgebung |
| 2 | Rustfmt-Normalisierung und Testwurzel-Isolation | **Erledigt** | Commits `9dc14d6` und `cd2f5fc`; wiederverwendbare isolierte Testwurzeln |
| 3 | Verifizierte GitHub-Spiegelung | **Erledigt bis HEAD** | Zum Snapshot `HEAD = origin/master = ea6c9a811bf78e68e15236ef5ce111ced5514660` |
| 4 | Goal-Abschluss-Gates | **Erledigt** | Commit `36a1766`: Planbindung, Artefaktevidenz, Duplikat-/Fake-Hash-Abwehr |
| 5 | Lokales Agententeam und Goal-Scheibe | **Erledigt** | Inventar und Reviewrollen sind dokumentiert; lokale Entitäten wurden geprüft |
| 6 | Browser-/Profil-/Swarm-/Worker-Topologie | **Teilweise erledigt** | Commit `ea6c9a8` begrenzt Brain-Wall/Grid-Runtime auf passende Features. Aktuell offene **uncommittete** Write-back-Härtung, siehe Abschnitt 5 |
| 7 | API-Bridge, persistente Evidenz, Goal-/UI-Verträge | **Offen** | API-Bridge ist geplant; keine vollständige konsolidierte Abnahme |
| 8 | Free-Cloud-Textchat bis realer Adaptergrenze | **Teilweise erledigt** | Lokale Registry, Mock-Stream, Hub-Metadaten- und Health-/Breaker-Verträge implementiert; echter HTTP-/Provider-Adapter bewusst nicht umgesetzt, siehe `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` |
| 9 | Benchmark-/Autoresearch-/Evidenzpipeline | **Teilweise erledigt** | Bestehende Benchmark- und Proof-Module; vollständige reproduzierbare Produktionspipeline noch offen |
| 10 | Mehragentenreviews, adversariale Tests, lokale Gesamtabnahme | **Offen** | Einzelne Scheiben wurden reviewt; projektweite Endabnahme fehlt |
| 11 | Betriebsdokumentation, Architekturkarte, Meilensteine | **Teilweise erledigt** | Zahlreiche Architektur-/Statusdokumente vorhanden; diese Übergabe wurde am 21.08.2026 begonnen und ist fortzuschreiben |
| 12 | Finale Goal-Abnahme | **Offen** | Darf erst nach Abschluss der offenen Punkte erfolgen |

### Verbleibende externe Freigaben und menschliche Entscheidungen

Ein realer Hugging-Face- oder anderer Providertransport, Browser-/Space-Automation, Credential-Verwendung, Kostenrouten und Login-Aktionen benötigen eine ausdrückliche Freigabe. Eine weitere Entscheidung ist die Auflösung eines Regelkonflikts: Der Sitzungskontext verlangte bisher eine Fast-Forward-Spiegelung nach jeder geprüften Scheibe, während das aktuell gelesene `AGENTS.md` §5 für Push/Tag/Release eine ausdrückliche Ansage fordert. **Vor dem nächsten Push muss Codex diesen Konflikt anhand der neuesten Nutzervorgabe klären, nicht still vermuten.**

## 3. Repository- und Worktree-Landkarte

### Kanonischer Arbeitsort

| Eigenschaft | Wert |
|---|---|
| Maßgeblicher Worktree | `C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-final-verification` |
| Aktueller Zustand | Detached HEAD auf `ea6c9a811bf78e68e15236ef5ce111ced5514660`; drei uncommittete Konfigurationsdateien |
| Remote | `origin` → `https://github.com/st0rax/webagent-rs.git` |
| Vorgesehener Integrationsbranch | `master` / `origin/master` |
| Maßgebliche Regel | Nur dieser Worktree darf für die aktuelle Scheibe beschrieben werden |

Die Worktree-Inventur muss vor jeder Fortsetzung mit `git worktree list --porcelain` erneut erfasst werden. Während dieses Snapshots wurden folgende historische oder abweichende Worktrees beobachtet; sie sind **kein** Schreibziel für die aktuelle Arbeit:

| Pfad | HEAD beim Snapshot | Ref | Einordnung |
|---|---|---|---|
| `C:\Users\storax\Desktop\desktop.archiv\webagent\webagent-rs` | `cc77f380fcc2c7b171112d5f23751a01b2834d4f` | `evolution/supervised-harvest` | Historisch/archiviert |
| `C:\Users\storax\Desktop\desktop.archiv\webagent\worktrees\codex-clean-edit-bench` | `1cebc1df7f8d98267bd37588d5ca7000fe660e39` | detached | Historisch/temporär |
| `C:\Users\storax\Desktop\desktop.archiv\webagent\worktrees\grok-graph-edges` | `cc77f380fcc2c7b171112d5f23751a01b2834d4f` | `graphify/edge-repair` | Historisch/Experiment |
| `C:\Users\storax\Desktop\desktop.archiv\webagent\worktrees\grok-send-profile` | `e0c0427c6ef55fe3ffa949864a29c9b378fa3f78` | `grok/profile-driven-send` | Historisch/Experiment |
| `C:\Users\storax\Desktop\desktop.archiv\webagent\worktrees\tui-bench-harvest` | `ff33b757f7cee3f0ea9c914d2bf5f6109c5fd4ac` | `tui-bench-harvest` | Historisch/Experiment |
| Weitere `webagent-*`-Worktrees | Nicht in dieser Datei als aktuell klassifiziert | `git worktree list --porcelain` ist Quelle | Vor Löschung oder Nutzung prüfen; nie pauschal löschen |

> **Parallelitätsregel:** Mehrere Agenten dürfen lesend oder in eigenen, ausdrücklich angelegten Worktrees arbeiten. Zwei Akteure dürfen niemals denselben Worktree parallel beschreiben. In dieser Übergabe ist ausschließlich `webagent-final-verification` maßgeblich.

## 4. Aktueller Git-Zustand

### Snapshot 21.08.2026

```text
HEAD:          ea6c9a811bf78e68e15236ef5ce111ced5514660
origin/master: ea6c9a811bf78e68e15236ef5ce111ced5514660
branch:        detached HEAD
status:        M src/config/mod.rs
               M src/config/profiles.rs
               M src/config/writeback.rs
Divergenz:     0 / 0 (HEAD...origin/master)
Stashes:       beim Snapshot keine aus `git stash list` ausgegeben
```

Die uncommitteten Änderungen stammen aus der aktuell offenen, von Manus integrierten **Profil-/Session-Write-back-Härtung**. Sie dürfen nicht überlagert, verworfen oder in andere Worktrees kopiert werden, ohne vorher `git diff --check`, die Testmatrix und den fachlichen Abschnitt 5 erneut zu prüfen.

| Datei | Herkunft und Zweck | Prüfstatus |
|---|---|---|
| `src/config/profiles.rs` | Strikte Sparse-Kopie und neuer `restore_sparse_backup`-Pfad, der relevante Zielartefakte vor einem Rollback bereinigt | Formatiert; im Headless-Gesamttest kompiliert/getestet; finale Review- und Clippy-Gates offen |
| `src/config/writeback.rs` | Fail-closed Write-back mit Backup vor Mutation, eindeutiger Backupreservierung und Post-Write-Verifikation | Formatiert; im Headless-Gesamttest kompiliert/getestet; finale Review- und Clippy-Gates offen |
| `src/config/mod.rs` | Isolierte Regressionstests für Write-back, Backupreservierung und Sparse-Restore | Formatiert; im Headless-Gesamttest kompiliert/getestet; finale Review- und Clippy-Gates offen |
| `docs/HANDOVER_TO_CODEX_2026-08-25.md` | File-based Codex handover requested by user | Living document; not committed |

Der zuletzt vollständig geprüfte und auf GitHub gespiegelt sichtbare Codecommit ist `ea6c9a8` (`fix: gate brain windowing by runtime features`). Die aktuelle Konfigurationsscheibe **darf noch nicht committed oder gepusht** werden, weil das letzte unabhängige Sicherheitsreview nach der finalen Reparatur sowie ein erneuter Clippy-/vollständiger Liefercheck noch fehlen.

### Wichtigste jüngere Lieferhistorie

| Commit | Inhalt |
|---|---|
| `aea8d12` | Free-Cloud-Registry-/Policy-Kern |
| `81e3d3e` | Windows-Stack-Overflow-Fix für Main-Worker-Stack |
| `1d7e02d` | Deterministischer lokaler Textstream-Vertrag |
| `2cca3eb` | Versionierter Hub-Metadatenadaptervertrag |
| `48b9d6b` | Lokaler Health-/Circuit-Breaker-Vertrag |
| `cd2f5fc` | Testisolation über eindeutige Dateiaction-Wurzeln |
| `36a1766` | Goal-Abschluss-Evidenzgates |
| `ea6c9a8` | Featuregrenze für Brain-Wall/Grid-Runtime |

## 5. Aktuell laufende Arbeit: fail-closed Profil-/Session-Write-back

### Ausgangsbefund

Der irreversible Rückschreibpfad vom Runtime-Browserprofil in das Masterprofil konnte einzelne Fehler im Sparse-Copy-Pfad zuvor protokollieren, aber unter Umständen dennoch als Erfolg fortsetzen. Das ist für Login-/Session-Artefakte nicht akzeptabel. Der Masterprofilpfad benötigt daher einen eigenen, strikt fail-closed Vertrag; die normalen Runtime- und Swarm-Klone dürfen dagegen aus Kompatibilitätsgründen bewusst best effort bleiben.

### Aktueller Reparaturansatz

| Vertrag | Aktuelle Umsetzung |
|---|---|
| Strikte Fehlerbehandlung | `copy_dir_sparse_strict` propagiert relevante I/O-Fehler für den Write-back-Pfad |
| Backup vor Mastermutation | Bei nichtleerem Master wird vor dem Entsiegeln gesichert; Sicherungsfehler brechen vor jeder Masteränderung ab |
| Eindeutige Backupreservierung | `reserve_unique_backup_dir` kombiniert Zeit in Nanosekunden, PID, Versuchszähler und atomisches `create_dir`; vorhandene Backups werden nicht wiederverwendet |
| Vollständiges Sparse-Rollback | `restore_sparse_backup` entfernt zuvor relevante Sparse-Zielartefakte und kopiert anschließend aus dem Backup zurück, sodass Runtime-only-Artefakte nicht erhalten bleiben |
| Post-Write-Verifikation | Login-Artefakte, Größenverhältnis und bekannte Sessionnachweise werden nach dem Kopieren geprüft; Fehler lösen Restore aus |
| Master-Siegel | Der Master wird nur rund um eine Mutation entsiegelt und danach wieder read-only gesetzt |

### Bisherige Reparaturgeschichte und Reviews

Der erste strengere Patch bestand lokale Tests, wurde jedoch im unabhängigen Opus-Review zu Recht nicht angenommen: Das Restore arbeitete als Overlay und konnte zusätzliche Runtime-Artefakte zurücklassen; außerdem waren sekundengenaue Backupnamen kollisionsgefährdet. Ein paralleles GPT-5-Review sah diesen Entwurf positiver, nannte aber dieselben Restrisiken. Die strengere Ablehnung wurde übernommen.

Der aktuelle Patch ergänzt daher das prunende Restore und die reservierte Backupnamensgebung. Die frühere externe Proxy-Reviewstrecke ist im aktuellen Lauf **blockiert**, weil sie `available_credits: 0` meldete. Zwei lokale Claude-Code-Aufrufe sind vor einer Bewertung an CLI-Syntaxproblemen gescheitert und gelten ausdrücklich **nicht** als Review. Es liegt folglich noch **kein finaler unabhängiger PASS** für den reparierten Stand vor.

### Bereits wirklich vorliegende Evidenz

| Gate | Ergebnis | Quelle |
|---|---|---|
| `cargo fmt --all` | Erfolgreich ausgeführt | lokaler Worktree, 21.08.2026 |
| `git diff --check` | Erfolgreich vor der letzten Testphase | lokaler Worktree, 21.08.2026 |
| `cargo test --no-default-features` | **Exit 0** für den reparierten Stand | temporärer Hintergrundlog unter `%TEMP%\webagent_parallel_002\cargo_fulltest.*` |
| Zieltests `writeback_` | Im vollständigen Headless-Lauf enthalten; die vorherige Importlücke wurde repariert | `src/config/mod.rs` und obiger Volltest |
| Claude Opus review | PASS; no blocker | Read-only review on 2026-08-21; residual risks recorded below |
| cargo clippy --no-default-features | Exit 0 | 13 known warnings only outside this change in transcript.rs/browser_pool.rs |

### Nächster konkret ausführbarer Schritt

1. Den lokalen Claude-Code-Review in funktionierendem Print/Prompt-Modus oder einen anderen unabhängigen read-only Reviewer erfolgreich ausführen und dessen Urteil als Artefakt sichern.
2. `cargo clippy --no-default-features` auf dem **aktuellen** Worktree ausführen und neue Befunde gegen die bekannten Headless-Warnungen abgrenzen.
3. `cargo fmt --all -- --check`, `git diff --check`, `git status --short --branch` erneut prüfen.
4. Default- und TUI-ohne-WebView-Testpfad für die aktuelle Konfigurationsscheibe erneut ausführen oder belastbar begründen, falls eine Plattformgrenze besteht.
5. Erst bei grünen Gates, zwei unabhängigen PASS-Reviews oder einer transparent dokumentierten Ersatzentscheidung und klarem Commitrecht committen.

## 6. Architektur und Zustandsverträge

Die gegenwärtige Architektur ist in `docs/ARCHITECTURE.md` und aktueller in `docs/OVERVIEW.md` beschrieben. `ARCHITECTURE.md` ist teilweise historisch; Zeilenzahlen und ältere Reifeangaben müssen immer am Quellbaum nachgemessen werden.

| Schicht | Verantwortliche Module | Rolle |
|---|---|---|
| UI | `main.rs`, `cli.rs`, `commands/`, `repl/`, `tui*` | CLI, REPL, TUI und Bedienpfade |
| Agent | `controller/`, `prompts.rs`, `executor.rs`, `capability*.rs`, `relay.rs` | Plan–Act–Observe, Aktionsausführung, Beweisbindung |
| Brain/Browser | `brain.rs`, `browser/`, `browser_pool.rs`, `page_driver.rs`, `login.rs`, `brain_grid.rs`, `brain_wall.rs` | Browser-Backends, Sessions, sichtbare Brain-Topologie |
| Config/Profil | `config/paths.rs`, `profiles.rs`, `writeback.rs`, `clone.rs`, `brains.rs` | Pfade, Profile, Kopieren, Write-back und Konfiguration |
| Worker/Swarm | `worker_pool.rs`, `bot2bot_worker.rs`, `watchdog.rs`, `doctor.rs` | parallele Rollen, Lebenszyklus, Diagnose |
| Benchmark/Evidenz | `benchmark/`, `bench_*.rs`, `brain_score.rs`, `capability_proof.rs`, `run_store.rs` | Messung, Scores, Proofs, persistierte Runs |
| Kernschutz | `shell_policy.rs`, `loop_guard.rs`, `timeouts.rs`, `circuit_breaker.rs` | Richtlinien, Abbruch, Zeitgrenzen und Zustandsgrenzen |

Wichtige Kontrollflüsse sind: CLI → `controller` → `BrainBackend`/Browser → Protokollaktion → `executor`/`file_actions` → `run_store`/`transcript`; Profilklon → Runtime-Browser → Write-back-Gate → Backup → Mutation → Post-Verify/Rollback. Die aktuelle Write-back-Scheibe betrifft ausschließlich den letzten Pfad.

### Persistente Zustände und Sicherheitsgrenzen

* Persistierte Benchmarkevents liegen unter `C:\Users\storax\AppData\Local\webagent\data\brain_score\events.jsonl`; der Ringpuffer ist nicht die persistierte Quelle.
* Circuit-Breaker-Zustand wird laut `AGENTS.md` unter `...\data\circuit_breaker\state.json` beobachtet.
* Browserprofile und Konfigurationspfade sind über `config::paths` bzw. `data_dir` zu ermitteln; keine Übergabe soll Cookies, Tokens oder Profilinhalte kopieren.
* Der einzige Capability-Beleg-Gate ist `capability_proof::record_measurement`/`record_proof`; keinen zweiten Belegspeicher anlegen.
* Fail-closed Grenzen bestehen im aktuellen Write-back, im Goal-Evidenzgate und im Free-only-Routing. Ein fehlender Kosten-/Freinachweis darf keine automatische Providerroute auslösen.

Bekannte Strukturthemen: Die TUI bleibt ein großer, strukturell schwacher Bereich; `capability.rs` und `brain.rs` sind weitere dokumentierte Kandidaten für spätere Modultrennung. Vor einer Aufteilung zuerst `docs/ARCHITECTURE.md`, die aktuelle Codeverwendung und `.graphify/` prüfen; keine historische Diff-Datei als Spezifikation übernehmen.

## 7. Build-, Test- und Qualitätsmatrix

Der Windows-GNU-Linker muss vor Cargo-Läufen gesetzt sein:

```powershell
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "C:\Users\storax\AppData\Local\Microsoft\WinGet\Packages\BrechtSanders.WinLibs.POSIX.UCRT.LLVM_Microsoft.Winget.Source_8wekyb3d8bbwe\mingw64\bin\x86_64-w64-mingw32-gcc.exe"
```

| Gate | Kanonischer Befehl | Zweck | Stand dieser Scheibe |
|---|---|---|---|
| Format | `cargo fmt --all -- --check` | Keine Formatdrift | nach finaler Reparatur erneut ausführen |
| Diff | `git diff --check` | Whitespace-/Patchintegrität | nach finaler Reparatur erneut ausführen |
| Headless-Test | `cargo test --no-default-features` | Kern ohne WebView/TUI | **Exit 0**, 21.08.2026 |
| Headless lint | `cargo clippy --no-default-features` | Detect new lints | Exit 0; 13 known warnings outside this slice |
| Default | `cargo test` | Default feature combination | Exit 0: 1,136 passed, 0 failed (2026-08-21) |
| TUI ohne WebView | projektüblicher `tui`-Feature-Test ohne `webview` | Featuregrenze absichern | für aktuelle Konfigurationsscheibe erneut ausführen |
| Lieferaudit | `git status --short --branch`; `git diff --check` | Herkunft, Sauberkeit, Remotevorbereitung | vor Commit zwingend |

Bekannte vorbestehende Headless-Warnungen betreffen unter anderem `transcript.rs` und `browser_pool.rs`. Sie sind nicht durch die aktuelle Konfigurationsscheibe entstanden; trotzdem darf ein neuer Warnungsbefund nicht pauschal als „bekannt“ abgetan werden. Der aktuelle Reparaturstand hat einen vollständigen Headless-Test mit Exit 0, aber noch keine vollständige finale Lint-/Featurematrix.

## 8. Agenten- und Reviewverfahren

| Entität | Zulässige Rolle | Schreibrecht im Hauptworktree | Bemerkung |
|---|---|---:|---|
| Manus | bisherige Koordination und Integration | Ja | Bis zur Übergabe Git-/Worktree-Owner |
| Codex/ChatGPT | künftige technische Leitung | Nach Übergabe ja | Windows-Store-App vorhanden; Shell-CLI nicht als belastbar bestätigt |
| Claude Code / Opus | Architektur-/Sicherheitsreview | Nein | `claude.cmd`, read-only mit `Read,Grep`, Output vor Übernahme sichern |
| OpenCode | adversarial read-only Review | Nein | keine direkten Änderungen in den Hauptworktree übernehmen |
| Grok Build | lange Cargo-Läufe oder kurze Gegenfragen | Nein | interaktive/headless Sitzungen hängen wiederholt; Exitcode/Log prüfen, nie Textbehauptung allein vertrauen |
| Proxy-Modelle | strukturierte Gegenreviews | Nein | aktueller Proxy-Review im Snapshot wegen verfügbarem Kontingent `0` blockiert |
| Cursor / Ollama | Ausgeschlossen | Nein | nicht Teil des aktiven Teams |

Für jede Sicherheits-, Architektur- oder Produktionsscheibe sind grundsätzlich zwei unabhängige read-only Urteile anzustreben. Ein Review-PASS benötigt nachvollziehbaren Auftrag, tatsächlichen Diff/Quellstand und ein gespeichertes Urteil. Bei hängenden Agenten: Zeitfenster setzen, Prozess sauber beenden, Ergebnis nur mit Exitcode/Artefakt werten und auf einen anderen Reviewer wechseln. Lange Cargo-Läufe dürfen delegiert oder als strikt loggender Hintergrundprozess ausgeführt werden; in **einem Worktree niemals zwei Cargo-Builds gleichzeitig** starten (`AGENTS.md` §1).

## 9. Entscheidungen und verworfene Ansätze

| Entscheidung | Begründung |
|---|---|
| Nur ein aktiver schreibender Integrator | verhindert Worktree-Kollisionen und unklare Diffherkunft |
| Best-effort nur für Runtime-/Swarm-Klone | Browser-Lockdateien und WebView-Handles sind dort erwartbar; der irreversible Master-Write-back verlangt dagegen strikte Fehlerbehandlung |
| Keine Warnungsunterdrückung für Brain-Featuregrenzen | `ea6c9a8` grenzt Runtime-Code strukturell nach Features ab statt `allow(dead_code)` zu verstecken |
| Keine reale Free-Cloud-Inferenz ohne Freigabe | Kosten-, Credential-, Datenschutz- und Adaptergrenze müssen vorher belegt sein |
| Kein zweiter Capability- oder Proof-Speicher | `capability_proof` ist der einzige Gate gemäß `AGENTS.md` |
| Overlay-Restore verworfen | Ein unabhängiger Review zeigte, dass Runtime-only Sparse-Artefakte nach Restore zurückbleiben könnten |
| Sekunden-Backupnamen verworfen | Mehrere Write-backs könnten in derselben Sekunde kollidieren; der neue Pfad reserviert eindeutig |

Wiederkehrende Fehlannahmen, die Codex vermeiden soll: historische Diffs nicht als aktuelle Spezifikation behandeln; keinen neuen Mechanismus bauen, bevor der Bestand geprüft wurde; keine unvollständige Test- oder Reviewantwort als PASS lesen; keine Remote-/Pushannahmen gegen die aktuelle `AGENTS.md`-Regel oder explizite Nutzervorgabe treffen.

## 10. Betrieb und externe Abhängigkeiten

| Bereich | Voraussetzung / Grenze |
|---|---|
| Toolchain | Rust stable `x86_64-pc-windows-gnu` und der obige WinLibs-GNU-Linker |
| Betriebssystempfad | Windows; PowerShell für lokale Diagnose- und TUI-Hilfen |
| Browser | WebView-/Browserprofile nur mit vorhandener lokaler Anmeldung und expliziter Freigabe verwenden |
| GitHub | `origin` wie oben; Push nur nach geklärter Lieferregel |
| Agenten | Claude Code, OpenCode, Grok Build lokal installiert; Verhalten/Anmeldung vor Verlass darauf prüfen |
| Persistenz | `%LOCALAPPDATA%\webagent\data\...` für Events und Circuit-Breaker; genaue Profile über `config::paths` feststellen |
| Free Cloud | Nur lokale Verträge derzeit; realer HTTP-/Providerzugang braucht Freigabe und aktuelle Kosten-/Datenschutzprüfung |

Keine Passwörter, API-Keys, Cookies oder Tokens gehören in diese Datei. Falls ein Tool eine Anmeldung benötigt, soll Codex die erforderliche Sitzung im jeweiligen lokalen Client prüfen oder den Nutzer um eine explizite Übernahme bitten.

Zum Snapshot läuft **kein erwarteter produktiver WebAgent-Daemon**. Der für die aktuelle Scheibe gestartete Cargo-Hintergrundtest schrieb temporäre Logs unter `%TEMP%\webagent_parallel_002\` und meldete Exit 0; vor einer neuen Session kann dieser temporäre Diagnoseordner nach Prüfung entfernt werden.

## 11. Risiken und empfohlene Prioritäten

| Priorität | Risiko | Auswirkung | Empfohlene Behandlung |
|---:|---|---|---|
| P0 | Uncommitteter Write-back-Patch ohne finalen unabhängigen PASS | Login-/Profildaten könnten bei übersehener Randbedingung gefährdet sein | Keine Integration; Review und Matrix abschließen |
| P0 | Regelkonflikt Push-Autonomie vs. `AGENTS.md` | Unerlaubte oder unterlassene Lieferung | Vor Push explizit aktuellen Nutzerwunsch und Repo-Regel abgleichen |
| P1 | Verbleibende Headless-Warnungen | Qualitätssignal verschleiert neue Befunde | Separat klassifizieren, nicht global unterdrücken |
| P1 | Agenten-/Reviewunzuverlässigkeit | Falsche Sicherheitsfreigabe oder Zeitverlust | Evidenz aus Prozesslogs/Artefakten; Alternative Reviewer |
| P1 | Free-Cloud-Providergrenze | Kosten-, Datenschutz- oder Credentialrisiko | Nur mit expliziter Freigabe und echten Adaptertests behandeln |
| P2 | Große TUI-/God-File-Bereiche | Langfristige Wartbarkeit | Erst untersuchen, Grenztests erhalten, kleine Schnitte wählen |

Empfohlene Reihenfolge nach dem 25.08.: zuerst diese Übergabe und `AGENTS.md` lesen; anschließend tatsächlichen Git-/Worktree-/Prozesszustand prüfen; den Write-back-Patch entscheiden; erst danach Phase 6 vollständig abschließen und mit Phase 7 fortfahren. Free-Cloud-I/O nicht vor Abschluss der lokalen Topologie- und Sicherheitsgates beginnen.

## 12. Startanweisung für Codex

> **Sicherer Startblock – zuerst ausführen, nicht interpretieren:**
>
> ```powershell
> $r = "C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-final-verification"
> Set-Location $r
> Get-Content AGENTS.md
> git status --short --branch
> git rev-parse HEAD
> git rev-parse origin/master
> git worktree list --porcelain
> git diff --check
> git diff -- src/config/mod.rs src/config/profiles.rs src/config/writeback.rs docs/HANDOVER_TO_CODEX_2026-08-25.md
> ```
>
> **Danach zuerst lesen:**
>
> 1. `docs/HANDOVER_TO_CODEX_2026-08-25.md` (diese Datei), `AGENTS.md` und `STATUS_LIVE.md`.
> 2. `src/config/writeback.rs`, `src/config/profiles.rs`, die `writeback_`-Tests in `src/config/mod.rs`.
> 3. `docs/OVERVIEW.md`, `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` und bei Architekturentscheidungen `docs/ARCHITECTURE.md`.
>
> **Erste sichere Aufgabe:** Die uncommittete Write-back-Härtung unabhängig reproduzieren und bewerten. Setze den GNU-Linker, führe Format-, Diff-, Headless-Test- und Clippy-Gates aus, sichere zwei read-only Reviewurteile und aktualisiere diese Datei plus `STATUS_LIVE.md` mit den tatsächlichen Ergebnissen. Erst danach darf ein Commit erwogen werden.
>
> **Stop-Bedingungen:** Sofort stoppen und den Nutzer einbeziehen, wenn Git-/Dokumentationszustand widersprüchlich ist, fremde/unbekannte Änderungen erscheinen, ein Review einen Datenverlustpfad findet, ein Provider-/Login-/Kostenpfad nötig wird oder die Pushregel nicht eindeutig ist. Keine Konflikte still durch Überschreiben, Reset oder pauschales Löschen „bereinigen“.

---

## Pflegevertrag dieser Übergabe

Diese Datei wird bis zur endgültigen Übergabe am **25.08.2026** nach jeder wesentlichen Änderung aktualisiert. Vor einem finalen Abschluss wird sie gegen `git status --short --branch`, exakten `HEAD`, `origin/master`, `git worktree list --porcelain`, laufende Prozesse und die dokumentierten Test-/Reviewartefakte abgeglichen. Aussagen ohne belegbaren Befehl, Log oder Artefakt werden als offen gekennzeichnet, nicht als PASS.

**Angelegt am:** 21.08.2026
**Maßgeblicher HEAD beim ersten Snapshot:** `ea6c9a811bf78e68e15236ef5ce111ced5514660`
**Bearbeitungsstatus:** Uncommittete Write-back-Sicherheitsreparatur noch in Abnahme.
### Checkpoint update: 2026-08-21

The following facts were verified after the first handover draft was installed. They supersede only conflicting status wording above; all other open items remain open.

| Gate | Result |
|---|---|
| `cargo test --no-default-features` | Exit 0 on the current write-back repair |
| `cargo clippy --no-default-features` | Exit 0; 13 known warnings only in `transcript.rs` / `browser_pool.rs`, outside this slice |
| `cargo test` | Exit 0; 1,136 passed, 0 failed after registering this handover as a living document |
| Claude Opus read-only review | PASS; blocker `none` |
| Review residual risks | Mojibake in user/log strings; no explicit test for post-verify-triggered restore or double restore failure |
| Proxy review | unavailable: `available_credits: 0`; not counted as PASS |
| Second review | Grok snapshot review started separately; pending at this checkpoint |

The new handover is now intentionally registered in `src/startup.rs` as a living operating document. This changed the active source set and was verified by the green default test above. Before a commit, still run `cargo fmt --all -- --check`, `git diff --check`, a current status audit, the TUI-without-WebView test path, and resolve or document the second review result.
### Checkpoint update: TUI test and second review disposition

| Additional gate | Result |
|---|---|
| `cargo test --no-default-features --features tui` | Exit 0 on the current five-file slice |
| Grok isolated read-only review | No usable output or exit artefact after two bounded wait windows; process stopped; **not counted as PASS** |
| Second independent review | Still missing. The proxy path is unavailable (`available_credits: 0`), OpenCode produced no verdict, and Grok produced no artefact. |

The implementation must therefore remain uncommitted until a second independent review is obtained or the responsible integrator records an explicit, evidence-based exception approved under the then-current project rules. The current quality gates prove compilation and behavior; they do not replace the missing review gate.
### Checkpoint update: second-review gate remains open

After the prior checkpoint, three additional read-only review attempts were bounded and ended without a usable final artefact: Grok Build snapshot review, Codex native `review`/`exec` review, and Claude Sonnet review. Codex did surface a possible fail-open concern in streaming context, but never produced a complete verdict; a direct caller audit confirmed that the automatic handoff logs `write_back_session_to_master()` errors and the CLI recovery route matches success/error explicitly. This manual audit is supporting evidence only, **not a second independent PASS**.

The only complete independent verdict remains the Claude Opus PASS without blockers. The second review gate is still open because the proxy path reports `available_credits: 0` and the other local agents failed to produce a final artefact. The uncommitted slice must not be declared fully reviewed, committed, or pushed until a valid second verdict or an explicitly authorized exception is documented.


## Finaler Evidenzcheckpoint — 2026-08-21 14:15 UTC+2

Die Phase-6-Slice für fail-closed Profil-Write-back ist commitbereit. Sie schützt jeden Masterprofil-Lesepfad durch Pre-Clone-Recovery und fs2-Sperre, publiziert Journale dauerhaft und verhindert stille Erfolgsrückgaben in Shutdown- und Shared-Browser-Pfaden.

| Kategorie | Nachweis |
|---|---|
| Format, Diff, Clippy | bestanden; Clippy Exit 0 mit 13 bekannten Baseline-Warnungen |
| Reduzierte Features | 1.089 Tests bestanden, 0 fehlgeschlagen |
| Standardfeatures | 1.155 Tests bestanden, 0 fehlgeschlagen |
| Review 1 | Codex VERDICT=PASS, keine Blocker; %TEMP%\\webagent_codex_direct_readonly_final_20260821_141114\\final_review.txt |
| Review 2 | Grok VERDICT=PASS, keine Blocker; %TEMP%\\webagent_grok_final_sharedteardown_20260821_135722\\stderr.log |

Der finale Umfang umfasst durable Journal- und Baum-Synchronisation, Reseal nach jedem fehlgeschlagenen Unseal, Fehlerpropagierung durch REPL/Pools/isolated_query und verpflichtendes Teardown/Write-back eines besessenen finalen Runtime-Klons bei weiterhin sicheren aktiven Shared-Tab-Referenzen.

Weiterhin offen in Phase 6: Swarm-/Worker-Topologieverträge, Profilpersistenz und Session-Handoff-Verifikation. Externe Providerzugriffe und Releases bleiben außerhalb dieser lokalen Slice.


## Swarmprofil-Lease-Evidenzcheckpoint — 2026-08-21 16:30 UTC+2

Die Profilpersistenz-Slice ist commitbereit. SwarmProfileLease bindet Lauf und Brain über nicht geheime Owner-Metadaten, arbeitet transaktional und verhindert bei jeder Vorbereitungsstörung den Start eines isolierten Workers oder einer Query. Owner-geprüfter Release und Cleanup sind idempotent und löschen niemals fremde Profilscopes.

| Kategorie | Nachweis |
|---|---|
| Format und Index-Diff | bestanden |
| Clippy | Exit 0 mit 13 bekannten Baseline-Warnungen |
| Reduzierte Features | 1.093 Tests bestanden, 0 fehlgeschlagen |
| Standardfeatures | 1.159 Tests bestanden, 0 fehlgeschlagen |
| Review 1 | Claude VERDICT=PASS, keine Blocker |
| Review 2 | Grok VERDICT=PASS, keine Blocker |

Die nächste getrennte Phase-6-Slice ist die Cross-Brain-Session-Handoff-Abstraktion. Sie darf das bestehende Same-Brain-Resume nicht aufweichen und muss die profilgebundene Lease-Eigentümerschaft respektieren.


## Cross-Brain-Session-Handoff-Evidenzcheckpoint — 2026-08-21 17:15 UTC+2

Die Session-Handoff-Slice ist commitbereit. Der versionierte CrossBrainHandoffEnvelope transportiert ausschließlich begrenzte Textmetadaten und validiert Source/Target-Brain, Run und Handoff-Kontext fail-closed. Er übernimmt keine conversation_ref; der Zielbrain startet frisch. Same-Brain-Resume behält seine bestehende, eigene Referenzsemantik.

| Kategorie | Nachweis |
|---|---|
| Format und Index-Diff | bestanden |
| Clippy | Exit 0 mit 13 bekannten Baseline-Warnungen |
| Reduzierte Features | 1.097 Tests bestanden, 0 fehlgeschlagen |
| Standardfeatures | 1.163 Tests bestanden, 0 fehlgeschlagen |
| Review 1 | Claude VERDICT=PASS, keine Blocker |
| Review 2 | Grok VERDICT=PASS, keine Blocker |

Offener weiterer Phase-6-Schritt: Parallel-Worktree-Ergebnisse von ChatGPT und Claude erst an expliziten Commitgrenzen bewerten und konfliktfrei integrieren. Keine uncommitteten externen Diffs übernehmen.


## Worker-Topologie-Evidenzcheckpoint — 2026-08-21 17:50 UTC+2

Die verbleibende fail-closed Topologielücke ist geschlossen: fehlende Heartbeats sind ausschließlich während der begrenzten Startup-Grace frisch. An oder nach deren Ende wird der Worker stale, beendet und als unavailable markiert; ein vorhandener Reservebrain kann noch im selben Tick übernommen werden. Künftige Heartbeat-Zeitstempel lösen keinen Fehlalarm aus.

| Kategorie | Nachweis |
|---|---|
| Format und Index-Diff | bestanden |
| Clippy | Exit 0 mit 13 bekannten Baseline-Warnungen |
| Reduzierte Features | 1.100 Bibliothekstests und 7 Binärtests bestanden |
| Standardfeatures | 1.166 Bibliothekstests und 7 Binärtests bestanden |
| Review 1 | Claude VERDICT=PASS, keine Blocker |
| Review 2 | Grok VERDICT=PASS, keine Blocker |

Nächste Integrationsgrenze: Nur committe Ergebnisse aus den parallelen ChatGPT- und Claude-Worktrees prüfen; uncommittete Diffs bleiben strikt getrennt.


## Goal-Audit und Git-Nachweis — 2026-08-21 18:00 UTC+2

Direkter Git-Nachweis: Aktiver HEAD und origin/master sind beide 1afbfbf962ab856d894c02d1a249cff548729ab9; der Arbeitsbaum ist sauber. Das gemeinsame Gitdir unter dem Archivpfad ist erreichbar und kein Integritätsfehler.

Die Goal-Karte bleibt aktiv, weil der projektinterne Abschlussstatus noch keine eigene Evidenz speichert und ein echter OHA-SSS-Harnesslauf Provider-/Browserzugriff auslösen kann. Dieser Lauf wird ausschließlich nach ausdrücklicher Freigabe ausgeführt. Parallelworktrees liefern aktuell keine commitbereiten Kandidaten: ChatGPT besitzt einen uncommitteten Pool-Diff, Claude ist sauber, aber hinter origin/master.
