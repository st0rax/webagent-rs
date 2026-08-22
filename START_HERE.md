# Hier anfangen

Dies ist der dauerhafte Einstieg für jeden neuen menschlichen oder KI-
Entwickler. Nicht mit einer datierten Übergabe oder `STATUS_LIVE.md` beginnen.

## Übernahme in fünf Minuten

1. Git-Wurzel und Zustand prüfen:

   ```powershell
   git rev-parse --show-toplevel
   git status --short --branch
   git log -5 --oneline --decorate
   ```

   Vor dem Editieren stoppen, wenn fremde Änderungen vorhanden sind. Nichts
   überschreiben; erst Eigentümer und Dateizuständigkeit klären.

2. In dieser Reihenfolge lesen:

   - [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md): aktueller Arbeitsstand,
     Evidenz und nächste sichere Aktion;
   - [`docs/OVERVIEW.md`](docs/OVERVIEW.md): Produktwahrheit, Architektur,
     Reifegrade und Roadmap;
   - [`AGENTS.md`](AGENTS.md): verbindliche Repo- und Agentenregeln;
   - [`CONTRIBUTING.md`](CONTRIBUTING.md): Setup, Workflow und Abnahme;
   - [`docs/COLLABORATION.md`](docs/COLLABORATION.md): GitHub-Auftrag,
     Zuständigkeit, Review und Entwicklerwechsel;
   - [`CONVENTIONS.md`](CONVENTIONS.md): Designregeln.

3. Vor einer Codeänderung die lokale Basis feststellen:

   ```powershell
   cargo fmt --all -- --check
   cargo clippy --no-default-features --all-targets -- -D warnings
   cargo test --no-default-features
   ```

   Unter Windows zusätzlich, sofern Toolchain und WebView2-Abhängigkeiten
   vorhanden sind:

   ```powershell
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

4. Genau eine begrenzte Aufgabe aus `docs/CURRENT_WORK.md` übernehmen. Vor
   einem neuen Modul, Store, Protokoll oder CLI-Flag mit `rg` nach dem
   bestehenden Mechanismus suchen. Dateien und Abnahmetest vorab benennen.

5. Mit einem fokussierten Commit, proportional grünen Gates, aktualisierter
   `docs/CURRENT_WORK.md` und einem Push des Arbeitsbranches abschließen — der
   Stand muss jederzeit über GitHub ziehbar sein. Grüne, abgeschlossene
   Scheiben merged der Integrator selbst; Tag und Release folgen nach den
   Abnahmebelegen. Live-Browser/Login und kostenpflichtige Provider laufen nur
   mit anwesendem Eigentümer, der die Anmeldung selbst vornimmt.

## Rangfolge der Wahrheitsquellen

Bei Widersprüchen gilt:

1. ausführbarer Code und Tests;
2. `docs/CURRENT_WORK.md` für aktuelle Arbeit und Evidenz;
3. `docs/OVERVIEW.md` für Produktstand und Roadmap;
4. `AGENTS.md`, `CONTRIBUTING.md`, `docs/COLLABORATION.md` und
   `CONVENTIONS.md` für Arbeitsregeln;
5. `README.md` und `docs/PROTOCOL_SCHEMA.md` für Bedienung und Verträge;
6. datierte Übergaben, Pläne, Reviews und `STATUS_LIVE.md` nur als Historie.

Historische Providerwerte sind kein aktueller Verfügbarkeitsbeleg. Ein
abgeschlossenes Goal oder grüne Unit-Tests sind keine Gesamtproduktabnahme.

## Kurzbild des Projekts

Webagent ist ein lokaler Rust-Harness: Web-Chat-„Brains“ planen, während ein
Controller über einen Plan-Act-Observe-Loop Werkzeuge im Workspace ausführt.
`BrainBackend` hält den Kern providerunabhängig. Das produktive Browserbackend
nutzt angemeldete Embedded-WebView2-Sitzungen unter Windows; Kern, REPL und TUI
besitzen zusätzlich browserfreie Pfade.

Das Repo kann neben oder innerhalb alter Python- und Experiment-Worktrees
liegen. Maßgeblich sind ausschließlich die von `git rev-parse --show-toplevel`
ausgegebene Git-Wurzel, der ausgecheckte Branch und dessen eigenes `origin`.

## Übergaberegel

Wer eine Aufgabe abgibt, aktualisiert `docs/CURRENT_WORK.md` mit:

- exaktem Branch und Commit;
- eigenen oder schmutzigen Dateien samt Grund;
- erledigter Arbeit und offenen Abnahmekriterien;
- tatsächlich ausgeführten Befehlen und Ergebnissen;
- genau einer sichersten nächsten Aktion;
- weiterhin freigabepflichtigen externen Aktionen.

Chatverlauf ist optionale Zusatzinformation, nie die einzige Übergabe.
