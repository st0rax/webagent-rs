<!-- **Referenz: Beleg der T-934-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-934 Handoff - OpenAI Chat+Tools+Fortsetzung (Pi-Bridge)

- Task: T-934
- Owner: spock
- Branch: `feature/T-934-pi-bridge-roundtrip`
- Portiert: `cb51e1f` cherry-pick auf frischem `origin/master` (kein Blind-Merge; kein Wire-Rollback 922/923/919)
- T-914 Media: deferred, unangetastet

## Kernpfad (DoD)

OpenAI-kompatibler **Chat Completions + Tools + Fortsetzung** gegen lokale Bridge:
1. Systemkontext sichtbar normalisiert
2. Assistant `content:null` + `tool_calls` akzeptiert
3. Tool-Results / Continuations als sichtbarer Kontext
4. Client-Tools an `browser_inference` (Bridge fuehrt Tools **nicht** lokal aus)

## Harness-Abdeckung

| Pfad | Status |
|------|--------|
| Unit `pi_system_null_assistant_and_tool_error_roundtrip` + OpenAI-foermige Prompt/Tool-Tests | abgedeckt |
| OpenAI Chat Completions buffered + tools → browser_inference | Code-Pfad abgedeckt (`handle_openai`) |
| Live Pi (pi-hi / pi-read) | Artefakte vorhanden; Re-Run dieses SHA: **pending** |
| Generisches curl/SDK Chat+Tools | **pending** |
| OpenAI SSE / Anthropic / Responses client-tools | bewusst weiterhin Reject — nicht Scope |
| `/v1/models` only | **kein** Done-Kriterium |

## Separater Note (nicht in diesem PR)

Wiederholte 502 „Composer-Feld nicht gefunden“ = Browser-Inference/Selektor bricht **vor** Tool-Nutzung ab — nicht der T-934 Tools-Pfad. Meta-Chat ohne Tools ≠ Brain ohne Tools. Composer-Timeout hier nur dokumentiert, nicht gefixt.

## Columbo QM

Bitte QM: Chat-Completions+Tools, echte/pending-ehrliche Proofs, kein models-only, kein Wire-Rollback, Bridge fuehrt Pi-Tools nicht lokal aus.
