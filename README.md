# webagent (Rust)

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant, lokale
Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare Web-Chats
(ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) — kein API-Key,
sondern die im Browser angemeldete Session.

Dies ist der **Rust-Port** des ursprünglichen Python-Projekts: plattformunabhängig
(Windows, Linux), ohne C-Toolchain für den Kern baubar, mit **Embedded WebView**
(`wry`/`tao`) statt CDP/Playwright.

> **Status (v0.8.0):** Kern vollständig portiert und getestet (`cargo test`
> + `cargo clippy --all-targets -D warnings` grün, Linux-CI grün).
> `comms.rs` (internes Messaging, ersetzt bot2bot für webagent-intern) in CLI/Controller verdrahtet.
> Browser-Steuerung über Embedded WebView + `BrowserPool`; REPL hält die Session über Turns offen.
>
> **Provider: 8 von 8 antworten headless** — chatgpt, deepseek, kimi, gemini, qwen,
> claude, mistral, zai. Gemessen per `relay` mit echten Antworten (nicht per
> Exit-Code), zwei volle Runden 8/8. Messwerte und Historie:
> [`PROVIDER_STATUS.md`](PROVIDER_STATUS.md).
>
> ⚠️ Die frühere Aussage „5/8 headless, chatgpt/claude/mistral scheitern an Cloudflare"
> war **falsch**: `cloudflare: false` bei allen acht. Drei Bugs hatten alles maskiert
> (tao-EventLoop panicte im Nicht-Main-Thread; `evaluate` lieferte für jeden Ausdruck
> `{}`, weil der JS-Wrapper ein Promise zurückgab; „headless" war ein Fenster ohne
> Fokus, sodass Enter nirgends ankam). Details in `PROVIDER_STATUS.md`.

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
| `comms` | Internal agent-to-agent messaging (data/comms/ history + per-agent inbox; wired to CLI/Controller) |
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
# Ein-Schritt Release inkl. WebView2Loader.dll (Windows)
pwsh -File scripts/build-release.ps1
# oder manuell:
#   cargo build --release
#   pwsh -File scripts/copy-webview2-loader.ps1 -Profile release
cargo test --no-default-features   # CI-Parität (ohne WebView/GTK)
cargo test                         # mit WebView-Feature (lokal)
```

Der Kern baut rein-Rust (`serde`, `serde_json`, `regex`, `fancy-regex`, `clap`, `time`).
WebView-Deps (`wry`, `tao`) sind optional (`--no-default-features` für headless CI).

## Nutzung

```
webagent login            --brain <id> [--timeout <sek>] [--force]
webagent login-all        [--timeout <sek>] [--force] [--parallel N]
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

Typischer Erstlauf: `webagent login-all` (oder `login --brain claude`), dann
`webagent diagnose --brain claude` (prüfen), dann `webagent run …` bzw. REPL `/swarm`.

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

`webagent login-all` (REPL: `/login-all`) loggt **alle** Brains **nacheinander**
ein (Default sequenziell; `--parallel N` max 3, experimentell). Schon eingeloggte
Profile werden übersprungen (`--force` erzwingt erneut).

### Profile (Swarm)

| Pfad | Rolle |
|---|---|
| `profiles/<brain>/` | **Canonical Login** — von `login` / `login-all` geschrieben |
| `profiles/reference/<brain>/` | Optional: goldene Vorlage (falls vorhanden, bevorzugte Swarm-Quelle) |
| `profiles/swarm/<run>_<brain>/` | Laufzeit-**Kopie** pro Swarm-Teilnehmer (Lock-frei, wird aufgeräumt) |
| Shared-Profil | Default für normalen `/chat` und `run` (kein Override) |

`/swarm` Ablauf (REPL):

1. Jedes Brain antwortet isoliert (Profil-Kopie, kein Shared-Pool).  
2. Orchestrator: fest (`/swarm 3 …`), sonst **Reliability** der Antwortenden;  
   teure Live-Abstimmung nur mit `WEBAGENT_SWARM_VOTE=1` (mit Antwort-Snippets).  
3. Nur der Orchestrator synthetisiert; Swarm-Profile werden aufgeräumt.

```powershell
# optional: Live-Vote statt Score
$env:WEBAGENT_SWARM_VOTE = "1"
```

## Konfiguration (Umgebungsvariablen)

| Variable | Wirkung |
|---|---|
| `WEBAGENT_TIMEOUT_MULT` / `_MIN` / `_MAX` | Skalierung der dynamischen Timeouts |
| `WEBAGENT_SHARED_BROWSER` | Gemeinsames Profil + `BrowserPool` aktivieren |
| `WEBAGENT_PERSIST_TABS` | Tabs nach Relay/Run offen lassen (Pool) |
| `WEBAGENT_SHELL_STRICT` | `1` = Shell nur risk-arme Prefixe + Denylist |
| `WEBAGENT_LOGIN_TO_REFERENCE` | `1` = nach `login-all` Profil zusätzlich nach `profiles/reference/<brain>` spiegeln |
| `WEBAGENT_SWARM_VOTE` | `1` = `/swarm` Phase-2 Live-Abstimmung (sonst Reliability-Score) |
| `WEBAGENT_PROFILE_DIR` | Überschreibt das Profil-Root (sonst `…/profiles`) |

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