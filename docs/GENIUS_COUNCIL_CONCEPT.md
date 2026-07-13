# Genius Council — Konzept

> **STATUS: DEFERRED / KONZEPT, NICHT v1.**
> Dieses Dokument ist ein *durchdachter Vorschlag*, keine Roadmap und keine
> Spezifikation für die aktuelle Version. Das Feature ist bewusst
> **zurückgestellt**. Storax ist über die genaue Endfunktion noch unsicher — das
> Dokument dient als ehrliche Grundlage für eine spätere Entscheidung, nicht als
> Auftrag zur Implementierung. Nichts hier blockiert oder verändert den v1-Kern
> (`login`, `run`, `repl`, `diagnose`, `doctor`, `watchdog`).

---

## 1. Idee in einem Satz

Statt **ein** Brain eine Aufgabe planen zu lassen, lässt der „Genius Council"
**mehrere** Web-Chat-Brains (chatgpt, claude, deepseek, gemini, kimi, qwen,
mistral, zai) als *Rat von Genies* zusammenarbeiten: sie schlagen unabhängig
Lösungen vor, prüfen sich gegenseitig, und ein Moderator-Brain synthetisiert
daraus eine Antwort. Der Name ist eine augenzwinkernde Anspielung („Council of
Geniuses", South-Park / Apple-„Genius").

**Warum das reizvoll ist:** Web-Chat-Brains sind kostenlos (angemeldete Session,
kein API-Key), unterschiedlich stark und machen unterschiedliche Fehler. Ein
Ensemble kann Einzelfehler ausgleichen und breitere Ideen liefern.

**Warum es zurückgestellt ist:** Der Nutzen gegenüber „ein gutes Brain" ist
unbewiesen. Der Orchestrierungsaufwand (mehrere Browser, Rate-Limits, Parsing
freier Chat-Antworten) ist hoch. Für v1 zählt ein *funktionierender*
Einzel-Brain-Agent mehr als ein beeindruckendes, aber fragiles Ensemble.

---

## 2. Rollen der Brains (Loadout)

Aus der Python-Version (`council.py`, `DEFAULT_ROLES`) übernehmenswert ist die
Idee eines **Loadouts**: jedem verfügbaren Brain wird eine Standardrolle nach
seinen Stärken zugewiesen. Das ist eine sinnvolle Voreinstellung, kein Gesetz.

| Brain | Rolle | Zweck |
|---|---|---|
| chatgpt | moderation+synthesis | Moderation, Orchestrierung, Synthese |
| claude | analysis+specification | Kritische Analyse, präzise Spezifikation |
| gemini | research+multimodal | Breites Wissen, Recherche |
| qwen | coding+multilingual | Coding, mehrsprachige Aufgaben |
| deepseek | math+coding+critique | Mathematik, Coding, Gegenprüfung |
| kimi | long-context+planning | Long-Context, agentische Planung |
| mistral | eu-perspective+review | Unabhängige Perspektive, knappe Reviews |
| zai | glm+reasoning | GLM, Chinesisch/Englisch Reasoning |

Regeln:

- **Nur verfügbare Brains** kommen ins Loadout (angemeldet, nicht rate-limited).
  Vgl. `build_loadout()` — es filtert unbekannte/fehlende Provider heraus.
- Rollen sind **Voreinstellungen, keine harten Fähigkeiten.** Ein Brain, dem die
  Rolle „math" zugewiesen ist, wird dadurch nicht besser in Mathe — die Rolle
  steuert nur, *welchen Prompt* es bekommt und *welches Gewicht* seine Stimme
  in seinem Fachgebiet hat.
- Genau **ein** Brain ist Moderator pro Runde (siehe §4).

---

## 3. Wie ein Ziel erreicht wird (High Level)

Der Council ist eine **Schicht über** dem bestehenden Einzel-Brain-Controller,
kein Ersatz. Ein Council-Lauf produziert am Ende *einen Plan/eine Antwort*, die
dann genauso wie heute vom Controller ausgeführt werden kann — oder die einfach
als Textantwort zurückgegeben wird.

```
Ziel ──▶ [Council-Deliberation]  ──synthetisierter Plan──▶  Controller/Executor
              (mehrere Brains)                                  (unverändert)
```

Zwei Modi sind denkbar (nicht beide für v-nächste nötig):

1. **Advisory-Modus (empfohlen zuerst):** Der Council liefert nur *Text* —
   Analyse, Optionen, Empfehlung. Kein automatisches Ausführen von Shell-Befehlen
   aus Council-Output. Risikoarm, sofort nützlich, leicht zu bauen.
2. **Executive-Modus (später, optional):** Der synthetisierte Plan wird ins
   `webagent/1`-Protokoll gegossen und vom bestehenden Controller ausgeführt.
   Deutlich riskanter (mehrere fehlbare Brains, deren Konsens Shell-Zugriff
   bekommt) — nur mit denselben Vertrauensannahmen wie heute und idealerweise
   mit menschlicher Bestätigung des Plans.

---

## 4. Deliberations- und Abstimmungsablauf

Der Kern, angelehnt an `council.py` + `battleroyale.py`. Vier Phasen:

### Phase A — Moderatorwahl (optional, einmalig pro Runde)

Wenn kein Moderator gesetzt ist, wählt der Rat einen. `council.py` bietet zwei
saubere Verfahren, die übernehmenswert sind:

- `elect_ranked_moderator()` — **Instant-Runoff** aus vollständigen Rankings.
  Jedes Brain ordnet alle Kandidaten; iterative Elimination des Schwächsten,
  Borda-Position als Tie-Break, lexikalische Ordnung als letzter stabiler
  Ausweg. **Keine zusätzlichen Web-Turns**, weil die Rankings schon vorliegen —
  das ist der günstige Weg und für Rust zuerst zu portieren.
- `elect_moderator()` — Plurality mit echten Stichwahl-Ballots über mehrere
  Runden. Flexibler, aber teurer (jede Runde = neue Prompts an alle Brains).

Das Ergebnis (`ElectionResult`) meldet ehrlich, ob ein `fallback_used` griff
(symmetrisches Patt → sortierter Default), statt eine Mehrheit vorzutäuschen.
Der gewählte Moderator wird persistiert (`CouncilState`), damit spätere Aufrufe
ihn wiederverwenden.

**Vereinfachung fürs erste Rust-Feature:** Moderatorwahl ganz weglassen. Nimm
`chatgpt` (oder das erste verfügbare Brain) fest als Moderator. Die Wahl ist
elegant, aber für den ersten Nutzen nicht nötig.

### Phase B — Vorschlag (parallel, unabhängig)

Jedes Nicht-Moderator-Brain bekommt dieselbe Aufgabe und antwortet **unabhängig**
(kein Brain sieht die Antworten der anderen). So bleiben die Vorschläge
unkorreliert — das ist der ganze Wert eines Ensembles.

Aus `battleroyale.py` übernehmenswert:
- Ein **enger Prompt** mit striktem Ausgabeformat (z. B. „max. 3 nummerierte
  Vorschläge, kein Ranking, keine Selbstbewerbung") → leichter zu parsen.
- **Robuste Parser** (`parse_suggestions`, `_extract_ideas`), die nummerierte
  Listen, Markdown-Blöcke und JSON tolerieren. Web-Chats halten sich **nie**
  perfekt ans Format; der Parser muss defensiv sein.
- **Rate-Limit-Erkennung** per Keyword (`"limit"`, `"nachrichtenlimit"`,
  `"quota"` …): ein limitiertes Brain wird als *unavailable* übersprungen, nicht
  als Fehler behandelt. Das ist bei kostenlosen Web-Sessions Alltag.

Praktisch heißt das in Rust: sequentielle Abfrage (ein Browser/Turn nach dem
anderen) ist stabiler als echte Parallelität — die Python-Version tut genau das
bewusst (`max_workers` wird ignoriert, „sequential for Playwright stability").

### Phase C — Review / Gegenprüfung

Die gesammelten Vorschläge werden den Brains zur **Bewertung** vorgelegt. Zwei
Bausteine aus der Python-Version:

- **Ranking** (`collect_rankings`, `ranking_prompt_for_ideas`): jedes Brain wählt
  aus dem Pool die besten N und gibt eine ID-Liste als JSON zurück. Aggregation
  über die Ranglisten ergibt eine Reihenfolge.
- **Verdict-Review** (`leader_calibration.py`): pro Vorschlag ein knappes
  `VERDICT:<approve|reject|escalate> REASON:<…>`. Einfach zu parsen, gut für
  Ja/Nein-artige Entscheidungen.

Wichtig aus `leader_calibration.py`: **blind reviewen.** Der Review-Prompt darf
weder die „Goldlösung" noch die Antworten anderer Brains durchsickern lassen
(`build_blind_review_prompt`, `gold_verdict_hidden_from_prompt`), sonst kopieren
die Brains einander statt unabhängig zu urteilen.

### Phase D — Moderator-Synthese

Der Moderator bekommt: die Aufgabe, alle Vorschläge und die Review-/Ranking-
Ergebnisse. Er produziert **eine** kohärente Endantwort — das ist die eigentliche
„Rat"-Leistung: nicht Mehrheitsvoting allein, sondern ein Brain, das begründet
zusammenführt, Widersprüche auflöst und die beste Option benennt.

Konsens-Regel (aus `consensus.py` übernehmenswert, sinngemäß): **Schweigen ist
keine Zustimmung.** Ein Vorschlag gilt nur dann als „vom Rat getragen", wenn es
belegte Zustimmung gibt — nicht, weil niemand widersprochen hat. Das hält die
Ausgabe ehrlich.

---

## 5. Was aus der Python-Version übernehmenswert ist — und was nicht

### Übernehmen (Wert klar, Aufwand vertretbar)

| Baustein | Quelle | Warum |
|---|---|---|
| Loadout / Rollen-Map | `council.py` | Klare, testbare Voreinstellung |
| `parse_selection` (`<alle\|a,b> <task>`) | `council.py` | Einfache, gute CLI-Syntax |
| Instant-Runoff-Wahl | `council.py::elect_ranked_moderator` | Keine Extra-Web-Turns, deterministisch |
| Ehrlicher `fallback_used`-Flag | `council.py` | Täuscht keine Mehrheit vor |
| Robuste Antwort-Parser | `battleroyale.py`, `council.py` | Web-Chats halten Formate nie exakt ein |
| Rate-Limit-als-*unavailable* | `council.py`, `battleroyale.py` | Realität kostenloser Sessions |
| Sequentielle Abfrage | beide | Stabiler als Pseudo-Parallelität im Browser |
| Blind-Review | `leader_calibration.py` | Verhindert Nachplappern |
| „Schweigen ≠ Zustimmung" | `consensus.py` | Hält Konsens ehrlich |
| Strikte, enge Prompts | alle | Weniger Parsing-Schmerz |

### Weglassen / stark vereinfachen (Über-Engineering-Gefahr)

| Baustein | Quelle | Warum weglassen |
|---|---|---|
| Kumulativer Leistungsindex mit Score-Ökonomie | `performance.py` | Umfangreiche Buchhaltung (Kosten-Einheiten, Effizienz, Retries, Ballot-Matches) — Reiz gegenüber Nutzen unklar. Wenn überhaupt, viel später. |
| Leader-Kalibrierung mit Gold-Standards & Inter-Rater-Metriken | `leader_calibration.py` | Ganze Forschungs-Maschinerie, um „wer moderiert am besten" zu messen. Für ein optionales Feature massiv überdimensioniert. |
| `NO_AUTO_APPOINTMENT`-Guards, verbotene Metrik-Keys | `leader_calibration.py`, `performance.py` | Diese Schutzwälle existieren nur, *weil* es die komplexe Autoritäts-/Score-Mechanik gibt. Ohne sie entfällt der Schutzbedarf. |
| Consensus-Workspace (Multi-Agent-Dateisystem-Protokoll) | `consensus.py` | Ein *separates*, viel größeres Feature (Agenten kommunizieren über Markdown-Outboxen). Nicht Teil von „Genius Council". |
| Battleroyale-Bot2Bot-Inbox-Posting | `battleroyale.py` | An die alte Grok/Claude-Bot2Bot-Pipeline gekoppelt; im Rust-Port nicht vorhanden. |
| Idea-Collection „Tabula Rasa" (10 Ideen/Brain) | `council.py` | Spezifisch für „optimiere dich selbst"-Läufe, nicht für Nutzerziele. Optional als Sonderkommando, nicht im Kern. |
| Persistente Moderator-Wahl-Historie | `council.py::CouncilState` | Nette Ergänzung, aber nicht für den ersten Nutzen nötig. |

**Leitprinzip:** Der Reiz der Python-Version liegt in `council.py`,
`battleroyale.py` und den *Parsern*. Der halbe Rest (`performance.py`,
`leader_calibration.py`) ist Infrastruktur, um Brains gegeneinander zu
*benoten* und Moderatoren *wissenschaftlich* zu küren — ein faszinierendes
Kaninchenloch, das den Kernnutzen (bessere Antwort durch Ensemble) **nicht**
braucht.

---

## 6. Abgrenzung: nicht über-engineeren

Wenn dieses Feature je gebaut wird, sollte die **erste** Iteration bewusst klein
sein:

- **Advisory-Modus zuerst.** Nur Text raus, kein Shell-Zugriff aus Council-
  Output. Beweist den Nutzen, bevor Risiko dazukommt.
- **Fester Moderator** (erstes verfügbares Brain). Keine Wahl-Mechanik in v0.
- **Eine Runde** Vorschlag → Review → Synthese. Keine Mehrfach-Iteration.
- **2–3 Brains genügen** zum Testen, nicht alle acht.
- **Kein Scoring, keine Kalibrierung, keine Persistenz von Ranglisten.**
- **Sequentiell, nicht parallel.** Ein Browser nach dem anderen.

Ein Feature, das man abschalten kann und das nie im Standardpfad liegt, ist
besser als eine ausgefeilte Ensemble-Engine, die den einfachen `run`-Befehl
verkompliziert. Der Einzel-Brain-Agent bleibt das Produkt; der Council ist Kür.

**Ehrliche Nutzenfrage, offen zu beantworten bevor gebaut wird:** Liefert ein
Council messbar bessere Ergebnisse als „nimm claude oder chatgpt allein"? Wenn
nein, ist das ganze Feature Zierrat. Ein billiger Vorab-Test (dieselben 10
Aufgaben einmal Einzel-Brain, einmal 3er-Council, Ergebnisse vergleichen) sollte
*vor* jeder ernsthaften Implementierung stehen.

---

## 7. Mögliche CLI-Oberfläche (Skizze, nicht verbindlich)

```
webagent council --brains alle       --task "<ziel>"   # Advisory, alle verfügbaren
webagent council --brains claude,chatgpt,deepseek --task "<ziel>"
webagent council --brains alle --moderator chatgpt --task "<ziel>"
```

Parsing-Syntax analog `parse_selection`: `alle` oder Kommaliste, dann die
Aufgabe. Ausgabe im Advisory-Modus: Vorschläge je Brain, aggregiertes Ranking,
Moderator-Synthese — als Text, nichts wird automatisch ausgeführt.

---

## 8. Referenzen (Python, nur zur Inspiration — nicht portieren-1:1)

- `src/webagent/council.py` — Loadout, Wahl-Verfahren, Parser, Idea-Collection
- `src/webagent/battleroyale.py` — Vorschlags-Sammlung, robuste Suggestion-Parser
- `src/webagent/leader_calibration.py` — Blind-Review, Verdict-Parsing (Muster gut, Umfang zu groß)
- `src/webagent/consensus.py` — „Schweigen ≠ Zustimmung", Multi-Agent-Workspace (eigenes Feature)
- `src/webagent/performance.py` — Score-Ökonomie (bewusst nicht übernehmen)
