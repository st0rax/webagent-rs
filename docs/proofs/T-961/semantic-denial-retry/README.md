**Referenzstatus:** Rohbelege einer neuen, falsifizierbaren T-961-Richtung; **kein** automatisch gesetztes `PASS`. Der Eigentümer entscheidet über die Abnahme. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — semantische Werkzeugverweigerung: Frisch-Chat-Retry

## Neue Richtung

Die vier bisher gemessenen Richtungen sind widerlegt (Verfügbarkeitsregel 2/8, `[FUNKTIONEN]` 6/8, `tool_choice=required` 1/8, Auto→Required 0/7). Neu ist nicht das Wort, sondern die Turn-Semantik: Eine *semantische* Werkzeugverweigerung war bisher ein ganz normal parsebarer Textabschluss unter `tool_choice=auto` (`src/browser_inference.rs`, `parse_response`) und wurde ohne jeden Wiederholungsversuch an den Client geliefert. Der Patch erkennt eng gefasst „Werkzeugbegriff + Verneinung + Verfügbarkeits-/Zugriffsbegriff“ und retryt **genau einmal in einem frischen Chat** mit dem unveränderten ursprünglichen Auftrag plus den bereits gesammelten Werkzeugergebnissen als reinen Daten. Kein T-953-Korrekturtext, kein „verfügbar“-Meta, keine `required`-Eskalation. Ein erneuter Verweigerungstext wird ehrlich weitergegeben und im Bridge-Log als `Retry erschoepft` markiert.

Code: `src/api_bridge/content.rs` (Retry-Prompt aus der Konversation), `src/api_bridge/inference.rs` und `src/api_bridge/provider_handlers.rs` (Durchreichung nur im OpenAI-Tool-Pfad), `src/browser_inference.rs` (Erkennung und einmaliger Frisch-Chat-Retry), `docs/API_BRIDGE.md`. Fixtures: enge Erkennung inkl. „nicht ausführbar verfügbar“, normaler Auto-Text bleibt unberührt, Retry-Prompt mit und ohne vorherige Tool-Ergebnisse.

## Live-Messung

Bridge: `webagent api serve --port 8788 --api-key-env WEBAGENT_API_KEY --brain chatgpt --headless --skip-startup-reconcile`, **ohne** `--auto-tool-choice-required`. Pro Lauf frischer, nur in der Prozessumgebung gesetzter Token; keine Kontoänderung, kein neues Login, keine Zugangsdaten/Profile/Cookies/Tokens gelesen. Isoliertes `PI_CODING_AGENT_DIR`, Pi mit `--tools read`, fünf sequenzielle `PI_TOOL_[1-5]_*`-Marker.

| Lauf | Tool-Starts | Nonces | stopReason=error | Fehlermarker | Relay-Korrekturen | T-961-Retries | Strict |
|---:|---:|---:|---:|---:|---:|---:|:---:|
| 5 | 5 | 5 | 0 | 0 | 0 | 0 | PASS |
| 6 | 5 | 5 | 0 | 0 | 0 | 0 | PASS |
| 7 | 5 | 5 | 0 | 0 | 0 | 0 | PASS |
| 8 | 5 | 5 | 0 | 0 | 0 | 0 | PASS |
| 9 | 5 | 5 | 0 | 0 | 0 | 0 | PASS |

Damit liegen **fünf aufeinanderfolgende strict-clean Pi-Läufe** mit je fünf echten Werkzeugturns vor. Die bytegenauen Transkripte `live-5.jsonl` … `live-9.jsonl`, die Bridge-Logs und die vollständigen Strict-Ausgaben `strict-5.txt` … `strict-9.txt` (jeweils mit SHA256) liegen in diesem Verzeichnis. Secret-Scan über Transkripte und Logs: sauber.

## Ehrliche Gegenbelege und Grenzen

- `negative-live-1.jsonl`: nur 3/5 Tool-Calls, danach vorzeitiger Textabschluss — **keine** Verweigerung, der Retry wurde nicht ausgelöst.
- `negative-live-4.jsonl` (noch vor der Korrektur der Erkennung): 0/5, Text *„Das read-Werkzeug ist in dieser Sitzung nicht ausführbar verfügbar.“* Die alte Erkennung traf nicht, weil die Wörter nicht direkt aufeinander folgten; dies ist im Patch behoben, aber **nicht live nachgestellt** worden.
- In allen fünf grünen Läufen war `t961_fresh_chat_retries = 0`. Der neue Retry-Pfad wurde also live **nicht ausgeübt**; die grüne Serie ist keine kausale Wirkung des Patches. Die Ausgangslage war bereits intermittierend (historisch 3/4 grün mit `eaed3d1`).
- Die Ursache des Modellverhaltens ist damit **nicht bewiesen eingegrenzt**. Der Patch ist eine eng begrenzte Abschwächung einer klar benannten Fehlerklasse, keine Heilung der Intermittenz.
- T-961 bleibt daher `free`, bis der Eigentümer die fünf Läufe ausdrücklich als Abnahme bestätigt. Kein Tag, kein Release, kein Deployment, kein Merge ohne neue Freigabe.

## Gates

`cargo fmt --check`, `cargo build`, `cargo test --lib` (1516 passed, 0 failed, 1 ignored), `cargo clippy --all-targets -- -D warnings`, `git diff --check`: grün. Ein einmaliges `web_ui::tests::api_rolle_zugriff_mit_token_liefert_modellliste` schlug während eines laufenden Bridge-Prozesses fehl und lief isoliert sowie im vollständigen Rerun grün (Port-Kollision, kein Codefehler).
