# Provider-Status (Live-Verifikation Rust-Port)

> **WICHTIG — Begriffsklärung:** Diese Tabelle bewertet die **Provider-Integrationen**
> `webagent/<id>` (die Browser-Automatisierung gegen die jeweilige Web-Chat-Seite),
> **NICHT** die KI-Entitäten dahinter.

> **Migration v0.5.0:** Browser-Backend von CDP auf **Embedded WebView** (`wry`/`tao`)
> umgestellt. Die untenstehenden End-to-end-Verifikationen stammen aus der **CDP-Ära**
> (2026-07-14). **Neu-Verifikation auf WebView steht aus** — gleiche Selektoren,
> anderer Page-Driver.

Stand der Antworterkennung pro **Provider-Integration** (CDP-Ära, Referenz):

| `webagent/<id>` | Status (CDP) | WebView (v0.5.0) |
|---|---|---|
| `webagent/qwen` | 🟢 LÄUFT | ⏳ ausstehend |
| chatgpt | 🟢 LÄUFT | ⏳ ausstehend |
| deepseek | 🟢 LÄUFT | ⏳ ausstehend |
| kimi | 🟢 LÄUFT | ⏳ ausstehend |
| claude | 🟢 LÄUFT | ⏳ ausstehend |
| gemini | 🟢 LÄUFT | ⏳ ausstehend |
| mistral | 🟢 LÄUFT | ⏳ ausstehend |
| zai | 🟢 LÄUFT | ⏳ ausstehend |

## Neu-Verifikation (WebView)

```powershell
$env:WEBAGENT_SHARED_BROWSER = "1"
cargo run --example inspect -- <brain>
cargo run -- relay --brain <brain> --message "Antworte nur mit OK"
```

Erwartung: `complete=true`, nicht abgeschnittene Antwort. Bei Abweichung:
brain-spezifische `send_*`-Pfade in `browser.rs` prüfen.

## Bekannte Stabilitäts-Fixes (weiter gültig)

1. `new_chat` VOR jedem frischen Run (Controller) — baseline>0 vermeiden.
2. Composer-Fokus + DOM-fill statt `.value`-Injection.
3. Fenstergröße 1280×900 — kein Mobil-Layout.
4. `js_scan`/`probe`: try/catch pro Selektor.