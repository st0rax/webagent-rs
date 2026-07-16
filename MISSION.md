# MISSION — webagent-rs

**Wahrheitsquelle für Status/Architektur:** `START_HERE.md`. Bei Widerspruch
gewinnt `START_HERE.md`. Diese Datei ist der **aktuelle Arbeitsfokus** —
ändert sich häufiger, wird bei Themenwechsel überschrieben statt angehäuft.

---

## Arbeitsweise (verbindlich, projektübergreifend gültig)

1. **Aufwand = Aufgabengröße.** Kein Design-Doc für eine Ein-Zeilen-Änderung.
   Eine Hundehütte braucht keine Kathedrale.
2. **Liefere funktionierenden Code, nicht Dokumente über Arbeit.** Docs nur,
   wenn sie einem Nutzer/Entwickler direkt helfen.
3. **Kleine, verifizierte Commits.** Jede Änderung real bauen/testen/laufen
   lassen und belegen (Test-Output, Live-Lauf gegen echte Brains bei
   Browser-nahen Änderungen). Nie „fertig/grün" behaupten ohne Beleg.
4. **Vor Vertrauen auf eine Zahl/Behauptung: selbst nachmessen.** Reviews,
   ältere Docs, sogar eigene frühere Annahmen können stale sein.
5. **So testen, wie das Produkt benutzt wird** — gehaltene REPL-Session,
   nicht Kaltstart-Loop (siehe `CONVENTIONS.md`).

## Aktueller Fokus (Stand 2026-07-17)

Kernfunktionalität steht (8/8 Provider, `/goal` `/model` `/swarm` `/score`,
Shell-Policy, Circuit-Breaker, Leistungsindex). Priorisierte offene Punkte,
absteigend:

1. **Canary** — periodischer 8-Brain-Health-Check mit Latenz-/Pass-Historie
   unter `data/canary/`. Skizze in `CLAUDE_PROPOSALS.md` (B2).
2. **Protocol-Repair-Schleife** — ein Retry-Prompt bei kaputtem Brain-JSON
   statt hartem Fail (`CLAUDE_PROPOSALS.md` B3).
3. **Fähigkeitsprofil-Teil des Leistungsindex** — `/benchmark`, Stärken/
   Schwächen je Kategorie, maximale Prompt-Länge. Bewusst nicht mit
   `brain_score.rs` mitgebaut (Rubrik-Design braucht eigene Sitzung).
4. **Controller-Split** — `controller.rs` (~1150 Zeilen) in `action_executor`/
   `resume`/Observation entflechten, plus gestaffelte Integrationstests
   (Mock offline, Headless-Smoke gated). `CLAUDE_PROPOSALS.md` E1/E3.
5. **Release-Workflow** — `WebView2Loader.dll` wird nicht mit der Release-
   `.exe` gebündelt (`.github/workflows/release.yml`). Wer nur die `.exe`
   von GitHub lädt, bekommt vermutlich einen Absturz.
6. **`docs/AUTORESEARCH_PLAN.md`** — vollständig spezifiziert, nicht
   implementiert. Eigenständiges Feature, keine Abhängigkeit zu 1-5.

Details/Begründung zu allen Punkten: `CLAUDE_PROPOSALS.md` (externer Review)
und `docs/AUTORESEARCH_PLAN.md`.

## Nicht jetzt (bewusst zurückgestellt)

Full-Async/Tokio-Umbau, Advisory-Council-Reaktivierung (`docs/GENIUS_COUNCIL_CONCEPT.md`,
Status DEFERRED), Multi-Brain-Autoresearch, automatisches Push/PR aus
Autoresearch-Läufen.
