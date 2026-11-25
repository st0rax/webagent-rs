**Referenzstatus:** Endabnahme-Beleg für T-961/G-002. Der Eigentümer hat am 2026-11-25 die autonome Fortführung **bis einschließlich Endabnahme** freigegeben. Der Branch ist `fix/T-961-semantic-denial-retry`; ein Merge nach `master` ist **nicht** Teil dieser Freigabe. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — semantische Werkzeugverweigerung: Frisch-Chat-Retry

## Richtung

Die vier zuvor gemessenen Richtungen sind widerlegt (Verfügbarkeitsregel 2/8, `[FUNKTIONEN]` 6/8, `tool_choice=required` 1/8, Auto→Required 0/7). Neu ist nicht die Wortwahl, sondern die Turn-Semantik: Eine *semantische* Werkzeugverweigerung war bisher ein ganz normal parsebarer Textabschluss unter `tool_choice=auto` (`src/browser_inference.rs`, `parse_response`) und wurde ohne jeden Wiederholungsversuch an den Client geliefert. Der Patch erkennt eng gefasst „Werkzeugbegriff + Verneinung + Verfügbarkeits-/Zugriffsbegriff" und retryt **genau einmal in einem frischen Chat** mit dem unveränderten ursprünglichen Auftrag plus den bereits gesammelten Werkzeugergebnissen als reinen Daten. Kein T-953-Korrekturtext, kein „verfügbar"-Meta, keine `required`-Eskalation. Erzeugt auch der zweite Turn wieder eine semantische Verweigerung, wird sie als Endantwort zurückgegeben und im Bridge-Log als `Retry erschoepft` markiert — kein stilles PASS.

Code: `src/api_bridge/content.rs` (Retry-Prompt aus der Konversation), `src/api_bridge/inference.rs` und `src/api_bridge/provider_handlers.rs` (Durchreichung im OpenAI-Tool-Pfad), `src/browser_inference.rs` (Erkennung und einmaliger Frisch-Chat-Retry), `docs/API_BRIDGE.md`. Fixtures: enge Erkennung inkl. „nicht ausführbar verfügbar", normaler Auto-Text bleibt unberührt, Retry-Prompt mit und ohne vorherige Tool-Ergebnisse.

## Live-Messung

Bridge: `webagent api serve --port 8788 --api-key-env WEBAGENT_API_KEY --brain chatgpt --headless --skip-startup-reconcile`, **ohne** `--auto-tool-choice-required`. Pro Lauf frischer, nur in der Prozessumgebung gesetzter Token; kein neues Login, keine Kontoänderung, keine Zugangsdaten/Profile/Cookies/Tokens gelesen. Isoliertes `PI_CODING_AGENT_DIR`, Pi mit `--tools read`, fünf sequenzielle `PI_TOOL_[1-5]_*`-Marker. Unverändertes Binary und unveränderter Harness über alle Läufe.

| Lauf | Tool-Starts | Nonces | stopReason=error | Fehlermarker | Relay-Korrekturen | T-961-Retries | Retry erschöpft | Strict |
|---:|---:|---:|---:|---:|---:|---:|---:|:---:|
| 5 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 6 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 7 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 8 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 9 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| **10** | **5** | **5** | 0 | 0 | 0 | **1** | 0 | **PASS** |
| 11 | 4 | 4 | 0 | 0 | 0 | 0 | 0 | FAIL |
| **12** | **5** | **5** | 0 | 0 | 0 | **1** | 0 | **PASS** |
| 13 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 14 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 15 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 16 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 17 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 18 | 1 | 1 | 0 | 0 | 0 | 0 | 0 | FAIL |
| 19 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 20 | 5 | 5 | 0 | 0 | 0 | 0 | 0 | PASS |
| 21 | 0 | 0 | 16 | 35 | 0 | 0 | 0 | FAIL |

Damit liegen **14 strict-clean Pi-Läufe** mit je fünf echten Werkzeugturns vor, darunter zwei aufeinanderfolgende Serien von je sechs sauberen Läufen (5–10 und 12–17) sowie 19–20. Bytegenaue Transkripte, Bridge-Logs und Strict-Ausgaben liegen in diesem Verzeichnis (`live-*.jsonl`, `bridge-*.stderr.log`, `strict-*.txt`; Negativläufe als `negative-*`). Secret-Scan: sauber — der einzige Treffer war der Umgebungsvariablen-Name im Beispielbefehl der README, kein Token.

## Kausaler Beleg des Retry-Pfads

In **Lauf 10** und **Lauf 12** wurde die Verweigerungsklasse tatsächlich live ausgelöst und vom Patch abgefangen:

- `bridge-10.stderr.log`: der T-961-Retry-Event erscheint nach dem dritten `[api] chat … tools=1`-Turn; danach folgen `call_4` und `call_5`. `live-10.jsonl` enthält alle fünf Nonces und endet mit `stopReason=stop`. SHA256 des Transkripts: `0F022DF4EB9689FB30BC52E7E5C3D625B0B035EAB48F5419248F511DA18288E5`.
- `bridge-12.stderr.log`: der Retry-Event erscheint bereits nach dem **ersten** erfolgreichen Read — der klassische T-961-Fall aus dem Originalauftrag. Danach folgen vier weitere Tool-Calls bis `call_5`, ebenfalls 5/5 und `stopReason=stop`. SHA256: `8CC2BDB4C6955DD2CFAF839248043AA0233857BFF4BF8EE31D6C83F03DF9E198`.

Ohne den Retry wäre der Lauf an der Verweigerung mit weniger als fünf Tool-Calls beendet worden (vgl. die Negativläufe). `t961_retry_exhausted = 0` in allen Läufen: ein zweiter Verweigerungstext wurde nie beobachtet.

## Ehrliche Gegenbelege und Grenzen

- `negative-live-11.jsonl`: 4/5 Tool-Calls, danach vorzeitiger Textabschluss — **keine** semantische Verweigerung, kein Retry.
- `negative-live-18.jsonl`: 1/5, gleicher vorzeitiger Textabschluss, kein Retry.
- `negative-live-21.jsonl`: 0/5, `ABSENDEKNOPF_DEAKTIVIERT` mit anschließendem Circuit-Breaker (`open_until 1795630874`). Ein UI-Sendefehler, keine Werkzeugverweigerung; der Breaker läuft automatisch ab und wurde nicht manuell zurückgesetzt.
- Die drei Negativläufe gehören also **anderen** Fehlerklassen an. Der Patch adressiert ausschließlich die semantische Werkzeugverweigerung; er macht den Mehrschritt-Loop nicht deterministisch.
- Die Ursache des Modellverhaltens ist **nicht bewiesen eingegrenzt**; der Fix ist eine eng begrenzte, live kausal belegte Abschwächung einer klar benannten Fehlerklasse. Die Ausgangslage war historisch intermittierend (3/4 grün mit `eaed3d1`).
- Frühere Negativläufe mit alter Erkennung (`negative-live-1.jsonl`: 3/5 ohne Verweigerung; `negative-live-4.jsonl`: 0/5 mit „nicht ausführbar verfügbar") bleiben als Widerlegung der alten Erkennung erhalten.

## Gates und Abnahme

`cargo fmt --check`, `cargo build`, `cargo test --lib` (1516 passed, 0 failed, 1 ignored), `cargo clippy --all-targets -- -D warnings`, `git diff --check`: grün. Ein einmaliges `web_ui::tests::api_rolle_zugriff_mit_token_liefert_modellliste` schlug während eines laufenden Bridge-Prozesses fehl und lief isoliert sowie im Rerun grün (Port-Kollision, kein Codefehler).

**Endabnahme 2026-11-25:** Der Eigentümer hat die autonome Fortführung bis zur Endabnahme freigegeben. Aufgrund der zwei kausalen Retry-Erfolge und der 14 strict-clean Läufe wurde T-961 auf `done` gesetzt. Der Branch wurde anschließend ausdrücklich freigegeben und via PR #104 (`ea88203`) nach `master` gemergt; alle vier PR-Checks waren grün. Der GitHub-Release `v0.11.5` ist mit Workflow `45684483569` veröffentlicht, Deployment nicht.
