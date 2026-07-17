# Brain Analyze & Add (`/aa`) — Design

> **STATUS: DESIGN, NICHT IMPLEMENTIERT.** Methodik, um ein neues Web-Chat-Brain
> (Provider) möglichst autonom zu erkennen und als Brain hinzuzufügen. Von Claude
> entworfen (2026-07-17), noch kein Code. Vor Umsetzung `git grep -n analyze_add
> src/` prüfen.

## 1. Ziel

Eingabe: eine Provider-URL (z.B. `https://neuer-chat.example/`), optional eine
Brain-ID. Ausgabe: ein **validiertes** `selectors/<brain>.json` + Config-Eintrag,
sodass `webagent relay/run/bot2bot-worker --brain <id>` sofort funktioniert.
Menschlicher Aufwand: **genau ein Login** (Prinzip „immer nur ein Login") und
ggf. eine kurze Abnahme unsicherer Selektoren.

## 2. „Reichen die 8 Brains + Selektoren?" — die Kernfrage

**Ja, zum Bootstrappen der Kandidaten. Nein, als alleinige Wahrheit.** Aufteilung:

- **Für Kandidaten-Generierung (few-shot):** Die 8 bestehenden `selectors/*.json`
  sind ein **Muster-Korpus** — sie zeigen, wie `composer` / `send_button` /
  `assistant_message` / `stop_button` / `new_chat_button` / `login_indicator` /
  `google_sso_button` über 8 reale Seiten aussehen (textarea, contenteditable/
  Lexical, diverse Button-/aria-Muster, Playwright-`text=`/`:has-text`). Das
  genügt, um einem LLM gute *erste* Selektor-Vorschläge zu entlocken.
- **Für Zuverlässigkeit:** Die 8 helfen NICHT beim Beweis, dass ein Vorschlag am
  **neuen** Provider wirklich greift. Das entscheidet ausschließlich die
  **Live-Validierung** (unten §3.4). Mehr Brains verbessern die Rate-Vermutung
  marginal; der eigentliche Zuverlässigkeitshebel ist der **Selbsttest-Loop**,
  nicht die Korpusgröße.

Fazit: 8 reichen als Starthilfe. Verlässlich macht es das empirische Antreiben
der Live-Seite mit Repair-Schleife — analog zur `run`-Protokoll-Härtung.

## 3. Ablauf `/aa` (Analyze & Add)

CLI: `webagent aa --url <url> --brain-id <id> [--headless=false zum Login]`.
REPL: `/aa <url> <id>`. Wiederverwendung von `browser`/`page_driver`/`JS_SEL_PRELUDE`.

1. **Öffnen + Login (headed, einmal).** Provider laden, Nutzer loggt sich ein
   (das eine Login). `is_logged_in`-Heuristik als Signal, dass eine Session steht.
2. **DOM-Harvest.** Die Seite nach Kandidaten scannen (ein JS-Eval, kein LLM):
   alle `textarea`, `[contenteditable]`, `[role=textbox]`, `button` sammeln —
   mit `tag`, `id`, `class`, `aria-label`, `role`, sichtbarem Text, `placeholder`,
   Sichtbarkeit/Position. Ergibt eine kompakte Kandidaten-Liste (JSON).
3. **Kandidaten-Generierung (LLM, few-shot).** Prompt = die 8 bestehenden
   `selectors/*.json` als Beispiele + die Harvest-Liste → „erzeuge das
   äquivalente `selectors/<id>.json` für diese Seite". **Dogfooding:** das kann
   ein vorhandener Brain-Worker via `relay --json` machen (z.B. deepseek/chatgpt)
   — der Pool implementiert neue Brains. Ergebnis: Kandidaten pro Schlüssel.
4. **Live-Validierung (der eigentliche Kern — Selbsttest).** Mit den Kandidaten
   real fahren, kein Raten:
   - `composer` finden + füllen (Testprompt) + `composer_contains` bestätigen.
   - `send_button`/Enter absenden, `verify_submitted` (URL-Wechsel/Stop-Button/
     neue Nachricht).
   - `wait_response` → kam eine **echte** Antwort? `assistant_message` liest sie.
   - `new_chat_button`, `login_indicator`, `google_sso_button` separat proben.
   Jeder Schlüssel bekommt ein **PASS/FAIL** aus echtem Verhalten.
5. **Repair-Schleife.** Für jeden FAIL: Fehlkontext (welcher Selektor griff nicht,
   was war im DOM) zurück an das LLM → neuer Kandidat → erneut validieren.
   Max N Runden (z.B. 3), dann als „braucht manuelle Abnahme" flaggen.
6. **Emit + Register.** Sobald ein voller `send → echte Antwort`-Zyklus PASST:
   `selectors/<id>.json` schreiben, Config-Eintrag (`BRAIN_TABLE`/`brains()`)
   ergänzen, Profil am kanonischen Ort anlegen. Unsichere Schlüssel im Report
   markieren. Ein abschließender `webagent diagnose --brain <id>` als Gegencheck.

## 4. Wiederverwendete Bausteine (nichts neu erfinden)

- `browser.rs`: `send_generic`/`wait_response`/`verify_submitted`/`composer_contains`
  = die Validierungsmaschine.
- `JS_SEL_PRELUDE` (`Q`/`QA`): löst auch `text=`/`:has-text`-Kandidaten auf.
- `diagnose` (`live_diagnose`): Login/Composer/Assistant/Cloudflare-Gegencheck.
- `relay --json`: Brain-Worker als Selektor-Generator (Pool implementiert Pool).
- `detect_block_banner`/`circuit_breaker`: Rate-Limit beim Testen sauber flaggen.

## 5. Was `/aa` NICHT automatisch löst (ehrliche Grenzen)

- **CAPTCHA / Bot-Detection** beim Login → bleibt menschlich (Policy: nie umgehen).
- **Exotische Editoren** (kimi-Lexical brauchte `execCommand`-Fill) → wenn der
  generische Fill scheitert, FAIL + Hinweis auf Sonderbehandlung, kein Blind-Add.
- **Sich ändernde UIs** → `/aa` ist ein Add-/Repair-Tool, kein Dauer-Monitor; für
  Drift gibt es `diagnose` + Selector-Hardening-Test.

## 6. Minimal-Umsetzung (Reihenfolge)

1. `dom_harvest()` (JS-Eval → Kandidaten-JSON), rein testbar gegen `mock_page`.
2. `validate_selectors()` (fährt einen Schlüsselsatz live, PASS/FAIL) —
   wiederverwendet `send`/`wait_response`.
3. `generate_candidates()` (relay an einen Brain mit few-shot Prompt).
4. Orchestrierung `analyze_add()` = harvest → generate → validate → repair → emit.
5. CLI `aa` + REPL `/aa` (Wiring durch den Integrator).
6. Unit-Tests: Harvest-Parsing, Repair-Abbruch nach N, Emit schreibt gültiges JSON.

## 7. Offene Fragen

- Reicht ein einziger Brain als Generator, oder Ensemble (2 Brains schlagen
  Kandidaten vor, Validierung entscheidet)? — Start: einer, bei FAIL zweiter.
- Sollen unsichere Selektoren committet oder erst nach Mensch-Abnahme? — Default:
  schreiben, aber `"_needs_review": ["send_button"]` markieren.
