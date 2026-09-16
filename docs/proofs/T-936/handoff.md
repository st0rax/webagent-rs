<!-- **Referenz: Beleg der T-936-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-936 Handoff — Sendepfad: Phase und Beobachtung je Turn protokollieren

- Task: T-936
- Owner: local/opencode
- Branch: `refactor/T-936-sendphase-log` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `dddeae0` (Status `claimed`, owner `local/opencode`)
- Beweise: `docs/proofs/T-936/gates.txt`
- Scope laut Board: `src/browser/send.rs`, `src/browser/composer.rs`,
  `src/brain_score.rs`. Ausnahme (verdrahtet unten): `src/browser/verify.rs` nur
  Test-Harness (ein Helper-String), keine Produktionslogik.

## Ziel

Fehlgeschlagene Sende-Turns waren nur ueber Sammelstrings wie
"Composer-Feld nicht gefunden" oder "blockiert" unterscheidbar; die reichen
Metriken (getBoundingClientRect-Geometrie, Clamp-Griff, Fokus, eingefuegte
Zeichen) wurden jedes Mal berechnet und verworfen. Es gibt bisher nur eine
Bannertext-Phrase im reason, aber ohne Bannertext.

## Was sich aendert

Beobachtungen fliessen ueber einen Side-Channel-Puffer `PENDING_TURN` in
`brain_score.rs` statt neuer Felder auf `WebBrainBackend` (browser/mod.rs
bleibt out of scope; relay-Signatur unveraendert). `record_event`/`record_event_at`
konsumiert den Puffer beim Schreiben in die events.jsonl und legt ihn anschliessend
leer — ein Folgeturn ohne neuen Pending bekommt kein Stale-Objekt angehaengt.

Pro Turn entsteht jetzt ein `turn`-Objekt in events.jsonl:

    {"phase":"insert|content_check|submit|settle|read|...",
     "selector_index": Option<u32>,
     "element_w"/"element_h": Option<f64>,
     "clamp_triggered": bool (default false),
     "focus_arrived": Option<bool>,
     "pasted_chars"/"expected_chars": Option<usize>,
     "banner": Option<String>}

Alle optionalen Felder sind serde-weggelassen wenn abwesend; `clamp_triggered`
hat `serde(default)` (kritisch fuers Ruecklesen alter/neuer Events ohne das Feld).

### send.rs

- `send_with_attachments` (Einstieg jedes Turns, vor Anhaengen): Puffer frisch
  (`TurnObservation::default()`).
- `send_generic`: Phase wird an jedem Checkpoint gesetzt (SelectorResolve,
  Focus, ContentCheck nach dem Fuell-Nachweis, Submit vor der Klickschleife,
  Settle bei bewiesenem Absenden). Bei `!filled` unterscheidet die
  Beobachtung ContentCheck (Elementmasse vorhanden → Klick daneben / Inhalt nie
  akzeptiert) von Insert (keine Masse → Selektor traf kein Element); der alte
  Substring "Composer-Feld nicht gefunden (Timeout)" bleibt im Detail erhalten.
  Mit `composer_char_count()` vs. `text.chars().count()` kommen pasted/expected.
- `submit_failed_error`: Fehlermeldungen unter `send_phase=submit:`-Praefix
  (Prae-Listen/Externes via contains() laufen weiter), pasted_chars aktualisiert,
  und im Banner-Zweig landet `turn_banner`-gekuerzter Bannertext (max 200 Zeichen,
  ohne PII — z.B. zai-Login-Cloudflare-Banner statt "blocked" ohne Kontext).
- `send_qwen`-Fill-Fehler ebenso phase-annotiert (ContentCheck). send_gemini
  durchlaeuft ohnehin `submit_failed_error`.

### composer.rs

- `note_composer_metrics`: der bisher weggeworfene Rects/Clamp/Index landet in
  der Turn-Beobachtung (SelectorResolve via `i`, `w`/`h`, `clamp`).
- `fill_composer_rich_multiline`, `fill_composer_dom_set`, `fill_composer`:
  `coord_body` liefert jetzt `{x,y,w,h,clamp,i}` statt nur `{x,y}` und ruft
  note_composer_metrics auf, sobald x/y vorhanden sind.
- `fill_composer`: zusaetzlich Fokus-Sonde (`activeElement`/`:focus`) nach
  Klick+focus() — als Observation (`focus_arrived`), nicht als Abbruchkriterium
  (die Korrektur ist T-937).

### brain_score.rs

- Neu: `SendPhase`-Enum (snake_case), `TurnObservation`, `PENDING_TURN`-Puffer,
  `set_pending_turn`/`update_pending_turn`/`pending_turn_snapshot`,
  `phase_error(phase, detail)` ("send_phase=insert: …"),
  `turn_banner(banner)` (trim + take(200)), `BANNER_TRUNCATE=200`.
- `Event` um `#[serde(default, skip_serializing_if="Option::is_none")]
  turn: Option<TurnObservation>` erweitert — alte events.jsonl-Zeilen ohne
  `turn` bleiben lesbar.
- `record_event_at` konsumiert `PENDING_TURN` vor dem Serialisieren.

## Fuenf-Zustaende in den Daten unterscheidbar (DoD)

| Zustand | Datenmuster |
|---|---|
| nicht angemeldet | Phase Insert/ContentCheck + `banner: Some(_)` (Login-Banner) |
| ausgeloggt mit Reauth | Phase frueh (SelectorResolve/Focus) + `banner: Some(_)` |
| kein Selektor getroffen | Phase insert, `element_w/h: None`, `pasted=0 != expected` |
| Klick neben dem Element | Phase content_check, `element_w/h: Some`, `pasted=0 != expected`, evtl. `clamp_triggered` |
| Fuellen in Deadline gelaufen | Phase insert/content_check, `pasted < expected` |

Test `turn_fuenf_zustaende_sind_in_den_daten_unterscheidbar` beweist die
paarweise Unterschiedlichkeit ueber die Signatur (Phase, Feld-Anwesenheit,
pasted==expected, banner). `record_event_at_konsumiert_pending_turn` beweist
Konsum + "kein Stale im Folgeturn".

## Messwerte (Handoff-Anforderung: vorher/nachher)

- Vorher: ein reason-String pro Fehlschlag ("Composer-Feld nicht gefunden
  (Timeout)" / "blockiert: …"); keine Phase, keine Metriken, kein Bannertext im
  Record. 4 Zustaende waren in den Daten NICHT unterscheidbar (nicht
  angemeldet vs. ausgeloggt teils, "Klick daneben" vs. "Deadline" gar nicht).
- Nachher: 1433/0/1 Lib-Tests (war 1429), 2 neue Guarantee-Tests, 2 neue
  Format-Tests; jeder Turn traegt Phase+Beobachtungen; Banner max. 200 Zeichen
  und ohne PII; alle alten Substrings und Marker (SEND_DISABLED_MARKER,
  "Absenden fehlgeschlagen", "blockiert: kein Absende-Beweis") bleiben
  fuer externe Erkennung erhalten.

## Ausnahme vom Scope

- `src/browser/verify.rs` (nur `composer_coords_expr`, Test-Helper):
  coord_body liefert jetzt w/h/clamp/i; das Mock matcht eval-Strings exakt und
  registrierte den alten Koordinaten-Ausdruck. Ohne Sync auf den neuen
  Production-Ausdruck wueren 5 Sende-/Verify-Tests an einem unregistrierten
  eval scheitern. Produktionslogik in verify.rs: unveraendert.

## Nicht umgesetzt (bewusst, laut Board Non-Goals)

- Kein Umbau des Sendeverfahrens; kein neues Feld auf `WebBrainBackend`
  (browser/mod.rs, relay.rs, backend.rs bleiben unveraendert); keine neue
  Abhaengigkeit.