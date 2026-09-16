<!-- **Referenz: Beleg der T-930-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-930 Handoff - Web-UI Smoke gegen Loopback-API (:8788)

- Task: T-930
- Owner: local/opencode
- Branch: `refactor/T-930-web-ui-smoke` (gepusht, Claim-Voraussetzung)
- Claim: `origin/master` `3216be3` (Status `claimed`, owner `local/opencode`)
- Beweise: `docs/proofs/T-930/gates.txt`

## Smoke-Schritte und Ports

Zwei Lib-Tests in `src/web_ui.rs` (Modul `web_ui::tests`), laufen in CI ueber
den bestehenden Job "Test (volle Features)" (`cargo test --lib`, ci.yml:95):

1. `smoke_ui_assets_health_sessions_chat_gegen_live_listener`
   - Startet die echte `serve()`-Schleife auf 127.0.0.1 mit ephemerem Port
     (Port 0) und einer Bridge-Rolle (`brain: "auto"`, `fake_reply`).
   - prüft in sechs Requests: UI-Asset `/`, Bridge-Health `/health`,
     `/api/health/brains`, `POST /api/sessions` (201 + run_id),
     `POST /v1/chat/completions` mit Bearer (200, Fake-Reply, assistant) und
     denselben Chat ohne Token (401).
2. `smoke_default_port_8788_laesst_sich_starten_wenn_frei`
   - Bindet den Standardport 127.0.0.1:8788 und prueft `/health` der API-Rolle.
   - Port belegt -> skip mit eprintln (kein Flake, jederzeit deterministisch).

Fake statt Live-Brain: `BridgeConfig.fake_reply` ist der Dauer-Bypass der
Browser-Bridge (inference.rs:62/114). Damit ist der Chat-Minimalpfad ohne
Netz/Provider deterministisch; Non-Goal "Keine Live-Brain-Abhaengigkeit"
eingehalten.

## Testausgabe

    running 2 tests
    test web_ui::tests::smoke_default_port_8788_laesst_sich_starten_wenn_frei ... ok
    test web_ui::tests::smoke_ui_assets_health_sessions_chat_gegen_live_listener ... ok
    test result: ok. 2 passed; 0 failed

Voll: `cargo test --features webview --lib` -> 1429 passed; 0 failed; 1 ignored;
clippy `-D warnings`=0; fmt=0; diff-check clean.

## Regressionen

Fängt T-905 (UI bedient echte /api/*-Ebene) und T-906 (Loopback-/Token-Schutz)
am laufenden Listener ab. Kombiniert testet der Smoke den gemeinsamen
Ein-Listener-Zwei-Rollen-Modus wie `api serve` (`main.rs:666`).

## Diff-Scope / Branch / PR

- `src/web_ui.rs`: +118 Zeilen (2 Tests + Helper `spawn_serve`, `smoke_request`,
  `t930_config`).
- `docs/proofs/T-930/`: `gates.txt`, `handoff.md`.
- Branch `refactor/T-930-web-ui-smoke`, Commit als opencode.
- Kein PR-Knopf noetig (hermes-Board-Workflow: Board-done + Merge --no-ff auf
  master, wie T-921/T-924/T-933).