# webagent (Rust)

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant, lokale
Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare Web-Chats
(ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) — kein API-Key,
sondern die im Browser angemeldete Session.

Dies ist der **Rust-Port** des ursprünglichen Python-Projekts: plattformunabhängig
(Windows, Linux), ohne C-Toolchain für den Kern baubar, mit **Embedded WebView**
(`wry`/`tao`) statt CDP/Playwright.

> **Status (v0.5.0):** Kern vollständig portiert und getestet (`cargo test --no-default-features`
> grün in CI). Browser-Steuerung über Embedded WebView + `BrowserPool` (ein Tab pro Brain).
> REPL hält die Browser-Session über Turns offen (`skip_brain_start/stop`).
> Alle CLI-Befehle verdrahtet inkl. `brains-health`, `relay`, `oobe`.
> Provider-Live-Verifikation nach WebView-Migration: siehe [`PROVIDER_STATUS.md`](PROVIDER_STATUS.md).

## Architektur

Ein Brain plant im `webagent/1`-Protokoll (JSON), der Controller führt die Actions
strikt seriell aus, Beobachtungen fließen zurück ins Brain:

```
Brain (Web-Chat)  ──plan──▶  Controller  ──shell──▶  Executor (PowerShell/sh)
      ▲                          │
      └──────── Observation ◀────┘
```

| Modul | Verantwortung |
|---|---|
| `protocol` | `webagent/1`-Parser inkl. `WEBAGENT/1 SHELL`-Rohformat |
| `controller` | Plan/Act/Observe-Zustandsmaschine, Resume, Loop-/Budget-Schutz |
| `brain` | Trait `BrainBackend` (Browser-neutral) |
| `browser` + `webview_runtime` + `page_driver` | Embedded WebView-Backend (`wry`/`tao`) |
| `browser_pool` | Shared-Profil: ein Runtime, ein Tab pro Brain |
| `executor` | Shell-Ausführung (Windows: PowerShell, Unix: sh/bash) |
| `run_store` · `transcript` · `memory` | Persistenz (JSON-Lines) |
| `doctor` · `watchdog` · `brains_health` | Diagnose & Pre-flight |
| `relay` · `oobe` | Single-turn Relay, Ersteinrichtungs-Wizard |
| `timeouts` · `loop_guard` · `observer` · `prompts` · `config` | Politik & Heuristiken |

Selektoren pro Provider liegen in [`selectors/`](selectors/); Portierungsregeln in
[`CONVENTIONS.md`](CONVENTIONS.md).

## Bauen

Voraussetzung: eine Rust-Toolchain. Auf Windows **ohne** Visual Studio genügt die
GNU-Toolchain (bringt ihren eigenen Linker mit):

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu   # im Projektordner
cargo build --release
cargo test --no-default-features   # CI-Parität (ohne WebView/GTK)
cargo test                         # mit WebView-Feature (lokal)
```

Der Kern baut rein-Rust (`serde`, `serde_json`, `regex`, `fancy-regex`, `clap`, `time`).
WebView-Deps (`wry`, `tao`) sind optional (`--no-default-features` für headless CI).

## Nutzung

```
webagent login            --brain <id> [--timeout <sek>]
webagent run              --brain <id> --task "<aufgabe>" [--headless] [--max-cycles N] [--resume <run_id>]
webagent repl             --brain <id> [--headless]
webagent diagnose         --brain <id> [--headless]
webagent doctor           [--brain <id>]... [--json]
webagent watchdog         [--repair] [--json]
webagent brains-health    [--allow-empty-profile]
webagent relay            --brain <id> --message "<text>" [--headless] [--timeout <sek>]
webagent oobe             [--brains <csv>] [--skip-login] [--yes]
webagent maintenance-check [--json]
```

Verfügbare Brains: `chatgpt, deepseek, kimi, gemini, qwen, claude, mistral, zai`.

Typischer Erstlauf: `webagent login --brain claude` (einloggen), dann
`webagent diagnose --brain claude` (prüfen), dann `webagent run …`.

Beispiel:

```powershell
webagent run --brain deepseek --task "Schreibe ein PowerShell-Skript, das die 10 groessten Dateien in C:\ auflistet"
```

Der Standard ist **sichtbarer** Browser. `--headless` öffnet ein **verstecktes**
Fenster (Hidden-Window-Policy), kein echtes Headless-Chromium.

### Login

`webagent login --brain <id>` öffnet ein **sichtbares** WebView-Fenster auf der
Provider-Seite und wartet, bis du dich **selbst** angemeldet hast — es werden
**keine Zugangsdaten eingegeben oder gespeichert**; der Agent pollt nur den
Login-Zustand. Danach nutzen `run`/`diagnose`/`relay` diese Session im
persistente Profil (`profiles/<brain>/` oder Shared-Profil). Prüfen mit
`webagent diagnose --brain <id>`.

## Konfiguration (Umgebungsvariablen)

| Variable | Wirkung |
|---|---|
| `WEBAGENT_TIMEOUT_MULT` / `_MIN` / `_MAX` | Skalierung der dynamischen Timeouts |
| `WEBAGENT_SHARED_BROWSER` | Gemeinsames Profil + `BrowserPool` aktivieren |
| `WEBAGENT_PERSIST_TABS` | Tabs nach Relay/Run offen lassen (Pool) |

## Daten

Runs liegen unter `data/runs/<run_id>/` (`meta.json`, `transcript.jsonl`,
`events.jsonl`), das Langzeitgedächtnis unter `data/memory.jsonl`. Browser-Profile
in `profiles/` — beide sind per `.gitignore` ausgeschlossen (enthalten Cookies).

## Sicherheitsmodell

Bewusst **kein** Befehlsfilter: das Brain darf beliebige Shell-Befehle im
angemeldeten Nutzerkontext ausführen. Nur in vertrauenswürdiger Umgebung nutzen.

## Entwicklung

- Portierungskonventionen: [`CONVENTIONS.md`](CONVENTIONS.md)
- Parität vs. Python: [`MERGE_AND_PARITY.md`](MERGE_AND_PARITY.md)
- Tests: `cargo test --no-default-features` (kein echter Browser in Unit-Tests;
  `MockPageDriver` für Browser-Logik). Live-Provider-Checks: `cargo run --example inspect -- <brain>`.

## Lizenz

MIT.