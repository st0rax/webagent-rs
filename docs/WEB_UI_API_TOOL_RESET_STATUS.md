# Umsetzungsstatus WEB_UI_API_TOOL_RESET

> **Handover-Datei.** Wer den Plan `docs/WEB_UI_API_TOOL_RESET.md` weiterfuehrt,
> beginnt hier. Stand ist nach jedem Umsetzungsschritt zu aktualisieren.

## Repo / Branch / Stand

- Branch: `master`
- Remote: `https://github.com/st0rax/webagent-rs.git`
- Letzter Stand (Commit): `6f9579e` (2026-09-03) — Live-Test-Tage: siehe Planphasen unten
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
 - [x] Phase 3.x Claude-Referenz (T-302) —
       **live erledigt (2026-09-03)** durch local/opencode: `webagent model --brain
       claude` + `--set` belegt Runtime-Liste statt fester Kodierung (live: „Sonnet 5
       Hoch", Fable 5.1, Opus 5 Pro, „Aufwand Hoch") und ehrliche No-op-Nachpruefung
       („bereits aktiv, kein Wechsel noetig"); `model_switch`-Round-Trip via verify =
       Passed (1759ms); Belege `docs/proofs/T-302/`; Matrix `model`/`effort`/`webui_chat`
       fuer claude = passed. (T-301 Delta-Streaming bleibt bei grok-agent.)
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
 - [~] Phase 5.x Alle Brains (T-501, laeuft bei local/opencode)
       **Live-Messung 2026-09-03, alle 9 Brains** via `webagent verify`:
       `chat` (webui_chat) **9×Passed** (zai anfangs failed „ABSENDEKNOPF_DEAKTIVIERT"
       — Root-Cause: Send-Button `disabled:false` aber `pointer-events:none` auf
       Button+Parent, Echtklick rauschte durch; Fix `send_button_pointer_transparent()`
       routet auf synthetischen Klick, Branch `fix/zai-send-absendeknopf`;
       `webui_chat/zai` failed→**passed** 11233ms, Beleg
       `docs/proofs/T-501/zai_send_fix_2026-09-03.jsonl`); `new_chat` 9×Passed;
       `model_switch` Passed bei claude/
       qwen/perplexity/zai, **unreachable** (kein sichtbarer Modell-Selektor) bei
       deepseek/gemini/kimi/mistral/chatgpt; `reasoning_toggle` (effort) Passed bei
       deepseek, unreachable bei zai. Belege `docs/proofs/T-501/`; Matrix gefuellt.
       **Zusaetzlich live ueber die API-Bridge (webagent api serve, :8787) belegt:**
       `api_models` **10×Passed** (alle 9 Brains + `webagent/auto`, Katalog+Modellobjekt,
       Belege `api_models_list_/.perbrain_2026-09-03.json`); `api_chat` **9×Passed**
       (alle 9 echten Brains, echte `POST /v1/chat/completions`, Antwort exakt
       `API_CHAT_OK` finish=stop; Beleg `api_chat_<brain>_2026-09-03.json`; zai bestaetigt
        den pointer-events-Fix auch ueber die Bridge). `api_chat/auto` Timeout >200s ->
        `not_run` (AutoRouter-Inferenz haengt). **Streaming 2026-09-04 (definitive
        Verifikation, capable-on-retry): 6/9 passed** (chatgpt/claude/deepseek/gemini/
        perplexity/qwen - sauberer `STREAM_OK`; kimi Reasoning-Echo, mistral Timestamp+
        Capacity, zai circuit_open -> fehlgeschlagen). **api_responses 2026-09-04: 7/9
        passed** (chatgpt/claude/deepseek/gemini/kimi/perplexity/qwen - sauberer `RESP_OK`;
        mistral flaky/Capacity, zai circuit_open). **Root-Cause:** Ausfaelle sind
        Provider-seitig (circuit_open/Capacity-Cooldowns zai+mistral, kimi streamt
        Gedankengang, chatgpt/claude flaky unter Last) - Bridge relayt sauber, sobald der
        Provider liefert; stabil: deepseek/gemini/perplexity/qwen. **DoD NICHT done**
        (volle 9/9 durch Provider-Instabilitaet und 09-04-Capacity blockiert).
        **security + health 2026-09-04: Bridge-global passed** (deterministisch, ohne
        flaky Browserturn): /health unauth->200 status=ok, /v1/* ohne Token->401,
        unbekannte Modell-ID->404, --bind 0.0.0.0->abgelehnt (Loopback-only); Belege
        `security_*`/`health_liveness_2026-09-04.json`, Matrix `security`/`health`=passed
        (je Brain). **attachment 2026-09-04: 3/4 gemessen passed** (deepseek/gemini/
        perplexity - image_url-Upload 1x1-Rot-PNG -> Antwort exakt RED; qwen fail-closed
        502 "0 von 1 Dateien uebernommen"; chatgpt/claude/kimi/mistral/zai nicht jetzt
        getestet/capacity). **managed_tools 2026-09-04: brain-global failed (by design)**
        - Bridge lehnt aktive Client-Function-Tools bewusst ab (clean browser text
        profile, c15586a, "verwaltete WebAgent-Tools folgen separat"), Request -> 400
        invalid_request_error; Beleg `managed_tools_rejected_2026-09-04.json`. **sources +
        groups 2026-09-04: passed (deterministisch, kein Live-Browser)** - T-602: 7/7
        source_scope-Tests + web_ui_api 12/12 inkl. /api/sessions/{id}/source + /api/quelle
        (Standard Browser-Chat, --save, kein Auto-Routing); T-701: groups-Flow create->
        list->run->events gruen (min 2 Bots, group:<id>, Done). Belege
        `sources_source_scope_deterministic`/`groups_ui_api_deterministic_2026-09-04.json`.
        Offen: effort (7), attachment chatgpt/claude/kimi/mistral/zai, auto-Zellen.
        Belege `docs/proofs/T-501/`.
- [x] Phase 6.1 rustls-HTTPS + providers.json (T-601) —
      erledigt (`src/https_client.rs` HTTP/1.1+tokio-rustls+webpki-roots+ring;
      `docs/providers.example.json`; Keys nur Env-Namen; `cargo test --lib`
      1259 passed / 1 ignored). Release-Gewicht wenn Artefakt existiert <10 MB.
- [x] Phase 6.4 /quelle + Session-Source-Scope (T-602) —
      erledigt (`src/source_scope.rs`, Web-UI `source-switch`, `/quelle` mit `--save` only;
      PR #22 merge `0892fc9`). Health-Balken-Detail bleibt offen.
- [ ] Phase 6.x rest: Health-Balken in der UI
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
