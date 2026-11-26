# T-1015 — Begrüßungs-Stubs und Tool-Umschlag mit Trailer

> **Referenz.** Task proof für T-1015. Er dokumentiert die Auswertung der
> OpenCode-Session `Greeting` und die Code-Änderung; er schaltet nichts live.

- Task: T-1015
- Owner: local/opencode
- Branch: `fix/T-1015-greeting-stub-retry`
- Status: umgesetzt, geprüft und vom Eigentuemer freigegeben live deployt;
  Gates siehe `gates.txt`, Deploy-Beleg
  `docs/proofs/deploy-T1015-bridge-restart-2026-11-26/README.md`

## Auswertung der Session `Greeting`

Quelle: lokale OpenCode-DB `C:\Users\storax\.local\share\opencode\opencode.db`
(nur-Lese-Zugriff), `session_v2`/`session_message` für
`ses_de6ca134effeoxqH4z2uBVf4yx` (Titel `Greeting`, Verzeichnis
`C:/Users/storax/Documents/Standardprojekt/webagent-rs-t1008`, zuletzt
2026-11-26 20:47 UTC).

- Aufträge mit aktiven Client-Tools (`[api] chat model=chatgpt stream=true
  tools=12`) wurden wiederholt mit reinen Stubs beendet: `chatgpt`, `Hallo.`
  und `chatgpt bereit.`. Es folgte kein Werkzeugaufruf und keine inhaltliche
  Antwort.
- Nur ein einziger `write`-Call kam tatsächlich durch und legte
  `C:\Users\storax\Desktop\notizbuch.md` an (4.011 Byte, SHA-256
  `39B99C3BC37FD4D6D58F4E18990168E8B6642A097E86AE556C5F2D94A9F3BFA1`,
  21:26:25 lokal).
- `events.jsonl` meldet um 21:25:39 UTC einen chatgpt-Turn mit
  `success=false`, `reason=model_output_envelope_invalid`, 90.037 ms bei
  49.604 Prompt-Zeichen; der nächste Turn um 21:26:25 UTC war erfolgreich.
- `bridge.err.restart-20261126.log` dazu:
  `MODEL_OUTPUT_ERROR: Ungueltiger Browser-Tool-Call-Umschlag: trailing
  characters at line 2 column 1`. Der Rohtext zeigt einen vollständigen
  `write`-Umschlag, danach `\n``` ` — der Markdown-Abschluss einer
  Codeblock-Antwort machte den gültigen JSON-Call ungültig.
- Die `[T-961]`-Meldungen im Log sind das dokumentierte, einmalige Verhalten
  einer semantischen Werkzeugverweigerung (`done`-Task), kein Defekt.
  `Retry erschoepft` kam in diesem Zeitraum nicht vor.
- Von 20:00 UTC bis zur Auswertung lagen neun chatgpt-Events vor, acht
  erfolgreich und genau das eine `model_output_envelope_invalid`.

## Ursache im Code

1. `crates/webagent-api/src/browser_inference.rs`,
   `complete_streaming_with_attachments`: parste die erste Antwort und gab
   einen reinen Begrüßungs-Stub als ganz normale Endantwort zurück. Es gab
   keine Erkennung dieser Fehlerklasse — anders als bei der leeren Antwort
   (`is_retryable_empty_response`) und der semantischen Verweigerung (T-961).
2. `envelope_payload`/`parse_response` übergaben den gesamten Payload nach
   `WEBAGENT_INFERENCE/1` an `serde_json::from_str`. Nur ein vorangestellter
   Markdown-Codeblock-Abschluss wurde entfernt; Trailer nach dem JSON
   (` ``` `, Prosa) führten zu `trailing characters at line 2 column 1`.

## Änderungen

1. **`crates/webagent-core/src/brain.rs`** — neue, enge
   `is_greeting_only_response`: erkennt nur die GANZE Antwort als Stub, wenn
   sie nach Trimmen und Entfernen von Schluss-Satzzeichen genau `chatgpt`,
   `chatgpt bereit`, `chatgpt ist bereit`, `bereit`, `hallo`, `hi`, `hola`,
   `hello` oder `hey` ist. Protokoll-Nutzlast hat Vorrang; mehrsätzige
   Antworten wie `chatgpt bereit. Was steht an?` fallen nicht hierunter.
2. **`crates/webagent-api/src/browser_inference.rs`** — bei aktiven
   Client-Tools (nicht `tool_choice=None`) und vorhandenem ursprünglichem
   Auftrag (`retry_prompt`) erhält ein solcher Stub genau EINEN Frisch-Chat
   mit dem bytegenau weitergetragenen Auftrag plus einem kurzen Hinweis
   (`greeting_retry_prompt`). Wiederholt das Modell den Stub, wird er ehrlich
   als Antwort geliefert und `Retry erschoepft` protokolliert — kein stilles
   PASS. Reine Textturns ohne Tools und echte Begrüßungen bleiben unberührt.
   Die vorhandene T-961-Logik bleibt unverändert und läuft auf dem Ergebnis
   des Stub-Retrys.
3. **`crates/webagent-api/src/browser_inference.rs`** — `envelope_json_slice`
   begrenzt den Payload auf das balancierte JSON-Objekt. Klammern in Strings
   zählen nicht (escaped Quotes berücksichtigt). Ein Markdown-Abschluss oder
   Prosa danach ist damit kein Parse-Fehler mehr; fehlt die schließende
   Klammer, bleibt der Originalpayload für die strenge Fehlermeldung
   erhalten (fail-closed).
4. **`docs/API_BRIDGE.md`** — Verhalten und Grenzen dokumentiert.

## Erwartung / Grenze

- Ein `chatgpt bereit.` als Endantwort auf einen Tool-Auftrag führt zu einem
  Frisch-Chat-Retry statt zu einer leeren Antwort an den Client.
- Ein vollständiger Tool-Call mit `\n``` ` danach wird ausgeführt statt als
  `model_output_envelope_invalid` zu scheitern.
- Der Fix beseitigt **nicht** das Modellverhalten selbst: ein erneuter Stub
  wird zurückgegeben, kein Erfolg behauptet. Es gibt keinen zweiten Retry
  derselben Fehlerklasse; die vorhandene T-961-Klasse bleibt separat.
- Die aktive Bridge läuft nach ausdruecklicher Freigabe jetzt mit der
  T-1015-EXE (`573719d11`), PID `2564`, Port `127.0.0.1:8788`, Health `ok`.
  Zwei kontrollierte Live-Turns und der Rueckweg sind im Deploy-Beleg
  `docs/proofs/deploy-T1015-bridge-restart-2026-11-26/README.md` dokumentiert.
  Die Rollback-Ordner `backup-release-before-T1015-20261126` und
  `backup-release-T1014-20261126` bleiben erhalten.
- Keine Zugangsdaten gelesen oder ausgegeben, kein Login, kein Tag, kein
  GitHub-Release, kein Versionssprung.
