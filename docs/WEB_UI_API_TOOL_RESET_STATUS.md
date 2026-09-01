# Umsetzungsstatus WEB_UI_API_TOOL_RESET

> **Handover-Datei.** Wer den Plan `docs/WEB_UI_API_TOOL_RESET.md` weiterfuehrt,
> beginnt hier. Stand ist nach jedem Umsetzungsschritt zu aktualisieren.

## Repo / Branch / Stand

- Branch: `feat/browser-inference-provider`
- Remote: `https://github.com/st0rax/webagent-rs.git`
- Letzter Stand (Commit): wird beim jeweiligen Update hier eingetragen
- Tags: v0.2.1, v0.5.0, v0.7.0–v0.11.0, `tui-ui-preservation-2026-09-01`

## Verifikationskommandos (Referenz)

```pwsh
# Default-Gate (jetzt webview-only, TUI hinter Feature)
cargo test --lib

# TUI baut weiterhin hinter seinem Feature
cargo check --features tui

# Ohne Defaultfeatures (CI-Zweig)
cargo check --no-default-features
```

Bekannter Stand Default-Gate (2026-09-01): 1207 passed / 1 ignored / 0 failed.

## Planphasen und Status

Symbolik: [x] erledigt · [~] laeuft · [ ] offen

- [x] Phase 0.1 TUI-Stand als Branch + annotiertes Tag gesichert
      (`archive/tui-ui`, `tui-ui-preservation-2026-09-01`)
- [x] Phase 0.2 Unqualifizierte `100 %`-Aussagen entfernt/qualifiziert
      (GENERIC_MASK_PLAN.md:21, CURRENT_WORK.md:180; Regel im Plan §Aussagegrenze)
- [ ] Phase 0.3 Maschinenlesbare Capability-/Konformitaetsmatrix
      → `docs/CAPABILITY_MATRIX.json` (anlegen, noch ohne Belege)
- [ ] Phase 0.4 Historische Belege als Teilbelege markieren
- [ ] Phase 1.1 `SessionService` + `EventStream` UI-neutral herausloesen
- [ ] Phase 1.2 `ToolRegistry`-Vertrag + vier Managed Tools (Policy-Grenzen)
- [ ] Phase 1.3 Fake-Brain fuer Textdelta-/Toolloop-/Abort-/Retry-/Exactly-once-Tests
- [ ] Phase 1.4 Promptbuilder Reiner Chat vs. Managed Agent trennen
- [ ] Phase 2.x Web-UI-Prototyp + lokaler Server + Health-Endpunkt
- [ ] Phase 3.x Claude-Referenz komplettieren
- [ ] Phase 4.x OpenAI-Konformitaetskern (Profile, SSE, State)
- [ ] Phase 5.x Alle Brains
- [ ] Phase 6.x Health-Dashboard + manuelle Quellen
- [ ] Phase 7.x Grok-Bot-Modus (Gruppen)

## Naechste Schritte

1. Phase 0.3: Matrixdatei belegen + Schema in Menuezeile prüfen.
2. Phase 1.0: Extraktionspunkte in `api_bridge.rs` / `controller.rs` /
   `webview_runtime.rs` lokalisieren (explore-Dossier).
3. Phase 1.1: `SessionService`/`EventStream` verschieben, danach `cargo test --lib`.

## Freigabegrenzen (unverändert)

- zCode-Config (`C:\Users\storax\.zcode\v2\config.json`) nicht anfassen.
- Live-Claude-Web-Abnahme: externe Releasegrenze (Anthropic Consumer Terms).
- perplexity: Funktionsstatus offen bis Endtest (Custom-Brain, Gegenprobe offen).