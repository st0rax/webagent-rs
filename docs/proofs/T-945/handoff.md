<!-- **Referenz: Beleg der T-945-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-945 Handoff - Offener Circuit-Breaker als 503 mit Retry-After

- Task: T-945
- Owner: local/opencode
- Branch: `fix/T-945-circuit-open-503` (gepusht)
- Claim: `origin/master` `5d04ded` (Status `claimed`, owner `local/opencode`)
- Voraussetzung: keine (`depends_on: []`)
- Beweise: `docs/proofs/T-945/gates.txt`
- Scope laut Board: `src/api_bridge/provider_handlers.rs`, `src/api_bridge/boundary.rs`,
  `src/api_bridge/wire.rs`, `src/api_bridge.rs`, `src/relay.rs`. Alle fuenf beruehrt.

## Ziel

Ein offener Circuit-Breaker (`circuit_open: <brain> uebersprungen, noch Ns Cooldown`,
`src/relay.rs`) wurde von der Bridge als HTTP 502 ohne Wartehinweis gemeldet.
Clients (Pi) werten 502 als voruebergehend und wiederholen sofort — drei
Wiederholungen je Anfrage. Am 2026-09-14 verwandelten diese Wiederholungen eines
einzigen Aufrufers den Breaker (900 s) in eine Sperre fuer alle Aufrufer. Ein
offener Breaker ist aber ein **nicht verfuegbarer bekannter Anbieter**: 503 mit
`Retry-After` gleich der Restsperre bringt Clients zum Warten.

## Statuscodes / Header vorher -> nachher

| Fehler der Browser-Inferenz | vorher | nachher |
|---|---|---|
| offener Breaker (`circuit_open`, Restsperre N > 0) | `502 Bad Gateway`, kein `Retry-After` | `503 Service Unavailable`, `Retry-After: N` |
| jeder andere Browserfehler (z.B. `timeout_no_text`) | `502 Bad Gateway` | unveraendert `502 Bad Gateway`, kein `Retry-After` |

Betroffene Handler (alle ueber den neuen Helfer):

| Handler | Ort | vorher |
|---|---|---|
| `handle_openai` (chat/completions) | `provider_handlers.rs` (Board: Zeile 80) | `api_error(_, 502, &error)` |
| `handle_responses` (responses, non-stream) | `provider_handlers.rs` (Board: Zeile 335) | `api_error(_, 502, &error)` |
| `handle_anthropic` (messages) | `provider_handlers.rs` | `api_error(_, 502, &error)` |
| `handle_image_generation` | `api_bridge.rs` | `api_error(_, 502, &error)` |
| `handle_audio_transcription` | `api_bridge.rs` | `api_error(_, 502, &error)` |

## Was sich aendert

1. `HttpResponse` (`api_bridge.rs`) hat ein Feld `retry_after_secs: Option<i64>`
   und die Methode `with_retry_after` (hebt Werte < 1 auf 1 an; `Retry-After: 0`
   waere ein sofortiger Wiederholungsauftrag und liefe dem Breaker zuwider).
2. `render_http_response` (`wire.rs`) schreibt `Retry-After: N\r\n` **nur** wenn
   gesetzt; sonst bleibt der Headerblock byte-gleich (die leere Einfuegung
   erzeugt nur das abschliessende CRLF).
3. `relay.rs`: Erzeuger und Parser teilen sich den Praefix
   (`CIRCUIT_OPEN_PREFIX`): `circuit_open_message(brain, secs)` baut die Meldung,
   `open_circuit_remaining_secs(error)` liest die Restsperre zurueck.
4. `boundary.rs`: neuer Helfer `browser_inference_error(flavor, message)` —
   `open_circuit_remaining_secs` Treffer -> `503` + `with_retry_after`, sonst
   `502`. Der Originalgrund bleibt im Fehlerkoerper (Diagnose).

Bewusst unberuehrt: `overload_response()` (Verbindungslimit, bereits 503),
`model_not_found`/400er-Faelle, die leere Transkription (`Provider lieferte kein
Transkript.` bleibt 502 — kein Breaker-Fall), `is_audio_capability_refusal`.

## DoD (geprueft)

- `circuit_open` -> 503 mit `Retry-After` in Sekunden: Test
  `api_bridge::wire::tests::offener_breaker_wird_zu_503_mit_retry_after`
  prueft Statuszeile, `Retry-After: 812` und erhaltenen Grund im Koerper.
- Anderer Browserfehler bleibt 502:
  `api_bridge::wire::tests::andere_browserfehler_bleiben_502_ohne_retry_after`.
- Zusaetzlich: Parser/Erzeuger-Roundtrip und Fremdmeldungen
  (`relay::tests::circuit_open_meldung_und_parser_passen_zusammen`,
  `relay::tests::parser_ignoriert_fremde_und_kaputte_meldungen`) sowie
  Headervertrag ohne Wartezeit
  (`api_bridge::wire::tests::json_ohne_retry_after_haelt_den_headervertrag`).

## Grenzen (ehrlich)

- **Inkrementeller Responses-SSE-Pfad** (`handle_responses_incremental`) bleibt
  aussen vor: er schreibt die `200`-SSE-Header, bevor `run_task_streaming` den
  Breaker prueft. Ein `circuit_open` dort erscheint weiter als
  `response.failed`-Event im offenen Stream — nach gesendeten Headern ist kein
  503 mehr moeglich. Die DoD betrifft die drei gepufferten Handler (Board nennt
  Zeile 80 und 335). Als Folge-Task-Kandidat notiert.
- Keine Aenderung an Breaker-Schwellen oder -Sperrdauern (non_goal). Ob ein
  Client-Retry als getrennter Fehlschlag zaehlt, bleibt unveraendert; die
  Bridge zaehlt weiterhin nur ihre eigenen Browser-Fehlschlaege.
- **Offene Frage aus dem Objective** ("ob Wiederholungen derselben Client-Anfrage
  als getrennte Fehlschlaege fuer den Breaker zaehlen sollen"): hier **nicht**
  entschieden — das braucht Zaehl-Schluessel (Request-Id) und ist eine eigene
  Aenderung, nicht Statusklasse/Header.
- Keine Live-Messung moeglich: geprueft ist die Drahtausgabe durch Unit-Tests
  von `render_http_response`/`browser_inference_error`; kein echter Pi-Client in
  dieser Umgebung (gleiche Einschraenkung wie T-936/T-937/T-938/T-939/T-943).

## Branch und Commit

- Branch: `fix/T-945-circuit-open-503` (Basis `origin/master` `5d04ded`)
- Commit: siehe `git log` auf dem Branch (`T-945: circuit_open als 503 + Retry-After`)
