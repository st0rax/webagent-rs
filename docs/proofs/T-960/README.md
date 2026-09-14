# T-960: Fehlerseite des Anbieters ist keine Antwort

**Referenz:** Nachweis zu T-960, Stand 2026-09-14. Verbindlich sind START_HERE.md und docs/TASKBOARD.json.

## Anlass

Pi-Session ueber `webagent/auto`, 18:54: chatgpt lieferte als Antworttext
"Something went wrong. If this issue persists please contact us through our
help center at help.openai.com. Erneut versuchen". Die Bridge gab das mit
HTTP 200 aus, brain_score verbuchte Erfolg.

## Aenderung

`relay.rs` prueft nach der Leer-Pruefung mit `is_provider_error_page`, das die
vorhandene Erkennung `brain::is_retryable_empty_response` nutzt. Treffer gelten
wie eine leere Antwort mit dem Grund "Anbieter-Fehlerseite statt Antwort".
Ausgenommen sind Texte mit `WEBAGENT_INFERENCE/1` und Texte ueber 400 Zeichen.

## Beleg

- `relay::tests::provider_error_page_is_not_an_answer`: der reale Text wird
  erkannt, eine normale Antwort nicht.
- `relay::tests::tool_envelope_and_long_answers_with_banner_phrases_stay_answers`:
  ein Umschlag mit `grep 'something went wrong'` und eine lange Antwort mit
  "usage limit" bleiben Antworten.
- `cargo test --lib`: 1425 bestanden.

## Grenzen

- Kein Live-Beleg: Die Fehlerseite laesst sich nicht gezielt ausloesen.
- Kurze echte Antworten unter 400 Zeichen, die eine Anbieter-Phrase wie
  "too many requests" enthalten, werden verworfen und wiederholt.
