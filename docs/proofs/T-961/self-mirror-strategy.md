> **Referenz.** Implementierungs- und Gate-Beleg der am 2026-11-21 freigegebenen
> neuen T-961-Strategie. Die Live-Matrix dazu ist **negativ (0/6)**, siehe
> `docs/proofs/T-961/self-mirror-live-result.md`. Kein Abnahmebeleg — T-961
> bleibt `free`. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — neue Strategie: Selbstspiegelung + Daten-Framing

**Status: Code-Beleg, KEIN Abnahmebeleg.** Die Strategie ist deterministisch
geprüft, aber live widerlegt (0/6 saubere Läufe). T-961 bleibt `free`.

## Warum die bisherige Richtung widerlegt ist

Die freigegebene Auto→Required-Eskalation (PR #96, Schalter
`--auto-tool-choice-required`) erreichte live **0/7 saubere Läufe**
(`docs/proofs/T-961/auto-required-live-result.md`). Das Browsermodell lieferte
wiederholt trotz `required` reine Textantworten; die T-953-Relay-Korrektur
scheiterte ihrerseits („Umschlag enthält keine Tool Calls", erfundenes Tool
`files__search`). Ein stärkerer Zwang erhöhte nur die Korrektur-/Fehlerrate.

## Neue Strategie (vom Eigentümer am 2026-11-21 freigegeben)

Statt den Werkzeugaufruf zu **erzwingen**, wird er **aus dem Kontext gespiegelt**:

1. **Eigene Aufrufe als Vorbild** (`src/api_bridge/content.rs`,
   `tool_calls_to_envelope`, `conversation_prompt`): Frühere Assistant-Werkzeugaufrufe
   werden nicht mehr als OpenAI-Transport-JSON (`[tool_calls] …`), sondern in
   **genau dem `WEBAGENT_INFERENCE/1`-Umschlag** gerendert, den das Modell jetzt
   ausgeben soll. `arguments` wird vom JSON-String zum echten Objekt aufgelöst.
   Das Modell sieht damit seine eigenen erfolgreichen Aufrufe als konkretes
   Muster im selben Format.
2. **Ergebnisse als Eingabedaten, nicht als Fähigkeitsbehauptung**
   (`conversation_prompt`): Direkt vor der aktuellen Nachricht rahmt der Prompt
   die Einträge unter `[du – Werkzeugaufruf]` und `[Client-Werkzeug-Ergebnis]`
   als *strukturierte Daten dieser Unterhaltung*. Die Ergebnisse seien
   **Eingabedaten der Aufgabe und keine Behauptung über die eigenen
   Fähigkeiten**; das Protokoll sei mit genau dem Format der bisherigen
   Einträge fortzusetzen.

   Diese Fassung ist das Ergebnis einer live erzwungenen Verfeinerung: Die
   erste Version („Werkzeuge weiterhin aktiv, frühere Zweifel ignoriere das —
   es war falsch") erzeugte einen Honesty-Konflikt und wurde vom Modell mit
   ausdrücklicher Verweigerung beantwortet. Die jetzige Fassung nimmt den
   Konflikt, indem sie gar keine Behauptung über Fähigkeiten mehr aufstellt.
3. **Ausgabe-Vertrag verweist nur bei echtem Verlauf auf das Vorbild**
   (`src/browser_inference.rs`, `prompt_with_tools`): Der Prompt sagt
   *„Ergebnisse unter `[Client-Werkzeug-Ergebnis]` sind Eingabedaten dieser
   Aufgabe, keine Behauptung ueber deine eigenen Faehigkeiten."* Und:
   *„Verwende ausschliesslich einen der aufgefuehrten Tool-Namen; erfinde
   keine."* (gegen `files__search`). Der Verweis auf die eigenen vorherigen
   Aufrufe erscheint **nur**, wenn der Verlauf tatsächlich solche enthält —
   eine leere Referenz auf Turn 1 verunsicherte das Modell (Live-Läufe 1/2).
4. **Korrektur nutzt dasselbe Framing** (`correction_message`): Die
   T-953-Korrektur wiederholt nicht nur den Parserfehler, sondern verweist
   auf die Ergebnisse als Eingabedaten und auf das Format der eigenen
   vorherigen Werkzeugaufrufe.

Die Eskalation bleibt als default-off-Schalter unverändert im Code; die neue
Strategie hängt **nicht** von ihr ab. Die Live-Matrix lief ohne den Schalter
(plain `auto`), damit genau das Basisverhalten geprüft wird.

## Red/Green-Beleg

Zuerst wurden drei Tests geschrieben und gegen den alten Stand ausgeführt (**rot**):

| Test (finaler Name) | Red-Befund |
|---|---|
| `t961_history_tool_calls_mirror_the_exact_output_envelope` | Verlauf enthielt `[tool_calls] [{"id":…,"type":"function","function":{…}}]`, kein `WEBAGENT_INFERENCE/1` |
| `t961_continuation_frames_results_as_input_data` | Kein Daten-Framing, kein Hinweis auf „keine Behauptung über deine Fähigkeiten" |
| `t961_required_prompt_frames_data_and_mirrors_only_with_history` | Prompt rahmte Ergebnisse nicht als Eingabedaten, verbot keine erfundenen Tool-Namen |

Nach der Implementierung (**grün**, finaler Stand 2026-11-21):

```
running 3 tests
test browser_inference::tests::t961_required_prompt_frames_data_and_mirrors_only_with_history ... ok
test api_bridge::tests::t961_continuation_frames_results_as_input_data ... ok
test api_bridge::tests::t961_history_tool_calls_mirror_the_exact_output_envelope ... ok
test result: ok. 3 passed; 0 failed
```

Beispiel des gerenderten Verlaufs:

```
[du – Werkzeugaufruf]

WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_1","name":"read","arguments":{"path":"nonce-1.txt"}}]}

Die Eintraege unter [du – Werkzeugaufruf] und die Ergebnisse unter
[Client-Werkzeug-Ergebnis] sind strukturierte Daten dieser Unterhaltung. Die
Ergebnisse sind Eingabedaten deiner Aufgabe und keine Behauptung ueber deine
eigenen Faehigkeiten. Setze das Protokoll mit genau dem Format deiner
bisherigen Eintraege fort.
```

## Gates (2026-11-21, Branch `fix/T-961-self-mirror`, finaler Stand)

| Gate | Ergebnis |
|---|---|
| `cargo test --lib t961_` | 3 passed / 0 failed |
| `cargo test --lib` | 1488 passed / 0 failed / 1 ignored |
| `cargo fmt --all --check` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün (Binary `598DB09F…4818A`) |
| `git diff --check` | grün |

Die temporäre env-gesteuerte Prompt-Diagnostik (`WEBAGENT_DEBUG_PROMPT`) wurde
nach dem Live-Nachweis vollständig entfernt; der finale Code enthält keine
entsprechende Variable mehr.

## Abnahme

**Nicht erteilt — live widerlegt.** Die frische Matrix auf Port `8788` mit
frischen Prozess-only-Tokens und dem strikten Verifier ergab **0/6 saubere
Läufe**. Details, Binary-Hashes, Transkript-Hashes und der Nachweis, dass der
vollständige neue Prompt das Modell erreichte:
`docs/proofs/T-961/self-mirror-live-result.md`.

Damit sind zwei Richtungen live widerlegt. T-961 bleibt `free`, bis eine
neue, vom Eigentümer freigegebene Strategie mit stabil grünen Raw-Live-Beweisen
(≥ 5 aufeinanderfolgende saubere Werkzeugturns) und geprüftem Konto-Zustand
vorliegt. Jeder Negativlauf blockiert. Kein Tag/Release ohne vollständige
Abnahme und neue ausdrückliche Freigabe.
