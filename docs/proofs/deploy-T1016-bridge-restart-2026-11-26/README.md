# Bridge-Restart mit der T-1016-EXE — 2026-11-26

> **Referenz.** Deploy-Beleg für den lokalen EXE-Tausch auf T-1016. Kein
> GitHub-Release, kein Tag, kein Versionssprung, keine Zugangsdatenausgabe.
>
> **Stand 2026-11-27:** Die hier beschriebene T-1016-EXE
> (`D96F4999…EEAB`, PID 16972) wurde zwischenzeitlich durch T-1017 ersetzt.
> Aktuell aktiv ist `webagent 0.11.8 (e326001d3)`, SHA-256
> `59B94B061B686999E8A392D0FF4B589876D60D88AF1B42F055905EC73AC2046B`,
> PID 14960. Beleg und Rueckweg:
> `docs/proofs/T-1017/README.md` (Abschnitt „Finaler Build …").

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

## Live-PASS auf der neuen EXE

- `POST /v1/chat/completions` mit `model=webagent/gemini`, `stream=false`,
  `temperature=0`, Prompt `Antworte exakt mit T1016-LIVE-OK.` →
  **HTTP 200**, `finish_reason=stop`, Antwort exakt `T1016-LIVE-OK`,
  20 115 ms. Das zugehoerige Event (22:27:26 UTC) trägt
  `success=true`, `sends=1`, `pasted_chars=202/expected_chars=202`.
- Der erste Anlauf war mit `pasted_chars=167/202` im ContentCheck
  abgelehnt; der vorhandene Relay-Retry (1/2) holte den Turn. Damit ist
  der komplette Pfad auf der aktiven T-1016-EXE belegt: HTTP-Auth,
  Brain-Start, Selektoren, Composer, Senden, Antwort, Score-Event.
- Der Browser-Prozess `webagent · chatgpt (1)` ist nach dem Restart
  vorhanden; WebView2-Kinder wurden neu gestartet.

## Ehrliche chatgpt-Grenze (kein PASS behauptet)

- Ein kontrollierter `chatgpt`-Turn mit 205 Zeichen lief 150 s, ein
  zweiter 420 s und ein dritter wurde nach 113 s als
  `client_disconnected` verbucht (`pasted_chars=1`). Fuer die ersten
  beiden wurde kein eigenes Event geschrieben; die API antwortete 502.
- Zeitgleich und danach schrieb ein **externer Client** (verbundener
  `opencode-cli.exe serve --service` auf Port 8788) wiederholt
  `tools=12`-Turns mit `prompt_chars≈196.028`, die alle an
  `ABSENDEKNOPF_DEAKTIVIERT` scheiterten (21:58, 22:12, 22:20, 22:24 UTC).
  `api_bridge/inference.rs` serialisiert Browserruns **pro Brain**; der
  kurze Turn stand deshalb in der Warteschlange hinter diesen
  ~4-Minuten-Laeufen. Kein Selektoren-Fehler, kein T-1016-Rueckfall.
- Betriebszustand zwischendurch: `chatgpt` mit
  `consecutive_failures=3`, `open_until=1795731207` (900 s). Auf die
  ausdrueckliche Anweisung „MACH JETZT" wurde der Eintrag **zweimal** mit
  Backup entfernt (`state-before-T1016-live-20261126.json`,
  `state-before-T1016-isolated-20261126.json`; funktional wie
  `circuit_breaker::clear`). Health blieb `ok`. Andere Brain-Eintraege
  wurden nicht angefasst.
- **Ursache der 196k-Fehler (Code-Befund):** `send.rs:1291-1296`
  dokumentiert eine Laengenablehnung der Provider-Oberflaeche: bei
  ~400.000 Zeichen steht der Text vollstaendig im Composer, der
  Absendeknopf ist grau, kein Dialog; bei 200.000 Zeichen funktioniert
  es. Die Events tragen `pasted_chars≈199.811` — genau diese
  Grenzklasse. `relay.rs` stuft `ABSENDEKNOPF_DEAKTIVIERT` bewusst als
  deterministisch ein (`kein Retry`). Eine Umgehung wuerde
  Kürzen/Zusammenfassen des Prompts und damit eine Aenderung der
  API-Semantik bedeuten; ohne ausdrueckliche Freigabe nicht implementiert.
- **Zusatzbefund (nicht gefixt):** `composer_holds_text` in
  `composer.rs:384-391` gibt bei Prompts ≤ 1.000 Zeichen ohne echte
  Pruefung `true` zurueck. Damit kann `verify_submitted` fuer kurze
  Prompts `composer_consumed` nie als Beweis nutzen; bei schnellen
  Antworten wird der Stop-Knopf verpasst. Das passt zum
  `pasted_chars=1`-Bild, ist aber nicht Teil von T-1016 und bleibt
  dokumentiert statt still geaendert.

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
