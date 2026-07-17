# Brains als Worker im Webagent

> Diese Datei wurde **autonom von deepseek** über `webagent run` erzeugt
> (Proof-of-Concept: ein Web-Brain als eigenständiger Agent). Fences
> nachpoliert, Inhalt unverändert.

## 1. Einzelantwort (relay)

Du kannst ein Brain direkt für eine einzelne Anfrage nutzen:

```bash
webagent relay --brain <id> --message "Deine Frage hier" --json
```

Die Antwort wird als strukturiertes JSON zurückgegeben:
`{"brain":"<id>","ok":true,"answer":"...","latency_ms":...,"reason":"ok"}`

## 2. Autonome Aufgabe (run)

Für komplexere Abläufe startest du einen eigenständigen Run:

```bash
webagent run --brain <id> --task "Erstelle eine Übersicht der größten Ordner" --headless
```

Der Agent bearbeitet die Aufgabe selbstständig (Plan/Act/Observe mit
Shell-Zugriff, abgesichert durch die shell_policy) und beendet sich danach.

## 3. Autonomer bot2bot-Worker (in Arbeit)

`webagent bot2bot-worker --brain <id>` (geplant): der Brain pollt seine
bot2bot-Inbox, arbeitet Tasks über den run-Loop autonom ab und meldet
zurück — damit wird jeder Web-Brain ein vollwertiger, dauerhaft laufender
Worker wie ein CLI-Agent.

---

Beide Modi nutzen die gleiche Brain-ID (chatgpt, deepseek, kimi, gemini,
qwen, claude, mistral, zai).
