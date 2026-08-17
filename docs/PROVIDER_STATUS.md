# Provider-Status (Live-Verifikation Rust-Port)

> **Referenz.** Zahlen nachmessen — kein Soll-Zustand. Betrieb: docs/OVERVIEW.md,
> TUI: AGENTS.md §6.

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

## Capability-Proof Phase 8: Erster echter `verify`-Lauf über alle 9 Brains (2026-08-09)

`webagent verify --brain <id>` im Debug-Build (`target/debug/webagent.exe`), pro Brain
**ein eigener Prozess**, sonst Fallen (siehe „Lauf-Artefakte"). Standard-Katalog = 16
Fähigkeiten; gemessen wird nur, wofür der Harness Selektor-Proofs konfiguriert hat,
alles andere bleibt Quest (kein Record). Belegbasis: `%LOCALAPPDATA%\webagent\data\capability\proofs.jsonl`.

Kriterien (§5/§10): `Passed` = Beleg aus JS-Roundtrip oder Generation-Sequenz;
`Failed` = Fähigkeit im Spiel, aber kein Beleg; `Unreachable` = nicht anfahrbar.

| Brain | chat | new_chat | stop_generation | Toggles | projects |
|---|---|---|---|---|---|
| chatgpt¹ | **Passed** 121 s (count>baseline) | Failed 0,8 s (kein URL-Wechsel) | Unreachable (Klick ohne belegbare Wirkung) | model_switch Passed² | **Passed** 1,5 s |
| claude | **Passed** 3,1 s | Failed 0,8 s | **Passed** 0 ms (Induced) | model_switch Passed | **Passed** 1,7 s |
| deepseek | **Passed** 9,6 s | Failed 0,3 s | Failed (Stop-Button nie sichtbar) | reasoning_toggle, web_search je **Passed** (je 30 s) | – |
| gemini | **Passed** 138,6 s (stop sichtbar) | Failed 0,3 s | Unreachable | – | – |
| kimi | **Passed** 16,5 s | Failed 0,7 s | Failed (Stop-Button nie sichtbar) | model_switch Passed | – |
| mistral | **Passed** 1,7 s | Failed 0,8 s | **Passed** 0 ms (Induced) | – | – |
| perplexity | Quest (keine Selektor-Config) | Quest | Quest | – | **Passed** 1,6 s (URL-Wechsel) |
| qwen | **Passed** 15,2 s | Failed 0,4 s | **Passed** 0 ms (Induced) | model_switch Passed | – |
| zai | **Passed** 17,4 s | Failed 0,9 s | Failed (Stop-Button nie sichtbar) | reasoning_toggle, web_search, model_switch je **Passed** (je ~1,7 s) | – |

¹ chatgpt-Daten aus Lauf 1 (09:19, erster Brain in der Sequenz, gültig); der Einzel-Retry
schlug mit `start_failed` fehl (siehe Artefakte).
² model_switch chatgpt: „Rückweg misslungen — Oberfläche stand auf overlay statt aus".

**Kernbefunde**

1. **`chat` ist auf allen 9 Brains belegt** — immer über `count>baseline` (bei gemini
   über `stop sichtbar`). Der Dreier-ODER des Generation-Polls funktioniert; das ist die
   Kernfähigkeit des Harness.
2. **`new_chat` scheitert überall gleich:** kein URL-Wechsel auf irgendeinem Brain
   (0,3–0,9 s). Der §5-Beleg „URL-Paar ändert sich" greift nirgends — entweder sind die
   SPA-URLs beim neuen Chat stabil oder der Selektor trifft das falsche Element. Das ist
   ein Katalog-/Selektor-Thema, kein Brain-Defekt; offen für §13.
3. **`stop_generation`:** 3× **Passed** (qwen, mistral, claude — „Stop geklickt,
   verschwunden, Text eingefroren", erste *Induced*-Beweise), 3× Failed weil der
   Stop-Button im Probe-Fenster nie auftauchte (deepseek, kimi, zai — Antwort kam, aber
   kein Stop angeboten oder zu spät), 2× Unreachable (chatgpt, gemini — Klick kam an,
   aber keine belegbare Wirkung).
4. **Toggles:** zai 3/3, deepseek 2/2 (je 30 s — der langsamste Toggle-Pfad), claude,
   kimi, qwen, chatgpt je model_switch Passed. Alle Passed-Beweise mit „Ausgangszustand
   wiederhergestellt" außer chatgpt.

**Lauf-Artefakte (nicht als Brain-Defekt werten)**

- Lauf 1 (alle 9 Brains in einem Prozess) meldete für claude/deepseek/kimi je
  16× `Unreachable (start_failed)`. Im Einzellauf funktionieren alle drei — die
  `start_failed`-Serie war Ressourcen-/Reihenfolge-Artefakt des 9-Brain-Prozesses.
  Verify-Läufe daher **pro Brain starten**.
- Der chatgpt-Einzel-Retry (nach ~8 parallelen WebView-Sitzungen in 20 min) schlug mit
  `start_failed` fehl — vermutlich Bot-Detection/Rate-Limit; Run-1-Daten bleiben gültig.
- `mistral`/`zai` hingen im Sammelprozess ohne Output — die Ursache war
  stdout-Blockpufferung bei Datei-Redirect (Verlust am Prozessende) plus 45-s-CDP-Timeouts
  bei langsamem WebView. Fix: `cmd_verify` flusht nach Header, Brain-Start und jeder
  Ergebniszeile (`src/commands/ops.rs`). Danach laufen beide Brains in <1 min durch.
- Build-Falle: ohne MSVC-`link.exe` baut das Projekt über die **gnu**-Toolchain
  (`rustup override set stable-x86_64-pc-windows-gnu` im Projektverzeichnis; `.cargo/config.toml`
  hat dafür `link-self-contained=yes`).

## Nachtrag zu Kernbefund 2: `new_chat` — Ursache gefunden und behoben (2026-08-09)

Der Befund oben („kein URL-Wechsel auf irgendeinem Brain") stimmte, die beiden
dort genannten Verdächtigen aber nicht: weder waren die SPA-URLs stabil, noch
traf der Selektor das falsche Element. Bei chatgpt (`a[href='/']`), claude
(`a[href='/new']`), mistral (`a[href='/chat']`) und zai
(`#sidebar-new-chat-button`) wurde der Knopf jeweils sauber getroffen.

**Die Ursache war die Position in der Sequenz.** `get_conversation_ref` ist
schlicht `driver.current_url()` (`browser/operations.rs`). Die Verify-Sitzung
startet auf `brain_url` — und die konfigurierten Wurzel-URLs *sind* bereits der
neue Chat (`claude.ai/new`, `chatgpt.com/`, `chat.deepseek.com/`, `chat.z.ai/`).
Es gab nichts zu verlassen, also konnte sich die URL nicht ändern. Der Beleg
wurde zum einzigen Zeitpunkt erhoben, an dem er unmöglich ist.

**Behoben** in `src/browser/verify.rs`:

- `new_chat` wird **zuletzt** belegt, nach der Probe — dann existiert eine
  Konversation mit eigener URL, und der Wechsel ist echt.
- Davor ein **unbewerteter** Hygiene-Klick, aber nur wenn `assistant_count() > 0`
  (also wenn die Oberfläche doch ein Gespräch wiederhergestellt hat). Sonst
  entfällt er.
- Der frühe `return` bei `new_chat`-Fehler ist entfallen: als letzter Schritt
  hängt nichts mehr daran.
- Nebeneffekt: das Konto bleibt nach dem Lauf auf einem leeren Chat stehen
  statt im Probe-Gespräch.

**Bestätigt** (`verify --brain claude --cap chat --cap new_chat --cap stop_generation`):

```
chat            = Passed  (141352 ms) — chat belegt (stop sichtbar)
stop_generation = Unreachable         — Stop-Klick ohne belegbare Wirkung
new_chat        = Passed  (1048 ms)   — URL-Wechsel
```

Zwei Mechanismen haben sich dabei mitbewiesen: der `chat`-Beleg griff über den
Trigger „stop sichtbar", nicht über den Zähler — genau der Zweig, den ein Beleg
allein auf `assistant_count` verloren hätte. Und `stop_generation` kam als
`Unreachable`, entzog dem früheren `Passed` aber nichts, weil `proof_state`
`Unreachable` überspringt.

**Ebenfalls behoben:** ein `verify`-Lauf, der ausschließlich `Unreachable`
liefert, meldet das jetzt laut auf stderr und endet mit Exitcode ≠ 0
(`src/commands/ops.rs`). Anlass war der Lauf mit 128 von 195 `start_failed`, der
im Store unauffällig aussah — „fertig" darf nicht wie „geprüft" aussehen.

### Breitentest, ein Prozess pro Brain (2026-08-09, 16:53–17:06)

`verify --brain <id> --cap chat --cap new_chat --cap stop_generation`, sequenziell.

| Brain | chat | stop_generation | **new_chat** |
|---|---|---|---|
| claude | Passed 141,4 s (stop sichtbar) | Unreachable (Klick ohne Wirkung) | **Passed** 1,0 s |
| chatgpt | Passed 121,7 s (stop sichtbar) | Unreachable (Klick ohne Wirkung) | **Passed** 0,9 s |
| zai | Passed 18,4 s (count>baseline) | Failed (Stop nie sichtbar) | **Passed** 0,9 s |
| deepseek | **Failed** 108,3 s (Timeout) | Failed (Stop nie sichtbar) | **Passed** 3,7 s |
| qwen | Passed 16,9 s (count>baseline) | Unreachable (Klick ohne Wirkung) | Failed **0,034 s** |
| kimi | Passed 20,6 s (count>baseline) | Failed (Stop nie sichtbar) | Failed **0,035 s** |
| gemini | Passed 137,8 s (stop sichtbar) | Unreachable (Klick ohne Wirkung) | Failed 0,845 s |
| mistral | Unreachable (start_failed) | Unreachable | Unreachable |

**`new_chat`: von 9× Failed auf 4 Belege.** Die Positionskorrektur trägt für
claude, chatgpt, zai und deepseek.

**Die drei verbliebenen Fehlschläge sind zwei verschiedene Dinge.** `new_chat()`
(`browser/backend.rs:128-141`) klickt entweder `new_chat_button` — dann folgen
**800 ms Schlaf** — oder navigiert ersatzweise zur Start-URL. qwen (34 ms) und
kimi (35 ms) können also **keinen** dieser Pfade durchlaufen haben; dort greift
der Selektor nicht und der Ersatzweg tut nichts Messbares. gemini dagegen
brauchte 845 ms: ein echter Versuch, der die URL nicht bewegt hat —
`gemini.google.com/app` bleibt `/app`. Das ist der Fall, für den das
URL-Kriterium tatsächlich nicht ausreicht.

Nächster Schritt getrennt nach Fehlerart: qwen/kimi brauchen einen greifenden
`new_chat_button` (Aufgabe für `brain_probe`), gemini ein zweites Kriterium.

**Offene Regression:** deepseek `chat` lief in dieser Runde in den Timeout
(108 s), war im Lauf davor aber `Passed` (9,6 s). Ursache ungeklärt —
Flakiness oder Nebenwirkung des Hygiene-Klicks. Vor weiteren Schlüssen
wiederholen.

**Der Exitcode-Fix hat beim ersten Einsatz gegriffen:** mistral endete mit
3× `start_failed` und meldete „KEIN EINZIGER BELEG … dieser Lauf hat nichts
gemessen" plus Exitcode 1. Genau der Fall, der vorher stumm durchgelaufen wäre.

### qwen + kimi: falscher Elementtyp, plus eine Schattendatei (2026-08-10)

`webagent probe --brain <id>` (nur Scan, kein `--write`, keine Nachricht) hat
die Ursache der 34/35-ms-Fehlschläge geliefert — in beiden Fällen stimmte der
**Elementtyp** nicht:

| Brain | tatsächlich im DOM | war konfiguriert |
|---|---|---|
| qwen | `div[aria-label*='New Chat' i]` | nur `button[…]` und `a[href='/']` |
| kimi | `a[aria-label*='Neuer Chat' i]` | u.a. `button[aria-label*='new' i]` |

Beide Male zielten alle hinterlegten Selektoren auf `button` bzw. `a`, das reale
Element ist ein `div` (qwen) bzw. ein `<a>` mit **deutschem** Label (kimi).
Deshalb lieferte `click_first` sofort `false`, und der Ersatzweg von `new_chat()`
tat nichts Messbares — daher die unmöglich kurzen 34/35 ms.

Selektoren ergänzt (gemessener Wert zuerst, alte als Fallback), Ergebnis:

```
qwen: new_chat = Passed (1092 ms) — URL-Wechsel
kimi: new_chat = Passed (1040 ms) — URL-Wechsel
```

**Der Zwilling hat dabei zugeschlagen.** qwen war nach der Repo-Änderung sofort
grün, kimi blieb bei 33 ms — weil
`%LOCALAPPDATA%\webagent\selectors\kimi.json` (vom 05.08.) die Repo-Datei
überschattet. Genau die Falle aus `UEBERGABE_2026-07-28.md`: *„Greift ein
verifizierter Fix live nicht, NICHT die Logik weiter ändern — erst nach dem
Zwilling suchen."* Und genau das, wovor der Kommentar in `commands/ui.rs`
warnt: ein `probe --write` friert den mitgelieferten Stand lokal ein, „spaetere
Pflege im Repo erreicht diese Maschine nie wieder".

Die Nutzerdatei enthielt inhaltlich **nur einen** eigenen Schlüssel
(`attach_button`), sonst den eingefrorenen Altstand. `new_chat_button` wurde
angeglichen (Backup: `kimi.json.bak-20260810`).

**Umfang der Schattendateien** (nachgemessen, meine erste Einschätzung war zu
pauschal — `load_selectors` merged **pro Schlüssel**, überschattet also nur, was
die Nutzerdatei tatsächlich führt):

| Nutzerdatei | Schlüssel | Wirkung |
|---|---|---|
| `kimi.json` | 12 (u.a. `assistant_message`, `composer`, `model_menu`) | überschattet fast alles — hier war die Repo-Pflege wirkungslos |
| `gemini.json` | 1 (`ui_options`) | überschattet nur den Nenner; Selektorpflege im Repo greift |
| `perplexity.json` | 9 | **kein Repo-Gegenstück** — die Datei ist die einzige Quelle, kein Schatten |

Also nur kimi war betroffen. Für perplexity ist die Nutzerdatei sogar die
einzige Konfiguration; sie gehört ins Repo, wenn perplexity dauerhaft
mitlaufen soll.

### `new_chat`: 8 von 8 belegt (2026-08-10, Abschluss)

gemini und mistral nachgezogen — beide Male war die Diagnose **falsch**, die
ich zuvor notiert hatte:

- **gemini** galt als „echter Fall ohne URL-Wechsel, braucht ein zweites
  Kriterium". Tatsächlich stand `a[href='/']` an dritter Stelle der Liste,
  traf ein anderes Element (vermutlich das Logo) und wurde geklickt — daher
  862 ms *mit* Klick und *ohne* Wirkung. Nach Voranstellen des gemessenen
  `a[aria-label*='Neuer Chat' i]` und Verschieben von `a[href='/']` ans Ende:
  `Passed — URL-Wechsel + Verlauf geleert (1 -> 0)`. gemini wechselt die URL
  sehr wohl.
- **mistral** war nie kaputt: der `start_failed` vom Vortag war flüchtig.
  Einzellauf: `Passed — URL-Wechsel + Verlauf geleert (1 -> 0)`.
- **deepseek `chat`** (108-s-Timeout im Breitentest) ebenfalls flüchtig:
  Nachlauf `Passed` nach 11,4 s. Keine Regression durch den Hygiene-Klick.

Damit dieselbe Ursache bei **vier** Brains (qwen, kimi, gemini + der
ursprüngliche Positionsfehler): zu enge oder zu generische Selektoren, nie ein
konzeptionelles Problem des URL-Kriteriums.

**Zweites Kriterium trotzdem eingebaut** (`new_chat_outcome` in
`browser/verify.rs`): Beleg gilt bei URL-Wechsel **oder** geleertem Verlauf
(`count_before > 0 && count_after == 0`). Die Schranke `count_before > 0`
verhindert, dass ein ohnehin leerer Verlauf (0 → 0) jeden wirkungslosen Klick
belegt. Beide Zweige werden im `evidence` getrennt ausgewiesen, damit sichtbar
bleibt, welches Kriterium trägt — sonst verdeckt ein blankes ODER beginnende
Drift. Ehrlichkeitshalber: die Hypothese, die diesen Zweig motiviert hat, war
falsch; er bleibt als unabhängiges Zweitsignal, nicht als der Fix.

**Stand:** `chat` 8/8, `new_chat` 8/8 (perplexity bleibt Quest — keine
Chat-Selektoren konfiguriert).

### `stop_generation`: Teilerfolg und zwei offene Befunde (2026-08-10)

Dieselbe Hypothese wie bei `new_chat` — alle `stop_button`-Selektoren der vier
Fehlschläge waren **ausschließlich `button`-verankert**, kein einziger
tag-agnostischer. Ergänzt um `[aria-label*='stopp' i]` und
`[aria-label*='stop' i]` (Repo; bei kimi zusätzlich die Nutzerdatei).

| Brain | vorher | nachher |
|---|---|---|
| zai | Failed „nie sichtbar" | **Unreachable** „Klick ohne belegbare Wirkung" |
| gemini | Failed/Unreachable | Unreachable „Klick ohne belegbare Wirkung" |
| deepseek | Failed „nie sichtbar" | unverändert |
| kimi | Failed „nie sichtbar" | unverändert |

**zai ist einen Schritt weiter:** der Knopf wird jetzt gefunden und geklickt,
nur die Wirkung (`stop weg` UND `Text eingefroren`) ist nicht belegbar. Das ist
ein anderes Problem als ein toter Selektor.

**deepseek und kimi sind mit aria-label nicht erreichbar.** Für deepseek
dokumentiert der Modulkopf von `capability.rs` den Grund: 107 Bedienelemente
als reine Icon-`div`s ohne aria-label, title, id, `data-*` oder Text. Dort hilft
kein Label-Selektor, sondern nur Klassen-/Strukturanalyse — und die braucht
einen DOM-Abzug **während** der Generierung. Ein statischer `probe`-Scan sieht
den Stop-Knopf nie, weil es ihn im Ruhezustand nicht gibt.

**Reihenfolge-Interaktion bei gemini (offen).** `new_chat` hängt davon ab, ob
`stop_generation` im selben Lauf geprüft wurde:

```
ohne --cap stop_generation:  Passed  "URL-Wechsel + Verlauf geleert (1 -> 0)"   2×
mit  --cap stop_generation:  Failed  "1 -> 1"                                   3×
```

Das ist die unangenehmste Sorte Fehler: der **volle** Lauf liefert ein
schlechteres Ergebnis als ein gezielter Teillauf. Ein Setzenlassen von 1,5 s vor
dem Neu-Chat-Klick hat **nichts** geändert — die Vermutung „Generierung läuft
noch" ist damit widerlegt (der Poll war ohnehin bis zum 137-s-Deadline
gelaufen). Wahrscheinlicher hinterlässt der Stop-Klick bei gemini einen
DOM-Zustand, in dem der Neu-Chat-Anker nicht mehr trifft.

Der Workaround wurde wieder **entfernt**: eine Änderung ohne Wirkungsnachweis
gehört nicht in dieses Modul. Der Befund steht als Kommentar an der Fundstelle
in `browser/verify.rs`.

**Nächster Schritt für beide offenen Punkte ist derselbe:** ein DOM-Abzug
während bzw. direkt nach der Generierung. Ohne den bleibt jede weitere
Selektor-Änderung Raten — genau der Modus, vor dem `UEBERGABE_2026-07-28.md`
warnt („NICHT die Logik weiter ändern — erst nach dem Zwilling suchen").

### `probe --generating` gebaut, und ein totes Feld gefunden (2026-08-10)

Neu: `webagent probe --brain <id> --generating` sendet eine Probe, wartet bis
die Antwort **nachweislich läuft** (Zähler wächst oder Text ändert sich — der
Stop-Zweig fehlt hier bewusst, der gesuchte Knopf darf nicht Voraussetzung
sein) und scannt erst dann. Bricht mit Fehler ab, wenn kein Antwortsignal
kommt: ein Scan im Ruhezustand wäre wertlos und würde als „Stop-Knopf nicht
gefunden" missverstanden.

**Dabei ein echter Bug aufgefallen:** `PROBE_SCRIPT` sammelte weder `class` noch
`title` ein. Beide Felder sind in `Candidate` deklariert **und ausdrücklich als
wichtig dokumentiert** — „`title` … oft die einzige Beschriftung eines
Icon-Buttons", `class` „fuer Discovery von Bedienelementen ohne Text/role".
`#[serde(default)]` füllte sie still mit Leerstring. Damit war der Rohabzug bei
genau den Oberflächen blind, für die er gedacht war. Behoben; `--dump` zeigt
jetzt zusätzlich `title` und `cls` und filtert auf **sichtbare** Kandidaten
(deepseek: 110 von 212).

**Ergebnis für deepseek — negativ, aber belastbar.** Der Abzug während der
Generierung zeigt die Icon-Buttons endlich mit Klassen:

```
<div role=button al="" txt="" title="" tid="" id=""
     cls="ds-button ds-button--iconLabelTertiary ds-button--icon ds-button--capsule ds-button--m …"
```

Genau wie `capability.rs` es beschreibt — und die Klassen sind untereinander
**identisch**. Der Stop-Knopf ist an keinem gesammelten Attribut von den übrigen
Icon-Buttons zu unterscheiden. kimi und zai liefern während der Generierung
ebenfalls keinen `stop_button`-Vorschlag, weil die Muster des Klassifizierers
label- und textbasiert sind.

**Damit ist die Grenze benannt statt geraten:** `stop_generation` für deepseek,
kimi und zai braucht Unterscheidung über SVG-Inhalt, Position oder Elternkette —
nicht über Label, Text, id, testid, title oder Klasse. Das ist ein eigenes
Stück Arbeit, kein Selektor-Nachtrag.

### deepseek `stop_generation` belegt — über das Verschwinden (2026-08-10)

Der Stop-Knopf hat ein Merkmal, das kein Attribut ist: **er existiert nur,
solange die Antwort läuft.** Neu: `probe --brain <id> --stop-diff` scannt
während der Generierung und danach und meldet, was nur währenddessen da war.
Verglichen wird über Lage und Größe — bei Oberflächen ohne Label ist das das
Einzige, was zwei Elemente unterscheidbar macht (dafür sammelt `PROBE_SCRIPT`
jetzt `x/y/w/h`).

Für deepseek engte das **110 sichtbare Kandidaten auf zwei** ein:

```
(1081,168) 28x28  cls="… ds-button--xs … db183363"
(1119,168) 28x28  cls="… ds-button--xs … d4910adc"
```

Einzeln durchgetestet: `div.d4910adc` ist der Stop-Knopf.

```
deepseek: stop_generation = Passed — Stop geklickt, verschwunden, Text eingefroren
```

Reproduziert; im selben Lauf sind `chat` und `new_chat` ebenfalls Passed.

**Zwei eigene Fehler auf dem Weg**, beide vom Messwert aufgedeckt:
`PROBE_SCRIPT` kürzte den Klassenstring auf 120 Zeichen — genau drei Zeichen zu
früh, sodass aus `db183363` ein `db183` wurde und der Selektor ins Leere lief.
Limit auf 400 erhöht. Und der erste Kandidat (`db183363`) war der falsche der
beiden; erst `d4910adc` trägt.

⚠ `db183363`/`d4910adc` sind CSS-Modul-Hashes und überleben ein Redeploy von
deepseek vermutlich nicht. Das ist vertretbar, weil genau dieser Fall
abgesichert ist: ändert sich die Selektordatei, entwertet der Hash den Beleg;
greift der Selektor nicht mehr, meldet der nächste `verify` ein `Failed`.

### Wo die Methode nicht greift: kimi und zai

- **kimi**: 56 sichtbare Elemente während wie nach der Generierung, kein
  Verschwinden und keine Änderung an gleicher Stelle. Deckt sich mit „Stop-Button
  nie sichtbar" aus dem Beleg-Lauf, der alle 300 ms pollt. Entweder erscheint
  der Knopf dort nie — dann ist `stop_generation` in kimis `ui_options` zu
  Unrecht deklariert — oder kürzer als ein Poll-Intervall.
- **zai**: 21 sichtbare Elemente während, 203 danach. zai blendet während der
  Antwort fast die ganze Oberfläche aus. Ein Bereitschafts-Kriterium (zwei
  aufeinanderfolgende Abzüge mit gleicher Anzahl) ändert daran nichts — die 21
  sind stabil. Der einzige Unterschied ist der Upload-Knopf, also nicht der
  gesuchte.

Für beide bleibt nur SVG-Inhalt oder Elternkette. **Nicht weiter geraten.**

**Records:** `proofs.jsonl` zählt nach Abschluss 162 Belege über alle 9 Brains
(akkumuliert über alle Läufe inkl. Artefakt-Runs).
