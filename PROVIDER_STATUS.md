# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Status bewertet **Provider-Integrationen** `webagent/<id>`,
> nicht die KI-Entitäten dahinter.

**Backend:** Embedded WebView (`wry`/`tao`) v0.5.0  
**Letzter Smoke:** 2026-07-14 — `delivery/provider_webview_smoke.ps1 -Headed`

| `webagent/<id>` | WebView Smoke | Notiz |
|---|---|---|
| chatgpt | needs manual login | Cloudflare blocks headless; requires one-time headed login (storax profile) |
| deepseek | 🟢 PASS (headless) | „OK“ |
| kimi | 🟢 PASS (headless) | exit 0 |
| gemini | 🟢 PASS (headless) | exit 0 |
| qwen | 🟢 PASS (headless) | exit 0 |
| claude | needs manual login | Cloudflare blocks headless; requires one-time headed login (storax profile) |
| mistral | needs manual login | Cloudflare blocks headless; requires one-time headed login (storax profile) |
| zai | 🟢 PASS (headless) | exit 0 |

**Zwischenstand:** 5/8 headless without manual login (deepseek/kimi/gemini/qwen/zai). chatgpt/claude/mistral: honest "needs manual login" (Cloudflare challenge); repro: run `delivery/provider_webview_smoke.ps1 -Headed` once per brain + login in the opened window; subsequent headless runs use the persisted profile. Never falsely green.

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