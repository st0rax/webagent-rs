# Provider-Status (Live-Verifikation Rust-Port)

Stand der Antworterkennung pro Brain, aus echten Läufen (`examples/inspect.rs`,
`examples/relay.rs`) gegen das eingeloggte Shared-Profil.

| Brain | Status | Notiz |
|---|---|---|
| qwen | 🔴 **UNFRIENDLY** | Zeigt trotz Desktop-Viewport, `webdriver=false`, Klick-Fokus, `insertText` hartnäckig „Current System does not Support / Download App" und nimmt keine Eingabe an. Provider-spezifischer Hard-Block; Playwright kommt auf demselben Profil durch. **Zuletzt angehen**, wenn alle anderen laufen. |
| chatgpt | 🟢 **LÄUFT** | End-to-end verifiziert: tippen→senden→vollständige Antwort (complete=true, nicht abgeschnitten, 403 Zeichen). Brauchte den Composer-Wartefix (Feld rendert verzögert nach ensure_ready=Ready). |
| deepseek | 🟢 **LÄUFT** | End-to-end verifiziert (complete=true, 604 Zeichen, nicht abgeschnitten). |
| kimi | 🟠 **HART** | Senden + Antwort erscheinen (inspect: `.user-content`/`.markdown`, assistant_message=1), aber `relay` bekommt `timeout_no_message` — Phase-1-Erkennung greift nicht (vermutlich Enter sendet nicht / Zähler-/Konversations-Eigenheit). Zu den harten Fällen (mit Qwen) am Ende. |
| claude | 🟢 **LÄUFT** | complete=true, 194 Zeichen. (Thinking-Label „Dachte 2s nach" wird mit erfasst — der Protokoll-Parser strippt es im Agenten-Fall.) |
| gemini | 🟠 **HART** | `timeout_no_text`: Phase 1 triggert, aber `assistant_message`-Text bleibt leer (Selektor-Drift beim Antwort-Container). |
| mistral | 🟠 **HART** | `timeout_no_message` (wie kimi). |
| zai | 🟢 **LÄUFT** | complete=true, 160 Zeichen. („Thought Process"-Prefix wird im Agenten-Fall gestrippt.) |

## Zwischenstand: 4 von 8 laufen end-to-end

🟢 **chatgpt, deepseek, claude, zai** — tippen→senden→**vollständige** Antwort erkannt (nicht abgeschnitten).
🟠 **kimi, gemini, mistral, qwen** — provider-spezifische Selektor-/Sende-Eigenheiten, je eine DOM-Inspektion nötig (wie bei Qwen). Zum Schluss.

Gemeinsames Muster der harten Fälle: `assistant_message`/`send_button`-Selektoren aus dem Python-Projekt sind für diese vier veraltet, ODER Enter sendet nicht → braucht `examples/inspect.rs <brain>` + korrigierte `selectors/<brain>.json`.

## Universelle Fixes aus der Qwen-Diagnose (bereits committet)

1. `js_scan`/`probe`: try/catch pro Selektor — ein ungültiger (`:has-text`) Selektor bricht nicht mehr die ganze Liste.
2. CDP-Enter mit `text:"\r"` — löst Submit aus.
3. Fenstergröße immer 1280×900 — kein Mobil-/„nicht unterstützt"-Layout.
4. Echtes Tippen: Composer per CDP anklicken (Fokus) + `Input.insertText` statt `.value`.
5. `--disable-blink-features=AutomationControlled` — `navigator.webdriver=false`.

## Testkommando

```powershell
$env:WEBAGENT_PROFILE_DIR = "C:\Users\storax\Desktop\webagent\data\profiles\shared"
cargo run --example inspect -- <brain>
cargo run --example relay   -- <brain> "<frage>"
```
