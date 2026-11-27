# T-1018 — Composer-Inhaltsbeweis auch für kurze Prompts

## Anlass

`crates/webagent-transport/src/browser/composer.rs` lieferte aus
`composer_holds_text` ohne jeden Composer-Vergleich `true`, sobald der
Prompt höchstens 1.000 Zeichen hatte:

```rust
if expected.chars().count() <= 1_000 || self.composer_matches_text(...) {
    return true;
}
```

Diese Kurzschaltung war ein falscher Beweis. Sie betrifft zwei Pfade:

- `wait_fill_composer`: ein leerer oder fremder Editor konnte als „gefüllt"
  gelten.
- `consumed = !composer_holds_text(...)` in `send_generic`, `send_gemini`
  und `send_qwen`: ein noch nicht konsumierter Prompt konnte als
  „bereits gesendet" gelten, wodurch ein weiterer Sendeversuch entfiel
  oder unnötig wiederholt wurde.

Der Befund war seit dem T-1016-Deploy dokumentiert, aber nicht gefixt.

## Änderung

- Der exakte Normalvergleich `composer_matches_text` ist jetzt für **alle**
  Promptlängen zwingend. Er wurde als `composer_exact_expr` herausgezogen,
  damit der Test denselben JS-Ausdruck scripten kann.
- Für kurze Prompts gibt es bewusst **keine** Toleranz: nach einem
  Fehlschlag des strikten Vergleichs liefert `composer_holds_text` `false`.
- Die 10-%-Längen-/Rand-Toleranz für lange Prompts bleibt unverändert
  (`long_text_holds`); sie ist nur für Rich-Editor-Normalisierung nötig.
- Kein Nutzerprompt-Text wird gekürzt, zusammengefasst oder umformuliert.

## Regressionstests

- `kurzer_prompt_ohne_composer_treffer_ist_kein_beweis`: kurzer Prompt,
  Composer meldet `false` → `composer_holds_text` ist `false`.
- `kurzer_prompt_mit_exaktem_composer_inhalt_ist_belegt`: gleicher Prompt,
  Composer meldet `true` → `composer_holds_text` ist `true`.

## Gates

Siehe `gates.txt`. Kurz gesagt: fmt 0, clippy 0, 1558 Tests / 0 Fehler /
1 ignored, Architektur 5 passed, TUI 0, no-default 0, diff-check 0.

## Finaler Build, Deploy und Live-Abnahme

- Commit `23ac4e9` (`master`, gepusht). Release-Build
  `webagent 0.11.8 (23ac4e9b3)`, Exit 0; EXE-SHA-256
  `BBB9C714A0074C6AC975582BB337CB2720B1F189703419A7C5711ECBF35EDA3B`,
  DLL unverändert `465A7DDF…A0A6F`.
- Rollback-Ordner
  `...\webagent-deploy-target\backup-release-before-T1018-20261127\`
  mit der vorherigen EXE `B990DA8B…F8FB93`, DLL und Breaker-State.
  PID `1144` gestoppt, Port frei. Neue PID `9008`, **identische**
  Command-Line (`--skip-startup-reconcile api serve --brain chatgpt
  --headless --port 8788 --timeout-secs 120`), Health `ok`, `/v1/models`
  200 mit Header / 401 ohne. Logs:
  `bridge.err|out.restart-T1018-20261127.log`.
- **Kurzer Turn:** 12 Tools, Prompt `Antworte exakt mit T1018-SHORT-OK.` →
  HTTP 200, Antwort exakt `T1018-SHORT-OK`, 20,6 s. Event
  `success=true`, `sends=1`, `prompt_chars=2677`. Der neue strikte
  Composer-Beweis hat den Kurzturn also nicht blockiert.
- **Guard unverändert:** 85.000 Zeichen Request, 12 Tools → 0,102 s,
  HTTP 502 mit `Maximum 83000`, `events_delta=0`, kein Browserlauf.
- **Großer Pfad unverändert:** 80.000 Zeichen Realtext, 12 Tools →
  HTTP 200, exakt `T1018-REAL80-OK`, 35,1 s, `success=true`,
  `sends=1`, `prompt_chars=82643`.
- Kein Tag, GitHub-Release, Versionssprung oder Login;
  `WEBAGENT_API_KEY` nur als Header verwendet und nie angezeigt.
- **Lokales Staging für den zweiten PC:**
  `...\webagent-deploy-target\T1018-20261127\` mit EXE, DLL und README;
  Hashes geprüft. Kein automatisches Deployment dort.
- **Rückweg:** PID `9008` stoppen, EXE/DLL aus
  `backup-release-before-T1018-20261127\` zurückkopieren, Breaker-State
  bei Bedarf wiederherstellen und mit identischer Command Line neu starten.
