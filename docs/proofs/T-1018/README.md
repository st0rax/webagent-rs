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

## Deploy und Live-Abnahme

Folgen nach dem Commit und dem Release-Build; dieser Abschnitt wird dann
um die aktive PID, Hashes und den Live-Turn ergänzt.
