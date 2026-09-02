# Umsetzungsstatus WEB_UI_API_TOOL_RESET

> **Handover-Datei.** Wer den Plan `docs/WEB_UI_API_TOOL_RESET.md` weiterfuehrt,
> beginnt hier. Stand ist nach jedem Umsetzungsschritt zu aktualisieren.

## Repo / Branch / Stand

- Branch: `master`
- Remote: `https://github.com/st0rax/webagent-rs.git`
- Letzter Stand (Commit): `8e25d36` (2026-09-02)
- Tags: v0.2.1, v0.5.0, v0.7.0–v0.11.0, `tui-ui-preservation-2026-09-01`
- Multidev-Betrieb: `docs/TASKBOARD.json` (Claim-Quelle der Wahrheit),
  `docs/TASKBOARD.md` (Spiegel), `docs/WORK_CONTRACT.md` (verbindlich);
  Eignungsmarker je Phase im Plan §Aufgabentafel

## Verifikationskommandos (Referenz)

```pwsh
# Default-Gate (jetzt webview-only, TUI hinter Feature)
cargo test --lib

# TUI baut weiterhin hinter seinem Feature
cargo check --features tui

# Ohne Defaultfeatures (CI-Zweig)
cargo check --no-default-features
```

Bekannter Stand Default-Gate (2026-09-02): 1238 passed / 1 ignored / 0 failed.

## Planphasen und Status

Symbolik: [x] erledigt · [~] laeuft · [ ] offen

- [x] Phase 0.1 TUI-Stand als Branch + annotiertes Tag gesichert
      (`archive/tui-ui`, `tui-ui-preservation-2026-09-01`)
- [x] Phase 0.2 Unqualifizierte `100 %`-Aussagen entfernt/qualifiziert
      (GENERIC_MASK_PLAN.md:21, CURRENT_WORK.md:180; Regel im Plan §Aussagegrenze)
- [x] Phase 0.3 Maschinenlesbare Capability-/Konformitaetsmatrix
      → `docs/CAPABILITY_MATRIX.json` angelegt (13 areas × 10 brains = 130
      Zellen, alle `not_run`, Schema v1); Belege folgen je Aufgabe
- [ ] Phase 0.4 Historische Belege als Teilbelege markieren
- [ ] Phase 1.0 Extraktionskarte → erledigt (Dossier: SessionService urn
      `AgentController`+`controller/resume`+`run_store`+`transcript`; ToolRegistry
      aus `controller.rs:659` + `file_actions` + `shell_policy` + `executor`;
      EventStream neu, `sequence_number` fehlt heute; Browser-Schicht nur unter
      `#[cfg(feature="webview")]`)
- [x] Phase 1.1 `SessionService` + `EventStream` UI-neutral herausloesen —
      **Code-Teil erledigt** (`src/session/` + `api_bridge`-Anbindung an den Kern;
      `cargo test --lib` grün, 1232 passed). Hinweis: die `api_*`-Matrixzellen
      sind **Live-Abnahme-Zellen (Phase 3+)**, nicht dieser Code-Teil — sie
      bleiben `not_run`, bis ein echter Brain gegen `/v1/responses` läuft.
- [x] Phase 1.2 `ToolRegistry`-Vertrag + vier Managed Tools (Policy-Grenzen) —
      erledigt (`src/tools.rs`, `cargo test --lib` grün, 1236 passed)
- [x] Phase 1.3 Fake-Brain fuer Textdelta-/Toolloop-/Abort-/Retry-/Exactly-once-Tests —
      erledigt (`src/fakebrain.rs`, Controller-Integrationstests, `cargo test --lib`
      grün, 1236 passed)
- [x] Phase 1.4 Promptbuilder Reiner Chat vs. Managed Agent trennen —
      erledigt (`src/prompts.rs`, getrennte Builder mit Vertragstests, `cargo test --lib`
      grün, 1238 passed)
- [x] Phase 2.1 Eingebettete Assets + Loopback-Serverstart (T-201) —
      erledigt (`src/web_ui.rs`, `web/index.html` via `include_str`, Default ohne
      Subcommand ist die Loopback-UI, `cargo test --lib` 1242 passed / 1 ignored)
- [ ] Phase 2.x rest: Endpunkte (T-202), Fake-Prototyp (T-203)
- [ ] Phase 3.x Claude-Referenz komplettieren
- [ ] Phase 4.x OpenAI-Konformitaetskern (Profile, SSE, State)
- [ ] Phase 5.x Alle Brains
- [ ] Phase 6.x Health-Dashboard + manuelle Quellen
- [ ] Phase 7.x Grok-Bot-Modus (Gruppen)

## Naechste Schritte

1. Phase 1.1 (T-101) **abgeschlossen** (Code-Teil): `SessionService`/`EventStream`
   extrahiert, `api_bridge` nutzt den Kern (`session_service()` + Start-/Delta-/
   Done-Stream), `cargo test --lib` grün (1232 passed); T-101 ist in
   `TASKBOARD.json` auf `done` gesetzt (Live-`api_*`-Zelle bleibt für Phase 3
   vorgemerkt).
2. Phase 1.3 (T-103 Fake-Brain) **abgeschlossen**: `src/fakebrain.rs` bindet
   den deterministischen Fake an den echten Controller-Loop; Toolloop,
   Abort, Retry und Exactly-once sind durch isolierte Integrationstests belegt.
3. Phase 1.4 (T-104) **abgeschlossen**: Reiner Chat und Managed Agent haben
   getrennte Promptbuilder; der Plain-Chat-Pfad injiziert keinen
   `WEBAGENT/1`- oder Toolvertrag.
4. Phase 2.1 (T-201) **abgeschlossen**: eingebettete HTML-Assets, Loopback-only,
   `webagent` / `webagent ui` startet die lokale UI; TUI bleibt `webagent tui`.
5. Naechste Zelle: T-202 (HTTP-Endpunkte). Phase 0.4 bleibt Doku-Nachzug.

## Freigabegrenzen (unverändert)

- zCode-Config (`C:\Users\storax\.zcode\v2\config.json`) nicht anfassen.
- Live-Claude-Web-Abnahme: externe Releasegrenze (Anthropic Consumer Terms).
- perplexity: Funktionsstatus offen bis Endtest (Custom-Brain, Gegenprobe offen).
