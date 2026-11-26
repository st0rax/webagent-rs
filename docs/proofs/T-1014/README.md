# T-1014 — Gemini-Prompt-Eingabe: gemessene Grenze und kein Zeichen-Nachtippen

> **Referenz.** Task proof für T-1014. Er dokumentiert die gemessene
> Gemini-Eingabegrenze und die Code-Änderung; er schaltet nichts live.

- Task: T-1014
- Owner: local/opencode
- Branch: `fix/T-1014-gemini-prompt-input`
- Status: umgesetzt und geprüft, auf `master` gemergt (`a6c8b97`); Gates siehe `gates.txt`.

## Befund (live, 2026-11-26)

`C:\Users\storax\AppData\Local\webagent\data\brain_score\events.jsonl`:

| Zeit (UTC) | Brain | Prompt-Zeichen | Dauer | `pasted_chars` | `sends` | Ergebnis |
|---|---|---:|---:|---:|---:|---|
| 17:03:10 | gemini | 38.561 | 283,6 s | 0 | 0 | `client_disconnected` |
| 17:04:19 | gemini | 38.571 | 69,2 s | 0 | 0 | `client_disconnected` |
| 17:48:12 | gemini | 150.325 | 667,2 s | 0 | 0 | `client_disconnected` |
| 17:57:10 | gemini | 43.609 | 356,3 s | 0 | 0 | `client_disconnected` |
| 17:58:06 | gemini | 43.609 | 56,8 s | 0 | 0 | `page-protocol: WebView-Tab beendet` |
| 17:59:49 | gemini | 43.609 | 93,4 s | 0 | 0 | `page-protocol: WebView-Tab beendet` |

Zusätzlich meldet `events.jsonl` um 18:09:04 UTC einen chatgpt-Turn mit
45.567 Zeichen, `pasted_chars=45.399`, `sends=0` und nach 51 s
`send_phase=content_check: Composer-Feld nicht gefunden (Timeout)`. Der Prompt
war also fast vollständig im Editor; der strikte Gleichheitsvergleich hat die
Rich-Editor-Normalisierung als Fehler gewertet.

Erfolgreiche Gemini-Turns desselben Tages: maximal 26.523 Zeichen. Die
Composer-Grenze wurde bereits in `docs/proofs/T-943/handoff.md` bei ~32.000
Zeichen gemessen. Es ist also die **Eingabegrenze der Web-Oberfläche**, nicht
das Modell-Kontextfenster (das der Katalog mit 1M bewirbt).

Die OpenCode-Testsitzung (`session_v2`, Verzeichnis `.../webagent-rs`) zeigt
zeitgleich Turns mit `circuit_open` (chatgpt, 391 s Cooldown) und
`auto_attach_timeout: routed=claude` — Folge der langen, erfolglosen Turns.

## Ursache im Code

`crates/webagent-transport/src/browser/send.rs`, `send_gemini`: Scheiterte der
chunk-basierte `insert_text`-Weg, folgte als Fallback
`fill_composer_dom_set` + `type_text_char_by_char(text)`. Letzteres ist **ein
PageDriver-Aufruf pro Zeichen**. Bei 43.609 Zeichen sind das über 43.000
einzelne Round-Trips; der Client trennt die Verbindung, bevor der Composer auch
nur gefüllt ist. Ein zweiter gleicher Durchlauf lief nach
`send_button_disabled() == Some(true)`. Das erklärt die gemessenen
69–667 s bei `pasted_chars=0, sends=0`.

## Änderungen

1. **`webagent-core/src/config/limits.rs`** — neue, zentrale und
   env-überschreibbare Werte: `DEFAULT_GEMINI_PROMPT_CHARS = 24_000`
   (Puffer unterhalb der gemessenen ~32k-Grenze), Gemini-Katalogwerte
   `12_000` Kontext / `4_000` Output Tokens, Resolver und Tests. Andere Brains
   behalten ihr Verhalten (`usize::MAX`), weil dort live bis 300k Zeichen
   belegt sind.
2. **`webagent-api/src/relay.rs`** — `relay_turn` prüft die Grenze, **bevor**
   ein Backend gestartet wird. Zu lange Prompts enden sofort mit
   `prompt_zu_lang: gemini nimmt maximal 24000 Zeichen an ...` und werden als
   Event protokolliert. Kein Circuit-Breaker-Fehler: die Oberfläche ist nicht
   schuld. Kein stummes Kürzen des Prompts.
3. **`webagent-api/src/api_bridge/catalog.rs`** — `/v1/models` meldet für
   Gemini die nutzbare Kapazität, damit Pi vor dem Senden kompaktiert statt
   einen Prompt zu bauen, den der Composer nicht annimmt.
4. **`webagent-transport/src/browser/composer.rs` / `send.rs`** — der
   zeichenweise Fallback bleibt nur für Prompts bis 256 Zeichen
   (`SHORT_PROMPT_TYPE_CHARS`); bei langen Prompts wird höchstens erneut
   chunk-basiert gefüllt. `composer_holds_text` akzeptiert bei langen Prompts
   Anfang, Ende und eine Zeichenzahl im 10-%-Band, wenn der strikte Vergleich
   an Rich-Editor-Normalisierung scheitert. Derselbe Nachweis gilt für den
   Consumed-Check beim Senden und für `verify_submitted`, damit ein normalisierter
   Prompt nicht fälschlich als „schon gesendet" gilt. Scheitern beide Füllwege,
   folgt sofort der benannte `content_check`-Fehler statt eines Minuten-Loops.

## Erwartung / Grenze

- Ein 43k-Prompt endet in Millisekunden mit benanntem Fehler statt nach
  356 s mit `pasted_chars=0`.
- Pi kompaktiert bei Gemini deutlich früher; dann liegen die Turns wieder im
  gemessenen Annahmefenster.
- Der laufende Bridge-Prozess (`15908`) trägt weiterhin die ausgelieferte
  `v0.11.8`-EXE. Ein Live-Test der Änderung erfordert einen bewusst freigegebenen
  Neustart mit neu gebauter Binary; er wurde nicht ohne Freigabe ausgeführt.
- Keine Zugangsdaten gelesen, kein Login und kein Live-Provider-Turn gestartet.
