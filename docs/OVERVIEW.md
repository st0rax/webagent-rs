# Webagent: Systemüberblick

**Stand:** 2026-08-22. Diese Datei ist die aktuelle Produkt- und
Architekturübersicht.

Nicht jede `.md` im Repo ist Soll-Zustand. `START_HERE.md` ist der stabile
Einstieg und `docs/CURRENT_WORK.md` die kurze operative Übergabe. `*_PLAN.md`,
`*_CONCEPT.md`, `PROGRESS.md`, `TUI_DESIGN.md`, `STATUS_LIVE.md` und datierte
Übergaben sind Log oder Entwurf. Betrieb der TUI steht in `AGENTS.md` §6. `webagent` ohne Subcommand
oeffnet die Session-Ansicht; `webagent repl` und `webagent tui` bleiben.

**Plattformen:** Session-TUI, REPL und CLI bauen auf Windows, Linux und
Android (Termux, `aarch64-linux-android`). Embedded-WebView-Brains und die
Kachelwand sind Windows/WebView2. Release-Binaries: Tag `v*` →
`.github/workflows/release.yml`.

## Ziel

Webagent ist ein lokales, provider-agnostisches Harness für autonome
Web-Chat-Brains. Das Harness stellt Sitzung, Kontext, Werkzeuge, Beobachtungen,
Persistenz und Grenzen bereit; das Brain entscheidet selbst, wie es eine Aufgabe
untersucht, ändert und verifiziert. Benchmark und Selbstverbesserung sind
Auftraggeber dieses Kerns, keine zweite Agentenarchitektur.

Provider-agnostisch bedeutet hier: Der Agentenloop hängt am
`BrainBackend`-Trait. Das derzeit produktive Backend steuert angemeldete
Web-Sessions über Embedded WebView. DOM-Selektoren und einzelne Sendepfade sind
weiterhin providerabhängige Adapter; vollständige Austauschbarkeit ist daher ein
Ziel, keine Behauptung über jede beliebige Chat-Seite.

## Reifegrade

- **Implementiert:** im normalen Codepfad verdrahtet und durch lokale Tests oder
  persistierte Laufdaten prüfbar.
- **Experimentell:** ausführbarer Code ist vorhanden, Live-Verhalten hängt aber
  von Web-UIs, Sessions oder noch nicht abgeschlossener Integration ab.
- **Geplant:** Entwurf oder isolierter Baustein; nicht als Produktfähigkeit
  behandeln.

## Aktueller Abnahmestand

**Stand 2026-08-23:** Alle vier Abnahmebelege der Roadmap liegen vor —
Live-Rezertifizierung (1), integrierte Mehr-Brain-Abnahme (2),
Benchmark-/Harvest-Systemabnahme (3) sowie Upstream-Integration und Release
(4, `v0.10.0`).

Ein abgeschlossener Goal-Datensatz belegt weiterhin nur den darin gebundenen
Arbeitsauftrag. Von den zwei Defekten aus den `v0.10.0`-Notizen ist einer
belegt behoben — `mistral` liefert die Antwort ohne Zeitstempel und
Feedback-Widget, mit Gegenprobe. `perplexity` antwortet wieder, aber die
Ursache ist unbekannt: Die Gegenprobe zeigte, dass der vermutete Selektor-Fix
nicht kausal war. Ein Rückfall ist damit nicht ausgeschlossen. Offen bleibt
ausserdem das `Thought Process` bei `zai`.

| Bereich | Stand am 2026-08-23 | Fehlender Abschlussbeleg |
|---|---|---|
| Lokaler Rust-Kern | Voll- und Headless-Gates grün auf `master`; 1.189 Bibliotheks- und 7 Binärtests mit Defaultfeatures sowie 1.123 + 7 ohne Defaultfeatures, striktes Clippy mit und ohne Defaultfeatures. CI seit `51d196f` wieder grün — sie war vom 22.08. bis dahin auf jedem Push rot, auch auf `master`. **Integriert** über PR #6–#10 | — |
| Provider-/WebView-Livebetrieb | **Rezertifiziert am 2026-08-23**: Diagnose 9/9, Relay gegen echte Browser. `mistral` ist behoben (Zeitstempel und Feedback-Widget nicht mehr im Antworttext, per Gegenprobe belegt). `perplexity` antwortet wieder, **die Ursache ist aber unbekannt** — der vermutete Selektor-Fix hielt der Gegenprobe nicht stand | `zai` stellt ein `Thought Process` voran (Anbieter überlastet, nicht vermessbar); bei `perplexity` fehlt die Erklärung, warum die Timeouts aufhörten |
| Pool, Worker und Cross-Brain-Handoff | **Abgenommen am 2026-08-23**: Poolstart, Auto-Recovery, Profil-Lease über neun Brains im Parallelbetrieb, geordneter Shutdown, Write-back ins Master (23→33 Hosts, additiv) und Worker-Heartbeat unter echter Last. Continuation, Stall und Cross-Brain-Handoff im Benchmark-Pfad end-to-end belegt | — |
| Benchmark/Autoresearch/Harvest | **Abgenommen** (2026-08-23, `src/benchmark/e2e_tests.rs`): providerfreier Lauf auf echtem temporärem Git-Repo, Auftrag → Bau → Gates → Ernte, mit Fresh, Continuation gleicher Run-ID, Stall, Cross-Brain-Handoff und fail-closed verworfenem Scope-Verstoß | — |
| Free-Cloud-Textchat | Registry, Policy, Mock-Stream, Metadaten- und Breaker-Verträge lokal implementiert | kein echter HTTP-/Provideradapter; externe Inferenz bleibt bis zur ausdrücklichen Freigabe außerhalb des Lieferstands |
| Release | **`v0.10.0` am 2026-08-23**: Artefaktlauf auf `master` grün für Windows, Linux und Android, danach Tag und Release mit angehängten Binaries | — |

Damit bedeutet „lokale Gates grün“ weiterhin **nicht** „Produkt fertig“ — wohl
aber sind die Systembelege da, auf die ein Release sich stützen kann.

## Kernarchitektur

```text
CLI / REPL / TUI / Benchmark
              |
              v
       AgentController
    Plan -> Act -> Observe
       |              |
       v              v
  BrainBackend     Tools / Executor
       |          shell, edit, edit_batch, write
       v              |
  WebBrainBackend     v
       |        Workspace + Git
       v
 Embedded WebView

Querschnitt: RunStore, Transcript, Memory, Audit, Timeouts,
             Loop-Guard, Circuit-Breaker, Capability-Proofs
```

Die zentrale Abhängigkeitsrichtung lautet:

1. `brain` abstrahiert eine Chat-Sitzung.
2. `controller` führt einen langlebigen Plan/Act/Observe-Lauf aus.
3. `protocol` validiert top-level `webagent/1`-Actions.
4. `action_engine`, `file_actions` und `executor` wirken im gebundenen
   Workspace; Shell-Policy und Audit sind Sicherheitsnetze, keine Sandbox.
5. `run_store` und `transcript` halten Zustand für Diagnose und Resume.
6. Benchmark, Self-Research und Autoresearch verwenden diese Schichten.

Das Harness erzwingt kein festes Arbeitsrezept wie „genau ein Read“. Cycle- und
Wall-Limits sind Circuit-Breaker. Vollständiger Zielkontext wird genutzt, wenn er
ins gemessene Brain-Budget passt; ansonsten wird Kontext begrenzt.

## Vertrauensleiter

Webagent trennt Angebot, Beobachtung und Beweis. Eine deklarierte Fähigkeit oder
ein vorhandener Selektor ist noch kein Können.

| Stufe | Aussage | Autoritative Evidenz |
|---|---|---|
| 0: konfiguriert | Provider/Selektor ist bekannt | Konfiguration, `selectors/` |
| 1: erreichbar | Session startet und Oberfläche ist bereit | Diagnose/Health-Run |
| 2: gemessen | konkrete UI-Fähigkeit wurde ausgeführt | `capability_proof`-Measurement |
| 3: aktuell belegt | letzter passender Beleg ist weder fehlgeschlagen noch abgelaufen und der Selektor-Hash stimmt | `ProofState::Proven` |
| 4: agentisch erledigt | Controller erzeugt nachvollziehbare Actions und Workspace-Artefakte | Run-Meta, Transcript, Action-Resultate |
| 5: akzeptiert | aufgabenspezifische Gates bestehen | Diff, Build/Test/Lint beziehungsweise konfigurierte Eval |
| 6: übernommen | Ergebnis wurde bewusst geerntet/committet | Git-Commit und Benchmark-Report |

`capability_proof` ist der einzige Beleg-Gate für UI-Fähigkeiten. Brain-Aussagen,
Exit-Code allein oder eine historische Provider-Tabelle dürfen keine höhere
Stufe vortäuschen.

## Funktionsstand

### Implementiert

- `BrainBackend`, WebView-Backend und persistente Browser-Runtime.
- Autonomer Controller mit Shell-, Edit-, transaktionaler Multi-Edit-, Write-,
  Message- und Finish-Action; Observation-Feedback und Protokoll-Reparatur.
- Workspace-Bindung, Shell-Policy/Audit, Loop-/Wall-Schutz, Circuit-Breaker.
- Run-Meta, Transcript, Action-Deduplizierung, Conversation-Resume und
  Continuation-Schnitt mit konkreter Reparaturanweisung.
- Capability-Proofs mit `Never`, `Failed`, `Expired` und `Proven` sowie
  Selektor-Hash-Bindung.
- Self-Research und Autoresearch sowie eine vote-getriebene Benchmark-Pipeline.
  Autoresearch und Benchmark binden Git-/Eval-Gates ein; der Benchmark führt
  zusätzlich Build-/Test-Scoring und optionale Ernte aus.
- Feasibility- und Work-Package-Bausteine zur belegten Aufgabenbeschreibung.

### Experimentell

- Provider-Livebetrieb: Web-UIs, Login, Quoten und DOM ändern sich außerhalb des
  Repos. `docs/PROVIDER_STATUS.md` ist ein datierter Messverlauf, keine Garantie.
- Benchmark-Dogfooding und Harvest: ausführbar, aber teuer und von erreichbaren
  Brains sowie einem sauberen Git-Tree abhängig.
- Multi-Brain-Pool, Handoff und TUI-Orchestrierung. Desktop-Layout unter
  Windows: TUI unten 30 %, Brain-Fenster oben 70 % (`brain_grid::split_areas`).
  Die Wall findet Fenster am Titel `webagent · …` im TUI-Prozessbaum und legt
  sie `HWND_TOPMOST` ohne Fokus. Overflow und TUI-Minimize parken auf
  `-32000`. `docs/TUI_DESIGN.md` ist der Dashboard-Entwurf von Juli, nicht
  dieses Layout.
- Persistente Query-Continuation, Controller-Continuation und die
  Wiederverwendung derselben Controller-Run-ID über Benchmark-Repair-
  Iterationen sind vorhanden. Der Livebetrieb bleibt wegen externer Web-Sessions
  experimentell.
- Work-Package/Feasibility existieren als geprüfte Bausteine; ihre vollständige
  Gate-Integration in jede Benchmark-Aufgabe ist noch im Ausbau.

### Geplant

- Semantisches Fortschrittsmodell statt überwiegend syntaktischer
  Loop-/Fehlerheuristiken.
- Symbolorientierte Context Bundles über Zieldefinition, Aufrufer und relevante
  Typen hinweg.
- Durchgängige Session-Leases für Auftrag, Repair und Handoff.
- Weitere Zerlegung großer Orchestrierungsdateien und schmalere öffentliche
  Moduloberfläche.

## Betriebsmodell

1. Mit `cargo test` und `cargo clippy --all-targets -- -D warnings` den lokalen
   Stand prüfen. WebView-Livechecks sind zusätzlich, nicht Ersatz für Tests.
2. Providerzustand mit Diagnose/Verify messen; alte Statusdokumente nicht als
   aktuelle Wahrheit verwenden.
3. Autonome Arbeit in einem expliziten Workspace starten. Jeder Run erhält
   Run-ID, Meta und Transcript unter dem konfigurierten Datenverzeichnis.
4. Bei Unterbrechung dieselbe Run-ID fortsetzen. Ein Repair soll den vorhandenen
   Diff und die konkrete Gate-Ausgabe erhalten, nicht die Aufgabe neu beginnen.
5. Benchmark nur auf sauberem Git-Tree ausführen. Kandidaten erst nach den
   konfigurierten Gates ernten; der Merge folgt erst nach grünen Gates.

Sicherheitsgrenze: Das System ist ein lokaler Coding-Agent mit weitreichender
Shell. Die Policy blockiert bekannte gefährliche Muster, ersetzt aber weder OS-
Sandbox noch Rechtebegrenzung. Unvertraute Repository- und Tool-Ausgaben werden
als Daten behandelt und in Prompts markiert.

## Dokumentenlandkarte

- `START_HERE.md`: verbindlicher Einstieg für neue Entwickler und Agenten.
- `CONTRIBUTING.md`: reproduzierbarer Entwicklungs- und Abnahmeprozess.
- `docs/COLLABORATION.md`: dauerhafte Kommunikation über Issue, Branch und PR.
- `docs/CURRENT_WORK.md`: kurzer aktueller Arbeitsstand, Evidenz und nächste
  sichere Aktion.
- `README.md`: Installation und öffentliche Bedienung.
- `docs/OVERVIEW.md`: aktuelle Produktarchitektur, Reifegrade und Betrieb
  (dieses Dokument).
- `docs/ARCHITECTURE.md`: detaillierte Modulkarte und Refactoring-Befunde;
  Momentaufnahme, Zahlen vor Verwendung neu messen.
- `CONVENTIONS.md` und `AGENTS.md`: verbindliche Entwicklungsregeln.
- `docs/PROTOCOL_SCHEMA.md`: Aktionsprotokoll; gegen `src/protocol/` prüfen,
  falls sich beide widersprechen.
- `docs/PROVIDER_STATUS.md`: datiertes Live-Messjournal.
- `STATUS_LIVE.md`: archiviertes chronologisches Arbeitsjournal, keine
  Architektur- oder Übergabequelle.
- `*_PLAN.md`, `*_CONCEPT.md`, `CODE_REVIEW.md`, `CLAUDE_PROPOSALS.md`:
  historische Entwürfe/Reviews; nur explizit belegte Teile gelten als umgesetzt.

## Priorisierte Roadmap

### 0. Erledigter Meilenstein: lokale Provider-Bridge

**Status:** umgesetzt als lokaler, token-geschuetzter Loopback-Dienst. Der Befehl webagent api serve bietet einen OpenAI-Chat-Completions- und einen Anthropic-Messages-Adapter, einen token-geschuetzten Modellkatalog sowie dokumentierte Text- und Streaminggrenzen.

Der verbindliche Betriebsvertrag, Pi-Konfigurationsbeispiele und die Sicherheitsgrenzen stehen in [API_BRIDGE.md](API_BRIDGE.md).

### 1. Live-Rezertifizierung — durchgeführt am 2026-08-23

Diagnose und Relay wurden mit anwesendem Eigentümer gegen die heutigen Web-UIs
neu gemessen; die Einzelheiten stehen in `CURRENT_WORK.md`. Diagnose 9/9,
Relay 8/9, frische Belege in `data/capability/proofs.jsonl`.

Der Run-Nachweis über den vollen Controller-Loop ist ebenfalls erbracht
(`write` + `shell`-Selbstprüfung, `status=done`, Ergebnis auf Platte
nachgemessen statt der Erfolgsmeldung geglaubt).

Offen aus dieser Messung: der `perplexity`-Antwortselektor (das Brain
antwortet, der Harness sieht es nicht) und das UI-Beiwerk bei `mistral` und
`zai`.

Historische Providerzahlen bleiben ungültig als Ersatz; Browser-, Login- und
Accountzugriffe weiterhin nur nach ausdrücklicher Nutzerfreigabe.

### 2. Integrierte Mehr-Brain-Abnahme — erledigt am 2026-08-23

Belegt: Poolstart, Auto-Recovery, Profil-Lease über neun Brains im
Parallelbetrieb, geordneter Shutdown, der Write-back ins Master-Profil
(23→33 Hosts, 102→144 Cookies, rein additiv) und der Worker-Heartbeat unter
echter Last — eine Aufgabe aus der Inbox wurde aufgenommen, in `_read/`
verschoben und in `state.json` als verarbeitet geführt, während der Heartbeat
durchlief.

Zwei Stolperstellen, die dokumentiert bleiben, weil sie jeden künftigen Lauf
kosten: Der Rückweg läuft nur mit `WEBAGENT_PERSIST_TABS=0`, weil
`persist_browser_tabs()` im Shared-Modus per Default `true` liefert. Und
`Msg::parse` verlangt die Header `From`, `To`, `Time` exakt so — eine Nachricht
mit abweichenden Headern bleibt still in der Inbox liegen und sieht von aussen
wie ein hängender Worker aus. Einzelheiten in `CURRENT_WORK.md`.

Unit-Tests der Einzelverträge ersetzen den fehlenden Rest nicht.

### 3. Benchmark-/Autoresearch-Abnahme — erledigt am 2026-08-23

Belegt in `src/benchmark/e2e_tests.rs`: die volle Pipeline auf einem echten
temporären Git-Repo mit echten Eval-Kommandos; gedoppelt ist nur die
Brain-Ausführung (Phase A über `query`, Phase B über `PhaseBRunner`).

Beide geforderten Ausgänge sind belegt — der geerntete Kandidat und der aus dem
richtigen Grund fail-closed verworfene. Dass der Grund wirklich der Datei-Scope
ist, hält ein Gegenprobe-Test fest, der denselben Patch ohne Auftrag durchgehen
lässt; ohne ihn wäre die Verwerfung auch von der Freitextprüfung erklärbar.

### 4. Konsolidierung und Release — erledigt am 2026-08-23 (`v0.10.0`)

Ausgeführt nach den drei technischen Abnahmebelegen: Artefaktlauf auf `master`
grün für alle drei Ziele, Version auf 0.10.0, Tag `v0.10.0`, Release mit
angehängten Binaries. Die beiden bekannten Defekte stehen in der Release-Notiz,
statt von der nächsten Person entdeckt zu werden.

Ursprünglicher Auftrag:

Nach den End-to-End-Gates werden README, Übersicht, Providerstatus und Übergabe
auf denselben Stand gebracht. Push, Merge, Tag und Release führt der Integrator
aus; vorhandene Workflows allein sind kein Releasebeleg. Ein Release setzt die
vier Abnahmebelege voraus (der dritte liegt seit dem 2026-08-23 vor) — ohne sie behauptet es eine Reife, für die kein
Nachweis existiert.

Die Live-Rezertifizierung ist dabei der einzige Schritt, der den Eigentümer
zwingend erfordert: Der Integrator startet die TUI, die WebView-Fenster öffnen
sich, und abgelaufene Sitzungen warten auf dessen Anmeldung. Das ist keine
Freigabehürde, sondern ein gemeinsamer Termin.

### Optional: realer Free-Cloud-Adapter

Der externe Adapter ist kein stiller Folgeschritt. Kosten-, Credential-,
Datenschutz- und Anbieterbedingungen müssen vor jedem realen Providerzugriff
erneut geprüft und vom Nutzer freigegeben werden.
