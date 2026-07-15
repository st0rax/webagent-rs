# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0
**Letzte Messung:** 2026-07-15 — `webagent diagnose --brain <id> --headless`,
Profil `data/profiles/shared`, Release-Build.

## Stand: 0/8 eingeloggt — kein Provider nutzbar

| `webagent/<id>` | session_state | logged_in | cloudflare | Landet auf |
|---|---|---|---|---|
| chatgpt | LoginRequired | false | false | `chatgpt.com/` |
| deepseek | LoginRequired | false | false | `chat.deepseek.com/sign_in` |
| kimi | LoginRequired | false | false | `www.kimi.com/` |
| gemini | LoginRequired | false | false | `gemini.google.com/app` |
| qwen | LoginRequired | false | false | `chat.qwen.ai/` |
| claude | LoginRequired | false | false | `claude.ai/logout` |
| mistral | LoginRequired | false | false | `chat.mistral.ai/chat` |
| zai | LoginRequired | false | false | `chat.z.ai/` |

## Warum die frühere Tabelle falsch war

Der vorige Stand behauptete **5/8 headless PASS** (deepseek/kimi/gemini/qwen/zai) und
für chatgpt/claude/mistral „Cloudflare blocks headless". Beides hält der Messung nicht
stand:

1. **`cloudflare: false` bei allen acht.** Es wird gar keine Cloudflare-Challenge
   ausgelöst. Die „needs manual login (Cloudflare)"-Notiz war eine Fehldiagnose.
2. **Auch die angeblich grünen fünf sind `LoginRequired`.** deepseek landet auf
   `/sign_in`. Es gibt keinen eingeloggten Provider.
3. **Ursache:** `data/profiles/shared` ist ein **Chrome**-Profil aus der Python/CDP-Ära
   (`Default/Network/Cookies`, `Local State`). Der WebView2-Backend liest das nicht —
   wry legt seinen eigenen Store unter `EBWebView/` im selben Verzeichnis an. Die
   3,25 GB Chrome-Cookies sind für den Rust-Port wirkungslos. Im WebView2-Profil hat
   sich schlicht nie jemand eingeloggt.
4. Der Smoke (`delivery/provider_webview_smoke.ps1`) wertet
   `$code -eq 0 -and ($out -match "OK|ok")` — ein Substring-Treffer auf „ok" in
   beliebiger Ausgabe. Das erklärt „PASS" ohne echte Session.

Zusätzlich war der WebView-Backend bis `2026-07-15` **komplett funktionsunfähig**:
`tao` panicte bei jedem Seitenaufruf („Initializing the event loop outside of the main
thread"), weil `run_event_loop` im dedizierten `webagent-webview`-Thread läuft.
Gefixt via `EventLoopBuilderExtWindows::with_any_thread(true)`. Vor diesem Fix konnte
kein einziger Provider-Smoke echte Ergebnisse liefern.

## Weg zu 8/8 (manueller Login, einmalig pro Provider)

`webagent login` öffnet einen **sichtbaren** Browser und wartet auf den manuellen
Login — **ohne Zugangsdaten-Eingabe durch das Tool**. Danach persistiert die Session
im WebView2-Profil und Headless-Läufe funktionieren.

```powershell
$env:WEBAGENT_PROFILE_DIR = "C:\Users\storax\Desktop\webagent\data\profiles\shared"
.\target\release\webagent.exe login --brain deepseek   # Fenster oeffnet sich, manuell einloggen
# ... je Provider wiederholen
```

Danach verifizieren:

```powershell
.\target\release\webagent.exe diagnose --brain deepseek --headless   # erwartet: logged_in true
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
