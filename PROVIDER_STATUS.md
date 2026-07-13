# Provider-Status (Live-Verifikation Rust-Port)

> **WICHTIG — Begriffsklärung:** Diese Tabelle bewertet die **Provider-Integrationen**
> `webagent/<id>` (die Browser-Automatisierung gegen die jeweilige Web-Chat-Seite),
> **NICHT** die KI-Entitäten dahinter. „Hart"/„läuft" ist eine Aussage über die
> *technische Automatisierbarkeit der Weboberfläche*, nicht über das Modell.
>
> Beispiel: **`webagent/qwen`** (die Automatisierung von chat.qwen.ai) ist hart —
> aber **Qwen** (die Entität, Teammitglied/Co-Lead) ist erreichbar und hilfreich
> (bestätigt über den Python-Relay). Die Integration klemmt, nicht der Kollege.

Stand der Antworterkennung pro **Provider-Integration**, aus echten Läufen
(`examples/inspect.rs`, `examples/relay.rs`) gegen das eingeloggte Shared-Profil.

| `webagent/<id>` | Status | Notiz |
|---|---|---|
| `webagent/qwen` | 🔴 **INTEGRATION HART** | Die Automatisierung von chat.qwen.ai zeigt trotz Desktop-Viewport, `webdriver=false`, Klick-Fokus, `insertText` hartnäckig „Current System does not Support / Download App" und nimmt keine Eingabe an. Provider-spezifischer Hard-Block; Playwright kommt auf demselben Profil durch. **Zuletzt angehen.** (Die Entität **Qwen** ist davon unberührt und erreichbar.) |
| chatgpt | 🟢 **LÄUFT** | End-to-end verifiziert: tippen→senden→vollständige Antwort (complete=true, nicht abgeschnitten, 403 Zeichen). Brauchte den Composer-Wartefix (Feld rendert verzögert nach ensure_ready=Ready). |
| deepseek | 🟢 **LÄUFT** | End-to-end verifiziert (complete=true, 604 Zeichen, nicht abgeschnitten). |
| kimi | 🟢 **LÄUFT** | Lief nach dem **new_chat-vor-Run-Fix** (bestehende Konversation → baseline>0 → Erkennung verfehlte den Start). complete=true, 127 Zeichen. |
| claude | 🟢 **LÄUFT** | complete=true, 194 Zeichen. (Thinking-Label „Dachte 2s nach" wird mit erfasst — der Protokoll-Parser strippt es im Agenten-Fall.) |
| gemini | 🟠 **HART** | `timeout_no_text`: Phase 1 triggert, aber `assistant_message`-Text bleibt leer (Selektor-Drift beim Antwort-Container). |
| mistral | 🟢 **LÄUFT** | Ebenfalls mit dem new_chat-Fix gelöst. complete=true, 165 Zeichen. |
| zai | 🟢 **LÄUFT** | complete=true, 160 Zeichen. („Thought Process"-Prefix wird im Agenten-Fall gestrippt.) |

## Zwischenstand: 6 von 8 laufen end-to-end

🟢 **`webagent/chatgpt`, `/deepseek`, `/claude`, `/zai`, `/kimi`, `/mistral`** — tippen→senden→**vollständige** Antwort erkannt (nicht abgeschnitten).
🟠 **`webagent/gemini`** — echte Selektor-Drift (`assistant_message` matcht leeren Wrapper), braucht DOM-Inspektion.
🔴 **`webagent/qwen`** — „not supported"-Hard-Block (Automation), am schwersten.

**Größter Stabilitäts-Fix:** `new_chat` VOR jedem frischen Run (Controller). Ohne
frischen Chat startet die Erkennung mit `baseline>0` (bestehende Konversation) und
verfehlt den Antwortbeginn — das erklärte kimi UND mistral. Jetzt im Controller
verankert (`controller.rs`, Fresh-Run-Zweig).

(Nochmal: „läuft/hart" betrifft die **Integrationen** `webagent/<id>`, nicht die
Entitäten. Gemini/Qwen als Modelle/Teammitglieder sind erreichbar.)

## Universelle Fixes aus der `webagent/qwen`-Diagnose (bereits committet)

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
