# webagent (Rust)

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant, lokale
Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare Web-Chats
(ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) — kein API-Key,
sondern die im Browser angemeldete Session.

Dies ist der **Rust-Port** des ursprünglichen Python-Projekts: plattformunabhängig
(Windows, Linux, Android), ohne C-Toolchain baubar, mit einem eigenen
CDP-Browsertreiber statt Playwright.

> **Status:** Kern vollständig portiert und getestet (`cargo test` grün). Der
> Browsertreiber ist live gegen Chromium verifiziert. **Noch offen:** der
> interaktive `login`-Flow — bis dahin muss die Brain-Session einmalig manuell im
> Profilverzeichnis angemeldet werden (siehe [Login](#login)).

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
| `browser` + `cdp` | Konkretes Backend: steuert Chromium über das Chrome DevTools Protocol |
| `executor` | Shell-Ausführung (Windows: PowerShell, Unix: sh/bash) |
| `run_store` · `transcript` · `memory` | Persistenz (JSON-Lines) |
| `doctor` · `watchdog` | Diagnose & Selbstheilung (verwaiste Runs, Locks) |
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
cargo test
```

Die Dependencies sind bewusst rein-Rust (keine C-Toolchain nötig): `serde`,
`serde_json`, `regex`, `fancy-regex`, `clap`, `tungstenite`.

## Nutzung

```
webagent run   --brain <id> --task "<aufgabe>" [--headless] [--max-cycles N] [--resume <run_id>]
webagent doctor  [--brain <id>]... [--json]
webagent watchdog [--repair] [--json]
webagent maintenance-check [--json]
```

Verfügbare Brains: `chatgpt, deepseek, kimi, gemini, qwen, claude, mistral, zai`.

Beispiel:

```powershell
webagent run --brain deepseek --task "Schreibe ein PowerShell-Skript, das die 10 groessten Dateien in C:\ auflistet"
```

Der Standard ist **sichtbarer** Browser (Cloudflare blockiert echtes Headless);
`--headless` nur, wenn der Provider es zulässt.

### Login

Der interaktive `login`-Befehl ist noch nicht portiert. Bis dahin: einmalig ein
Chromium auf das Profilverzeichnis des Brains anmelden — danach nutzt der Agent
die persistente Session. Profilpfade: `profiles/<brain>/` (siehe `webagent doctor`).

## Konfiguration (Umgebungsvariablen)

| Variable | Wirkung |
|---|---|
| `WEBAGENT_CHROME` | Pfad zur Chromium/Chrome/Edge-Binary (sonst Auto-Erkennung) |
| `WEBAGENT_TIMEOUT_MULT` / `_MIN` / `_MAX` | Skalierung der dynamischen Timeouts |
| `WEBAGENT_SHARED_BROWSER` | gemeinsames Profil aktivieren |

## Daten

Runs liegen unter `data/runs/<run_id>/` (`meta.json`, `transcript.jsonl`,
`events.jsonl`), das Langzeitgedächtnis unter `data/memory.jsonl`. Browser-Profile
in `profiles/` — beide sind per `.gitignore` ausgeschlossen (enthalten Cookies).

## Sicherheitsmodell

Bewusst **kein** Befehlsfilter: das Brain darf beliebige Shell-Befehle im
angemeldeten Nutzerkontext ausführen. Nur in vertrauenswürdiger Umgebung nutzen.

## Entwicklung

- Portierungskonventionen: [`CONVENTIONS.md`](CONVENTIONS.md)
- Tests: `cargo test` (kein echter Browser/Netz in Unit-Tests). Der Live-CDP-Test
  läuft nur explizit: `cargo test --lib cdp -- --ignored`.

## Lizenz

MIT.
