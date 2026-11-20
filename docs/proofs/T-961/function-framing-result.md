**Referenz:** Echtes Live-Ergebnis des Kandidaten `f899ed3` (`fix/T-961-function-framing`), gemessen am 21.11.2026. **Negativbeleg — kein Abnahmebeleg.** Der Code wurde zurückgenommen; T-961 bleibt `free`.

# T-961 — Kandidat „Funktions-Framing" ist gescheitert

## Hypothese

Die Überschrift `[Client-Werkzeuge]` könnte das Browsermodell glauben lassen, die Schemata gehörten einem anderen Client und seien für es selbst nicht aufrufbar. Der Kandidat nannte den Block deshalb `[FUNKTIONEN]` und sprach in der Aufrufanweisung von Funktionen statt von Tools. Bewusst **ohne** jede Aussage über Verfügbarkeit, Ausführbarkeit, Provider, Bridge oder Konto — die vorherige Verfügbarkeitsregel war mit 2/8 grün gescheitert (`availability-candidate-result.md`).

Red-Test: `function_block_has_no_client_or_availability_meta`. Gates grün: fmt, clippy `-D warnings`, 1467 Lib-Tests. Binary-SHA256 `A60E216B61DEABDB04E46581B5111958678682810E753E80848E087328F844C1`.

## Messung

Acht aufeinanderfolgende echte Pi-1.5.2-Läufe, identische Prompts, Port 8788, frische Prozess-Tokens, frisches Temp-Verzeichnis pro Lauf. Rohtranskripte: `function-negative-{1..8}.jsonl`, unveränderter strenger Verifier: `function-negative-verify.txt`.

| Lauf | strict | Tool-Starts | Befund |
|---:|:---:|---:|---|
| 1 | ja | 5/5 | grün |
| 2 | ja | 5/5 | grün |
| 3 | nein | 1/5 | *„Das read-Werkzeug ist in dieser Sitzung nicht verfügbar. Daher kann ich die Aufrufe für nonce-2.txt bis nonce-5.txt nicht ausführen."* |
| 4 | ja | 5/5 | grün |
| 5 | nein | 1/5 | *„… Die vier weiteren Dateien kann ich in dieser Sitzung nicht mit dem angeforderten read-Werkzeug aufrufen."* |
| 6 | ja | 5/5 | grün |
| 7 | ja | 5/5 | grün |
| 8 | ja | 5/5 | grün |

Ergebnis: **6/8 streng grün, aber zwei Läufe mit exakt der T-961-Fehlerklasse.** Das entspricht 75 % und damit der Quote des unveränderten `master`-Binary `eaed3d1` (3/4). Die Umbenennung bringt keine messbare Verbesserung.

## Einordnung

1. Der Kandidat wird **nicht** gemergt; die Code-Änderung wurde auf dem Branch zurückgenommen.
2. Zwei Prompt-Varianten sind jetzt widerlegt: ausdrückliche Verfügbarkeitsregel (2/8) und reine Umbenennung des Tool-Blocks (6/8, keine Verbesserung gegen 3/4 baseline). Die Fehlerklasse tritt unabhängig von diesen Formulierungen auf.
3. Der Kontozustand ist geprüft und erklärt sie nicht (`account-state.md`). Die Ursache bleibt **ungeklärt**; am wahrscheinlichsten ist ein modellseitiges, stochastisches Verhalten der ChatGPT-Weboberfläche bei clientseitig serialisierten Werkzeugaufrufen. Das ist eine Vermutung, kein Beleg.
4. **T-961 bleibt `free`.** Kein PASS, kein `done`, kein Tag/Release (G-009 offen). Weitere Prompt-Wortwahl allein ist keine vielversprechende Richtung; eine echte Lösung bräuchte eine strukturelle Fehlerbehandlung oder ein anderes Brain, beides mit eigener Abnahme.
5. Alle acht Rohtranskripte sind secret-gescannt: 0 Treffer für `Bearer`, `apiKey`, `api_key`, `cookie`, `authorization`, `sk-`, `WEBAGENT_API_KEY`.
