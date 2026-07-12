# webagent (Rust)

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant, lokale
Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare Web-Chats
(ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) — kein API-Key,
sondern die im Browser angemeldete Session.

Dies ist der **Rust-Port** des ursprünglichen Python-Projekts: plattformunabhängig
(Windows, Linux, Android), ohne C-Toolchain baubar, mit einem eigenen
CDP-Browsertreiber statt Playwright.

> **Status:** Kern vollständig portiert und getestet (`cargo test` grün). Der
> Browsertreiber ist live gegen Chromium verifiziert. Alle Befehle sind
> verdrahtet: `login`, `run`, `repl`, `diagnose`, `doctor`, `watchdog`,
> `maintenance-check`. (Die `repl` startet in v1 den Browser pro Turn neu — eine
> über Turns offene Session ist eine mögliche spätere Optimierung.)

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
webagent login    --brain <id> [--timeout <sek>]
webagent run      --brain <id> --task "<aufgabe>" [--headless] [--max-cycles N] [--resume <run_id>]
webagent repl     --brain <id> [--headless]
webagent diagnose --brain <id> [--headless]
webagent doctor   [--brain <id>]... [--json]
webagent watchdog [--repair] [--json]
webagent maintenance-check [--json]
```

Verfügbare Brains: `chatgpt, deepseek, kimi, gemini, qwen, claude, mistral, zai`.

Typischer Erstlauf: `webagent login --brain claude` (einloggen), dann
`webagent diagnose --brain claude` (prüfen), dann `webagent run …`.

Beispiel:

```powershell
webagent run --brain deepseek --task "Schreibe ein PowerShell-Skript, das die 10 groessten Dateien in C:\ auflistet"
```

Der Standard ist **sichtbarer** Browser (Cloudflare blockiert echtes Headless);
`--headless` nur, wenn der Provider es zulässt.

### Login

`webagent login --brain <id>` öffnet ein **sichtbares** Chromium auf der
Provider-Seite und wartet, bis du dich **selbst** angemeldet hast — es werden
**keine Zugangsdaten eingegeben oder gespeichert**; der Agent pollt nur den
Login-Zustand und schließt danach sauber, damit Chrome die Session ins
persistente Profil (`profiles/<brain>/`) schreibt. Danach nutzen `run`/`diagnose`
diese Session. Prüfen mit `webagent diagnose --brain <id>`.

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

## Android (via GitHub)

Das Projekt ist für Android nutzbar — geklont über GitHub und auf dem Gerät
(Termux) gebaut, **ohne Android‑NDK**. Verbindung zu einem entfernten Chrome
(Metaline) über den CDP‑Endpunkt, statt lokalen Chrome zu starten.

```bash
# In Termux (oder jedem Linux/arm64):
pkg install rust git
git clone https://github.com/st0rax/webagent-rs
cd webagent-rs
cargo install cargo-zigbuild --locked
export CARGO_BUILD_TARGET=aarch64-linux-android
cargo zigbuild --release
# Binär: target/aarch64-linux-android/release/webagent
```

Danach den Agenten zu einem Desktop‑Chrome lenken (kein Chrome auf dem Handy nötig):

```bash
export WEBAGENT_CDP_ENDPOINT=ws://192.168.1.10:9222/devtools/page/<id>
# oder host:port  ->  http://host:port/json wird nach dem page-Target abgefragt
./target/aarch64-linux-android/release/webagent run --brain claude --task "…"
```

Bei gesetztem `WEBAGENT_CDP_ENDPOINT` überspringt `start()` den lokalen
Browser‑Launch komplett (siehe `src/cdp.rs` / `src/browser.rs`). Eine CI, die den
arm64‑Build automatisch prüft, liegt unter `.github/workflows/android.yml`.

## Lizenz

MIT.
