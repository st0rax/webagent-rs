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
- Neu: `MAX_BROWSER_PROMPT_CHARS = 140_000`. Der Wert wurde live zwischen
  138.561 (gesendet) und 147.459 (abgelehnt) Zeichen eingegrenzt; der erste
  Entwurf mit 180.000 war zu hoch. Mit dem kleinen Schema landet der externe
  ~132k-Prompt bei ~134k und damit unter der Grenze.

`crates/webagent-api/src/browser_inference.rs`:

- Neue `check_browser_prompt_budget(prompt)` direkt nach
  `prompt_with_tools` + `prompt_with_identity` in
  `complete_streaming_with_attachments`. Damit ist der Guard vor dem
  Browserlauf wirksam; `complete` und `api_bridge::inference` nutzen
  denselben Pfad.
- Fehlertext ist benannt: `Prompt zu gross fuer die Browser-Oberflaeche:
  <n> Zeichen (Maximum 140000); Werkzeugbeschreibungen sind bereits
  komprimiert`. Ein solcher Turn scheitert sofort statt nach ~90 Sekunden
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

- Erwartung für die externen `tools=12`-Turns: kompakter Prompt < 140k,
  Browser sendet ihn, Antwort wird belegt. Wenn der externe Prompttext
  allein bereits über 140k liegt, bleibt nur der schnelle, benannte Fehler —
  ein Kürzen des Nutzertextes ist nicht Teil dieses Tasks.
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
  des 240-s-Client-Timeouts keine Antwort (`client_disconnected`). Das ist
  ein Antwort-/Testprompt-Problem, keine Längenablehnung mehr.
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
