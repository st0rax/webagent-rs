# Beleg: echter `webagent play`-Lauf mit geladenem Kern `deepseek` (2026-11-26)

**Referenz.** Lokaler Testbetrieb auf Loopback. Kein fernes oder produktives Deployment, kein PR, kein Merge nach `master`. Keine Zugangsdaten gelesen oder ausgegeben; `auth.json` unberührt; `WEBAGENT_API_KEY` blieb Prozessumgebung des Brains.

## Ausgangslage

- Binary: `target\debug\webagent.exe`, Branch `fix/T-976-api-turn-reveal`.
- Preflight `webagent doctor --brain deepseek`: `healthy: deepseek`, `selectors: ok`, `profile: ok`, `login_state: unknown`, `last_run: keiner`; Circuit-Breaker ohne `deepseek`-Eintrag.
- Laufende Bridge PID 17372 blieb unberührt; `/health` vor und nach den Läufen `ok`.
- Kommandos im Spiel (per stdin): `j`, `read buch`, `ask deepseek lies das buch`, `status`, `go ost`, `scan`, `map`, `quit`.

## Drei echte Läufe, chronologisch

### 1. Erster Lauf — `0 XP`, zwei Fehler (headful)

`Kern deepseek geladen. Zustand: angemeldet`. Der Tutorial-Zug lieferte **keine Observations**: `Belege 0, XP +0, fehlerhaft`. Der `scan`-Zug führte einen Shell-Befehl aus, dessen `$`-Zeichen fehlten (`key = env:WEBAGENT_API_KEY` statt `$key = $env:WEBAGENT_API_KEY`) → PowerShell-Syntaxfehler, Exit 1, kein Beleg. Endstand: `deepseek XP 0 / fehlerhaft 2`, Geister 2, `run status=brain_incomplete`.

**Root cause aus dem echten Transcript** (`data/runs/20261126_062826_5d0c4f70/transcript.jsonl`):

1. Die erste Brain-Antwort war ein **korrektes** `WEBAGENT/1 SHELL`-Rohskript mit `$env:WEBAGENT_API_KEY`, begann aber mit dem WebView-UI-Chrome `textCopyDownloadWEBAGENT/1 SHELL`. Der zentrale Parser entfernt UI-Labels nur, wenn sie als eigene Zeilen auftauchen; angeklebtes `textCopyDownload` blieb stehen → Rohformat-Regex matchte nicht → `Ungültiges JSON: expected ident at line 1 column 2`.
2. Der Repair-Hinweis veranlasste das Brain, denselben Befehl ohne UI-Chrome, aber **ohne alle `$`** erneut zu senden. Dieser Rohformat-Call wurde ausgeführt (daher die Terminal-Ausgabe) und scheiterte an PowerShell.
3. Die dritte Antwort hatte wieder `textCopyDownload…` und wurde erneut abgelehnt; nach zwei Zyklen `brain_incomplete`.

Der alte Spielstand wurde als `save-bugrun-20261126.json` unter `%TEMP%\opencode\dogfooding-play\` gesichert, bevor eine frische Ausgangslage gesetzt wurde.

### 2. Zwei Weck-Versuche headful — WebView-Navigation

Beide Versuche brachen beim Start ab: `Navigation timeout after 8.01s to https://chat.deepseek.com/ (limit 15s): page-timeout: Page-Befehl timeout (WebView möglicherweise eingefroren)`. Kein Browser-Prozess hing, kein Profil-Lock, kein `deepseek`-Circuit-Eintrag; ein Brave-Fenster mit DeepSeek und ein Einstellungen-Fenster lagen über dem WebView. Mit `--headless` (gleiches Profil, gleiche Session) startete der Kern sofort: `Kern deepseek geladen. Zustand: angemeldet`.

### 3. Erfolgreicher Lauf — `--headless`

| Zug | Echte Action | Belege | Ergebnis |
|---|---|---:|---|
| `ask deepseek lies das buch` | `shell: Get-Content docs/DOGFOODING.md`, Exit 0, Observation im Terminal | 1 | `Belege 1, XP +30, korrekt` — `TUTORIAL-ABGESCHLOSSEN … DOGFOODING-OK-20261126`, Abzeichen `startschein` |
| `scan` in `bridge>` | `shell: bridge-check-1`, `Invoke-WebRequest` gegen `/health` und `/v1/models`, Exit 0 | 1 | `Belege 1, XP +30, korrekt` — `/health` HTTP 200 `{"service":"webagent-provider-bridge","status":"ok"}`, `/v1/models` HTTP 200 mit 9 Modellen; Abzeichen `brueckenstein` |

Run `20261126_064021_00154c0c`: `status=done`, `cycles=2`, `act_steps=1`, `observation_bytes=4219`, `episode_memory_id=3`. Der Brain nutzte `$env:WEBAGENT_API_KEY` nur im Prozess und redaktierte den Token in der Antwort (`[REDACTED]`); kein Tokenwert in Log, Transkript oder Spielstand.

**Spielstand nach `quit`** (`data/play/save.json`): `deepseek XP 60 / Level 1 / korrekt 2 / fehlerhaft 0`, Geister 0, Abzeichen `startschein, brueckenstein`, Raum `bridge>`, Kern `deepseek` aktiv. XP kamen ausschließlich aus zurückgepipeten Observations, nie aus Behauptungen.

## Code-Änderung aus dem Lauf

`src/protocol/parser.rs`: neue Hilfsfunktion `is_glued_ui_controls` entfernt WebView-Button-Labels, die ohne Trennzeichen am `WEBAGENT/1`-Marker kleben (`textCopyDownload…`, `textKopierenHerunterladen…`). Der Vorspann wird nur verworfen, wenn **alle** Zeilen aus solchen Controls bestehen; echte Prosa davor bleibt erhalten und wird weiter abgelehnt. Damit bleibt es bei **einem** Parser — kein zweiter Parser, kein zweiter Proof-Store.

Neue Regressionstests in `src/protocol/mod.rs`: `test_parse_rendered_deepseek_glued_controls_before_raw_shell` (gültiges Rohskript mit `$env:` nach `textCopyDownload`) und `test_parse_keeps_prosa_before_glued_controls` (Prosa davor bleibt ungültig).

## Gates nach dem Fix (real gemessen)

`cargo fmt --all -- --check` 0 · `cargo check --locked --all-targets` 0 · `cargo clippy --all-targets --locked -- -D warnings` 0 · `cargo test --locked --lib` **1535 passed / 0 failed / 1 ignored** · `cargo build --locked` 0 · `git diff --check` 0.

## Grenzen

- Der erfolgreiche Lauf war **headless**; headful blieb bei zwei Versuchen an der WebView-Navigation hängen. Ursache nicht endgültig geklärt (Fenster-Overlay wahrscheinlich, nicht bewiesen).
- Kein `give`, kein Item-Bonus, kein `load`/`eject`-Wechsel und kein `arena/`-Zug in diesem Lauf — diese Mechaniken bleiben unit-getestet, aber live ungeprüft.
- Kein Login, kein CAPTCHA, kein Zugangsdaten-Zugriff, kein Deployment, kein Neustart der Bridge, kein Tag/Release/PR/Merge.
