# Auftrag: webagent-rs zu einem eigenständigen, Pi-inspirierten Coding-Agenten weiterentwickeln

## Ziel

Entwickle das bestehende Rust-Projekt `webagent-rs` zu einem leistungsfähigen, modularen Coding-Agenten weiter, der sich konzeptionell an Pi orientiert, aber **keine 1:1-Kopie** ist.

Der Agent soll Aufgaben verstehen, den Code untersuchen, Dateien bearbeiten, Befehle ausführen, Ergebnisse überprüfen und mehrstufige Aufgaben zuverlässig bis zum Abschluss bearbeiten können. Er soll sich über unterschiedliche Modelle und Backends nutzen und erweitern lassen.

Das vorhandene Projekt ist Ausgangspunkt. Es ist kein Anlass, das Repository blind neu zu schreiben.

## 1. Zuerst den Ist-Zustand untersuchen

Untersuche das vollständige Repository, bevor du Änderungen vornimmst.

- Lies Architektur, Module, Abhängigkeiten, Konfiguration, Tests und Dokumentation.
- Prüfe die bestehende Agentenlogik, Browser-/WebView-Integration, Protokolle, Modellanbindung und vorhandene Tool-Schnittstellen.
- Führe vorhandene Tests aus und dokumentiere den tatsächlichen Ausgangszustand.
- Identifiziere wiederverwendbare Komponenten, technische Schulden und fehlende Fähigkeiten.
- Unterscheide nachweislich vorhandene Funktionen von bloßen Annahmen.

**Wichtig:** Verwechsle die vorhandene Browser-Automatisierung nicht mit einem vollständigen Coding-Agenten. Bewahre sie als eigenständige Fähigkeit und integriere sie über eine saubere Schnittstelle, sofern das architektonisch sinnvoll ist.

Ändere zunächst keinen bestehenden Code.

## 2. Zielarchitektur definieren

Erstelle auf Grundlage der tatsächlichen Codebasis einen konkreten Architekturvorschlag.

Untersuche insbesondere diese Bereiche:

### Agentenkern

- Robuster Agent-Loop mit Tool-Aufrufen und Tool-Ergebnissen
- Streaming von Modellantworten und Tool-Events
- Mehrstufige Aufgabenbearbeitung ohne künstliche Statusmeldungen
- Abbruch, Fortsetzung, Fehlerbehandlung und begrenzte Wiederholungsversuche
- Klare Trennung von Modellantworten, internen Ereignissen und Tool-Ausgaben

### Coding-Werkzeuge

- Dateien lesen und durchsuchen
- Dateien erstellen und überschreibend schreiben
- Gezielte Änderungen an bestehenden Dateien
- Shell- und Prozessausführung mit erfassten Ausgaben und Exitcodes
- Diffs, Änderungsübersichten und überprüfbare Ergebnisse
- Bestehende Browser-Automatisierung als optionales Werkzeug

### Modelle und Backends

- Einheitliche Schnittstelle für Modellanbieter und Inferenz-Backends
- Streaming, Tool-Calling und unterschiedliche Fähigkeiten der Modelle
- Korrekte Behandlung von Nachrichtenrollen, Tool-IDs, Argumenten und Ergebnissen
- Konfigurierbare Modelle ohne unnötige Bindung an einen Anbieter
- Klare Trennung zwischen Modelladapter, Agentenlogik und Werkzeugen

Erfinde keine Kompatibilität, die das jeweilige Backend nicht unterstützt.

### Kontext und Sitzungen

- Persistente Sitzungen und Fortsetzung nach Neustart
- Sinnvolle Verwaltung des Gesprächsverlaufs
- Kontextbegrenzung und Zusammenfassung älterer Nachrichten
- Laden projektbezogener Anweisungen
- Skills und wiederverwendbare Prompt-Vorlagen mit bedarfsgerechtem Laden
- Nachvollziehbare Herkunft zusätzlicher Anweisungen

### Bedienung und Integration

- Interaktive Terminaloberfläche, sofern zur vorhandenen Architektur passend
- Skriptmodus für automatisierte Aufrufe
- Strukturierter Ereignisstrom, beispielsweise JSONL
- Eine geeignete RPC- oder API-Schnittstelle für externe Anwendungen
- Konfigurierbare Berechtigungen und Bestätigungen für riskante Aktionen

### Erweiterbarkeit

- Klar definierte Schnittstellen für Werkzeuge, Modellanbieter und Erweiterungen
- Kleine, verständliche Kernarchitektur
- Neue Fähigkeiten sollen möglichst ohne Änderungen am gesamten Agentenkern integrierbar sein.

## 3. Nicht Pi blind nachbauen

Verwende Pi als Referenz für Architekturprinzipien und Arbeitsabläufe, nicht als starre Spezifikation.

Übernimm nur Funktionen, die einen konkreten Nutzen für dieses Projekt haben. Begründe größere Architekturentscheidungen anhand der bestehenden Implementierung.

Bevorzuge einen kleinen, testbaren Kern und austauschbare Komponenten gegenüber einer monolithischen Implementierung.

## 4. Umsetzung in überprüfbaren Etappen

Erstelle eine priorisierte `TASKBOARD.md` mit:

- Aufgaben und Abhängigkeiten
- Priorität und Akzeptanzkriterien
- Betroffenen Modulen
- Konkreten Tests
- Status: offen, in Arbeit oder nachweislich abgeschlossen

Arbeite anschließend inkrementell. Beginne nach der Bestandsaufnahme mit der sinnvollsten ersten Implementierungsetappe, statt nach dem Architekturvorschlag stehenzubleiben.

Für jede Etappe:

1. Implementiere eine klar abgegrenzte Funktion.
2. Ergänze oder aktualisiere Tests.
3. Führe die relevanten Tests tatsächlich aus.
4. Prüfe Regressionen und bestehende Funktionalität.
5. Aktualisiere Dokumentation und Taskboard.
6. Erstelle einen aussagekräftigen Git-Commit.
7. Führe regelmäßige Pushes durch, sofern ein korrekt konfiguriertes Remote verfügbar und der Push autorisiert ist.

Keine großen, ungetesteten Sammeländerungen. Keine behaupteten Tests, Builds oder Pushes, die nicht tatsächlich ausgeführt wurden.

## 5. Qualitätsanforderungen

- Vorhandene funktionierende Funktionen nicht ohne Not entfernen.
- Bestehende Architektur vor einer Neuentwicklung kritisch bewerten.
- Keine unnötigen Abhängigkeiten.
- Fehler nicht verschlucken.
- Tool-Aufrufe und ihre Ergebnisse nachvollziehbar protokollieren.
- Sicherheitsrelevante Aktionen explizit behandeln.
- Keine Zugangsdaten in Logs, Sitzungen oder Commits offenlegen.
- Tests müssen die reale Funktion prüfen und dürfen Erfolg nicht bloß simulieren.
- Technische Entscheidungen und bekannte Einschränkungen dokumentieren.

## 6. Was zuerst geliefert werden soll

Liefere zunächst:

1. Eine präzise Zusammenfassung des aktuellen Repository-Zustands.
2. Eine Übersicht der vorhandenen und fehlenden Fähigkeiten.
3. Einen Architekturvorschlag mit Begründung.
4. Die priorisierte `TASKBOARD.md`.
5. Die erste Umsetzungsetappe mit konkreten Akzeptanzkriterien.

Setze nach dieser Bestandsaufnahme die Umsetzung fort. Halte die Änderungen klein und überprüfbar. Wenn eine Entscheidung tatsächlich blockiert ist, stelle eine konkrete Rückfrage; ansonsten triff eine nachvollziehbare Entscheidung und dokumentiere sie.

**Erfolg bedeutet nicht, möglichst viele Dateien zu ändern. Erfolg bedeutet, dass aus dem vorhandenen Projekt schrittweise ein funktionierender, testbarer und erweiterbarer Coding-Agent wird.**
