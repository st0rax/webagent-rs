<!-- **Referenz: Beleg der T-947-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-947 Handoff - Relay sendet denselben Prompt nicht mehr bis zu dreimal

- Task: T-947
- Owner: local/opencode
- Branch: `fix/T-947-relay-single-send` (gepusht)
- Claim: `origin/master` `9e0735d` (Status `claimed`, owner `local/opencode`)
- Voraussetzung: T-945 (erfuellt, gemergt in `beb3e69`)
- Beweise: `docs/proofs/T-947/gates.txt`
- Scope laut Board: `src/relay.rs`, `src/browser/backend.rs`, `docs/API_BRIDGE.md`.
  `src/browser/backend.rs` blieb unveraendert (siehe Grenzen).

## Ziel

`relay_single_turn_with_attachments_streaming` (`src/relay.rs`) fuhr je Anfrage
ohne Anhaenge bis zu `MAX_TURNS = 3` volle Turns: je `new_chat`, erneutes Senden
des kompletten Prompts und `wait_response` mit dem **vollen** Zeitbudget. Endete
ein Turn mit leerem Text (`timeout_no_message`), folgte der naechste. Folgen am
2026-09-14: mistral-Anfragen liefen 856-868 s statt 270 s (dreimal das Budget),
obwohl das Senden jeweils Erfolg meldete - derselbe grosse Prompt ging bis zu
15 Mal in 15 neuen Konversationen an den Anbieter. Zusammen mit Pis drei
Wiederholungen je Anfrage bis zu neun Sendungen pro Client-Anfrage.

## Vorher -> nachher

| Aspekt | vorher | nachher |
|---|---|---|
| Leerer Turn nach **erfolgreichem** Senden (`timeout_no_message`) | naechster Turn: `new_chat` + Prompt erneut + Warten | Schleife endet, **kein** erneutes Senden |
| Zeitbudget | je Turn (bis 3x `wait_timeout`) | je **Anfrage** (Deadline `started + wait_timeout`, einmal) |
| Fehlermeldung | `timeout_budget=270s` (Client wartete 856 s) | nennt `gesamt=<tatsaechlich>s` und `sendungen=<n>` |
| Brain-Score | keine Sendungszahl | `sends` je Ereignis in `events.jsonl` |
| Sende-bedingtes Wiederholen | immer (auch nach erfolgreichem Senden) | nur wenn **nicht** gesendet wurde |

## Was sich aendert

1. `src/relay.rs`: Die Inline-Schleife wurde in `run_turn_loop<B: RelayBackend>`
   ausgelagert (generic, damit ein Mock-Backend testbar ist). `sends: u32` zaehlt
   erfolgreiche Sendungen. `deadline = started + wait_timeout` gilt ueber alle
   Turns; `turn_wait = remaining.min(wait_timeout)`. Text leer **nach
   erfolgreichem** Senden -> `break` (nicht `continue`).
2. Neues privates Trait `RelayBackend` (+ `impl` fuer `WebBrainBackend`,
   delegiert nur). Methodennamen mit `relay_`-Praefix, weil `WebBrainBackend`
   bereits `BrainBackend` implementiert und gleichnamige Methoden am konkreten
   Typ `E0034` (mehrdeutiger Aufruf) ausgeloest haetten.
3. `src/brain_score.rs`: `Event` bekommt `sends: Option<u32>`
   (`serde(default, skip_serializing_if = "Option::is_none")`), neuer
   `record_event_with_sends(..., sends: u32)`; `record_event_at` nimmt `sends`
   als Parameter. Bestehende Ein-Turn-Aufrufer schreiben weiter ohne Feld.
4. `docs/API_BRIDGE.md` (Zeile ~348): `--timeout-secs` dokumentiert jetzt als
   Budget der ganzen Anfrage (ueber alle Wiederholungs-Turns), plus Hinweis, dass
   ein Turn nach erfolgreichem Senden ohne Nachricht nicht wiederholt wird.

Unveraendert: Wiederholung bleibt, wenn **nachweislich nichts gesendet** wurde
(`new_chat`-Fehler, Composer-Fehler, nicht-deterministische Sendefehler) und bei
einer Anbieter-Fehlerseite statt Antwort. Rate-Limit/Blocked bleiben terminal.
Anhaenge bleiben bei einem Versuch ein Versuch (Upload-Fehler deterministisch).

## DoD (geprueft)

- Stummer Anbieter erhaelt genau eine Sendung:
  `relay::tests::stummer_anbieter_erhaelt_genau_eine_sendung` (Mock-Backend
  `StummerAnbieter`: Senden zaehlt, wartet nie eine Antwort; Test prueft
  `sends == 1` und dass die Fehlermeldung `sendungen=1` nennt).
- Wiederholung nur ohne Absenden: `relay::tests::sendefehler_ohne_absenden_wird_weiter_versucht`
  (Mock `Sendefehler`: Senden schlaegt fehl, `attempts > 0`, kein Warten).
- Brain-Score protokolliert `sends`:
  `brain_score::tests::sends_werden_pro_ereignis_geschrieben` (Some(3)) und
  `brain_score::tests::sends_fehlen_bei_ein_turn_ereignissen` (None).
- Gesamtdauer in der Fehlermeldung: der Leer-Turn-Zweig formatiert
  `gesamt={started.elapsed()}`; damit stimmt die genannte Zeit mit der
  tatsaechlichen Wartezeit ueberein (frueher stand dort nur das Turn-Budget).

## Grenzen (ehrlich)

- **Keine Live-Messung**: geprueft ist das Verhalten der Schleife mit
  Mock-Backends, nicht gegen einen echten Anbieter (gleiche Einschraenkung wie
  T-936/T-937/T-938/T-939/T-943/T-945). Kein echter Browser/CDP-Lauf.
- `src/browser/backend.rs` blieb unveraendert: die Ursache lag in `relay.rs`
  (Schleife), nicht in `timeout_no_message`. Der Scope-Eintrag deckt den
  Nachbarpfad ab, war aber nicht noetig. Der `backend_status` fliesst in die
  Fehlermeldung ein.
- `MAX_TURNS = 3` bleibt als Obergrenze bestehen; reduziert wurde die **Zahl der
  Sendungen** (kein Retry nach erfolgreichem Senden) und das **Budget** (einmal
  je Anfrage), nicht die Zahl der Versuche bei fehlgeschlagenem Senden.
- Der Attachment-Pfad war schon vorher auf einen Versuch begrenzt; hier nur
  unveraendert mitgetragen.
- Pi-seitige Wiederholungen (bis zu drei je Anfrage) sind ausserhalb dieser
  Aenderung; T-947 senkt nur die Sendungen **pro** Bridge-Anfrage.

## Branch und Commit

- Branch: `fix/T-947-relay-single-send` (Basis `origin/master` `9e0735d`)
- Commit: siehe `git log` auf dem Branch (`T-947: Relay sendet stummen Anbieter
  nur einmal an`)
