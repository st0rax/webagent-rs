# Provider-Chat-Turn — 26.11.2026

> **Referenz.** Target-Betrieb "Provider für Pi/OpenCode": echter Turn durch
> die Loopback-Brücke des deployten `v0.11.7`-Binary.

- **Zeit:** 2026-11-26T00:14Z (01:14 +01:00).
- **Befehl:** `webagent.exe --skip-startup-reconcile api serve --brain chatgpt --headless --port 8788` mit prozessonly generiertem `WEBAGENT_API_KEY`; anschließend `POST /v1/chat/completions` mit `model=webagent/chatgpt`, `messages=[{role=user, content="Antworte exakt mit PROVIDER-OK-20261126."}]`, `stream=false`.
- **Ergebnis:** HTTP 200, `id=chatcmpl-18eb7029b20cca24`, `model=webagent/chatgpt`, `finish_reason=stop`, `content=PROVIDER-OK-20261126`.
- **Grenze:** Ein Turn, ein Brain, bestehende Sitzung; kein Dauerbetrieb. Der Token existierte nur im Prozess und wurde danach entfernt; der Prozess wurde gestoppt, Port 8788 war danach frei. Keine Zugangsdaten gelesen oder gespeichert.
