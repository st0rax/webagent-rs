**Referenz:** Echtes Live-Ergebnis der strukturellen Probing-Richtung „erzwungener Werkzeugaufruf" vom 21.11.2026. **Negativbeleg — kein Abnahmebeleg.** Es wurde **keine** Code-Änderung vorgenommen; T-961 bleibt `free`.

# T-961 — Kandidat „tool_choice=required" ist gescheitert

## Hypothese und warum sie geprüft wurde

Zwei Prompt-Varianten waren bereits widerlegt (`availability-candidate-result.md`: 2/8, `function-framing-result.md`: 6/8). Die seitherige Architektur-Analyse ergibt: `tool_choice: "required"` ist bereits Bestandteil des OpenAI-Adapters (`src/api_bridge/content.rs`) und wird in `src/browser_inference.rs` fail-closed durchgesetzt. Ein erzwungener Aufruf wäre also **keine** Änderung der API-Semantik und bräuchte keine Freigabe — im Gegensatz zu einer serverseitigen Auto→Required-Eskalation oder einem versteckten Retry.

Hypothese: Wenn der Client auf jedem Werkzeugturn `required` sendet, kann das Modell nicht mehr behaupten, das Werkzeug sei nicht verfügbar; der Mehrschritt-Loop wird deterministisch.

## Methode

- Unveränderter Stand `master` = `bf0a65b`, frisch gebaut; Binary-SHA256
  `3532478DE92084B9426DA97994F1D5130C3F104761E3ECF0B1DE8B5AFF59D710`.
- Direkter OpenAI-Client (kein Pi) gegen `http://127.0.0.1:8788/v1/chat/completions`,
  Modell `webagent/chatgpt`, eine `read`-Funktion, fünf aufeinanderfolgende
  Turns mit je `tool_choice: "required"`. Prozess-only-Token, frisches
  Temp-Verzeichnis pro Lauf, Port 8788, keine Repo-Dateien während der Läufe
  verändert.
- Jeder rohe Request und jede rohe Response ist in
  `required-mode-negative-{1..8}.jsonl` erhalten; der unveränderte strenge
  Verifier liegt in `required-mode-negative-verify.txt`.

## Ergebnis

| Lauf | Tool-Turns | Marker | Befund |
|---:|---:|---:|---|
| 1 | 0/5 | 0/5 | 502: *„Das Browsermodell lieferte trotz verpflichtendem Tool-Call nur Text."* |
| 2 | 0/5 | 0/5 | gleicher 502 |
| 3 | 5/5 | 5/5 | streng grün |
| 4 | 1/5 | 1/5 | 502 in Turn 2: nur Text |
| 5 | 4/5 | 4/5 | 502 in Turn 5: nur Text |
| 6 | 3/5 | 3/5 | 502 in Turn 4: nur Text |
| 7 | 1/5 | 1/5 | `ABSENDEKNOPF_DEAKTIVIERT` in Turn 2 |
| 8 | 0/5 | 0/5 | `ABSENDEKNOPF_DEAKTIVIERT` in Turn 1 |

**Ergebnis: 1/8 streng grün.** Ein zusätzlicher Validierungslauf
(`required-mode-validation-green.jsonl`, leicht abweichendes Skript mit einem
unnötigen abschliessenden Textturn) hatte ebenfalls fünf saubere Tool-Turns,
der abschliessende Textturn lief jedoch in einen Renderer-Timeout. Er ist
nicht Teil der vergleichbaren 8er-Reihe, aber der Vollständigkeit halber
aufbewahrt.

## Einordnung

1. `required` **beseitigt die Fehlerklasse nicht**. Das Modell verweigert auch
   bei erzwungenem Tool-Call; die Bridge macht daraus regressionsfest einen
   fail-closed 502 statt einer Textabsage. Das ist ein besseres Fehlerverhalten
   für Aufrufer, aber kein erfolgreicher Mehrschritt-Loop.
2. Zwei weitere Läufe scheitern an `ABSENDEKNOPF_DEAKTIVIERT` — eine
   UI-Ebene, die von der Prompt-Formulierung unabhängig ist.
3. Damit sind jetzt drei Richtungen gemessen: Verfügbarkeitsregel 2/8,
   `[FUNKTIONEN]`-Framing 6/8, erzwungener Tool-Call 1/8. Keine erreicht
   Stabilität. Eine Änderung der API-Semantik (automatisches Erzwingen unter
   `auto`) oder ein versteckter Retry wurde **nicht** implementiert; beides
   bräuchte eine ausdrückliche Freigabe und eine eigene Abnahme.
4. **T-961 bleibt `free`.** Kein PASS, kein `done`, kein Code-Merge, kein
   Tag/Release (G-009 offen). Die Ursache bleibt ungeklärt; modellseitige
   Stochastik ist weiterhin eine Vermutung, kein Beleg.
5. Alle neun Rohtranskripte sind secret-gescannt: 0 Treffer für `Bearer`,
   `apiKey`, `api_key`, `cookie`, `authorization`, `sk-`, `WEBAGENT_API_KEY`.
