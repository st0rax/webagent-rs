<!-- **Referenz: diagnostischer Laufbericht; keine aktuelle Betriebsanweisung.** -->
# Live-Abnahme T-805/T-806/T-807 — Survey 2026-09-12

Frische Oberflaechen-Analyse je Brain (`probe --brain <id> --verify --headless`),
Head `0902593`, Debug-Build mit kopierter WebView2Loader.dll.

| Brain | Composer | Send | model_menu-Verdict | Notiz |
|---|---|---|---|---|
| claude | 95% chat-input | chat-input-send | FAIL=Unreachable | Menuebedienung belegt keinen Modellwechsel |
| chatgpt | 85% prompt-textarea | (hit history-item-19-options) | — | Selektoren wirken verschoben (attach trifft history-item) |
| deepseek | 60% Message DeepSeek | nicht gefunden (erste Scanrunde) | FAIL=Unreachable | Composer gefunden, Send-Button erst nach Editor-Fuellung |
| gemini | 70% Prompt eingeben | 70% Nachricht senden | FAIL=Unreachable | model_menu traf SWARM-Thema statt Modellwahl |
| kimi | 55% role=textbox | (nicht gefunden) | — | schwache Selectoren |
| mistral | 50% div:has-text test | 70% Senden | — | **reasoning_toggle PASS** via echtem Mausklick, Ausgang wiederhergestellt |
| perplexity | 85% ask-input | (read_file Tool-Label) | FAIL=Unreachable | model_menu=Modell-Button |
| qwen | 60% Ask Qwen | 70% Send | FAIL=Unreachable | model_menu=Select Model |
| zai | 85% chat-input | 85% send-message-button | FAIL=Unreachable | **reasoning_toggle PASS aber Rueckweg misslungen** (steht jetzt auf data-selected=true) — echter T-805-Live-Befund |

## Beobachtungen

- `model_menu` klassifiziert jede Menuebedienung ohne nachweisbaren Modellwechsel als
  `FAIL`/Unreachable (`verify --cap model_switch` prueft erst die Laufzeit-Auswahl
  und den Rueckweg) — konsistent mit `verdict_outcome` (Probe belegt nichts -> nie Failed).
- mistral + zai: `reasoning_toggle` Zustandswechsel per echtem Klick belegt (verdict PASS).
- zai: Rueckweg zum Ausgangszustand misslungen — Oberflaeche steht nach dem Probe auf
  `data-selected=true` statt `data-selected=false`; dokumentierbarer Live-Zustand.
- chatgpt: mehrere Selectoren treffen heute andere DOM-Knoten (history-item statt
  attach_button) — Verdacht auf Oberflaechen-Refresh; Neuvermessung noetig.
- deepseek: Send-Button erscheint erst bei gefuelltem Editor (bewusst: zweite Scanrunde).

## Live-Verify 2026-09-12 (`verify --brain <id> --headless`)

| Brain | model_switch | reasoning_effort/toggle | chat | stop_generation | new_chat | projects | web_search |
|---|---|---|---|---|---|---|---|
| chatgpt | failed 2316ms | — | passed 8127ms | unreachable | passed 892ms | passed 1540ms | — |
| claude | failed 1192ms | effort passed 23ms | passed 7725ms | unreachable | passed 953ms | passed 1523ms | — |
| deepseek | — | toggle passed 30263ms | passed 7191ms | failed 0ms | passed 4588ms | — | passed 30254ms |
| gemini | **passed 11067ms** | — | passed 9038ms | unreachable | failed 903ms | — | — |
| kimi | passed 5616ms | effort **failed** 4695ms | passed 30739ms | failed 0ms | passed 1014ms | **failed** 3052ms | — |
| mistral | **unreachable** 4218ms | — | passed 2100ms | passed 0ms | passed 882ms | — | — |
| perplexity | failed 2158ms | — | passed 9403ms | failed 0ms | passed 1183ms | passed 1615ms | — |
| qwen | passed 5449ms | effort passed 23ms | passed 22081ms | passed 0ms | passed 1177ms | — | — |
| zai | passed 10588ms | toggle **unreachable** 4261ms | passed 9892ms | unreachable | passed 1045ms | — | passed 1683ms |

### Highlights

- **gemini model_switch passed** (war bisher failed): echter Laufzeit-Wechsel 3.6 Flash -> 3.5 Flash-Lite,
  Rueckweg ok, 11067ms. Historische Matrix-Zelle aber nicht umgedeutet (additiv belegt).
- **qwen + mistral stop_generation passed** (Stop geklickt, verschwunden, Text eingefroren); gemini/claude/
  chatgpt/zai: unreachable ("Stop-Klick ohne belegbare Wirkung"); deepseek/kimi/perplexity: failed (Stop-Button nie sichtbar).
- **kimi reasoning_effort failed**: Pfad ["Denkaufwand Hoch","Hoch"] geklickt, Beschriftung bleibt 'Schnell'.
- **kimi projects failed**: projects_button-Klick ohne Seitenwechsel (URL unveraendert).
- **gemini new_chat failed**: weder URL-Wechsel noch geleerter Verlauf (1 -> 1).
- **mistral model_switch unreachable**: kein sichtbarer Modellselektor.
- chatgpt/claude/perplexity model_switch failed fail-closed: aktuelles Modell nicht eindeutig in Laufzeitliste
  erkennbar, kein Wechsel versucht.

## T-806 E2E-Receipt (2026-09-12, headless)

- Negativprobe `20260912_045327_f41ae05c` (qwen): Receipt mit fremder run_id -> `Taskabschluss verweigert`,
  `task_completion_rejected` im Eventlog (seq=10), Board bleibt `claimed`, Abbruch.
- Positivprobe `20260912_045415_98fde10c` (qwen, cycles=1): korrekt gebundenes Receipt -> Board `done`,
  kein Rejection-Event. Beweis: `docs/proofs/T-806/verified-completion-2026-09-12.md` + Receipts dort.
- Merkmal: `cmd_run` liest `--completion-receipt` erst im done-Zweig -> deterministischer E2E.

## Windows-Prozessprobe T-807 (2026-09-12)

- Nach 9 Live-Verifys + 5 E2E-Runs: `webagent.exe`-Prozesse: 0, neue `msedgewebview2`-Kinder: 0.
- Profil-Lock `shared.session-writeback.lock` ohne offenen Handle -> unlock.
- 6 fremde `msedgewebview2` gehoeren TeamViewer (PID 2212 ff., seit 11.09. 05:18) — nicht webagent.
- Beleg: `docs/proofs/T-807/windows-process-probe-2026-09-12.json`.

## Naechste Schritte

- Ergebnisse liegen in `docs/proofs/T-501/live_verify_2026-09-12.json` + proofs.jsonl; Matrix `as_of` aktualisiert.
- Offene Brains/Faehigkeiten (voice_input, mode_switch, deep_research, canvas, file_attach, temporary_chat,
  regenerate) je nach Plan in eigenen Diagnose-Laefen anmessen.
- Endabnahme T-807 (Live-Matrix + Windows-Prozessproben + Release) nach ausdruecklicher Freigabe.