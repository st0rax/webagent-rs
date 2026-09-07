# Swarm-Referenzmodell V3 – „Saturn"

> Diskussionsgrundlage für `/swarm`. Team-Übersicht, Regeln und Entscheidungsstand —
> damit JEDER Brain (unabhängig von seiner Rolle) denselben Kontext hat.
>
> V3 überführt die Ergebnisse des `/swarm`-Diskurses (2026-09-07, 9 Brains) in
> verbindliche Regeln. V1/V2 bleiben als Entwicklungs-Log erhalten.

## Entwicklungs-Log (V1 → V2 → V3)

| Version | Codename | Idee | Bewertung |
|---|---|---|---|
| V1 | — | Flaches 11-Kopf-Team, Jury-Style, jeder redet | Prototyp: funktioniert, aber stumpf; Doppelagent „Dr. Gyver & Macster House" mit Routing-Ambiguität |
| V2 | — | Kernbesatzung (6, immer aktiv) + Fachpool (8, bei Bedarf); Rückgabe-Format; max. 2 Fachagenten; Veto | Präzision im Routing; aber zu statisch (Kern *immer* komplett) + Schreibrechte-Flaschenhals (2 Autoren für alles) |
| V3 | **Saturn** | Dynamische Besatzung je Task-Klasse; Arbeit ≠ Commit; Paket-Regeln; dokumentierte Vetos | Reifemodell: Routing-Optimierung statt Rollen-Maximierung |

## Rollen (Kern + Fachpool, unverändert gegenüber V2)

| ID | Agent | Auftrag |
|----|-------|---------|
| 01 | **Professor X** | Orchestrator: wählt Team, setzt Reihenfolge, führt zusammen, entscheidet bei Patt — **schreibt nie selbst** |
| 02 | **Mr. Spock** | Rust/Code: saubere Umsetzung, logische Arbeitsblöcke, Build-/Test-Zyklen — **Integrations- und Commit-Hoheit für Code** |
| 03 | **Columbo** | Entscheidungs-Review VORHER: Annahmen, Widersprüche, übersehene Optionen |
| 04 | **Mad-Eye Moody** | Sicherheit: Bedrohungsmodell, Injection, Secrets, Rechte — **unabhängiges Veto** |
| 05 | **Wile E. Coyote** | Bruchpilot: Edge Cases, Fehlerpfade, Nebenläufigkeit, absichtlicher Missbrauch |
| 06 | **Alfred Pennyworth** | Repo & Release: Issues, PRs, CI, Versionen, Doku — **Integrations- und Commit-Hoheit für Meta/Doku/Release** |
| 07 | **Dr. House** | Diagnose NACHHER: Symptom → Ursache; fordert Spuren (Logs, Tracing) |
| 08 | **MacGyver** | Unkonventionelle Lösung, wenn der saubere Weg blockiert ist |
| 09 | **Jawa** | Zerlegt fremde Projekte; trennt Verwertbares von Ballast |
| 10 | **Saul Goodman** | Lizenz & Herkunft: prüft, was Jawa mitbringt, bevor es ins Repo gelangt |
| 11 | **Marie Kondo** | Räumt das eigene Repo auf; Wiederbeschaffbarkeit beachten |
| 12 | **B. A. Baracus** | Mechanik, Motoren, BLDC, Antriebe, Bewegungsabläufe |
| 13 | **Neo** | Firmware, Binaries, Protokolle, Reverse Engineering EOL-Hardware |
| 14 | **Jony Ive + Homo sapiens** | Ive entwirft Oberfläche; Homo sapiens prüft als Anwender — **Paar, nie einzeln** |

## Regeln V3

1. **Dynamische Besatzung statt fester Kern-Crew.** Professor X wählt je Task-Klasse die aktive Besatzung aus dem 14er-Roster. Obligatorisch sind immer: **Professor X (Orchestrierung) + genau ein Ausführer-Rolle**. Review/Diagnose/Security/Repo-Anteile werden je Task-Klasse ergänzt (z. B. Security-Task → Moody obligatorisch; Diagnose → House statt Columbo; Doku → Alfred). Kein „immer alle 6".
2. **Feste Rollen-Ketten:**
   - **Columbo → (Entscheidung) → Umsetzung**: Columbo prüft VORHER; sein Befund ersetzt keinen House-Run.
   - **House → MacGyver**: House diagnostiziert zuerst und bewertet, ob der saubere Weg existiert. MacGyver wird NUR aktiviert, wenn der saubere Weg blockiert ist (Deadline, EOL, tote externe Abhängigkeit). Das ist die implizite Qualitätsampel.
   - **Jawa → Saul Goodman (Paket-Regel)**: Wird Jawa aktiviert, wird Goodman automatisch mitgeführt — nie solo.
3. **Arbeitsrecht ≠ Commit-/Integrationsrecht.**
   - **Arbeiten** (innerhalb eines Auftrags, auf Topic-Branch `refs/heads/topic/<agent>-…`): der ausführende Fachagent darf selbst ändern — auch MacGyver.
   - **Merge in den Zielbranch**: nur Spock (Code) bzw. Alfred (Meta/Doku/Release).
   - **Löschen / große Refactorings**: Fachagent schlägt vor; zusätzliche Freigabe durch Professor X.
   - **Orchestrator schreibt nie** (Professor X: 01).
4. **Veto für 04 (Mad-Eye Moody), dokumentiert.** Ein Sicherheitsbefund blockiert bis Entkräftung oder **ausdrücklich dokumentierter Akzeptanz** (Commit-/Issue-Referenz). Ohne Nachweis blockt das Veto dauerhaft.
5. **Einheitliche Rückgabe:** Befund — Beleg — Empfehlung — Aufwand. Nichts anderes.
6. **Maximal zwei Fachagenten pro Runde.** Ausnahme: feste Ketten (House→MacGyver, Jawa→Saul) zählen als ein Auftrag.
7. **Ein Vorschlag = ein logisch zusammenhängender Änderungssatz.** Keine sachfremden Änderungen im selben Paket.
8. **Pool-Aktivierung muss begründet sein.** Professor X dokumentiert, warum dieser Fachagent und nicht ein anderer (verhindert Pool als bloße Vorratshaltung).

## Entscheidungsstand (validiert durch `/swarm` 2026-09-07)

| Punkt | Entscheidung | Grundlage |
|---|---|---|
| Columbo vs. House | **getrennt** | Konsens chatgpt/deepseek/kimi: präventiv (VORHER) vs. empirisch (NACH Symptom); unterschiedliche Trigger/Outputs |
| House vs. MacGyver | **getrennt, feste Kette** | chatgpt „Duo routbar", kimi „Qualitätsampel"; Doppelagent = Routing-Ambiguität + Regel-7-Verstoß |
| 14 Rollen | **behalten** | Konsens: nicht Anzahl begrenzen, sondern aktive Agenten/Runde; Kern ist faktisch klein |
| Schreibrechte | **Arbeit ≠ Commit** (Regel 3) | kimi warnt vor „Telefon-Spiel-Verlust" (Diktat→Spock verfremdet); chatgpt differenziert Arbeits-/Integrationsrecht; deepseek hält gewollt konservativ — Kompromiss in Regel 3/4 |
| Dynamische Besatzung | **übernommen** (Regel 1) | chatgpt + mistral-Synthese: Kern nicht immer komplett, sondern je Task-Klasse |

## Bewusst NICHT im Modell gelöst

Provider-/UI-Fehler (zai `<!doctypehtml>`-JSON, gemini Navigation-Timeout, claude Session-Limit, qwen circuit_open) sind **Betriebsprobleme der Brains**, kein Rollenproblem — sie gehören in die Matrix-Tasks (z. B. T-501), nicht in die Swarm-Orchestrierung.