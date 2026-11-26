# Bridge-Restart mit der T-1015-EXE — 2026-11-26

> **Referenz.** Deploy- und Live-Beleg für den vom Eigentuemer freigegebenen
> lokalen EXE-Tausch. Kein GitHub-Release, kein Tag, kein Versionssprung.

## Freigabe und Quellstand

- Der Eigentuemer schrieb „autonom auf fertigstellung hinarbeiten". Danach
  fragte ich konkret nach EXE-Tausch und Neustart; er antwortete
  „Ja, deployen + restarten". Erst danach wurde die laufende Bridge angefasst.
- Quellstand: `origin/master` `8c56abd` mit T-1015 aus `573719d` / Merge
  `7b61afc`. `cargo build --release --locked` Exit 0; `--version` →
  `webagent 0.11.8 (573719d11)`, Exit 0; `WebView2Loader.dll` vorhanden.
- Zusätzliche Gates auf dem finalen Baum: Architektur-Guard 5 passed
  (`modules=92, edges=282, cycles=3, upward_edges=26,
  crate_violations=0`), `git diff --check` 0.

## Vorher / Sicherung

- Alter Prozess: PID `4484`, CreationDate `2026-11-26 19:56:30`, Command Line
  `"...\webagent-deploy-target\release\webagent.exe" --skip-startup-reconcile
  api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`.
- Alte aktive EXE: SHA-256
  `6CC16E34E9B2E58E45E81E9BDB80F08EB0A916FB2BC9E32F4395CA58E390B72B`
  (`webagent 0.11.8 (84ba99fb5)`). DLL SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`.
- Neuer Rollback-Ordner:
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\backup-release-before-T1015-20261126\`
  mit `webagent.exe` (Hash `6CC16E34…` nach Kopieren geprüft),
  `WebView2Loader.dll`, `webagent-windows-x86_64.exe.sha256` und
  `circuit-state-before.json`. Der ältere Ordner
  `backup-release-T1014-20261126` blieb unangetastet.
- PID `4484` wurde gestoppt; Prozess danach nicht mehr alive und Port 8788
  ohne `Listen`-Eintrag.

## Aktivierung

- Neue aktive EXE: SHA-256
  `E35A709265019B66E8D1E18C6A2FDA7389A67C5DBB99B2BA8CAD4B40795780C8`,
  `--version` → `webagent 0.11.8 (573719d11)`, Exit 0.
- Neue DLL: SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`
  (unverändert zur vorherigen DLL). Die vorhandene SHA-Datei im
  Release-Ordner wurde **nicht** überschrieben: der frische Build erzeugt
  keine passende `.sha256`-Datei, und die vorhandene beschreibt das
  publizierte `v0.11.8`-Asset.
- Neuer Prozess: PID `2564`, CreationDate `2026-11-26 22:11:01`, identische
  Command Line wie zuvor, `127.0.0.1:8788` im Zustand `Listen`, Health
  `{"service":"webagent-provider-bridge","status":"ok"}`.
- `/v1/models` mit dem bestehenden Header: neun Modelle, `webagent/gemini`
  `context_window=12000`, `max_tokens=4000`. Ohne Header: HTTP 401.
- Neue Logs: `bridge.err.restart-T1015-20261126.log` und
  `bridge.out.restart-T1015-20261126.log`.

## Live-Verifikation (zwei kontrollierte Turns, `webagent/chatgpt`, tools=1)

1. **Begruessungs-Stub:** Prompt verlangte genau `chatgpt bereit.`. HTTP 200
   nach 43.386 ms, `finish_reason=stop`, Antwort genau `chatgpt bereit.`.
   Log:
   `[T-1015] Begruessungs-Stub erkannt; ein Frisch-Chat-Retry mit dem
   ursprünglichen Auftrag.` und
   `[T-1015] Retry erschoepft: auch der zweite Turn ist nur eine Begruessung;
   kein PASS.`
   Das ist das erwartete ehrliche Verhalten: genau ein Retry, kein
   vorgetäuschter Erfolg. Der Client erhält den Stub als Antwort.
2. **Umschlag mit Trailer:** Prompt verlangte einen vollständigen
   `WEBAGENT_INFERENCE/1`-Umschlag für `read_file` und danach eine Zeile mit
   drei Backticks. HTTP 200 nach 15.072 ms, `finish_reason=tool_calls`,
   `read_file` mit `{"path":"docs/API_BRIDGE.md"}`, kein
   `model_output_envelope_invalid`. `events.jsonl` zeigt für beide Turns
   `success=true`, `sends=1`.

**Grenze der Live-Aussage:** Der Rohtext des zweiten Turns wird nicht
persistiert; dass das Modell den Trailer tatsächlich sendete, ist nicht direkt
belegt. Die Trailer-Klasse ist daher zusätzlich durch den Regressionstest
`trailing_markdown_and_prose_after_envelope_are_ignored` abgedeckt. Der
Stub-Retry ist dagegen direkt über die beiden `[T-1015]`-Logzeilen belegt.

## Rückweg / Grenze

- Rollback: PID `2564` stoppen, `webagent.exe`, `WebView2Loader.dll` und
  SHA-Datei aus `backup-release-before-T1015-20261126\` nach
  `...\release\` zurückkopieren, bei Bedarf `circuit-state-before.json`
  wiederherstellen und mit identischer Command Line neu starten.
- Kein Login, kein Browser-Login, keine Zugangsdatenausgabe. Der Wert von
  `WEBAGENT_API_KEY` wurde ausschließlich als Authorization-Header verwendet
  und nicht angezeigt oder gespeichert.
- Kein Tag, kein GitHub-Release, kein Versionssprung und kein neues
  öffentliches Artefakt.
