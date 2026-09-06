# reasoning_effort selectors (2026-09-06)

Slice: 1–2 brains with UI, not claude/deepseek (already passed).
No Matrix `passed` in this PR — selectors + DOM note only. Claim/DoD untouched.

## qwen — UI found (headless survey)

Commands (laptop binary `target/debug/webagent.exe`):

- `webagent survey --brain qwen --dump --headless`
- `webagent survey --brain qwen --dump --headless --open reasoning_effort_menu`

Findings:

- Composer control `.qwen-thinking-selector` shows current level (`Auto`).
- Opening it yields popup `aria-label=Thinking` with items **Auto**, **Think**, **Fast**.
- Shipped selectors: `reasoning_effort_menu` + `reasoning_effort_path: ["Think"]`.
- `model_option` extended with `.qwen-chat-v2-dropdown-menu-item` so
  `select_in_menu_path` can click Think/Fast (items are DIVs, not role=menuitem).

Artifacts on laptop checkout (may be copied): `docs/proofs/T-501/effort_survey_qwen_*.txt`.

## chatgpt — no separate effort control in composer dump

`webagent survey --brain chatgpt --dump --headless` (logged-in): composer +
sidebar only; no Thinking/Aufwand/effort control among labeled controls.
Effort may live inside the model switcher (Claude-style) — not configured here
without a second open-dump through `model_menu`. Honest: **not added**.

## Non-goals

- Matrix cells not set to `passed`
- T-501 claim/DoD unchanged
- HomBot eis
