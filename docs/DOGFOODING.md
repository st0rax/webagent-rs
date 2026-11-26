> **Referenz.** Lokale Bedienungsanleitung für die Bridge; alle Blöcke folgen den implementierten Verträgen. Nur Testbetrieb auf `127.0.0.1`, kein Fern-/Produktivzugriff, kein Deployment.

# Bedienungsanleitung lesen, um die Bridge zu bedienen

## Start-Quest für eine frische Session

> **Quest:** bedienungsanleitung lesen um die bridge zu bedienen
>
> Du hast keinen Verlauf. Lies diese Anleitung mit dem `read_file`-Tool. Hier ist der
> korrekt formatierte Beispielaufruf — kopiere das Format, nicht den Dateinamen als
> Vermutung:

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_1","name":"read_file","arguments":{"path":"docs/DOGFOODING.md"}}]}
```

Danach arbeitest du die Quest ab: verstehe die Bridge, finde das GitHub-Repository
und verbessere das Produkt in einer kleinen, prüfbaren Scheibe. Das ist das exakte
Wire-Format aus `src/browser_inference.rs`: Marker `WEBAGENT_INFERENCE/1` am
Zeilenanfang, danach streng gültiges JSON in **einem** Markdown-Codeblock `text`,
kein Text davor oder danach. Der Tool-Name muss aus der Liste `[Client-Werkzeuge]`
stammen, die die Bridge im Turn anbietet.

## Wo die Anleitung liegt

- `docs/DOGFOODING.md` — diese Datei: Quest, Tool-Beispiele, Abnahme.
- `docs/API_BRIDGE.md` — Bridge-Vertrag, Endpunkte, Pi-Anbindung.
- `START_HERE.md` und `docs/CURRENT_WORK.md` — Repo-Regeln und aktueller Stand.
- **`https://github.com/st0rax/webagent-rs`** — das öffentliche Repository. Der
  aktuelle Stand liegt dort auf `master`; für den lokalen Testbetrieb ist der
  Arbeitsbranch `fix/T-976-api-turn-reveal` maßgeblich.

Mit PowerShell abrufen (öffentlich, ohne Zugangsdaten):

```powershell
git clone --depth 1 https://github.com/st0rax/webagent-rs.git "$env:TMP\webagent-rs"
Get-Content -LiteralPath "$env:TMP\webagent-rs\README.md" -Encoding UTF8
```

Nur lesen, nicht in das laufende Deployment schreiben. Keine Tokens, Cookies oder
ProfilDateien öffnen.

## Weitere Tool-Call-Beispiele

Alle Beispiele nutzen denselben Umschlag. Mehrere **unabhängige** Calls dürfen in
einem `tool_calls`-Array stehen; abhängige Schritte folgen erst der Observation.

Datei lesen:

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_read","name":"read_file","arguments":{"path":"docs/API_BRIDGE.md"}}]}
```

Shell-Befehl (Name und Felder müssen zur angebotenen Tool-Definition passen):

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_shell","name":"bash","arguments":{"command":"git -C $env:TMP\\webagent-rs log -1 --oneline","timeout":60}}]}
```

Datei schreiben und anschließend ändern (nie im selben Umschlag, wenn sie von der
Schreibung abhängt):

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_write","name":"write","arguments":{"path":"docs/notiz.md","content":"Belegzeile\n"}}]}
```

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_edit","name":"edit","arguments":{"path":"docs/notiz.md","old_string":"Belegzeile","new_string":"Geprüfte Zeile"}}]}
```

Zwei unabhängige Lesecalls gebündelt:

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_a","name":"read_file","arguments":{"path":"START_HERE.md"}},{"id":"call_b","name":"read_file","arguments":{"path":"docs/CURRENT_WORK.md"}}]}
```

JSON-Regeln: Backslashes in Strings verdoppeln (`C:\\Users`), Anführungszeichen
escapen, Zeilenumbrüche als `\n` schreiben, Pfade möglichst mit `/` angeben. Bei
einem Parserfehler wird der Rohtext mit Fehlerstelle zurückgemeldet — dann das
Format korrigieren, nicht das Werkzeug wechseln.

### Autonomer Pfad (`webagent/1`, ohne Bridge-Tool)

Der autonome Controller antwortet ausschließlich mit `webagent/1`. Lesen läuft dort
über `shell`, weil es keinen eigenen `read`-ActionType gibt:

```text
{"protocol":"webagent/1","actions":[{"id":"read-manual-1","type":"shell","command":"Get-Content -LiteralPath docs\\DOGFOODING.md -Encoding UTF8","timeout_seconds":60}]}
```

### Bridge-Clients (Pi, curl, OpenCode)

Clients sprechen OpenAI Chat Completions; der Providerpfad interpretiert **kein**
`webagent/1` und führt keine lokalen Werkzeuge aus:

```json
{
  "model": "webagent/auto",
  "messages": [
    {
      "role": "user",
      "content": "bedienungsanleitung lesen um die bridge zu bedienen — docs/DOGFOODING.md, antworte mit DOGFOODING-OK-20261126."
    }
  ]
}
```

Senden mit dem Token **nur im Header**, nie im Body:

```powershell
$body = @{
  model = "webagent/auto"
  messages = @(@{ role = "user"; content = "bedienungsanleitung lesen um die bridge zu bedienen — docs/DOGFOODING.md, antworte mit DOGFOODING-OK-20261126." })
} | ConvertTo-Json -Depth 5
Invoke-RestMethod -Method Post -Uri 'http://127.0.0.1:8788/v1/chat/completions' `
  -Headers @{ Authorization = "Bearer $env:WEBAGENT_API_KEY" } `
  -ContentType application/json -Body $body
```

`model=webagent/auto` nutzt den AutoRouter; ein einzelnes Brain geht mit
`webagent/<brain>` (`chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`,
`qwen`, `zai`). Die Liste kommt live aus `GET /v1/models`.

## Quest-Log (Gamification)

Punkte gibt es **nur mit Beleg**. Eine Behauptung, ein leerer Turn oder ein geratener
Marker zählt nicht. Der Beleg ist immer eine Observation, ein Logeintrag, eine
Belegdatei oder ein Commit.

| Aktion | Beleg | XP |
|---|---|---:|
| Anleitung per `read_file` gelesen | Tool-Ergebnis enthält Text aus dieser Datei | +10 |
| Korrekter `WEBAGENT_INFERENCE/1`-Umschlag | Bridge-Event mit `tool_calls` | +5 |
| Erfolgreicher Bridge-Turn mit Marker | Antworttext **und** `events.jsonl` | +20 |
| Drittes unterschiedliches Tool im Auftrag | drei verschiedene Tool-Calls mit Observation | +15 |
| GitHub-Repo per PowerShell abgerufen | `git clone` + gelesene `README.md` | +15 |
| Belegdatei unter `docs/proofs/` | Datei existiert und ist verlinkt | +25 |
| Alle Gates grün | fmt, clippy, Tests, Node, diff-check | +30 |
| Produktverbesserung committet und gepusht | Commit-SHA auf dem Arbeitsbranch | +40 |
| Live-Turn gegen die laufende Bridge | Event + Marker, kein `timeout_no_message` | +50 |

**Level:** L1 Neuling 0–49 · L2 Leser 50–99 · L3 Brückenbauer 100–179 ·
L4 Werkzeugmeister 180–279 · L5 Dogfooder 280+.

**Abzeichen:** 📖 *Leser* (Anleitung gelesen) · 🌉 *Brückenbauer* (erster guter
Bridge-Turn) · 🧰 *Werkzeugmeister* (drei verschiedene Tools) · 📜 *Belegführer*
(Belegdatei) · ✅ *Harte Scheibe* (alle Gates grün) · 🏅 *Produktverbesserer*
(Commit + Push ohne Deployment) · 🎯 *Dogfooder* (Level 5).

**Hauptquest:** „Bedienungsanleitung lesen um die Bridge zu bedienen" — 100 XP,
abgeschlossen mit Lese-Observation **und** einem erfolgreichen Bridge-Turn.

**Nebenquests:** Repo abrufen (15 XP) · drei Tool-Arten nutzen (15 XP) · kleine
Produktverbesserung mit Beleg (40 XP).

**Scoreboard, Stand 2026-11-26:** 0 XP, alle Abzeichen offen. Die Anleitung und die
Blöcke sind vorbereitet; es gibt noch keinen Live-Turn und damit keinen
verdienten Punkt. Nicht selbst vergeben — Punkte folgen dem Beleg.

## Tutorial-Abschluss und Übergabe an Aufgaben

Das Tutorial gilt erst als durch, wenn alle fünf Punkte belegt sind:

1. Anleitung per `read_file` gelesen (Observation enthält Text aus dieser Datei).
2. Mindestens drei verschiedene Tool-Arten korrekt aufgerufen.
3. GitHub-Repo `st0rax/webagent-rs` per PowerShell abgerufen und `README.md` gelesen.
4. Ein erfolgreicher Bridge-Turn mit `DOGFOODING-OK-20261126` **und** Eintrag in
   `events.jsonl`.
5. Belegdatei unter `docs/proofs/` angelegt und in `docs/CURRENT_WORK.md` verlinkt.

Dann **eine** Abschlussmeldung, keine weitere Aktion:

> TUTORIAL-ABGESCHLOSSEN: XP=&lt;n&gt;, Level=&lt;n&gt;, Abzeichen=&lt;liste&gt;. Bereit für Aufgaben.

Im Bridge-Pfad ist das der Antworttext; im autonomen Pfad eine `message`-Action.
Danach wird gewartet. **Neue Aufgaben kommen vom Nutzer**, nicht aus dem Repo
geraten und nicht aus dem Scoreboard erfunden. Pro Aufgabe gilt weiterhin: kleine,
getestete Scheibe mit Beleg, kein Deployment, kein PR, kein Merge ohne Freigabe.

## Voraussetzung (read-only)

```powershell
(Invoke-WebRequest -Uri 'http://127.0.0.1:8788/health' -UseBasicParsing -TimeoutSec 5).Content
# {"service":"webagent-provider-bridge","status":"ok"}
```

`WEBAGENT_API_KEY` bleibt eine Benutzer-Umgebungsvariable. Kein Tokenwert in Datei,
Kommandozeile, Log oder Prompt; `auth.json` wird nicht geöffnet.

## Abnahme und Grenze

- Erfolg ist nur belegt, wenn die Anleitung per `read_file` gelesen wurde und die
  Antwort den Marker `DOGFOODING-OK-20261126` enthält; zusätzlich protokolliert die
  Bridge den Turn in `%LOCALAPPDATA%\webagent\data\brain_score\events.jsonl`.
- Ein leerer Turn oder `finish_reason=length` ist kein Erfolg. Bei `401` ist die
  Umgebungsvariable im falschen Prozess gesetzt; bei `503` ist das Brain gesperrt
  oder im Cooldown. Nicht raten, nicht umgehen, nicht neu anmelden.
- Produktverbesserung heißt hier: kleine, getestete Scheibe mit Beleg. Kein neues
  Deployment, kein PR, kein Merge nach `master`, kein Fernzugriff ohne Freigabe.
