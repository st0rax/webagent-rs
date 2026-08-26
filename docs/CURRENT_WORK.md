# Aktueller Arbeitsstand

**Aktualisiert:** 2026-08-26
**Zweck:** verbindlicher Wiedereinstieg und operative Wahrheit. Historische Befunde stehen in `docs/OVERVIEW.md` sowie in den datierten Übergaben; diese Datei ersetzt sie nicht, sondern hält nur den aktuellen Abschlusspfad fest.

## Aktueller Repositoryzustand

Die Arbeit läuft auf `task/v1-release-baseline`, ausgehend von `origin/master` bei `1d214e8`. Die erste abgeschlossene Baseline-Scheibe ist als `2659baf` (`fix: restore headless release baseline`) committet. Sie stellte den browserfreien Releasezustand wieder her: fehlende Bibliotheksmodule wurden registriert, der Root-Eintrag in `Cargo.lock` wurde auf `0.11.1` korrigiert, reine Kernmodule wurden von unnötigen TUI-Gates entkoppelt, strikte Lint-/Testdrift wurde behoben und die alte Wilson-Dublette wird auf die gemeinsame Implementierung zurückgeführt.

Besonders wichtig ist die Korrektur von `capture_patch`: Eine globale Git-Einstellung wie `color.ui=always` lieferte ANSI-Sequenzen in `git diff` und machte die Scope- sowie Harvest-Prüfung blind. Der maschinell verarbeitete Patch wird nun explizit ohne Farbe erzeugt; reale Gegenproben auf einem temporären Git-Repository belegen In-Scope-Harvest und fail-closed Out-of-Scope-Ablehnung.

| Gate | Ergebnis auf dem aktuellen Arbeitsstand |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | **1.157 Bibliotheks-, 7 Binärtests bestanden; 1 Test bewusst ignoriert** |
| `git diff --check` | bestanden |
| Reale Scope-/Harvest-Gegenprobe | bestanden; ANSI-farbige Git-Diffs werden sicher verarbeitet |

## v1.0-Ziel und Definition of Done

> **v1.0 bedeutet nicht, dass jede Idee umgesetzt ist.** Webagent ist fertig, wenn der dokumentierte lokale Browser-Agent verlässlich gebaut, getestet, betrieben und anhand aktueller Evidenz freigegeben werden kann. Ein grüner Unit-Test oder ein abgeschlossener Teil-Goal reicht dafür nicht aus.

| Abschlussbereich | Verbindliches Abnahmekriterium |
|---|---|
| **Reproduzierbarer Kern** | Formatcheck, striktes Headless-Clippy und vollständige browserfreie Test-Suite sind auf dem Release-Candidate grün; der Lockfile-Stand ist konsistent. |
| **Plattformgrenze** | Die Vollfeature-Gates (`cargo clippy --all-targets -- -D warnings`, `cargo test`) sind auf Windows mit WebView2 oder durch nachvollziehbare Windows-CI-Evidenz grün. Linux bleibt ein unterstützter browserfreier Kern-/TUI-Pfad, ersetzt aber keine Windows-WebView-Abnahme. |
| **Providerfähigkeit** | Jeder in der Releasekonfiguration aktivierte Provider besitzt eine frische, nachvollziehbare Send-/Antwort-Evidenz oder ist bewusst deaktiviert und mit Ursache dokumentiert. Keine historische Erfolgsmeldung gilt als Live-Beleg. |
| **Mehr-Brain-Betrieb** | Poolstart, Heartbeat, Recovery, Profil-Lease, Write-back, bereinigbarer Shutdown sowie Continuation und Cross-Brain-Handoff sind systemisch belegt. Ein abgebrochener Lauf darf weder das Master-Profil beschädigen noch ungemeldete Klonreste hinterlassen. |
| **Harvest-Sicherheit** | Ein gültiger Patch wird nach Wiederanwendung erneut gebaut und getestet; Scope-Verstöße, Löschungen und nicht nachvollziehbare Patches bleiben fail-closed ausgeschlossen. |
| **Betriebsdokumentation** | `docs/OVERVIEW.md`, diese Datei, `CHANGELOG.md` und die Release-Checkliste stimmen mit Code, Tests und bekannten Grenzen überein. |
| **Releaseentscheidung** | Der Release Candidate ist auf einem sauberen Tree und über GitHub erreichbar; erst nach dokumentierter Review- und Eigentümerfreigabe folgen Merge, Tag und GitHub-Release. |

## Scope-Freeze

Der v1.0-Kritische Pfad umfasst ausschließlich die nachfolgenden Arbeiten. Jede Erweiterung, die keine Definition-of-Done-Zeile erfüllt, bleibt außerhalb des Releasepfads.

| Im v1.0-Scope | Bewusst nach v1.0 verschoben |
|---|---|
| Browserfreie Releasequalität, Windows-Vollfeature-Evidenz und CI-Reproduzierbarkeit | Terminal-Renderer und ARIA-zentrierte Vermittlungsschicht |
| Frische Provider-Rezertifizierung und klarer Umgang mit Drift bzw. Ausfällen | CDP-/AX-Spike über die heutige Browseranbindung hinaus |
| Shutdown-, Profil-, Lease- und Klonbereinigungshärtung | Reale Free-Cloud-Transportadapter oder kostenpflichtige Fallbacks |
| End-to-End-Abnahme für Pool, Continuation, Cross-Brain-Handoff und Harvest | Genius Council, zusätzliche Agentenrollen und größere UX-Erweiterungen |
| Dokumentierte Release-Candidate- und Freigabevorbereitung | Neue Provider, neue Persistenz- oder Protokollmechanismen ohne Abschlussbezug |

## Kritischer Pfad und Blocker

Die nächste technische Scheibe ist die **Provider-Rezertifizierung**. Vor dem Live-Lauf werden die bestehenden Selektoren, Capability-Proofs und Fehlerbilder lokal inventarisiert. Die eigentliche Messung darf nur stattfinden, wenn der Eigentümer anwesend ist: Browserfenster können sich öffnen, aber Anmeldungen, Einmalcodes, CAPTCHAs und Zugangsdaten verbleiben vollständig beim Eigentümer. Der Integrator protokolliert ausschließlich die beobachteten Zustände und nutzt keine Geheimnisse.

Der aktuell bekannte spezifische Providerrest ist `zai`: Die Antwort kann einen `Thought Process` voranstellen; ein früherer Überlastungszustand verhinderte die belastbare Gegenprobe. Ein Provider mit frischer, nicht klassifizierbarer Antwort wird nicht still als releasebereit bewertet. Für alle Provider gilt dieselbe Regel: frische Evidenz oder dokumentierte, bewusst deaktivierte Grenze.

| Abhängigkeit | Status | Umgang |
|---|---|---|
| Eigentümer für Live-Browser/Anmeldung | erforderlich | Vor der tatsächlichen Live-Rezertifizierung gezielt um Freigabe bitten; bis dahin nur lokale Vorbereitung. |
| Windows-WebView2-Gates | offen | Über Windows-Runner bzw. Windows-CI nachweisen; Linux-Headless-Gates sind kein Ersatz. |
| Release/Merge/Tag | offen | Erst nach vollständiger Evidenz und Eigentümerfreigabe vorbereiten; keine Veröffentlichung vorab. |

## Nächste sichere Aktion

Die nächste sichere Aktion ist eine **read-only Provider- und Evidenzinventur**: aktive Provider, Selektorstände, Capability-Proofs, vorhandene Fehlersignaturen und die exakte Testmatrix für die anstehende Eigentümer-Session zusammenstellen. Dabei werden weder Browserfenster geöffnet noch Logins, Quoten oder Provideraktionen ausgelöst.

## Übergabe

- **Branch:** `task/v1-release-baseline`
- **Abgeschlossene Scheibe:** `2659baf` plus nachfolgender, noch zu commitender Scope-/DRY-Checkpoint
- **Eigentümerschaft:** aktueller Integrator bearbeitet Baseline und v1.0-Abschluss; Live-Anmeldungen bleiben beim Eigentümer.
- **Externe Freigaben:** erforderlich für Live-Browser, Logins, kostenpflichtige Providerpfade, Merge, Tag und GitHub-Release.
- **Arbeitsbaum:** nach Commit dieses Checkpoints erneut prüfen und den Branch pushen.

> **Hinweis zur Wahrheitspflege:** Historische Live-Befunde aus August 2026 sind nützlich für die Diagnose, aber kein aktueller Verfügbarkeitsbeleg. Maßgeblich für v1.0 sind frische, reproduzierbare Belege am Release Candidate.
