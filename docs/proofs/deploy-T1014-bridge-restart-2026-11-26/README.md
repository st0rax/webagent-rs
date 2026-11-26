# Bridge restart with the T-1014 binary — 2026-11-26

> **Referenz.** Deploy proof for the owner-approved local bridge restart. It records which binary is active and which checks passed; it is not a GitHub release or a new version publication.

## Freigabe und Quellstand

- Der Eigentuemer schrieb „start die bridge bitte nue", nachdem ausdruecklich gesagt worden war, dass EXE-Tausch und Neustart damals noch Freigabe brauchen. Damit ist der Neustart freigegeben.
- Build-Quelle: lokaler Stand `84ba99fb5` (T-1014-Gemini-Fix plus `play.rs`-Platzhalter aus GitHub-`master`). `cargo build --release --locked` Exit 0; `WebView2Loader.dll` vorhanden; `--version` → `webagent 0.11.8 (84ba99fb5)`, Exit 0.

## Vorher / Sicherung

- Alter Prozess: PID `15908`, gestartet 17:46:46 UTC, command line `"...\webagent-deploy-target\release\webagent.exe" --skip-startup-reconcile api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`.
- Alte aktive EXE: SHA-256 `A58D319482E8CB652A692F854D2DBAD8514E20FB80D5F8669AD406C30F09719F`, `--version` → `fb7b88607` (ausgeliefertes `v0.11.8`). DLL SHA-256 `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`.
- Sicherung vor dem Tausch: `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\backup-release-T1014-20261126\` mit `webagent.exe.v0.11.8-published`, `WebView2Loader.dll`, `webagent-windows-x86_64.exe.sha256` und `circuit-state-before.json`. Backup-EXE-Hash nach dem Kopieren geprueft: `A58D3194…`.
- `taskkill /PID 15908` war erfolgreich; PID danach nicht mehr alive; Port 8788 hatte keinen `Listen`-Eintrag mehr.

## Aktivierung und Proof

- Neue aktive EXE: SHA-256 `6CC16E34E9B2E58E45E81E9BDB80F08EB0A916FB2BC9E32F4395CA58E390B72B`, `--version` → `webagent 0.11.8 (84ba99fb5)`, Exit 0.
- Neue DLL: SHA-256 `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F` (unveraendert gegenueber der ausgelieferten DLL).
- Neuer Prozess: PID `4484`, CreationDate `2026-11-26 19:56:30`, gleiche Command Line wie zuvor, `127.0.0.1:8788` im Zustand `Listen`, Prozess alive.
- Health: `GET http://127.0.0.1:8788/health` → `{"service":"webagent-provider-bridge","status":"ok"}`.
- Katalog: `GET /v1/models` mit dem vorhandenen Env-Header → 9 Modelle; `webagent/gemini` meldet `context_window=12000`, `max_tokens=4000`. Damit ist die T-1014-Katalogaenderung live belegt.
- Breaker: der noch offene `chatgpt`-Eintrag wurde aus `data/circuit_breaker/state.json` entfernt; die Sicherung liegt im Backup-Ordner. Alle uebrigen Eintraege haben `open_until` in der Vergangenheit. Kein Eintrag wurde gelesen oder veraendert, um Zugangsdaten zu erhalten.
- Logs: neue Prozess-Streams gehen nach `bridge.err.restart-20261126.log` und `bridge.out.restart-20261126.log`; beide tails waren beim Check leer. `events.jsonl` zeigt nach dem Restart keinen neuen Turn; letzter Eintrag bleibt 18:50:51 UTC.

## Grenze / Rueckweg

- Kein Provider-Turn, kein Login, kein Browser-WebView und keine Zugangsdatenausgabe. Der Env-Variablenname `WEBAGENT_API_KEY` wurde nur geprueft; der Wert wurde als Header uebergeben, nicht angezeigt oder gespeichert.
- Kein Tag, GitHub-Release, Versionssprung oder anderer Deploy-Schritt. Rollback: PID `4484` stoppen, die drei Dateien aus dem Backup-Ordner nach `...\release\` zurueckkopieren und bei Bedarf `circuit-state-before.json` wiederherstellen.
