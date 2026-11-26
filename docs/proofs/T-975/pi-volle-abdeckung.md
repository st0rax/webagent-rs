**Referenz.** Real gemessene Abdeckungspruefung der lokalen Pi-Installation am 2026-11-26. Kein Login, keine Zugangsdaten, keine Profil-/Token-Dateien gelesen. Der Token `WEBAGENT_API_KEY` blieb als Benutzer-Umgebungsvariable gesetzt; sein Wert ist in keiner Datei und keiner Ausgabe enthalten.

## 1. Alle Brains in der Nutzer-Pi-Installation registriert

Die dynamische Extension `examples/pi/extensions/webagent-models.ts` wurde nach `%USERPROFILE%\.pi\agent\extensions\webagent-models.ts` installiert. Sie liest den Katalog vom lokalen Endpoint und registriert daraus alle Brains. `pi --list-models webagent` (exit 0) liefert:

| Provider | Modell | Kontext | max-out | Bilder |
|---|---|---:|---:|---|
| webagent | webagent/auto | 128K | 16.4K | ja |
| webagent | webagent/chatgpt | 128K | 16.4K | ja |
| webagent | webagent/claude | 128K | 16.4K | ja |
| webagent | webagent/deepseek | 128K | 16.4K | ja |
| webagent | webagent/gemini | 128K | 16.4K | ja |
| webagent | webagent/kimi | 128K | 16.4K | ja |
| webagent | webagent/mistral | 128K | 16.4K | ja |
| webagent | webagent/qwen | 128K | 16.4K | nein |
| webagent | webagent/zai | 128K | 16.4K | nein |

## 2. Neun echte Pi-Turns, einer pro registriertem Modell

Jeweils `pi --offline --provider webagent --model webagent/<brain> --no-tools --no-session --no-context-files --no-skills --no-prompt-templates --mode json -p "Antworte exakt mit WEBAGENT-<BRAIN>-OK-20261126."` gegen die dauerhaft laufende Bridge. Marker exakt im JSON-Modus geprueft, Tokenredaktion aktiv.

| Brain | exit | Marker exakt gefunden |
|---|---:|---|
| auto | 0 | ja |
| chatgpt | 0 | ja |
| claude | 0 | ja |
| deepseek | 0 | ja |
| gemini | 0 | ja |
| kimi | 0 | ja |
| mistral | 0 | ja |
| qwen | 0 | ja |
| zai | 0 | ja |

## 3. Sichtbarer Anmelde-Dialog ueber die echte Autostart-Verknuepfung

Die Verknuepfung `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk` wurde real gestartet (`Start-Process -FilePath <lnk>`, PID 11932). Das resultierende Fenster war sichtbar:

- Titel `WebAgent Bridge Start`, Prozess `WindowsTerminal`, `hwnd=23791118`
- `visible=True`, `rect=122,130,1251,765` (via `GetWindowRect`/`IsWindowVisible` gemessen)

**Antwort `n{ENTER}`:** Fenster aktiviert (`AppActivate=True`), danach `optin=False`, `port=False`. `prompt.log`: `2026-11-26T03:30:21 Antwort=aus`. Nach 15 Sekunden war das Fenster automatisch geschlossen (`window_after_autoclose=False`).

**Antwort `j{ENTER}`:** Verknuepfung erneut gestartet, Fenster aktiviert, danach `optin=True`, `port=True`. `prompt.log`: `2026-11-26T03:30:53 Antwort=start`. `supervisor.log`: `03:30:57 Bridge gestartet PID=7000`, `03:30:57 OK Bridge gesund PID=7000`.

## Grenze

Der Anmelde-Dialog wurde ueber die echte Verknuepfung und das echte Skript ausgeloest und mit sichtbarem HWND/Rect sowie den Antwortpfaden geprueft. Ein echter Reboot/Re-Login wurde nicht durchgefuehrt; dafuer ist eine gesonderte ausdrueckliche Freigabe noetig. Kein Last-, Dauerlast- oder Parallelitätstest, keine neue Browser-Anmeldung, kein Fernzugriff und keine Aenderung an der Bridge-Runtime.
