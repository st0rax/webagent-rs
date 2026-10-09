<!-- **Referenz: Beleg der T-943-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-943 Handoff - Gekuerzte Eingabe wird als Erfolg gemeldet

- Task: T-943
- Owner: local/opencode
- Branch: `refactor/T-943-composer-fulltext-check` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `a6693dc` (Status `claimed`, owner `local/opencode`)
- Voraussetzung: T-939 (gemergt, `b85d70e`) - `composer_matches_text` ist der
  Standard-Fill-Nachweis.
- Beweise: `docs/proofs/T-943/gates.txt`
- Scope laut Board: `src/browser/composer.rs`, `src/browser/send.rs`,
  `src/brain_limits.rs`. Ausnahme: `src/browser/verify.rs` (nur Test-Mocks
  angepasst, siehe unten). `composer.rs`/`brain_limits.rs` blieben unveraendert.

## Ziel

Am 2026-09-14 meldete gemini bei einer 52.287-Zeichen-Eingabe Erfolg
(HTTP 200, 26-28 s), lieferte aber eine Rueckfrage zum abgeschnittenen
Tabellenformat statt der Aufgabe: die Oberflaeche hatte die Eingabe still
gekuerzt. `composer_matches_text` (T-939) prueft zwar den ganzen Editorinhalt,
aber nur **Gleichheit** gegen den gewuenschten Text - ein Fuellvorgang, der
den Text erst gar nicht komplett hineinbekommt, wurde trotzdem als Erfolg
verbucht, weil der Fill-Verifier nur den (bereits gekuerzten) Ist-Zustand sah
bzw. der Pfad ohne Consumed-Beweis weiterlief. Aus einem gekuerzten Auftrag
darf kein HTTP 200 werden.

## Was sich aendert

Ein einziger Helfer in `src/browser/send.rs` (`WebBrainBackend`-impl):

```
fn ensure_composer_full(&self, text: &str) -> Result<(), String>
```

- misst die Zeichenzahl des tatsaechlichen Composerinhalts
  (`composer_char_count`, `el.value` bzw. `el.innerText`);
- schreibt die Messung als `SendPhase::ContentCheck`-Observation
  (`pasted_chars` / `expected_chars`) via `update_pending_turn`;
- erlaubt 10% Abweichung (Whitespace-Normalisierung in
  contenteditable/ProseMirror) und meldet sonst einen **benannten**
  `ContentCheck`-Fehler statt `Ok`.

Eingebunden nach dem erfolgreichen Fuellen in allen drei Sende-Pfaden:

| Pfad | Ort |
|---|---|
| `send_generic` | nach `wait_fill_composer`-Kette, vor Submit-Phase |
| `send_gemini` | nach dem Disabled-Fallback, vor `url_before` |
| `send_qwen`  | nach dem `wait_fill_composer`-Ketten-Fallback, vor dem Sleep |

Die fruehere Duplikation (dieselbe Inline-Pruefung dreimal) ist damit ersetzt;
`send_gemini` hatte sie vorher noch gar nicht.

## DoD (geprueft)

- Nach dem Fuellen wird geprueft, dass der **vollstaendige** Text im Composer
  steht: `ensure_composer_full` vergleicht Ist- und Soll-Zeichenzahl in allen
  drei Sende-Pfaden.
- Eine unvollstaendige Befuellung fuehrt zu einem benannten Fehler statt zu
  HTTP 200: Test `gekuerzte_eingabe_ist_contentcheck_fehler` (200 von 1000
  Zeichen -> `ContentCheck`-Fehler, Text enthaelt "gekuerzt"). Kein
  falscher Alarm im Normalfall:
  `vollstaendige_eingabe_passiert_contentcheck`.
- Gemessene Eingabegrenze je Anbieter im `proof_path`: siehe
  "Gemessene Grenzen" unten - ehrlich als **nicht neu gemessen** markiert.

## Gemessene Grenzen (ehrlich)

In dieser Umgebung sind keine echten Sende-Durchlaeufe gegen die
Provider-Webseiten moeglich (gleiche Einschraenkung wie T-936/T-937/T-938/
T-939). Die einzige belastbare Messung stammt aus dem Board-Objective von
T-943 und wird hier nur zitiert, nicht neu gemessen:

| Anbieter | Eingabegrenze (gemessen) | Quelle | Datum |
|---|---|---|---|
| gemini | ~32.000-32.400 Zeichen (Eingabe 52.287 -> gekuerzt) | Board T-943 objective | 2026-09-14 |
| uebrige Anbieter | unbekannt | - | - |

Die Pruefung ist damit ein **nachgelagerter Ist-Soll-Vergleich** und keine
Vorab-Kappung: der Code kennt keine feste Grenze und baut keine ein (Board-
non_goal "Keine geschaetzten Grenzen fest einbauen; nur gemessene Werte
verwenden"). Der 10%-Puffer faengt Normalisierung, nicht Werte knapp unter
einer Anbietergrenze; fuer eine exakte Grenze je Anbieter braeuchte es
Live-Messung mit steigender Laenge.

## Grenzen (technisch)

- Erkennung nur, wenn der Anbieter schon **beim Fuellen/Anzeigen** kuerzt.
  Rechnet/kuerzt der Server erst spaeter, bleibt es ein Consumed/Verify-send-
  Fall (kein Composerbefund) - diese Task deckt nur den Composerzustand.
- `composer_char_count` zaehlt Unicode-Zeichen ueber JS `.length`
  (UTF-16-Codeunits), der Vergleich nutzt Rust `chars().count()` (Unicode-
  Scalarwerte). Fuer BMP-Text identisch; bei vielen Emoji/Astralzeichen kann
  `.length` hoeher liegen als `chars()` - das faellt in den 10%-Puffer bzw.
  in die gefaehrliche Richtung (Ist groesser), blockiert also nicht.
- `src/brain_limits.rs` blieb unberuehrt (keine gemessenen Grenzen vorhanden,
  die haetten hinterlegt werden koennen).
- `src/browser/verify.rs` nur Mock-Anpassung: die 5 Sende-Fixtures
  beantworten jetzt den `composer_char_count`-Ausdruck, sonst schlaegt die
  neue Pruefung faelschlich an.
