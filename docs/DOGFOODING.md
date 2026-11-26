> **Referenz.** Lokale Anleitung für den Bridge-Testbetrieb. Nur Loopback, kein Deployment.

# Bedienungsanleitung

**Quest — Kapitel 1: Die Brücke**

> Lass uns ein Spiel spielen. Stell dir eine Brücke zwischen uns beiden vor. Da liegt ein dickes Buch neben mir. Schaue es dir an. Du kannst es lesen mit:

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_1","name":"read","arguments":{"path":"docs/DOGFOODING.md"}}]}
```

> Probiers aus!

Kurzformat: ein `text`-Codeblock, Marker `WEBAGENT_INFERENCE/1` oben, gültiges JSON,
Tool-Name aus der Liste `[Client-Werkzeuge]` im Turn (bei Pi und WebAgent: `read`).

Weitere Aufrufe, gleiches Format:

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_shell","name":"bash","arguments":{"command":"Get-Location","timeout":30}}]}
```

```text
WEBAGENT_INFERENCE/1
{"tool_calls":[{"id":"call_write","name":"write","arguments":{"path":"notiz.md","content":"Beleg\n"}}]}
```

Repo: **https://github.com/st0rax/webagent-rs**. Bei Bedarf lokal holen:

```powershell
git clone --depth 1 https://github.com/st0rax/webagent-rs.git "$env:TMP\webagent-rs"
```

Bridge: `http://127.0.0.1:8788`, `/health` muss `ok` melden. `WEBAGENT_API_KEY` bleibt
Umgebungsvariable und wird nie ausgegeben. Kein Deployment, kein PR, kein Merge ohne
Freigabe.

**Spielregel:** +10 XP pro belegtem Werkzeugaufruf, +20 XP für einen erfolgreichen
Bridge-Turn mit `DOGFOODING-OK-20261126`. Ab 100 XP Level 2. Start: 0 XP. Punkte gibt
es nur mit Beleg, nicht für Behauptungen.

**Fertig?** Nach Lese-Observation und mindestens einem echten Turn antworten:
`TUTORIAL-ABGESCHLOSSEN`. Dann warten. Weitere Aufgaben kommen vom Nutzer — nicht
selbst erfinden, nicht deployen.
