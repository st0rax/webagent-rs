**Referenz.** Code-/Mock-DOM-Beleg zu T-949, kein Betrieb und kein Live-Nachweis. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`.

# T-949 — Messung: Zustimmungs-Texte werden vom Dialog-Schliesser nicht geklickt

**Art des Belegs:** Code-/Mock-DOM-Test. Es wurde **kein** Live-Dialog mit einem Anbieter
gefahren. Ein Live-Nachweis für mistrals Zustimmungsdialog ist Sache von T-948 und bleibt
dort offen. Dieser Beleg deckt ausschließlich die DoD ab: *„dismiss_consent prueft das vor
jedem Klick und klickt im Zweifel nicht; ein Test mit Mock-DOM belegt, dass Accept and
Continue und OK, I agree in einem offenen Dialog nicht geklickt werden, Got it und Close
weiterhin schon."*

## Geänderter Pfad

- `src/browser/operations.rs`: `dismiss_consent` ruft für `dialog_dismiss_button` jetzt
  `click_dialog_dismiss_guarded` statt `click_first`. Vor dem Klick wird die Beschriftung
  des Treffers gelesen und gegen die Wortlisten `AGREEMENT_LABEL_WORDS` /
  `NON_AGREEMENT_MARKERS` geprüft. Ist die Beschriftung nicht lesbar, wird fail-closed
  nicht geklickt.
- Nicht geändert: `consent_reject_button`, `notice_close_button`, `profile_block_button`
  und der generische `dismiss_modal_buttons`. Damit bleiben Cookie-Ablehnung und
  „Nur notwendige Cookies" unangetastet (non-goal). Insbesondere qwens
  `Accept only necessary cookies` und mistrals `Continue without accepting` werden über
  `NON_AGREEMENT_MARKERS` ausdrücklich nicht als Zustimmung gewertet.

## Geprüfte DoD-Fälle (Mock-DOM, `dismiss_consent_tests`)

| Test | Erwartung | Ergebnis |
|---|---|---|
| `mock_dom_klickt_accept_and_continue_nicht` | kein Klick | ok |
| `mock_dom_klickt_ok_i_agree_nicht` | kein Klick | ok |
| `mock_dom_klickt_got_it_und_close_weiterhin` | Klick | ok |
| `nicht_lesbare_beschriftung_klickt_nicht` | kein Klick (fail-closed) | ok |
| `zustimmungsbeschriftungen_werden_erkannt` | 6 Zustimmungs-Texte erkannt | ok |
| `harmlose_beschriftungen_bleiben_erlaubt` | 9 harmlose/Cookie-Texte nicht erkannt | ok |

Der Mock registriert für den Klick-Ausdruck `true`. Ein Test ist nur dann grün, wenn
`dismiss_consent` den Klick-Ausdruck bei einem Zustimmungs-Text **gar nicht erst
aufruft**; bei `Got it`/`Close` muss er ihn aufrufen.

## Gate-Ergebnisse (20.11.2026, Branch `fix/T-949-consent-guard`)

| Kommando | Ergebnis |
|---|---|
| `cargo fmt` | Exit 0 |
| `cargo fmt --check` | Exit 0 |
| `cargo clippy --all-targets -- -D warnings` | Exit 0 |
| `cargo test --lib dismiss_consent_tests` | 6 passed; 0 failed |
| `cargo test --lib` | 1460 passed; 0 failed; 1 ignored |

Verwendete Toolchain: `cargo 1.x`, Ziel `x86_64-pc-windows-gnu`, mingw64-Linker aus dem
WinLibs-Paket. Keine Zugangsdaten, keine Profile, keine live Provider-Anfrage.
