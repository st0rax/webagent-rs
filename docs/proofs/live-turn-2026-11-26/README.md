# Live-Turn-Abnahme — 26.11.2026

> **Referenz.** Beleg für den Zielbetrieb auf diesem Windows-Rechner: Browser-Chat als
> Gehirn, bestehende WebView-OAuth-Sitzung, lokale Provider-Rolle.

## Finaler Turn (deploytes Binary `v0.11.7`)

- **Zeit:** 2026-11-26T00:11Z (01:11 +01:00).
- **Binary:** `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\webagent.exe`, `--version` → `webagent 0.11.7 (d54d34755)`, SHA-256 `0f45d96377857220ab7d25eda44bfe3f6e8886e6bb01136136d5d2dcfd973e84` (identisch mit dem Release-Asset-Digest).
- **Befehl:** `webagent.exe --skip-startup-reconcile cloud web-view-stream --brain chatgpt --prompt "Antworte exakt mit LIVE-OK-20261126-V0.11.7." --net-trace` — ohne `--headless`, d. h. mit echtem sichtbarem WebView2-Fenster.
- **Ergebnis:** Exit 0. Ereignisse `started` → `token` → `completed`; Token-Text exakt `LIVE-OK-20261126-V0.11.7`. `oauth_session=true`, `api_key=false`.
- **Netzbeleg:** nur lesende CDP-Sensoren (`Network.enable`), keine Interception; URLs auf `scheme://host` redaktiert. Verdict wählt eine 200-SSE-Antwort (`text/event-stream`, POST, Origin der Dokumentseite) als `provider_response`, Dauer 602 ms. Das Sensor-Feld `finished=false` betrifft die SSE-Klassifikation; der Adapter meldet unabhängig `completed`.
- **Rohdatei:** `webview-oauth-live-turn.json`, SHA-256 `a912be43305e7e197cecea46e1d03552778e33556d1f50f915d4baf94e494cc0`. Secret-Scan auf `bearer`, `cookie`, `authorization`, `secret`: 0 Treffer; einziger `api_key`-Treffer ist das Feld `api_key=false`.

## Vorausgegangener Preflight (deploytes Binary `v0.11.6`)

- gleicher Befehl mit `--headless`, Antwort exakt `LIVE-OK-20261126`, Exit 0, `provider_response` 200-SSE (591 ms). Damit war vor dem Release belegt, dass die vorhandene Sitzung greift.

## Provider-Rolle am deployten `v0.11.7`

- `api serve --brain chatgpt --headless --port 8788` mit prozessonly generiertem Token: `GET /health` → 200, `GET /v1/models` mit Bearer → 9 Modelle (erste `webagent/auto`), ohne Bearer → 401. Prozess gestoppt, Port 8788 danach frei.
- Echter Provider-Turn per `POST /v1/chat/completions`: `PROVIDER-OK-20261126`, `finish_reason=stop`; Details in `provider-chat-turn.md`.

## Grenzen

- Ein Brain (chatgpt), ein Turn, eine Anmeldung, die bereits vorlag. Der Agent hat keine Zugangsdaten gelesen, keine Anmeldung durchgeführt und keine Profil-/Token-Dateien geöffnet.
- Kein Dauerbetrieb, keine Last-/Parallelitätstests, keine Monitoring-Integration. Der Live-Turn ist ein Einzelfallbeleg, kein Zuverlässigkeitsnachweis.
- Die Bridge-Semantik von T-961 (Tool-Retry) ist hiervon unabhängig und bleibt in den dortigen Belegen dokumentiert.
