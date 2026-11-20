**Referenz:** Strukturierter Konto-Befund zu T-961 vom 20.11.2026. **Kein Abnahmebeleg.** Es werden ausschliesslich strukturelle DOM-Fakten dokumentiert; persönliche Erinnerungsinhalte werden nicht in das Repository kopiert.

# T-961 — Kontozustand (read-only WebView-DOM)

Die DoD von T-961 nennt als mögliche Ursache gespeicherte ChatGPT-Erinnerungen, die Werkzeuge verneinen. Dieser Punkt ist jetzt ohne Benutzerhilfe geprüft worden.

## Methode

- Read-only DOM-Inspektion der angemeldeten ChatGPT-WebView über das vorhandene `probe --brain-id chatgpt --dump`-Werkzeug mit einem temporären, sofort zurückgerollten Diagnose-Patch (`src/brain_probe.rs`, `src/browser/operations.rs`). Der Patch fügte nur `document.body.innerText` und Link-/Button-Attribute zur vorhandenen Kandidatenliste hinzu und änderte keine Einstellungen.
- Geöffnet wurde `https://chatgpt.com/settings/personalization`. Der Unterdialog „Erinnerungszusammenfassung" wurde über den vorhandenen „Verwalten"-Knopf geöffnet (synthetischer Klick, danach echter Mausklick) und mit 30 s Wartezeit beobachtet.
- Keine Zugangsdaten eingegeben, keine Profil-, Cookie- oder Token-Dateien gelesen, keine Kontoeinstellung geändert. Der temporäre Patch wurde mit `git checkout --` entfernt; `git status` war danach sauber.

## Beobachtete Fakten

| Beobachtung | Wert |
|---|---|
| Schalter „ChatGPT-Erinnerung aktivieren" | vorhanden (`role=switch`), im Live-DOM aktiviert (`aria-checked=true`, `data-state=checked`) |
| Abschnitt „Benutzerdefinierte Anweisungen" | vorhanden, mit Bearbeiten-Schaltfläche |
| „Erinnerungszusammenfassung" | geöffnet, nach 30 s „Gerade aktualisiert" / „Überblick" |
| Eintrag, der Werkzeugzugriff verneint oder einschränkt | **nicht vorhanden** |

Der Inhalt der Zusammenfassung wurde gelesen, aber bewusst nicht in dieses Repository aufgenommen: er enthält persönliche Projekt- und Lebensumstände und ist für die T-961-Frage nicht relevant. Der entscheidende Befund ist die Abwesenheit einer Anweisung, die dem Modell den Werkzeuggebrauch untersagt.

## Einordnung

- Der Kontozustand erklärt die Intermittenz **nicht**. Er ist aber jetzt geprüft und muss nicht mehr stellvertretend vom Eigentümer kontrolliert werden.
- Die Fehlerklasse bleibt reproduzierbar (siehe `after-framing-negative.jsonl`). Die Ursache liegt weiterhin im Browser-Inferenzpfad bzw. im Modellverhalten und ist ungeklärt.
- **T-961 bleibt `free`.** Es gibt keinen PASS und kein `done`, bis stabile grüne Live-Läufe mit einem unveränderten Binary vorliegen.
