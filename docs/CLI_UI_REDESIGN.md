> **Archiv (Design-Entwurf, Stand 2026-09-04).** Datiertes Redesign-Log, keine
> laufende Betriebsanleitung. Grundsatzentscheidung und Sofort-Fixes (Abschnitt 6)
> sind umgesetzt; der Auto-Router im CLI (run/repl/relay `--brain auto`, Default
> `auto`) folgte als Folgepunkt C2 auf `feature/cli-auto-brain`; der
> `ask`-Einheitsbefehl (3.2) auf `feature/cli-ask`. Offen aus Abschnitt 7:
> Port-Vereinheitlichung. aktueller
> Betrieb: README/„Nutzung", `docs/API_BRIDGE.md`, `docs/OVERVIEW.md`.

# CLI-/UI-Schnittstellen-Redesign — Entwurf (Ist-Befund + Vorschlag)

> **Status:** Grundsatzentscheidungen getroffen, Sofort-Fixes umgesetzt (siehe
> Abschnitt 6). Auto-Router im CLI (C2) umgesetzt auf `feature/cli-auto-brain`;
> `ask`-Einheitsbefehl (3.2) umgesetzt auf `feature/cli-ask`.
> Offene Design-Folgepunkte in Abschnitt 7. Datenbasis:
> `src/cli.rs` (898 Z.), `src/main.rs` (680 Z.), `src/web_ui.rs`,
> `src/web_ui_api.rs`, `src/config/brains.rs`, `src/api_bridge.rs` (Auto-Router),
> `src/repl/commands.rs`, README/AGENTS/OVERVIEW/TUI_DESIGN.

---

## 1. Ist-Zustand: die komplette Befehlsflaeche heute

### 1.1 Top-Level-Befehle (`webagent <sub>`)

| Gruppe | Befehl | Zweck |
|---|---|---|
| **Kern (Bedienen)** | `run` | autonomer Run (Default `brain=auto`, Router) |
| | `repl` | interaktive REPL (Default `brain=auto`, Router) |
| | `login` / `login-all` | manueller Login (sichtbares Fenster) |
| | `diagnose` | Live-Diagnose eines Brains |
| | `relay` | einzelner send+wait-Turn |
| | `swarm` | Multi-Brain-Swarm + Synthese (JSON/CLI) |
| | `tui` | Pool/Wand/Bench-TUI (Legacy seit Web-UI) |
| | `ui` | **lokale Web-UI** (Default ohne Subcommand, Port 8788) |
| **Messung/Benchmark** | `benchmark`, `count`, `verify`, `runs-report`, `measure-limits`, `autoresearch`, `autoresearch-self`, `design-vote`, `quests` | Code-/Capability-Matrizen |
| **Diagnose/Health** | `doctor`, `watchdog`, `brains-health`, `canary`, `maintenance-check` | Pre-flight/Repair |
| **Selector-Werkzeuge** | `probe`, `survey`, `shot`, `section`, `mode`, `menu`, `toggle`, `model` | Brain-Oberflaeche vermessen/steuern |
| **Betrieb/Integration** | `api serve`, `bot2bot-worker`, `workers`, `oobe`, `sync-master` | Bridge/Daemons/Setup |
| **Vertrag/Steuerung** | `goal`, `plan`, `cloud` | Ziel/Plan/Registry |

### 1.2 Syntaktische Unregelmaessigkeiten (Ist)

- Mehrere Befehle heissen **anders als ihr clap-Variant-Name** (`--version`-Alias):
  `runs-report` = `RunsReport`, `design-vote`, `bot2bot-worker`, `workers`,
  `ui`, `tui`, `sync-master`, `api serve`, `cloud …`, `autoresearch-self`.
- **Default-Varianten inkonsistent (Stand: teils korrigiert):** `run`/`repl`/
  `relay` → Default `--brain auto` (Router); `api serve` → `--brain chatgpt` +
  `--brain auto` (Auto-Router) aber ohne Default-Brain-Auswahl im Katalog;
  `tui`/`workers` → `--brains` leer = alle.
- **Merkbare Nomenklatur:** `brains` (Liste) vs. `brain` (ein Wert); `set` vs.
  `--write`; `options` überall anders.
- **Zwei Ports:** API-Bridge `8787`, Web-UI `8788` — leicht verwechselbar.

### 1.3 Redundanz: `run` / `repl` / TUI-`/chat` / `/swarm` (Ausgangspunkt des Users)

Alle vier liefern eine **`Task → Brain → Antwort`** auf gemeinsamer Session:

| Oberflaeche | Zweck | gleiche Sitzung? |
|---|---|---|
| `webagent run --task …` | einmaliger autonomer Run, beendet nach `status=done` | neu je Aufruf |
| `webagent repl` | mehrere Aufgaben nacheinander, `/chat <t>` = reine Konversation | hält Session offen |
| `webagent tui` | Pool/Wand/Bench (Worker-Dashboard) | eigene Session POV |
| `webagent ui` (Web-UI) | Sitzungen erstellen/steuern über `/api/sessions` | echte Web-Sessions |

`run`, `repl` und die Web-UI teilen den **Controller/SessionService** — nur die
Eingabe-/Laufzeitform unterscheidet sich. `run` ist eine CLI-Einmal-Kapsel von
`repl` (eine Aufgabe statt Prompt-Loop).

---

## 2. Reibungspunkte / Befunde (das, was „erst beim Testen“ auffaellt)

1. **Default-Drift (P1):** `main.rs:71` startet ohne Subcommand die **Web-UI**
   (`Commands::Ui`, Port 8788). `README.md:153` und `AGENTS.md:136` bezeichnen
   denselben Start aber als **„Session-TUI“**. Drei Wahrheiten, eine ist falsch.
2. **Pi-Absatz an falscher Stelle:** `README.md:151` (ganzer Absatz im Kern-
   „Nutzung“) gehoert laut User nicht dort; vollstaendige Anleitung existiert
   bereits in `docs/API_BRIDGE.md`. Entfernen/Kuenzen → Verweis.
3. **Login-Claim (`P2`):** `README.md:18` „Nutzerkonto bei mindestens einem der
   Chat-Dienste“ ist laut User nicht generell zutreffend (einige Brains gehen
   ohne Login). Bisherige Messung zeigt: deepseek mit frischem No-Login-Profil →
   `session_state=LoginRequired` + 6h-Circuit-Breaker. Widerspruch zum Gast-Modus
   der Provider; mit 6h-Sperre je falschem Maustest teuer zu pruefen. → pro-User
   Vorgehen noetig, nicht blind messen.
4. **`repl` / `/chat`-Dualitaet** (in README erklaert, im CLI nicht sichtbar):
   `repl`-Eingabe laeuft als autonome Aufgabe; nur `/chat` ist reine Konversation.
   Verwirrend, weil „repl“ im Alltag Chat meint.
5. **Kein einheitlicher Auto-Router im CLI-Modell (Ist-Beobachtung; inzwischen
   umgesetzt als C2, siehe Abschnitt 6):** `api serve --brain auto`
   existiert (Bild/Coding/Research-Tools → Ziel-Brain), aber `run/repl/swarm/bot2bot`
   hatten **kein** `--brain auto` und kein `--brain <auto-rule>`. `webagent/auto`
   war nur ein Bridge-Modellkatalog-Eintrag, kein CLI-Pfad.
6. **Seiten-Oberflaeche vs. TUI:** Die neue **Web-UI** ist der Default, aber
   README/AGENTS und die ausgefuehrten Beispiele sprechen durchgehend von TUI/REPL.
   Die Oberflaechen-Empfehlung des Users ist offen: Web-UI (`ui`) vs. Session-TUI.

---

## 2.5 Projektweites Konsistenz-/Redundanz-Audit (redundant · überflüssig · unklar · inkonsistent)

> Vom User als „not-IRL“-Artige Sammelpruefung angefragt: das ganze Projekt auf
> Redundanz, Überflüssiges, Unklares und Inkonsistentes pruefen. Belegte Punkte
> unten — jeder mit Datei/Zeile und Schweregrad. **Noch kein Umsetzungsauftrag.**

### A. Normativ inkonsistent (Kernbefunde)

| # | Befund | Beleg | Schwere |
|---|---|---|---|
| A1 | **Default-Oberflaeche dreifach widerspruechlich.** Code startet ohne Subcommand die **Web-UI** (`Commands::Ui`); README & AGENTS nennen dasselbe „Session-TUI“/„Session-Ansicht“; andere Docs (OVERVIEW/START_HERE) sprechen korrekt von „Web-UI als primaere“ Ebene. Drei Wahrheiten, eine ist falsch. | `src/main.rs:71`, `README.md:153`, `AGENTS.md:136`, `docs/OVERVIEW.md:9`, `docs/CURRENT_WORK.md:17` | **Hoch** |
| A2 | **Flag-Semantik invertiert in Selector-Debug-Befehlen.** `section/mode/menu/toggle/model/shot/survey/probe` nutzen `--visible` (Default false), der Rest `--headless` (Default false = sichtbar). `main.rs` negiert zu `!visible`, d.h. Default dieser Befehle = **unsichtbar**, exakt der Gegensatz zum Rest der CLI. Erkennbar nur beim Testen. | `src/cli.rs:228,239,246,262,278,304,325,346`, `src/main.rs:191,198,206,212,224,231,239,264` | **Hoch** |
| A3 | **Zwei Loopback-Ports, leicht verwechselbar.** API-Bridge `8787`, Web-UI `8788`. Im README steigt man bei Pi mit `api serve` ein; Web-UI ist Default. | `src/cli.rs:534,798` | Mittel |

### B. Redundant / ueberfluessig

| # | Befund | Beleg | Schwere |
|---|---|---|---|
| B1 | **`run`/`repl`/`relay`/`swarm` teilen denselben Turn-Kern** (`relay_single_turn`, Controller-Session). `relay` ist `run`-Einzelturn ohne Actions-Grenze; `run` ist `repl`-Einmal-Kapsel. Kein CLI-Flag bindet sie zusammen; nur Doku erklaert den Unterschied. | `src/commands/ops.rs:263,289,484`, README `run/repl`-Block | Mittel |
| B2 | **Modul-Tabellen dreifach gepflegt.** README (Z.108-120 „Modul/Verantwortung“), `docs/ARCHITECTURE.md` (Modul-Karte) und `docs/OVERVIEW.md` (Kernarchitektur) listen dieselben Module teils mit abweichenden Namen/Dateien und Versionszaehlen (README `v0.10.1`, OVERVIEW/ARCHITECTURE `v0.10.0`/`v0.8.1`). Eine Wahrheit pro Modul fehlt. | `README.md:108-120`, `docs/ARCHITECTURE.md:79-107`, `docs/OVERVIEW.md:67-97` | Mittel |
| B3 | **`sync-master` + TUI-Exit + Pool-Write-back** ueberlappen: drei Wege, eine Vulkan-Kopie ins Master zurueckzuspielen, mit explizitem Kommentar, dass der TUI-Exit den Rueckweg nicht immer selbst schafft. | `src/commands/ops.rs:23`, `src/cli.rs:726-731`, `src/cli.rs:562-572` | Niedrig |
| B4 | **`doctor` vs `diagnose` vs `brains-health` vs `canary` vs `watchdog`**: fuenf Diagnose-Pfade mit ueberlappenden Zwecken (Pre-flight, Login-Zustand, Selektor-Gesundheit, Stale-Runs). Kein gemeinsamer Einstieg, keine einheitliche JSON-Form. | `src/cli.rs` (alle fuenf) | Mittel |

### C. Unklar

| # | Befund | Beleg | Schwere |
|---|---|---|---|
| C1 | **`repl`-Dualitaet**: normale Eingabe = autonome Aufgabe, nur `/chat <t>` = reine Konversation. Widerspricht Erwartung „REPL = Chat“. | `src/repl/commands.rs:370`, README `repl`-Absatz | Mittel |
| C2 | **`--brain auto` nur im `api serve`, nicht im CLI-Run/`repl`**. Auto-Router ist ein Bridge-/Modellkatalog-Feature (`webagent/auto`); wer Rust/Code direkt will, muss bewusst ein Brain waehlen. Ungleiche Faehigkeit zwischen zwei Eingabepfaden. | `src/api_bridge.rs:2604-2716`, `src/cli.rs` (kein `auto` in run/repl) | Mittel |
| C3 | **`api serve --brain` Default = `chatgpt`**, aber `--brain auto` gibt es im Katalog — welche Form der User als Default will (hart `chatgpt` vs. Router) ist unklar. | `src/cli.rs:800` | Niedrig |

### D. Konkret empfohlene Sofort-Fixes (unabhaengig vom groesseren Redesign)

1. **A1:** README.md:153 + AGENTS.md:136 auf „Session-Web-UI“ (Ist) umstellen; `tui` als Operator-Begriff klarstellen.
2. **A2:** Flag-Defaults vereinheitlichen: `--headless` statt `--visible` (bzw. `--visible/--headless` konsistent ueberall, Default sichtbar) — ohne Inversion.
3. **B2:** Modul-Beschreibung zentralisieren; README verweist nur noch auf OVERVIEW/ARCHITECTURE.
4. **C1/C2:** im USAGE-Block `run --brain auto` als empfohlenes Beispiel; `repl`-Dualitaet in der Hilfe sichtbar machen.

---

## 3. Design-Vorschlag (zur Abstimmung — bewusst drei Offenpunkte)

Prinzipien: **eine Sache pro Befehl**, **ein Zweck, ein Weg**, **Defaults, die
zur haeufigsten Nutzung passen**, und **`--json` ueberall**, wo es automatisiert
abgreifbar sein soll.

### 3.1 Offenpunkt A — WELCHE Default-Oberflaeche?

> Der User fragte ausdruecklich nach Defaults (`run settings ui, api?, brain/autorouter`).

| Option | Default ohne Subcommand | Auswirkung |
|---|---|---|
| **A1 (empfohlen)** | Web-UI `ui` (Ist-Code), aber Doku auf „Session-Web-UI” ehrlich umstellen; `login`-Pfad in Web-UI einbettbar. | Nutzer-Erstkontakt = Browser. |
| A2 | Session-TUI bleibt Default (`tui`), Web-UI nur explizit `webagent ui`. | Zurueck zu README-Aussage; Widerspruch zum Ist-Code. |
| A3 | Kontextbasiert: interaktiv (TTY) → TUI, ansonsten Web-UI/batch. | Kein Default-Konflikt, aber Zustandsvielfalt. |

Empfehlung: **A1** — der Ist-Code ist bereits Web-UI; die Doku nachziehen statt
den Code gegen die README zu biegen. TUI bleibt `webagent tui` fuer Operator.

### 3.2 Konsolidierung der Kern-Befehle (Kernstueck des Vorschlags)

Einfuehrung eines **einzigen Eingabe-Befehls** mit explizitem Oberflaechen-Flag,
plus **Smart-Default-Brain**:

```
webagent ask [task]            # Default-Oberflaeche (siehe Offenpunkt A)
    --brain <id>|auto          # default: auto (Router) statt hart chatgpt
    --new|--resume <id>        # Session-Verwaltung (statt eigener run/repl resuen)
    --chat | --auto            # reine Konversation vs. autonome Aufgabe
    --headless                 # Browser-Fenster aus
    --max-cycles N --budget S  # Laufgrenzen (statt dupliziert in run/repl)
    --json                     # einmaliger Ein-/Ausgabe-Turn
```

- **`run`**: wird (in README + Obergruppe) als Kurzform *gesehen*; technisch
  ist der Code unveraendert, aber README nennt `ask --auto --headless` als
  empfohlene Form, `run` bleibt kompatibler Alias.
- **`repl`**: bleibt als zeilenweise REPL; die Dualitaet „normale Eingabe = autonome
  Aufgabe, `/chat` = Konversation” wird in der Hilfe/README sichtbar gemacht,
  alternativ `repl` auf reine Konversation und `ask`/`run` fuer Autonomie.
- **Default-Brain**: `run --brain chatgpt` → vorschlagen `--brain auto`, damit
  Bild/Coding/Research automatisch ein verfuegbares Brain zieht; per
  Umgebungsvariable `WEBAGENT_DEFAULT_BRAIN=auto` steuerbar.
- **`relay`** ist ein `ask --json --headless`-Einzelturn — kann hinter `ask`
  als `--json` laufen; bleibt fuer Skripte als Alias erhalten.

### 3.3 Einheitliche Flags (Konvention)

- **`--brain`** = eine Brain-ID **oder** `auto`; **`--brains`** = Liste (csv).
- **`--json`** ueberall, wo etwas maschinell abgreifbar sein soll.
- **`--visible`/`--headless`** ueberall gleich (Default sichtbar).
- **Port/Router** konsistent: API-Bridge `8787`, Web-UI `8788` → **einen** Port,
  Fuenfstelligen Ziffernraum dokumentieren (8787+8788 bleiben Dead-Zahlen).

### 3.4 Auto-Router in den CLI-Modell-Punkt heben

- `webagent ask --brain auto` nutzt denselben Router wie `webagent/auto`
  (`select_auto_brain`), damit CLI und Bridge ein **gleiches Verhalten** haben.
  Heute ist `--brain auto` nur Bridge-/Modellkatalog-Feature.

---

## 4. README-/AGENTS-Korrekturen (sofort umsetzbar — unabhaengig vom Redesign)

Direktive 9-freundlich in kleinen Commits:

1. `README.md:153` + `AGENTS.md:136`: Default = **Session-Web-UI** (Ist) statt
   „Session-TUI”; TUI bleibt `webagent tui`.
2. `README.md:151`: Pi-Absatz kuerzen auf Verweis auf `docs/API_BRIDGE.md`.
3. `README.md:18`: Login-Claim per-Brain auffuehren/abschwaeachen (siehe 2.3)
   — erst mit dem User klaren, welche Brains ohne Login funktionieren.
4. `README.md` USAGE-Block: um `run/repl/ui/tui/ask`-Zeilen konsolidiert,
   mit `--json`-Hinweis.

---

## 6. Umgesetzt (2026-09-04, Feature-Branch `feature/docs-cli-ui-fixes`)

Getroffene Entscheidungen (aus Abschnitt 5, vom Nutzer beantwortet):

1. **Default-Oberflaeche:** Web-UI (Ist-Code) beibehalten, Doku nachgezogen
   (README/AGENTS/OVERVIEW statt „Session-TUI“ → „Session-Web-UI“).
2. **Kern-Befehle:** `ask`-Einheitsbefehl umgesetzt auf `feature/cli-ask`
   (2026-09-04, siehe unten). `run`/`relay` bleiben kompatible Aliasse.
3. **Auto-Router im CLI (C2):** umgesetzt auf `feature/cli-auto-brain`
   (2026-09-04, siehe unten). `run`/`repl`/`relay` haben jetzt Default
   `--brain auto`; die Aufloesung teilt denselben Router wie die Bridge
   (`classify_auto_route` + `first_available_auto_brain` in `api_bridge.rs`).
4. **A2 (Flag-Inversion) umgesetzt:** `section/mode/menu/toggle/model/shot/
   survey/probe` nutzen jetzt `--headless` (Default sichtbar) statt `--visible`
   (Default unsichtbar + `!visible` in main.rs). 7 Identische cli.rs-Stellen +
   Probe; main.rs-Dispatch ohne Inversion. `cargo check --no-default-features`
   gruen.
5. **Login-Realitaet:** unveraendert offen (Messung kostet 6h-Sperre pro Brain).

### Konkrete Aenderungen in diesem Branch

- `README.md`: Z.16ff Login-Claim neutralisiert („angemeldeter Browser bei dem
  Dienst, den du nutzen willst“); Z.11 „Session-Web-UI“; Z.77 Status-Default;
  Nutzungs-Absatz: Web-UI-Default + repl-Dualitaet klar, Pi-Bridge als separater
  Verweis auf `docs/API_BRIDGE.md`, USAGE-Block um `[--auto]` (login),
  `run/repl/relay` mit `--headless/--json`, `webagent/auto` als Bridge-only
  markiert; Release-Tabelle „Session-Web-UI“.
- `AGENTS.md` Z.135ff: „Session starten“ — Web-UI + Port 8788, repl/tui bleiben.
- `docs/OVERVIEW.md`: Z.7-15 Web-UI-Default statt „Session-Ansicht“/„Session-TUI“.
- Modul-Tabelle in README: Kopf-Verweis auf OVERVIEW/ARCHITECTURE (Details
  zentral dort).
- `src/cli.rs` + `src/main.rs`: A2-Flag-Fix (s. o.).

### Auto-Router im CLI (`feature/cli-auto-brain`, 2026-09-04)

- `src/api_bridge.rs`: Router-Teile (`classify_auto_route`,
  `first_available_auto_brain`, `available_brains`, `AutoPurpose`/`AutoRoute`)
  gegenueber der Lib exportiert; Auswahlkern `first_available_auto_brain_in`
  injizierbar und deterministisch getestet; neue `select_auto_brain_for_cli(task)`
  ohne BridgeConfig (Default-Fall → erstes verfuegbares, nicht gesperrtes Brain).
- `src/cli.rs`: `run`/`repl`/`relay` bekommen Default `brain=auto`.
- `commands/ops.rs`: `resolve_brain_for_task` loest `auto` fuer `run`/`relay`
  auf und validiert explizite IDs (Exit 2 bei unbekannt; `[auto-router]…` auf
  stderr).
- `src/repl/mod.rs`: `run_repl` loest `auto` beim Start auf.
- `README.md`: USAGE `[--brain <id>|auto]`, Auto-Router-Absatz statt des
  überholten „auto nur in der Bridge"-Hinweises.

### ask-Einheitsbefehl (`feature/cli-ask`, 2026-09-04)

- `src/cli.rs`: `Ask`-Command `webagent ask --task "<aufgabe>"` mit
  `--brain <id>|auto` (Default `auto`), `--auto` (Default) vs. `--chat`
  (Konversations-Einzelturn, `conflicts_with = "auto"`), `--resume`,
  `--headless`, `--max-cycles`, `--no-memory`, `--json` (nur `--chat`).
  `--budget` aus 3.2 nicht umgesetzt (kein Laufgrenzen-Budget im Controller
  vorhanden; nicht erfunden).
- `src/main.rs`: `Commands::Ask` → `cmd_ask`-Dispatch.
- `commands/ops.rs`: `cmd_ask` delegiert 1:1 — `--chat` → `relay_single_turn`
  (mit `--json`-Ausgabe {brain/ok/answer/latency_ms/reason}), sonst →
  `cmd_run`. Keine doppelte Run-/Relay-Logik.
- `README.md`: `ask` als empfohlene Eingabe dokumentiert; `run` = `ask --auto`,
  `relay` = Konversations-Einzelturn (Aliasse).
- `run`/`relay`/`repl` technisch unveraendert — `ask` ist neuer Einstieg auf
  denselben Pfaden.

---

## 7. Naechste Schritte (nach Abnahme dieses Branches)

1. Review des Branches `feature/docs-cli-ui-fixes` → merge auf master, push.
2. Offen aus dem Design: **Port-Vereinheitlichung** (3.3) — API-Bridge `8787`
   und Web-UI `8788` auf einen gemeinsamen Port zu legen verlangt eine
   Architektur-Entscheidung (wer bedient wen, wie koexistiert beides auf einem
   Sockel) und bricht konsumierende Skripte. Konkreter Vorschlag, bevor gebaut
   wird: gemeinsame Port-Konstante + `ui`/`api serve` teilen einen Port über
   einen `--port`-Default-Datenpunkt; 8787/8788 als dokumentierte Dead-Zahlen.
3. Login-Realitaet pro Brain: nur mit Nutzer-Freigabe messen (6h-Sperre).