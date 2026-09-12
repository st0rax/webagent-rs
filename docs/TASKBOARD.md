# Aufgabentafel (Web-UI-, API- und Tool-Neuschnitt)

> **So übernimmst du eine Aufgabe:** Öffne `docs/TASKBOARD.json` und setze bei
> deiner `id` (z. B. `"T-102"`) `"status": "claimed"`, `"owner"` (z. B.
> `"claude-code"`), `"branch"` und `"claimed_at"`. Vollständige Anleitung:
> `../START_HERE.md` Abschnitt 3.

> **Quelle der Wahrheit:** `docs/TASKBOARD.json` — diese Markdown ist ein
> Spiegel. Claims werden nur in der JSON-Datei gesetzt (owner, branch,
> claimed_at). Verbindliche Arbeitsregeln: `docs/WORK_CONTRACT.md`.

**Regel:** Ein Entwickler, eine Aufgabe. Übernahme nur per Claim-Eintrag in
`docs/TASKBOARD.json`. DoD siehe Aufgabe + Arbeitsvertrag.

**Kanten:** Ein Task ist **blockiert**, bis alle Vorgänger in `depends_on`
(aus `docs/TASKBOARD.json`) `done` sind. Leeres `depends_on` = kann parallel
laufen. Niemals eine verkettete Aufgabe aus dem Block reissen.

**Agenten:** `chatgpt-codex` · `claude-code` · `grok-agent` · `manus` · `local/opencode` · `chief`

| ID | Phase | Aufgabe | Typ | Geeignet | Status | Zuständig | Branch |
|---|---|---|---|---|---|---|---|
| T-001 | 0 | Umsetzungsstatus + Capability-Matrix pflegen (Handover) | docs | local, chief | done | local/opencode | feat/browser-inference-provider |
| T-101 | 1 | SessionService + EventStream UI-neutral herauslösen (monotone sequence_number) | code | codex, claude, local | done | local/opencode | feature/T-101-api-bridge |
| T-102 | 1 | ToolRegistry + read/bash/edit/write mit Policy-Grenzen | code | codex, claude, local | done | local/opencode | feature/T-102-tool-registry |
| T-103 | 1 | Fake-Brain (Textdelta/Toolloop/Abort/Retry/Exactly-once) | code | codex, claude, local | done | chatgpt-codex | feature/T-103-fakebrain |
| T-104 | 1 | Promptbuilder trennen: Reiner Chat vs. Managed Agent | code | codex, claude, local | done | chatgpt-codex | feature/T-104-prompt-builders |
| T-201 | 2 | Eingebettete Assets + Loopback-Serverstart (eine Binary) | code | claude, manus | done | grok-agent | feature/T-201-web-ui-server |
| T-202 | 2 | Endpunkte: Session/Capability/Health/Upload/Chat/Stop/Event | code | codex, claude | done | grok-agent | feature/T-202-ui-endpoints |
| T-203 | 2 | Klickbarer Fake-Prototyp (Grok-Layout) + A11y-Gates | code | claude, manus, local | done | grok-agent | feature/T-203-grok-layout-prototype |
| T-301 | 3 | Claude-Referenz: Delta-Streaming live (Freigabegrenze!) | live | claude, local | claimed | grok-agent | – |
| T-302 | 3 | Claude: Modelle/Aufwand runtime ermitteln, wählen, nachprüfen | live | claude, local | done | local/opencode | feature/T-302-claude-runtime-model |
| T-401 | 4 | DTOs feldvollständig + monotone sequence_number in Responses-SSE | code | codex | done | grok-agent | feature/T-401-responses-sse-seq |
| T-402 | 4 | Negativfelder/Fehler: seed/logprobs/... klar ablehnen; IDs/Header | code | codex, claude | done | grok-agent | feature/T-402-reject-unsupported |
| T-403 | 4 | openai-local-state-v1 auf echtem Store + Restart-Test | code | codex, claude | done | grok-agent | feature/T-403-persistent-state |
| T-404 | 4 | SDK-Blackbox: offizielle Python-/JS-SDKs + zwei Clients | tests | codex, local | done | grok-agent | feature/T-404-sdk-blackbox |
| T-501 | 5 | Alle-Brains-Matrix je Brain (Chat/Streaming/Modell/Anhang/Tools) | live | claude, grok, local | done | chatgpt-codex | fix/T-501-model-proof |

Matrix-Stand 2026-09-11 (effort-Spalte komplett + perplexity als Custom-Brain): effort nur `claude/qwen passed`; chatgpt/deepseek/gemini/mistral/perplexity/zai `not_run`/`failed` (kein eigenes Effortmenue ausser kimi — Pfad `['Denkaufwand Hoch','Hoch']` klickt, verify-Failed strukturell, Label bleibt 'Schnell'); `auto` `not_run`. `model_switch`: qwen + kimi via Namens-Knoten-Selektoren pass, **gemini (3.6 Flash->3.5 Flash-Lite) + zai (GLM-5.3->GLM-5.3-Flash) via exakter Auswahl-API (ID-Roundtrip) pass 3/3**; chatgpt failed strukturell (Sofort-Pill einzige Modelloption im Free-Tier); deepseek removed by-design (Modus-Bar vom Anbieter entfernt). claude + perplexity model strukturell failed — dokumentierte Audit-Grenze. Offen (DoD NICHT done): `model/auto` (not_run, virtuell ohne Browser), auto RED, zai-GLM-5.3 Webchat flaky 2026-09-07.
| T-601 | 6 | rustls-HTTPS-Client + data/providers.json + <10-MB-Budget | code | codex, grok | done | grok-agent | feature/T-601-rustls-https |
| T-602 | 6 | /quelle + UI-Schalter + Session-Source-Scope (manueller Hybrid) | code | codex, claude | done | grok-agent | feature/T-602-quelle-impl |
| T-701 | 7 | Gruppen (2-6), Runden, @Brain, Leader-Synthese | code | codex, claude | done | grok-agent | feature/T-701-swarm-groups |

## Brain-Vereinheitlichung (Plan 2026-09-07)

Quelle: TASKBOARD.json; Abnahme und Reihenfolge: [BRAIN_UNIFICATION_PLAN.md](BRAIN_UNIFICATION_PLAN.md). Alle neuen Aufgaben sind free; Abhaengigkeiten gelten.

| Task | Inhalt |
|---|---|
| T-801 | Gemeinsamer Brain-Vertrag und Konformitaetsfixtures |
| T-802 | Einheitliches Fill Verify Submit ohne Doppelversand |
| T-803 | Antwortstream durch Controller REPL Swarm UI und API |
| T-804 | Profil-Leases und gemeinsame Blocker-/Resetbehandlung |
| T-805 | Generische Probe mit bestehendem Capability-Proof-Gate |
| T-806 | Taskabschluss mit verifizierten Run-Belegen und Crashschutz |
| T-807 | Alle-Brains-Live-Abnahme und Windows-Release |
| T-808 | Dauerhaftes Run-Ledger, Prozesssicherheit und Crash-Recovery |

## Phase 9 (ab 2026-09-12) — Folgearbeiten aus dokumentierten Befunden

Quelle: TASKBOARD.json; Abnahme und Reihenfolge: [BRAIN_UNIFICATION_PLAN.md](BRAIN_UNIFICATION_PLAN.md). Alle neuen Aufgaben sind free; Abhaengigkeiten gelten.

| Task | Inhalt | Abgeleitet aus |
|---|---|---|
| T-901 | zai-Webchat-Flaky vermessen und stabilisieren (Thought-Process-Praefix, Normal/DeepThink) | docs/CURRENT_WORK.md Restposten |
| T-902 | auto-Router-Live-Beleg: /v1/chat/completions modell=webagent/auto routet real | CAPABILITY_MATRIX model/auto not_run |
| T-903 | chatgpt-Selektor-Drift nachmessen (attach_button trifft history-item) | docs/diagnostics/2026-09-12-live-survey.md |
| T-904 | Free-Cloud-Scheibe: Registry/decide mit echten VerifiedFree-Adaptern verbinden | docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md |
| T-905 | Web-UI: Fake-Prototyp -> echte API-Anbindung + Layout | docs/WEB_UI_API_TOOL_RESET.md (Status: sonst tote Scheibe) |
| T-906 | Web-UI: T-905-Restluecken schliessen (Gruppenlauf-Live, Upload, Brain-Fenster) | T-905 Beweis verbleibende_grenzen |

**Phase-9-Status (2026-09-12):** T-901 done (zai stabil 4/4, reasoning_toggle-Fix, Beweis docs/proofs/T-901/), T-902 done (AutoRouter-Live-Beleg, proofs/T-902/), T-903 done (chatgpt-Slideover-Drift vermessen, proofs/T-903/), T-904 done (VerifiedFree-Providergrenze dokumentiert, proofs/T-904/), T-905 done (Web-UI echt an /api/* angebunden, Beweis proofs/T-905/), T-906 done (Web-UI-Restluecken: Gruppenlauf-Live, Upload, Brain-Fenster, Beweis proofs/T-906/).
