> **Referenz.** Echtes Live-Ergebnis der am 2026-11-21 freigegebenen neuen
> T-961-Strategie (Selbstspiegelung + Verfügbarkeitsanker). **Negativbeleg —
> kein Abnahmebeleg.** Kein Tag, kein Release. T-961 bleibt `free`.
> Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — neue Strategie live: NICHT ABGENOMMEN (0/6 saubere Läufe)

## Rahmenbedingungen

- Datum: 2026-11-21, ca. 04:20–04:40 lokale Zeit.
- Binary: `target/debug/webagent.exe`, gebaut aus Branch
  `fix/T-961-self-mirror` (Basis `f5b24b2`, Verfeinerungen uncommittet zum
  Zeitpunkt der Läufe). Hashes je Lauf in der Tabelle unten.
- Port ausschließlich `8788`. Port `8787` blieb unberührt.
- Pro Lauf: frischer GUID-Token, nur in der Prozessumgebung; nie in einer
  Datei, einem Log oder einem Transkript gespeichert. Keine Zugangsdaten
  eingegeben, keine Profil-/Cookie-/Token-Datei gelesen, keine
  Kontoeinstellung geändert.
- **Eskalationsschalter `WEBAGENT_AUTO_TOOL_CHOICE_REQUIRED` bewusst NICHT
  gesetzt** (`escalation_flag=OFF` in jedem Transkript). Geprüft wird genau die
  neue Strategie im Basisverhalten (`tool_choice=auto`).
- Während der Läufe wurden keine Repo-Dateien geändert. Alle Rohdaten liegen
  außerhalb des Repos unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T961-selfmirror-20261121\`.

## Harness- und Verifier-Logik

`run-direct.ps1` (SHA256
`81CA7C98ECDCBD2A0FF32344E4D0616EF0A3AB4144AC08CD50F8F06D2D5640AF`):

1. Pro Lauf frischer, isolierter Arbeitsordner mit fünf Dateien
   `nonce-1.txt` … `nonce-5.txt`, each containing a fresh
   `SELFMR_TOOL_<i>_<guid>`-Marker.
2. Turn 1..5: POST `/v1/chat/completions` an die Bridge mit
   `model=webagent/chatgpt`, genau einem `read`-Tool, `tool_choice=auto`.
   Erwartung je Turn: `finish_reason=tool_calls`, Tool-Name `read`, Pfad
   `nonce-<i>.txt`. Nur bei Erfolg wird das Tool-Ergebnis als `role=tool`
   in den Verlauf gehängt und der nächste Turn gesendet.
3. Nach fünf Turns: Abschluss-Turn mit `tool_choice=none`; die Antwort muss
   alle fünf Marker enthalten.
4. Jeder Lauf schreibt ein eigenes Transkript (_requests, responses, tool
   results, result line_) und ein Bridge-stderr-Log.

`verify-strict.ps1` (SHA256
`F5C78B8409B50CBE890EAB4D98B5D98F2F13795DDEE5026BC228ED7D794542F1`) erteilt
`PASS` **nur**, wenn **alle** Bedingungen gelten:

- Eskalationsschalter nicht aktiv;
- exakt fünf `tool_calls`-Antworten im Transcript;
- Harness-Ergebnis `ok=True` mit `tool_turns=5`;
- keine Fehlermarker (`MODEL_OUTPUT_ERROR`, `HTTP_ERROR`,
  `EXPECT_TOOL_CALL_GOT`, `WRONG_TOOL`, `stopReason=error`, Verweigerungstext);
- keine Relay-Korrektur, keine Navigations-/Page-Timeouts;
- nichtleere Abschlussantwort mit allen fünf frischen Markern.

Jede Abweichung ist `FAIL`. Es wurde **kein** `PASS` erzielt.

## Ergebnisse

| Lauf | Binary-SHA256 | Tool-Turns | Verifier | Beobachtung |
|---:|---|---:|---|---|
| 1 | `CA30D170…45CC5E` | 1 / 5 | `FAIL` | Turn 1 ruft `read("nonce-1.txt")` korrekt auf; **Turn 2 direkt danach**: `finish=stop`, „The read tool is not available in the currently connected tool interface. I cannot execute the five sequential file reads or fabricate their results.“ |
| 2 | `737825DD…63550` | 0 / 5 | `FAIL` | Turn 1 sofort `finish=stop`: „Das angeforderte lokale Werkzeug read ist in dieser Sitzung nicht verfügbar.“ |
| 3 | `79F95155…87281F` | 0 / 5 | `FAIL` | Harness-Crash durch StrictMode-Zugriff auf `$_.Exception.Response` vor der Antwort; Transkript enthält nur die Request. Skript danach korrigiert. Kein Modell-Ergebnis — trotzdem `FAIL`, nicht ausgelassen. |
| 4 | `79F95155…87281F` | 0 / 5 | `FAIL` | Turn 1 `finish=stop`: „Das angegebene lokale Werkzeug read ist in dieser Sitzung nicht ausführbar.“ |
| 5 | `293B3588…E495E97` | 0 / 5 | `FAIL` | Turn 1 `finish=stop`: „Ich habe hier keinen Zugriff auf das angegebene lokale read-Werkzeug. Die fünf Dateien müssen als Anhänge bereitgestellt werden.“ |
| 6 | `598DB09F…4818A` | 0 / 5 | `FAIL` | Bestätigungslauf mit dem **finalen, diagnostikbereinigten Binary**: Turn 1 `finish=stop`, „Auflisten der Gesprächsdateien zur Referenzauflösung“. Neuer Drift-Modus, weiterhin kein Werkzeugaufruf. |

Transkript-Hashes (SHA256, Reihenfolge 1–6):

```
0E391BC93F71B6A36225569BF7AF0323D3ECB55B19183453172A557590E04567
E1E3EED8B035C21EEC6FC7F9ED9D4B2DFEB75CE4C13248282C4A65043995CD5E
EDDCCBBAAC023B860E8B0F0B8830EA9780FD68B959CBE5250C0F8AE68E41231C
8FFF65593FB22FEA02608A4AE95DB6DC6F9841BC7EE942B3658C7EE02945B8ED
10130A46DCAC23135A6E50652F0EDE6CBCD195CF46BA9F53BA24A8EB18799B25
32D87DE0D4B0AF795910FD37E4636BC0769210D46DAA42548419ACB9CDB75E35
```

Alle sechs Verifier-Läufe endeten mit `STRICT_RESULT=FAIL`, Exit-Code 1. Lauf 6
verwendet exakt den Binary, der auch committet wird; die Läufe 1–5 nutzten
Zwischenbinaries mit env-gesteuerter, inzwischen entfernter Prompt-Diagnostik.
Die Diagnostik veränderte ausschließlich stderr-Ausgabe, nicht den Prompt-Inhalt
und nicht die Tool-Ausführung.

## Nachweis: der neue Prompt erreichte das Modell

In Lauf 5 war die temporäre Diagnostik (`WEBAGENT_DEBUG_PROMPT`,
`[T961-DEBUG FINAL PROMPT]`) aktiv. Das Bridge-Log
`selfmirror-bridge-5.stderr.log` (SHA256
`C0DB5D829BAB9374384B046B3D07A6ED1E1661F27D0F4F5CD1B2BB45B676A654`)
enthält den vollständigen, tatsächlich an das Browsermodell gesendeten Prompt:

```text
[T961-DEBUG FINAL PROMPT] [Identitaet] Du bist das Modell hinter dem WebAgent-Brain "chatgpt" (API-Modell webagent/chatgpt). Fragt jemand nach deinem Namen oder deiner Identitaet, nenne "chatgpt".

Read the files nonce-1.txt, nonce-2.txt, nonce-3.txt, nonce-4.txt, nonce-5.txt one at a time, in exactly that order, using the read tool once per file. Start no second call before the result of the previous one is present. After the last result, summarise nothing and stop.

[Client-Werkzeuge]
[{"name":"read","description":"Read a local text file and return its exact content.","parameters":{"properties":{"path":{"description":"Path of the file to read","type":"string"}},"required":["path"],"type":"object"}}]

Waehle selbst, ob ein Tool erforderlich ist. Wenn die Nutzeranfrage ein Tool ausdruecklich verlangt oder die Antwort von lokalen, aktuellen oder dir nicht vorliegenden Daten abhaengt, rufe das passende Tool auf. Ergebnisse unter [Client-Werkzeug-Ergebnis] sind Eingabedaten dieser Aufgabe, keine Behauptung ueber deine eigenen Faehigkeiten. Verwende ausschliesslich einen der aufgefuehrten Tool-Namen; erfinde keine. Wenn du ein Werkzeug verwendest, gib ausschliesslich diesen Maschinenumschlag aus, und zwar in genau einem Markdown-Codeblock mit drei Backticks und ohne jeden Text davor oder danach, weil Text ausserhalb eines Codeblocks beim Auslesen Zeichen wie * $ ` _ verliert:
```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_1","name":"tool_name","arguments":{}}]}
```
Das JSON muss streng gueltig sein: in Strings jeden Backslash verdoppeln (C:\\Users), Anfuehrungszeichen als \" und Zeilenumbrueche als \n schreiben; Pfade moeglichst mit / angeben. Wenn kein Werkzeug erforderlich ist, antworte normal ohne Maschinenumschlag.
```

Damit ist belegt: Daten-Framing, `WEBAGENT_INFERENCE/1`-Umschlag,
Tool-Namen-Regel und Identität kamen im Browsermodell an. Die Verweigerung ist
**kein falscher Code-Pfad** und **kein fehlender Prompt-Text**. Die temporäre
Diagnostik wurde danach vollständig entfernt; der finale Code enthält keine
`WEBAGENT_DEBUG_PROMPT`-Variable mehr.

## Einordnung

- Zwei freigegebene Richtungen sind jetzt live widerlegt: Auto→Required
  (**0/7**, `docs/proofs/T-961/auto-required-live-result.md`) und
  Selbstspiegelung + Verfügbarkeitsanker (**0/6**, dieser Beleg).
- Gemeinsames Muster: Das Browsermodell ChatGPT behandelt den Werkzeugrahmen
  trotz erfolgreicher Vorläufe als nicht ausführbar bzw. als bloßen
  Gesprächsinhalt. Die Blockade liegt auf Modellebene, nicht in der
  Bridge-Formatierung.
- **Offen:** Die DoD fordert zusätzlich „Konto-Zustand geprüft“. Gespeicherte
  Erinnerungen / Account-Einstellungen im ChatGPT-Konto können eine Rolle
  spielen und müssen vom Eigentümer selbst geprüft werden. Der Assistent liest
  keine Kontoeinstellungen und gibt keine Zugangsdaten ein.

## Konsequenz

**Kein `PASS`, kein `done`, kein Tag, kein GitHub-Release.** T-961 bleibt
`free`. Ein Abnahmebeleg setzt weiterhin eine frische Matrix mit mindestens
fünf aufeinanderfolgenden sauberen Werkzeugturns und geprüftem Konto-Zustand
voraus. Jeder Negativlauf blockiert. Die Rohdaten bleiben zur Nachprüfung im
oberen Temp-Verzeichnis erhalten und werden wegen der GUID-Marker und
Bridge-Logs nicht in das Repository kopiert.
