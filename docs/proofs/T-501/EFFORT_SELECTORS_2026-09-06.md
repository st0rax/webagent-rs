> **Archiv.** Selector-/DOM-Notiz + Live-Proofs qwen effort 2026-09-06; kein Betrieb. Lebend: Matrix und docs/WEB_UI_API_TOOL_RESET_STATUS.md + PR #53.

# reasoning_effort selectors (2026-09-06)

Slice: qwen Thinking UI (chatgpt: no separate effort control in composer dump).
No Matrix `passed` here — selectors + DOM note + verify key fix. Kanonisch: PR #53 / `feature/T-501-finish` (#52 superseded).

## qwen — UI found (headless survey)

Commands (laptop binary `target/debug/webagent.exe`):

- `webagent survey --brain qwen --dump --headless`
- `webagent survey --brain qwen --dump --headless --open reasoning_effort_menu`

Findings:

- Composer control `.qwen-thinking-selector` shows current level (`Auto`).
- Opening it yields popup `aria-label=Thinking` with items **Auto**, **Think**, **Fast**.
- On #53: `reasoning_effort_menu` + `reasoning_effort_option` (not only `model_option`) +
  `reasoning_effort_path: ["Fast"]`.
- `select_in_menu_path` aligned with `select_in_menu` (wait/reopen/exact/pointer) in
  `fd55056` / escape fix `8ac6195` — live Fast/Think still needs derfuhrer re-verify.

Artifacts: `docs/proofs/T-501/effort_survey_qwen_*.txt` (also chatgpt dump on #53).

## chatgpt — no separate effort control in composer dump

`webagent survey --brain chatgpt --dump --headless` (logged-in): composer +
sidebar only; no Thinking/Aufwand/effort control among labeled controls.
Effort may live inside the model switcher — **not** configured without a second
open-dump through `model_menu`. Honest: **not added**.

## Non-goals

- Matrix cells not set to `passed`
- HomBot eis
