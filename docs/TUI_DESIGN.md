# TUI-Redesign — animiertes Multi-Agent-Dashboard (pi.dev / grok-CLI Stil)

> **STATUS: DESIGN, ZUR UMSETZUNG FREIGEGEBEN (nach login-persist).** Vorbild:
> pi.dev + grok-CLI-Dashboard. Ziel: die aktuelle `src/tui.rs` (rudimentär) zu
> einem animierten Dashboard mit **Agenten-Umschaltung** ausbauen. Design von
> mistral (Brain-Worker) erarbeitet + Claude an webagents echte Datenquellen
> angepasst (2026-07-17). Umsetzer: qwen. Vor Start `git grep -n "fn.*tui" src/`
> lesen — auf dem Bestehenden aufbauen, nicht neu schreiben.

## 0. Was heute fehlt

`tui.rs` zeigt Pool-Status statisch/listenartig. Storax will: **animiert** (Spinner,
Live-Log, sanfte Übergänge) **und zwischen Agenten umschaltbar** (Dashboard mit
Agenten-Liste + Detail-Pane, wie grok). Crate: **ratatui** + `crossterm` (bereits
Terminal-Backend-Standard). Kein Async-Zwang — Poll im Frame-Loop reicht (Daten
liegen als Dateien vor, s. §5).

## 1. Layout (3 Panes)

```
┌────────────┬─────────────────────────────────────────┐
│ AGENTEN    │ AGENT-STATUS  (brain, state, latenz)     │  ← oben 40%
│ (Liste,    ├─────────────────────────────────────────┤
│  scroll,   │ LIVE-LOG-STREAM  (auto-scroll, 100 Z.)   │  ← mitte 30%
│  Highlight │├─────────────────────────────────────────┤
│  = aktiv)  │ AKTIVE TASKS  (Spinner + Gauge je Task)  │  ← unten 30%
│ 28% Breite │ 72% Breite                                │
└────────────┴─────────────────────────────────────────┘
 Footer: Keybinding-Hints (↑↓ wechseln · t task · x abbrechen · q quit)
```

`Layout::horizontal([Constraint::Percentage(28), Percentage(72)])`, rechts
`Layout::vertical([Percentage(40), Percentage(30), Percentage(30)])`.

## 2. Widgets pro Pane

| Pane | ratatui-Widget | Inhalt |
|---|---|---|
| Agenten-Liste | `List` + `ListState` | ein Eintrag pro Brain; Highlight = ausgewählter Agent; Farbe nach state (grün available / gelb active / rot blocked/stale) |
| Agent-Status | `Block`+`Paragraph` | brain, state, PID, Latenz (letzter Task), letzte Antwort (gekürzt), Heartbeat-Alter |
| Live-Log | `Paragraph` (wrap) + `Scrollbar` | letzte ~100 Zeilen aus `history.jsonl`/Worker-Log des gewählten Brains, auto-scroll ans Ende |
| Aktive Tasks | `List`, je Zeile Custom-Spinner + `LineGauge` | offene Tasks des Brains; Spinner solange laufend, Gauge = Fortschritt/Zeit |
| Latenz-Ampel | `Gauge` | letzte Latenz, grün <5s / gelb / rot >30s |

## 3. Die 3 wichtigsten Animationen (Frame-Loop, Tick ~80–100ms)

Ein **Tick** treibt alle Animationen; `App::on_tick()` inkrementiert einen
`tick: u64` und re-poll't die Dateien nur alle N Ticks (z.B. jede 1s), nicht jeden
Frame (I/O sparen).

1. **Spinner** (laufende Tasks/Worker): Frame-Array
   `["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]`, `frames[(tick % 10) as usize]`.
   Tick 80ms.
2. **Live-Log-Stream**: bei neuen Zeilen `scroll = lines.len().saturating_sub(view_h)`
   (auto-scroll ans Ende), sanftes Nachrücken. Poll der Log-Datei alle ~500ms.
3. **Status-/Fortschritts-Gauge**: `LineGauge`/`Gauge` mit gedämpftem Nachziehen
   (`shown += (target - shown) * 0.2` pro Tick) statt Sprung → wirkt „smooth".

Frame-Loop (sync, crossterm):
```rust
let tick = Duration::from_millis(80);
let mut last = Instant::now();           // Instant::now() ist ok in echtem Code
loop {
    terminal.draw(|f| ui(f, &app))?;
    let timeout = tick.saturating_sub(last.elapsed());
    if event::poll(timeout)? { if handle_key(&mut app, event::read()?) { break; } }
    if last.elapsed() >= tick { app.on_tick(); last = Instant::now(); }
}
```

## 4. Keybindings (grok-artiges Umschalten)

| Taste | Aktion |
|---|---|
| `↑`/`↓` oder `j`/`k` | Agent in der Liste wechseln (ListState) → Detail-Panes laden sofort den Brain |
| `Enter` | Agent „fokussieren" (Log/Tasks dieses Brains anheften) |
| `t` | Task an gewählten Agent: Eingabe-Popup (`Clear`+Input-Block), schreibt `agents/<brain>/inbox/…` bzw. `pool_control.json` |
| `+`/`-` | target_active erhöhen/senken (bestehende Pool-Steuerung beibehalten) |
| `r` | reflag_all (bestehende Funktion) |
| `PageUp`/`PageDown` | Live-Log scrollen |
| `x` | laufenden Task des Brains abbrechen |
| `q`/`Esc` | stop (schreibt `pool_control.json` stop:true) |

## 5. Datenquellen — webagents ECHTE Dateien (nicht erfinden)

- **Pool/Agenten-Status:** `${WEBAGENT_BOT2BOT_ROOT}/workers/pool_state.json`
  (existiert: PoolEntry mit brain/state available|active|unavailable/PID). Steuerung:
  `workers/pool_control.json` (target_active, stop). → Agenten-Liste + Status-Pane.
- **Heartbeat/Latenz:** `workers/heartbeat_<brain>.json` (mtime = Lebendigkeit;
  stale >300s = rot). → Heartbeat-Alter + Ampel.
- **Aktive Tasks / Inbox:** `${WEBAGENT_BOT2BOT_ROOT}/agents/<brain>/inbox/*.msg.txt`
  (offen) + `inbox/_read/` (erledigt). → Task-Liste + Zähler.
- **Live-Log / letzte Antwort:** Worker-`history.jsonl` bzw. Writeback-Dateien pro
  Brain. → Log-Stream + „letzte Antwort".

Alles reine Datei-Reads im Frame-Loop (throttled). Kein `notify`-Crate nötig für v1;
mtime-Vergleich pro Poll genügt. Falls später gewünscht: `notify` als Optimierung.

## 6. Umsetzungs-Reihenfolge (klein halten, testbar)

1. `App`-State: `agents: Vec<AgentView>`, `selected: usize`, `tick: u64`,
   `log_scroll`, `input_mode`. Reine Struct-Logik → unit-testbar (select-wrap,
   on_tick-Spinnerindex, gedämpftes Gauge-Nachziehen).
2. `load_state()`: liest die §5-Dateien in `Vec<AgentView>` (mtime-throttled).
   Test gegen ein Fixture-Verzeichnis.
3. Rendering `ui(f, &app)` — die 3 Panes + Footer, read-only.
4. Keys/Umschalten + Task-Popup (schreibt in Inbox/pool_control).
5. In `webagent tui` verdrahten (Default-Subcommand bleibt), altes tui.rs ersetzen.
6. `cargo clippy --all-targets -- -D warnings` = 0, Unit-Tests grün.

## 7. Grenzen / Nicht-Ziele v1

- Keine Maus-Interaktion (nur Tastatur) — v1.
- Kein eigener Render-Thread; ein Frame-Loop genügt bei ≤8 Agenten.
- Task-Eingabe-Popup validiert nur Nicht-Leer; komplexe Task-Editoren später.
