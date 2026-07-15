# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0
**Letzte Messung:** 2026-07-15 — `webagent diagnose --brain <id> --headless`,
Profil `data/profiles/shared`, Release-Build.

## Stand: 7/8 eingeloggt — aber Relay liefert noch leere Antworten

`webagent diagnose --brain <id> --headless`, Profil `data/profiles/shared`:

| `webagent/<id>` | session_state | logged_in | composer | cloudflare |
|---|---|---|---|---|
| chatgpt | Ready | true | FEHLT¹ | false |
| deepseek | Ready | true | ok | false |
| kimi | Ready | true | ok | false |
| gemini | Ready | true | ok | false |
| qwen | Ready | true | ok | false |
| **claude** | **LoginRequired** | **false** | FEHLT | false |
| mistral | Ready | true | ok | false |
| zai | Ready | true | ok | false |

¹ chatgpt: `logged_in` kommt über `new_chat_button` (3–4 Treffer). Der Composer matcht
laut `cargo run --example inspect -- chatgpt` sehr wohl (`composer: 2`), nur nicht zum
Sampling-Zeitpunkt von `live_diagnose` — ein Timing-Rennen, keine Selektor-Drift.

**Offen:** `relay` kommt sauber durch `ensure_ready` + `send`, aber `wait_response`
liefert leeren Text (exit 0, 0 Zeichen Antwort). Das ist die nächste Baustelle;
solange das so ist, ist kein Provider wirklich nutzbar, auch die grünen nicht.

**Einzige Nutzeraktion:** `claude` braucht einen manuellen Login (siehe unten).

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

### Zwei Bugs, die alles maskiert haben (beide 2026-07-15 gefixt)

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
