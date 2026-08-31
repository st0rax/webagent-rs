> **Referenz.** Implementierter lokaler API-Vertrag; fuer den aktuellen Codezustand massgeblich.

# Lokale Provider-Bridge für Pi

## Zweck und Betriebsgrenze

`webagent api serve` stellt WebAgent **ausschließlich auf dem lokalen Rechner** als Browser-Inference-Provider bereit. Der Dienst bindet nur an IPv4- oder IPv6-Loopback-Adressen, verlangt für alle Provider-Endpunkte einen API-Token und führt pro Anfrage genau einen frischen Browser-Modellturn aus. Anfragen desselben Brains werden bewusst serialisiert, damit ein Browserprofil nicht konkurrierend von mehreren Inference-Aufrufen gesteuert wird; unterschiedliche Brains können parallel arbeiten. Der Providerpfad startet keinen `AgentController`, interpretiert kein `webagent/1` und führt keine lokalen Shell- oder Dateiwerkzeuge aus.

> **Sicherheitsinvariante:** Eine Adresse wie `0.0.0.0` wird vor dem Listener-Start abgelehnt. Der Token wird nur über eine Umgebungsvariable gelesen und nie in CLI-Argumenten, Logs oder Konfigurationsdateien gespeichert.

Die Bridge akzeptiert die für Pi relevanten Formate **OpenAI Chat Completions** und **Anthropic Messages**. Pi unterstützt beide API-Typen für benutzerdefinierte Provider; für OpenAI-kompatible Endpunkte empfiehlt die Pi-Dokumentation `openai-completions` als den breitesten Kompatibilitätsweg.[1] [2]

| Eigenschaft | Implementierter Vertrag |
|---|---|
| Bindung | `127.0.0.1:8787` als Standard; ausschließlich Loopback zugelassen |
| Authentifizierung | `Authorization: Bearer <token>`; Anthropic-kompatibel zusätzlich `x-api-key: <token>` |
| Brain | `chatgpt` als Standard; `model=webagent/<brain>` routet jede Anfrage auf das gewaehlte eingebaute oder Custom-Brain |
| Nebenläufigkeit | Eine Anfrage zur Zeit pro Dienstprozess |
| Ergebnisquelle | Direkt beobachteter Antworttext des einzelnen Browser-Modellturns |
| Streaming | SSE; reine Textturns und multimodale Browserturns liefern wachsende Text-Snapshots, Tool-Streams bleiben bis zur Validierung gepuffert |
| Unterstützter Inhalt | Text sowie base64-kodierte Bilder und Audiodateien; OpenAI-Function-Tools experimentell; externe Medien-URLs und nicht unterstützte Inhaltsarten werden mit HTTP 400 abgelehnt |

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
| `GET /health` | JSON | Lokaler Liveness-Check ohne Browserturn |
| `GET /v1/models` | OpenAI-Modellliste | Liefert automatisch alle aktuell konfigurierten eingebauten und Custom-Brains als `webagent/<brain>` |
| `GET /v1/models/{id}` | OpenAI-Modellobjekt | Liefert das einzelne konfigurierte Brain; unbekannte IDs werden mit 404 abgelehnt |
| `POST /v1/chat/completions` | OpenAI Chat Completions | Akzeptiert Textrollen, OpenAI-`image_url`-data-URLs, `input_audio`-Base64, Function-Tools, Assistant-`tool_calls` und `role=tool`-Ergebnisse; Streams werden inkrementell übertragen |
| `POST /v1/responses` | OpenAI Responses | Akzeptiert String-/Message-Input, `input_image`-data-URLs, `input_audio`-Base64, Responses-Function-Tools, `function_call`/`function_call_output`, `store` und `previous_response_id`; liefert Response-Objekt sowie gepufferten Responses-SSE-Eventstrom |
| `GET /v1/responses/{id}` | OpenAI Response-Retrieval | Liefert eine gespeicherte Response; unbekannte oder mit `store=false` erzeugte IDs werden mit 404 abgelehnt |
| `GET /v1/responses/{id}/input_items` | OpenAI Input-Item-Liste | Liefert den normalisierten, im lokalen State gespeicherten Verlauf der Response |
| `DELETE /v1/responses/{id}` | OpenAI Response-Lifecycle | Entfernt eine gespeicherte Response aus dem lokalen In-Memory-Store; Folgezugriffe liefern 404 |
| `POST /v1/messages` | Anthropic Messages | Akzeptiert `max_tokens`, top-level `system`, Textrollen `user`/`assistant` sowie Anthropic-Base64-Bild-/Audio-Blöcke |

OpenAI Chat Completions modelliert eine Unterhaltung als Nachrichtenliste und kann eine reguläre Completion oder gestreamte Chunks liefern.[3] Die Bridge übernimmt dabei Text sowie die beschriebenen Bild-/Audio-Content-Parts. Anthropic Messages erwartet `messages` und `max_tokens`; Systeminstruktionen liegen dort auf Top-Level statt in einer `system`-Rolle.[4]

Message-Inhalte dürfen als String oder Content-Array übergeben werden. Textteile werden in den Browser-Prompt übernommen. Bilder werden als `data:image/...;base64,...` (`image_url` bzw. `input_image`) und Audio als OpenAI-`input_audio` mit Base64-Daten plus Format oder als Anthropic-`source: {type: "base64", media_type, data}` akzeptiert. Die dekodierten Bytes werden vor dem Senden in ein vorhandenes `input[type=file]` des Browser-Composers eingesetzt; im Prompt bleibt zusätzlich ein transparenter Attachment-Marker. Externe `http(s)`-URLs, `file_id`-Referenzen sowie Dokument-, Thinking- und unbekannte Blöcke werden nicht automatisch geladen oder stillschweigend entfernt, sondern führen zu einer providerkonformen `400 invalid_request_error`-Antwort.

### Multimodale Requests

Die Bridge lädt Medien nicht aus dem Internet. Der Client liest die lokale Datei, kodiert sie als Base64 und sendet sie im jeweiligen Provider-Content-Block. Ein OpenAI-Chat-Request sieht beispielsweise so aus:

~~~powershell
$image = [Convert]::ToBase64String([IO.File]::ReadAllBytes((Resolve-Path .\foto.png)))
$audio = [Convert]::ToBase64String([IO.File]::ReadAllBytes((Resolve-Path .\notiz.wav)))
$body = @{
  model = "webagent/chatgpt"
  messages = @(@{ role = "user"; content = @(
    @{ type = "text"; text = "Was ist auf dem Bild und was wird gesagt?" }
    @{ type = "image_url"; image_url = @{ url = "data:image/png;base64,$image" } }
    @{ type = "input_audio"; input_audio = @{ data = $audio; format = "wav" } }
  ) })
} | ConvertTo-Json -Depth 8
Invoke-RestMethod -Method Post -Uri http://127.0.0.1:8787/v1/chat/completions -Headers @{ Authorization = "Bearer $env:WEBAGENT_API_KEY" } -ContentType application/json -Body $body
~~~

Die Responses-Variante verwendet type input_image mit derselben Data-URL und type input_audio; Anthropic verwendet type image bzw. type audio mit source.type base64. Pro Datei gilt ein dekodiertes Maximum von 8 MiB, pro Request höchstens 16 Dateien. Die Weboberfläche muss ein input[type=file] besitzen; wenn ein Brain diese Upload-Funktion nicht anbietet, antwortet der Endpoint mit einem erklärenden 502 statt die Datei zu verwerfen.

Ein solcher `no_file_input`-Fehler gilt nur für den jeweiligen multimodalen
Request. Er öffnet nicht den allgemeinen Text-Circuit-Breaker und verschlechtert
nicht den Brain-Score; ein späterer text-only Request an dasselbe Brain bleibt
damit möglich. Bei einem Client, der eine Bildnachricht im vollständigen Verlauf
mitführt, muss für diese Brains entweder ein neuer text-only Verlauf begonnen
oder ein Brain mit Datei-Upload verwendet werden.

### Ausgabegrenzen und Providerverhalten

Die aktuelle Bridge extrahiert den beobachteten Browserturn als **Text**. Deshalb
melden `/v1/models` und die Responses-Ausgabe derzeit nur `output: ["text"]`;
Bild- und Audio-Inputs werden unterstützt, generierte Bildartefakte aber noch
nicht als `image`-Output aus der Weboberfläche übernommen. Eine ChatGPT-Webantwort
mit Bildgenerierung kann daher höchstens als Textstatus erscheinen, nicht als
downloadbares Bild im API-Response. Das ist eine bewusst ehrliche
Fähigkeitsangabe und keine fehlende Client-Konfiguration.

Providerseitige Ablehnungen bleiben unverändert erhalten. Insbesondere Claude
kann eine Anfrage aus Sicherheitsgründen ablehnen; die Bridge versucht nicht,
solche Regeln zu umgehen. Die Browser-Einleitung verwendet dafür nur noch einen
neutralen Gesprächsrahmen und erwähnt weder Bridge-Identität noch internes
Transportprotokoll. Das reduziert Meta-Diskussionen über die Übertragung, ändert
aber nichts an der Sicherheitsentscheidung des jeweiligen Providers.

## Pi-Konfiguration

Die aktuelle Pi-Version wird laut Pi-Dokumentation unter Windows so installiert (Node.js **22.19 oder neuer**):

```powershell
npm install -g --ignore-scripts @earendil-works/pi-coding-agent
pi --version
```

Zum Ausprobieren ohne Eingriff in eine vorhandene Pi-Konfiguration liegt unter `examples/pi/models.json` eine isoliert nutzbare Ein-Modell-Vorlage. `scripts/test-pi-provider.ps1` setzt `PI_CODING_AGENT_DIR` nur fuer seinen eigenen Prozess auf dieses Verzeichnis, prueft einen echten Textturn und danach einen echten `read`-Tool-Loop mit einer zufaelligen temporaeren Datei.

Fuer den normalen Multi-Brain-Betrieb ist `examples/pi/extensions/webagent-models.ts` massgeblich. Die Pi-Erweiterung fragt `/v1/models` beim Start ab, registriert alle gelieferten Brains dynamisch und aktualisiert sie bei `/models` erneut. Damit erscheinen auch spaeter hinzugefuegte Custom-Brains ohne manuelle Pflege einer Pi-Modellliste.

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
        "supportsStore": false,
        "supportsDeveloperRole": true,
        "supportsReasoningEffort": false,
        "supportsUsageInStreaming": false,
        "supportsFinishReason": true,
        "supportsStrictMode": false,
        "maxTokensField": "max_tokens"
      },
      "models": [
        {
          "id": "webagent/chatgpt",
          "name": "WebAgent ChatGPT",
          "reasoning": false,
          "input": ["text", "image", "audio"],
          "contextWindow": 128000,
          "maxTokens": 16384,
          "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0 }
        }
      ]
    }
  }
}
```

`--brain` bestimmt nur noch das Standard-Brain fuer den Alias `model=webagent`. Eine explizite Modell-ID routet den einzelnen Request: `webagent/deepseek` verwendet DeepSeek, `webagent/claude` Claude usw. Unbekannte Modell-IDs scheitern vor dem Browserstart.

### Automatischer Pi-Modellkatalog und `/models`

Installiere die Erweiterung in Pis globales Erweiterungsverzeichnis und entferne eine eventuell alte statische `webagent`-Modellliste aus `models.json`, da diese den dynamischen Katalog sonst wieder ueberschreiben kann:

```powershell
New-Item -ItemType Directory "$env:USERPROFILE\.pi\agent\extensions" -Force | Out-Null
Copy-Item examples\pi\extensions\webagent-models.ts `
  "$env:USERPROFILE\.pi\agent\extensions\webagent-models.ts"
```

Danach stehen in Pi beide Wege zur Verfuegung:

```text
/models             # Katalog aktualisieren und Brain-Auswahl oeffnen
/models claude      # direkt auf Claude wechseln
/models gemini      # direkt auf Gemini wechseln
/model              # Pis eingebaute allgemeine Modellauswahl funktioniert ebenfalls
```

Ausserhalb des TUI zeigt `pi --list-models webagent` denselben automatisch geladenen Katalog. Ist der Endpoint beim Pi-Start noch nicht erreichbar, registriert die Erweiterung zunaechst die acht Standard-Brains; der naechste `/models`-Aufruf aktualisiert sie vom laufenden Endpoint und nimmt dabei auch Custom-Brains auf.

### Reproduzierbarer lokaler Smoke-Test

Terminal 1 startet die Bridge. Der Token bleibt lokal und muss in beiden Terminals identisch sein:

```powershell
$env:WEBAGENT_API_KEY = [guid]::NewGuid().ToString("N")
pwsh -File scripts/build-release.ps1
target\release\webagent.exe api serve --brain chatgpt --headless --timeout-secs 120
```

Terminal 2 bekommt denselben Token und die lokale Pi-Installation. Bei einer Installation ausserhalb von `PATH` kann `-PiCommand` auf `pi.cmd` oder `pi.exe` zeigen.

```powershell
$env:WEBAGENT_API_KEY = "<derselbe Token aus Terminal 1>"
pwsh -File scripts/test-pi-provider.ps1 -PiCommand pi
```

Erfolg endet mit `PASS: Textturn und echter Pi-read-Tool-Loop sind gruen.` Der Test fuehrt nur Pis eingebautes `read`-Tool in einem neu erzeugten Temp-Verzeichnis aus. Er schreibt weder in das Repository noch nach `%USERPROFILE%\.pi` und entfernt sein Temp-Verzeichnis wieder.

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
          "input": ["text", "image", "audio"],
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

Der Service serialisiert Browserturns pro Brain. Chat-Completions- und Responses-Streams übertragen wachsende DOM-Snapshots inkrementell und senden bei längeren Denkpausen SSE-Keepalives; Function-Tool-Streams bleiben bis zur validierten Tool-Antwort gepuffert. Damit erfüllen beide OpenAI-Pfade ihre Abschlussformate, inklusive Responses-Function-Tools, Output-Item-Events und lokalem Conversation-State. Multimodale Eingaben werden vor dem Browserturn validiert, dekodiert und als Datei-Upload angehängt; nicht übertragbare Medien werden fail-closed abgelehnt. Diese Begrenzung hält die harnessfreie Inference-Scheibe überprüfbar und verhindert semantische Datenverluste.

Responses werden standardmäßig in einem auf 256 Einträge und 64 MiB serialisierte Nachrichten begrenzten In-Memory-Store abgelegt und können über `GET /v1/responses/{id}` abgerufen werden.[5] `previous_response_id` lädt den normalisierten Nachrichtenverlauf der referenzierten Response und stellt ihn dem nächsten Browserturn voran; neue `instructions` gelten nur für den neuen Turn.[6] `store=false` verhindert sowohl Retrieval als auch eine spätere Verknüpfung. Der Store ist absichtlich pro Prozess und nicht dauerhaft: Nach einem Neustart sind die IDs nicht mehr verfügbar.

Der Wire-Vertrag ist zusätzlich mit dem offiziellen OpenAI-Python-SDK 3.6.0 live geprüft: Modellliste, Response-Retrieval, inkrementeller Responses-Textstream, Responses-Function-Call-Stream und inkrementeller Chat-Completions-Textstream werden vom SDK ohne Sonderadapter geparst; `get_final_response()` bzw. die Chat-Chunk-Faltung liefern jeweils das vollständige Ergebnis. Die SSE-Reihenfolge enthält dafür die kanonischen `output_item`-, `content_part`-, Delta-, Done- und Completion-Ereignisse.

`--timeout-secs` setzt optional das Zeitlimit für den einzelnen Browserturn. Ohne Angabe verwendet WebAgent die bestehende dynamische Timeout-Auflösung des ausgewählten Brains.

## OpenAI-Tool-Calling

Der OpenAI-Adapter normalisiert Function-Tools und die Varianten `tool_choice=auto`, `none`, `required` sowie eine erzwungene Function. Für einen Tool-Aufruf fordert die Browser-Inference-Schicht vom Web-LLM einen strikten `WEBAGENT_INFERENCE/1`-Umschlag an und wandelt diesen anschließend in reguläre OpenAI-`tool_calls` um. Tool-Ergebnisse können im nächsten Request als `role=tool` mit `tool_call_id` zurückgegeben werden.

Viele Coding-Clients senden gleichzeitig mehrere Dutzend MCP-Werkzeuge mit sehr
langen Beschreibungen. Vor dem Browserturn wird der serialisierte Toolblock
deshalb stufenweise auf höchstens 64 KiB kompaktisiert: Werkzeugnamen und
Parameterschemata bleiben erhalten, nur Beschreibungstexte werden gekürzt. Das
verhindert den generischen Web-UI-Fehler „Something went wrong“, den ChatGPT
bei einem ungekürzten ZCode-Request mit 53 Werkzeugen reproduzierbar zeigte.
Falls selbst die beschreibungsfreie Schemaform zu groß ist, wird der Request
fail-closed mit einem erklärenden Providerfehler beendet.

Diese Schicht **führt das Tool nicht aus**. Die Ausführung gehört dem aufrufenden Harness, beispielsweise Deep Agents. Unbekannte Toolnamen, doppelte Call-IDs, ein falsches erzwungenes Tool oder reine Textausgabe bei `tool_choice=required` werden fail-closed als Providerfehler behandelt.

Die JSON- und API-Semantik ist lokal getestet. Fuer ChatGPT sind ein Webturn, ein Pi-0.84.4-Textturn und der vollstaendige Pi-`read`-Tool-Loop live belegt: Tooldefinition zum Browser, `tool_calls` zurueck zu Pi, lokale Ausfuehrung, `role=tool` zum Browser und finale Antwort mit einem zufaelligen Dateiinhalt. Der mitgelieferte Smoke-Test macht diese weiterhin provider- und modellabhaengige Formatstabilitaet reproduzierbar pruefbar. Ein anderes Brain oder Webmodell gilt erst nach demselben Tool-Smoke als belegt. Der Anthropic-Adapter nutzt denselben Browser-Uploadpfad fuer Base64-Bild-/Audio-Blöcke.

Ein Deep-Agents-Client kann nach erfolgreicher Live-Gegenprobe einen `ChatOpenAI`-Adapter mit `base_url=http://127.0.0.1:8787/v1` verwenden. Der dort konfigurierte API-Key ist lediglich der lokale `WEBAGENT_API_KEY`; er ersetzt keinen Login der Browser-Sitzung.

HTTP-Verbindungen werden bis zu einer festen Grenze von **acht** gleichzeitig bearbeitet. Uebersteigt die Zahl aktiver Verbindungen diese Grenze, antwortet die Bridge sofort mit **503 Service Unavailable** und dem Fehlercode overloaded; Clients sollen die Anfrage nach kurzer Wartezeit erneut stellen. Die Grenze schuetzt den lokalen Dienst gegen blockierte oder unvollstaendige HTTP-Anfragen, ohne parallele Browserruns zuzulassen.

## References

[1] [Pi: Custom Providers](https://pi.dev/docs/latest/custom-provider)

[2] [Pi: Custom Models](https://pi.dev/docs/latest/models)

[3] [OpenAI: Create chat completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create/)

[4] [Anthropic: Messages API](https://platform.claude.com/docs/en/api/messages)

[5] [OpenAI: Retrieve a response](https://developers.openai.com/api/reference/cli/resources/responses/methods/retrieve)

[6] [OpenAI: Create a response](https://developers.openai.com/api/reference/cli/resources/responses/methods/create)
