> **Lebend (kimi-Nachzug).** Reproof 2026-09-06c nach CoT-Prefix-Filter (#44). Archiv 06b unten.

# T-501 streaming reproof (filter + CoT)

- mistral: passed STREAM_OK (~19s) offscreen — proof `streaming_mistral_2026-09-06b.json` (#42/#43)
- zai: passed STREAM_OK (~22s) offscreen — proof `streaming_zai_2026-09-06b.json` (#42/#43)
- kimi: passed STREAM_OK (~18s) offscreen — proof `streaming_kimi_2026-09-06c.json` (build `d4fb6dd`, PR #44)

Binary: master `d4fb6dd`. headless. port 8788.
**streaming: 9/9** (nach Chrome-Filter + incomplete CoT prefixes).

## Archiv 2026-09-06b (vor #44)
- kimi: failed — joined starts with CoT fragment "The user" (not clean STREAM_OK)
