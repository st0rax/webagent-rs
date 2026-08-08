# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`)
**Letzte Messung:** 2026-07-16 — `webagent relay --brain <id> --headless`,
Profil `data/profiles/shared`, Release-Build.

## Stabilität: 5 Runden × 8/8 in Folge (2026-07-16), dann qwen-Tageslimit

Ziel war 8/8 in 10 Runden hintereinander. Erreicht wurden **zweimal 5 volle Runden
8/8 = 40/40 Relays** mit strengem Kriterium (Antwort muss echt „OK" enthalten, nicht
nur exit 0). Ab Runde 6 fiel **qwen** aus — nicht wegen eines Bugs, sondern weil
sein **Account-Tageslimit** erreicht war (wörtliche Antwort: „You have reached the
daily usage limit. Please wait 7 hours before trying again."). Die umfangreiche
Messung des Tages (>100 qwen-Aufrufe) hat die Quote aufgebraucht; sie setzt sich
nach ~7 h zurück. Die vollen 10 Runden 8/8 brauchen also entweder den Quoten-Reset
oder einen ruhigeren Tag.

Härtungen aus dieser Runde (v0.8.1), alle allgemein, kein Brain-Sonderfall:
- **Bestätigtes Füllen:** `send_generic` sendet erst, wenn der Text nachweislich im
  Editor steht (`composer_contains`) — nicht mehr blind.
- **Voller Turn-Retry im Relay:** bis zu 3 Anläufe (new_chat + send + wait), deckt
  auch die Antwort-Erkennungs-Flakiness ab, nicht nur das Submit. Rate-Limit
  ausgenommen, Retries sichtbar auf stderr.
- **Rate-Limit-Erkennung nur für claude:** `is_claude_limit_response_text` lief auf
  allen acht Brains und meldete für qwens „…limit…"-Text fälschlich
  `claude_rate_limited` (terminal, ohne Retry). Jetzt claude-spezifisch, per
  Regressionstest festgenagelt. qwens echtes Limit wird nun ehrlich als seine
  Antwort durchgereicht statt fehlattribuiert.

## Stand: 8/8 antworten headless

Gemessen mit `webagent relay --brain <id> --message "Antworte nur mit dem Wort OK."
--timeout 60 --headless`, Profil `data/profiles/shared`. **Echte Antworten, keine
Exit-Codes.** Zwei volle Runden hintereinander, beide 8/8:

| `webagent/<id>` | Relay | Dauer (R1 / R2) |
|---|---|---|
| chatgpt | 🟢 PASS | 11,8s / 10,7s |
| deepseek | 🟢 PASS | 8,7s / 10,8s |
| kimi | 🟢 PASS | 18,1s / 38,7s |
| gemini | 🟢 PASS | 10,5s / 14,1s |
| qwen | 🟢 PASS | 12,6s / 15,0s |
| claude | 🟢 PASS | 12,5s / 10,4s |
| mistral | 🟢 PASS | 20,2s / 15,5s |
| zai | 🟢 PASS | 17,9s / 20,6s |

kimis längere Läufe (~38s) sind die, in denen der erste Sende-Anlauf scheiterte und
der Relay-Retry griff (siehe unten).

**Flakiness:** qwen und zai fielen früher gelegentlich mit `timeout_no_text` bzw.
`timeout_no_message` durch und gingen bei direkter Wiederholung durch. Der
Sende-Retry unten fängt einen guten Teil davon ab.

### kimi: von „nie" über „jedes zweite Mal" auf **8/8** (2026-07-16)

kimi ging vor dem WebView-Fix **nie** durch, danach exakt jeden zweiten Lauf.
Ursache — Schritt für Schritt am DOM nachgemessen, nicht geraten:

- Ein erfolgreicher Lauf navigierte auf `kimi.com/chat/<id>`, ein fehlschlagender
  blieb auf `kimi.com/` — die Nachricht ging nie raus.
- In **allen** Fällen meldete `send()` aber `ok=true`. `verify_submitted` wertete
  einen **leeren Composer** als „abgeschickt" — dabei war das Feld leer, weil das
  Füllen von kimis **Lexical-Editor** still fehlschlug (`textContent=…` rendert
  sichtbar, aber Lexical verwirft es beim Reconcile, und Enter schickt nichts ab).

Drei aufeinander aufbauende Fixes:

1. **`verify_submitted` verlangt einen echten Absende-Beweis** — URL-Wechsel,
   Stop-Button oder wachsender Assistant-Zähler. Ein leerer Composer allein zählt
   nicht mehr. Damit meldet `send()` bei nicht abgeschickter Nachricht ehrlich
   `false` (statt Fehlalarm + 125 s stiller Timeout) und die Schleife sendet erneut.
2. **`fill_composer` nutzt `execCommand('insertText')`** als Fallback statt rohem
   `textContent` — das feuert `beforeinput`/`input`, die Lexical/ProseMirror als
   echte Eingabe verarbeiten.
3. **Relay-Send-Retry:** schlägt `send` fehl, wurde nachweislich nichts gepostet,
   also ein zweiter `new_chat` + `send` — gefahrlos, kein Doppel-Post. Das hebt kimi
   von ~75 % auf **8/8** (24/24 in den Abschlussläufen).

Kein kimi-Sonderfall im Code — alle drei Änderungen sind allgemein und verbessern
auch die Ehrlichkeit/Robustheit der anderen Brains.

### Frühere Fehldiagnosen zu kimi (waren falsch)

Frühere Einträge behaupteten nacheinander: kimi sei nicht eingeloggt; die Sidebar
blockiere; ein `.login-modal-content` blockiere den Versand. **Alles falsch.**
Selektor-genau nachgemessen ist kimi eingeloggt (Avatar/User-Info sichtbar),
`nav[class*='sidebar']` matcht gar nicht, und das Login-Modal ist ein verstecktes
Overlay. Das echte Problem war das Lexical-Füllen — siehe oben.

### Bekannte Ungenauigkeit: `logged_in` war zu optimistisch

`is_logged_in()` war true, sobald **eines** von `login_indicator`, `composer` oder
`new_chat_button` sichtbar war — der Composer beweist aber nur „Seite geladen",
nicht „angemeldet". Seit 2026-07-15 gilt: ist `login_indicator` konfiguriert,
entscheidet **nur** der; Composer/New-Chat sind Fallback für Brains ohne den Key.
Alle acht Brains konfigurieren ihn, daher ist die Änderung **heute
verhaltensneutral** (vorher wie nachher: 8× `logged_in: true`) — sie beseitigt
eine latente Maskierung, keinen aktuellen Fehler.

**Der Relay bleibt die verlässliche Messung, nicht `diagnose`.**

### Playwright-Selektoren: seit 2026-07-15 **lebendig** statt wirkungslos

**96 von 283** Einträgen in `selectors/*.json` waren `:has-text(…)` / `text=…` —
Playwright-Syntax aus der Python-Referenz. `document.querySelector` wirft darauf,
`js_scan` schluckte die Exception pro Selektor: sie waren stumm wirkungslos.
**Acht Keys bestanden ausschließlich daraus**, ihr Feature konnte nie feuern:

| Brain | Key | tot |
|---|---|---|
| gemini | `login_button` | 7/7 |
| zai | `login_button` | 7/7 |
| gemini | `response_preference_prompt` | 4/4 |
| qwen | `login_button` | 4/4 |
| deepseek | `login_button` | 3/3 |
| qwen | `consent_reject_button` | 3/3 |
| gemini | `consent_reject_button` | 2/2 |
| gemini | `notice_close_button` | 2/2 |

`js_scan` bringt jetzt ein Prelude mit: `Q(sel)`/`QA(sel)` verstehen `text=foo`,
`text=/re/i` und `base:has-text('x')` und fallen sonst auf `querySelector*` zurück.
Bei Textmatches werden nur die **innersten** Treffer geliefert (sonst matcht jeder
Vorfahr bis `<body>`). Gegen ein bekanntes DOM verifiziert: 10/10 Proben, inkl.
„normales CSS bleibt unverändert".

Zwei Keys sind weiterhin **konfiguriert, aber vom Code nirgends gelesen**:
`response_preference_prompt` (gemini) und `dialog_dismiss_button` (mistral) — tote
Config, kein toter Selektor.

## Warum die frühere Tabelle falsch war

Der vorige Stand behauptete **5/8 headless PASS** (deepseek/kimi/gemini/qwen/zai) und
für chatgpt/claude/mistral „Cloudflare blocks headless". Beides ist falsch — aber
anders, als es zuerst aussah:

1. **`cloudflare: false` bei allen acht.** Es wird nie eine Cloudflare-Challenge
   ausgelöst. Die „needs manual login (Cloudflare)"-Notiz war eine Fehldiagnose;
   mistral und chatgpt sind eingeloggt und erreichbar.
2. **Die Logins waren die ganze Zeit da.** Sie waren nur unsichtbar, weil jede
   DOM-Abfrage leer zurückkam (siehe Bug 2).
3. Der Smoke (`delivery/provider_webview_smoke.ps1`) wertet
   `$code -eq 0 -and ($out -match "OK|ok")` und zählt zusätzlich jedes `exit 0` als
   `PASS(exit0)`. Eine **leere Antwort mit exit 0 zählt damit als Erfolg** — genau der
   Zustand, in dem sich der Relay bis heute befindet. Daher „5/8 PASS" ohne eine
   einzige echte Antwort.

### Drei Bugs, die alles maskiert haben (alle 2026-07-15 gefixt)

1. **WebView war komplett tot.** `tao` panicte bei jedem Seitenaufruf
   („Initializing the event loop outside of the main thread"), weil `run_event_loop`
   bewusst im dedizierten `webagent-webview`-Thread läuft, aber `EventLoop::new()`
   rief. Fix: `EventLoopBuilderExtWindows::with_any_thread(true)`.
2. **Jedes `evaluate` lieferte `{}`.** Der JS-Wrapper war eine `async`-IIFE — die gibt
   ein Promise zurück, und WebView2 serialisiert ein Promise zu `{}`. Damit sah jede
   DOM-Abfrage leer aus: `logged_in=false`, `composer=FEHLT`, `assistant_msgs=0`, für
   alle Provider. Das `await` stammte aus der CDP-Zeit (`Runtime.evaluate` hatte ein
   `awaitPromise`-Flag; WebView2 nicht) — kein einziger Ausdruck im Code braucht es.
   Fix: synchrone IIFE, die das Objekt **direkt** zurückgibt (kein `JSON.stringify` —
   das erzeugt einen doppelt kodierten String). Gemessene Rohformen:

   | Skript | Roh-Rückgabe |
   |---|---|
   | `1+1` | `"2"` |
   | IIFE + `JSON.stringify` | `"\"{\\\"ok\\\":true}\""` (doppelt kodiert) |
   | IIFE, Objekt direkt | `"{\"ok\":true,\"value\":2}"` ✅ |
   | async-IIFE (alt) | `"{}"` ← der Bug |

   Abgesichert durch Regressionstests in `webview_runtime::tests`.

3. **„Headless" war ein Fenster ohne Fokus.** WebView2 kennt kein echtes Headless;
   `--headless` setzte `with_visible(false)`. Ein nie gezeigtes Fenster kann aber
   keinen **Fokus** bekommen, also landeten Tastendrücke nirgends — und bei Brains
   ohne matchenden Send-Button (deepseek: `send_button: 0`) ist `press_enter()` der
   einzige Absende-Weg. Messbar: derselbe Relay lief **headed in 9,5s mit Antwort
   „OK", headless in 114s Timeout mit leerer Antwort**. Anti-Throttling-Flags
   (`--disable-background-timer-throttling` etc.) halfen nicht — es lag nicht am
   Drosseln, sondern am Fokus. Fix: Fenster **off-screen** (`-32000,-32000`) statt
   versteckt; für Chromium ein normales, fokussierbares Fenster, für den Nutzer
   unsichtbar. Danach: headless 9,6s, Antwort `OK`.

### Gemini: echte Selektor-Drift (2026-07-15 gefixt)

Gemessen pro Selektor, direkt nach einer Antwort:

| Selektor | Treffer | Text |
|---|---|---|
| `message-content[class*='model-response-text']` | 0 | — |
| `div[class*='model-response-text']` | 0 | — |
| `div[class*='response-text']` | 0 | — |
| `div.prose` | 0 | — |
| **`div[class*='markdown']`** | **1** | **`Eins\nZwei\nDrei\nVier\nFünf`** |
| `div[class*='message-content']` | 0 | — |
| `div[class*='response']` | 12 | `[0,43,0,0,0,0,43,43,43,17,0,0]` — letztes leer |

`probe_generation` nimmt den **ersten** Selektor mit Treffern und davon das **letzte**
Element. Solange nur `div[class*='response']` matchte (12 Wrapper, letztes ohne Text),
gab es `timeout_no_text`. Die kanonischen `model-response-text`-Selektoren treffen bei
Gemini nichts mehr. Fix: `div[class*='markdown']` nach vorn, die zu breiten
`div[class*='response']`/`div[class*='message-content']` raus. Danach: 11,2s → `OK`.

## Manueller Login (claude, kimi, mistral)

`webagent login` öffnet einen **sichtbaren** Browser und wartet auf den manuellen
Login — **ohne Zugangsdaten-Eingabe durch das Tool**. Danach persistiert die Session
im WebView2-Profil (`EBWebView/` unterhalb von `WEBAGENT_PROFILE_DIR`).

```powershell
$env:WEBAGENT_PROFILE_DIR = "C:\Users\storax\Desktop\webagent\data\profiles\shared"
.\target\release\webagent.exe login --brain claude  --timeout 540
.\target\release\webagent.exe login --brain kimi    --timeout 540
.\target\release\webagent.exe login --brain mistral --timeout 540   # dabei AGB bestaetigen
```

Das Fenster heißt `webagent-<n>` und kann **hinter anderen Fenstern liegen** — per
Alt+Tab nach vorn holen. Danach verifizieren (der Relay, nicht `diagnose`):

```powershell
.\target\release\webagent.exe relay --brain claude --message "Antworte nur mit dem Wort OK." --timeout 90 --headless
```

## Bekannte Stolpersteine

1. **`WebView2Loader.dll`** muss neben `webagent.exe` liegen. Der Build legt sie nur
   unter `target/release/build/webview2-com-sys-*/out/x64/` ab und kopiert sie **nicht**
   ans Ziel. Nach jedem `cargo clean` startet die Binary sonst mit `0xC0000135`
   (DLL not found) und ohne Fehlermeldung.
2. `WEBAGENT_PROFILE_DIR` muss gesetzt sein, sonst nutzt jedes Brain ein eigenes Profil
   unter `profiles/<brain>` und der Login landet woanders als der spätere Lauf ihn sucht.

## Testkommando

```powershell
pwsh -File Desktop\webagent\delivery\provider_webview_smoke.ps1 -Headed
```

## Bekannte Stabilitäts-Fixes

1. `new_chat` vor frischem Run (Controller).
2. Composer-Fokus + DOM-fill.
3. Fenstergröße 1280×900.
4. `BrowserPool` + `WEBAGENT_PERSIST_TABS` für Relay-Ketten.
5. `with_any_thread(true)` für den WebView-EventLoop im Nicht-Main-Thread.
