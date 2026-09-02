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

Bekannter Stand Default-Gate (2026-09-02): nach Merge Phase 2+4 auf master; `cargo test --lib` vor Push.

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
- [x] Phase 2.2 Server-Endpunkte (T-202) —
      erledigt (`src/web_ui_api.rs`: Session/Capability/Health/Upload/Chat/Stop/Event
      auf SessionService + `doctor::run_doctor` ohne Browserstart; HTTP-Wire-Tests;
      `cargo test --lib` 1248 passed / 1 ignored)
- [x] Phase 2.3 Fake-Prototyp (T-203) —
      erledigt (`web/index.html`: Health-Balken oben, System-Kategorien links,
      Chat-Mitte, Fake-Events ohne `fetch`; A11y: Skip-Link, `:focus-visible`,
      `aria-live`, `prefers-reduced-motion`, semantische Buttons; `cargo test --lib`
      1249 passed / 1 ignored)
- [ ] Phase 3.x Claude-Referenz komplettieren
- [x] Phase 4.1 DTOs + Responses-SSE `sequence_number` (T-401) —
      erledigt (`src/api_bridge.rs`: monotone `sequence_number` ab 0 auf jedem
      Responses-SSE-Event; Response/Chat/Model-Felder ergaenzt; `usage` bleibt
      `null`; `cargo test --lib` 1239 passed / 1 ignored). SDK-Blackbox bleibt T-404.
- [x] Phase 4.2 Negativfelder (T-402) —
      erledigt (`unsupported_parameter`/`unsupported_value` fuer seed, logprobs,
      service_tier, n>1 u. a.; unbekannte Felder bleiben toleriert; `X-Request-Id`;
      Responses-IDs `resp_…`; `cargo test --lib` 1241 passed / 1 ignored)
- [x] Phase 4.3 persistenter State (T-403) —
      erledigt (`openai-local-state-v1` je Mandant auf Platte; Retrieve/Delete/
      previous_response_id nach Cache-Reset; `cargo test --lib` 1242 passed / 1 ignored)
- [x] Phase 4.x rest: SDK-Blackbox (T-404) —
      erledigt (`scripts/t404/`: offizielles OpenAI-Python-SDK, OpenAI-JS-SDK,
      urllib, Node-fetch; Loopback-Bridge mit `BridgeConfig.fake_reply`;
      Dumps in `docs/proofs/T-404/`; `cargo test --lib` 1254 passed / 1 ignored).
      Kein clientbezogener Servercode. Live-`api_*`-Zellen bleiben Phase 3/5.
- [ ] Phase 5.x Alle Brains
- [x] Phase 6.1 rustls-HTTPS + providers.json (T-601) —
      erledigt (`src/https_client.rs` HTTP/1.1+tokio-rustls+webpki-roots+ring;
      `docs/providers.example.json`; Keys nur Env-Namen; `cargo test --lib`
      1259 passed / 1 ignored). Release-Gewicht wenn Artefakt existiert <10 MB.
- [ ] Phase 6.x rest: /quelle (T-602), Health-Balken in der UI
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
4. Phase 2 (T-201..T-203) **auf master gemergt**. Phase 4 (T-401..T-404)
   **erledigt**. Naechste Live-Zelle: T-301 (Freigabegrenze). Naechste
   Code-Zelle: T-601. Phase 0.4 bleibt Doku-Nachzug.

## Freigabegrenzen (unverändert)

- zCode-Config (`C:\Users\storax\.zcode\v2\config.json`) nicht anfassen.
- Live-Claude-Web-Abnahme: externe Releasegrenze (Anthropic Consumer Terms).
- perplexity: Funktionsstatus offen bis Endtest (Custom-Brain, Gegenprobe offen).
