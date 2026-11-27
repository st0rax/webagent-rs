# T-1021 — Bridge-WebView sichtbar schalten

**Referenzstatus:** laufende Scheibe; der operative Stand steht in `docs/CURRENT_WORK.md`.

**Datum:** 2026-11-27
**Branch:** `feature/T-1021-visible-webview`
**Claim:** `cd0888c` (gepusht vor der Source-Änderung)

## Auftrag

Der Eigentümer will den tatsächlich an das Brain gesendeten Prompt aus der
Provider-Oberfläche kopieren. Die Bridge auf `127.0.0.1:8788` lief mit
`--headless`; die Provider-WebViews lagen dadurch auf `-32000/-32000`.

## Umsetzung

`scripts/supervise-bridge.ps1` startet die Bridge bewusst **ohne `--headless`**.
Die drei T-1019-Flags (`WEBAGENT_USE_SHARED_BROWSER`, `WEBAGENT_PERSIST_TABS`,
`WEBAGENT_CONTINUE_PROVIDER_CHAT`), Loopback-Bindung, Authentifizierung und der
Umgang mit `bridge.pid` sind unverändert. Ein Supervisor-Neustart übernimmt die
Änderung automatisch; bestehende Bridge-Prozesse werden nicht angefasst.

Der aktive Prozess wurde über den Supervisor neu gestartet:

- `bridge.pid`: `18580`
- Command Line: `"…\webagent-deploy-target\release\webagent.exe" --skip-startup-reconcile api serve --brain chatgpt --port 8788 --timeout-secs 120`
- `supervisor.log`: `06:40:14 Bridge gestartet PID=18580`, `06:40:15 OK Bridge gesund PID=18580`

## HWND/Rect-Beleg (Windows PowerShell, EnumWindows + GetWindowRect)

Minimaler Live-Turn gegen `POST /v1/chat/completions`, Modell `webagent/chatgpt`:
`success=true`, Antwort 2 Zeichen. Der Token wurde ausschließlich als
`Authorization`-Header verwendet und nicht ausgegeben. Es wurde kein Prompt-
Rohtext persistiert.

WebView-Fenster `webagent · chatgpt (1)`, **HWND `1442666`**, PID `18580`:

| Zeit | Rect | Zustand |
|---|---|---|
| 06:41:57–06:42:00 | `-32000,-32000,-30704,-31061` | beim Start geparkt |
| 06:42:01–06:42:18 | `320,66,1616,1005` | sichtbar im Arbeitsbereich |
| ab 06:42:19 | — | Fenster geschlossen |

## Befund und Grenze

Ohne `--headless` ist das Provider-Fenster **während eines Turns** sichtbar;
der Prompt kann in diesem Zeitraum aus dem Composer kopiert werden. **Dauerhaft
sichtbar ist es nicht:** nach dem Turn ruft `BrowserPool::stop_brain` bei
`refs == 0` `complete_required_teardown` auf, das über
`shutdown_shared_runtime_with_result` → `close_shared_tabs` den Shared-Runtime
und damit alle WebView-Fenster schließt. Das gilt auch mit
`WEBAGENT_PERSIST_TABS=1`: der Tab bleibt im Profil erhalten, das Fenster aber
nicht.

## Finale Umsetzung (nach ausdrücklicher Freigabe)

Der Eigentümer hat am 27.11.2026 die Binary-Änderung und den EXE-Tausch
ausdrücklich freigegeben. Umgesetzt:

- `WEBAGENT_KEEP_WINDOWS_VISIBLE` als Opt-in in `config::brains` (Default aus).
- `webview_reveal::park_offscreen` lässt das Fenster bei gesetztem Flag stehen
  (kein `set_bounds(None)`, kein Fokusentzug).
- `browser_pool::stop_brain` überspringt bei gesetztem Flag den
  `complete_required_teardown`; der Shared-Runtime und damit alle WebView-
  Fenster bleiben über Turns hinweg offen. Der geordnete Write-back läuft
  beim Prozessende über den Pool-Drop.
- Der Supervisor setzt das Flag zusätzlich zu den drei T-1019-Flags.

Der Deploy der neuen EXE und der abschließende HWND/Rect-Nachweis werden in
diesem Beleg ergänzt, sobald die produktive Bridge neu gestartet ist.

Kein Login, keine Profil-/Token-Lektüre, kein Prompt-Rohtext im Log. Kein Tag
und kein GitHub-Release in dieser Scheibe.
