# Bridge-Restart mit der T-1016-EXE — 2026-11-26

> **Referenz.** Deploy-Beleg für den lokalen EXE-Tausch auf T-1016. Kein
> GitHub-Release, kein Tag, kein Versionssprung, keine Zugangsdatenausgabe.

## Freigabe

- Der Eigentuemer schrieb zuerst „autonom auf fertigstellung hinarbeiten"
  und dann nach der Zwischenmeldung, die EXE liege nur im Staging-Ordner:
  „und ich warte immer noch auf ne neue binarsy". Ich habe dies als konkrete
  Anweisung verstanden, die neue Binary **in den aktiven Betrieb** zu bringen,
  und den Tausch genau wie bei T-1015 mit Backup und identischer Command Line
  ausgeführt. Die urspruengliche T-1016-Non-Goal-Notiz „kein lokaler Restart"
  ist damit durch diese spaetere Eigentuemer-Anweisung ersetzt.
- Quellstand: `master` `6ccec0e34` (Merge von `c5c3901`, T-1016). Der
  anschliessende Docs-Commit `672f1d8` aendert keinen Code.

## Vorher / Sicherung

- Alter Prozess: PID `2564`, Command Line
  `"C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\webagent.exe"
  --skip-startup-reconcile api serve --brain chatgpt --headless --port 8788
  --timeout-secs 120`. Health vor dem Tausch `ok`.
- Alte aktive EXE: SHA-256
  `E35A709265019B66E8D1E18C6A2FDA7389A67C5DBB99B2BA8CAD4B40795780C8`
  (`webagent 0.11.8 (573719d11)`, T-1015). DLL SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`.
- Neuer Rollback-Ordner:
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\backup-release-before-T1016-20261126\`
  mit `webagent.exe` (Hash nach dem Kopieren geprueft), `WebView2Loader.dll`,
  `webagent-windows-x86_64.exe.sha256` und `circuit-state-before.json`.
  Die Ordner `backup-release-before-T1015-20261126` und
  `backup-release-T1014-20261126` blieben unangetastet.
- PID `2564` gestoppt, Prozess nicht mehr alive, Port 8788 ohne
  `Listen`-Eintrag.

## Aktivierung

- Neue aktive EXE: SHA-256
  `D96F4999693AE24F69DEE20A010483A55863B1CA8E813E32956E8781E573EEAB`,
  `--version` → `webagent 0.11.8 (6ccec0e34)`, Exit 0.
- Neue DLL: SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`
  (unveraendert). Die vorhandene `.sha256`-Datei wurde wie bei T-1015
  **nicht** ueberschrieben: sie beschreibt das publizierte `v0.11.8`-Asset.
- Neuer Prozess: PID `16972`, CreationDate `2026-11-26 22:49:24` (lokal),
  identische Command Line, Port `127.0.0.1:8788` in `Listen`, Health
  `{"service":"webagent-provider-bridge","status":"ok"}`. Der Supervisor
  meldet durchgehend `OK Bridge gesund, keine Aenderung`.
- Neue Logs: `bridge.err.restart-T1016-20261126.log` und
  `bridge.out.restart-T1016-20261126.log`.
- `GET /v1/models` mit dem vorhandenen Header: HTTP 200, neun Modelle.
  Ohne Header: HTTP 401 (unveraendete Zugriffsschutz-Schwelle).

## Live-Grenze (ehrlich)

- Ein kontrollierter `POST /v1/chat/completions`-Turn auf
  `webagent/chatgpt` mit `stream=false`, `tools=0` wurde gestartet und
  nach 150 s durch den Client abgebrochen. Fuer **diesen** Turn wurde kein
  Event in `brain_score/events.jsonl` geschrieben. Ich melde deshalb
  **keinen** Live-PASS.
- Stattdessen schrieb eine parallele Anfrage (`tools=12`,
  `prompt_chars=196027`) um 21:53:36 UTC ein Event mit
  `success=false`, `ABSENDEKNOPF_DEAKTIVIERT` nach 243584 ms
  (`focus_stations=0`, `pasted_chars=199804`). Das ist ein
  UI-Sendefehler bei sehr grossem Prompt, **kein** Selektoren-Fehler; es
  gibt keinen `[selectors] … nicht lesbar`-Beleg. Diese Fehlerklasse ist
  nicht Teil von T-1016 und bleibt offen.
- Die API serialisiert Browserruns pro Brain; der kurze Turn wartete
  hoechstwahrscheinlich hinter dieser langen Anfrage. Das ist nicht
  bewiesen; die Live-Abnahme des Backend-Pfads bleibt offen.
- Betriebszustand danach: `circuit_breaker/state.json` zeigte `chatgpt` mit
  `consecutive_failures=3`, `open_until=1795731207` (900 s Sperre) und
  genau dieser `ABSENDEKNOPF_DEAKTIVIERT`-Ursache. Der identische Fehler
  stand bereits vor dem Tausch im Event von 21:46:48 UTC — er ist also
  **kein** T-1016-Rueckfall. Weitere gesperrte Brains im selben File
  (claude, deepseek, kimi, mock) sind alter Zustand und wurden nicht
  angefasst.
- Auf die ausdrueckliche Anweisung „MACH JETZT" wurde der `chatgpt`-Eintrag
  mit Backup unter
  `data/circuit_breaker/state-before-T1016-live-20261126.json` entfernt
  (funktional identisch zu `circuit_breaker::clear`). Health blieb `ok`.
- **Ursache der Sendefehler (Code-Befund):** `send.rs:1291-1296` dokumentiert
  eine Laengenablehnung der Provider-Oberflaeche: bei ~400.000 Zeichen
  steht der Text vollstaendig im Composer, der Absendeknopf ist grau und
  es erscheint kein Dialog; bei 200.000 Zeichen funktioniert es. Die
  fehlgeschlagenen Events tragen `prompt_chars=196027` /
  `pasted_chars=199804` — genau diese Grenzklasse. `relay.rs` stuft
  `ABSENDEKNOPF_DEAKTIVIERT` bewusst als deterministisch ein
  (`kein Retry`). Eine Umgehung wuerde Kürzen/Zusammenfassen des Prompts
  und damit eine Aenderung der API-Semantik bedeuten; das ist ohne
  ausdrueckliche Freigabe nicht implementiert.
- Belegt ist: Prozessstart, Health, Models, Hashes, identische Command Line.
  Nicht belegt ist: ein erfolgreicher Provider-Turn auf der neuen EXE.
- Der Browser-Prozess `webagent · chatgpt (1)` ist nach dem Restart
  vorhanden; WebView2-Kinder wurden neu gestartet.

## Rückweg / Grenze

- Rollback: PID `16972` stoppen, `webagent.exe`, `WebView2Loader.dll` und
  SHA-Datei aus `backup-release-before-T1016-20261126\` nach `...\release\`
  zurueckkopieren, bei Bedarf `circuit-state-before.json` wiederherstellen
  und mit identischer Command Line neu starten.
- Kein Login, keine Browser-Anmeldung, keine Zugangsdatenausgabe. Der Wert
  von `WEBAGENT_API_KEY` wurde ausschliesslich als Authorization-Header
  verwendet und nicht angezeigt oder gespeichert (nur Presence geprueft).
- Kein Tag, kein GitHub-Release, kein Versionssprung und kein neues
  oeffentliches Artefakt.
