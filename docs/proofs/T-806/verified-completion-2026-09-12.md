<!-- **Referenz: Beleg der Abnahmeprobe; keine aktuelle Betriebsanweisung.** -->
# Verifizierter Taskabschluss T-806 — E2E 2026-09-12

Erster Resum-Versuch (Brain mistral, headless):
- run_id: `20260912_043716_532f5290`, brain_id: `mistral`, status: `done`, cycles: 2
- zugrunde liegende Kernabnahme liegt in `docs/proofs/T-806/`

## Negativprobe (falsches Receipt) — nachgewiesen fail-closed

- run_id: `20260912_045327_f41ae05c`, brain_id: `qwen`, headless, forces Aufgabe T-806
- Receipt wurde mit fremder run_id (`20260912_044259_42f2ec30`) gebunden
- Ergebnis: `Taskabschluss verweigert: Alter/fremder Beleg: Receipt run_id=... != erwartet 20260912_045327_f41ae05c`
- Eventlog `events.jsonl` seq=10: `task_completion_rejected` mit reason + task_id + when
- Board: `status=claimed` (kein done), Exit-Code 1, keine Löschung des Belegs

## Positivprobe (korrektes Receipt) — nachgewiesen done

- run_id: `20260912_045415_98fde10c`, brain_id: `qwen`, headless, cycles: 1
- Aufgabe: „Sende genau eine WEBAGENT/1-Protokollantwort ... final-response mit text: E2E-OK"
- Receipt korrekt auf diese run_id + HEAD `170c1c9` gebunden
- Ergebnis: `[run] task=T-806 status=done proof=docs/proofs/T-806/verified-completion-2026-09-12.md`
- Eventlog: `status_changed`/`meta_saved`, **kein** `task_completion_rejected`
- Board: `status=done`, owner `webagent:qwen`, branch `master`, `done_at` gesetzt

## System-Merkmal (belegt)

`cmd_run` liest `--completion-receipt` erst im done-Zweig nach der Antwort
(`src/commands/ops.rs:845`). Dadurch ist das Receipt deterministisch schreibbar,
auch wenn die run_id erst waehrend des Laufs entsteht (was hier genutzt wurde).
Wrong/foreign/crash receipts werden verworfen und legen eine Rejection an.