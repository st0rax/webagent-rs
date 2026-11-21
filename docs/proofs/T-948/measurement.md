**Referenz.** Lokaler Red-/Green-Beleg zu T-948 mit Mock-DOM, **kein Live-Nachweis** am echten mistral-Profil. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`; aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

# T-948 — Abgemeldete Seite mit Zustimmungsdialog wird nicht mehr als Erfolg gemeldet

Datum: 2026-11-21  
Branch: `fix/T-948-consent-preflight` (ab `master` `32aa2e0`)  
Scope: `selectors/mistral.json`, `src/browser/operations.rs`, `src/browser/send.rs`, `src/mock_page.rs`, `src/browser/verify.rs`, `docs/API_BRIDGE.md`  
Belegart: lokale Unit-Tests gegen `MockPageDriver` mit den ausgelieferten mistral-Selektoren. **Kein Live-Anspruch**, keine Anmeldung, keine Zustimmung, kein Tag/Release.

## Ausgangslage (Messbefund aus dem Taskboard)

Am 2026-09-14 endeten sechs Bridge-Anfragen an `mistral` zwischen 11:28:55 und 11:34:18 ohne Antwort — alle mit `timeout_no_message` aus `backend.rs`, dreimal je 92 s Budget. Das Senden meldete jedes Mal Erfolg; danach sah Phase 1 von `wait_response_streaming` weder neue Assistant-Nachricht noch Stop-Knopf noch Textänderung. Länge (32.134 Zeichen) und Pi als Aufrufer waren als Ursache ausgeschlossen (77-Zeichen-Kontrolle ebenfalls erfolglos).

`webagent shot --brain mistral --headless` um 11:36 belegte die Ursache: die Seite war abgemeldet, oben rechts standen „Anmelden"/„Registrieren", und ein modaler Dialog „Vibe Nutzungsbedingungen" mit dem Knopf „Akzeptieren und fortfahren" verdeckte den Composer. Drei Lücken ließen das unbemerkt:

1. `login_indicator` nannte den ProseMirror-Composer, der auch abgemeldet hinter dem Dialog im DOM steht → `is_logged_in()` lieferte wahr.
2. `login_button` kannte nur englische und französische Beschriftungen, nicht „Anmelden".
3. Der Sendepfad meldete Erfolg, obwohl der Dialog den Composer blockierte; `webagent doctor` meldete dasselbe Brain gleichzeitig `healthy` mit `login_state unknown`.

## DoD 1 — Benannter Fehler statt `timeout_no_message`

**Vorher:** `send_generic` rief `wake_renderer`, füllte den Composer und meldete Absende-Erfolg. Der nicht zugestellte Prompt lief in die Wartephase und endete nach drei Budgets als `timeout_no_message`.

**Nachher:** `operations::pre_send_block_reason` prüft vor `wake_renderer` und vor jedem Fill rein lesend zwei Signale — zuerst `terms_consent_dialog`, dann `login_button`. Gefunden wird `TERMS_CONSENT_REQUIRED_MARKER` (`terms_consent_required`) bzw. `LOGIN_REQUIRED_MARKER` (`login_required`). `send_generic` setzt die Phase `SelectorResolve` und gibt `send_phase=preflight: <marker>: <brain> ist nicht bereit, den Prompt anzunehmen; Modals und Anmeldung bleiben beim Menschen` zurück. Über `relay.rs` → `browser_inference_error` wird daraus HTTP 502 mit dem Marker im Fehlertext. Es wird nichts in den Composer geschrieben und kein Enter gedrückt.

Der Guard greift **nur**, wenn das Brain `send_preflight_guard` in seiner Selektordatei deklariert (derzeit `mistral`). Damit bleiben `chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `perplexity`, `qwen` und `zai` bei ihrem weichen Login-Verhalten (T-946/T-953: Selektor-Drift und noch ladende Seiten dürfen nicht hart blockieren). Sichtbare Anmeldeknöpfe schlagen auch hier jeden positiven Indikator — dieselbe Priorität wie in `is_logged_in`.

## DoD 2 — `login_indicator` nur im angemeldeten Zustand

`selectors/mistral.json` ersetzt den Composer im `login_indicator` durch Profil-/Konto-Knöpfe (`button[aria-label*='Profil' i]`, `…Profile…`, `…Account…`, `…Konto…`) und ergänzt `login_button` um „Anmelden" als Button und Link. Test `mistral_login_indicator_trifft_nicht_die_abgemeldete_seite`: mit sichtbarem Composer + sichtbarem Anmeldeknopf + unsichtbarem Profilknopf liefert `is_logged_in()` **falsch**; ohne Anmeldeknopf und mit sichtbarem Profilknopf **wahr**.

**Grenze:** Die neuen Selektorstrings folgen der Projektkonvention (chatgpt nutzt dieselben Profil-Labels, kimi `.user-profile-trigger`), sind aber **nicht live vermessen**. Der Beleg gilt für die Mock-DOM-Entscheidungslogik, nicht für einen realen Treffer auf der aktuellen mistral-Seite.

## DoD 3 — Mock-DOM des Dialogs: nicht gesendet, nicht zugestimmt

Test `mistral_terms_dialog_bricht_vor_dem_senden_ab` scriptet den ausgelieferten mistral-Selektor `terms_consent_dialog` als sichtbar und verlangt:

- `error.contains(operations::TERMS_CONSENT_REQUIRED_MARKER)`
- `insert_text_calls() == 0` — kein Prompt geschrieben
- `press_key_calls() == 0` — kein Enter gedrückt
- `eval_count_for(consent_click_expr) == 0` — der Klick-Ausdruck von `dialog_dismiss_button` (exakt `DIALOG_CLICK_BODY` aus `operations.rs`) wird nie ausgewertet

Test `mistral_anmelden_knopf_bricht_vor_dem_senden_ab` scriptet denselben Dialog als unsichtbar und den „Anmelden"-Knopf als sichtbar; er erwartet `login_required` und ebenfalls keine Fill-/Key-Aktivität. `preflight_guard_ist_nur_fuer_deklarierte_brains_aktiv` prüft mit einer minimalen Selektor-Mappe ohne Guard, dass ein sichtbarer Login-Knopf dort **keinen** Grund liefert.

Die beiden Marker sind als `pub const` in `operations.rs` definiert; `mock_page.rs` zeichnet alle `eval`-Ausdrücke auf (`eval_count_for`), damit „nicht geklickt" messbar ist statt behauptet.

## Rot vor Green

Mit temporär deaktiviertem Guard (`if false` vor `pre_send_block_reason`) scheiterten beide Guard-Tests mit dem alten generischen Fehler:

```text
browser::verify::tests::mistral_terms_dialog_bricht_vor_dem_senden_ab ... FAILED
panicked at src\browser\verify.rs:2183:9:
send_phase=insert: Composer-Feld nicht gefunden (Timeout)

browser::verify::tests::mistral_anmelden_knopf_bricht_vor_dem_senden_ab ... FAILED
(ebenfalls send_phase=insert: Composer-Feld nicht gefunden (Timeout))

RED_TERMS_EXIT=101
RED_LOGIN_EXIT=101
```

Nach Wiederherstellung und erzwungenem Rebuild (Timestamp des Quellfiles aktualisiert, damit Cargo den zwischenzeitlich zwischengespeicherten Testbinary nicht wiederverwendet):

```text
mistral_terms_dialog_bricht_vor_dem_senden_ab ... ok
mistral_anmelden_knopf_bricht_vor_dem_senden_ab ... ok
GREEN_TERMS=0
GREEN_LOGIN=0
```

Die Datei wurde während der Läufe nicht anderweitig verändert; das Backup lag unter `%TEMP%\opencode\send.rs.T948.backup`.

## Gates

```text
cargo fmt --all -- --check                        ok
cargo clippy --all-targets -- -D warnings          ok (0 warnings)
cargo test --lib                                  1483 passed; 0 failed; 1 ignored
git diff --check                                  ok
```

Zusätzlich einzeln grün: `mistral_terms_dialog_bricht_vor_dem_senden_ab`, `mistral_anmelden_knopf_bricht_vor_dem_senden_ab`, `mistral_login_indicator_trifft_nicht_die_abgemeldete_seite`, `preflight_guard_ist_nur_fuer_deklarierte_brains_aktiv`.

Lokale Umgebung: MinGW (`windres.exe`, `ar.exe`) aus einem WinGet-Paket wurde nur für die Cargo-Läufe an `PATH` gehängt; keine Repo-Datei wurde dafür geändert.

## Nicht erledigt / bewusst draußen

- Kein echter mistral-Lauf, kein Screenshot nach dem Fix, keine Live-Prüfung der neuen Profil-/Anmelde-Selektoren. Der `handoff`-Wunsch „Fehlermeldung nachher gegen den abgemeldeten Zustand" ist **nicht** erfüllt; dieser Beleg ist ausschließlich lokal.
- Keine Änderung an der Wiederholungslogik (`is_deterministic_send_failure` bleibt unberührt — T-947). Ein Preflight-Fehlruf durchläuft daher die üblichen Turns, sendet aber in keinem Prompt; der benannte Marker übersteht die Wiederholungen und landet im `brain_score`-Grund.
- Kein Zustimmungs- oder Anmelde-Klick, keine Profil-/Cookie-/Token-Datei gelesen, keine Kontoeinstellung geändert.
- Kein Tag, kein GitHub-Release.
