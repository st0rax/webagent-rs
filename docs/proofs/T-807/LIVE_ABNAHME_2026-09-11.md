# T-807 Live-Abnahme 2026-09-11 (Zwischenstand)

> **Archiv.** Live-Abnahme-Befunde T-807 (Log/Beweise); kein Betrieb.
> Lebend: docs/CAPABILITY_MATRIX.json + docs/PROVIDER_STATUS.md (Matrix),
> docs/TASKBOARD.md (T-807), docs/BRAIN_UNIFICATION_PLAN.md (Gates).

Datum/Uhrzeit: 2026-09-11 00:00–02:10 UTC; alle Brains vorab frisch eingeloggt
(Login-Runde am 2026-09-10/11, kimi/mistral per `--force`).

## Vorgehen

Gemeinsame Konformitätssuite pro Brain via `webagent verify --brain <id>` (16
Capabilities bei Bedarf, sonst gezielte Pruefungen `--cap chat`), Canary bleibt
Smoke. Belege: `%LOCALAPPDATA%\webagent\data\capability\proofs.jsonl`
(1492 Zeilen, alle Messungen 2026-09-11). Matrix-Update erfolgt separat und
ohne Umdeuten von failed/unreachable.

## Frische Providerbelege (Stand 09-11 01:00 UTC)

| Brain | model_switch | reasoning | chat | stop_generation | new_chat | projects | Bemerkung |
|---|---|---|---|---|---|---|---|
| chatgpt | Unreachable | — | **Passed** (12,2 s) | Unreachable | **Passed** | **Passed** (5) | nach --force-Relogin |
| qwen | **Passed** | **Passed** (reasoning_effort) | **Passed** (10,9 s) | Unreachable | **Passed** | — | nach --force-Relogin |
| deepseek | — | **Passed** (reasoning_toggle) | **Passed** | Failed (Stop nie sichtbar) | **Passed** | — | web_search Passed |
| gemini | **Passed** | — | **Passed** | Unreachable | Failed (URL kein Wechsel) | — | |
| kimi | **Passed** | — | **Passed** | Failed | **Passed** | Failed (bleibt @new_chat) | |
| zai | **Passed** | Unreachable (reasoning_toggle) | **Passed** | Unreachable | **Passed** | — | web_search Passed |
| claude | — | — | **Passed** (15,5 s / 17,6 s) | — | — | **Passed** | Selektor-Fix + Nachtipp-Profil |
| mistral | Unreachable | — | **Passed** (11,3 s / 6,9 s) | — | — | — | DOM-Set-Fill + geduldige Aktivierung |
| perplexity | — | — | **Passed** (10,3 s / 10,3 s) | **Passed** | — | **Passed** | Send-Button-Selektor `aria-label*='Senden' i` |

## Befunde aus der Live-Vermessung (probe)

- **claude**: aktueller Send-Knopf `[data-testid='chat-input-send']` (probe 95%).
  Alte Selektoren (`[aria-label]=Send message` u.a.) trafen einen deaktivierten
  Knopf — deshalb ABSENDEKNOPF_DEAKTIVIERT, kein Login-Problem (Login frisch,
  diagnose: keine Cloudflare). Zusaetzlich: Claude nutzt Tiptap/ProseMirror;
  reines DOM-Set aktiviert den Send-Knopf nicht. Ergaenztes `SendFlowProfile:
  claude()` = Gemini-Fill (`FillStrategy::Gemini`, Zeichen-nach-Zeichen
  Nachtippen bei grauem Knopf) + `GestureStyle::EnterThenButton`.
- **mistral**: aktueller Send-Knopf `button[aria-label*='Senden' i]` (probe 70%);
  exaktes `aria-label='Senden'` in Datei vorhanden, aber Substring-Variante fehlte
  als erster Treffer.
- **perplexity**: probe fand keinen sauberen send_button (Attach-Link als
  send_button fehlgedeutet); Composer `#ask-input` (85%) korrekt. Send-Button
  per `button[aria-label*='Senden' i]` ergaenzt.

## Gates nach Selektor-/Profil-Aenderungen

- cargo test --lib: 1411 passed / 0 failed (58,7 s)
- cargo test --lib --features tui: 1444 passed / 0 failed (58,0 s)
- cargo check --features tui: ok (3 Warnings vorbestehend)
- cargo check --no-default-features: ok (3 Warnings vorbestehend)

## Geänderte Dateien (uncommittet, siehe Bitbucket/PR)

- selectors/claude.json (send_button: chat-input-send vorangestellt)
- selectors/mistral.json (send_button: aria-label*='Senden' i vorangestellt)
- selectors/perplexity.json (send_button: aria-label*='Senden' i ergaenzt)
- src/browser/send.rs (SendFlowProfile::claude(), SendFlowProfile::mistral(),
  FillStrategy::Claude/Mistral + Dispatch in send_generic; send_button_disabled:
  Klassen-Token-Statt-Substring-Match — Tailwind-Variante `aria-disabled:…` ist
  keine Disabled-Oberkategorie)
- src/browser/composer.rs (clear_composer neu fuer Nachtipp-Strategien)

## Fix-Diagnose (Live gemessen 09-11)

1. claude: Send-Button live `[data-testid='chat-input-send']`; der Knopf war NIE
   HTML-disabled. `send_button_disabled` meldete fälschlich deaktiviert, weil der
   Substring-Check `cls.indexOf('disabled')` die Tailwind-Erzeugnis-Klasse
   `aria-disabled:…` trat. Danach noch Tiptap-Eigenschaft: Text → Composer, aber
   Knopf grau → Composer erst leeren, dann echt tippen.
2. mistral: Composer ist `div.ProseMirror`, Text steht im DOM, `disabled:true`
   bleibt trotzdem — ProseMirror verbucht CDP-Input nicht als echte Eingabe.
   Fix: Geduldig (bis 1,5 s) auf Aktivierung des Buttons warten; nur falls dann
   noch deaktiviert, DOM-Set + InputEvent(data='insertText'). Frueheres
   automatisches DOM-Set zerstoerte den sonst guten Editor-Zustand (Race:
   Lauf 1 Passed, Lauf 2 ABSENDEKNOPF vor dem Fix).
3. perplexity: probe fand keinen send_button; `button[aria-label*='Senden' i]`
   ergaenzt — chat Passed.

## Blocker (Win 11, WebView2 152.0.4191.66)

- Headed-Browserstart ist zeitweise maschinenweit eingefroren (Navigation
  timeout 8 s/15 s; Start-Reconcile vergroessert das Fenster). Workaround fuer
  nachweisbare Messungen: `--headless` + `--skip-startup-reconcile`. Alle
  Live-Messungen oben liefen damit; die Ergebnisse sind echte Stream-Beweise
  (count>baseline, appends/replaces).
- run_ledger-Flake (`zwei_threads_schreiben_lueckenlose_kette`) unter Volllast
  (Lock-Timeout 5 s); isoliert und im ruhigen Gesamtlauf grün.

## Naechster Schritt / Blocker

Live-Matrix komplett. Matrix-Update (docs/CAPABILITY_MATRIX.json) und
PROVIDER_STATUS folgen separat; failed/unreachable werden nicht umgedeutet.