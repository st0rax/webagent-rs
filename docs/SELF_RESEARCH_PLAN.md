# /autoresearch.self — Swarm-Selbstbewertung (Spec)

> STATUS: SPEZIFIZIERT 2026-07-21, Umsetzung delegiert. Verwandt, aber NICHT
> identisch mit dem metrik-getriebenen `autoresearch` (Modify/Verify/Keep/
> Discard). Dies ist eine **Prioritätsfindung durch den Schwarm**: N Vorschläge
> je Brain einsammeln, konsolidieren, abstimmen, gerankte Top-K ausgeben.
> Muster manuell erprobt (2026-07-20/21, Wiki [[verbesserungs-top10-2026-07]]).

## 1. Zweck

Ein Befehl, der das Projekt vom eigenen Brain-Pool bewerten lässt: "Was sind
die wichtigsten nächsten Verbesserungen?" — reproduzierbar, ohne Handarbeit.

## 2. Ablauf (4 Phasen)

1. **Sammeln:** Jedes verfügbare Brain bekommt Projektfakten + "gib GENAU N
   konkrete Verbesserungsvorschläge, nummerierte Liste". Isolierte Profile wie
   beim Swarm (`prepare_swarm_profile`). Blockierte/fehlgeschlagene Brains
   werden übersprungen (flaggen, nicht abbrechen — external-blocks-Regel).
2. **Konsolidieren:** alle ~N·M Vorschläge poolen; EIN Orchestrator-Brain
   (Reliability-Auswahl wie im Swarm, `brain_score`) dedupliziert/clustert sie
   zu einem nummerierten Katalog distinkter Vorschläge (Prompt: "fasse
   Duplikate zusammen, geib eine nummerierte Liste distinkter Vorschläge").
   **Fallback:** schlägt die Konsolidierung fehl → roher gepoolter Katalog
   (dedupe per exaktem Zeilenvergleich), damit die Runde nie ganz scheitert.
3. **Abstimmen:** der Katalog geht an jedes Brain: "wähle die K wichtigsten,
   nur Nummern, absteigende Priorität". Antworten geparst (Ziffern, 1..=Kat.).
4. **Auszählen:** gewichtete Borda-Zählung (Platz 1 = K Punkte … Platz K = 1),
   plus Zustimmungshäufigkeit als Tiebreaker. Ausgabe: gerankte Top-K mit
   Punkten und Stimmen.

## 3. Oberfläche

- **REPL:** `/autoresearch.self [N] [--top K]` — nutzt den Pool, Default N=10,
  K=10. Live-Fortschritt: `[self-research] sammeln 3/8 …`, `konsolidieren via
  <brain> …`, `abstimmen 5/8 …`, dann die Rangliste.
- **CLI:** `webagent autoresearch-self [--suggestions N] [--top K] [--headless]
  [--facts <datei>]`.
- **Ergebnis** wird zusätzlich als Wiki-Seite `self-research-<stamp>` unter
  data/memory/wiki/ abgelegt (Dogfooding der Wiki-Memory).

## 4. Projektfakten (Phase-1-Kontext)

Da Brains keine URLs laden: die Fakten kommen aus dem Repo selbst, gebündelt
und auf ~1200 Zeichen begrenzt: erste Zeilen von README.md + neuester
PROGRESS.md-Abschnitt + Modulliste (`src/*.rs` mit Zeilenzahl). Optional per
`--facts <datei>` überschreibbar. So bewertet der Schwarm den AKTUELLEN Stand.

## 5. Modul & Wiederverwendung

- Neues Modul `src/self_research.rs` (Kernlogik + reine, testbare Helfer:
  `parse_vote_line(&str, catalog_len) -> Vec<usize>`, `tally(votes, k) ->
  Vec<(usize, u32, u32)>`, `dedupe_pool(lines) -> Vec<String>`,
  `build_facts(...) -> String`).
- Der Browser-Teil (per-Brain-Relay in isoliertem Profil, Orchestrator-Auswahl)
  wird aus `repl.rs::ReplSession::swarm_query` / `run_swarm` wiederverwendet —
  nicht duplizieren; ggf. gemeinsame Helfer extrahieren.

## 6. Tests

Reine Funktionen voll unit-getestet: parse_vote_line (Ziffern, Duplikate,
Out-of-Range, Prosa drumherum, leere Antwort), tally (Borda + Tiebreaker,
Gleichstand), dedupe_pool (exakte Duplikate, Whitespace/Case-Normalisierung).
Kein Live-Brain im Test (der Orchestrator prüft e2e).

## 7. Außerhalb v1

- Semantische statt exakter Dedup in Phase 2 ohne LLM (später).
- Automatisches Umsetzen der Top-K (das ist der ANDERE autoresearch-Loop).
- Mehrrunden-Konvergenz.
