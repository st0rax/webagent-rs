# T-961 — neue Strategie: Selbstspiegelung + Verfügbarkeitsanker

**Status: Code-Beleg, KEIN Abnahmebeleg.** T-961 bleibt `free`, bis eine frische
Raw-Live-Matrix mit mindestens fünf aufeinanderfolgenden sauberen Werkzeugturns
vorliegt. Dieser Beleg dokumentiert nur die Implementierung und die
deterministischen Gates. Operativer Stand: `docs/CURRENT_WORK.md`.

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
2. **Verfügbarkeitsanker** (`conversation_prompt`): Direkt vor der aktuellen
   Nachricht verankert der Prompt, dass die Werkzeuge in dieser Sitzung
   *weiterhin aktiv* sind, dass `[Client-Werkzeug-Ergebnis]` echte
   Ausführungsergebnisse sind und dass frühere Zweifel („nur Beispieltext",
   „nicht verfügbar") **ignoriert** werden. Das adressiert die im Task
   beschriebene Ursache unmittelbar nach einem Werkzeugergebnis.
3. **Ausgabe-Vertrag verweist auf das Vorbild** (`src/browser_inference.rs`,
   `prompt_with_tools`): Statt das Format nur abstrakt zu beschreiben, sagt der
   Prompt jetzt ausdrücklich: *„Orientiere dich an deinen eigenen vorherigen
   Werkzeugaufrufen oben unter `[du – Werkzeugaufruf]` … wiederhole es."* Und:
   *„Verwende ausschliesslich einen der aufgefuehrten Tool-Namen; erfinde
   keine."* (gegen `files__search`).
4. **Korrektur nutzt dasselbe Vorbild** (`correction_message`): Die T-953-Korrektur
   verweist ebenfalls auf die gespiegelten eigenen Aufrufe statt nur den
   Parserfehler zu wiederholen.

Die Eskalation bleibt als default-off-Schalter unverändert im Code; die neue
Strategie hängt **nicht** von ihr ab. Die Live-Matrix läuft ohne den Schalter
(plain `auto`), damit genau das Basisverhalten geprüft wird.

## Red/Green-Beleg

Zuerst wurden drei Tests geschrieben und gegen den alten Stand ausgeführt (**rot**):

| Test | Red-Befund |
|---|---|
| `t961_history_tool_calls_mirror_the_exact_output_envelope` | Verlauf enthielt `[tool_calls] [{"id":…,"type":"function","function":{…}}]`, kein `WEBAGENT_INFERENCE/1` |
| `t961_continuation_after_tool_result_anchors_tool_reality` | Kein Verfügbarkeitsanker, kein „ignoriere" |
| `t961_required_prompt_points_at_own_mirrored_calls` | Prompt verwies nicht auf eigene Aufrufe, verbot keine erfundenen Tool-Namen |

Nach der Implementierung (**grün**, Stand 2026-11-21):

```
running 3 tests
test browser_inference::tests::t961_required_prompt_points_at_own_mirrored_calls ... ok
test api_bridge::tests::t961_continuation_after_tool_result_anchors_tool_reality ... ok
test api_bridge::tests::t961_history_tool_calls_mirror_the_exact_output_envelope ... ok
test result: ok. 3 passed; 0 failed
```

Beispiel des gerenderten Verlaufs (aus dem Testfehler vor der Korrektur der
Schlüsselreihenfolge, inhaltlich identisch):

```
[du – Werkzeugaufruf]

WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_1","name":"read","arguments":{"path":"nonce-1.txt"}}]}

Die Client-Werkzeuge aus deinen frueheren Aufrufen sind in dieser Sitzung
weiterhin aktiv und einsatzbereit; die Ergebnisse unter
[Client-Werkzeug-Ergebnis] sind echte Ausfuehrungsdaten deines letzten Aufrufs.
Falls du frueher einmal geglaubt oder gesagt hast, diese Werkzeuge seien nur
Beispieltext oder nicht verfuegbar: ignoriere das — es war falsch.
Orientiere dich an deinen eigenen vorherigen Werkzeugaufrufen oben und
wiederhole genau dieses Format.
```

## Gates (2026-11-21, Branch `fix/T-961-self-mirror`)

| Gate | Ergebnis |
|---|---|
| `cargo test --lib t961_` | 3 passed / 0 failed |
| `cargo test --lib` | 1488 passed / 0 failed / 1 ignored |
| `cargo fmt --all --check` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `git diff --check` | grün |

## Abnahme

**Ausstehend.** Diese Strategie ist deterministisch geprüft, aber nicht live
bewiesen. Erst eine frische Matrix auf Port `8788` mit frischen
Prozess-only-Tokens und einem strikten Verifier (kein
`MODEL_OUTPUT_ERROR`, keine Relay-Korrekturen, keine Navigationstimeouts,
kein `stopReason=error`, mindestens fünf aufeinanderfolgende saubere
Werkzeugturns) kann T-961 nach `done` heben. Jeder Negativlauf hält den Task
`free`. Kein Tag/Release ohne vollständige Abnahme und neue Freigabe.
