<!-- **Referenz: Beleg der T-938-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-938 Handoff - Fokus-Abweichungen erkennen und fail-closed benennen

- Task: T-938
- Owner: local/opencode
- Branch: `refactor/T-938-focus-gate` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `fd3f05d` (Status `claimed`, owner `local/opencode`)
- Beweise: `docs/proofs/T-938/gates.txt`
- Scope laut Board: `src/browser/composer.rs`, `selectors/`.
  Ausnahmen (verdrahtet unten): `src/brain_score.rs` (additive
  Beobachtungsfelder, DoD-Unterscheidbarkeit), `src/brain_probe.rs`
  (Gegenprobe: `focus_*` als benigne Ueberschneidung).

## Ziel

T-937 setzt `focus_method:"none"`, wenn weder Tab, noch `el.focus()`, noch
Koordinaten den Composer erreichen - dann gilt "Fokus kam nie an" ohne Grund.
T-938 macht aus diesem Zustand eine **Diagnose** statt eines Ratespiels: das
Fokus-Inventar der sichtbaren Elemente wird aufgenommen und fail-closed benannt.
Beobachtet am 2026-09-14: Claude-Reauth-Loginseite, ein Sperr-Hinweis, ein
ChatGPT-Dialog mit Angebot ohne Datenanalyse fortzufahren. Genau diese drei
Fälle werden unterscheidbar benannt (login / blocked / unknown-mit-Wortlaut);
Consent wird als eigener harmloser Fall geführt.

## Was sich aendert

Fokus-None-Branch in `focus_composer_verified` (`composer.rs`):

1. **Trap-Erkennung**: Der Tastatur-Loop protokolliert je Fehlversuch den
   `activeElement`-Descriptor (`tag#id|label|role|text`). Blieb die Runde auf
   genau **einer** Station stehen und kam der Composer nie an = modaler Dialog
   faengt den Fokus → `focus_trap: true`.
2. **Fokus-Inventar**: EINE Page-Eval-Runde (kein Klick mitten in der
   Diagnose) sammelt alle sichtbaren fokussierbaren Elemente
   (`input,textarea,select,button,[tabindex],[contenteditable],a[href],[role]`)
   und klassifiziert sie gegen die Fokus-Kategorien via `mm()` aus dem
   `JS_SEL_PRELUDE` → `[{cat,tag,role,label,text,ae}]`.
3. **Fail-closed-Diagnose** (`diagnose_focus_failure`), Prioritaet
   blocked > quota > login(→ `not_logged_in`) > consent(→ `consent_dialog`);
   leer → `no_focusable`; sonst → `unknown` mit wörtlichem Bestandsauszug der
   bis zu 3 ersten Stationen im `banner` (nie geraten, nie ein unbekannter
   Knopf geklickt, nie pauschal Accept/Upgrade - die koennen Kosten).
4. **Protokoll**: `focus_trap`, `focus_diagnosis` und `focus_stations` je Turn.

Selektoren: flache Keys `focus_blocked` / `focus_quota` / `focus_login` /
`focus_consent` (positive whitelists, mehrsprachig wo belegt).
`selectors/_generic.json` bildet die Basis fuer alle Brains; `claude.json`
(login aus Reauth-Beobachtung, quota aus `rate_limit_banner`) und `zai.json`
(login, Continue-with-X-Muster) erweitern sie mit belegten Texten.
`Selectors::list` liest nur flache Arrays - die four Keys sind das Format.

## DoD / Zustands-Unterscheidbarkeit (geprueft)

| Zustand | Datenmuster (events.jsonl turn) |
|---|---|
| Fokus-None + Reauth/Login-Feld | `focus_diagnosis:"not_logged_in"`, `focus_stations:N` |
| Fokus-None + Sperrbanner | `focus_diagnosis:"blocked"` |
| Fokus-None + Quota/Hinweis | `focus_diagnosis:"quota"` |
| Fokus-None + bekannter Consent-Dialog | `focus_diagnosis:"consent_dialog"` (nie pauschal akzeptiert) |
| Fokus-None + unbekannte Stationen | `focus_diagnosis:"unknown"`, `banner` mit wörtlichem Auszug (≤3) |
| Fokus-None + kein fokussierbares Element | `focus_diagnosis:"no_focusable"` |
| Tab-Runde haengt auf einer Station | `focus_trap:true` (zusätzlich) |

Beweis: 6 neue Unit-Tests (s. gates.txt). Diagnose-Tests registrieren exakt
den `focus_inventory_expr()`-String im Mock (JSON-Rueckgabe {cat,tag,role,
label,text,ae}); der Trap-Test fixt `active_element_descriptor_expr()` ueber
`on_eval_seq` mit identischer Station ueber alle 5 Tauchversuche und prueft
`focus_trap` + `focus_diagnosis` im Turn.

## Messwerte (ehrlich)

- Vorher: `focus_method:"none"` ohne Grundlage; nichts Unterschiedbares.
- Nachher (hier messbar): 6 neue Tests, 1443/0/1 (war 1437); Diagnose je
  Zustand geprueft. **Nicht hier messbar:** Live-Inventar-Aufnahme gegen echte
  Provider-Seiten - wie T-937/T-936 heute kein Browser-Lauf in dieser Session.
  Die Selektoren sind **inkubiert** (Grundmuster aus beobachteten Faellen), kein
  Teil wurde in dieser Session live gegen Claude/ChatGPT/Kimi verifiziert; der
  `probe`-Pfad kalibriert sie bei den nächsten Live-Laeufen nach. Bis dahin ist
  fail-closed die Garantie: unbekannt bleibt `unknown` mit Wortlaut.

## Ausnahmen vom Scope

- `src/brain_score.rs`: additive Felder `focus_trap: bool` (serde-default,
  skip-when-false) sowie `focus_diagnosis`/`focus_stations` (`Option`).
  Alte events.jsonl-Zeilen lesen weiter.
- `src/brain_probe.rs`: `gegenprobe_schlaegt_nichts_falsches_vor` erkennt
  `focus_*` als benigne Ueberschneidung - Diagnose-Merkmale benennen bewusst
  dieselben Elemente wie `login_button`/`consent_*`/`send_button`.
- `selectors/` (im Scope): flache `focus_*`-Keys; `_generic.json` als Basis
  (Overlay-Verhalten laut `config/selectors.rs` - Top-Level-Key ersetzt,
  daher fuehren `claude.json`/`zai.json` die benoetigten Eingabefeld-Muster
  selbst mit).

## Nicht umgesetzt (bewusst, laut Board Non-Goals)

- Kein Cloudflare/CAPTCHA-Bedienen; diese werden als `unknown`/Literal
  gemeldet, nicht umgangen.
- Keine Sonderfall-Liste im Rust-Code - Beschriftungen gehoeren in die
  Selektordateien (`focus_*`-Keys).
- Kein pauschales Klicken von `Accept`/`Upgrade`-Knoepfen (Kostenrisiko);
  Consent wird nur benannt und ausgewiesen.
- Kein OS-Level Tab (SendInput) - weiterhin Hintergrund-Folgetask aus T-937.