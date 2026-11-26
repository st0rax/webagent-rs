**Referenz.** Real gemessener Beleg für T-976 und T-977 vom 2026-11-26. Kein Login, keine Zugangsdaten, keine Profil-/Token-Dateien gelesen. `WEBAGENT_API_KEY` blieb Benutzer-Umgebungsvariable; sein Wert steht in keiner Datei und keiner Ausgabe.

# T-976 — API-Turns decken geparkte Brain-Fenster auf

Datum: 2026-11-26. Branch: `fix/T-976-api-turn-reveal`. Basis: `master` `93d33a3`.

## Ausgangslage (aus echten Logs)

Drei Pi-Sessions direkt nach dem Reboot um 04:00 Uhr:

| Session | Beobachtung |
|---|---|
| `01a215fd` | „kannst du dich mal auf meinem pc umschauen" → nur „Hi." ohne Tool-Call, von der Bridge als `success=true` gezählt. |
| `01a21601` | 7 echte `bash`-Calls, Kontext 16 k→52 k, danach `timeout_no_message` nach 290 s und `auto_attach_timeout` nach 52 s (`content_check: Composer-Feld nicht gefunden`). |
| `01a21608` | `webagent/claude`, 2-Zeichen-Prompt, nach 94,6 s `aborted`, kein Bridge-Event; das Claude-Fenster lag bei `-32000`. |

`brain_score/events.jsonl` belegt dieselben Muster: `timeout_no_message: renderer may be unresponsive after wake`, `content_check: Composer-Feld nicht gefunden`, `client_disconnected`.

## Root cause

Der API-/Relay-Pfad rief vor dem Turn kein `reveal_onscreen` auf. `ensure_ready` parkt bei `SessionState::Ready` aktiv wieder; `wake_renderer` (`src/browser/surface.rs`) ist nur ein CDP-`move_pointer`. Bei ausgeschalteter Wall und offscreen liegendem WebView kommt der Renderer dadurch nicht zuverlässig hoch.

## Änderung

- `src/browser/mod.rs`: neues `turn_visible`-Flag, `reveal_for_turn()`, `end_turn_visibility()`, `is_turn_visible()`, `park_on_ready()` und `Drop`, das auf jedem Rückkehrpfad parkt.
- `src/browser/backend.rs`: `ensure_ready` parkt während eines API-Turns nicht sofort wieder.
- `src/relay.rs`: `reveal_for_turn()` direkt nach `start()`; Fehler werden gemeldet, brechen den Turn aber nicht ab.
- Neuer Unit-Test `t976_turn_sichtbarkeit_verhindert_parken_bei_ready`.

## Gates (real gemessen)

- `cargo fmt --all -- --check` → 0
- `cargo clippy --all-targets --locked -- -D warnings` → 0
- `cargo test --lib --locked` → 1525 passed, 0 failed, 1 ignored
- `git diff --check` → 0

## Deployment und Beleg

- Alte Bridge: PID 1952, Binary `v0.11.7`, Backup `webagent.exe.bak-T977-20261126` im Deploy-Ziel.
- Neues Release gebaut und deployt; SHA-256 `8b7877c2136c035e8e5b3d8fa51c5fe92f76948ad2cd0e2146ceed5bb9696e52`.
- Bridge neu gestartet mit identischen Argumenten (`--skip-startup-reconcile api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`), PID 17372, `/health` = ok.
- Echter Pi-Turn gegen die neue Bridge: `pi --provider webagent --model webagent/auto 'hi'` → Antwort „Hi! Wie kann ich dir heute helfen?". Der erste Versuch lief über drei `content_check`-Fehler mit 26.526 Zeichen System-Prompt (inkl. AGENTS.md) und wurde vom AutoRouter auf gemini umgestellt; der vierte Turn war sauber (`brain_score`: `gemini success=true`, 23.384 ms).
- `/v1/models` liefert nach dem Deployment `context_window=1000000`, `max_tokens=65536` (T-977).
- Pi-Default bleibt `webagent/auto`; neue interaktive Session PID 1552.

## Offene Risiken (nicht als gelöst behauptet)

- `content_check` scheitert weiterhin bei sehr grossen Prompts: gemessen `element_h=15002` bei 52 k Zeichen und `pasted_chars=26358` statt `expected_chars=26526` bei 26 k Zeichen. T-976 löst den ruhenden Renderer, nicht die Insert-/Content-Prüfung bei übergrossen Texten.
- Ein Client-Turn mit ~105 k Zeichen Prompt endete nach 299 s mit `client_disconnected`; das ist eine Client-/Latenz-Grenze, kein beobachtetes Token-Limit.
- Der durch den Smoke-Test ausgelöste chatgpt-Circuit-Breaker wurde aufgehoben (Backup `state.json.bak-T977-20261126`); die eigentliche Insert-Ursache bleibt offen.

Keine Zugangsdaten gelesen oder gespeichert; `auth.json` unberührt; `WEBAGENT_API_KEY` blieb Benutzer-Umgebungsvariable.
