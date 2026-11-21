**Referenz.** Ehrliche Auswertung der echten Live-Laeufe vom 20.11.2026; **kein
Abnahmebeleg**. T-961 bleibt `free`. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 Live Acceptance - NICHT ABGENOMMEN (Intermittenz besteht fort)

## Kurzergebnis

Drei deterministische Blocker sind behoben und gemergt (PR #82/#83/#84). Eine
weitere, objektiv nachweisbare Rahmungsasymmetrie wurde gefunden und mit PR #86
(`eaed3d1`) behoben: das **aktuelle** Werkzeugergebnis war im Browser-Prompt
nackter Text, waehrend historische Ergebnisse als `[Client-Werkzeug-Ergebnis]`
mit `[tool_call_id]` gerahmt waren.

Der Live-Erfolg ist **nicht stabil**: mit dem finalen Binary `eaed3d1` waren 3
von 4 aufeinanderfolgenden Laeufen gruen, aber der erste Lauf reproduzierte die
T-961-Fehlerklasse erneut — diesmal nach vier erfolgreichen Reads mit dem Wortlaut
*„Das fünfte read-Werkzeug ist in dieser Sitzung nicht verfügbar."*
**Deshalb kein PASS, kein `done`, T-961 bleibt `free`.**

## Nachtrag 21.11.2026: freigegebene Auto→Required-Eskalation — 0/7 sauber

Der Eigentuemer hat am 21.11.2026 die serverseitige Eskalation `auto` →
`required` freigegeben (PR #96, `f85010a`; Code im Rebuild `61DC1281…`). Die
frische Live-Matrix mit sieben Laeufen auf Port 8788, frischen Prozess-Tokens
und gesetztem Schalter ist in `auto-required-live-result.md` dokumentiert.
Ergebnis: **kein einziger sauberer Lauf**. Das Modell liefert wiederholt trotz
`required` reine Textantworten; die vorbestehende T-953-Relay-Korrektur rettet
einzelne Turns, scheitert aber oftmals ihrerseits („Umschlag enthaelt keine Tool
Calls", „unbekanntes Tool `files__search`"). Zwei Pi-Laeufe und vier direkte
Laeufe endeten mit HTTP 502 `model_output_envelope_invalid`, Navigationstimeouts
oder einem 180-s-Timeout im Schlussturn. Ein Pi-Lauf meldete zwar „PASS", hatte
aber 15 Relay-Korrekturen, 20 `stopReason=error` und keine Textabschlussantwort;
der strikte Verifier weist ihn zurueck.

Damit ist auch diese Richtung live widerlegt. T-961 bleibt `free`; eine erneute
Aenderung der Eskalationsstrategie braucht eine neue Freigabe. Kein Tag/Release.

## Behobene deterministische Blocker

| Blocker | Symptom im Live-Lauf | Fix (gemergt) |
|---|---|---|
| Selektor-Drift | Antwort im DOM sichtbar, `probe_generation` sah `count=0/text=""` -> `timeout_no_message`, Bridge 502 | PR #82 `901e8e1` (`selectors/chatgpt.json`) |
| Inference-Envelope | gueltiger Tool-Call mit `Nur Text\n` davor -> `strip_prefix` verwarf ihn, 0 Tool-Starts | PR #83 `5355e18` (`src/browser_inference.rs`) |
| Login-Vorabpruefe | `session_state=Unbestimmt` (False Positive) blockierte den Prompt | PR #84 `976abd3` (`src/brain.rs`, `src/relay.rs`) |
| Rahmung des aktuellen Tool-Ergebnisses | letzte Nachricht (`role=tool`) ohne Label -> Modell musste sie aus Rohtext als eigene Ausfuehrungsdaten erraten | PR #86 `eaed3d1` (`src/api_bridge/content.rs`, Red/Green-Test) |

PR #86 ist ein korrekter, unit-getesteter Fix der Asymmetrie, aber **kein
Nachweis der Ursache**: der negative Live-Lauf blieb.

## Alle echten Live-Laeufe (20.11.2026)

| Datei | Binary | Tool-Starts | Ergebnis | SHA256 (rohes Transkript) |
|---|---|---:|---|---|
| `before.jsonl` | `dd78128` | 0/5 | Baseline: `session_state=Unbestimmt`, `circuit_open` | – |
| `after.jsonl` | `976abd3` | 6/5 | gruen: alle 5 Nonces `isError=false`, Finalanswer mit allen Markern | `71FD0429...C8977` |
| `after-negative-intermittent.jsonl` | `976abd3` | 1/5 | **negativ**: „Der Rückgabewert von nonce-1.txt fehlt ..." | `ECAF124A...C935C` |
| `after-premerge-green-1.jsonl` | `37fe0fc` (Fix-Branch) | 5/5 | gruen | `EBDC87A1...C2B8` |
| `after-premerge-green-2.jsonl` | `37fe0fc` (Fix-Branch) | 5/5 | gruen | `6F3CD623...E835` |
| `after-framing-negative.jsonl` | `eaed3d1` (master) | 4/5 | **negativ**: „Das fünfte read-Werkzeug ist in dieser Sitzung nicht verfügbar." | `167FACC5...28B9` |
| `after-framing-green-1.jsonl` | `eaed3d1` (master) | 5/5 | gruen | `A1E92F04...CB75` |
| `after-framing-green-2.jsonl` | `eaed3d1` (master) | 5/5 | gruen | `A0A26019...5304` |
| `after-framing-green-3.jsonl` | `eaed3d1` (master) | 5/5 | gruen | `229478DD...26C2` |

Alle Laeufe: gleiche Prompts, Pi 1.5.2, Port 8788, frische Prozess-Tokens,
frisches Temp-Verzeichnis. Die Transkripte sind rohe Pi-JSON-Modus-Streams;
`docs/proofs/T-961/*.jsonl` sind per `.gitattributes` bytegenau gespeichert
(keine CRLF-Konvertierung), damit die SHA256-Werte stimmen.

## Was die gruenen Laeufe belegen

- Textturn: assistant `stop=stop`, echter Inhalt `PI_BRIDGE_OK` (kein Echo).
- Tool-Turn: fuenf aufeinanderfolgende `read`-Aufrufe, alle `isError=false`,
  Schlussantwort enthaelt alle fuenf `PI_TOOL_*`-Marker in Reihenfolge.
- `SESSION_ERRORS=0`, `CIRCUIT_ERRORS=0`, `SUSPECT_PATTERNS=0`.
- Harness-Gates unveraendert: `toolStarts -lt 5` und fehlende-Marker-Pruefung
  sind aktiv; `SCRIPT_OK` wurde nicht erleichtert.
- Secret-Scan ueber alle Transkripte: 0 Treffer fuer Bearer/apiKey/cookie/
  authorization/sk-/WEBAGENT_API_KEY. Hex-Treffer sind die absichtlichen
  `PI_TOOL_*`-Nonces und Temp-GUIDs.

## Was der negative Final-Lauf (`after-framing-negative.jsonl`, 42.458 B) belegt

- Textturn: `PI_BRIDGE_OK`, 0 Session-/Circuit-Fehler.
- call_1..call_4: erfolgreich (`isError=false`), Nonces 1..4 gelesen.
- Danach assistant `stop=stop`: *„Die ersten vier Dateiinhalte liegen vor. Das
  fünfte read-Werkzeug ist in dieser Sitzung nicht verfügbar."* — die vier
  bekannten Marker werden ausgegeben, der fuenfte fehlt.
- `TOOL_STARTS=4`, fehlender Marker 5, `SCRIPT_OK=False`.

Damit ist die Fehlerklasse der Task (Werkzeugzugriff nach eigenem erfolgreichen
Aufruf verneinen) **weiterhin reproduzierbar**, nur an anderer Stelle und mit
anderem Wortlaut. Pi uebermittelt die `toolResult`-Nachrichten korrekt; die
Ursache liegt im Browser-Inferenzpfad bzw. im Modellverhalten und ist
**ungeklaert**.

## Einordnung / naechster Schritt

1. **Nicht abnehmen.** 3/4 gruen ist besser als vorher, aber ein Negativlauf mit
   demselben Binary verbietet `done`. Die DoD verlangt eine eingegrenzte Ursache
   und eine belastbare Transkriptfolge.
2. **Konto-Zustand ist geprueft (20.11.2026, read-only WebView-DOM).** Die
   Personalisierungsseite der angemeldeten WebView zeigt den aktivierten
   Erinnerungs-Schalter und den Abschnitt „Benutzerdefinierte Anweisungen".
   Die „Erinnerungszusammenfassung" wurde geoeffnet; nach 30 s war der
   Inhalt aktuell („Gerade aktualisiert"/„Überblick") und enthielt **keinen**
   Eintrag, der Werkzeugzugriff verneint. Persönliche Inhalte sind nicht in
   das Repository aufgenommen, keine Einstellungen geändert, keine Profile,
   Cookies oder Zugangsdaten gelesen. Details: `account-state.md`. Die
   Intermittenz wird dadurch nicht erklaert.
3. Danach erneut mehrere aufeinanderfolgende gruene Laeufe mit dem dann
   unveraenderten Binary fordern und hier ablegen; erst bei stabilen Laeufen
   T-961 auf `done` setzen. Der Kandidat mit neutraler Werkzeug-
   Verfuegbarkeitsregel (`0039cd4`) wurde live gemessen und ist **gescheitert**:
   nur 2/8 Laeufe gruen, teils Absagen vor dem ersten Werkzeugaufruf. Die
   Code-Aenderung wurde zurueckgenommen; Rohtranskripte und Auswertung liegen
   in `availability-negative-{1..8}.jsonl` und
   `availability-candidate-result.md`. Das spricht dafuer, bereits das
   Meta-Thema „Verfuegbarkeit" im Prompt zu vermeiden. Ein zweiter Kandidat ohne dieses Meta (Block `[FUNKTIONEN]` statt `[Client-Werkzeuge]`, `f899ed3`) erreichte 6/8 streng gruen, scheiterte aber in zwei Laeufen mit exakt derselben Fehlerklasse — keine Verbesserung gegenueber der 3/4-Baseline. Auch er wurde zurueckgenommen; Belege: `function-negative-{1..8}.jsonl`, `function-negative-verify.txt`, `function-framing-result.md`. Reine Prompt-Wortwahl ist damit als Richtung widerlegt.
   Eine dritte, strukturelle Richtung wurde ebenfalls gemessen: erzwungenes
   `tool_choice=required` über einen direkten OpenAI-Client auf dem
   unveränderten `master` `bf0a65b` ergab nur **1/8** streng grüne Läufe. In
   sechs Läufen lieferte das Modell trotz Verpflichtung nur Text (fail-closed
   502), zwei Läufe scheiterten an `ABSENDEKNOPF_DEAKTIVIERT`. Belege:
   `required-mode-negative-{1..8}.jsonl`, `required-mode-negative-verify.txt`,
   `required-mode-result.md`. Damit ist auch die strukturelle Variante ohne
   API-Semantik-Änderung widerlegt; eine serverseitige Auto→Required-Eskalation
   oder ein versteckter Retry bleibt eine Freigabeentscheidung des Eigentümers.
4. Bis dahin: `Status: NICHT ABGENOMMEN / INTERMITTIEREND`. Kein PASS, kein
   `done`, kein Tag/Release (G-009 bleibt offen).
