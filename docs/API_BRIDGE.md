> **Referenz.** Implementierter lokaler API-Vertrag; fuer den aktuellen Codezustand massgeblich.

# Lokale Provider-Bridge für Pi

## Zweck und Betriebsgrenze

`webagent api serve` stellt WebAgent **ausschließlich auf dem lokalen Rechner** als Provider für Pi bereit. Der Dienst bindet nur an IPv4- oder IPv6-Loopback-Adressen, verlangt für alle Provider-Endpunkte einen API-Token und führt pro Anfrage genau einen frischen WebAgent-Controller-Lauf aus. Anfragen werden bewusst serialisiert, damit ein Browserprofil nicht konkurrierend von mehreren Agentenläufen gesteuert wird.

> **Sicherheitsinvariante:** Eine Adresse wie `0.0.0.0` wird vor dem Listener-Start abgelehnt. Der Token wird nur über eine Umgebungsvariable gelesen und nie in CLI-Argumenten, Logs oder Konfigurationsdateien gespeichert.

Die Bridge akzeptiert die für Pi relevanten Formate **OpenAI Chat Completions** und **Anthropic Messages**. Pi unterstützt beide API-Typen für benutzerdefinierte Provider; für OpenAI-kompatible Endpunkte empfiehlt die Pi-Dokumentation `openai-completions` als den breitesten Kompatibilitätsweg.[1] [2]

| Eigenschaft | Implementierter Vertrag |
|---|---|
| Bindung | `127.0.0.1:8787` als Standard; ausschließlich Loopback zugelassen |
| Authentifizierung | `Authorization: Bearer <token>`; Anthropic-kompatibel zusätzlich `x-api-key: <token>` |
| Brain | `chatgpt` als Standard, über `--brain` austauschbar |
| Nebenläufigkeit | Eine Anfrage zur Zeit pro Dienstprozess |
| Ergebnisquelle | Nur strukturierte `answer-*`, `finish-*`, `final-*`, `review-*` oder `eval-*`-Aktionen; **kein Transcript-Fallback** |
| Streaming | SSE-Abschlussformat nach vollständig beendetem Controller-Run, nicht tokeninkrementell |
| Unterstützter Inhalt | Textnachrichten und Textblöcke; andere Inhaltsarten werden mit HTTP 400 abgelehnt |

## Dienst starten

Erzeuge zunächst einen zufälligen lokalen Token. Ein Token darf nicht in das Repository geschrieben werden.

```powershell
$env:WEBAGENT_API_KEY = [guid]::NewGuid().ToString("N")
webagent api serve --brain chatgpt --headless
```

Der Prozess protokolliert dann ausschließlich seine lokale URL. Für einen anderen lokalen Port kann `--port` verwendet werden. Ein externes Binding wird absichtlich verweigert.

```powershell
webagent api serve --bind 0.0.0.0
# [api] Sicherheitsgrenze: --bind muss eine Loopback-Adresse sein.
```

Ein schneller lokaler Gesundheitscheck benötigt keinen Token. Der Modellkatalog dagegen ist absichtlich token-geschützt.

```powershell
Invoke-RestMethod http://127.0.0.1:8787/health
Invoke-RestMethod -Headers @{ Authorization = "Bearer $env:WEBAGENT_API_KEY" } `
  http://127.0.0.1:8787/v1/models
```

## Endpunkte

| Methode und Pfad | Format | Verhalten |
|---|---|---|
| `GET /health` | JSON | Lokaler Liveness-Check ohne Agentenlauf |
| `GET /v1/models` | OpenAI-Modellliste | Liefert `webagent/<brain>` nach erfolgreicher Tokenprüfung |
| `POST /v1/chat/completions` | OpenAI Chat Completions | Akzeptiert Textrollen `system`, `developer`, `user` und `assistant` |
| `POST /v1/messages` | Anthropic Messages | Akzeptiert `max_tokens`, top-level `system` sowie Textrollen `user` und `assistant` |

OpenAI Chat Completions modelliert eine Unterhaltung als Nachrichtenliste und kann eine reguläre Completion oder gestreamte Chunks liefern.[3] Die Bridge implementiert genau diesen textbasierten Teil. Anthropic Messages erwartet `messages` und `max_tokens`; Systeminstruktionen liegen dort auf Top-Level statt in einer `system`-Rolle.[4]

Nicht unterstützte Bild-, Audio-, Dokument-, Thinking- und Tool-Blöcke werden nie stillschweigend entfernt. Sie führen zu einer providerkonformen `400 invalid_request_error`-Antwort. Das verhindert, dass ein Client irrtümlich annimmt, nicht verarbeitete Daten seien beim Agenten angekommen.

## Pi-Konfiguration

### OpenAI Chat Completions

Lege in `%USERPROFILE%\.pi\agent\models.json` eine benutzerdefinierte OpenAI-kompatible Providerdefinition an. Pi kann Provider über `models.json` konfigurieren und löst Umgebungsvariablen in `apiKey` auf.[1] [2]

```json
{
  "providers": {
    "webagent": {
      "baseUrl": "http://127.0.0.1:8787/v1",
      "api": "openai-completions",
      "apiKey": "$WEBAGENT_API_KEY",
      "authHeader": true,
      "compat": {
        "supportsDeveloperRole": true,
        "supportsReasoningEffort": false,
        "supportsUsageInStreaming": false
      },
      "models": [
        {
          "id": "webagent/chatgpt",
          "name": "WebAgent ChatGPT",
          "reasoning": false,
          "input": ["text"],
          "contextWindow": 128000,
          "maxTokens": 16384,
          "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
        }
      ]
    }
  }
}
```

Der Modellname muss dem beim gestarteten Dienst ausgewählten Brain entsprechen. Beispielsweise liefert `--brain deepseek` das Modell `webagent/deepseek`.

### Anthropic Messages

Für den Anthropic-Adapter wird derselbe Token verwendet. Die `baseUrl` endet **nicht** auf `/v1`, weil Pi für `anthropic-messages` den Messages-Pfad selbst ergänzt.

```json
{
  "providers": {
    "webagent-anthropic": {
      "baseUrl": "http://127.0.0.1:8787",
      "api": "anthropic-messages",
      "apiKey": "$WEBAGENT_API_KEY",
      "authHeader": true,
      "models": [
        {
          "id": "webagent/chatgpt",
          "name": "WebAgent ChatGPT (Anthropic format)",
          "reasoning": false,
          "input": ["text"],
          "contextWindow": 128000,
          "maxTokens": 16384,
          "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
        }
      ]
    }
  }
}
```

## Bewusste erste Scheibe

Die Bridge ist ein **lokaler Adapter**, keine öffentliche API-Plattform. Sie besitzt weder TLS-Termination noch Benutzerverwaltung, Request-Pooling oder automatische Tokenrotation. Ein Dienst darf deshalb nicht über Portweiterleitung, Reverse Proxy oder Cloud-Tunnel freigegeben werden, ohne einen separaten Sicherheitsentwurf.

Der Service serialisiert Browserruns und rendert SSE erst nach Abschluss des Agentenlaufs. Damit erfüllt die Response-Semantik die Providerformate, aber nicht tokenweises Echtzeitstreaming. Tool-Calls, multimodale Eingaben und OpenAI Responses werden derzeit absichtlich abgelehnt beziehungsweise nicht angeboten. Diese Begrenzung hält die erste Pi-Integration überprüfbar und verhindert semantische Datenverluste.

## References

[1] [Pi: Custom Providers](https://pi.dev/docs/latest/custom-provider)

[2] [Pi: Custom Models](https://pi.dev/docs/latest/models)

[3] [OpenAI: Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create/)

[4] [Anthropic: Messages API](https://platform.claude.com/docs/en/api/messages)
