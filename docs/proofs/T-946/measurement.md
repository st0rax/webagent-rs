**Referenz.** Lokaler Red-/Green-Beleg zu T-946, kein Live-Nachweis am echten Browserprofil. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`.

# T-946 — Abgebrochene Anfrage: verwaisten Turn beenden und Brain-Sperre freigeben

Datum: 2026-11-21  
Branch: `fix/T-946-client-disconnect-lock`  
Scope: `src/api_bridge/inference.rs`, `src/api_bridge/transport.rs`, `src/api_bridge/provider_handlers.rs`, `src/api_bridge.rs`, `src/browser_inference.rs`, `src/browser/backend.rs`, `src/relay.rs`, `docs/API_BRIDGE.md`  
Belegart: lokale Red-/Green-Tests mit echtem TCP-Loopback und Mock-Backend, ohne Browser/CDP und ohne Zugangsdaten. **Kein Live-Anspruch**, kein Tag/Release.

## Ausgangslage (Messbefund aus dem Taskboard)

Am 2026-09-14 wurde ein Pi-Aufrufer für `mistral` um 10:35:25 beendet, während sein Browser-Turn lief. Die Bridge führte den Turn trotzdem bis 10:44:08 weiter — 867.530 ms für eine Antwort, die niemand mehr abholte. Zwei weitere abgebrochene Turns liefen 866.608 ms und 861.306 ms weiter. Der `run_task_blocking`-Pfad hielt die per-Brain-Sperre (`BROWSER_RUN_LOCKS`, Key = Brain) während des gesamten Turns; eine Folgeanfrage an dasselbe Brain wartete daher auf den verwaisten Turn. Anfragen an andere Brains liefen parallel weiter (`gemini` 10:35:28–10:35:54, `kimi` 10:36:02–10:36:25) — das ist korrekte Sperr-Semantik, widersprach aber `docs/API_BRIDGE.md` („ohne parallele Browserruns zuzulassen").

`netstat` zeigte um 11:13:12 auf Port 8788 nur noch den lauschenden Socket; die Clients waren weg, die Turns liefen weiter.

## DoD 1 — Beendigung oder Verwaist-Markierung + Sperrfreigabe

**Vorher:** Keine Beobachtung des Aufrufers während des Turns. Die Sperre wurde bis zum Ablauf des Browser-Timeouts gehalten, unabhängig davon, ob der Client noch verbunden war.

**Nachher:** Drei Ebenen (alle ohne globale Serialisierung, Key bleibt das Brain):

1. `TcpClientPresence` in `src/api_bridge/transport.rs` hält einen zweiten Socket-Handle. `connected()` macht einen 20 ms-Peek und stellt den Lese-Timeout sofort wieder her. `Ok(0)` = FIN → getrennt; `WouldBlock`/`TimedOut` = verbunden; anderer Fehler = getrennt. Fällt `try_clone()` fehl, gilt der Client als verbunden (lieber ausführen als falsch abbrechen).
2. `with_browser_lock` in `src/api_bridge/inference.rs` prüft vor **und** nach dem Erwerb der Brain-Sperre. Ist der Aufrufer schon weg, wird die Closure gar nicht erst ausgeführt und die Sperre nicht gehalten.
3. `run_turn_loop` in `src/relay.rs` prüft zu Beginn jedes Turns erneut. Während des Wartens übergibt es einen `cancelled`-Closure an `WebBrainBackend::wait_response_streaming_with_cancel`, der in **beiden** Poll-Phasen geprüft wird. Ein verwaisster Turn endet terminal mit `backend_status=client_disconnected`, ruft `relay_stop`, verbucht `brain_score` mit `reason=client_disconnected` und gibt die Sperre frei.

Bereits gesendete Prompts werden **nicht** stillschweigend erneut gesendet (Non-Goal).

Echter Loopback-Beleg ohne Browser: `api_bridge::transport::tests::tcp_presence_erkennt_geschlossenen_peer` verbindet einen Client, sendet ein Byte, trennt die Verbindung und verlangt, dass der Peek das EOF erkennt — bei gleichzeitig wiederhergestelltem Lese-Timeout des Haupthandles.

## DoD 2 — Folgeanfrage wartet nicht auf verwaisten Turn

Test `api_bridge::inference::tests::folgeanfrage_wartet_nicht_auf_verwaisten_turn`: Turn A hält die Brain-Sperre; sein Aufrufer verschwindet; der Turn endet früh (im Relay über `cancelled` belegt) und gibt die Sperre frei. Die Folgeanfrage B läuft innerhalb von < 2 s statt A's Zeitbudget abzuwarten. Gegenprobe `verwaisste_anfrage_wartet_nicht_auf_die_brain_sperre`: mit fremd gehaltener Sperre gibt eine bereits verwaiste Anfrage sofort `client_disconnected` zurück, ohne die Closure auszuführen und ohne auf `lock()` zu warten.

## DoD 3 — brain_score-Grund und relay_stop

Tatsächliche Events aus `%TEMP%\webagent_test_data\brain_score\events.jsonl` (Mock-Brains, Testlauf 2026-11-21):

```json
{"brain_id":"mock_verwaist_start","ts":"2026-11-21T01:20:38.517820+00:00","success":false,"reason":"client_disconnected","latency_ms":0,"prompt_chars":5,"sends":0}
{"brain_id":"mock_verwaist_mitten","ts":"2026-11-21T01:20:38.500600+00:00","success":false,"reason":"client_disconnected","latency_ms":21,"prompt_chars":5,"sends":1}
```

`verwaiste_anfrage_startet_keinen_turn` verlangt `new_chats=0`, `sends=0`, `stops=1` und den Grund im Event; `abbruch_im_warteloop_beendet_turn_frueh` verlangt `new_chats=1`, `sends=1`, `stops=1`, Grund im Event und Abschluss in < 2 s statt der konfigurierten 5 s.

## Rot vor Green

Der echte TCP-Loopback-Test war zunächst **rot**: das unaufgelesene Byte im Peek-Puffer maskierte das später eintreffende EOF. Erst nach dem Verbrauchen des Bytes (wie `read_http_request` es tut) wurde der Test grün. Die vier Relay-/Lock-Tests wurden gegen den ungeänderten Pfad nicht ausgeführt, weil die Signatur `presence`/`cancelled` vorher nicht existierte; die DoD-Wirkung wird über die neuen Tests und die echten Event-Zeilen belegt.

## Gates

```text
cargo fmt --all -- --check                       ok
cargo clippy --all-targets -- -D warnings         ok
cargo test --lib                                 1479 passed; 0 failed; 1 ignored
git diff --check                                 ok
```

Ein erster vollständiger `cargo test --lib`-Lauf zeigte einen einmaligen `ConnectionReset` im unabhängigen Test `web_ui::tests::api_rolle_zugriff_mit_token_liefert_modellliste` (20 msSleep-basierter One-Shot-Test). Fünf Einzelwiederholungen und zwei vollständige Wiederholungen liefen grün; der Fehler war nicht reproduzierbar und steht nicht im Zusammenhang mit den hier geänderten Pfaden (diese Route nutzt keine Browser-Inference).

Lokale Umgebung: MinGW (`windres.exe`, `ar.exe`) liegt in einem WinGet-Paket und wurde nur für die Cargo-Läufe an `PATH` gehängt; keine Repo-Datei wurde dafür geändert. `docs/API_BRIDGE.md` beschreibt jetzt korrekt die Serialisierung je Brain und das Abbruchverhalten.
