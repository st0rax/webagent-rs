# T-1015 — vorbereiteter lokaler Deploy/Restart (ausgeführt)

> **Referenz (historisch).** Dokumentiert den vorbereiteten Ablauf; dieser
> Abschnitt ist kein aktueller Live-Status.

> **Status:** Plan und Ablauf wurden nach ausdrücklicher Freigabe ausgeführt.
> Der tatsächliche Beleg mit Hashes, PIDs und Live-Turns steht unter
> `docs/proofs/deploy-T1015-bridge-restart-2026-11-26/README.md`.
> Dieser Abschnitt bleibt als nachvollziehbare Vorbereitung erhalten und
> beschreibt nicht mehr den aktuellen Live-Zustand.

## Stand vor dem Schritt (geprüft 2026-11-26)

- `origin/master` = `41d0315`; T-1015-Änderung in `573719d`, Merge `7b61afc`.
- Aktive Bridge: PID `4484`, CreationDate `2026-11-26 19:56:30`, Command Line
  `"...\webagent-deploy-target\release\webagent.exe" --skip-startup-reconcile
  api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`.
- `127.0.0.1:8788` hört auf PID `4484`; `GET /health` → `status=ok`.
- Aktive EXE: `...\webagent-deploy-target\release\webagent.exe`,
  SHA-256 `6CC16E34E9B2E58E45E81E9BDB80F08EB0A916FB2BC9E32F4395CA58E390B72B`
  (`webagent 0.11.8 (84ba99fb5)`). DLL SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`.
- Bestehender Rollback-Ordner: `...\webagent-deploy-target\backup-release-T1014-20261126\`
  mit der damaligen EXE (`A58D3194…`), DLL, SHA-Datei und
  `circuit-state-before.json`. Er bleibt unangetastet.

## Freigabe — historischer Zustand vor dem Schritt

Ein EXE-Tausch und Neustart der laufenden Bridge hat Außenwirkung auf die
aktive API und die Browser-Sitzungen. Er darf erst nach einer **konkreten**
Freigabe durch den Eigentuemer erfolgen (z. B. „start die bridge bitte nue mit
T-1015"). Ein allgemeines „mach weiter" genuegt dafuer nicht. Die konkrete
Freigabe wurde danach mit „Ja, deployen + restarten" erteilt.

## Ablauf nach Freigabe

1. Release-Binary aus dem gemergten Stand bauen:
   `cargo build --release --locked`; `--version` und `WebView2Loader.dll`
   pruefen; Hash des neuen `target\release\webagent.exe` notieren.
2. Rolle und Zustand sichern: neuer Ordner
   `...\webagent-deploy-target\backup-release-before-T1015-20261126\` mit
   `webagent.exe` (Hash `6CC16E34…`), `WebView2Loader.dll`,
   `webagent-windows-x86_64.exe.sha256` und
   `data\circuit_breaker\state.json`. Hashes nach dem Kopieren verifizieren.
3. PID `4484` stoppen, Port 8788 ohne `Listen` verifizieren.
4. Drei Dateien aus `target\release\` nach `...\release\` kopieren; EXE/DLL
   Hashes und `--version` der aktiven Datei pruefen.
5. Bridge mit **identischer** Command Line neu starten, neue Log-Dateien
   verwenden (z. B. `bridge.err.restart-T1015-20261126.log`), keine
   Zugangsdaten anzeigen oder speichern. Der Header `WEBAGENT_API_KEY` darf
   nur als Header verwendet werden.
6. Verifikation: `/health` → `ok`; `GET /v1/models` → neun Modelle; ein
   kontrollierter Chatgpt-Turn mit aktiven Tools, der einen Begruessungs-Stub
   `chatgpt bereit.` liefert und danach den Frisch-Chat-Retry auslöst;
   Logzeile `[T-1015] Begruessungs-Stub erkannt`; `events.jsonl` auf Erfolg
   und `sends` prüfen. Zusätzlich ein Umschlag mit `\n``` ` Trailer und
   gültigem Tool-Call. Kein Login und keine Zugangsdatenausgabe.
7. Beleg als neues Dokument `docs/proofs/deploy-T1015-bridge-restart-.../`
   schreiben; `docs/CURRENT_WORK.md` aktualisieren. Kein Tag, GitHub-Release
   oder Versionssprung.

## Rückweg

PID der T-1015-Bridge stoppen, die drei gesicherten Dateien aus
`backup-release-before-T1015-20261126\` nach `...\release\` zurueckkopieren,
optional `state.json` wiederherstellen und die Bridge mit derselben Command
Line neu starten. Der bestehende T-1014-Rollback-Ordner bleibt untouched.
