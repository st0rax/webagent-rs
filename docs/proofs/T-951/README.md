# T-951: Brain-Identitaet und Verlauf in zweiter Person

**Referenz:** Live-Nachweis zu T-951, Stand 2026-09-14. Verbindlich sind START_HERE.md und docs/TASKBOARD.json.

## Build

- master 08fbed6 (enthaelt T-951 a024665 und T-954), `cargo build` nach
  `webagent-deploy-target/debug/webagent.exe`, gestartet 13:42:59 als
  `api serve --port 8788 --headless`.

## Live-Beleg

Frage je Brain: "Wie lautet dein Brain-Name? Antworte nur mit dem Namen in
Kleinbuchstaben." Kein Hinweis auf den Namen im Prompt ausser der
Identitaetszeile der Bridge. `verlauf` sendet vorher einen Zwei-Turn-Verlauf
und loest damit den `[du]`-Rahmen aus.

| Brain | Fall | HTTP | Dauer | Antwort | exakt |
|---|---|---|---|---|---|
| deepseek | einzeln | 200 | 19 s | deepseek | ja |
| qwen | einzeln | 200 | 32 s | qwen | ja |
| deepseek | verlauf | 200 | 14 s | deepseek | ja |
| qwen | verlauf | 200 | 25 s | qwen | ja |

Vorher, ohne Identitaetszeile, nannte sich deepseek ueber Pi auf die Frage
nach dem Kandidatennamen "claude".

## Grenzen

- `auto` erhaelt keine Identitaetszeile, weil erst der Pool das Brain waehlt.
- chatgpt, gemini, kimi, mistral, claude und zai sind in diesem Nachweis nicht
  geprueft.
