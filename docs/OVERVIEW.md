# Webagent: Systemüberblick

**Stand:** 2026-08-17. Diese Datei ist die aktuelle Produkt- und
Architekturübersicht.

Nicht jede `.md` im Repo ist Soll-Zustand. `*_PLAN.md`, `*_CONCEPT.md`,
`PROGRESS.md`, `TUI_DESIGN.md`, `START_HERE.md`, `STATUS_LIVE.md` und die
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
   konfigurierten Gates ernten; Push und Merge bleiben menschliche Entscheidungen.

Sicherheitsgrenze: Das System ist ein lokaler Coding-Agent mit weitreichender
Shell. Die Policy blockiert bekannte gefährliche Muster, ersetzt aber weder OS-
Sandbox noch Rechtebegrenzung. Unvertraute Repository- und Tool-Ausgaben werden
als Daten behandelt und in Prompts markiert.

## Dokumentenlandkarte

- `README.md`: Installation und öffentliche Bedienung.
- `docs/OVERVIEW.md`: aktuelle Produktarchitektur, Reifegrade und Betrieb
  (dieses Dokument).
- `docs/ARCHITECTURE.md`: detaillierte Modulkarte und Refactoring-Befunde;
  Momentaufnahme, Zahlen vor Verwendung neu messen.
- `CONVENTIONS.md` und `AGENTS.md`: verbindliche Entwicklungsregeln.
- `docs/PROTOCOL_SCHEMA.md`: Aktionsprotokoll; gegen `src/protocol/` prüfen,
  falls sich beide widersprechen.
- `docs/PROVIDER_STATUS.md`: datiertes Live-Messjournal.
- `STATUS_LIVE.md`: chronologisches Arbeitsjournal, keine Architekturquelle.
- `*_PLAN.md`, `*_CONCEPT.md`, `CODE_REVIEW.md`, `CLAUDE_PROPOSALS.md`:
  historische Entwürfe/Reviews; nur explizit belegte Teile gelten als umgesetzt.

## Priorisierte Roadmap

### API: lokale Provider-Bridge fuer externe Agentenoberflaechen

**Status:** umgesetzt als lokaler, token-geschuetzter Loopback-Dienst. Der Befehl webagent api serve bietet einen OpenAI-Chat-Completions- und einen Anthropic-Messages-Adapter, einen token-geschuetzten Modellkatalog sowie dokumentierte Text- und Streaminggrenzen.

Der verbindliche Betriebsvertrag, Pi-Konfigurationsbeispiele und die Sicherheitsgrenzen stehen in [API_BRIDGE.md](API_BRIDGE.md).
