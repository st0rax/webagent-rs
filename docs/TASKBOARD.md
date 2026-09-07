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

## Einstieg am 2026-09-07

1. Lies zuerst [`CURRENT_WORK.md`](CURRENT_WORK.md), danach die JSON-Zeile der
   Aufgabe, die du übernehmen willst. `TASKBOARD.json` ist verbindlich;
   `TASKBOARD.md` hilft nur beim Überblick.
2. Claim nur **eine** geplante Aufgabe und setze dann Status, Owner, Branch und
   Datum in der JSON-Datei. T-501 ist der Rahmen; neue Arbeit wird in T-502
   (Modell-Nachprüfung), T-503 (Venice) oder T-504 (Managed Tools) übernommen.
3. Lies vor T-502 [`DEEPSEEK_MODEL_EQUIVALENT.md`](DEEPSEEK_MODEL_EQUIVALENT.md),
   vor T-503 [`PROVIDER_VENICE.md`](PROVIDER_VENICE.md) und vor T-504
   [`MANAGED_TOOLS_LOCAL_MODE.md`](MANAGED_TOOLS_LOCAL_MODE.md).
4. Die Matrix darf nur mit einem frischen, abgelegten Beleg unter
   `docs/proofs/T-501/` verbessert werden. `not_run`, `unreachable` und
   `failed` werden nicht narrativ zu `passed` umgedeutet.
5. Der vorhandene Checkout auf `feature/T-501-effort-rest` ist fremd und
   uncommittiert. Arbeite in einem eigenen Worktree; `CURRENT_WORK.md`
   beschreibt die Grenze und die laufende Instanz.

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
| T-501 | 5 | Alle-Brains-Matrix je Brain (Chat/Streaming/Modell/Anhang/Tools) | live | claude, grok, local | claimed* | chatgpt-codex | feature/T-501-model-reproof |
| T-502 | 5 | Modellauswahl neu abnehmen: DeepSeek-Modus gleichwertig, übrige Provider mit Rückweg | live | codex, local | planned | – | – |
| T-503 | 5 | Venice als Browser-Brain registrieren und seine 13 Matrixzellen abnehmen | code + live | codex, local | planned | – | – |
| T-504 | 5 | Expliziten lokalen Managed-Tools-Modus für die API-Bridge umsetzen | design + code + live | codex, local | planned | – | – |

\* Re-claimed 2026-09-05 nach 2026-09-04 als `done` markiert, obwohl DoD NICHT erfüllt war. Der aktuelle Stand nach Modell-Audit, Qwen-Nachprüfung und Venice-Erweiterung lautet 104/143 passed, 10 failed, 4 unreachable, 25 not_run. T-502 bis T-504 zerlegen die verbleibenden Arbeiten in getrennte Abnahmeverträge.
| T-601 | 6 | rustls-HTTPS-Client + data/providers.json + <10-MB-Budget | code | codex, grok | done | grok-agent | feature/T-601-rustls-https |
| T-602 | 6 | /quelle + UI-Schalter + Session-Source-Scope (manueller Hybrid) | code | codex, claude | done | grok-agent | feature/T-602-quelle-impl |
| T-701 | 7 | Gruppen (2-6), Runden, @Brain, Leader-Synthese | code | codex, claude | done | grok-agent | feature/T-701-swarm-groups |
