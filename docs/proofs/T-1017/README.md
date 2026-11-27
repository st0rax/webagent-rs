# T-1017 - Browser-Prompt-Budget statt stiller UI-Laengenablehnung

> **Referenz.** Task-Beleg für T-1017. Die Änderung begrenzt das
> Werkzeug-Schema und lehnt überlange Prompts vor dem Browserlauf ab. Sie
> kürzt keinen Nutzertext und beschreibt zunächst keinen Deploy.

- Task: T-1017
- Owner: local/opencode
- Branch: `fix/T-1017-browser-prompt-budget`
- Status: umgesetzt und gates-grün; Gates siehe `gates.txt`
- Basen: `origin/master` `e75c895`

## Befund

Die aktive T-1016-EXE (`D96F4999…EEAB`) lieferte für `webagent/gemini` einen
Live-PASS. Für `chatgpt` mit `tools=12` scheiterten die Turns dagegen
reproduzierbar in der Provider-Oberfläche:

- `brain_score/events.jsonl`, z. B. `2026-11-26T23:29:38+01:00`:
  `prompt_chars=196030`, `turn.pasted_chars=199811`,
  `reason=send_phase=submit: ABSENDEKNOPF_DEAKTIVIERT …`, `sends=0`,
  `latency_ms≈219000`.
- Der externe `opencode-cli.exe serve --service` (PID 4944) sendet laufend
  weitere `tools=12`-Turns. Die per-Brain-Sperre serialisiert sie; kurze
  Kontroll-Turns verhungern bzw. laufen in Client-Timeouts.
- Das ist eine UI-Längenablehnung nahe 200k Zeichen, **kein** Selektorenfehler
  und kein Rückfall von T-1016. Der Breaker öffnete mehrfach für 900 s.

## Änderung

`crates/webagent-core/src/inference_types.rs`:

- `MAX_TOOL_SCHEMA_BYTES`: `64 * 1024` → `12 * 1024`.
  `TOOL_DESCRIPTION_LIMITS = [0]` entfernt **alle** Beschreibungen sofort
  (statt Zwischenstufen 1024/512/256/128/0 zu versuchen); **alle**
  Werkzeugnamen, Parameternamen und Typen bleiben erhalten. Ein 12-Tool-
  Schema schrumpft damit von ~9 KiB auf ~2,4 KiB.
- `MAX_BROWSER_PROMPT_CHARS` zuerst `180_000`, dann `140_000`, jetzt
  **`120_000`**. Die untere Beobachtung ist ein Antwort-PASS bei 102.806
  Zeichen; bei 130.215 Zeichen kam trotz Sendung nach 353 s kein Text mehr.
  Der Guard liegt jetzt unterhalb dieser Silent-Non-Response-Grenze.

`crates/webagent-api/src/browser_inference.rs`:

- Neue `check_browser_prompt_budget(prompt)` direkt nach
  `prompt_with_tools` + `prompt_with_identity` in
  `complete_streaming_with_attachments`. Damit ist der Guard vor dem
  Browserlauf wirksam; `complete` und `api_bridge::inference` nutzen
  denselben Pfad.
- Fehlertext ist benannt: `Prompt zu gross fuer die Browser-Oberflaeche:
  <n> Zeichen (Maximum 120000); Werkzeugbeschreibungen sind bereits
  komprimiert`. Ein solcher Turn scheitert sofort statt nach 90–350 Sekunden
  und erzeugt keinen `ABSENDEKNOPF_DEAKTIVIERT`-Breakereintrag mehr.

Neue Regressionstests:

- `twelve_verbose_tools_stay_within_the_browser_schema_budget` - 12 Tools
  mit sehr langen Beschreibungen bleiben unter dem Schema-Budget, Namen und
  `path`-Parameter überleben.
- `oversized_browser_prompt_is_rejected_before_the_browser_run` - genau
  `MAX_BROWSER_PROMPT_CHARS` Zeichen sind erlaubt, ein Zeichen mehr wird mit
  benanntem Fehler abgelehnt (keine fest eingeschriebene Zahl mehr).

## Live-Kalibrierung mit dem ersten T-1017-Build (`03dbe14be`)

Die erste Fassung (32 KiB Schema, 180k Guard) wurde gebaut, mit Backup
deployt (PID 5688, identische Command Line, Health ok, 9 Modelle) und
verifiziert:

- **PASS mit 12 Tools:** `POST /v1/chat/completions`, `model=chatgpt`,
  `stream=false`, 12 Tools mit je ~6.400 Zeichen Beschreibung → HTTP 200,
  Antwort exakt `T1017-LIVE-OK`, 29,2 s. Event 23:44:52 `+01:00`:
  `success=true`, `sends=1`, `prompt_chars=27432`,
  `pasted_chars=27447`, `focus_method=keyboard`.
- **Guard-PASS:** Prompt mit 190.000 Zeichen + 12 Tools → 502
  `invalid_request_error` mit `Prompt zu gross fuer die Browser-Oberflaeche:
  192439 Zeichen (Maximum 180000); Werkzeugbeschreibungen sind bereits
  komprimiert` in 0,13 s. Kein Browserlauf, kein Breaker-Eintrag.
- **Grenze zu hoch:** 159.505 Zeichen → `ABSENDEKNOPF_DEAKTIVIERT` nach
  94 s; 147.459 Zeichen → dasselbe nach 90 s. Damit ist die alte
  Code-Notiz „200.000 Zeichen funktionieren" falsch.
- **Untere Beobachtung:** 127.459 und 138.561 Zeichen wurden beide
  tatsächlich gesendet (`sends=1`, kein `ABSENDEKNOPF_DEAKTIVIERT`), aber
  die synthetischen Füllprompts lieferten innerhalb von 200 s / 260 s keine
  Antwort. Das beweist die Akzeptanz der Eingabe, **keinen** Antwort-PASS.
- **Folge:** Der Guard wurde auf 140.000 gesenkt. Mit
  `TOOL_DESCRIPTION_LIMITS = [256, 0]` blieb das 12-Tool-Schema bei ~9 KiB
  (Event `prompt_chars=9073`): der externe ~132k-Prompt hätte die 140k-Grenze
  erneut überschritten. Deshalb werden Beschreibungen jetzt sofort vollständig
  entfernt (`[0]`); das Schema liegt bei ~2,4 KiB. Der nachfolgende Build
  muss dies mit einem großen Turn erneut belegen.

## Erwartung und Grenze

- Unterstützt und live belegt: kompakter Prompt bis 120k mit 12 Tools →
  Browser sendet, Antwort kommt (Beleg bei 102.806 Zeichen).
- Oberhalb 120k gab es auch ohne Tools bei ~130k keine Antwort (353 s,
  `timeout_no_text`). Der Turn scheitert jetzt in Bruchteilen einer Sekunde
  mit benanntem Fehler, statt die Brain-Sperre minutenlang zu belegen.
- Ein Kürzen des Nutzerprompt-Textes ist nicht Teil dieses Tasks; deshalb
  bleibt ein externer Prompt über 120k ein bewusster, schneller Fehler.
- Kein Prompt-Text wird gekürzt, zusammengefasst oder umformuliert.
- Kein Tag, GitHub-Release, Versionssprung oder neues öffentliches Artefakt.
- Keine Zugangsdaten gelesen, angezeigt oder weitergegeben.

## Finaler Build, Deploy und Live-Abnahme (2026-11-27)

- Commit `e326001` (`master`, gepusht). Release-Build
  `webagent 0.11.8 (e326001d3)`, Exit 0; EXE-SHA-256
  `59B94B061B686999E8A392D0FF4B589876D60D88AF1B42F055905EC73AC2046B`,
  DLL unverändert `465A7DDF…A0A6F`, Budget-Fehlertext im Binary vorhanden.
- Rollback-Ordner
  `...\webagent-deploy-target\backup-release-before-T1017-descriptions-20261127\`
  (EXE `27544821…02E1`, DLL, SHA-Datei, Breaker-State). Alte PID `16644`
  gestoppt, Port frei. Neue PID `14960`, **identische** Command-Line
  (`--skip-startup-reconcile api serve --brain chatgpt --headless --port 8788
  --timeout-secs 120`), Health `ok`, `/v1/models` 200 mit Header / 401 ohne.
  Logs: `bridge.err|out.restart-T1017-final-20261127.log`.
- **Kleiner Turn:** 12 Tools mit je ~6.400 Zeichen Beschreibung, Prompt
  `Antworte exakt mit T1017-DESC0-OK.` → HTTP 200, Antwort exakt
  `T1017-DESC0-OK`, 22,6 s. Event 00:15:48 `+01:00`: `success=true`,
  `sends=1`, `prompt_chars=2917` (statt 9073 vor `[0]`).
- **Großer Turn:** 12 Tools, 99.935 Zeichen synthetischer Kontext plus
  Schlussanweisung → HTTP 200, Antwort exakt `T1017-MEDIUM-OK`, 39,7 s.
  Event `success=true`, `sends=1`, `prompt_chars=102806`. Damit ist die
  Browser-Inference mit großem Prompt und 12 Werkzeugen live abgenommen.
- **Grenzerprobe:** 134.851 Zeichen wurden gesendet (`sends=1`, kein
  `ABSENDEKNOPF_DEAKTIVIERT`); der synthetische Füllprompt lieferte innerhalb
  des 240-s-Client-Timeouts keine Antwort (`client_disconnected`).
- **Echter 132k-Turn (Realtext aus `docs/`, nicht Fülltext):** 132.906 Zeichen,
  `sends=1`, **keine** Längenablehnung, aber nach 351 s
  `timeout_no_text: renderer may be unresponsive after wake – move mouse or
  restart brain, generation_complete=false, raw_chars=0`. Damit ist die
  Längengrenze behoben, aber ein Antwort-PASS bei ~132k ist **nicht** belegt.
- **Brain danach gesund:** Direkt im Anschluss 12 Tools + Kurzprompt →
  HTTP 200, exakt `T1017-POSTBIG-OK`, 20,4 s, `success=true`, `sends=1`,
  `prompt_chars=2415`. Der grosse Turn hat den Brain nicht beschädigt.
- **Guard:** 150.000 Zeichen + 12 Tools → 0,15 s, 502
  `invalid_request_error`, Meldung `Prompt zu gross fuer die
  Browser-Oberflaeche: 152427 Zeichen (Maximum 140000); Werkzeugbeschreibungen
  sind bereits komprimiert`. Kein Browserlauf, kein Breaker-Eintrag.
- Breaker `chatgpt` nach der Abnahme ohne Eintrag. Kein Tag, GitHub-Release,
  Versionssprung oder Login; `WEBAGENT_API_KEY` nur als Header verwendet,
  nie angezeigt oder gespeichert.
- **Rückweg:** PID 14960 stoppen, EXE/DLL/SHA-Datei aus
  `backup-release-before-T1017-descriptions-20261127\` zurückkopieren,
  Breaker-State bei Bedarf wiederherstellen, mit identischer Command Line
  neu starten. Zwischenstufen: `backup-release-before-T1017-20261126\` und
  `backup-release-before-T1017-calibrated-20261127\`.

## 2026-11-27 - Guard auf die Realtext-Grenze kalibriert

Nach dem Final-Deploy wurde die verbleibende Lücke weiter eingegrenzt: ein
`tools=12`-Turn mit 132.906 Zeichen wurde gesendet, lieferte aber nach 351 s
`timeout_no_text` (`generation_complete=false`, `raw_chars=0`). Zur
Isolierung wurde derselbe Realtext ohne Werkzeuge erneut geschickt
(130.043 Zeichen Request, Event `prompt_chars=130215`): HTTP 502 nach 352,9 s,
`timeout_budget=312s`, `backend_status=timeout_no_message`, `sends=1`,
`raw_chars=0`. Werkzeugdefinitionen sind damit nicht die Ursache.

Zunächst wurde `MAX_BROWSER_PROMPT_CHARS` auf **120.000** gesenkt und als
PID `3768` deployt (EXE `CD51BBE6…52A4B4F`). Live mit dieser Binary:
150.097 Zeichen Request (Event zählt 152.740) → 0,166 s, benannter 502-Fehler
`Maximum 120000`, kein Event, kein Browserlauf. Damit ist die schnelle
Ablehnung belegt.

Weitere Messungen mit derselben Binary zeigten jedoch einen
**Inhalts-Effekt**: synthetischer Fülltext bei 102.743 Zeichen antwortete in
33,3 s (`success=true`), Realtext aus `docs/` gleicher Länge lief 352 s in
`timeout_no_message`. Realtext bei 82.643 Zeichen antwortete in 44,3 s
(`success=true`); bei 87.643 und 92.643 Zeichen kam nach 341 s bzw. 340 s
keine Antwort. Ein Kurzturn dazwischen war jeweils grün, der Renderer war
also nicht global verklemmt.

Deshalb ist der Guard jetzt konservativ auf **83.000** gesetzt: zwischen der
belegten Realtext-Antwort bei 82.643 und der ersten belegten
Realtext-Nicht-Antwort bei 87.643. Er schützt vor dem schlechteren Fall,
ohne den Nutzerprompt zu kürzen, zusammenzufassen oder umzuformulieren.
Turns oberhalb der Grenze scheitern sofort benannt statt fünf bis sechs
Minuten die Brain-Sperre zu belegen.

Gates für die 120k-Zwischenstufe: vierte Serie in `gates.txt` grün. Gates,
Build, Deploy und Live-Abnahme der finalen 83k-Grenze stehen im nächsten
Abschnitt.

## 2026-11-27 - Finaler 83k-Build, Deploy und Live-Abnahme

- Commit `039a00d` (`master`, gepusht). Release-Build
  `webagent 0.11.8 (039a00daf)`, Exit 0; EXE-SHA-256
  `B990DA8B219036DD4C6BD5692E709DC3CC67F15E6969763A64E888F9F1F8FB93`,
  DLL unverändert `465A7DDF…A0A6F`. Der Budget-Hinweis liegt als Formatstring
  im Binary; der numerische Wert wird zur Laufzeit aus der Konstante
  formatiert. Gates: fünfte Serie in `gates.txt` grün.
- Rollback-Ordner
  `...\webagent-deploy-target\backup-release-before-T1017-guard83k-20261127\`
  mit der vorherigen EXE `CD51BBE6…52A4B4F`, DLL und Breaker-State.
  PID `3768` gestoppt, Port frei. Neue PID `1144`, **identische**
  Command-Line (`--skip-startup-reconcile api serve --brain chatgpt
  --headless --port 8788 --timeout-secs 120`), Health `ok`, `/v1/models`
  200 mit Header / 401 ohne. Logs:
  `bridge.err|out.restart-T1017-guard83k-20261127.log`.
- **Guard-PASS:** Realtext-Request mit 85.000 Zeichen, 12 Tools → 0,121 s,
  HTTP 502 `invalid_request_error`, Meldung
  `Prompt zu gross fuer die Browser-Oberflaeche: 87643 Zeichen (Maximum 83000);
  Werkzeugbeschreibungen sind bereits komprimiert`. `events_delta=0`, kein
  Browserlauf, kein Breaker-Eintrag.
- **Antwort-PASS:** Realtext aus `docs/` mit 80.000 Zeichen, 12 Tools →
  HTTP 200, exakt `T1017-GUARD83-REAL80-OK`, 48,3 s, `finish_reason=stop`.
  Event `success=true`, `sends=1`, `prompt_chars=82643`.
- **Ehrliche Grenze:** Belegt ist ein sicherer Bereich bis 83.000 Zeichen
  finaler Promptlänge für den getesteten Realtext. Darüber scheitert der
  Turn sofort benannt; ein 100%-PASS für beliebige größere Prompts gibt es
  nicht. Synthetischer Fülltext antwortete zwar bei 102.743 Zeichen, wurde
  aber nicht als Grundlage der Grenze genommen. Kein Prompttext gekürzt,
  kein Login, kein Tag/Release/Versionssprung; `WEBAGENT_API_KEY` nur als
  Header verwendet und nie angezeigt.
- **Lokales Staging für den zweiten PC:**
  `...\webagent-deploy-target\T1017-guard83k-20261127\` mit EXE, DLL und
  README; Hashes geprüft. Kein automatisches Deployment auf dem zweiten PC.
- **Rückweg:** PID `1144` stoppen, EXE/DLL aus dem
  `backup-release-before-T1017-guard83k-20261127\` zurückkopieren,
  Breaker-State bei Bedarf wiederherstellen und mit identischer Command
  Line neu starten. Ältere Rollback-Stufen bleiben erhalten.
