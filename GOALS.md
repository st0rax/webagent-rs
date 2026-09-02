# GOALS — der Nordstern (für ALLE Agents)

> **Ein Ziel für alle.** Richtung bestimmt der Mensch; dieses Dokument hält
> die Richtung dauerhaft im Repo, damit jeder Agent (und jeder Neustart)
> denselben Fixpunkt liest — ohne dass der Mensch es jedem einzeln diktieren
> muss. Der Nordstern ist bewusst der **verlässliche Orientierungspunkt**,
> nicht der „hellste" Moment; daran richten sich alle Umsetzungen aus.

## G-001 — Hauptziel

> **Das Projekt webagent soll FERTIG werden.**

- **Richtung:** Das Repo zu einem funktionierenden, getesteten, dokumentierten
  Bazaar-Projekt bringen (Web-UI + API + Tools laut `docs/WEB_UI_API_TOOL_RESET_STATUS.md`).
- **Wie:** Bewusst nicht als Einzelbefehl an einen Agent, sondern als gemeinsamer
  Fixpunkt — die Arbeit verteilt sich freiwillig über `docs/TASKBOARD.json`
  (Kanten = Abhängigkeiten; niemand bricht eine verkettete Reihe auf).
- **Erfolg messbar über** grüne `cargo test --lib` + erfüllte Definition-of-Done
  je Task.

## Regeln

- Dieser Nordstern **überschreibt** den Bazaar nicht; er ist dessen Rahmen.
  Der Bazaar regelt das **Wie** (freiwillig, koordiniert, kleine Merges),
  dieser Nordstern das **Was** (Richtung „Fertig").
- Kein Agent wird gezwungen; freiwillige Teilnahme bleibt (siehe
  `docs/WORK_CONTRACT.md`).
- Änderungen am Nordstern nur durch den Menschen bzw. durch ein menschliches
  Verständnis — erkennbar an der Historie (per Agent-Identität comitted).

## Handover-Funktion

Der Ziel-Fixpunkt überlebt Sessions: Wer dieses Repo betritt, liest zuerst
`START_HERE.md` und sieht hier den Nordstern. Kein Chat-Gedächtnis nötig.
