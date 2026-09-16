<!-- **Referenz: Beleg der T-937-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-937 Handoff — Composer fokussieren: Tastatur mit Verifikation statt Koordinatenklick

- Task: T-937
- Owner: local/opencode
- Branch: `refactor/T-937-keyboard-focus` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `d2bb98e` (Status `claimed`, owner `local/opencode`)
- Beweise: `docs/proofs/T-937/gates.txt`
- Scope laut Board: `src/browser/composer.rs`, `src/browser/send.rs`.
  Ausnahmen (verdrahtet unten): `src/mock_page.rs` (nur Test-Harness),
  `src/brain_score.rs` (additive Beobachtungsfelder, DoD-unterscheidbarkeit).

## Ziel

Der Composer wurde blind per Koordinaten-Klick auf den Viewport-Schnitt des
`getBoundingClientRect()`-Rects fokussiert. Bei ChatGPT-ProseMirror luegt das
Rect bei grossen Prompts (h=13k, y=-10k): der geometrische Mittelpunkt liegt
ausserhalb der WebView, der Klick daneben. Gemessen am 2026-09-14 bei 52.287
Zeichen: gemini bestand im 1. Versuch (33s), chatgpt und kimi scheiterten je
dreimal. T-936 hat das als Observation (focus_arrived) erfasst, nicht als
Korrektur — die Korrektur ist diese Aufgabe.

## Was sich aendert

Neuer verifizierter Fokusweg in `composer.rs`, von allen drei Fill-Pfaden
benutzt (`fill_composer`, `fill_composer_rich_multiline`, `fill_composer_dom_set`):

1. **Tastatur (Tab)**: bis zu `FOCUS_TAB_TRIES = 5` Tab-Drucke ueber den
   bestehenden `press_key`-Weg (keine Koordinaten). Nach **jedem** Druck prueft
   eine `activeElement`-/`:focus`-Sonde gegen den Composer-Selektor. Erstes
   Zusammentreffen = gewonnene Fokusstrategie `keyboard`, `focus_tries` = Anzahl
   Drucke. Selbstkorrigierend: jeder Druck fragt den realen DOM-Zustand ab.
2. **In-Page `focus()` + Verifikation** (Muster `webview_runtime.rs:1198`):
   `el.focus()` und erneut die selbe Sonde. Greift auch bei `tabindex=-1`
   (dort erlaubt Script-Fokus, Tab nicht) — markiert mit `tabindex_fallback`.
3. **Koordinaten-Klick** (bestehender Weg) bleibt als letzter Rueckfall
   (Non-Goal: nicht entfernen). Auch nach dem Klick wird die Fokus-Lage
   geprueft und als `click` + `focus_arrived` ausgewiesen.

Eingang in die T-936-Turn-Beobachtung (Side-Channel `PENDING_TURN`): neue
additive Felder `focus_method` ("keyboard"|"el_focus"|"click"|"none"),
`focus_tries`, `tabindex_fallback`; `focus_arrived` (T-936) wird jetzt vom
verifizierten Weg statt von einer unverbindlichen Nach-Klick-Sonde gesetzt.
`send.rs` ist unveraendert (der Focus-Verdrahtungspunkt liegt in `fill_composer`,
das `send_generic`/`send_qwen` ohnehin aufrufen).

## DoD / Zustands-Unterscheidbarkeit (geprueft)

| Zustand | Datenmuster (events.jsonl turn) |
|---|---|
| Fokus per Tab erreicht | `focus_method:"keyboard"`, `focus_arrived:true`, `focus_tries:N`, `tabindex_fallback:false` |
| Composer hat `tabindex=-1`, In-Page-Fokus half | `focus_method:"el_focus"`, `focus_arrived:true`, `tabindex_fallback:true` |
| Klick-Rueckfall (noch nicht messbar besser) | `focus_method:"click"`, `focus_arrived:bool` |
| „Fokus kam nie an" | `focus_method:"none"`, `focus_arrived:false` — klar von el_focus-Erfolg unterscheidbar |
| Kein Selektor / keine Koordinaten + kein Fokus | `focus_method:"none"`, `element_w/h` gepraegt durch T-936 |

Beweis: 4 neue Unit-Tests in `composer.rs` (s. gates.txt). Der Mock simuliert
`activeElement`-Ankunft per `on_eval_seq`: nach dem dritten Tab-Druck `true`
→ keyboard/Triebzahl 3, keine Koordinaten noetig. `tabindex`-Fall: 5 Fehlversuche
→ el_focus-Sonde `true` → klar unterscheidbar. Klick-Rueckfall: koordinaten
vorhanden (ProseMirror-Clamp), Sonde bleibt `false` → `click`/`arrived:false`.

## Messwerte (Handoff-Anforderung: vorher/nachher — ehrlich)

- Vorher: blinder Rect-Klick, keine Verifikation, kein Tastaturweg; bei
  chatgpt/kimi (52.287 Zeichen, 2026-09-14) je 3 Fehlversuche; `focus_arrived`
  war unverbindliche Nach-Klick-Sonde.
- Nachher (in dieser Umgebung messbar): 4 neue Tests, 1437/0/1 (war 1433);
  Fokus-Strategie + Verifikation + Distinktpraeferenz (keyboard > el_focus >
  click) in den Daten. **Nicht hier messbar:** die Live-52k-Charakter-Vergleichs-
  messung gegen chatgpt/kimi braucht einen echten WebView-Lauf mit gemessener
  Erfolgsrate pro Provider — in dieser Session kein Browser-Lauf (wie T-924/T-930/
  T-936 unit-basiert). Folge-Messung ist Teil des Tasks; bis dahin faellt der
  Tastaturweg in Live nur selten ein und der Klick bleibt aktiv (ehrlicher
  Stand: noch nicht „gemessen besser", deshalb bleibt der Klick).

## Realitaets-Check Tastatur-Events (offengelegt)

`press_key` dispatchiert **synthetische** in-page `KeyboardEvent`s
(press_key_script, `webview_runtime.rs:1565`) an `document.activeElement`.
Chromium fuehrt fuer untrusted/synthetische Events keine echte Tab-Fokus-
navigation aus — der Keyboardweg gewinnt also nur dort, wo die Seite den
Tab-Empfang selbst beantwortet/den Fokus weitergibt. Ein vertrauenswuerdiger
Tab (OS-Level SendInput wie der Paste-Pfad) liegt ausserhalb des T-937-Scopes
(`webview_runtime.rs`). Konsequenz: Tastaturweg ist best-effort mit
Verifikation; darunter faengt `el.focus()` (tabindex=-1-faehig) ab; darunter
der Klick. Alle drei sind in den Daten unterscheidbar — das war das DoD.

## Ausnahmen vom Scope

- `src/mock_page.rs`: `press_key` speichert die Tastendruecke
  (`press_key_calls`/`press_key_payloads`) — reiner Test-Harness, damit die
  Tastatur-Fokus-Loop deterministisch pruefbar ist. Produktionslogik: unveraendert.
- `src/brain_score.rs`: nur additive `Option`-Felder mit `serde(default)`
  (`focus_method`, `focus_tries`) und `tabindex_fallback: bool` (serde-default,
  skip when false). Alte events.jsonl-Zeilen lesen weiter; bestehende
  T-936-Tests unveraendert gruen.
- `src/browser/send.rs` (laut Board im Scope): unveraendert — der Focus-Einstieg
  findet in `fill_composer` statt, den alle Send-Pfade bereits rufen.

## Nicht umgesetzt (bewusst, laut Board Non-Goals)

- Kein Umbau am Absendeweg; kein Warten auf Antwort-Veraenderung.
- Klickpfad nicht entfernt (gemessen-besser-Huerde nicht bestanden, da heute
  keine Live-Messung moeglich).
- Kein OS-Level Tab (SendInput) in `webview_runtime.rs` — ausserhalb Scope,
  als moegliche T-937-Folge/T-938-Hintergrund vermerkt.