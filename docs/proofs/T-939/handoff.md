<!-- **Referenz: Beleg der T-939-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-939 Handoff - brain_id-Verzweigungen im Sendepfad sind Selektordaten

- Task: T-939
- Owner: local/opencode
- Branch: `refactor/T-939-send-data` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `e6c698c` (Status `claimed`, owner `local/opencode`)
- Beweise: `docs/proofs/T-939/gates.txt`
- Scope laut Board: `src/browser/send.rs`, `src/browser/composer.rs`,
  `selectors/`. Ausnahme: `src/browser/verify.rs` (Test-Helper und eine
  T-962-Regression, siehe unten).

## Ziel

Der Sendepfad verteilte 13+ Oberflaechen-Tatsachen ueber `brain_id`-Zweige
(gemini-3x, mistral-2x, kimi-9x, dazu deepseek-1x). Diese Tatsachen sind keine
Laufzeit-Entscheidungen, sondern deklarierte Eigenschaften der jeweiligen
Provider-Oberflaeche und stehen jetzt in der Selektordatei des Brains. Ausserdem
pruefte der generische Pfad das Fuellen nur mit einem 8-Zeichen-Praefix
(`composer_contains`), der bei prompts mit gleichem Anfang eine verschmutzte
Restausgabe des Editors nicht erkennen konnte. Standard ist jetzt
`composer_matches_text` (gesamter sichtbarer Inhalt, nur Leerraum normalisiert).

## Was sich aendert

Policy-Helfer in `send.rs` (`WebBrainBackend`-impl, alle aus `self.sel(...)`):
- `strat_flag(key)` - Marker-Key gesetzt (hat Eintraege) -> bool
- `strat_first(key)` - erster Eintrag als `Option<String>`
- `strat_ms(key, default)` - erster Eintrag als Zahl, sonst Default

Ersetzte brain_id-Zweige -> Selektordaten:

| alt (Code) | neu (Selektordaten) | Dateien |
|---|---|---|
| gemini `image_gen`-Gate + Providername + trusted surface | `image_gen_enabled`, `image_gen_provider_name`, `image_gen_open_trusted` | gemini, chatgpt |
| deepseek Vision-Modus (`attach_image_mode_segment/reset`) | `attach_image_mode_segment:["Vision"]`, `attach_image_mode_reset:["Instant"]` | deepseek |
| qwen/zai/mistral trusted CDP Upload (`prefers_trusted_cdp_upload`, 3 Stellen) | `attach_trusted_cdp` | qwen, zai, mistral |
| mistral OS-Clipboard-Paste, 2 Stellen | `attach_clipboard_paste` | mistral |
| kimi Bild-Paste (2 Stellen) und Stale-Draft-Reset | `attach_image_paste`, `attach_reset_stale_drafts` | kimi |
| kimi Settle-Zeiten | `attach_settle_ms:["1500"]` (Default 250), `attach_draft_settle_ms:["300"]` (Default 50) | kimi |
| kimi transient-file-inject | `attach_inject_file_objects` | kimi |
| kimi Lexical-Fill | `composer_fill:["rich_multiline"]` | kimi |
| kimi Send-Button statt Enter | `composer_submit_button` | kimi |
| kimi Proof erst nach Consumed | `composer_proof_consumed_first` | kimi |

`composer_matches_text` ist jetzt der einzige Fill-Verify (send_generic,
send_gemini, send_qwen) und der einzige Consumed-Check (alle drei Sende-Loops
plus `verify_submitted`). Mit dem Wegfall von `composer_contains` sind auch
`composer_needle` (T-962-Fixprodukt) und der zugehoerige Regressionstest
entfernt - die Nadel existierte nur fuer den 8-Zeichen-Praefix. Der verify.rs
Test-Helper registriert stattdessen den `composer_matches_text`-Ausdruck
wortgleich (Mock registriert Seiten-Skripte als exakte Strings).

`prefers_trusted_cdp_upload` ist entfernt; der Test
`qwen_zai_mistral_prefer_trusted_cdp_upload` prueft jetzt die Selektordaten
(`qwen_zai_mistral_waehlen_trusted_cdp_aus_selektordaten`).

## DoD (geprueft)

- Keine `brain_id`-Verzweigung mehr im Sendepfad: `rg` auf
  `brain_id == / != / matches!` in `send.rs` ist leer (verbliebene Nennungen
  sind Log-/Pfadstrings).
- Fuell- und Pruefstrategie stehen je Anbieter in der Selektordatei (Tabelle oben).
- `composer_matches_text` ist der Standard in allen drei Send-Pfaden und dem
  gemeinsamen Verify.

## Grenzen (ehrlich)

- Unit-Tests belegen die Pfade und die Datenlage; echte Sende-, Bild- und
  Uploaddurchlaeufe auf den Provider-Webseiten sind in dieser Umgebung nicht
  moeglich (gleiche Einschraenkung wie T-936/T-937/T-938). Verhaltensaenderung
  durch die Umstellung ist moeglich, wenn die Selektordaten von den Live-UIs
  abweichen - die Marker spiegeln den Code-Stand vom 2026-09-14 und frueher.
- Der Dispatch auf `send_generic`/`send_gemini`/`send_qwen` lebt in
  `backend.rs` (aussenhalb des Scope); die drei Funktionen blieben getrennt
  (Verschmelzen waere Verhaltensrisiko ohne Datenlage).
- Disk: fuer die Gates wurden 21,7 GB target-Ordner bereits gemergter
  Worktrees geloescht (frei war 0 Byte).