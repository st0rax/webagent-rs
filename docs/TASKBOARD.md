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

**Agenten:** `chatgpt-codex` · `claude-code` · `grok-agent` · `manus` · `local/opencode` · `chief`

| ID | Phase | Aufgabe | Typ | Geeignet | Status | Zuständig | Branch |
|---|---|---|---|---|---|---|---|
| T-001 | 0 | Umsetzungsstatus + Capability-Matrix pflegen (Handover) | docs | local, chief | done | local/opencode | feat/browser-inference-provider |
| T-101 | 1 | SessionService + EventStream UI-neutral herauslösen (monotone sequence_number) | code | codex, claude, local | done | local/opencode | feature/T-101-api-bridge |
| T-102 | 1 | ToolRegistry + read/bash/edit/write mit Policy-Grenzen | code | codex, claude, local | done | local/opencode | feature/T-102-tool-registry |
| T-103 | 1 | Fake-Brain (Textdelta/Toolloop/Abort/Retry/Exactly-once) | code | claude, local | free | – | – |
| T-104 | 1 | Promptbuilder trennen: Reiner Chat vs. Managed Agent | code | codex, claude, local | free | – | – |
| T-201 | 2 | Eingebettete Assets + Loopback-Serverstart (eine Binary) | code | claude, manus | free | – | – |
| T-202 | 2 | Endpunkte: Session/Capability/Health/Upload/Chat/Stop/Event | code | codex, claude | free | – | – |
| T-203 | 2 | Klickbarer Fake-Prototyp (Grok-Layout) + A11y-Gates | code | claude, manus, local | free | – | – |
| T-301 | 3 | Claude-Referenz: Delta-Streaming live (Freigabegrenze!) | live | claude, local | free | – | – |
| T-302 | 3 | Claude: Modelle/Aufwand runtime ermitteln, wählen, nachprüfen | live | claude, local | free | – | – |
| T-401 | 4 | DTOs feldvollständig + monotone sequence_number in Responses-SSE | code | codex | free | – | – |
| T-402 | 4 | Negativfelder/Fehler: seed/logprobs/... klar ablehnen; IDs/Header | code | codex, claude | free | – | – |
| T-403 | 4 | openai-local-state-v1 auf echtem Store + Restart-Test | code | codex, claude | free | – | – |
| T-404 | 4 | SDK-Blackbox: offizielle Python-/JS-SDKs + zwei Clients | tests | codex, local | free | – | – |
| T-501 | 5 | Alle-Brains-Matrix je Brain (Chat/Streaming/Modell/Anhang/Tools) | live | claude, grok, local | free | – | – |
| T-601 | 6 | rustls-HTTPS-Client + data/providers.json + <10-MB-Budget | code | codex, grok | free | – | – |
| T-602 | 6 | /quelle + UI-Schalter + Session-Source-Scope (manueller Hybrid) | code | codex, claude | free | – | – |
| T-701 | 7 | Gruppen (2-6), Runden, @Brain, Leader-Synthese | code | codex, claude | free | – | – |
