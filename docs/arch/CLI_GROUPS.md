# CLI groups per layer — design only (T-1005)

> **Referenz.** Design snapshot, not a live CLI contract. The live contract is
> `src/cli.rs` plus the built binary's `--help`. No code changed for T-1005.

Date: 2026-11-26. Status: proposal. This document changes no command, no
argument and no behavior. Implementation needs a separate, approved task.

## 1. Snapshot method

The default `webview` binary on this host exits `0xC0000005` before `main`, so
the snapshot was taken from the browser-free binary built at `938bbf1` with
`cargo build --no-default-features`:

```text
target\debug\webagent.exe --help
target\debug\webagent.exe <command> --help     # once for every command below
```

`--help` text is identical across feature sets because the command enum in
`src/cli.rs` is not feature-gated; only runtime availability of browser-backed
commands differs. 43 commands were counted (excluding clap's own `help`).

## 2. Before: top-level help (verbatim)

```text
Gehirnunabhängiger lokaler Agent (Rust-Port)

Usage: webagent.exe [OPTIONS] [COMMAND]

Commands:
  run                Autonomen Run starten
  ask                Einheitliche Eingabe: autonomer Run (Default) oder Konversations-Einzelturn. Nachfolger von `run`/`relay` (beide bleiben kompatible Aliasse)
  login              Sichtbaren Browser oeffnen und auf manuellen Login warten (keine Zugangsdaten-Eingabe)
  show               Offscreen-WebView eines laufenden Brains onscreen holen (Login/Captcha). Spricht bevorzugt die lokale Web-UI/API (`127.0.0.1:8788`) an
  hide               Onscreen-WebView wieder offscreen parken
  login-all          Alle Brains einloggen (canonical profiles/<brain>). Parallel startet pro Brain einen eigenen login-worker-Kindprozess
  diagnose           Live-Diagnose: echten Browser oeffnen und Login/Composer/Selektoren pruefen
  repl               Interaktive REPL: mehrere Aufgaben nacheinander gegen dasselbe Brain
  doctor             Pro-Brain Diagnose: Selektoren, Profil-Lock, letzte Antwort, Recovery
  verify             Capability-Verifikation: echte Live-Laeufe belegen die Faehigkeiten (ProofKind-gesteuert, schreibt Belege nach proofs.jsonl)
  count              Zaehl-Spiel: die Brains zaehlen abwechselnd der Reihe nach bis `count` (Ball wandert im Kreis). Performance- und Zuverlaessigkeitsmass: eine Sitzung pro Brain, Traces werden als JSON gespeichert und ausgewertet
  watchdog
  runs-report        Fehlerursachen vergangener Laeufe klassifizieren: trennt Harness-Fehler von echter Unfaehigkeit der Brains
  design-vote        Swarm entwirft ein TUI-Design, stimmt im Ausscheidungsverfahren ab (kick vote), der Gewinner wird von einem Brain umgesetzt
  brains-health      Pre-flight: Profile, Selektoren, Flags (ohne Browser)
  canary             Offline-Canary: prueft je Brain nur, ob Spec und Selektor-Datei da sind (kein Browser, kein Login, kein Netz — dafuer `diagnose`)
  section            Einen Bereich der Oberflaeche oeffnen (z.B. projects_button) und den Wechsel ueber die URL belegen
  mode               Segmentleiste umschalten (alle Stellungen sichtbar, z.B. deepseeks Instant/Expert/Vision)
  menu               Beliebiges Aufklappmenue lesen oder waehlen (z.B. Denkstufe)
  toggle             Eine Option umschalten (z.B. reasoning_toggle, web_search_toggle) und belegen, dass sich der Zustand wirklich geaendert hat
  wall               Bilderwand: alle Brains nebeneinander in einem Fenster, wie mehrere TV-Kanaele gleichzeitig
  model              Modelle eines Brains auflisten oder umschalten (mit Nachpruefung, dass der Wechsel wirklich griff)
  shot               Nimmt die Oberflaeche eines Brains als PNG auf (Vorlage fuer die Faehigkeits-Vermessung: was im DOM keinen Namen hat, ist im Bild sichtbar)
  survey             Vermisst die Oberflaeche eines Brains im echten DOM und traegt die gefundenen Optionen als `ui_options` ein (der Nenner des Levels)
  probe              Oberflaechen-Analyse wie die Link-Analyse in JDownloader: oeffnet eine Chat-URL, sammelt die Bedienelemente ein und deutet sie als Selektoren-Vorschlaege. Mit `--write` wird das Brain automatisch eingebunden: Selektoren-Datei schreiben + als Custom-Brain registrieren
  quests
  measure-limits     Misst je Brain die zulaessige Eingabelaenge und merkt sie sich dauerhaft
  relay              Single send+wait turn (bot2bot bridge debugging)
  swarm              Multi-Brain-Swarm (Relay je Brain + Synthese). Default menschenlesbar; `--json` fuer CLI-Anbindung
  bot2bot-worker     Autonomer bot2bot-Worker: Inbox pollen, Task via Controller abarbeiten, Ergebnis zurueck an Absender (grok-Aequivalent). Jeder Prozess nutzt ein eigenes isoliertes Profil (Q5-copy) -> N Worker laufen parallel
  workers            Worker-Pool-Manager (Teil 1): haelt N aktive bot2bot-Worker (je ein eigener Kindprozess) am Leben, Failover bei Crash (Brain -> unavailable, naechster Reserve-Brain promoviert). Status pro Brain in pool_state.json (available/active/unavailable, extern re-flaggbar)
  ui                 Lokale Web-UI (eingebettete Assets, Loopback). Default ohne Subcommand. `--api` aktiviert die OpenAI-/Anthropic-kompatible `/v1/*`-Rolle auf demselben Port (ein Listener, zwei Rollen; 8787 bleibt als Dead-Zahl nur im Doku-Archiv)
  tui                Pool/Wand/Bench-TUI. Ohne Subcommand startet `webagent` die Web-UI; `webagent tui` bleibt diese Pool-Wand (Legacy)
  oobe               First-run setup: Brain-Auswahl und optional Login-Hinweise
  autoresearch       Autoresearch: messbare Metrik autonom verbessern (Modify→Verify→Keep/Discard, Git als Sicherheitsnetz, eigener Branch autoresearch/<timestamp>)
  autoresearch-self  Swarm-Selbstbewertung: der Brain-Pool sammelt/konsolidiert/abstimmt die wichtigsten nächsten Verbesserungen (Prioritätsfindung, kein Modify-Loop)
  benchmark          Code-Benchmark: vote-driven objektiver Code-Kompetenz-Score. Der Schwarm stimmt über den nächsten Verbesserungsschritt ab; jedes Brain baut den Sieger sequenziell, gemessen wird hart (Compiler + Tests, kein Selbst-Report)
  maintenance-check  Read-only gate for autonomous maintenance
  sync-master        Spielt die neueste Laufzeit-Kopie (profiles/encapsulated/pool_*) ins Master-Profil zurueck. Manueller Rettungsweg, wenn der TUI-Exit den Rueckweg nicht mehr selbst schafft (alter Binärstand) oder eine Kopie uebrig geblieben ist
  goal               Versionierten Zielvertrag lokal verwalten
  plan               Aktive Arbeitsscheiben des lokalen Zielvertrags verwalten
  api                Lokale OpenAI-/Anthropic-kompatible Provider-Bridge fuer Pi starten
  cloud              Lokale Registry und Free-only-Entscheidungen fÃ¼r optionale Cloud-Textchats
  help               Print this message or the help of the given subcommand(s)

Options:
      --skip-startup-reconcile  Verwaiste Runs beim Start nicht automatisch reconciliieren. Der Check bleibt standardmäßig aktiv; dieses Flag ist für schnelle Teststarts und laufende Diagnose-Sessions gedacht
  -h, --help                    Print help
  -V, --version                 Print version
```

## 3. Findings

- 43 commands in one flat list; several are implementation details
  (`bot2bot-worker`, `sync-master`, `maintenance-check`) next to everyday ones
  (`ask`, `login`, `show`).
- `run`, `relay` and `ask` already overlap by design: `ask` is the successor and
  the two old names are documented as compatible aliases.
- The server role is split between `ui --api` and `api`; the proposed `serve`
  name does not exist yet and would be a new canonical alias, not a removal.
- Two commands (`watchdog`, `quests`) have no description line in their help.

## 4. Proposed groups

Canonical groups: `brain`, `run`, `check`, `api`, `dev`. `dev` also answers to
`advanced`. Every existing command name remains invocable at the root as a
hidden clap alias; grouping adds a second path, never removes the first one.

```text
brain   login, login-all, show, hide, model, survey, measure-limits, oobe, sync-master
run     ask, run, relay, repl, swarm, count, bot2bot-worker, workers, ui
check   diagnose, doctor, watchdog, brains-health, maintenance-check, canary,
        runs-report, verify
api     serve (alias of today's `api`), api
dev     autoresearch, autoresearch-self, benchmark, tui, design-vote, goal,
        plan, cloud, section, mode, menu, toggle, wall, shot, probe, quests
```

`api` keeps a second root-level path to `ui --api` only if the implementation
can do so without changing the listener; otherwise `serve` maps to the current
`api` command and `ui --api` stays documented as it is.

## 5. After: proposed default help (illustrative, not generated)

```text
Gehirnunabhängiger lokaler Agent (Rust-Port)

Usage: webagent [OPTIONS] [COMMAND]

Commands:
  ask                Einheitliche Eingabe: Run oder Einzelturn
  login              Brain sichtbar anmelden
  show               Brain onscreen holen
  diagnose           Live-Diagnose eines Brains
  ui                 Lokale Web-UI (Default ohne Subcommand)
  api                Provider-Bridge (Alias: serve)
  brain              Brain-Zugang, Profile, Selektoren, Limits
  run                Runs, Swarm, Worker, REPL
  check              Offline-/Live-Pruefungen und Belege
  dev                Legacy-TUI, Benchmark, Autoresearch, Ziele, Cloud
  help               Print this message or the help of the given subcommand(s)

Options:
      --skip-startup-reconcile  Verwaiste Runs beim Start nicht automatisch reconciliieren
  -h, --help                    Print help
  -V, --version                 Print version
```

Group help then lists the nested commands with their current descriptions, so
no documentation text is lost; root aliases stay hidden from the default view
but remain callable and appear in `webagent <group> --help`.

## 6. Default-help policy

Visible at the root: the five groups plus `ask`, `login`, `show`, `diagnose`,
`ui` and `api`. Hidden at the root but still callable: all other 37 old names.
This keeps the default help to roughly a dozen lines while making every
existing invocation valid. `webagent --help` and `webagent <group> --help`
together expose all 43 commands.

## 7. Alias table

| Old name (kept) | Canonical path | Visible at root |
|---|---|---|
| `ask` | `run ask` | yes |
| `run` | `run run` | no |
| `relay` | `run relay` | no |
| `repl` | `run repl` | no |
| `swarm` | `run swarm` | no |
| `count` | `run count` | no |
| `bot2bot-worker` | `run bot2bot-worker` | no |
| `workers` | `run workers` | no |
| `ui` | `run ui` | yes |
| `login` | `brain login` | yes |
| `login-all` | `brain login-all` | no |
| `show` | `brain show` | yes |
| `hide` | `brain hide` | no |
| `model` | `brain model` | no |
| `survey` | `brain survey` | no |
| `measure-limits` | `brain measure-limits` | no |
| `oobe` | `brain oobe` | no |
| `sync-master` | `brain sync-master` | no |
| `diagnose` | `check diagnose` | yes |
| `doctor` | `check doctor` | no |
| `watchdog` | `check watchdog` | no |
| `brains-health` | `check brains-health` | no |
| `maintenance-check` | `check maintenance-check` | no |
| `canary` | `check canary` | no |
| `runs-report` | `check runs-report` | no |
| `verify` | `check verify` | no |
| `api` | `api serve` | yes |
| `serve` (new alias) | `api serve` | yes |
| `autoresearch` | `dev autoresearch` | no |
| `autoresearch-self` | `dev autoresearch-self` | no |
| `benchmark` | `dev benchmark` | no |
| `tui` | `dev tui` | no |
| `design-vote` | `dev design-vote` | no |
| `goal` | `dev goal` | no |
| `plan` | `dev plan` | no |
| `cloud` | `dev cloud` | no |
| `section` | `dev section` | no |
| `mode` | `dev mode` | no |
| `menu` | `dev menu` | no |
| `toggle` | `dev toggle` | no |
| `wall` | `dev wall` | no |
| `shot` | `dev shot` | no |
| `probe` | `dev probe` | no |
| `quests` | `dev quests` | no |

## 8. Boundaries and open questions

- Design only: no clap attribute, no shell completion, no documentation outside
  this file and the T-1005 proof was changed.
- Not decided here: whether `serve` should map to `api` or to `ui --api`; that
  needs an owner decision before implementation because it touches role
  routing. The alias table above chooses the non-invasive option (`api`).
- Not decided here: whether hidden root aliases should appear in
  `webagent help --all` or only in group help.
- No command name is removed; the legacy TUI remains a documented exception
  (`docs/arch/layer_exceptions.txt`).
