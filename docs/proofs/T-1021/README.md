# T-1021 — Bridge-WebView dauerhaft sichtbar schalten

**Referenzstatus:** abgeschlossene Scheibe; der laufende Stand steht in `docs/CURRENT_WORK.md`.
**Status:** done (Code-Commit `f11f604` auf `master`; Claim `cd0888c`)
**Datum:** 2026-11-27
**Branch:** `feature/T-1021-visible-webview` (per Fast-Forward in `master`)

## Auftrag

Der Eigentümer will den tatsächlich an das Brain gesendeten Prompt aus der
Provider-Oberfläche kopieren. Die Bridge auf `127.0.0.1:8788` lief mit
`--headless`; die Provider-WebViews lagen dadurch auf `-32000/-32000`.

## Umsetzung

`scripts/supervise-bridge.ps1` startet die Bridge bewusst **ohne `--headless`**
und setzt `WEBAGENT_KEEP_WINDOWS_VISIBLE=1` zusätzlich zu den drei
T-1019-Flags (`WEBAGENT_USE_SHARED_BROWSER`, `WEBAGENT_PERSIST_TABS`,
`WEBAGENT_CONTINUE_PROVIDER_CHAT`). Loopback-Bindung, Authentifizierung und
Umgang mit `bridge.pid` sind unverändert.

 Nach ausdrücklicher Freigabe wurde der Binary-Pfad ergänzt:

- `WEBAGENT_KEEP_WINDOWS_VISIBLE` als Opt-in in `config::brains` (Default aus).
- `webview_reveal::park_offscreen` lässt das Fenster bei gesetztem Flag stehen
  (kein `set_bounds(None)`, kein Fokusentzug).
- `browser_pool::stop_brain` überspringt bei gesetztem Flag den
  `complete_required_teardown`; der Shared-Runtime und damit alle WebView-
  Fenster bleiben über Turns hinweg offen. Der geordnete Write-back läuft
  beim Prozessende über den Pool-Drop.

## Deploy

- Quell-Commit: `f11f604`, Version `0.11.9`, Release-Build `cargo build --release --locked` = 0.
- Produktive EXE: SHA-256
  `9C33E843EFD4F49CC188150334234960C59DF7099E472FD76661AA6D34EF0CE7`.
- `WebView2Loader.dll`: SHA-256
  `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`.
- Rollback: `webagent-deploy-target\backup-release-before-T1021-20261127\`
  mit alter EXE `CE6F874FBDAD3A8D1F479847621DBD28645AA93CB5461299750F524DF0118F2D`.
- Supervisor-Log: `2026-11-27T07:03:03 Bridge gestartet PID=18544`,
  `2026-11-27T07:03:03 OK Bridge gesund PID=18544`.
- Aktive Bridge: `bridge.pid=18544`, Command Line
  `"…\webagent-deploy-target\release\webagent.exe" --skip-startup-reconcile api serve --brain chatgpt --port 8788 --timeout-secs 120`
  (**ohne** `--headless`), `GET /health` → `status=ok`.

## Ausgangslage vor dem Binary-Eingriff (historisch)

Ohne `--headless` allein war das Provider-Fenster nur während eines Turns
sichtbar. Minimaler Live-Turn `POST /v1/chat/completions` (`webagent/chatgpt`):
`success=true`, Antwort 2 Zeichen. WebView `webagent · chatgpt (1)`, HWND
`1442666`, PID `18580`:

| Zeit | Rect | Zustand |
|---|---|---|
| 06:41:57–06:42:00 | `-32000,-32000,-30704,-31061` | beim Start geparkt |
| 06:42:01–06:42:18 | `320,66,1616,1005` | sichtbar im Arbeitsbereich |
| ab 06:42:19 | — | Fenster geschlossen |

Ursache: `BrowserPool::stop_brain` rief bei `refs == 0`
`complete_required_teardown` → `shutdown_shared_runtime_with_result` →
`close_shared_tabs` auf. Das galt auch mit `WEBAGENT_PERSIST_TABS=1`: der Tab
blieb im Profil, das Fenster nicht.

## Finaler HWND/Rect-Beleg nach dem Deploy

Live-Turn 07:04:30 gegen `POST /v1/chat/completions`, Modell
`webagent/chatgpt`: HTTP-Erfolg, `success=true` im Event
`brain_score/events.jsonl` (`prompt_chars=1041`, `sends=1`,
`focus_method=keyboard`). Der Token wurde ausschließlich als
`Authorization`-Header verwendet und nicht ausgegeben; kein Prompt-Rohtext
wurde persistiert.

Danach wurde **ohne weiteren Turn** siebenmal im Abstand von 5 Sekunden
derselbe HWND gemessen (Rohdaten in `visibility-after-turn.txt`):

| Zeit | HWND | Visible | Rect |
|---|---|---|---|
| 07:07:23 | 3803212 | True | `320,66,1616,1005` |
| 07:07:28 | 3803212 | True | `320,66,1616,1005` |
| 07:07:33 | 3803212 | True | `320,66,1616,1005` |
| 07:07:38 | 3803212 | True | `320,66,1616,1005` |
| 07:07:43 | 3803212 | True | `320,66,1616,1005` |
| 07:07:48 | 3803212 | True | `320,66,1616,1005` |
| 07:07:53 | 3803212 | True | `320,66,1616,1005` |

Zusätzlich blieb derselbe HWND während eines anschließenden Turns
(07:05:17–07:06:37) durchgehend auf `Rect=320,66,1616,1005`. Dieser zweite
Turn endete in einem HTTP-Client-Timeout; das Fenster verschwand auch danach
nicht. Für diesen Timeout wird **kein** `success=true` behauptet — der
Abnahme-Turn ist der um 07:04:30.

**Ergebnis:** das Provider-WebView bleibt nach dem Turn dauerhaft sichtbar;
der tatsächlich gesendete Prompt kann aus der Oberfläche kopiert werden.

Kein Login, keine Profil-/Token-Lektüre, kein Prompt-Rohtext im Log. Kein Tag
und kein GitHub-Release in dieser Scheibe. Tag und Release `v0.11.9` sind
Gegenstand einer eigenen, freigegebenen Scheibe.
