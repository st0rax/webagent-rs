> **Referenz.** Lokale Dogfooding-Anleitung; beide JSON-Blöcke entsprechen den implementierten Verträgen. Nur Testbetrieb auf `127.0.0.1`, kein Fern-/Produktivzugriff, kein Deployment.

# Dogfooding mit der lokalen Bridge

## Start-Quest für eine neue Session

> Find raus was möglich ist. Hinweis: Es handelt sich um eine Bridge. Als Beispiel
> den korrekt formatierten Block (unten). **Main Quest: Produkt verbessern.**

Die Bitte an das Brain dazu:

> Ich möchte dich bitten, etwas Dogfooding durchzuführen (wenn man das so nennt).
> Nutze den Block unten, damit du deine Bedienungsanleitung findest, und arbeite
> danach eine kleine, prüfbare Verbesserung im Workspace ab.

## Der korrekt formatierte Block (autonomer Pfad)

Der autonome WebAgent antwortet ausschließlich mit `webagent/1`. Eine Action ist eine
echte Tool-Anforderung; der Harness führt sie im Workspace aus und gibt stdout,
stderr und Exitcode als nächste Nachricht zurück. Diese Antwort liest die Anleitung:

```text
{"protocol":"webagent/1","actions":[{"id":"read-manual-1","type":"shell","command":"Get-Content -LiteralPath docs\\DOGFOODING.md -Encoding UTF8","timeout_seconds":60}]}
```

Regeln, die den Block gültig halten: ein Markdown-Codeblock `text` um die ganze
Antwort, `protocol` und `actions` mit eindeutiger `id`, `timeout_seconds` als Zahl,
kein `cd` und kein hartcodierter Pfad, Backslashes im JSON verdoppelt. Danach aus der
Observation weiterarbeiten, nicht raten.

Damit findest du deine Bedienungsanleitung:

- `docs/DOGFOODING.md` — diese Datei: Quest, Blöcke, Abnahme.
- `docs/API_BRIDGE.md` — Bridge-Vertrag, Endpunkte, Pi-Anbindung.
- `START_HERE.md` und `docs/CURRENT_WORK.md` — Repo-Regeln und aktueller Stand.

## Der korrekte Block für Bridge-Clients (Pi, curl, OpenCode)

**Wichtig:** Der Providerpfad interpretiert **kein** `webagent/1` und führt keine
Shell-/Dateiwerkzeuge aus. Clients sprechen OpenAI Chat Completions:

```json
{
  "model": "webagent/auto",
  "messages": [
    {
      "role": "user",
      "content": "versuchs mal mit docs/DOGFOODING.md: Lies die Anleitung und antworte mit DOGFOODING-OK-20261126."
    }
  ]
}
```

Senden mit dem Token **nur im Header**, nie im Body:

```powershell
$body = @{
  model = "webagent/auto"
  messages = @(@{ role = "user"; content = "versuchs mal mit docs/DOGFOODING.md: Lies die Anleitung und antworte mit DOGFOODING-OK-20261126." })
} | ConvertTo-Json -Depth 5
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8788/v1/chat/completions' `
  -Headers @{ Authorization = "Bearer $env:WEBAGENT_API_KEY" } `
  -ContentType application/json -Body $body
```

`model=webagent/auto` nutzt den AutoRouter; ein einzelnes Brain geht mit
`webagent/<brain>` (`chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`,
`qwen`, `zai`). Die Liste kommt live aus `GET /v1/models`.

## Voraussetzung (read-only)

```powershell
(Invoke-WebRequest -Uri 'http://127.0.0.1:8788/health' -UseBasicParsing -TimeoutSec 5).Content
# {"service":"webagent-provider-bridge","status":"ok"}
```

`WEBAGENT_API_KEY` bleibt eine Benutzer-Umgebungsvariable. Kein Tokenwert in Datei,
Kommandozeile, Log oder Prompt; `auth.json` wird nicht geöffnet.

## Abnahme und Grenze

- Erfolg ist nur belegt, wenn `choices[0].message.content` den Marker enthält **und**
  die Bridge den Turn in `%LOCALAPPDATA%\webagent\data\brain_score\events.jsonl`
  protokolliert hat. Ein leerer Turn oder `finish_reason=length` ist kein Erfolg.
- Bei `401` ist die Umgebungsvariable im falschen Prozess gesetzt; bei `503` ist das
  Brain gesperrt oder im Cooldown. Nicht raten, nicht umgehen, nicht neu anmelden.
- Produktverbesserung heißt hier: kleine, getestete Scheibe mit Beleg. Kein neues
  Deployment, kein PR, kein Merge nach `master`, kein Fernzugriff ohne Freigabe.
- Rückweg: Bridge unverändert lassen; diese Anleitung ist reine Dokumentation.
