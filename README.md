# webagent (Rust)

> **Verbindliche Arbeitsgrundlage:** Vor jeder Arbeit ist [`AGENTS.md`](AGENTS.md) vollständig zu lesen und strikt zu befolgen. Projektspezifische Regeln gelten ergänzend; bei Konflikten gilt die strengere Schutzregel.

Ein **lokaler, browsergesteuerter Agent**: ein Web-Chat („Brain") plant, lokale
Werkzeuge (PowerShell/Shell) führen aus. Die Brains sind austauschbare Web-Chats
(ChatGPT, Claude, DeepSeek, Gemini, Kimi, Qwen, Mistral, Z.ai) — kein API-Key,
sondern die im Browser angemeldete Session.

Dies ist der **Rust-Port** des ursprünglichen Python-Projekts: Session-Web-UI,
REPL und CLI auf Windows, Linux und Android (Termux); Embedded-WebView-Brains
(`wry`/`tao`) auf Windows über WebView2 und auf Linux über WebKitGTK.

## Schnellstart (Benutzen, nicht Entwickeln)

Du brauchst **kein** Rust und keinen Quellcode — nur das fertige Programm aus
[Releases](https://github.com/st0rax/webagent-rs/releases/latest) und ein
angemeldeter Browser bei einem der Chat-Dienste, die du nutzen willst
(kein API-Key).

**1. Herunterladen und ausführbar machen**

```bash
# Linux
chmod +x webagent-linux-x86_64
```

Unter Windows: `webagent-windows-x86_64.exe` und `WebView2Loader.dll` in
denselben Ordner legen.

**2. Systempakete (nur Linux)**

Der eingebettete Browser nutzt WebKitGTK. Unter Debian/Ubuntu:

```bash
sudo apt update && sudo apt install -y libwebkit2gtk-4.1-0
```

Unter Windows ist WebView2 in aktuellen Systemen bereits vorhanden.

**3. Einrichten und anmelden**

```bash
./webagent-linux-x86_64 oobe
./webagent-linux-x86_64 login --brain chatgpt
```

`login` öffnet ein Browserfenster. **Du meldest dich dort selbst an** — das
Programm fragt nie nach Zugangsdaten und speichert keine. Die Sitzung bleibt
im eigenen Profil erhalten, eine Anmeldung je Dienst genügt.

**4. Erste Aufgabe**

```bash
./webagent-linux-x86_64 run --brain chatgpt --task "Liste die Dateien hier auf und fasse zusammen, worum es geht"
```

Der Chat plant, dein Rechner führt aus. Prüfen, ob eine Oberfläche fahrbar ist:
`diagnose --brain chatgpt`.

> **Stand Linux:** Der WebView-Build für Linux ist neu und bisher nur *gebaut*,
> nicht auf einem Linux-Desktop *erprobt*. Unter Windows läuft er im täglichen
> Gebrauch. Wenn unter Linux etwas klemmt, ist das kein Bedienfehler — bitte ein
> Issue aufmachen.

> **Android/Termux:** Das aarch64-Binary enthält Kern, REPL und CLI, aber
> **keinen** eingebetteten Browser — dafür bräuchte es ein APK. Brains lassen
> sich dort nicht fahren.

---

> **Neu im Projekt?** Beginne mit [`START_HERE.md`](START_HERE.md). Der kurze
> aktuelle Arbeitsstand steht in
> [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md), der Entwicklungsprozess in
> [`CONTRIBUTING.md`](CONTRIBUTING.md) und die Zusammenarbeit in
> [`docs/COLLABORATION.md`](docs/COLLABORATION.md).

> **Status (v0.10.1):** Session-Web-UI ist der Default. Der genaue aktuelle
> Abnahme- und Arbeitsstand steht in
> [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md); lokale Testergebnisse sind
> immer an den dort genannten Commit gebunden.
> `comms.rs` (internes Messaging, ersetzt bot2bot für webagent-intern) in CLI/Controller verdrahtet.
> Browser-Steuerung über Embedded WebView + `BrowserPool`; REPL hält die Session über Turns offen.
>
> **Historischer Provider-Nachweis:** Am 2026-07-16 antworteten 8 von 8 Brains
> headless — chatgpt, deepseek, kimi, gemini, qwen, claude, mistral und zai.
> Das waren echte `relay`-Antworten, nicht nur erfolgreiche Exit-Codes. Wegen
> veränderlicher Web-UIs, Sessions und Quoten ist dies keine aktuelle
> Verfügbarkeitszusage. Messwerte und spätere Capability-Historie:
> [`docs/PROVIDER_STATUS.md`](docs/PROVIDER_STATUS.md).
>
> ⚠️ Die frühere Aussage „5/8 headless, chatgpt/claude/mistral scheitern an Cloudflare"
> war **falsch**: `cloudflare: false` bei allen acht. Drei Bugs hatten alles maskiert
> (tao-EventLoop panicte im Nicht-Main-Thread; `evaluate` lieferte für jeden Ausdruck
> `{}`, weil der JS-Wrapper ein Promise zurückgab; „headless" war ein Fenster ohne
> Fokus, sodass Enter nirgends ankam). Details in `docs/PROVIDER_STATUS.md`.

## Architektur

Ein Brain plant im `webagent/1`-Protokoll (JSON), der Controller führt die Actions
strikt seriell aus, Beobachtungen fließen zurück ins Brain:

```
Brain (Web-Chat)  ──plan──▶  Controller  ──shell──▶  Executor (PowerShell/sh)
      ▲                          │
      └──────── Observation ◀────┘
```

| Modul | Verantwortung (siehe [`docs/OVERVIEW.md`](docs/OVERVIEW.md) und [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) für Details) |
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

> **Aktueller Systemüberblick:** [`docs/OVERVIEW.md`](docs/OVERVIEW.md) — Ziel,
> Vertrauensleiter, Reifegrade und Betrieb. Die detaillierte
> [`Modulkarte`](docs/ARCHITECTURE.md) ist eine datierte Momentaufnahme.

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

Die lokale **Web-UI** ist die Standard-Oberflaeche: `webagent` **ohne Parameter**
startet sie und oeffnet den Browser auf `http://127.0.0.1:8788/`. Dort laufen
Sitzungen ueber `/api/sessions`. Die zeilenweise **REPL** ist `webagent repl`
(siehe unten), die Pool-/Wand-/Bench-Ansicht `webagent tui`.

**REPL fragen vs. autonom:** In der REPL laufen normale Eingaben als
**autonome Aufgabe** (das Brain darf Dateien aendern und lokale Shell ausfuehren).
`/chat <text>` ist dagegen **reine Konversation** ohne Werkzeuge. Derselbe
Slash-Parser gilt in beiden Oberflaechen: `/new`, `/resume`, `/status`,
`/model`, `/dashboard`, `/quit`.

Aus einer Pipe: `webagent tui --force-tui --view=session`. Unter Windows stehen
die Brain-Fenster oben, die TUI unten — `AGENTS.md` §6.

Die lokale Browser-Inference-Bridge fuer Pi (`webagent api serve`, Loopback,
Token-geschuetzt) ist ein separates Integrationsthema — Details, Pi-Konfiguration
und Beispielskripte in [`docs/API_BRIDGE.md`](docs/API_BRIDGE.md).

```
webagent login            --brain <id> [--timeout <sek>] [--force] [--auto]
webagent login-all        [--timeout <sek>] [--force] [--parallel N]
webagent run              --brain <id> --task "<aufgabe>" [--headless] [--max-cycles N] [--resume <run_id>] [--no-memory]
webagent repl             --brain <id> [--headless]
webagent relay            --brain <id> --message "<text>" [--headless] [--timeout <sek>] [--json]
webagent diagnose         --brain <id> [--headless]
webagent doctor           [--brain <id>]... [--json]
webagent watchdog         [--repair] [--json]
webagent brains-health    [--allow-empty-profile]
webagent autoresearch     --brain <id> --goal "<text>" --eval "<cmd>" [--direction higher|lower]
                          [--max-iterations N] [--no-improve-abort N] [--eval-timeout <sek>] [--workdir <pfad>]
webagent oobe             [--brains <csv>] [--skip-login] [--yes]
webagent maintenance-check [--json]
```

`relay --json` liefert einen einzelnen send+wait-Turn maschinenlesbar
(`{"brain","ok","answer","latency_ms","reason"}`). Die Auto-Routing-Brain
`webagent/auto` (Bild-, Coding-, Recherche- oder Textlast) gibt es derzeit nur
im API-Katalog der Bridge — Details in [`docs/API_BRIDGE.md`](docs/API_BRIDGE.md).

**Datei-Aktionen im Protokoll:** Brains ändern Dateien über `edit`
(path/old_string/new_string, Anker muss exakt einmal matchen) und `write`
(neue Datei) — nativ ausgeführt, ohne Shell-Escaping-Risiko.

**Autoresearch** (Karpathy-Muster): `--eval` muss als letzte stdout-Zeile eine
Zahl liefern; verbesserte Iterationen werden auf einem `autoresearch/`-Branch
committet, verschlechterte komplett revertet. Merge bleibt manuell.
Log: `data/autoresearch/<run_id>/log.md`.

**Wiki-Memory** (Karpathy-LLM-Wiki): Markdown-Seiten mit `[[links]]` und
`index.md` unter `data/memory/wiki/`; der Index fließt automatisch als Kontext
in autonome Runs, Brains pflegen Seiten per edit/write. REPL: `/wiki`,
`/wiki <suche>`, `/wiki lint`.

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

Das Brain führt Shell-Befehle im angemeldeten Nutzerkontext aus.
`shell_policy` blockiert bekannte gefährliche Muster und protokolliert
Ausführungen, ist aber keine Betriebssystem-Sandbox. Nur in einer
vertrauenswürdigen Umgebung und mit angemessen begrenzten Nutzerrechten nutzen.

## Entwicklung

- Einstieg und Übernahme: [`START_HERE.md`](START_HERE.md)
- Workflow und Abnahme: [`CONTRIBUTING.md`](CONTRIBUTING.md)
- Aktuelle Arbeit: [`docs/CURRENT_WORK.md`](docs/CURRENT_WORK.md)
- GitHub-Kommunikation: [`docs/COLLABORATION.md`](docs/COLLABORATION.md)
- Portierungskonventionen: [`CONVENTIONS.md`](CONVENTIONS.md)
- Parität vs. Python: [`docs/MERGE_AND_PARITY.md`](docs/MERGE_AND_PARITY.md)
- Tests: `cargo test --no-default-features` (kein echter Browser in Unit-Tests;
  `MockPageDriver` für Browser-Logik). Live-Provider-Checks: `cargo run --example inspect -- <brain>`.

## Plattformen und Release-Binaries

| Plattform | Artifact | Was darin steckt |
|---|---|---|
| Windows x86_64 | `webagent-windows-x86_64.exe` + `WebView2Loader.dll` | Session-Web-UI, REPL, Pool/Wand, Embedded WebView2 |
| Linux x86_64 | `webagent-linux-x86_64` | Session-Web-UI, REPL, CLI **und Embedded WebView über WebKitGTK** (`--features tui,webview`); braucht `libwebkit2gtk-4.1-0` |
| Android aarch64 | `webagent-aarch64-linux-android` | dasselbe für Termux, kein Play-Store-APK, kein Embedded WebView |

GitHub-Releases entstehen beim Tag `v*` (`.github/workflows/release.yml`).
Embedded-WebView-Brains laufen unter Windows (WebView2) und Linux (WebKitGTK).
Android/Termux nutzt denselben Kern **ohne** eingebetteten Browser — dafür
bräuchte es ein APK, kein CLI-Binary. Android-CI baut denselben Termux-Target (`.github/workflows/android.yml`).

## Lizenz

MIT.
