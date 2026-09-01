# Doku-Index — alle Markdown-Dokumente auf einen Blick

> **Referenz.** Vollständiger Index aller Markdown-Dokumente dieses Repos.
> Das ist die einzige Stelle, die jedes Dokument nach Rolle einordnet — so
> findest du sofort, was du brauchst, statt 45 Dateien zu sichten. Lebende
> Ablage: siehe `docs/OVERVIEW.md` und `docs/CURRENT_WORK.md`; Einstieg:
> `START_HERE.md`.

## So liest du das

- **Betrieb = aktiv** (pflegen/lesen): der dauerhafte Kern.
- **Plan = abgestimmtes Soll** (bei Bedarf erneut lesen).
- **Referenz/Archiv = Historie** (nur zählen/nachmessen; keine Soll-Zustände).
- **Datierte Übergabe = Archiv**; sie werden nicht fortgeschrieben.

Im Zweifel: `START_HERE.md` → `docs/WEB_UI_API_TOOL_RESET.md` →
`docs/WEB_UI_API_TOOL_RESET_STATUS.md` (`docs/TASKBOARD` + `WORK_CONTRACT`).

## Betrieb (lebende Dokumente, keine Archiv-Banner)

| Datei | Rolle |
|---|---|
| `START_HERE.md` | Wurzel-Einstieg für Menschen und Agents — beginne hier |
| `AGENTS.md` | Repo-Regeln / bot-Mapping |
| `CONVENTIONS.md` | Coding-Konventionen |
| `CONTRIBUTING.md` | Beitragsprozess |
| `README.md` | Kurzvorstellung + Einstiegslinks |
| `docs/OVERVIEW.md` | Produkt-/Architekturüberblick (lebender Stand) |
| `docs/CURRENT_WORK.md` | laufende operative Arbeit (lebender Stand) |
| `docs/PROTOCOL_SCHEMA.md` | Protokoll-Schema (lebend) |
| `docs/COLLABORATION.md` | Zusammenarbeit zwischen Agents |
| `docs/ARCHITECTURE.md` | Modul-/Schichtarchitektur (lebend) |
| `docs/API_BRIDGE.md` | API-Bridge-Dokumentation (lebend) |
| `docs/WEB_UI_API_TOOL_RESET.md` | **verbindlicher Umsetzungsplan (3 Flächen)** |
| `docs/WEB_UI_API_TOOL_RESET_STATUS.md` | planbezogene Umsetzungsstatus/Handover |
| `docs/TASKBOARD.md` | Aufgabentafel (Spiegel; Claim-Quelle ist `.json`) |
| `docs/TASKBOARD.json` | Claim-Quelle der Wahrheit |
| `docs/WORK_CONTRACT.md` | **verbindlicher Arbeitsvertrag für Agents** |
| `docs/CAPABILITY_MATRIX.json` | Beleg-Matrix (maschinenlesbar) |

## Pläne (abgestimmte Soll-Zustände; bei Bedarf lesen)

- `docs/BENCHMARK_PLAN.md`, `docs/AUTORESEARCH_PLAN.md`,
  `docs/SELF_RESEARCH_PLAN.md`, `docs/CAPABILITY_PROOF_PLAN.md`,
  `docs/WIKI_MEMORY_PLAN.md`, `docs/GENERIC_MASK_PLAN.md`,
  `docs/CDP_INPROCESS_PLAN.md`, `docs/WEBAGENT_FREE_CLOUD_TEXTCHAT_PLAN.md`,
  `docs/MERGE_AND_PARITY.md`, `docs/PARITY.md`, `docs/BRAINS_AS_WORKERS.md`,
  `docs/CODE_REVIEW.md`, `docs/TUI_DESIGN.md`

## Referenz / Archiv (Historie — nur nachmessen, kein Soll-Zustand)

| Datei | Zweck |
|---|---|
| `CHANGELOG.md` | Versionshistorie |
| `docs/PROVIDER_STATUS.md` | Provider-Verifikationszahlen (historisch) |
| `docs/PROGRESS.md` | älterer Fortschritt (2026-07) |
| `docs/MISSION.md` | historische Übergabe (2026-07-17) |
| `docs/CLAUDE_PROPOSALS.md` | Claude/Qwen/Grok-Vorschläge (2026-07) |
| `docs/GENIUS_COUNCIL_CONCEPT.md` | Ideen-Sammlung (Konzept) |
| `docs/BRAIN_ANALYZE_ADD.md` | Brain-Analyse-Notiz |
| `docs/AGENT_REVIEW_BRIEF_FREE_CLOUD.md` | Agenten-Briefing (2026) |
| `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` | Free-Cloud-Status (historisch) |
| `docs/HANDOVER_TO_CODEX_2026-08-25.md` | Codex-Übergabe (Archiv) |
| `docs/HANDOVER_FROM_CODEX_TO_CLAUDE_2026-08-22.md` | Claude-Übergabe (Archiv) |
| `docs/UEBERGABE_2026-07-28.md` | Übergabe (Archiv) |
| `DEVIN_BRIEFING.md` (Root) | veraltet — Archiv |
| `STATUS_LIVE.md` (Root) | veraltet — Archiv (Stand: OVERVIEW/CURRENT_WORK) |

## Datierte Übergaben (Archiv, nicht fortgeschrieben)

`docs/HANDOVER_*`, `docs/UEBERGABE_2026-07-28.md`, `docs/MISSION.md`.

## Einrichtung für neue Agents

1. Lies `START_HERE.md`.
2. Akzeptiere `docs/WORK_CONTRACT.md`.
3. Wähle freie Aufgabe in `docs/TASKBOARD.md`/`.json`, setze `claimed` (owner, branch, claimed_at).
4. Arbeite reversibel, Gates grün (`cargo test --lib`, `cargo check --features tui`, `--no-default-features`), Beleg in `docs/CAPABILITY_MATRIX.json`.
5. Beim Abschluss: Belegpfad, `docs/WEB_UI_API_TOOL_RESET_STATUS.md`, Zelle `done`.