# Provider-Status (Live-Verifikation Rust-Port)

> **Begriffsklärung:** Diese Tabelle bewertet die **Provider-Integrationen**
> `webagent/<id>` (Browser-Automatisierung gegen die jeweilige Web-Chat-Seite),
> **nicht** die KI-Entitäten dahinter.

> **Stack v0.5.0:** Embedded **WebView** (`wry`/`tao`), kein CDP. `--headless` =
> Hidden Window (`with_visible(false)`).

## WebView-Smoke 2026-07-14

Methode: `delivery/provider_webview_smoke.ps1` — `relay --brain <id> --message "Antworte nur mit OK." --headless`
mit `WEBAGENT_SHARED_BROWSER=1`, `WEBAGENT_PROFILE_DIR=<parent>/data/profiles/shared`.

Evidence: `{SCRATCH}/provider-webview-smoke.log` — **SUMMARY ok=5 fail=3 total=8**

| `webagent/<id>` | CDP (Referenz) | WebView v0.5.0 (2026-07-14) | Anmerkung |
|---|---|---|---|
| `webagent/qwen` | 🟢 LÄUFT | 🟢 PASS | Antwort „OK.“ |
| chatgpt | 🟢 LÄUFT | 🔴 Cloudflare | `session_state=Cloudflare` (headless) |
| deepseek | 🟢 LÄUFT | 🟢 PASS | Antwort „OK“ |
| kimi | 🟢 LÄUFT | 🟢 PASS | exit 0 |
| claude | 🟢 LÄUFT | 🔴 Cloudflare | `session_state=Cloudflare` (headless) |
| gemini | 🟢 LÄUFT | 🟢 PASS | Antwort „OK.“ |
| mistral | 🟢 LÄUFT | 🔴 Cloudflare | `session_state=Cloudflare` (headless) |
| zai | 🟢 LÄUFT | 🟢 PASS | Antwort „OK“ |

Cloudflare-Fails betreffen headless Hidden-Window — gleiche Provider funktionieren
in CDP-Ära mit sichtbarem Browser/Profil. Re-Test headed: `provider_webview_smoke.ps1 -Headed`.

## Neu-Verifikation (WebView)

```powershell
$env:WEBAGENT_SHARED_BROWSER = "1"
$env:WEBAGENT_PROFILE_DIR = "$env:USERPROFILE\Desktop\webagent\data\profiles\shared"
cargo run -- relay --brain <brain> --message "Antworte nur mit OK."
```

Erwartung: `complete=true`, nicht abgeschnittene Antwort.

## Bekannte Stabilitäts-Fixes (weiter gültig)

1. `new_chat` VOR jedem frischen Run (Controller) — baseline>0 vermeiden.
2. Composer-Fokus + DOM-fill statt `.value`-Injection.
3. Fenstergröße 1280×900 — kein Mobil-Layout.
4. `js_scan`/`probe`: try/catch pro Selektor.