**Referenz.** Ehrliche Auswertung der echten Live-Laeufe vom 20.11.2026; **kein
Abnahmebeleg**. T-961 bleibt `free`. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 Live Acceptance - NICHT ABGENOMMEN

## Kurzergebnis

Rohe Pi-JSON-Modus-Transkripte vor und nach dem Fix liegen jetzt vor
(`before.jsonl`, `after.jsonl`). Sie belegen das **Gegenteil** einer Abnahme:
beide Laeufe scheitern an derselben Ursache. Es gab **keinen** einzigen
`tool_execution_start`, keine verwertbare Assistant-Antwort und keine
fuenf aufeinanderfolgenden Werkzeugturns. Der Kontozustand wurde geprueft; die
Anmeldung des chatgpt-Browsers ist **nicht bestaetigt**.

## Was jetzt tatsaechlich gemessen wurde

| Beleg | Before (dd78128) | After (d0856d9, enthaelt Fix 864e346 via 493e9a6) |
|---|---|---|
| Rohtranscript | 29.021 Bytes, 2 Turns | 29.305 Bytes, 2 Turns |
| Assistant-Text | leer, `stopReason: error` | leer, `stopReason: error` (`assistant_ok=False`) |
| Fehlermeldung | `Browser-Inference fehlgeschlagen: session_state=Unbestimmt` | identisch |
| `tool_execution_start` | 0 / 5 | 0 / 5 |
| `session_state=Unbestimmt` | 13x | 3x |
| `circuit_open` | 10x | 20x (Breaker nach 5 Fehlschlaegen erneut 900 s offen) |
| Secret-Scan | 0 Treffer | 0 Treffer |

Die erste After-Ausfuehrung (20:49 lokal) produzierte wegen eines
StrictMode-Fehlers im temporaeren Harness-Skript kein Transkript; das Skript
wurde korrigiert (Events ohne `message`-Feld werden uebersprungen, Rohausgabe
wird immer gespeichert) und der Lauf wiederholt. Das hier ausgewertete
`after.jsonl` stammt aus diesem zweiten, echten Lauf.

## Kontozustand (Direktive 7/8: keine Zugangsdaten, keine Profile)

Pruefung ueber `GET /api/health/brains` der laufenden Bridge, ohne Oeffnen oder
Lesen einer Profildatei und ohne Zugangsdaten:

- `chatgpt`: `login_state = "unknown (old)"`, `profile_exists = true`,
  `last_done_run = ""`, `recovery_hint` verweist auf `webagent login --brain chatgpt`.
- Bewertung: **Anmeldung nicht bestaetigt**. Das deckt sich mit dem wiederholten
  `session_state=Unbestimmt` aus der Live-Inferenz.

Damit ist der Account-Check durchgefuehrt (nicht `not accessible`), sein Ergebnis
ist aber keine Freigabe. Die Bridge kann den Sitzungszustand nicht bestaetigen;
ein Agent darf die Anmeldung nicht selbst vornehmen.

## Einordnung des Fix

Der Code-Fix fuer die Werkzeug-Kontext-Rahmung liegt in `master`
(PR #78 `864e346`, enthalten im PR-#80-Squash `493e9a6`). Das ist eine
Code-Aenderung, kein Live-Nachweis. Da die Bridge schon vor dem Senden der
eigentlichen Antwort an `session_state=Unbestimmt` scheitert, wird der
Werkzeug-Fortsetzungspfad in diesem Lauf gar nicht erst erreicht. Der Fix
kann hier weder bestaetigt noch widerlegt werden.

## Offene Punkte / naechster Schritt

1. Eigentuemer meldet den chatgpt-Browser an (`webagent login --brain chatgpt`
   bzw. TUI/WebView-Fenster). Der Agent tippt keine Zugangsdaten und liest keine
   Profile.
2. Danach denselben Harness-Lauf erneut ausfuehren und nur bei echtem Erfolg
   (`assistant_ok=True`, >= 5 `tool_execution_start`, keine Session-/Circuit-
   Fehler, bestaetigter Kontozustand) T-961 auf `done` setzen.
3. Bis dahin: `Status: BLOCKED_BY_UNCONFIRMED_LOGIN`. Kein PASS, kein `done`,
   kein Tag/Release (G-009 bleibt offen).
