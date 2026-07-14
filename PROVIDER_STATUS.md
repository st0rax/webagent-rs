# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0  
**Letzter Smoke:** 2026-07-14 — `delivery/provider_webview_smoke.ps1 -Headed`

| `webagent/<id>` | WebView Smoke | Notiz |
|---|---|---|
| chatgpt | 🟢 PASS | Headed (headless → Cloudflare) |
| deepseek | 🟢 PASS | „OK“ |
| kimi | 🟢 PASS | exit 0 |
| gemini | 🟢 PASS | exit 0 |
| qwen | 🟢 PASS | exit 0 |
| claude | 🟢 PASS | Headed (headless → Cloudflare) |
| mistral | 🟢 PASS | Headed (headless → Cloudflare) |
| zai | 🟢 PASS | exit 0 |

**Zwischenstand: 8 von 8** auf WebView (headed). Headless scheitert bei chatgpt/claude/mistral an Cloudflare — Standard bleibt sichtbar.

Evidence: `%TEMP%\grok-goal-*\implementer\provider-webview-smoke.log`

## Testkommando

```powershell
pwsh -File Desktop\webagent\delivery\provider_webview_smoke.ps1 -Headed
```

## Bekannte Stabilitäts-Fixes

1. `new_chat` vor frischem Run (Controller).
2. Composer-Fokus + DOM-fill.
3. Fenstergröße 1280×900.
4. `BrowserPool` + `WEBAGENT_PERSIST_TABS` für Relay-Ketten.
5. Sichtbarer Browser für Cloudflare-anfällige Provider.