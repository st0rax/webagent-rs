# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0  
**Letzter Smoke:** 2026-07-14 — `delivery/provider_webview_smoke.ps1` (relay „OK“, `--headless`, Shared-Profil)

| `webagent/<id>` | WebView Smoke | Notiz |
|---|---|---|
| deepseek | 🟢 PASS | Antwort „OK“ |
| kimi | 🟢 PASS | Antwort leer, exit 0 |
| gemini | 🟢 PASS | „OK.“ |
| qwen | 🟢 PASS | „OK.“ |
| zai | 🟢 PASS | „OK“ |
| chatgpt | 🔴 FAIL | `session_state=Cloudflare` (headless) |
| claude | 🔴 FAIL | `session_state=Cloudflare` (headless) |
| mistral | 🔴 FAIL | `session_state=Cloudflare` (headless) |

**Zwischenstand: 5 von 8** auf WebView (headless). Cloudflare-Blocker bei chatgpt/claude/mistral — Retry mit sichtbarem Browser (`-Headed`).

Evidence: `%TEMP%\grok-goal-*\implementer\provider-webview-smoke.log`

## Testkommando

```powershell
pwsh -File Desktop\webagent\delivery\provider_webview_smoke.ps1 -Headed
# oder einzeln:
$env:WEBAGENT_PROFILE_DIR = "Desktop\webagent\data\profiles\shared"
$env:WEBAGENT_USE_SHARED_BROWSER = "1"
webagent relay --brain gemini --message "Antworte nur OK"
```

## Bekannte Stabilitäts-Fixes

1. `new_chat` vor frischem Run (Controller).
2. Composer-Fokus + DOM-fill.
3. Fenstergröße 1280×900.
4. `BrowserPool` + `WEBAGENT_PERSIST_TABS` für Relay-Ketten.