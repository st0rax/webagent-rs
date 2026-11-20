**Referenz:** Echtes Live-Ergebnis des Kandidaten `0039cd4` (`fix/T-961-tool-availability`), gemessen am 20./21.11.2026. **Negativbeleg — kein Abnahmebeleg.** Der Code des Kandidaten wurde deshalb zurückgenommen; T-961 bleibt `free`.

# T-961 — Kandidat „neutrale Verfügbarkeitsregel" ist gescheitert

## Hypothese

Der Browser-Prompt sollte zusätzlich zum Tool-Schema eine kurze, neutrale Regel enthalten: die aufgeführten Werkzeuge seien aktiv und ausführbar, ein bereits geliefertes Ergebnis bedeute nicht, dass das Werkzeug aufgebraucht sei, weitere Aufrufe verwendeten dasselbe Werkzeug. Bewusst ohne Provider-, Bridge- oder Konto-Meta-Aussagen, weil eine ältere Formulierung (`2b576ed`) auf anderen Brains Meta-Antworten auslöste.

Red-Test: `tool_availability_is_stated_without_provider_meta`. Gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1467 passed). Binary-SHA256 `8B902256F166F36B610355C0E67791D1326E653BE698DA3359921D419CBBB6AA`.

## Messung

Acht aufeinanderfolgende echte Pi-1.5.2-Läufe, identische Prompts, Port 8788, frische Prozess-Tokens, frisches Temp-Verzeichnis pro Lauf. Rohtranskripte: `availability-negative-{1..8}.jsonl`. Der unveränderte strenge Verifier liegt in `availability-negative-verify.txt`.

| Lauf | strict | Tool-Starts | Marker | Befund |
|---:|:---:|---:|---|---|
| 1 | ja | 5/5 | 1–5 | grün |
| 2 | nein | 3/5 | fehlen 1–5 | *„Die lokalen read-Werkzeugaufrufe 4 und 5 sind in dieser Sitzung nicht verfügbar."* |
| 3 | nein | 3/5 | fehlen 4–5 | *„… da das lokale read-Werkzeug in dieser Sitzung nicht verfügbar ist."* |
| 4 | ja | 5/5 | 1–5 | grün |
| 5 | nein | 0/5 | fehlen 1–5 | *„Das angeforderte lokale read-Werkzeug ist in dieser Sitzung nicht ausführbar."* |
| 6 | nein | 3/5 | fehlen 4–5 | nach drei erfolgreichen Reads ohne weitere Werkzeugaufrufe beendet |
| 7 | nein | 2/5 | fehlen 1–5 | `ABSENDEKNOPF_DEAKTIVIERT`, danach `circuit_open` (5 Treffer) |
| 8 | nein | 0/5 | fehlen 1–5 | *„Das angeforderte read-Werkzeug ist in dieser Sitzung nicht direkt ausführbar."* |

Ergebnis: **2/8 grün**. Zum Vergleich mit dem unveränderten `master`-Binary `eaed3d1` (ohne die neue Regel): 3/4 grün, ein Negativlauf mit derselben Fehlerklasse. Die Euphemismen „aktiv und ausführbar" provozieren zusätzliche Meta-Antworten über Verfügbarkeit — teils sogar vor dem ersten Werkzeugaufruf. Das ist das Gegenteil der erhofften Wirkung.

## Konsequenz

1. Der Kandidat wird **nicht** gemergt. Die Code-Änderung wurde auf dem Branch zurückgenommen (`git revert`); der Branch enthält nur noch die ehrliche Belegdokumentation.
2. Die Intermittenz ist nicht gelöst. Die Ursache bleibt ungeklärt; der Kontozustand ist geprüft und erklärt sie nicht (`account-state.md`).
3. **T-961 bleibt `free`.** Kein PASS, kein `done`, kein Tag/Release (G-009 bleibt offen).
4. Alle acht Rohtranskripte sind secret-gescannt: 0 Treffer für `Bearer`, `apiKey`, `api_key`, `cookie`, `authorization`, `sk-`, `WEBAGENT_API_KEY`. Hex-Zeichenfolgen sind die absichtlichen `PI_TOOL_*`-Nonces und Temp-GUIDs.
