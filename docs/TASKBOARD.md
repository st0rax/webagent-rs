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

**Agentenregel:** Jeder Agent darf jeden freien Task übernehmen. Eine
`suitable`-/Kompetenzspalte gibt es bewusst nicht; der Task selbst, sein Scope
und die Gates sind maßgeblich.

**Kanten:** Ein Task ist **blockiert**, bis alle Vorgänger in `depends_on`
(aus `docs/TASKBOARD.json`) `done` sind. Leeres `depends_on` = kann parallel
laufen. Niemals eine verkettete Aufgabe aus dem Block reissen.

**Agenten:** `chatgpt-codex` · `claude-code` · `grok-agent` · `manus` · `local/opencode` · `chief`

| ID | Phase | Aufgabe | Typ | Status | Zuständig | Branch |
|---|---|---|---|---|---|---|---|
| T-001 | 0 | Umsetzungsstatus + Capability-Matrix pflegen (Handover) | docs | done | local/opencode | feat/browser-inference-provider |
| T-101 | 1 | SessionService + EventStream UI-neutral herauslösen (monotone sequence_number) | code | done | local/opencode | feature/T-101-api-bridge |
| T-102 | 1 | ToolRegistry + read/bash/edit/write mit Policy-Grenzen | code | done | local/opencode | feature/T-102-tool-registry |
| T-103 | 1 | Fake-Brain (Textdelta/Toolloop/Abort/Retry/Exactly-once) | code | done | chatgpt-codex | feature/T-103-fakebrain |
| T-104 | 1 | Promptbuilder trennen: Reiner Chat vs. Managed Agent | code | done | chatgpt-codex | feature/T-104-prompt-builders |
| T-201 | 2 | Eingebettete Assets + Loopback-Serverstart (eine Binary) | code | done | grok-agent | feature/T-201-web-ui-server |
| T-202 | 2 | Endpunkte: Session/Capability/Health/Upload/Chat/Stop/Event | code | done | grok-agent | feature/T-202-ui-endpoints |
| T-203 | 2 | Klickbarer Fake-Prototyp (Grok-Layout) + A11y-Gates | code | done | grok-agent | feature/T-203-grok-layout-prototype |
| T-301 | 3 | Claude-Referenz: Delta-Streaming live (Freigabegrenze!) | live | claimed | grok-agent | – |
| T-302 | 3 | Claude: Modelle/Aufwand runtime ermitteln, wählen, nachprüfen | live | done | local/opencode | feature/T-302-claude-runtime-model |
| T-401 | 4 | DTOs feldvollständig + monotone sequence_number in Responses-SSE | code | done | grok-agent | feature/T-401-responses-sse-seq |
| T-402 | 4 | Negativfelder/Fehler: seed/logprobs/... klar ablehnen; IDs/Header | code | done | grok-agent | feature/T-402-reject-unsupported |
| T-403 | 4 | openai-local-state-v1 auf echtem Store + Restart-Test | code | done | grok-agent | feature/T-403-persistent-state |
| T-404 | 4 | SDK-Blackbox: offizielle Python-/JS-SDKs + zwei Clients | tests | done | grok-agent | feature/T-404-sdk-blackbox |
| T-501 | 5 | Alle-Brains-Matrix je Brain (Chat/Streaming/Modell/Anhang/Tools) | live | done | chatgpt-codex | fix/T-501-model-proof |

Matrix-Stand 2026-09-11 (effort-Spalte komplett + perplexity als Custom-Brain): effort nur `claude/qwen passed`; chatgpt/deepseek/gemini/mistral/perplexity/zai `not_run`/`failed` (kein eigenes Effortmenue ausser kimi — Pfad `['Denkaufwand Hoch','Hoch']` klickt, verify-Failed strukturell, Label bleibt 'Schnell'); `auto` `not_run`. `model_switch`: qwen + kimi via Namens-Knoten-Selektoren pass, **gemini (3.6 Flash->3.5 Flash-Lite) + zai (GLM-5.3->GLM-5.3-Flash) via exakter Auswahl-API (ID-Roundtrip) pass 3/3**; chatgpt failed strukturell (Sofort-Pill einzige Modelloption im Free-Tier); deepseek removed by-design (Modus-Bar vom Anbieter entfernt). claude + perplexity model strukturell failed — dokumentierte Audit-Grenze. Offen (DoD NICHT done): `model/auto` (not_run, virtuell ohne Browser), auto RED, zai-GLM-5.3 Webchat flaky 2026-09-07.
| T-601 | 6 | rustls-HTTPS-Client + data/providers.json + <10-MB-Budget | code | done | grok-agent | feature/T-601-rustls-https |
| T-602 | 6 | /quelle + UI-Schalter + Session-Source-Scope (manueller Hybrid) | code | done | grok-agent | feature/T-602-quelle-impl |
| T-701 | 7 | Gruppen (2-6), Runden, @Brain, Leader-Synthese | code | done | grok-agent | feature/T-701-swarm-groups |

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
| T-907 | API-Bridge-Routing und Request-Dispatch isolieren | done `grok-agent` / `refactor/T-907-api-bridge-routing` |
| T-908 | API-Bridge-Provider-Handler fachlich trennen | done `grok-agent` / `refactor/T-908-api-bridge-handlers` |
| T-909 | API-Bridge-HTTP-Transport und SSE-Schreiben isolieren | done `grok-agent` / `refactor/T-909-api-bridge-transport` |
| T-910 | API-Bridge-Auth und Fehlervertrag als Boundary-Modul ordnen | done `grok-agent` / `refactor/T-910-api-bridge-boundary` |
| T-911 | API-Bridge-Tests in eigenstaendige Testmodule ordnen | done `grok-agent` / `refactor/T-911-api-bridge-tests` |
| T-912 | API-Bridge-Architektur und Agenten-Einstieg dokumentieren | done `grok-agent` / `docs/T-912-api-bridge-architecture` |
| T-913 | Ergebnisse T-907–T-912 kontrolliert in Root-Datei integrieren | done `grok-agent` / `refactor/T-913-api-bridge-integrate` |
| T-914 | API-Bridge-Medienhandler (Bild/Audio/Multipart) isolieren | Refactoring-Scope: `src/api_bridge/media.rs` |
| T-915 | API-Bridge-Response-Store und Lifecycle isolieren | done `grok-agent` / `refactor/T-915-api-bridge-store` |
| T-916 | API-Bridge-Prompt- und Tool-Normalizer isolieren | done `grok-agent` / `refactor/T-916-api-bridge-content` |
| T-917 | API-Bridge-Modellkatalog und Auto-Router isolieren | done `grok-agent` / `refactor/T-917-api-bridge-catalog` |
| T-918 | API-Bridge-JSON/SSE-Antwortkoerper isolieren | done `grok-agent` / `refactor/T-918-api-bridge-protocol` |
| T-919 | Inference-Lauf verdrahtet — done (grok-agent, 98ad7a5) |
| T-920 | API-Bridge-Modulkarte nach Phase 11 aktualisieren | done `grok-agent` / `docs/T-920-api-bridge-architecture` |
| T-921 | Phase-11-Module kontrolliert in Root-Datei integrieren | Integrations-Scope: `src/api_bridge.rs` | blocked bis Vorgänger done |

**Phase-9-Status (2026-09-12):** T-901 done (zai stabil 4/4, reasoning_toggle-Fix, Beweis docs/proofs/T-901/), T-902 done (AutoRouter-Live-Beleg, proofs/T-902/), T-903 done (chatgpt-Slideover-Drift vermessen, proofs/T-903/), T-904 done (VerifiedFree-Providergrenze dokumentiert, proofs/T-904/), T-905 done (Web-UI echt an /api/* angebunden, Beweis proofs/T-905/), T-906 done (Web-UI-Restluecken: Gruppenlauf-Live, Upload, Brain-Fenster, Beweis proofs/T-906/).

## Phase 10 (ab 2026-09-13) — parallele API-Bridge-Refaktorierung

T-907 bis T-912 sind eigenständige Slots mit disjunkten Zielpfaden.
Jeder Agent claimt genau einen Task in `docs/TASKBOARD.json`, arbeitet nur im
dort genannten Scope und liefert die dort genannten Gates und Belege. Niemand
ändert in diesen Slots `src/api_bridge.rs`; das macht erst T-913 nach Abschluss
aller Vorgänger. T-913 ist deshalb absichtlich blockiert, bis alle Vorgänger
`done` sind.

**Phase-10-Claims (2026-09-13):** T-907–T-913 done (`grok-agent`). Phase 10 abgeschlossen.

## Phase 11 (ab 2026-09-13) — Rest der Root-Datei zerlegen

T-914 bleibt frei/deferred. T-919 claimed (`grok-agent` / `feature/T-919-wire-inference`). T-915–T-918 done (`grok-agent`).
T-914 bis T-920 sind eigenständige Slots mit disjunkten Zielpfaden.
Niemand ändert in diesen Slots `src/api_bridge.rs`; das macht erst T-921.
Typen (`HttpRequest`, DTOs, `BridgeConfig`) bleiben in der Root-Datei, bis T-921
sie bei Bedarf nur verdrahtet — kein paralleler Types-Slot.

| Task | Inhalt |
|---|---|
| T-914 | Medien (Bild/Audio/Multipart) |
| T-915 | Store + retrieve/delete/input_items |
| T-916 | Prompts, Content, Tools, unsupported fields |
| T-917 | Katalog + Auto-Router |
| T-918 | JSON/SSE-Antwortkörper |
| T-919 | `run_task_blocking` / streaming |
| T-920 | Modulkarte aktualisieren |
| T-921 | Verdrahtung (blocked) |

## Phase 12 (ab 2026-09-13) — Verdrahtungs-Nachzug + Folgearbeit

User-Skips: **T-914** bleibt deferred (nicht claimen). T-919 claimed (`grok-agent`).
Columbo-P0: T-915/T-916 Dateiextrakte ohne `mod` gelten als **unwired ≠ done**;
Nachzug in T-922/T-923.

| Task | Inhalt |
|---|---|
| T-922 | Store echt verdrahten (`mod store`, Root-Duplikate weg) | done `grok-agent` |
| T-923 | Content echt verdrahten — done (grok-agent, 8aa43f8) |
| T-924 | CI-Gate gegen orphan `src/api_bridge/*.rs` |
| T-925 | Phase-11/12 Modulkarte: unwired≠done + Skip T-914/T-919 — done (pflege) |
| T-926 | Free-Cloud: ersten VerifiedFree-Adapter anbinden - done (local/opencode, WebView-OAuth, `feature/T-926-webview-oauth-adapter`) |
| T-927 | Matrix `model/auto` Live-Beleg |
| T-928 | Effort-Spalte ehrlich bereinigen |
| T-929 | Linux-CI Flake `config::profiles` …reclaimed |
| T-930 | Web-UI Smoke gegen Loopback `:8788` |
| T-931 | Catalog echt verdrahten (`mod catalog`, Root-Duplikate weg) |
| T-932 | LICENSE (MIT) committen — done (pflege) |
| T-933 | response_protocol echt verdrahten (T-918-Nachzug) |

T-921 depends_on: T-915/916/917/918/920 + T-922/923/931; ohne T-914/T-919. T-932 parallel (LICENSE).


## Phase 14 (2026-10-15) — Repository onboarding, Bazaar integration, and documentation language

The source of truth remains `docs/TASKBOARD.json`. The Pi-inspired coding-agent prompt is an additional development prompt; it does not replace the existing WebAgent runtime or architecture.

| Task | Description | Status | Branch |
|---|---|---|---|
| T-963 | Integrate the Pi-inspired coding-agent prompt into the Bazaar workflow | done — chatgpt-codex | `docs/T-934-bazaar-agent-prompt` |
| T-964 | Make the repository understandable without chat context | done — chatgpt-codex | `docs/T-964-newcomer-onboarding` |
| T-965 | Standardize maintained project documentation on English | done — chatgpt-codex | `docs/T-965-english-first-docs` |
| T-967 | Harden the release gate: tag/manifest/tests and duplicate CI runs | done - local/opencode | `chore/T-967-release-gate` (unabhaengige Umsetzung auf `chore/T-967-release-gate-opencode`) |

T-964 was completed in PR #73 (merge commit `76ba3b30`). The central newcomer guide is `docs/NEWCOMER_GUIDE.md`, linked from the README.

**Documentation language policy:** English is the default for maintained documentation, task titles, agent instructions, issues, and pull requests. Preserve dated historical reports and evidence when rewriting them would harm traceability; add an English summary when useful. User-facing output language is a separate product/runtime decision.

T-965 scope: translate the README and newcomer guide, record this policy in maintained entry points, and keep the taskboard mirror and current handover consistent. No Rust code, runtime, login-profile, or release changes.

## Phase 13 follow-up (2026-11-25) — live capability gaps

The matrix still carries the structural gaps in `model`, `effort`, and `managed_tools`. On 2026-11-25 the following sessions were verified as logged in without a new login: `chatgpt`, `deepseek`, `gemini`, `kimi`, `qwen`, and `zai`; `claude` and `mistral` were logged out, and `perplexity` was not registered in the local configuration. T-968 therefore starts with the reproducible kimi effort failure instead of a speculative Claude/Perplexity model change.

| Task | Description | Status | Branch |
|---|---|---|---|
| T-968 | kimi: make the reasoning-effort roundtrip live-verifiable (full trigger label, real submenu entries, change + restore) | done — local/opencode | `fix/T-968-kimi-effort-roundtrip` |

T-968 result: live `verify` passed headless on the existing kimi session —
`Schnell Hoch` → `Standard`, independently read as `Schnell Standard`, original
value restored in the same session (10054 ms). Raw transcript and DOM survey:
`docs/proofs/T-968/README.md`. PR #103 was merged to `master` as squash `a2a144a` after all four CI checks
passed; the remaining structural gaps in `model` and `managed_tools` are
unchanged. No tag, release, or deployment.

## Phase 16 - architecture layers (T-997..T-1005)

Source: `docs/TASKBOARD.json`; edges are `depends_on`. Phase 16 starts at T-997 because
T-976/T-977 are already `done` on the published board and T-978..T-996 are the Play-Mode tasks.
The source fragment `TASKBOARD.refactor.json` was absent from the workspace and git history;
the owner pasted it in chat and approved renumbering the phase-16 tasks to T-997..T-1005.

| ID | Phase | Task | Type | Status | Owner | Branch |
|---|---|---|---|---|---|---|
| T-997 | 16 | Layer ratchet: guard test for module dependency direction and cycles | test | claimed | local/opencode | feature/T-997-arch-layers |
| T-998 | 16 | Break protocol/brain -> browser edges | refactor | claimed | local/opencode | refactor/T-998-protocol-browser-edge |
| T-999 | 16 | Remove config -> worker_pool dependency | refactor | claimed | local/opencode | refactor/T-999-config-worker-pool-edge |
| T-1000 | 16 | Remove config -> brain_limits dependency | refactor | claimed | local/opencode | refactor/T-1000-config-brain-limits-edge |
| T-1001 | 16 | Resolve capability <-> capability_proof cycle | refactor | free | - | - |
| T-1002 | 16 | Resolve remaining TUI-side cycles | refactor | free | - | - |
| T-1003 | 16 | Keep Windows GUI dependencies out of the browser-free build | build | free | - | - |
| T-1004 | 16 | Crate split decision: core / transport / api (design only) | docs | free | - | - |
| T-1005 | 16 | CLI structure: command groups per layer (design only) | docs | free | - | - |

## Machine-readable mirror (generated from docs/TASKBOARD.json)

<!-- TASKBOARD-MIRROR-BEGIN -->
| ID | Status |
|---|---|
| T-001 | done |
| T-101 | done |
| T-102 | done |
| T-103 | done |
| T-104 | done |
| T-201 | done |
| T-202 | done |
| T-203 | done |
| T-301 | done |
| T-302 | done |
| T-401 | done |
| T-402 | done |
| T-403 | done |
| T-404 | done |
| T-501 | done |
| T-601 | done |
| T-602 | done |
| T-701 | done |
| T-801 | done |
| T-802 | done |
| T-803 | done |
| T-804 | done |
| T-805 | done |
| T-806 | done |
| T-807 | done |
| T-808 | done |
| T-901 | done |
| T-902 | done |
| T-903 | done |
| T-904 | done |
| T-905 | done |
| T-906 | done |
| T-907 | done |
| T-908 | done |
| T-909 | done |
| T-910 | done |
| T-911 | done |
| T-912 | done |
| T-913 | done |
| T-914 | done |
| T-915 | done |
| T-916 | done |
| T-917 | done |
| T-918 | done |
| T-919 | done |
| T-920 | done |
| T-921 | done |
| T-922 | done |
| T-923 | done |
| T-924 | done |
| T-925 | done |
| T-926 | done |
| T-927 | done |
| T-928 | done |
| T-929 | done |
| T-930 | done |
| T-931 | done |
| T-932 | done |
| T-933 | done |
| T-934 | done |
| T-935 | done |
| T-936 | done |
| T-937 | done |
| T-938 | done |
| T-939 | done |
| T-940 | done |
| T-941 | done |
| T-942 | done |
| T-943 | done |
| T-944 | done |
| T-945 | done |
| T-946 | done |
| T-947 | done |
| T-948 | done |
| T-949 | done |
| T-950 | done |
| T-951 | done |
| T-952 | done |
| T-953 | done |
| T-954 | done |
| T-955 | done |
| T-956 | done |
| T-957 | done |
| T-958 | done |
| T-959 | done |
| T-960 | done |
| T-961 | done |
| T-962 | done |
| T-963 | done |
| T-964 | done |
| T-965 | done |
| T-967 | done |
| T-968 | done |
| T-969 | done |
| T-970 | done |
| T-971 | done |
| T-972 | done |
| T-973 | done |
| T-974 | done |
| T-975 | done |
| T-997 | claimed |
| T-998 | claimed |
| T-999 | claimed |
| T-1000 | claimed |
| T-1001 | free |
| T-1002 | free |
| T-1003 | free |
| T-1004 | free |
| T-1005 | free |
<!-- TASKBOARD-MIRROR-END -->
