# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0
**Letzte Messung:** 2026-07-15 — `webagent diagnose --brain <id> --headless`,
Profil `data/profiles/shared`, Release-Build.

## Stand: 4/8 antworten headless wirklich

Gemessen mit `webagent relay --brain <id> --message "Antworte nur mit dem Wort OK."
--timeout 90 --headless`, Profil `data/profiles/shared`. **Echte Antworten, keine
Exit-Codes.**

| `webagent/<id>` | Relay | Dauer | Antwort / Fehler |
|---|---|---|---|
| chatgpt | 🟢 PASS | 14,4s | `OK` |
| deepseek | 🟢 PASS | 12,5s | `OK` |
| qwen | 🟢 PASS | 13,9s | `OK` |
| zai | 🟢 PASS | 14,5s | `Thought Process OK` |
| kimi | 🔴 FAIL | 139,9s | `timeout_no_message` |
| gemini | 🔴 FAIL | 141,7s | `timeout_no_text` |
| mistral | 🔴 FAIL | 150,5s | `timeout_no_message` |
| **claude** | 🔴 FAIL | 19,7s | `Composer-Feld nicht gefunden` — **nicht eingeloggt** |

**Einzige Nutzeraktion:** `claude` braucht einen manuellen Login (siehe unten).
Danach ist zu erwarten, dass er wie chatgpt läuft.

**Offen (Technik, keine Nutzeraktion):** kimi, gemini und mistral senden, bekommen
aber keine erkannte Antwort. Drei verschiedene Fehlerbilder, also drei eigene
Ursachen — `timeout_no_message` heißt: kein Assistant-Container erschien;
`timeout_no_text` heißt: Container da, aber Text leer erkannt.

Login-Status separat (`diagnose --headless`): 7 von 8 sind eingeloggt, nur `claude`
nicht. `cloudflare: false` bei allen acht.

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

## Manueller Login (nur noch claude)

`webagent login` öffnet einen **sichtbaren** Browser und wartet auf den manuellen
Login — **ohne Zugangsdaten-Eingabe durch das Tool**. Danach persistiert die Session
im WebView2-Profil (`EBWebView/` unterhalb von `WEBAGENT_PROFILE_DIR`).

```powershell
$env:WEBAGENT_PROFILE_DIR = "C:\Users\storax\Desktop\webagent\data\profiles\shared"
.\target\release\webagent.exe login --brain claude --timeout 540
```

Das Fenster heißt `webagent-<n>` und kann hinter anderen Fenstern liegen.
Danach verifizieren:

```powershell
.\target\release\webagent.exe diagnose --brain claude --headless   # erwartet: logged_in true
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
