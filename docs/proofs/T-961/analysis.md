**Referenz.** Ehrliche Auswertung der echten Live-Laeufe vom 20.11.2026; **kein
Abnahmebeleg**. T-961 bleibt `free`. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 Live Acceptance - NICHT ABGENOMMEN (intermittierendes Verhalten)

## Kurzergebnis

Es gibt jetzt einen **echten, gruenen Lauf** (`after.jsonl`): Textturn antwortet
`PI_BRIDGE_OK`, und der Tool-Turn liest alle fuenf Nonces in Folge erfolgreich
und gibt sie in der Schlussantwort zurueck. Fruehere Laeufe an diesem Tag
scheiterten an drei **deterministischen** Blockern, die alle behoben und gemergt
sind (PR #82, #83, #84).

Aber: ein weiterer Lauf **mit exakt demselben Binary** (`after-negative-intermittent.jsonl`,
Minuten vor dem gruenen Lauf) reproduziert das **urspruengliche T-961-Symptom**:
nach dem erfolgreichen ersten `read` schreibt chatgpt *„Der Rückgabewert von
nonce-1.txt fehlt. Daher kann ich den zweiten sequenziellen Aufruf noch nicht
starten."* und bricht ab. Das ist genau die gemeldete Fehlerklasse (Werkzeug
leugnen direkt nach erfolgreichem Werkzeugaufruf).

**Deshalb kein PASS, kein `done`.** Ein einzelner gruener Lauf beweist bei einem
nachweislich intermittierenden Verhalten nicht, dass die Ursache behoben ist.
Die DoD verlangt einen belastbaren Beleg, nicht einen guenstigen Zufall. Die
Reproduzierbarkeit ist offen; T-961 bleibt `free`.

## Die drei behobenen deterministischen Blocker

| Blocker | Symptom im Live-Lauf | Fix (gemergt) |
|---|---|---|
| Selektor-Drift | Antwort war im DOM sichtbar, `probe_generation` sah `count=0/text=""` -> `timeout_no_message`; Bridge 502 | PR #82 `901e8e1` (`selectors/chatgpt.json`) |
| Inference-Envelope | chatgpt gab gueltigen Tool-Call aus, aber mit `Nur Text\n` davor -> `strip_prefix` verwarf ihn, Pi sah nur Text, 0 Tool-Starts | PR #83 `5355e18` (`src/browser_inference.rs`) |
| Login-Vorabpruefe | `session_state=Unbestimmt` (False Positive) blockierte den Prompt, obwohl die Sitzung funktionierte | PR #84 `976abd3` (`src/brain.rs`, `src/relay.rs`) |

Diese Fixes sind Code-Aenderungen; der gruene Lauf ist der erste echte Live-Nachweis,
dass die Kette Textturn + Tool-Fortsetzung jetzt ueberhaupt durchlaeuft.

## Was der gruene Lauf (`after.jsonl`, 51.529 B) wirklich zeigt

| Pruefung | Ergebnis |
|---|---|
| Textturn assistant | `stop=stop`, `content="PI_BRIDGE_OK"` (echte Antwort, kein Echo) |
| Tool-Starts | 6 (call_1 doppelt, dann call_2..call_5) |
| Nonces gelesen | nonce-1..5, alle `isError=false` |
| Schlussantwort | enthält alle fuenf `PI_TOOL_*`-Marker in Reihenfolge |
| `SESSION_ERRORS` | 0 |
| `CIRCUIT_ERRORS` | 0 |
| `SUSPECT_PATTERNS` | 0 |
| Harness-Gates | `toolStarts -lt 5` und fehlende-Marker-Pruefung unveraendert aktiv |
| Secret-Scan | 0 Treffer fuer Bearer/apiKey/cookie/authorization/sk-/WEBAGENT_API_KEY |

Die 112 Hex-Treffer im groben `[A-Fa-f0-9]{32,}`-Scan sind ausschliesslich die
fuenf absichtlichen `PI_TOOL_*`-Nonces und die Temp-Verzeichnis-GUID — keine
Zugangsdaten.

## Was der negative Lauf (`after-negative-intermittent.jsonl`, 27.075 B) zeigt

Gleicher Binary (`976abd3`), gleiche Prompts, frisches Temp-Verzeichnis,
Minuten vor dem gruenen Lauf:

- Textturn: `PI_BRIDGE_OK`, 0 Session-/Circuit-Fehler (die PR-#84-Schwelle wirkt).
- Tool-Turn: `tool_execution_start` call_1 -> `PI_TOOL_1_...`, `isError=false`.
- Danach assistant `stop=stop`: *„Der Rückgabewert von nonce-1.txt fehlt. Daher
  kann ich den zweiten sequenziellen Aufruf noch nicht starten."*
- `TOOL_STARTS=1`, fehlende Marker 4, `SCRIPT_OK=False`.

Pi sendete das `toolResult` mit `PI_TOOL_1_...` korrekt an die Bridge (im
Transcript sichtbar). Der Fehler liegt im weiterleitungspfad bzw. im Modell,
nicht in Pi. Die Rahmenung in `conversation_prompt` (PR #78/`864e346`, bereits
in `master`) labels Tool-Ergebnisse als `[Client-Werkzeug-Ergebnis]`; warum das
Ergebnis manchmal nicht als ausfuehrungsdaten erkannt wird, ist **ungeklaert**.

## Einordnung / naechster Schritt

1. **Nicht abnehmen.** Ein gruener Lauf bei gleichzeitigem Negativlauf mit dem
   selben Binary ist kein Beleg fuer eine behobene Ursache.
2. Ursache der Intermittenz eingrenzen: Tool-Ergebnis-Rahmung variieren
   (Klarstellung „Ausfuehrungsdaten, nicht zitierter Inhalt"), Konto-/Memory-
   Zustand ihres chatgpt-Kontos pruefen (möglicher Einfluss gespeicherter
   Erinnerungen aus Sitzungen, in denen Werkzeuge verneint wurden — nur durch
   Eigentuemer einsehbar, kein Agent liest Profile).
3. Danach mehrere aufeinanderfolgende gruene Laeufe fordern und hier ablegen;
   erst bei stabilem Verhalten T-961 auf `done` setzen.
4. Bis dahin: `Status: NICHT ABGENOMMEN / INTERMITTIEREND`. Kein PASS, kein
   `done`, kein Tag/Release (G-009 bleibt offen).
