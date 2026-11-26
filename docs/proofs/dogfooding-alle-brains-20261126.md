> **Referenz.** Live-Matrix der gamifizierten Quest gegen alle Brains der lokalen Bridge. Kein Deployment, kein PR, kein Merge.

# Dogfooding-Quest: alle Brains — 2026-11-26

## Aufbau

- Bridge: `webagent api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`, PID 17372, `/health` = `ok`.
- Client: Pi 1.6.1, pro Brain isoliertes `PI_CODING_AGENT_DIR`, `--no-session`, `--tools read,bash,write,edit`, `--mode json`.
- Quest: exakt der Text aus `docs/DOGFOODING.md` (291 Byte / 83 Tokens `cl100k_base`, 84 `o200k_base`), als **erste** User-Nachricht.
- Skript: `C:\Users\storax\AppData\Local\Temp\opencode\dogfooding_all_brains.ps1` (außerhalb des Repos).
- Rohtranscripts: `C:\Users\storax\AppData\Local\Temp\opencode\dogfooding-all\<brain>\transcript.jsonl` (temporär, nicht dauerhaft archiviert).
- Token nur als Prozessumgebungsvariable; `auth.json` unberührt; kein Wert in Datei, Log oder Kommandozeile.

**Zählregel für die Tabelle:** XP = 10 × Anzahl `tool_execution_start` mit Observation + 20 nur, wenn die **endgültige Antwort** `DOGFOODING-OK-20261126` enthält. Selbstberichte des Brains zählen nicht; Treffer im eingelesenen Anleitungstext zählen nicht.

## Ergebnis

| Brain | Tools (Belege) | Marker | `TUTORIAL-ABGESCHLOSSEN` | XP belegt | Verdict |
|---|---:|---|---|---:|---|
| `webagent/auto` | 0 | nein | nein | 0 | Verweigerung: „keinen Zugriff auf lokale Dateien"; kein Tool-Call |
| `webagent/claude` | 0 | nein | nein | 0 | `MODEL_OUTPUT_ERROR`: ungültiger Werkzeugumschlag (trailing characters) |
| `webagent/deepseek` | 8 (`read`, `bash`) | ja | ja | **100** | vollständig; echter Bridge-Turn mit Marker, danach Warten |
| `webagent/gemini` | 1 (`read`) | nein | ja | 10 | Abschluss ohne Marker → die +20 sind nicht belegt |
| `webagent/kimi` | 0 | nein | nein | 0 | `send_phase=content_check: Composer-Feld nicht gefunden (Timeout)` |
| `webagent/mistral` | 2 (`read`, `bash`) | ja | ja | 40 | vollständig nach Regel; kein drittes Tool, kein GitHub-Beleg |
| `webagent/qwen` | 3 (`read`, `bash`) | nein | ja | 30 | Abschluss ohne Marker |
| `webagent/zai` | 2 (`read`, `bash`) | ja | ja | 40 | vollständig nach Regel |
| `webagent/chatgpt` | 1 (`read`) | nein | nein | 10 | Abbruch nach Composer-Timeout, danach Circuit-Breaker offen |

**5 von 9** Brains meldeten `TUTORIAL-ABGESCHLOSSEN`. **3 von 9** erbrachten den Marker in der Endantwort (`deepseek`, `mistral`, `zai`). `chatgpt` scheiterte erst nach dem erfolgreichen Lese-Call am Folgeturn. `claude` und `kimi` scheiterten an der Browser-/Umschlag-Schicht, nicht an der Quest. `auto` verweigerte ausdrücklich ("Ich bin Gemini … kann das Buch leider nicht öffnen").

## Was die Brains tatsächlich taten

- **deepseek:** `read docs/DOGFOODING.md` → Lese-Observation (1703 Zeichen); `curl /health` → `{"service":"webagent-provider-bridge","status":"ok"}`; `curl /openapi.json` → `not found`; ein Composer-Fehler mit anschließendem erfolgreichem Retry; `curl /` → vollständige SPA. Endantwort mit Marker, XP und „Warte jetzt auf die nächsten Aufgaben vom Nutzer". Acht belegte Tool-Calls.
- **mistral / zai:** Lese-Observation plus Healthcheck, Marker in der Endantwort, danach Warten.
- **gemini / qwen:** korrekter Abschluss, aber ohne Marker; die +20 bleiben unbelegt.
- **chatgpt:** ein `read`-Call lief, der Folgeturn bekam dreimal `Composer-Feld nicht gefunden (Timeout)`, danach `circuit_open` mit Cooldown.

## Grenzen

- Die XP-Tabelle ist eine **Auswertung der Observations**, kein Spielstand. Ein dauerhaftes Scoreboard existiert in dieser Runde noch nicht.
- Mehrere Brains kommen ohne Zutun des Prompts zum Abschluss, obwohl die Quest nur drei Past tense-Tätigkeiten und keine „Main Quest" mehr enthält; die Anleitung selbst steuert das Verhalten.
- `webagent/auto` ist ein virtueller Router, kein Brain; sein Verdict ist die Entscheidung des Routers plus des gewählten Modells.
- Keine Zugangsdaten gelesen oder ausgegeben; kein Deployment, kein PR, kein Merge nach `master`.
