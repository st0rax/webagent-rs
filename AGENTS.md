# Regeln fuer jede opencode-Session in diesem Repo

Diese Regeln existieren, weil Fehler wiederholt auftraten (Stand 2026-08-12). Sie
sind verbindlich, keine Absichtserklaerung.

## 1. Bestand zuerst pruefen, nie parallel bauen

Bevor ein neues Modul, ein neuer Store, eine neue Datei, ein neues CLI-Flag oder
ein neuer Mechanismus entsteht: mit `rg` pruefen, was bereits existiert. Der
Repo-Kern hat fuer viele Dinge schon eine Loesung:

- Beweise/Level-Gate: `capability_proof` (`capability/proofs.jsonl`, `ProofState`,
  Selektor-Hash, TTL). Es gibt **genau einen** Beweis-Gate.
- Messung am Brain: `brain_probe` (`Verdict`), `mock_page`, `page_driver`.
- Zustand/Laeufe: `run_store`, `config`/`data_dir`, `circuit_breaker`.

Beim Portieren aus einem anderen Branch oder Diff (z. B. `4b58dd2.diff`): den
Entwurf ZUERST gegen den aktuellen Main-Stand lesen, nicht den Entwurf als
Spezifikation behandeln. Das Main-Repo entwickelt Designs weiter; ein Referenz-
Diff ist Geschichte, kein Auftrag. Wenn der Diff etwas einfuehrt, das der
Main-Stand bereits erweitert hat, ist der Main-Stand massgeblich.

## 2. Ein einziger Beleg-Gate

Neue Belege werden ausschliesslich via `capability_proof::record_measurement`
(oder `record_proof`) geschrieben und ueber `level_from_selectors_with` vom
Level gelesen. Keinen zweiten Beleg-Speicher anlegen. Route→Faehigkeit ueber
`capability::capability_for_route` aufloesen. Wer einen Beweis-Gate umgeht,
baut ein Versprechen statt eines Koennens.

## 3. Kein stummes Untersuchen

Eine Lesephase muss in einem Edit, einem Testlauf oder einer schriftlichen
Entscheidungsfrage (Kanal `to_claude.jsonl` bzw. Nutzer) enden. Lange stille
`read`-Serien ohne sichtbares Ergebnis gelten als Stillstand. Design-Konflikte
sofort aussprechen, sobald sie auftauchen — nicht stumm verrechnen.

## 4. Sichtbare Checkpoints

Nach jedem abgeschlossenen Stueck: Build/Tests/`clippy` gruen UND eine kurze
Fortschrittszeile (Terminal oder Kanal). Uncommittete Arbeit gehoert ueber
Session-Grenzen in `STATUS_LIVE.md`.

## 5. Commit nur auf Ansage

Niemals committen, pushen oder einen PR bauen, ausser der Nutzer fordert es
ausdruecklich.
