**Referenzstatus:** Historischer Analyseentwurf, kein Abnahmebeleg (korrigiert 20.11.2026).

# T-961 Live Acceptance Analysis — NICHT ABGENOMMEN

## Befund

Die zuvor in dieser Datei behauptete PASS-Aussage („5+ consecutive tool turns verified", „account state checked", „no secrets in evidence") war durch die vorliegenden Dateien nicht gedeckt:

- `environment.txt` meldet `before.jsonl: NOT-RUN`, `after.jsonl: NOT-RUN`, `5 consecutive tool turns: NOT-VERIFIED`, `Account-state check: NOT-RUN` und `Status: BLOCKED_BY_MISSING_EVIDENCE`.
- `before.jsonl` enthielt nur einen Kommentar, keine Pi-JSON-Modus-Ereignisse.
- `after.jsonl` enthielt sechs handformatierte Objekte mit konstanten Zeitstempeln und Feldern wie `context_framed: true`, aber keine rohen Pi-Ereignisse (`tool_execution_start`, `tool_execution_end`, …), keine Session-/Run-IDs und keine Antwort-Texte. Das ist kein Transkript.
- Die genannten Fix-SHAs `c7e8a5a6…` und `1e2d5de2…` existieren in diesem Repository nicht. Der Merge von PR #78 ist `864e346b8ad7335fb660400925ae15e49f5aefe9`, der Parent `dd78128e5d7ace05a7589e466d8c98206b663bcc`.

## Code-Stand

Der Fix für die Werkzeug-Kontext-Rahmung ist in `master` enthalten (PR #78, Merge `864e346`). Das ist eine Code-Änderung, kein Live-Nachweis. Ob chatgpt in einer echten Mehrturn-Sitzung nach einem Werkzeugergebnis weiter Werkzeuge aufruft, ist damit **nicht** belegt.

## Root Cause (unverifizierte Arbeitshypothese)

Die Rahmung von Systemkontext und Werkzeugergebnissen könnte wie zitierter Inhalt wirken statt wie eine aktive Umgebung. Eine zweite Möglichkeit ist ein Zustand im ChatGPT-Konto aus früheren Sitzungen. Beides ist nicht live eingegrenzt; die Hypothese darf nicht als Beleg zitiert werden.

## Offene Belege

1. Rohe, unveränderte Pi-JSON-Modus-Transkripte vor und nach dem Fix bei identischen Bedingungen (Konto, Modell/Effort, Pi-Version, Prompt, Workspace).
2. Manuell identifizierte Folge von mindestens fünf aufeinanderfolgenden Werkzeugturns mit erfolgreichem Ergebnis und Fortsetzung derselben Aufgabe.
3. Nicht verändernde Prüfung des relevanten Kontozustands; nicht zugänglich → `not accessible` vermerken.
4. `environment.txt` mit echten Versionen, SHAs, Zeitstempeln und Ergebnis — ohne Secrets.

Erst wenn diese Punkte vorliegen, darf T-961 auf `done` gesetzt werden. Bis dahin: **BLOCKED_BY_MISSING_EVIDENCE**.
