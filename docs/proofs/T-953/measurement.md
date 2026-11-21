**Referenz.** Lokaler Red-/Green-Beleg zu T-953, kein Betrieb und kein Live-Nachweis. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`.

# T-953 — Umschlagfehler: Fehlertyp, brain_score, einmalige Korrektur

Datum: 2026-11-21  
Branch: `fix/T-953-model-output-error`  
Scope: `src/browser_inference.rs`, `src/relay.rs`, `src/api_bridge/provider_handlers.rs`, `docs/API_BRIDGE.md`  
Belegart: lokale Red-/Green-Tests und Mock-Backend ohne Browser/CDP. **Kein Live-Anspruch**, kein Tag/Release.

## Ausgangslage (aus dem Taskboard)

Am 2026-09-14 blieben vier fehlgeschlagene Turns einer Pi-Session im
`brain_score` als Erfolg stehen, weil `relay.rs` die Browserantwort als Erfolg
verbuchte, bevor `browser_inference.rs` den Werkzeugumschlag prüfte. Die
Fehlerantwort war HTTP 502 mit `type: invalid_request_error`, obwohl die
Client-Anfrage gültig war. Eine Korrektur in derselben Konversation gab es
nicht; Pi wiederholte die ganze Anfrage.

## DoD 1 — Fehlerantwort weist die Modellausgabe als Ursache aus

**Vorher:** `api_error()` in `boundary.rs` erzeugte für jeden Inference-Fehler
`{"error":{"type":"invalid_request_error", ...}}` — die Schuld lag demnach beim
Client, nicht am Browsermodell.

**Nachher:** Jeder `parse_response`-Fehler trägt den stabilen Marker
`MODEL_OUTPUT_ERROR:` (Quelle: `src/browser_inference.rs`). Der Marker kann
durch die Auto-Router-Annotation auch mittig im Text stehen; die Bridge prüft
deshalb `contains`. `inference_failure_response()` in
`src/api_bridge/provider_handlers.rs` antwortet dann:

```json
{"error":{"message":"Browser-Inference fehlgeschlagen: MODEL_OUTPUT_ERROR: …","type":"server_error","param":null,"code":"model_output_envelope_invalid"}}
```

Anthropic erhält entsprechend `{"type":"error","error":{"type":"server_error", …}}`.
Alle anderen Fehler behalten die bestehende Übersetzung (502 ohne Marker →
weiterhin `invalid_request_error`; offener Breaker → 503 mit `Retry-After`).
Der Marker gilt auch für den DSML-Protokollfehler aus T-957.

Test: `api_bridge::provider_handlers::tests::modellausgabe_fehler_ist_server_error_mit_stabilem_code`
und `andere_fehler_bleiben_invalid_request_error`; Quelltest
`browser_inference::tests::parse_fehler_tragen_den_modellausgabe_marker`.

## DoD 2 — brain_score verbucht einen Fehlschlag mit eigenem Grund

**Vorher:** `answer = Some(text)` wurde vor der Umschlagprüfung gesetzt; der
Erfolgszweig rief `brain_score::record_event_with_sends(..., true, None, ...)`
auf. Ein kaputter Umschlag erschien als Erfolg.

**Nachher:** `relay::run_turn_loop` prüft die Modellausgabe über die
`OutputCheck::validate`-Closure, **bevor** `answer` gesetzt wird. Der
Fehlerzweig ruft `record_event_with_sends(..., false,
Some(MODEL_OUTPUT_FAILURE_REASON), ...)` mit dem stabilen Grund
`model_output_envelope_invalid`. Der Circuit-Breaker bleibt bewusst unberührt:
das Brain war erreichbar und hat geantwortet, nur die Ausgabe war ungültig.

Tatsächliche Events aus `%TEMP%\webagent_test_data\brain_score\events.jsonl`
(Mock-Brains, Testlauf 2026-11-21):

```json
{"brain_id":"mock_modellausgabe_fehler","success":false,"reason":"model_output_envelope_invalid","latency_ms":9,"prompt_chars":5,"sends":2}
{"brain_id":"mock_nur_einmal","success":false,"reason":"model_output_envelope_invalid","latency_ms":10,"prompt_chars":5,"sends":2}
{"brain_id":"mock_korrektur_erfolg","success":true,"reason":null,"latency_ms":10,"prompt_chars":5,"sends":2}
```

Test: `relay::tests::ungueltiger_umschlag_ist_fehlschlag_mit_eigenem_grund`
liest das letzte Ereignis für sein Mock-Brain und verlangt `success=false`
sowie `reason=model_output_envelope_invalid`. Gegenprobe:
`erfolgreiche_korrektur_wird_als_erfolg_verbucht` verlangt `success=true`
erst nach erfolgreicher Validierung.

## DoD 3 — höchstens eine Korrektur in derselben Konversation

`browser_inference::complete_streaming_with_attachments` übergibt einen
`OutputCheck` an den neuen geprüften Relay-Pfad. Dessen `validate` verwendet
dieselbe Normalisierung und denselben Parser wie der finale Aufruf; die
Korrektur enthält die Parserfehlermeldung und verlangt einen gültigen
`WEBAGENT_INFERENCE/1`-Umschlag. Der Relay sendet sie per
`relay_send_with_attachments` **ohne** `new_chat` und wertet nur diese eine
Antwort erneut aus. Ein zweiter Fehler beendet den Request; die äußere
Turn-Schleife läuft nicht weiter. Das Zeitbudget gilt weiterhin für die
gesamte Anfrage inklusive Korrektur.

Test: `relay::tests::korrektur_nur_einmal_in_derselben_konversation` gibt vier
ungültige Antworten zurück und verlangt `new_chats=1`, `sends=2`. Die
Gegenprobe mit gültiger zweiter Antwort verlangt ebenfalls `new_chats=1`,
`sends=2` und `Ok`.

## Rot vor Green

Die drei Relay-Tests wurden gegen den ungeänderten Pfad zuerst ausgeführt:

```text
ungueltiger_umschlag_ist_fehlschlag_mit_eigenem_grund ... FAILED
  ungueltiger Umschlag darf kein Erfolg sein
korrektur_nur_einmal_in_derselben_konversation ... FAILED
  assertion failed: result.is_err()
erfolgreiche_korrektur_wird_als_erfolg_verbucht ... FAILED
  left: 1, right: 2   (nur eine Sendung, keine Korrektur)
```

Nach der Implementierung: alle grün.

## Gates

```text
cargo fmt --all -- --check                       ok
cargo clippy --all-targets -- -D warnings         ok
cargo test --lib                                 1474 passed; 0 failed; 1 ignored
cargo test --lib startup::tests::
  betriebs_markdown_hat_eine_wahrheit             ok
git diff --check                                 ok
```

Lokale Umgebung: MinGW (`windres.exe`, `ar.exe`) liegt in einem WinGet-Paket
und wurde nur für die Cargo-Läufe an `PATH` gehängt; keine Repo-Datei wurde
dafür geändert. `docs/API_BRIDGE.md` beschreibt Fehlerklasse, brain_score-Grund
und Korrekturgrenze im Abschnitt „OpenAI-Tool-Calling".
