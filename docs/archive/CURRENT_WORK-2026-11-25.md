# Archiv - CURRENT_WORK bis 25.11.2026

> **Archiv:** Historische Uebergaben. Operativer Stand: `docs/CURRENT_WORK.md`.

## 2026-11-25 - T-972: Controller und WebView-Runtime aufgeteilt

- **Branch:** `refactor/T-972-controller-webview-split`, Basis `23c10e9` (Claim-Commit). Nur lokale ├änderungen; kein Merge, kein `done`.
- **Ausgangslage:** Entgegen der geclaimten Sch├ñtzwerte hatte `src/controller.rs` 3349 und `src/webview_runtime.rs` 3315 Zeilen. Beide liegen jetzt bei 1909 bzw. 2229 Zeilen.
- **├änderung (reine Bewegung):** `controller::tests` ÔåÆ `src/controller/tests.rs` (1439 Zeilen); `webview_runtime::tests` ÔåÆ `src/webview_runtime/tests.rs` (95 Zeilen); die koh├ñrenten WebView2-Datei-Upload-Helfer (Chooser, FileList-Fallback, Remount-Watch, CDP-Setzen, MIME) ÔåÆ `src/webview_runtime/file_input.rs` (996 Zeilen). Nur `set_file_input_files_cdp` wurde f├╝r den bestehenden Aufrufer `pub(super)`; alle anderen Signaturen, `cfg(windows)`/`cfg(not(windows))`-Pfade und das Verhalten bleiben unver├ñndert. Kein neues Modul mit eigener Verantwortung au├ƒer der Aufteilung.
- **Gleichheit gepr├╝ft:** Testzahl vorher/nachher Controller 44 / Controller 44, WebView 6 / WebView 6; `cargo test --lib` 1517 passed / 0 failed / 1 ignored ÔÇö identisch zur Basis.
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1517 passed / 0 failed / 1 ignored; `git diff --check` exit 0.
- **Grenze:** Kein Login, kein WebView2-Fensterstart, kein Live-Provider-Call, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - T-971: VerifiedFree ehrlich als Registry-Beleg gekennzeichnet

- **Branch:** `fix/T-971-verifiedfree-proof`, Basis `23c10e9` (Claim-Commit). Nur lokale ├änderungen; kein Merge, kein `done`, kein Live-Provider-Call.
- **Befund:** `AccessMode::VerifiedFree` ist der einzige automatisch routbare Free-Status, hatte aber keinen Bezug zum einen Beleg-Gate. Der Live-Nachweis liegt als datiertes Markdown unter `docs/proofs/T-926/webview-oauth-live-result.md` (2026-11-21), nicht als Record in `capability/proofs.jsonl` (dort 0 Treffer auf `free|cloud|webview|stream|oauth`).
- **Entscheidung:** Variante A ÔÇö ehrliche Kennzeichnung ohne Store-Schreibzugriff und ohne Verhaltens├ñnderung. Neu: `VerificationBasis` und `verification_basis()` in `src/free_cloud_chat.rs` (`kind = registry_only_dated_doc`, `at = 2026-11-21`, `proof_path = docs/proofs/T-926/...`, immer `capability_gate = false`); lokale Mock-/Hub-Vertr├ñge werden als `local_contract` ausgewiesen. `cloud list` (indexgleich zu `models`) und `cloud decide` geben das Feld `verification` aus. `decide_route`, `ProofState`, TTL und Store-Schema bleiben unangetastet; `src/capability_proof.rs` wurde bewusst nicht ge├ñndert. Kein nachtr├ñgliches Einf├╝gen eines 2026-11-21-Records.
- **Beleg:** Zwei neue Unit-Tests pr├╝fen die Herkunftsangabe und die unver├ñnderte Routing-Invariante sowie `ProofState::Never` auf einem frischen, unber├╝hrten Store-Pfad.
- **Doku:** `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` nennt den Unterschied zwischen Registry-Beleg und Capability-Gate ausdr├╝cklich.
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1519 passed / 0 failed / 1 ignored; `git diff --check` exit 0. Lokaler CLI-Smoke `cloud decide --model-id webagent/webview-oauth-session` exit 0 mit `capability_gate=false`, `kind=registry_only_dated_doc` und datiertem `proof_path`.
- **Grenze:** Kein Login, keine Profil-/Token-Lekt├╝re, kein zweiter Belegspeicher, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - T-970: Turn-Beobachtungen thread-lokal isoliert

- **Branch:** `fix/T-970-observation-scope`, Basis `23c10e9` (Claim-Commit). Nur lokale ├änderungen; kein Merge, kein `done`, kein Live-Lauf.
- **Ursache:** `src/brain_score.rs` hielt `PENDING_TURN`, `PENDING_WEB_SEARCH` und `PENDING_SOURCE_HINTS` als prozessglobale Mutexe. `record_event_at` konsumierte alle drei bei jedem Event, auch bei Early Returns ohne eigenen Turn. Parallele L├ñufe (Bridge pro Verbindung, Swarm, REPL-Pool, Benchmark) konnten sich gegenseitig ├╝berschreiben; `brain_id` allein w├ñre als Schl├╝ssel nicht ausreichend.
- **├änderung:** Die drei Mutexe sind durch ein `thread_local!`-`RefCell<PendingObservations>` ersetzt. Ein `PendingScope` (RAII) nimmt beim Laufbeginn vorhandene Reste beiseite und stellt sie beim Verlassen wieder her; verschachtelte Scopes sind sicher. Guards an den Laufgrenzen: `src/relay.rs` (`relay_image_generation`, `relay_single_turn_checked`, `run_turn_loop`) und `src/repl/pool.rs` (`isolated_query`, `PersistentBrain::query`). Alle ├Âffentlichen `set_pending_*`/`update_pending_turn`/`pending_turn_snapshot`-Signaturen und das Event-Format bleiben unver├ñndert; `WRITE_LOCK` sch├╝tzt weiterhin nur das Datei-Append.
- **Beleg:** Zwei neue Unit-Tests: `t970_parallelaeufe_teilen_keine_beobachtungen` (zwei Threads, getrennte Websuch-/Quellenhinweise) und `t970_pending_scope_verhindert_reste_aus_frueheren_laeufen` (Reste aus fr├╝herem Lauf werden nicht in den frischen Scope gezogen und nach Ende wiederhergestellt).
- **Gates real gemessen:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1519 passed / 0 failed / 1 ignored; `git diff --check` exit 0.
- **Grenze:** Kein Umbau von Proof-Gate, Brain-Score-Formel oder Event-Schema; kein Login, keine Profil-/Token-Lekt├╝re, kein Live-Provider-Call, kein Tag, kein Release, kein Deployment.

## 2026-11-25 - Greenfield-Abgleich: vier lokale H├ñrtungsscheiben geclaimt

- **Auftrag:** Der Eigent├╝mer hat nach dem Prompt-Abgleich "optimier das projekt soweit m├Âglich" freigegeben. Der urspr├╝nglich zu breit gefasste T-969 wurde auf ausdr├╝cklichen Wunsch in vier Tasks mit je eigenem Branch und Typ zerlegt: **T-969** `docs/T-969-protocol-security` (PROTOCOL.md + SECURITY.md), **T-970** `fix/T-970-observation-scope` (globale Turn-Puffer), **T-971** `fix/T-971-verifiedfree-proof` (Belegpfad), **T-972** `refactor/T-972-controller-webview-split` (God-Files).
- **Claims:** Alle vier stehen in `docs/TASKBOARD.json` auf `claimed`, Eigent├╝mer `local/opencode`, Basis `master` = `1c93cfe`; der Markdown-Spiegel ist nachgezogen. `done` wird erst nach Merge gesetzt. Die Claim-Zeilen werden in einem gemeinsamen Basis-Commit festgehalten, damit jeder der vier Branches dieselbe Tafel tr├ñgt.
- **Arbeitsstand:** Aktuell l├ñuft T-969 auf `docs/T-969-protocol-security`. T-970 bis T-972 sind geclaimt, aber noch nicht begonnen; die Hintergrund-Recherche zu Puffern und Beleg-Gate l├ñuft separat.
- **T-969 (Doku, auf Branch):** `docs/PROTOCOL.md` und `docs/SECURITY.md` neu, aus Parser, Shell-Policy, Datei-Grenzen, Bridge-Auth, Redaktion, Run-Store und dem einen Proof-Gate abgeleitet. Beide tragen ein `**Referenz**`-Banner, damit der Betriebs-Dokumentationstest in `src/startup.rs` gr├╝n bleibt. Gates real gemessen: `cargo fmt --all -- --check` exit 0, `cargo clippy --all-targets -- -D warnings` exit 0, `cargo test --lib` 1517 passed / 0 failed / 1 ignored, `git diff --check` exit 0. Kein Merge, kein `done`.
- **Grenzen unver├ñndert:** Kein Login, keine CAPTCHA-/2FA-Bedienung, keine Lekt├╝re von Profilen, Cookies oder Tokens, kein Live-Provider-Call, kein Tag, kein GitHub-Release, kein Deployment. Die vier Scheiben enden mit lokalen Gates und einem Push auf ihre Arbeitsbranches; ein Merge in `master` braucht weiterhin eine eigene Freigabe.

## 2026-11-25 - Release v0.11.6 mit Canary-Fix ausgeliefert

- **Freigabe:** Der Eigent├╝mer hat mit ÔÇ×k├╝mmer dich drum" die eigenst├ñndige Auslieferung des bereits gemergten Canary-Fix per Patch-Release ├╝bernommen. Ein Produktiv-Deployment blieb nicht freigegeben; gesucht wurde dennoch in Diensten, geplanten Tasks und allen Laufwerken ÔÇô kein Ziel gefunden.
- **PR [#107](https://github.com/st0rax/webagent-rs/pull/107):** Branch `release/v0.11.6-canary`, Versionsbump `0.11.5` ÔåÆ `0.11.6` in `Cargo.toml`/`Cargo.lock`, CHANGELOG und `docs/RELEASE.md` nachgezogen. Alle vier PR-Checks gr├╝n; Squash-Merge `1149dff` auf `master`.
- **Lokale Gates:** `cargo fmt --all -- --check`, `cargo build --locked`, `cargo test --lib --locked`, `cargo clippy --all-targets -- -D warnings`, `git diff --check` gr├╝n. `verify-release-version.ps1 -Tag v0.11.6` ÔåÆ Exit 0, `-Tag v0.11.5` ÔåÆ Exit 1 (Mismatch erwartet).
- **Tag und Publish:** Tag `v0.11.6` auf `1149dff` gesetzt und gepusht. Release-Workflow [`45702611313`](https://github.com/st0rax/webagent-rs/actions/runs/45702611313) vollst├ñndig gr├╝n inkl. `github-release`; sieben Assets, ver├Âffentlicht 21:21:55 UTC, nicht Draft/Prerelease. Alle Hash-Dateien und Asset-Digests gepr├╝ft.
- **Publish-Pfad real belegt:** `download-artifact@v8` und `action-gh-release@v3` im Publish-Job ausgef├╝hrt und gr├╝n; damit ist der zuvor nur statisch gepr├╝fte Pfad nachgewiesen. Eine interne `[DEP0005] Buffer() is deprecated`-Meldung des Node-Runtime wurde dokumentiert; keine `Node.js 20`-Action-Warnung.
- **Lokales Ziel nachgezogen:** `webagent-deploy-target\release\` enth├ñlt jetzt die `v0.11.6`-Assets (EXE-SHA256 `568c5001941ef30df7dbe98f7e9c3a0f14ed8e147bff8458e84d1a3d47f8d5d3`, DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`). Die `v0.11.5`-Dateien liegen unter `v0.11.5-backup\` als R├╝ckweg. `--version` ÔåÆ `0.11.6 (1149dff2f)`; `canary` ÔåÆ Exit 0, 8/8 `ok` (vorher 7/8 Fehlalarm); `brains-health` ÔåÆ Exit 0, 8/8 `selectors=ok`, `bot2bot=MISSING`; UI-Smoke `GET /` und `GET /health` ÔåÆ 200, Prozess gestoppt, Port 8788 frei.
- **Bewertung:** `docs/PRODUCTION_READINESS.md` auf `v0.11.6` umgeschrieben. Zwei fr├╝here Risiken (Canary-Fehlalarm, ungetesteter Publish-Pfad) sind geschlossen; f├╝nf bleiben. Release-Reife ja, Produktivreife im Zielbetrieb weiter nicht belegt ÔÇô kein Produktivziel, kein echter WebView2-Fensterstart/Live-Turn, keine bewusste Restrisiko-Akzeptanz.

## 2026-11-25 - Produktreife-Bewertung und Canary-Korrektur

- **Bewertung:** `docs/PRODUCTION_READINESS.md` trennt Release-Reife (ja) von Produktivreife im Zielbetrieb (nicht vollst├ñndig belegt) und nennt sechs konkrete Restrisiken sowie Go/No-Go-Kriterien.
- **Runtime-Smoke mit dem deployten Binary:** `webagent.exe ui --no-open --skip-startup-reconcile --port 8788` ÔåÆ Port nach 1 s im Listen-Zustand, `GET /` 200 (38358 Byte), `GET /health` 200 (31 Byte); Prozess gestoppt, Port danach frei. `webagent.exe brains-health` ÔåÆ Exit 0, `shared_profile=ok`, 8/8 `selectors=ok`, `bot2bot=MISSING`.
- **Canary-Befund:** Dasselbe Binary meldete `canary` 7/8 `missing_selectors`, obwohl `load_selectors` eingebettete Selektoren korrekt l├ñdt und `brains-health` sie mitz├ñhlt. Ursache: die leichte Probe in `src/canary.rs` pr├╝fte nur die Datei am Build-Pfad. Korrektur plus Unit-Test (`selectors_available`, eingebettete Kopie gilt als vorhanden) sind mit PR [#106](https://github.com/st0rax/webagent-rs/pull/106) (`57584a8`) nach `master` gemergt; alle lokalen Gates und die vier PR-Checks gr├╝n. Die Korrektur ist mit `v0.11.6` ausgeliefert (siehe Abschnitt oben).
- **Grenze:** Publish-Pfad mit den neuen Actions wurde nicht erneut ausgef├╝hrt (taggebunden), kein echter WebView2-Fensterstart, kein Produktivziel. Deployment weiterhin nur lokal.

## 2026-11-25 - Lokales Deployment von v0.11.5

- **Freigabe und Ziel:** Der Eigent├╝mer hat Deployment freigegeben und um einen Zielvorschlag gebeten. Im Workspace gibt es kein `delivery/deploy_webagent_rs.ps1`, kein `runtime-workers/` und kein `webagent-deploy-target/`; `webagent` ist weder im PATH noch als Prozess vorhanden. Gew├ñhlt wurde daher der lokale, klar benannte Zielordner `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\`. **Kein Fern- oder Produktiv-Deployment**, keine PATH-├änderung, kein Prozessstart, keine Zugangsdaten.
- **Quelle und Pr├╝fung:** Exakt die GitHub-Release-Assets `v0.11.5` (kein lokaler Rebuild). EXE-SHA256 `a1baa187bd435e4902beb1b519df7398029212e3b68f33e12940ca6dad4d562c` und DLL-SHA256 `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f` stimmen mit den Release-Digests ├╝berein; nach dem Kopieren erneut gepr├╝ft.
- **Post-Deploy-Check:** `webagent.exe --version` ÔåÆ `webagent 0.11.5 (9b2ad7e6e)`, Exit 0; `WebView2Loader.dll` liegt neben der EXE. Die `.sha256`-Datei des Release liegt ebenfalls im Zielordner.
- **R├╝ckbau:** Zielordner l├Âschen; sonst keine System├ñnderung. Ein Produktiv-Deployment bleibt weiterhin eine gesonderte Entscheidung mit eigenem Ziel.

## 2026-11-25 - CI-Warnungen nach dem Release bereinigt

- **Anlass:** `actions/checkout@v4`, `actions/cache@v4`, `actions/upload-artifact@v4`, `actions/download-artifact@v4` und `softprops/action-gh-release@v2` erzeugten Node-20-Deprecation-Warnungen. Der ver├Âffentlichte Release blieb unver├ñndert.
- **├änderung:** PR [#105](https://github.com/st0rax/webagent-rs/pull/105) (`c1f01f0`) hebt auf checkout v7, cache v6, upload-artifact v7, download-artifact v8 und action-gh-release v3. Alle vier PR-Checks gr├╝n; YAML lokal geparst, lokale Gates gr├╝n. Zwei veraltete T-961-Belegs├ñtze wurden auf den ver├Âffentlichten Release nachgezogen.
- **Verifiziert:** Release-Workflow per `workflow_dispatch` auf `master` ([Lauf `45690875482`](https://github.com/st0rax/webagent-rs/actions/runs/45690875482)) vollst├ñndig gr├╝n; `Copy WebView2Loader.dll` weiterhin ok, `github-release` wie erwartet ├╝bersprungen (kein Tag). Im Log des Laufs: 0 Zeilen mit `Node.js 20` oder `deprecated`. Tag `v0.11.5` zeigt weiter auf `9b2ad7e`; GitHub-Release unver├ñndert mit 7 Assets.
- **Grenze:** Der Publish-Job mit den neuen Action-Versionen wurde nicht ausgef├╝hrt ÔÇö daf├╝r w├ñre ein neuer Tag n├Âtig, der nicht freigegeben ist. Die verwendeten Inputs (`generate_release_notes`, `files`, `merge-multiple`, `path`) wurden statisch gegen die `action.yml` der neuen Versionen gepr├╝ft. Zu diesem Zeitpunkt kein Deployment; siehe Abschnitt zum lokalen Deployment oben.

## 2026-11-25 - Release v0.11.5: Windows-DLL-Pfad korrigiert

- **Fehler:** Der Tag `v0.11.5` auf `8d7eae3` startete den Release-Workflow [`45680524253`](https://github.com/st0rax/webagent-rs/actions/runs/45680524253). `verify`, Android und Linux waren gr├╝n, der Windows-Build lief durch; `Copy WebView2Loader.dll` scheiterte mit `release: WebView2Loader.dll nicht unter build/webview2-com-sys-*/out/x64 gefunden` (`scripts/copy-webview2-loader.ps1:47`). Der `github-release`-Job wurde nicht ausgef├╝hrt: kein GitHub-Release, keine Assets.
- **Ursache:** Mit Rust/Cargo 1.100 liegt `OUT_DIR` unter `target/<profile>/build/webview2-com-sys/<hash>/out`, nicht mehr wie fr├╝her unter `build/webview2-com-sys-<hash>/out`. Das Skript kannte nur das alte Layout. Belegt durch die lokale Datei `target/debug/build/webview2-com-sys/dc268415efa03ec9/out/x64/WebView2Loader.dll` und den CI-Log.
- **Fix:** `Find-Loader` in `scripts/copy-webview2-loader.ps1` pr├╝ft jetzt beide Layouts; die Fehlermeldung nennt beide Pfade. Lokal gepr├╝ft: `-Profile debug` gegen das neue Layout ÔåÆ `[ok]`, simuliertes altes Layout mit `-Profile release` ÔåÆ `[ok]`. Keine Code-, Versions- oder Workflow-├änderung.
- **Freigabe:** Der Eigent├╝mer hat am 2026-11-25 ausdr├╝cklich erlaubt, den bereits gepushten Tag `v0.11.5` nach dem Fix auf den neuen Commit zu legen. Deployment bleibt ausgeschlossen.
- **Abschluss:** Der Fix wurde als `9b2ad7e` nach `master` gepusht. Der Remote-Tag `v0.11.5` wurde mit ausdr├╝cklicher Freigabe gel├Âscht und auf `9b2ad7e` neu gesetzt (alter Tag: `8d7eae3`). Der neue Release-Workflow [`45684483569`](https://github.com/st0rax/webagent-rs/actions/runs/45684483569) ist vollst├ñndig gr├╝n: `verify`, Android, Linux, Windows (`Copy WebView2Loader.dll` jetzt gr├╝n) und `github-release`. Das [GitHub-Release](https://github.com/st0rax/webagent-rs/releases/tag/v0.11.5) ist am 25.11.2026 19:28:56 UTC ver├Âffentlicht, nicht Draft/Prerelease, mit sieben Assets: Windows-EXE, `WebView2Loader.dll`, Linux-, Android-Binary und drei `.sha256`-Dateien. Die drei Hash-Dateien wurden heruntergeladen und stimmen mit den Asset-Digests ├╝berein. Kein Deployment.

## 2026-11-25 - T-961/G-002: Endabnahme des engen Frisch-Chat-Retries

- **Freigabe:** Der Eigent├╝mer hat am 2026-11-25 zun├ñchst die autonome Fortf├╝hrung ÔÇ×bis einschlie├ƒlich endabnahme" freigegeben. Die vorherige Entscheidung ÔÇ×noch nicht abnehmen" und die vom 2026-11-21 (ÔÇ×Keine ├änderung") bezogen sich auf fr├╝here konkrete Strategien und sind damit ersetzt. Anschlie├ƒend hat er separat den vollen Ablauf **PR + Merge + Release `v0.11.5`** freigegeben; Deployment ist weiterhin nicht enthalten.
- **Branch und Eigent├╝merschaft:** `fix/T-961-semantic-denial-retry`, abgezweigt von `master` bei `7481b02`. Code-Commits `c6d8493`, `0e00682`, `b7d8ba4`, sp├ñter `ee33b33` und `08f05b6`, gepusht und mit `origin` synchron. `master` stand zu diesem Zeitpunkt bei `7481b02`; dieser Scheiben-Commit f├╝hrte die finalen Belege und Statusdokumente auf dem Branch nach. Eigent├╝merschaft `local/opencode`.
- **Code:** `src/api_bridge/content.rs`, `src/api_bridge/inference.rs`, `src/api_bridge/provider_handlers.rs`, `src/browser_inference.rs`. Erkennung nur bei ÔÇ×Werkzeugbegriff + Verneinung + Verf├╝gbarkeits-/Zugriffsbegriff"; genau **ein** Retry im frischen Chat mit dem unver├ñnderten urspr├╝nglichen Auftrag plus den bisherigen Tool-Ergebnissen als Daten. Kein Korrektur-Meta, keine `required`-Eskalation. Erneute Verweigerung wird ehrlich gemeldet (`Retry erschoepft`), kein stilles PASS. Fixtures in `src/api_bridge/tests.rs` und `src/browser_inference.rs`. Doku: `docs/API_BRIDGE.md`.
- **Gates real gemessen:** `cargo fmt --check`, `cargo build`, `cargo test --lib` (1516 passed / 0 failed / 1 ignored), `cargo clippy --all-targets -- -D warnings`, `git diff --check` gr├╝n. Ein einmaliges `web_ui::tests::api_rolle_zugriff_mit_token_liefert_modellliste` scheiterte bei laufendem Bridge-Prozess an der Port-Kollision und lief isoliert sowie im Rerun gr├╝n.
- **Kausale Live-Belege (ohne neues Login, nur bestehende chatgpt-Sitzung, prozess-only Token, ohne `--auto-tool-choice-required`):** 14 strict-clean Pi-L├ñufe mit je 5/5 Tool-Calls und Nonces (5ÔÇô10, 12ÔÇô17, 19ÔÇô20), `stopReason_error 0`, Fehlermarker 0, Relay-Korrekturen 0. In **Lauf 10** und **Lauf 12** wurde die semantische Verweigerung live ausgel├Âst; `t961_fresh_chat_retries=1`, danach jeweils bis `call_5` weitergearbeitet, `STRICT_RESULT=PASS`. Transkript-SHA256: `0F022DF4EB9689FB30BC52E7E5C3D625B0B035EAB48F5419248F511DA18288E5` (10) und `8CC2BDB4C6955DD2CFAF839248043AA0233857BFF4BF8EE31D6C83F03DF9E198` (12). `t961_retry_exhausted=0` in allen L├ñufen; rohe Transkripte, Bridge-Logs, Strict-Ausgaben und Negativl├ñufe unter `docs/proofs/T-961/semantic-denial-retry/`, Secret-Scan sauber.
- **Ehrliche Grenze:** Die Ursache des Modellverhaltens bleibt nicht isoliert; keine Kontoeinstellung wurde gepr├╝ft oder ge├ñndert. Negativlauf 11 (4/5) und 18 (1/5) sind vorzeitiger Textabschluss ohne Verweigerung, Lauf 21 (0/5) ein `ABSENDEKNOPF_DEAKTIVIERT`-Sendefehler mit automatischem Circuit-Breaker (`open_until 1795630874`, nicht manuell zur├╝ckgesetzt). Diese Klassen werden vom Retry-Pfad nicht adressiert; der Mehrschritt-Loop bleibt nicht deterministisch.
- **Abnahme:** T-961 ist in `docs/TASKBOARD.json` und `docs/TASKBOARD.md` auf `done` gesetzt, G-002 in `GOALS.md` geschlossen. Die Detailbegr├╝ndung steht in `docs/proofs/T-961/semantic-denial-retry/README.md`. Merge, Tag und GitHub-Release `v0.11.5` sind freigegeben; Deployment bleibt ausgenommen.

## 2026-11-25 - T-961 via PR #104 gemergt, Release v0.11.5 vorbereitet

- **Freigabe:** Der Eigent├╝mer hat nach der Endabnahme ausdr├╝cklich den vollen Ablauf **PR + Merge + Release** freigegeben; Deployment ist weiterhin ausgeschlossen.
- **PR/CI:** PR [#104](https://github.com/st0rax/webagent-rs/pull/104) wurde gegen `master` ge├Âffnet. Alle vier Checks waren gr├╝n (`aarch64`, `build & test`, Windows volle Features, Linux-WebView). Der Squash-Merge ist `ea88203`.
- **Versionsgate lokal:** `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` ÔåÆ Exit 0; `v0.11.4` ÔåÆ Exit 1 wie erwartet. `Cargo.toml` und `Cargo.lock` stehen auf `0.11.5`.
- **Arbeitsstand zu diesem Zeitpunkt:** `master` war bei `ea88203`, Working Tree sauber. Tag und GitHub-Release `v0.11.5` wurden danach nach gr├╝ner Master-CI gesetzt; `release.yml` erzeugte die Artefakte und das GitHub-Release selbst. Kein Deployment.

## 2026-11-25 - T-968 via PR #103 nach `master` gemergt

- **Freigabe und Merge:** Der Eigentuemer hat den Merge ausdruecklich freigegeben.
  PR #103 (https://github.com/st0rax/webagent-rs/pull/103) wurde mit
  `a2a144a` als Squash nach `master` gemergt; alle vier PR-Checks waren vorher
  gruen. `master` lokal und remote steht auf `a2a144a`, kein Tag, kein
  GitHub-Release, kein Deployment. Board zu diesem Zeitpunkt: 93 Tasks /
  92 `done` / 1 `free` (T-961) / 0 `claimed`; `GOALS.md`,
  `docs/OVERVIEW.md` und `CHANGELOG.md` waren auf diesen Stand nachgezogen.
- **Damalige Ausnahme:** T-961/G-002 war zu diesem Zeitpunkt `free`; die
  damaligen rohen Pi-Transkripte waren negativ (0/5 Tool-Turns,
  `session_state=Unbestimmt`). Kein erfundenes PASS und keine erneute
  Anmeldung durch einen Agenten. Die sp├ñtere Endabnahme steht im Abschnitt
  oben; `master` ist dadurch unver├ñndert geblieben.

## 2026-11-25 - T-968: kimi `reasoning_effort` live belegt (umgesetzt im Worktree)

- **Fortsetzung von DeepSeek's Claim:** auf `b073d26` im Worktree
  `webagent-rs-t968-kimi-effort`, Branch `fix/T-968-kimi-effort-roundtrip`.
  Dessen uncommittete Arbeit (`click_key_chain`, `--open`-Ketten-Doku, Surveys)
  wurde uebernommen und weitergefuehrt, nicht verworfen.
- **Ursache geklaert:** die beiden alten Surveys waren byte-gleich ÔÇö das
  Untermenue wurde nie geoeffnet. `reasoning_effort_menu` las nur den Modellnamen
  `Schnell`; `reasoning_effort_option` traf mit `[role='menuitem']` /
  `#v-3-menu-trigger` erneut den Trigger. Der rohe Hover-Survey zeigt
  `aria-expanded=true` und die echten Eintraege
  `[data-testid='model-effort-option'][role='menuitemradio']` = `Standard`,
  `Hoch`. Zusaetzlich lag ein Nutzer-Override ausserhalb des Repos obenauf, der
  den Trigger als Option definierte; nur dieser Key wurde korrigiert, das
  Original liegt als `kimi.json.bak-T968-20261125` im selben Verzeichnis.
- **Live-Beleg (kein neues Login):** `webagent --skip-startup-reconcile verify
  --brain kimi --cap reasoning_effort --headless` ÔåÆ **Passed, 10054 ms**:
  `Schnell Hoch` ÔåÆ `Standard`, unabhaengig gelesen `Schnell Standard`,
  Rueckweg=true. Proof-Store `ts 2026-11-25T02:18:35Z`, `outcome=passed`,
  `selector_hash=2705021080`. Der ehrliche Fehlversuch mit altem Override ist
  ebenfalls gespeichert. Alles roh unter `docs/proofs/T-968/README.md`.
- **Code:** `src/browser/{js,operations,ui,verify}.rs`, `src/brain_probe.rs`,
  `src/cli.rs`, `selectors/kimi.json` ÔÇö Hover-/Trusted-Click-Pfad, echte
  Optionsliste aus dem Live-DOM, Roundtrip mit unabhaengiger Nacherfassung und
  Wiederherstellung; kein zweiter Beleg-Gate.
- **Gates real gemessen:** `cargo fmt`, `cargo build`, `cargo test --lib`
  (1512 passed / 0 failed / 1 ignored), `cargo clippy --all-targets --
  -D warnings`, `git diff --check` alle gruen. Matrix `effort/kimi` nur aus dem
  rohen Lauf auf `passed` gesetzt; Board zu diesem Zeitpunkt 93 Tasks /
  92 `done` / 1 `free` (T-961) / 0 `claimed`, MD-Spiegel synchron.
- **Grenzen:** PR #103 wurde mit ausdruecklicher Freigabe geoeffnet und nach
  gruenen Checks (aarch64, build & test, Linux-WebView, Windows) als Squash
  `a2a144a` nach `master` gemergt; seither kein Tag, kein GitHub-Release, kein
  Deployment. Keine Zugangsdaten, Profile, Cookies oder Tokens gelesen, keine
  Kontoeinstellung geaendert. T-961 war zu diesem Zeitpunkt `free`; die
  spaetere Endabnahme steht im Abschnitt oben. Der Branch wurde als
  `88918df` (Umsetzung), `141736b` und `4da55d9` (Handoff) committet und
  gepusht; die Uebergabe enthaelt Pfad, Zweck und Eigentuemerschaft
  (`local/opencode`).

## 2026-11-25 - Projektabschluss im erreichbaren Umfang

- **Entscheidung:** Der Eigentuemer hat klargestellt, dass er sich nicht erneut
  anmelden wird. Es wird deshalb **kein** weiterer Live-Lauf und keine
  Anmeldungs-, Einmalcode- oder CAPTCHA-Bedienung durch einen Agenten verlangt.
- **Stand:** T-967 ist via PR #102 (`c0890d5`) auf `master` gemergt; alle
  PR- und Master-CI-Laeufe sind gr├╝n; `cargo test --all-targets --locked`
  zusaetzlich gr├╝n (1505 + 7 passed). Board zu diesem Zeitpunkt: 92 Tasks /
  91 `done` / 1 `free` (T-961), kein `claimed`.
- **Damalige Ausnahme:** G-002/T-961 war zu diesem Zeitpunkt `free`. Die
  damaligen rohen Pi-Transkripte sind negativ (0/5 Tool-Turns,
  `session_state=Unbestimmt`); daraus wurde kein PASS erfunden. Die spaetere
  Endabnahme mit kausalem Retry-Beleg steht im Abschnitt oben.
- **Grenzen:** kein neuer Tag, kein GitHub-Release, kein Deployment. Latest
  Release bleibt `v0.11.4`; der historische Tag ist unangetastet.
- **DeepSeek-Review (01:33 Uhr):** der Child-Agent war wieder verfuegbar und
  hat den gemergten Stand read-only mit `review ok` bewertet. Zwei minimale
  Befunde eingearbeitet: Taskboard-Branch/Handoff auf den gemergten Stand
  korrigiert und `--locked` auch in die drei Release-Builds sowie den
  Android-Build ergaenzt. Der Android-Target ist lokal ohne NDK nicht
  nachgefahren; CI prueft ihn. Kein Logikfehler am Gate gefunden.

## 2026-11-24 ÔÇö T-967 unabhaengig im getrennten Worktree umgesetzt

- **Parallele Arbeit:** DeepSeek arbeitet auf `chore/T-967-release-gate`; zur
  Vermeidung von Dateikonflikten liegt diese Umsetzung im Worktree
  `webagent-rs-t967-opencode` auf `chore/T-967-release-gate-opencode` (ab
  `b7d75fd`). Der Versuch, einen neuen DeepSeek-Child-Agenten zu starten,
  scheiterte mit `Free usage limit reached` (Reset laut Eigentuemer 1:31); von
  diesem Child wurde nichts ausgefuehrt. Eine laufende DeepSeek-Session kann
  nicht von hier gesteuert werden.
- **Implementierung:** `scripts/verify-release-version.ps1` (fail-closed,
  Exit 0/1/2, nur `Cargo.toml` + `Cargo.lock`); `release.yml` mit `verify`-Job
  (Versionspruefung nur fuer Tag-Refs plus `cargo test --lib --locked`) und
  `needs: [verify]` an allen Build-/Publish-Jobs, `contents: read` fuer Verify
  und Builds; `ci.yml` mit Push nur auf `master`, `contents: read` f├╝r alle
  Jobs und `--locked` in jedem Cargo-Schritt; Version `0.11.5` in Manifest
  und Lockfile; `docs/RELEASE.md`,
  `CHANGELOG.md`, Beleg und Board nachgezogen.
- **Gates (real gemessen):** Skript `v0.11.5` ÔåÆ 0, `v0.11.4` ÔåÆ 1,
  `0.11.5` ÔåÆ 2; vor der Hebung `v0.11.3` ÔåÆ 0 und `v0.11.4` ÔåÆ 1; PyYAML
  `safe_load` und `actionlint 1.7.12` auf beiden Workflows OK; `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1505 passed /
  0 failed / 1 ignored), `git diff --check` gruen. Nachtrag zum `verify`-Job:
  Ubuntu-Runner haben `pwsh`, aber `default = ["webview"]` braucht WebKitGTK;
  die gebundene apt-Installation aus `ci.yml` wurde erg├ñnzt und erneut gepr├╝ft.
- **Board:** T-967 ist nach PR #102 auf `master` `done` mit `done_at` und
  echtem Pruefprotokoll; T-961 bleibt `free` (Userentscheidung ÔÇ×Keine
  Aenderung"). Der Spiegelblock in `docs/TASKBOARD.md` ist synchron.
- **Grenzen:** PR #102 wurde nach ausdruecklicher Freigabe per Squash nach
  `master` gemergt (`c0890d5`); **kein** Tag, kein GitHub-Release, kein
  Deployment. Der historische Tag `v0.11.4` ist unangetastet; ein Workflow-Lauf
  dagegen scheitert absichtlich. Die GitHub-Action wurde nicht live auf einem
  Tag ausgefuehrt. Keine Live-Provider-Laeufe, keine Zugangsdaten, keine
  Profile/Cookies/Tokens, keine Kontoeinstellung angefasst.
- **Dokumentenabgleich (25.11.2026):** `GOALS.md` und `docs/OVERVIEW.md` zeigten
  noch veraltete Zaehlungen (91 Tasks / 14 frei) und einen angeblichen
  `pending`-PR #69. Nach PR #102 stehen auf `master` 92/91/1; PR #69 ist
  `CLOSED` ohne Merge; die rohen T-961-Transkripte sind vorhanden, aber
  negativ (0/5 Tool-Turns, Anmeldestatus unbestaetigt).
- **PR #102:** https://github.com/st0rax/webagent-rs/pull/102 ist `MERGED`
  (`chore/T-967-release-gate-opencode` ÔåÆ `master`, Squash `c0890d5`). Alle vier
  CI-Checks des PRs waren gr├╝n (Linux-WebView 1m42s, aarch64 2m37s,
  build & test 2m16s, Windows 3m38s). Kein Tag, kein Release.
- **Offen:** die parallele DeepSeek-Umsetzung auf `chore/T-967-release-gate`
  ist nicht bewertet. Weitere Merge-, Tag- oder Release-Schritte brauchen
  erneut eine ausdrueckliche Freigabe.

## 2026-11-24 ÔÇö PR-Kette nach Freigabe vollstaendig nach `master` gemergt

- **Freigabe:** Der Eigentuemer hat auf die Frage ÔÇ×Soll ich die PR-Kette jetzt in
  der Reihenfolge 97 bis 101 nach master mergen?" ausdruecklich ÔÇ×Ja, alle fuenf"
  geantwortet. Damit war der Merge freigegeben; **kein Tag, kein
  GitHub-Release**, keine Live-Laeufe, keine Zugangsdaten/Profile/Cookies/
  Tokens, kein Eingriff in FreeRouter (8787) oder Supervisor (4096).
- **Reihenfolge und Squash-Merges:** #97 T-926 `a3f27ec`, #98 T-940 `df9e4b5`,
  #99 T-942 `ae595a5`, #100 T-944 `4d206cc`, #101 T-950 `e0b1ea8`. Alle fuenf
  PRs `MERGED`; `master` lokal und remote `e0b1ea8`.
- **Retarget und Sync:** Weil die PRs auf ihre Vorgaenger-Branches basierten,
  wurden #98ÔÇô#101 vor dem Merge auf `master` retargetet. Je Branch ein Sync-Merge
  von `origin/master` (neue Commits, kein Rebase, kein Force-Push); Konflikte in
  `docs/CURRENT_WORK.md`, `src/cli.rs`, `src/main.rs`, `selectors/deepseek.json`
  und `src/browser/mod.rs` zugunsten des weiter fortgeschrittenen Chain-HEADs
  geloest ÔÇö die T-926/T-940/T-942-Abschnitte und -Code waren dort bereits
  enthalten. Sync-Commits: T-940 `438e8bb`, T-942 `2780c6d`, T-944 `34efaec`,
  T-950 `e62acc5`.
- **CI und Gates vor jedem Merge:** alle Jobs des jeweiligen Head gruen; lokal
  nach jedem Sync `cargo test --lib` (1498 / 1499 / 1501 / 1505 passed, 0 failed,
  1 ignored), `cargo check --all-targets` und `git diff --check` gruen.
- **Verifikation auf echtem `master`:** `WebViewSessionAdapter`
  (`src/free_cloud_chat.rs`), `answer_markdown` (`src/browser/mod.rs`,
  `selectors/{deepseek,gemini}.json`), `reasoning_exclude`
  (`selectors/deepseek.json`), `source_hints` (`src/observer.rs`,
  `src/brain_score.rs`, `src/relay.rs`) und `web_search_isolation`
  (`src/api_bridge/catalog.rs`, `docs/API_BRIDGE.md`) sind vorhanden. Board
  unveraendert 91 Tasks / 90 `done` / 1 `free` (T-961, Userentscheidung ÔÇ×Keine
  Aenderung") / 0 `claimed`; die fuenf `done`-Zellen sind jetzt durch den echten
  Merge gedeckt und nicht mehr nur eine dokumentierte Diskrepanz zu `master`.
- Der waehrend der Merge-Arbeit frisch erschienene Remote-Branch
  `chore/T-967-release-gate` wurde nicht angefasst.

## 2026-11-24 ÔÇö PR-Kette zur Abnahme vorbereitet, CI-Blocker behoben

- **Freigabeumfang:** Der Eigentuemer hat den Audit-Vorschlag bestaetigt und die
  Projektverantwortung uebertragen. Umgesetzt wurden PR-Eroeffnung und
  CI-Reparaturen. **Kein Merge, kein Tag, kein GitHub-Release**; T-961 bleibt
  `free` (ÔÇ×Keine Aenderung"), kein Live-Lauf, keine Zugangsdaten/Profile/
  Cookies/Tokens, kein Eingriff in FreeRouter (8787) oder Supervisor (4096).
- **Fuenf PRs, kaskadierend** (je ein Task, ein Branch):
  [#97](https://github.com/st0rax/webagent-rs/pull/97) T-926 ÔåÆ `master`,
  [#98](https://github.com/st0rax/webagent-rs/pull/98) T-940 ÔåÆ T-926,
  [#99](https://github.com/st0rax/webagent-rs/pull/99) T-942 ÔåÆ T-940,
  [#100](https://github.com/st0rax/webagent-rs/pull/100) T-944 ÔåÆ T-942,
  [#101](https://github.com/st0rax/webagent-rs/pull/101) T-950 ÔåÆ T-944.
  Alle f├╝nf sind `OPEN`, `MERGEABLE`, mit Scope, Livebelegen (Nonces/Hashes),
  Gates und ehrlichen Grenzen beschrieben. Merge-Reihenfolge nur 1 ÔåÆ 2 ÔåÆ 3 ÔåÆ
  4 ÔåÆ 5; die Branches bauen linear aufeinander auf.
- **Zwei CI-Blocker aus aelteren Pushes behoben** (neue Commits, kein
  Force-Push): `attach_network_observation` war nur unter `#[cfg(windows)]`
  definiert, aber unbedingt aufgerufen ÔåÆ Nicht-Windows-Stub `Ok(None)` auf
  T-940/T-942/T-944/T-950 (`dd833c3`, `223e2ca`, `1c6173d`, `4a863d9`). Im
  T-944-Livebeleg fehlte das `**Referenz**`-Banner ÔåÆ
  `betriebs_markdown_hat_eine_wahrheit` rot; mit `5b34bde` ergaenzt, auf T-950
  war es bereits vorhanden.
- **CI-Stand nach den Fixes:** alle fuenf Prueflaeufe gruen (T-926 `success`,
  T-940 `success`, T-942 `success`, T-950 `success`, T-944 nach einem
  einmaligen Flake `executor::tests::test_stderr_capture` ÔÇö leere Streams bei
  Exit 0, Push-Lauf desselben SHA gruen ÔÇö im Rerun ebenfalls `success`).
  Lokal auf T-950-Head: `cargo test --lib` 1505 passed / 0 failed / 1 ignored,
  fmt/clippy/build/`git diff --check` gruen.
- **Abnahmeuebersicht (nicht im Repo):** `%TEMP%\opencode\pr-acceptance-overview.md`
  mit PR-Tabelle, Branch-SHAs, Gates, Reparaturen und Grenzen. Der Board-Status
  `done` bleibt gegenueber `master` (`9017249`) inkonsistent, bis die PRs
  tatsaechlich gemergt sind; das war zu diesem Zeitpunkt noch nicht erledigt
  und ist jetzt im Abschnitt oben nachgeholt.

## 2026-11-24 ÔÇö Audit: Board-Status gegenueber `master`, Gates nachgemessen

- **Auftrag:** Stand/Audit pr├╝fen. Der Eigentuemer hat danach die
  Projektverantwortung an diesen Agenten delegiert. Daraus wurde **keine**
  Freigabe fuer Merge, Tag, Release oder Live-/Kontozugriff abgeleitet.
- **Gepruefter Baum:** `webagent-rs`, `feature/T-950-websearch-isolation`,
  HEAD `956425f`, identisch mit `origin/...`; Working Tree vor und nach dem
  Audit sauber. Remote-`master` per `git ls-remote`:
  `9017249157a3182de900fa927ae6f4d8ee33f906` (lokal gleich).
- **Board:** 91 Tasks, 90 `done`, 1 `free` (T-961), 0 `claimed`. Der
  maschinenlesbare Spiegelblock in `docs/TASKBOARD.md` ist synchron;
  `taskboard::tests::taskboard_invarianten_halten` gruen (1 passed, 1505
  gefiltert). Aeltere Prosotabellen im selben File sind historisch und nicht
  die Quelle.
- **Gates am 24.11. auf HEAD nachgemessen:** `cargo test --lib` 1505 passed /
  0 failed / 1 ignored; `cargo fmt --all -- --check` Exit 0;
  `cargo clippy --all-targets -- -D warnings` Exit 0; `git diff --check`
  Exit 0. Diese Zahlen gelten fuer den Feature-Branch, **nicht** fuer
  `master`.
- **Befund (Diskrepanz):** `START_HERE.md` Abschnitt 4 und `AGENTS.md`
  Regel 5 definieren `done` als ÔÇ×Branch in `master` gemergt". Fuer `T-926`,
  `T-940`, `T-942`, `T-944`, `T-950` steht das Board auf `done`, der Code
  liegt aber nur auf den gleichnamigen, gepushten Feature-Branches.
  `git branch --merged master` fuehrt keinen davon; `git grep` in `master`
  findet `WebViewSessionAdapter`, `answer_markdown`, `reasoning_exclude`,
  `source_hints`, `web_search_isolation` nicht, auf HEAD dagegen schon.
  `docs/CURRENT_WORK.md` verbietet Merge ohne neue ausdrueckliche Freigabe ÔÇö
  der Board-Status `done` behauptet hier mehr, als `master` ausliefert.
- **Entscheidung dieses Agenten:** Befund dokumentieren, **keinen** Merge,
  keinen Tag, kein Release, keine Statuskorrektur im Board ohne Freigabe.
  T-961 bleibt `free` (ÔÇ×Keine Aenderung" des Eigentuemers), kein `PASS`, kein
  `done`. Empfehlung: fuer die fuenf Branches zuerst PRs oeffnen und pruefen
  und `done`/`done_at` erst nach tatsaechlichem Merge bestaetigen; alternativ
  den Board-Status bewusst korrigieren ÔÇö beides braucht eine ausdrueckliche
  Entscheidung des Eigentuemers.
- **Keine Live-Laeufe**, keine Zugangsdaten, keine Profil-/Cookie-/Token-
  Dateien, kein Eingriff in FreeRouter (8787) oder Opencode-Supervisor (4096).

## 2026-11-21 ÔÇö Entscheidung zu T-961: keine ├änderung

- Der Eigent├╝mer hat den Konto-Check mit **ÔÇ×Angemeldet / OK"** beantwortet.
- Ein konkreter neuer Strategievorschlag (ÔÇ×Auftrag statt Werkzeug-Meta":
  Tool-Schema als Preamble nach vorn, Choice-/Verf├╝gbarkeitss├ñtze entfernt,
  auftragsf├Ârmiger User-Turn ohne Werkzeug-Meta, ÔëÑ5 aufeinanderfolgende saubere
  Raw-L├ñufe) wurde mit Ziel, Umfang, erwarteter Wirkung und Messplan vorgelegt.
- Der Eigent├╝mer hat ausdr├╝cklich **ÔÇ×Keine ├änderung"** gew├ñhlt. Damit gilt:
  T-961 bleibt bewusst `free`, **kein Code, kein Live-Lauf, kein Tag, kein
  GitHub-Release, kein Merge**. Die bereits widerlegten Richtungen (0/7 und
  0/6) bleiben die letzte Aussage; es gibt kein `PASS` und kein `done`.
- Endstand des Boards: **90 `done` / 1 `free` (T-961) / 0 `claimed` / 91**.
  Damit ist ÔÇ×100 %" ohne neue, ausdr├╝cklich freigegebene Strategie nicht
  erreichbar ÔÇö ehrliche Grenze statt decayed Status.

## 2026-11-21 ÔÇö G-010: T-950 Websuche-Isolation und Quellenhinweise live belegt

- **Branch:** `feature/T-950-websearch-isolation`, ausgehend von
  `feature/T-944-reasoning-exclude` (`584f071`). Kein Tag, kein Release, kein
  Merge ohne neue ausdr├╝ckliche Freigabe.
- **Surveys (Binary `64ADA47AÔÇªC90`):** je Brain DOM-Survey mit frischer
  WebView-OAuth-Session. Nur DeepSeek hat einen sichtbaren Websuche-Schalter
  (`Search`, `ds-toggle-button--selected`); chatgpt/gemini zeigen nur die
  Chatverlaufssuche, kimi/mistral/qwen keinen Schalter, zai trifft einen
  Sidebar-Men├╝eintrag ohne Zustandswechsel, claude ist nicht angemeldet,
  perplexity unbekannt.
- **Toggle-Beleg:** deepseek `aria-pressed=true|ÔÇª--selected ÔåÆ
  aria-pressed=false|ÔÇª` (Hash `FC4869A0ÔÇª4DB8AF3F`). Folge-Survey wieder
  `selected` ÔÇö nur Konversationszustand, keine dauerhafte Konto├ñnderung.
- **Umsetzung:** `WebSearchObservation` + Puffer und Event-Felder `web_search` /
  `source_hints` in `src/brain_score.rs`; `source_hints(text)` in
  `src/observer.rs` (nur explizit markierte Hinweise, kein Flie├ƒtext-Raten,
  Text unver├ñndert); `ensure_web_search_off()` mit bounded Wait nach
  `new_chat` in `src/browser/ui.rs`; Turn-Verdrahtung in `src/relay.rs`;
  `web_search_isolation(brain)` + `/v1/models`-Feld in
  `src/api_bridge/catalog.rs` und Test in `src/api_bridge/tests.rs`.
- **Bridge-Livebeleg (Port 8788, frischer prozess-only Token, Final-Binary
  `20DC7552ÔÇª82FB8`):** `/v1/models` markiert nur deepseek als
  `switchable_disabled_for_api`, alle ├╝brigen als `not_isolatable`. Turn
  `T950F-CDD54DDB` (chatgpt, URL auf ├Âffentliche Rust-README): Antwort endet
  `Quelle: Offizielle Rust-README.md auf GitHub.` ÔåÆ Event mit
  `source_hints=["Offizielles Rust-README.md auf GitHub"]` und
  `web_search=not_isolatable`. Turn `T950G-C9645851` (deepseek): Event mit
  `web_search=off_before_send` und belegtem Zustandswechsel. Beide
  `success=true`, `sends=1`. W├ñhrend der L├ñufe keine Repo-├änderung, Rohdaten nur
  unter `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T950-websearch-20261121\\`.
- **Ehrlicher Zwischenbefund:** ein erster Lauf mit Zwischen-Binary
  `2D67A22ÔÇª29078` meldete deepseek `not_isolatable` (ÔÇ×nicht im DOM"), weil die
  SPA den Schalter direkt nach `new_chat` noch nicht gerendert hatte; bounded
  Wait (20 ├ù 150 ms) behebt das, Final-Lauf ist `off_before_send`. Grenze
  bleiben stille Provider-Abrufe ohne Quellenhinweis: sie sind f├╝r die Bridge
  nicht erkennbar und werden nicht behauptet.
- **Gates:** `cargo test --lib` 1505 passed / 0 failed / 1 ignored; fmt,
  clippy (`-D warnings`), build, `git diff --check`, Board-Invariante gr├╝n.
- **Beleg:** `docs/proofs/T-950/websearch-isolation-live-result.md` und
  `docs/API_BRIDGE.md` (neuer Abschnitt ÔÇ×Anbieterseitige Websuche"). Board:
  T-950 `done`, 90 `done` / 1 `free` / 91.
- **Offen:** nur noch T-961 `free` (beide Richtungen live widerlegt: 0/7 und
  0/6; kein `PASS`, neue Strategie nur nach neuer ausdr├╝cklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigent├╝mers weiter offen). Kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-944 Reasoning-Ausschluss live belegt

- **Branch:** `feature/T-944-reasoning-exclude`, ausgehend von
  `feature/T-942-markdown-structure` (`19e8c6f`). Die abgelehnte T-961-Scheibe
  wurde **nicht** mitgezogen.
- **DOM-Beleg:** DeepSeek rendert den Denkbereich als Geschwister
  `.ds-think-content` (Textbeginn `We need answer...`) neben
  `div.ds-markdown.ds-assistant-message-main-content`; Dump
  `A62C877FÔÇª830C2A`. Gemini zeigte im gemessenen Turn **keinen** separaten
  Denkbereich (`3DD89CC0ÔÇª353D`), deshalb dort auch kein Ausschluss erfunden.
  ZAI-Toggle nicht anklickbar, ChatGPT ohne Reasoning-Key, Claude nicht
  angemeldet ÔÇö f├╝r diese wird keine Ausschluss-Behauptung aufgestellt.
- **Fix:** deklarierter Ausschluss `reasoning_exclude` nur in
  `selectors/deepseek.json`; beide Auslesepfade (`answer_read_body` und
  `probe_generation_js` in `src/browser/mod.rs`) entfernen die Knoten aus einer
  DOM-Kopie vor `TX`/`TXMD`, mit R├╝ckfall auf den Originaltext. Ohne
  Deklaration bytegenau unver├ñndert, kein Text-/Formulierungsfilter
  (non_goals eingehalten). `src/browser/verify.rs`: Signatur mit `"[]"`.
  Tempor├ñrer DOM-Dump (`WEBAGENT_T944_DOMDUMP`) vor dem Final-Build entfernt.
- **Zustand:** DeepThink war eingeschaltet; erster Toggle `true ÔåÆ false`,
  sofortiger Restore `false ÔåÆ true`, Abschluss-Survey weiterhin
  `ds-toggle-button--selected`. Kein dauerhafter Kontoeingriff. (DeepSeek
  `Search` war ebenfalls selected ÔÇö T-950, nicht ver├ñndert.)
- **Finaler Livebeleg (Binary `64ADA47AÔÇªC90`, 1501 Tests gr├╝n):** deepseek
  `T944Z-9FDB9E20`, gemini `T944Z-B2AC9CFF`, qwen `T944Z-A651188A`, zai
  `T944Z-FD92CE43`, chatgpt `T944Z-E7002AD0`; alle Exit 0, `webview_oauth`,
  `api_key: false`, `oauth_session: true`, keine Reasoning-Marker in den
  Antworten. Ehrliche Grenze: chatgpt lieferte die Nonce nicht mit (Antwort
  nur `391`) ÔÇö dieser Lauf belegt nur ÔÇ×kein Reasoning im Text". kimi/mistral
  Exit 2 mit leerer stdout, claude ohne Login ÔåÆ keine Aussage.
- **Gates:** `cargo test --lib` 1501 passed / 0 failed / 1 ignored (zwei neue
  T-944-Tests); `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`,
  `cargo build`, `git diff --check` gr├╝n.
- **Beleg:** `docs/proofs/T-944/reasoning-exclusion-live-result.md` mit
  DOM-Beleg, Rohhashes und Einschr├ñnkungen. Rohdaten unter
  `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T944-reasoning-20261121\\`
  (nicht ins Repo kopiert). Board: T-944 `done`, 89 `done` / 2 `free` / 91.
- **Offen (Stand der Uebergabe):** T-950 brauchte damals noch echte
  DOM-/Antwortmessungen; T-950 ist inzwischen oben dokumentiert `done`.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdr├╝cklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigent├╝mers weiter offen). Kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-942 Markdown-Struktur live zurueckgewonnen

- **Branch:** `feature/T-942-markdown-structure`, ausgehend von
  `feature/T-940-network-observation` (`2241eeb`). Die abgelehnte T-961-Scheibe
  wurde **nicht** mitgezogen.
- **Vorher-Negativbeleg:** deepseek `T942-05223CD7`, gemini `T942-F228D3C9` ÔÇö
  Tabellen als Tab-Text, Fences weg, UI-Labels `Copy`/`Download` im Antworttext,
  Listenmarker weg. Binary `A0B5D0D1ÔÇª3306CD`, Exit 0.
- **Fix:** neues `TXMD` im gemeinsamen JS-Prelude (`src/browser/js.rs`)
  rekonstruiert Markdown aus echten DOM-Elementen (`table`/`pre`/`ul`/`ol`/`h*`)
  und verwirft UI-Chrome; Opt-in `answer_markdown` in `src/browser/mod.rs`,
  gesetzt in `selectors/deepseek.json` und `selectors/gemini.json`. Ohne Flag
  bleibt `TX` unver├ñndert. Kein Raten aus Tab-Text, keine Zwischenablage.
- **Nachher-Livebeleg (finales Binary `9EE80CE4ÔÇªB760`):** deepseek
  `T942H-23A8FEF4` (11,3 s), gemini `T942H-C9CE5F14` (9,0 s), beide Exit 0,
  leeres stderr. Ausgelieferter Text ist **STRUKTURGLEICH** mit dem bekannten
  Quell-Markdown (115/115 Zeichen nach CRLFÔåÆLF): Tabelle mit Pipes und
  Separatorzeile, rust-Code-Fence, Listen mit `-`. Kein Copy/Download/`?`-Icon.
- **Gates:** `cargo test --lib` 1499 passed / 0 failed / 1 ignored (neuer Test
  `t942_prelude_fuehrt_markdown_strukturell_zurueck`); `cargo fmt --all`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build`, `git diff --check`
  gr├╝n.
- **Beleg:** `docs/proofs/T-942/markdown-structure-live-result.md` mit
  Vorher/Nachher, Rohhashes und Einschr├ñnkungen (nur deepseek/gemini gemessen,
  andere Brains unver├ñndert). Rohdaten unter
  `C:\\Users\\storax\\AppData\\Local\\Temp\\opencode\\T942-markdown-structure-20261121\\`
  und `ÔÇª\\T942-markdown-fix-20261121\\` (nicht ins Repo kopieren). Board: T-942
  `done`, 88 `done` / 3 `free` / 91.
- **Offen (Stand der Uebergabe):** T-944 und T-950 brauchten damals noch echte
  DOM-/Antwortmessungen; T-944 ist inzwischen oben dokumentiert `done`.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdr├╝cklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigent├╝mers weiter offen). Kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-940 lesende Netzwerkbeobachtung live belegt

- **Branch:** `feature/T-940-network-observation`, ausgehend von
  `feature/T-926-webview-oauth-adapter` (`9af4033`, nur
  Branch-Pr├ñfixkorrektur). Die abgelehnte T-961-Scheibe wurde **nicht**
  mitgezogen.
- **Implementierung:** neues reines Modul `src/network_observation.rs`
  (Redaktion auf `scheme://host`, 512-Eintrags-Grenze mit `dropped_events`,
  gewichtete Auswahlheuristik mit `selection_reason`); drei **lesende**
  CDP-Receiver in `src/webview_runtime.rs` (`Network.enable` sowie
  `requestWillBeSent`/`responseReceived`/`loadingFinished`, kein Intercept,
  kein Port/WebSocket); opt-in `--net-trace` in `src/cli.rs`/`src/main.rs`.
- **Negativbefund zuerst:** Lauf 1 (`T940-EB0F8B8395`, Exit 0) lieferte die
  richtige Antwort, aber die **falsche Klassifikation** ÔÇö die Heuristik ÔÇ×j├╝ngster
  Request gewinnt" w├ñhlte einen 4-ms-Static-Fetch von
  `fe-static.deepseek.com` statt des SSE-Provider-Calls. Daraufhin wurde die
  Auswahl gewichtet (SSE +8, POST +3, Site-Origin +2, 2xx +1) und das Urteil
  um `request_id`, `mime_type` und `selection_reason` erweitert.
- **Live-Nachweis Lauf 2:** deepseek, Nonce `T940-A58B1F5DC0`, Exit 0, 14,9 s,
  Token exakt. `verdict`: `request_id 7812.198`, Origin
  `https://chat.deepseek.com`, `text/event-stream`, Status 200, 1101 ms,
  `finished: true`, `selection_reason` ÔÇ×SSE-Antwort ÔÇª POST, Origin der
  Dokumentseite, 2xx". Rohereignisse 3198/3539/4299 ms ergeben exakt 1101 ms.
  Z├ñhler 74 Requests / 54 Responses / 54 Finished / 0 verworfen. Binary
  `A0B5D0D1ÔÇª3306CD`. Keine Repo-├änderung w├ñhrend der L├ñufe, kein HTTP-Port.
- **Gates:** `cargo test --lib` 1498 passed / 0 failed / 1 ignored;
  `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo build`,
  `git diff --check` gr├╝n; sechs neue `t940_`-Tests gr├╝n.
- **Beleg:** `docs/proofs/T-940/network-observation-live-result.md` mit beiden
  L├ñufen, Korrektur, Rohereignissen und ehrlichen Einschr├ñnkungen (nur 2 L├ñufe,
  nur deepseek, Heuristik statt Beweis). Rohdaten unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T940-network-observation-20261121\`
  (nicht ins Repo kopieren). Board: T-940 `done`, 87 `done` / 4 `free` / 91.
- **Offen:** T-942, T-944, T-950 brauchen weiterhin echte DOM-/Antwortmessungen.
  T-961 bleibt `free` (beide Richtungen live widerlegt: 0/7 und 0/6; kein
  `PASS`, neue Strategie nur nach neuer ausdr├╝cklicher Freigabe;
  ChatGPT-Konto-Check-Ergebnis des Eigent├╝mers weiter offen). Kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-926 VerifiedFree-Adapter live belegt (WebView-OAuth, kein API-Key)

- **Branch:** `feature/T-926-webview-oauth-adapter`, ausgehend von `master`
  (`9017249`) ÔÇö die abgelehnte T-961-Scheibe wurde **nicht** mitgezogen.
- **Freigabe:** Der Eigent├╝mer hat ÔÇ×keine api keys. nur oauth." vorgegeben und
  die WebView-Session als Basis best├ñtigt. Es gibt bewusst **kein** Key-Argument
  in Code oder CLI; Groq/Gemini-Key/Cloudflare/NVIDIA/OpenRouter wurden nicht
  angebunden.
- **Implementierung:** `WebViewSessionTransport` (ohne Key/Token-Parameter),
  `BrowserInferenceWebViewTransport` ├╝ber `browser_inference::complete()`,
  `WebViewSessionAdapter<T>` als `TextStreamAdapter`, und die einzige
  `VerifiedFree`-Registry-Kennung `webagent/webview-oauth-session`. Der Adapter
  pr├╝ft Free-only-Route ÔåÆ Modellbindung ÔåÆ Sitzung; verweigerte Routen r├╝hren die
  Sitzung nicht an.
- **Red/Green:** sechs Tests rot gegen den leeren Rumpf (5 `FAILED`), danach
  sieben gr├╝n; `cargo test --lib` 1491 passed / 0 failed / 1 ignored; `cargo
  fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build`,
  `git diff --check` alle gr├╝n.
- **Live-Nachweis:** drei echte Turne auf frisch angemeldeten WebViews ÔÇö 2├ù
  gemini (Nonces `T926-0A8327B448`, `T926-38A1C3E14F`) und 1├ù deepseek
  (`T926-AD4B110890`), alle Exit 0, `oauth_session: true`, `api_key: false`,
  `started ÔåÆ token ÔåÆ completed`. Binary `97E4B4DBÔÇªB611FB`. Lauf 1 enthielt
  zus├ñtzlich UI-Text, Lauf 3 einen harmlosen WebView2-Teardown-Hinweis; das ist
  im Beleg dokumentiert. Kein HTTP-Port, keine Repo-├änderung w├ñhrend der L├ñufe.
- **Belege:** `docs/proofs/T-926/webview-oauth-live-result.md` und
  `docs/FREE_CLOUD_IMPLEMENTATION_STATUS.md` aktualisiert. Rohdaten unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T926-webview-oauth-20261121\`
  (nicht ins Repo kopieren).
- **Offen (Stand dieses Abschnitts):** T-961 bleibt `free` (beide Richtungen
  live widerlegt: 0/7 und 0/6; die zugeh├Ârigen Belege liegen auf
  `fix/T-961-self-mirror`). T-940, T-942, T-944, T-950 brauchten zu diesem
  Zeitpunkt echte Messungen; **T-940 ist inzwischen `done`** (siehe Abschnitt
  oben). Kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-927/T-928 ÔÇö Matrix ehrlich bereinigt, beide `done`

- **Methode:** read-only DOM-Surveys (`webagent survey --brain <brain> --dump
  --headless`) mit dem Binary aus `cd76926` (SHA256 `61DC1281ÔÇª`). Kein `--write`,
  keine Zugangsdaten eingegeben, keine Kontoeinstellung ge├ñndert, keine
  Profil-/Cookie-/Token-Datei gelesen.
- **T-927 (`done`):** `model/auto` war `not_run`, obwohl T-902 den Routing-Pfad
  schon live belegt hatte (HTTP 200, `AUTO-ROUTER-OK`, `selected=chatgpt`).
  Frischer Survey: `Unbekanntes Brain: auto` ÔåÆ auto ist ein virtueller Router
  ohne eigene Oberfl├ñche. Zelle jetzt `passed` **nur f├╝r den Routing-Pfad**, mit
  explizitem Hinweis, dass keine Browser-Modellwahl behauptet wird. Beleg:
  `docs/proofs/T-927/model-auto-evidence.json` + roher Survey-Dump.
- **T-928 (`done`):** alle offenen `effort`-Zellen haben jetzt Datum, Status und
  Belegpfad: chatgpt/deepseek/gemini/mistral/perplexity/zai/auto ÔåÆ `unreachable`
  (fehlende Oberfl├ñche, abgelaufene Sitzung oder Brain nicht registriert), kimi
  bleibt `failed` mit dokumentiertem Strukturbefund. claude/qwen (`passed`)
  unver├ñndert. **Kein `passed` erfunden.** Beleg:
  `docs/proofs/T-928/effort-survey-evidence.json` + 7 rohe Survey-Dumps.
- **Matrix:** `as_of` 2026-09-12 ÔåÆ 2026-11-21; `live_verify` (alter
  Gesamt-Live-Lauf vom 12.09.) bewusst unangetastet. `.gitattributes` sichert die
  Rohdumps bytegenau. Board-Spiegel: 85 `done`, 6 `free`, 91 gesamt. T-961 bleibt
  `free` (0/7 sauber), kein Tag/Release.

## 2026-11-21 ÔÇö G-010: T-961 frische Live-Matrix ÔÇö 0/7 sauber, weiter `free`

- **Binary:** `target/debug/webagent.exe` aus `0d7d802` (Code identisch zum
  gemergten `f85010a`), SHA256 `61DC1281EA96EAC5B8B788C3D3CBE1418F2F4BB1E83FFAE1456949ABCDFDB69E`.
- **Harness:** ausschlie├ƒlich Port 8788, pro Lauf frischer GUID-Token nur in der
  Prozessumgebung, keine Repo-Datei w├ñhrend eines Laufs ge├ñndert, keine
  Zugangsdaten/Profile/Cookies gelesen, keine Kontoeinstellung ge├ñndert. Der
  Textturn best├ñtigte die bestehende Anmeldung (`PI_BRIDGE_OK`). Tempor├ñre
  Harness-Kopie unter `%TEMP%\opencode\T961-live-20261121\`; dortige
  `models.json` nur f├╝r Pi 1.5.2 auf `input: [text, image]` korrigiert.
- **Ergebnis:** sieben L├ñufe (2 Pi, 5 direkte OpenAI-Client-L├ñufe), **0 sauber**.
  Direct 3/4/5: HTTP 502 `model_output_envelope_invalid` in Turn 3/4/1. Pi 2:
  0/5 Tools, Navigationstimeout. Pi 3: 18 Tool-Starts, aber 15
  Relay-Korrekturen, 20 `stopReason=error`, 6 Auto-Retries, kein `stopReason=stop`
  ÔÇö der Pi-Harness meldete hier f├ñlschlich ÔÇ×PASS"; der neue strikte Verifier
  weist ihn zur├╝ck. Direct 6: f├╝nf Tool-Turns clientseitig sauber, aber 3
  Relay-Korrekturen und 180-s-Timeout im `none`-Schlussturn.
- **Beleg:** `docs/proofs/T-961/auto-required-live-result.md` mit Methodik,
  Ergebnistabelle, Hash-Tabelle und Rohdateien (`auto-required-*.jsonl` /
  `auto-required-*-bridge-*.log`). `README.md`, `analysis.md` und
  `API_BRIDGE.md` sind aktualisiert. Secret-Scan: 0 Treffer.
- **Entscheidung:** T-961 bleibt `free`. Kein PASS, kein `done`, kein Tag/Release.
  Die Eskalation ist funktional verdrahtet, aber nicht stabil; eine ├änderung der
  Strategie braucht eine neue ausdr├╝ckliche Freigabe. Die ├╝brigen sieben
  `free`-Tasks brauchen weiterhin echte Live-Belege.

## 2026-11-21 ÔÇö G-010: T-961 AutoÔåÆRequired-Eskalation ÔÇö Code gemergt, Live-Abnahme offen

- **PR #96 gemergt** (Squash `f85010a`, alle CI-Checks gr├╝n), Branch
  `fix/T-961-auto-required-escalation` gel├Âscht. `master` lokal und remote bei
  `f85010a`, Working Tree vor dieser Statusfortschreibung sauber.
- **Freigabe umgesetzt:** `effective_tool_choice` eskaliert `Auto` ÔåÆ `Required`,
  wenn der Betreiber `--auto-tool-choice-required` bzw.
  `WEBAGENT_AUTO_TOOL_CHOICE_REQUIRED=1` setzt. Opt-in, default aus: `required`
  verhindert eine Textabschlussantwort im Werkzeugloop. Explizites `none`,
  `required`, Function und Tool-freie Requests bleiben unver├ñndert. Alle f├╝nf
  Parser-Aufrufstellen sind verdrahtet.
- **Zusatzfix:** Der Prompt bot bei `Required`/`Function` widerspr├╝chlich eine
  Textantwort an; der Hinweis steht jetzt nur noch bei `Auto`.
- **Belege:** `docs/proofs/T-961/auto-required-escalation.md` mit echter
  Red-/Green-Demonstration (ohne Eskalation `left: Auto, right: Required`, Exit
  101; danach gr├╝n) und Prompt-Test. `docs/API_BRIDGE.md` dokumentiert den
  Schalter und die Folge f├╝r Aufrufer (Schlussturn `none` oder ohne Tools).
- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test --lib` (1485 gr├╝n, 1 ignored),
  `taskboard_invarianten_halten`, `git diff --check`.
- **T-961 bleibt `free`:** Der PR liefert nur die freigegebene Code-Semantik,
  keinen Live-Nachweis. Die frische Live-Matrix auf Port 8788 mit prozess-only
  Tokens und gesetztem Schalter steht noch aus; jeder Negativlauf blockiert die
  Abnahme. Kein Tag/Release. Die bisherige Messreihe `required-mode-*`
  (clientseitiges `required`, 1/8 gr├╝n) bleibt ein Negativbefund.

## 2026-11-21 ÔÇö Eigent├╝mer-Entscheidungen zu den Rest-Tasks

Der Nutzer hat am 2026-11-21 ausdr├╝cklich freigegeben:

- **T-914:** USER-SKIP aufgehoben ÔÇö Medienhandler-Refactor darf geclaimt und
  umgesetzt werden.
- **T-961:** ÔÇ×AutoÔåÆRequired freigeben" ÔÇö der Auto-Modus darf Tool-Calls
  erzwingen. Das ist eine bewusste Semantik├ñnderung und verlangt eine neue
  Abnahme mit frischer Live-Matrix; der bisherige Negativbefund bleibt als
  Historie unter `docs/proofs/T-961/` erhalten.
- **Live-L├ñufe:** erlaubt. Harness ausschlie├ƒlich auf Port `8788`, frische
  prozess-only Tokens, keine Repo-Datei w├ñhrend eines Laufs ├ñndern. Logins,
  CAPTCHAs und Einmalcodes bedient der Nutzer im Fenster; ich lese keine
  Profil-/Cookie-/Token-Dateien und gebe keine Zugangsdaten ein.

Abarbeitung: T-914 zuerst (lokal), dann T-961 mit Live-Matrix, danach die
├╝brigen Live-Belege. Kein Tag/Release ohne vollst├ñndige Abnahme und neue
ausdr├╝ckliche Freigabe.

## 2026-11-21 ÔÇö G-010: T-914 abgeschlossen (Medienmodul)

- **PR #95 gemergt** (Squash `a6d6909`, alle CI-Checks gr├╝n), Branch
  `refactor/T-914-media-module` gel├Âscht. `master` lokal und remote bei
  `a6d6909`, Working Tree vor dieser Statusfortschreibung sauber.
- **Refactor:** Zehn Medienbl├Âcke (Bildgenerierung, Audio-Transkription,
  -Absageerkennung, -Speech, Multipart-Parser, `audio_mime`, Request-Typen)
  unver├ñndert nach `src/api_bridge/media.rs` ├╝bernommen; nur Sichtbarkeit
  `pub(super)` f├╝r Root und Tests. Root hat keine Multipart-Schleife mehr.
- **Beleg:** `docs/proofs/T-914/measurement.md` mit Block-f├╝r-Block-Vergleich
  gegen `HEAD` (10/10 identisch nach Normalisierung von Sichtbarkeit und
  `rustfmt`-Whitespace, SHA256 `338C4B57ÔÇª`), Schleifenz├ñhler 1 ÔåÆ 0 und den
  bestehenden Audio/Bild-Tests. Kein Live-Aufruf der Endpunkte.
- **Gates:** `cargo fmt --all -- --check`,
  `cargo clippy --features webview --all-targets -- -D warnings`,
  `cargo test --features webview --lib api_bridge::tests::` (54 gr├╝n),
  `cargo test --lib` (1483 gr├╝n, 1 ignored), `git diff --check`.
- **Board:** T-914 auf `done`, Stand 83/91 `done`, 8 `free`.

## 2026-11-21 ÔÇö G-010: T-948 abgeschlossen (Preflight vor dem Senden)

- **PR #94 gemergt** (Squash `23879aa`, alle CI-Checks gr├╝n), Branch
  `fix/T-948-consent-preflight` gel├Âscht. `master` lokal und remote bei
  `23879aa`, Working Tree vor dieser Statusfortschreibung sauber.
- **Ursache (Messbefund 2026-09-14):** Sechs mistral-Anfragen endeten mit
  `timeout_no_message`, obwohl das Senden Erfolg meldete. `webagent shot`
  belegte eine abgemeldete Seite mit ÔÇ×Anmelden"/ÔÇ×Registrieren" und einem
  modalen ÔÇ×Vibe Nutzungsbedingungen"-Dialog. `login_indicator` zeigte auf den
  Composer (auch abgemeldet im DOM), `login_button` kannte ÔÇ×Anmelden" nicht.
- **Fix (rein lesend, nur f├╝r deklarierte Brains):** `send_generic` ruft
  `operations::pre_send_block_reason` vor `wake_renderer`/Fill. Sichtbarer
  Zustimmungsdialog ÔåÆ `terms_consent_required`, sichtbarer Anmeldeknopf ÔåÆ
  `login_required`; Meldung `send_phase=preflight: ÔÇª`, HTTP 502 mit Marker im
  Text. Kein Zustimmungsknopf, keine Anmeldung, kein Prompt ÔÇö beides bleibt beim
  Menschen. `selectors/mistral.json` korrigiert `login_indicator` auf
  Profil-/Konto-Kn├Âpfe, erg├ñnzt ÔÇ×Anmelden" und `send_preflight_guard`.
- **Belege:** `docs/proofs/T-948/measurement.md` mit Mock-DOM-Tests und echter
  Red-/Green-Demonstration (beide Guard-Tests ohne Guard:
  `send_phase=insert: Composer-Feld nicht gefunden (Timeout)`, Exit 101; danach
  gr├╝n). `docs/API_BRIDGE.md` nennt die Marker. **Kein Live-Anspruch:** die
  neuen Profil-/Anmelde-Selektoren sind nicht live vermessen; der `handoff`
  ÔÇ×Fehlermeldung nachher gegen den abgemeldeten Zustand" bleibt offen.
- **Gates lokal gr├╝n:** `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1483 gr├╝n,
  0 failed, 1 ignored), `git diff --check`.
- **Board:** T-948 im JSON und Spiegel auf `done`, Stand 82/91 `done`,
  9 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigent├╝mer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdr├╝ckliche Freigabe.

## 2026-11-21 ÔÇö G-010: T-946 abgeschlossen (verwaiste Anfrage / Brain-Sperre)

- **PR #93 gemergt** (Squash `ae9cb2c`, alle CI-Checks gr├╝n), Branch
  `fix/T-946-client-disconnect-lock` gel├Âscht. `master` lokal und remote bei
  `ae9cb2c`, Working Tree sauber.
- **Ursache:** Eine abgebrochene mistral-Anfrage lief am 2026-09-14 dreimal
  (~867/867/861 s) im Browser weiter, obwohl der Client l├ñngst weg war. Die
  per-Brain-Sperre hielt den verwaisten Turn; Folgeanfragen an dasselbe Brain
  warteten. Andere Brains liefen parallel ÔÇö das war korrekt, aber
  `docs/API_BRIDGE.md` behauptete f├ñlschlich keine parallelen Browserruns.
- **Fix (drei Ebenen, ohne globale Serialisierung):**
  1. `TcpClientPresence` in `api_bridge/transport.rs` beobachtet den Client ├╝ber
     einen zweiten Socket-Handle (20 ms Peek, Lese-Timeout sofort restore).
  2. `with_browser_lock` in `api_bridge/inference.rs` pr├╝ft vor und nach dem
     Sperrerwerb; verwaisste Anfragen starten keinen Turn.
  3. `run_turn_loop` in `relay.rs` pr├╝ft zu Beginn jedes Turns und ├╝bergibt
     einen `cancelled`-Closure an `wait_response_streaming_with_cancel`
     (`browser/backend.rs`, Checks in beiden Poll-Phasen). Abbruch ÔåÆ
     `backend_status=client_disconnected`, `relay_stop`, brain_score-Grund
     `client_disconnected`, Sperre frei. Kein erneutes Senden eines bereits
     gesendeten Prompts.
- **Belege:** `docs/proofs/T-946/measurement.md` mit echtem TCP-Loopback-Test,
  vier Red-/Green-Tests, echten brain_score-Zeilen und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1479 gr├╝n, 1 ignored), `git diff --check`. Ein einmaliger
  `ConnectionReset` im unabh├ñngigen Web-UI-One-Shot-Test war nicht
  reproduzierbar (5 Einzel- + 2 Gesamtwiederholungen gr├╝n). `docs/API_BRIDGE.md`
  beschreibt jetzt Serialisierung je Brain und Abbruchverhalten korrekt.
- **Board:** T-946 im JSON und Spiegel auf `done`, Stand 81/91 `done`,
  10 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigent├╝mer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdr├╝ckliche Freigabe. G-010 bleibt in_progress.

## 2026-11-21 ÔÇö G-010: T-953 abgeschlossen (Modellausgabe-Fehler)

- **PR #92 gemergt** (Squash `02b6987`, alle CI-Checks gr├╝n), Branch
  `fix/T-953-model-output-error` gel├Âscht. `master` lokal und remote bei
  `02b6987`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **DoD 1 ÔÇö Fehlertyp:** Jeder `parse_response`-Fehler tr├ñgt jetzt den stabilen
  Marker `MODEL_OUTPUT_ERROR:`. `inference_failure_response()` in
  `provider_handlers.rs` meldet 502 mit OpenAI-`type: server_error` und
  `code: model_output_envelope_invalid` (Anthropic entsprechend) statt
  `invalid_request_error`. Andere Fehler behalten ihre alte ├£bersetzung; der
  DSML-Fehler aus T-957 ist nun ebenfalls als Modellausgabe gekennzeichnet.
- **DoD 2 ÔÇö brain_score:** `relay::run_turn_loop` pr├╝ft die Ausgabe ├╝ber
  `OutputCheck::validate`, bevor `answer` gesetzt wird. Ein ungueltiger
  Umschlag ist ein Fehlschlag mit dem eigenen Grund
  `model_output_envelope_invalid`; der Circuit-Breaker bleibt bewusst unber├╝hrt
  (Brain war erreichbar). Vorher wurde der Turn als Erfolg verbucht.
- **DoD 3 ÔÇö Korrekturgrenze:** H├Âchstens eine Korrektur im selben Chat (kein
  `new_chat`) mit der Parserfehlermeldung; danach fail-closed. Das Zeitbudget
  gilt f├╝r die gesamte Anfrage inklusive Korrektur. Mock-Backend-Tests pr├╝fen
  `new_chats=1`, `sends=2` und den brain_score-Eintrag.
- **Beleg:** `docs/proofs/T-953/measurement.md` mit Rot-/Green-Tests, echten
  Event-Zeilen aus dem Test-Data-Verzeichnis und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1474 gr├╝n), `git diff --check`. `docs/API_BRIDGE.md`
  beschreibt Fehlerklasse, Grund und Korrekturgrenze.
- **Board:** T-953 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  80/91 `done`, 11 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigent├╝mer-Entscheidung nicht abschlie├ƒbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 ÔÇö G-010: T-957 abgeschlossen (DSML fail-closed)

- **PR #91 gemergt** (Squash `f2a5aac`, alle CI-Checks gr├╝n), Branch
  `fix/T-957-dsml-fail-closed` gel├Âscht. `master` lokal und remote bei
  `f2a5aac`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **Entscheidung fail-closed statt ├£bersetzung:** `parse_response` erkennt den
  DSML-Marker aus dem realen deepseek-Beispiel (Vollbreiten-Form und die im
  Taskboard gespeicherte verformte Form) und meldet
  `DSML_TOOL_FORMAT_UNSUPPORTED`. Keine ├£bersetzung in `tool_calls`, weil die
  Argumente aus derselben verformenden Lesestrecke stammen, die T-956 nur f├╝r
  den JSON-Umschlag abgesichert hat. G├╝ltige `WEBAGENT_INFERENCE/1`-Umschl├ñge
  bleiben unber├╝hrt. `docs/API_BRIDGE.md` beschreibt die Grenze.
- **Beleg:** `docs/proofs/T-957/measurement.md` mit Red-Test (vorher
  `Ok(Textantwort)`, `tool_calls: []`), Green-Tests und Gates. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1468 gr├╝n), `git diff --check`. Der erste CI-Lauf war
  rot, weil ein neues Beleg-Markdown das Referenz-Banner brauchte; mit Banner
  sind alle Checks gr├╝n. MinGW liegt lokal in einem WinGet-Paket und wurde nur
  f├╝r die Cargo-L├ñufe an `PATH` geh├ñngt (keine Repo-├änderung).
- **Board:** T-957 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  79/91 `done`, 12 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigent├╝mer-Entscheidung nicht abschlie├ƒbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 ÔÇö T-961: auch erzwungener Tool-Call widerlegt ÔÇö weiter NICHT abgenommen

- **Dritte Richtung gepr├╝ft (strukturell, ohne Code-├änderung):** ein direkter
  OpenAI-Client auf dem unver├ñnderten `master` `bf0a65b` (frisch gebaut,
  SHA256 `3532478D...F710`) sendete auf f├╝nf aufeinanderfolgenden Turns
  `tool_choice: "required"` ÔÇö bereits vom Adapter unterst├╝tzt, also keine
  API-Semantik-├änderung und keine Freigabe n├Âtig. Ergebnis aus 8 L├ñufen:
  **1/8 streng gr├╝n**. Sechs L├ñufe lieferten trotz Verpflichtung nur Text
  (fail-closed 502), zwei scheiterten an `ABSENDEKNOPF_DEAKTIVIERT`. Ein
  zus├ñtzlicher Validierungslauf mit f├╝nf sauberen Tool-Turns hatte einen
  abschliessenden Renderer-Timeout und ist nicht Teil der Vergleichsreihe.
  Rohbelege: `docs/proofs/T-961/required-mode-*`, Auswertung
  `required-mode-result.md`, 0 Secret-Treffer.
- **Einordnung:** `required` macht die Absage f├╝r Aufrufer sichtbar (502 statt
  Text), beseitigt aber die Fehlerklasse nicht. Damit sind drei Richtungen
  gemessen: Verf├╝gbarkeitsregel 2/8, `[FUNKTIONEN]` 6/8, `required` 1/8.
  Eine serverseitige AutoÔåÆRequired-Eskalation oder ein versteckter Retry
  w├╝rde API-Semantik ├ñndern und braucht eine ausdr├╝ckliche Eigent├╝mer-Freigabe
  samt eigener Abnahme; beides ist **nicht** implementiert.
- **Eigent├╝mer-Entscheidung vom 21.11.2026:** Negativbefund dokumentieren,
  **T-961 offen lassen**. Kein PASS, kein `done`, kein Code-Merge, kein
  Tag/Release. G-009 bleibt offen. Die bekannte Grenze steht jetzt in
  `docs/API_BRIDGE.md` inklusive des `required`-Befunds.

## 2026-11-21 ÔÇö T-961: zwei Prompt-Kandidaten widerlegt ÔÇö weiter NICHT abgenommen

- **PR #88 gemergt** (`8693b99`, Squash, alle CI-Checks gruen): Kontobefund und
  der ehrliche 2/8-Negativbeleg zum Verf├╝gbarkeits-Kandidaten liegen jetzt in
  `master`.
- **Zweiter Kandidat ebenfalls widerlegt:** `f899ed3` nannte den Schema-Block
  `[FUNKTIONEN]` statt `[Client-Werkzeuge]` und vermied jedes Meta-Wort zur
  Verf├╝gbarkeit. Alle lokalen Gates gruen (fmt, clippy, 1467 Tests), aber
  8 echte Live-Laeufe lieferten **6/8 streng gruen** und zwei Laeufe mit exakt
  der T-961-Fehlerklasse (ÔÇ×Das read-Werkzeug ist in dieser Sitzung nicht
  verf├╝gbar"). Das ist nicht besser als die 3/4-Baseline von `master`. Die
  Code-Aenderung wurde auf dem Branch mit `a7eb7ef` zurueckgenommen; Rohtrans-
  kripte und Auswertung liegen unter `docs/proofs/T-961/function-*`.
- **Ergebnis der Prompt-Versuche:** ausdr├╝ckliche Verf├╝gbarkeitsregel 2/8,
  reine Umbenennung 6/8 (Baseline 3/4). Reine Prompt-Wortwahl ist als Richtung
  widerlegt; der Kontozustand scheidet als Ursache aus. Die Ursache bleibt
  **ungekl├ñrt** (modellseitige Stochastik ist eine Vermutung, kein Beleg).
- **Entscheidung:** kein PASS, kein `done`, kein Merge eines Kandidaten,
  kein Tag/Release. **T-961 bleibt `free`**, G-009 bleibt offen. Die Bridge
  dokumentiert die bekannte Grenze jetzt in `docs/API_BRIDGE.md`, damit
  Aufrufer unvollst├ñndige Werkzeugl├ñufe anhand der Marker pruefen.

## 2026-11-21 ÔÇö T-961: Kontobefund belegt, Kandidat widerlegt ÔÇö weiter NICHT abgenommen

- **PR #87 gemergt** (`dfebc47`, Squash, CI komplett gruen), `master` lokal und remote
  bei `e4386f0`. Er dokumentiert ehrlich, dass der Rahmungsfix die Intermittenz
  nicht beseitigt hat.
- **Kontozustand autonom geprueft** (DoD-Punkt, bisher als Eigentuemer-Auftrag
  markiert): read-only DOM der angemeldeten ChatGPT-WebView auf
  `/settings/personalisation` und im geoeffneten Dialog
  ÔÇ×Erinnerungszusammenfassung". Erinnerung aktiv, Abschnitt
  ÔÇ×Benutzerdefinierte Anweisungen" vorhanden, Zusammenfassung aktuell
  (ÔÇ×Gerade aktualisiert"/ÔÇ×├£berblick"), **kein Eintrag, der Werkzeugzugriff
  verneint**. Keine Einstellung geaendert, keine Zugangsdaten/Profile/Cookies
  gelesen, temporaere Diagnose-Patches vollstaendig zurueckgerollt. Pers├Ânliche
  Inhalte bewusst nicht committet. Beleg: `docs/proofs/T-961/account-state.md`.
- **Modellwechsel nicht moeglich:** `webagent model --brain chatgpt` meldet
  `aktuell ''` und `'model_menu' nicht anklickbar`; sichtbarer Modus lief in eine
  Navigation timeout. Kein Menue- und kein Konto-Settings-Eingriff vorgenommen.
- **Kandidat widerlegt:** `0039cd4` ergaenzte den Browser-Prompt um eine neutral
  formulierte Regel (Werkzeuge seien aktiv und ausfuehrbar, ein Ergebnis
  verbrauche das Werkzeug nicht). Alle lokalen Gates waren gruen (1467 Tests),
  aber 8 echte Live-Laeufe mit diesem Binary (Port 8788, frische Tokens,
  identische Prompts) lieferten nur **2/8 streng gruen**. In fuenf Laeufen
  verneinte das Modell den Werkzeugzugriff ÔÇö teils schon vor dem ersten Aufruf
  (ÔÇ×ist in dieser Sitzung nicht ausf├╝hrbar"), teils nach drei Reads; ein Lauf
  endete in `ABSENDEKNOPF_DEAKTIVIERT` + `circuit_open`. Die Formulierung hat
  das Meta-Thema ÔÇ×Verfuegbarkeit" also eher provoziert. Die Code-Aenderung
  wurde auf dem Branch mit `63a6f7a` zurueckgenommen; der Branch enthaelt nur
  noch die Belegdokumentation (`docs/proofs/T-961/availability-*`).
- **Entscheidung jetzt:** kein PASS, kein `done`, kein Merge des Kandidaten,
  kein Tag/Release. G-009 bleibt offen. **T-961 bleibt `free`.** Die
  Intermittenz ist nicht erklaert; der Kontozustand scheidet als Ursache aus.
  Ein naechster Versuch muss ohne jedes Verf├╝gbarkeits-Meta im Prompt auskommen
  und braucht erneut eine belastbare Raw-Transkript-Reihe.

## 2026-11-20 ÔÇö T-961: Rahmungsfix gemergt, Intermittenz bleibt ÔÇö weiter NICHT abgenommen

- **PR #86 gemergt** (`eaed3d1`, Squash, CI gruen, Branch geloescht). Ursache:
  `conversation_prompt` labelte Tool-Ergebnisse im **Verlauf** mit
  `[Client-Werkzeug-Ergebnis]` + `[tool_call_id]`, rendering aber die **letzte**
  Nachricht (genau Pi's `toolResult`) als nackten Text unter ÔÇ×Aktuelle Nachricht
  an dich". Red-Test zuerst (`current_tool_result_gets_explicit_label_and_call_id`),
  danach Fix in `src/api_bridge/content.rs`; Einzelnachricht-Pfad unveraendert.
  `docs/API_BRIDGE.md` dokumentiert die Rahmenung jetzt ausdruecklich.
- **Gates gruen:** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1466 passed), davon 54 `api_bridge`-Tests. PR #85
  (Belegdoku) ist ebenfalls gemergt (`4b8a80c`). Lokal `master` = `eaed3d1`,
  Working Tree bis auf die hier dokumentierte Belegergaenzung sauber.
- **Neue echte Live-Laeufe mit finalem Binary `eaed3d1`** (SHA256
  `A2913A3D...904C7`, 4 aufeinanderfolgende Laeufe, gleiche Prompts, frische
  Tokens, Port 8788): 3 gruen (jeweils 5/5 Nonces `isError=false`, Finalanswer
  mit allen Markern, 0 Session-/Circuit-Fehler), aber **Lauf 1 negativ**:
  call_1..call_4 erfolgreich, dann *ÔÇ×Das f├╝nfte read-Werkzeug ist in dieser
  Sitzung nicht verf├╝gbar."* (4/5, fehlender Marker 5). Zwei Vorab-Laeufe mit
  dem Fix-Branch-Binary `37fe0fc` waren ebenfalls gruen.
- **Entscheidung: kein PASS, kein `done`.** T-961 bleibt `free`. Der Rahmungsfix
  ist korrekt und unit-getestet, aber die Fehlerklasse (Werkzeugzugriff nach
  eigenem Erfolg verneinen) ist mit dem finalen Binary reproduzierbar. Ein
  gruener Lauf, auch drei von vieren, beweist keine geheilte Ursache.
- **Naechster Schritt (nur Eigentuemer):** ChatGPT-Konto-Speicher /
  Personalisierung auf Erinnerungen pruefen, die Werkzeuge verneinen (DoD nennt
  das ausdruecklich). Kein Agent liest Profile, Cookies oder Zugangsdaten. Danach
  erneut mehrere stabile gruene Laeufe fordern. G-008 und G-009 bleiben offen.
- **Belege:** `docs/proofs/T-961/analysis.md`, `environment.txt`, `README.md`,
  `after-framing-negative.jsonl`, `after-framing-green-{1,2,3}.jsonl`,
  `after-premerge-green-{1,2}.jsonl` (jeweils mit `-summary.txt`). Neue
  `.gitattributes`-Regel haelt die Rohtranskripte bytegenau, damit die
  dokumentierten SHA256-Werte stimmen.

## 2026-11-20 ÔÇö T-961: drei Blocker behoben, gruener Lauf, aber intermittierend ÔÇö NICHT abgenommen

- **Branch/PRs (alle gemergt):** PR #82 `901e8e1` (Selektor-Drift `selectors/chatgpt.json`),
  PR #83 `5355e18` (Inference-Envelope toleriert Text vor `WEBAGENT_INFERENCE/1`),
  PR #84 `976abd3` (Login-Vorabpruefe weich: nur `Error`/`Cloudflare` hart,
  `Unbestimmt`/`LoginRequired` entscheidet die echte Antwort). Lokal `master`
  = `976abd3`, fast-forward mit `origin/master`. CI aller drei PRs gruen; lokal
  `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1465 passed) gruen.
- **Live-Diagnose (Port 8788, frischer Prozess-Token, keine Zugangsdaten/Profile gelesen):**
  drei deterministische Blocker gefunden und behoben:
  1. chatgpt-DOM hatte `data-message-author-role` entfernt -> Antwort im DOM
     sichtbar, aber `probe_generation` sah 0 Antworten -> `timeout_no_message`,
     Bridge 502. Neue stabile Anker (`data-dil-response`, `data-response-body`,
     `data-markdown-text-style='assistant-message'`, ...).
  2. chatgpt gab einen gueltigen Tool-Call mit `Nur Text\n` davor aus ->
     `strip_prefix` verwarf ihn, Pi sah nur Text, 0 Tool-Starts.
  3. `session_state=Unbestimmt` (False Positive) blockierte den Prompt trotz
     funktionierender Sitzung.
- **Echter gruener Lauf** (`docs/proofs/T-961/after.jsonl`, 51.529 B, SHA256
  `71FD0429...975C8977`): Textturn `PI_BRIDGE_OK` (`assistant_ok=True`), 6
  Tool-Starts, alle fuenf Nonces `isError=false`, Schlussantwort mit allen
  fuenf Markern, 0 Session-/Circuit-Fehler, 0 Suspect-Patterns. Harness-Gates
  (`toolStarts -lt 5`, fehlende Marker) waren unveraendert aktiv; Secret-Scan 0.
- **ABER nicht abgenommen:** `after-negative-intermittent.jsonl` (27.075 B),
  Minuten vorher mit **exakt demselben Binary** `976abd3`, reproduziert das
  Ursprungssymptom: call_1 erfolgreich (`isError=false`), dann *ÔÇ×Der
  R├╝ckgabewert von nonce-1.txt fehlt..."*, 1/5 Tool-Starts. Ein einzelner
  gruener Lauf beweist bei nachweislich intermittierendem Verhalten keine
  geheilte Ursache. **T-961 bleibt `free`**, kein PASS, kein `done`.
- **Offen:** Intermittenz eingrenzen (Tool-Ergebnis-Rahmung in
  `conversation_prompt` variieren; Konto-Memory nur durch Eigentuemer
  einsehbar), dann mehrere stabile gruene Laeufe fordern. G-008 bleibt offen;
  G-009 (Tag/Release) bleibt offen. Belege: `docs/proofs/T-961/analysis.md`,
  `environment.txt`, `README.md`, `after-summary.txt`,
  `after-negative-intermittent-summary.txt`.

## 2026-11-20 ÔÇö G-006 abgeschlossen: echte T-961-Belege (negativ)

- **Rohe Transkripte jetzt vorhanden:** `docs/proofs/T-961/before.jsonl` (29.021 B)
  und `after.jsonl` (29.305 B) sind echte Pi-1.5.2-JSON-Modus-Streams, secret-gescannt
  (0 Treffer). Commit `1049369` auf `master`. Vorher lag nur ein Platzhalter vor.
- **Ergebnis ehrlich negativ:** Beide Laeufe scheitern mit
  `Browser-Inference fehlgeschlagen: session_state=Unbestimmt`; `assistant_ok=False`,
  `tool_execution_start = 0/5`, Circuit Breaker oeffnet erneut (900 s). Der
  Nach-Fix-Lauf wurde nach einem Harness-StrictMode-Fehler korrigiert wiederholt;
  das ausgewertete `after.jsonl` stammt aus diesem echten zweiten Lauf.
- **Kontozustand geprueft** (nicht `not accessible`): `GET /api/health/brains` meldet
  fuer chatgpt `login_state="unknown (old)"`, `profile_exists=true`, keinen
  erfolgreichen Run ÔÇö Anmeldung **nicht bestaetigt**. Kein Agent hat Zugangsdaten
  oder Profildateien gelesen.
- **Konsequenz:** T-961 bleibt `free`. Das ist ein Negativbeleg, keine Abnahme.
  Rezertifizierung erst nach Anmeldung durch den Eigentuemer
  (`webagent login --brain chatgpt`) und erneutem Lauf. G-006 ist als
  "Belege beschafft" done; G-008/G-009 bleiben offen.
- **PR #81 (T-949) gemergt:** Squash `0f80973`, alle CI-Checks gruen; lokaler
  `master` auf `origin/master`, danach dieser Handover-Stand committed.

## 2026-11-20 ÔÇö T-949 (Branch `fix/T-949-consent-guard`)

- **G-008, lokal abgeschlossen:** `dismiss_consent` prueft vor dem Klick auf
  `dialog_dismiss_button` die Beschriftung des Treffers. Zustimmungstexte
  (`Accept and Continue`, `OK, I agree`, `Akzeptieren`, `Zustimmen`,
  `einverstanden`) werden nicht geklickt; fail-closed bei nicht lesbarer
  Beschriftung. Cookie-Ablehnung und Nur-notwendige-Cookies bleiben bewusst
  unangetastet (non-goal, ueber `NON_AGREEMENT_MARKERS` explizit erlaubt).
- **Beleg:** `docs/proofs/T-949/measurement.md` (Mock-DOM-Tests + Gate-Ergebnisse,
  kein Live-Anspruch). `cargo fmt --check`, `cargo clippy --all-targets --
  -D warnings`, `cargo test --lib` (1460 gruen) und
  `taskboard_invarianten_halten` gruen. T-949 im Board auf `done`, Spiegel
  synchron (13 freie Tasks).
- **Nicht erledigt:** kein Live-Zustimmungsdialog gefahren (gehoert zu T-948);
  Board-Status liegt auf dem Branch und wird mit dem Merge wirksam.

## 2026-11-20 ÔÇö G-008 Triage der restlichen freien Tasks

Einordnung der freien Tasks nach dem, was **ohne** Zugangsdaten/Profile und ohne
Live-Anbieter-Sitzung ehrlich abgeschlossen werden kann. Diese Triage ist keine
Abnahme und kein Defer-Beschluss; sie bereitet die Entscheidungsvorlage fuer den
Eigentuemer vor (G-008 bleibt offen, G-009 bleibt offen).

| Kategorie | Tasks | Begruendung |
|---|---|---|
| A ÔÇö lokal umgesetzt + geprueft | T-949 | Code-/Mock-DOM-Beleg, PR #81 offen; nach CI-Fix (Referenz-Banner im Proof) erneut in CI. |
| B ÔÇö lokal umsetzbar, noch offen | T-926, T-940, T-946, T-953, T-957 | DoD mit `cargo test --lib` und/oder statischem Beleg erfuellbar; keine Live-Sitzung noetig. Kandidaten fuer weitere Scheiben. |
| C ÔÇö braucht echte Live-Sitzung / Anmeldung | T-927, T-928, T-948, T-961 | DoD verlangt frische Live-Matrix-/Effort-Belege, einen echten mistral-Zustimmungsdialog bzw. rohe Pi-Transkripte vor/nach Fix. Integrator liest keine Zugangsdaten und keine Profile. T-961-Lauf siehe G-006. |
| D ÔÇö teils Live-Messung noetig | T-942, T-944, T-950 | DoD verlangt zusaetzlich eine Messung je Anbieter (Quell-Markdown gegen ausgelieferten Text, Denkmodus-Turn, Websuche-Zustand vor dem Senden); nicht rein lokal belegbar. |
| E ÔÇö explizit deferred | T-914 | Im Taskboard bereits `[DEFERRED]` markiert; kein aktiver Auftrag. |

Konsequenz: autonom weiter bearbeitbar sind Kategorie B; Kategorie C/D benoetigen
entweder eine Anmeldung durch den Eigentuemer oder einen ausdruecklichen
Defer-Beschluss. Ein solcher Beschluss wird nicht stellvertretend erfunden.

## 2026-11-20 ÔÇö Zielstack G-005..G-007 (autonomer Fortschritt)

- **G-005 done:** PR [#80](https://github.com/st0rax/webagent-rs/pull/80) wurde als Squash-Commit `493e9a613d72dcf319151b813ed7ffc84662c657` in `master` gemergt; lokaler `master` ist auf `origin/master` fast-forwarded und war bei Beginn dieser Scheibe sauber.
- **G-006 in_progress:** Echte T-961-Live-Messung wurde vorbereitet und der Vor-Fix-Lauf ausgefuehrt. Frisch gebaute Binaries: `master` (`493e9a6`, `webagent 0.11.3`) und isolierter Worktree `dd78128` (`webagent 0.11.3`). Pi ist installiert als **1.5.2** (nicht mehr die im alten Plan genannte 0.84.4). Die Bridge startet auf `127.0.0.1:8788` mit frischem, nicht gespeichertem Prozess-Token; Port 8787 bleibt bewusst unberuehrt (dort laeuft ein fremder Free-Router-Prozess).
  - Vor-Fix-Rohtranscript: `C:\Users\storax\AppData\Local\Temp\webagent-t961-harness\before.jsonl` (29.021 Bytes, frei von Token-/Cookie-/Key-Mustern). Beide Pi-Turns endeten mit `502: Browser-Inference fehlgeschlagen: session_state=Unbestimmt`; `tool_execution_start` = 0/5. Der Circuit Breaker fuer `chatgpt` waren daraufhin 900 s offen.
  - Wichtiger Befund: Das Repo-Skript `scripts/test-pi-provider.ps1` erkennt `PI_BRIDGE_OK` im echoenden User-Prompt und meldet den Textturn daher falsch als erfolgreich. Die temporaere Harness-Kopie prueft jetzt nur noch die Assistant-Nachricht. Live-Eingaben (Prompts, Modell, Port) blieben unveraendert.
  - Der Nach-Fix-Lauf wurde nach Ablauf des Breakers ausgefuehrt; echte rohe Belege und der Kontozustand sind inzwischen committed (siehe Abschnitt G-006 oben). Ergebnis negativ, T-961 bleibt `free`.
- **G-007 done:** PR [#69](https://github.com/st0rax/webagent-rs/pull/69) wurde am 20.11.2026 19:32 UTC bewusst als **ueberholt geschlossen**, nicht gemergt. Sein Code (Systemkontext, `content:null` + `tool_calls`, Tool-Fortsetzung) liegt bereits via PR #78/`864e346` und PR #80/`493e9a6` in `master`; gegenueber `master` verblieben nur T-934-Referenzbelege. Die offen gebliebenen Live-Gates auf dem veralteten Head-SHA `f361402` wurden nicht fabriziert.
- **Offen:** G-006 Nach-Fix-Ergebnis, G-008 Triagierung der restlichen 14 freien Tasks, G-009 Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-11-20 ÔÇö T-966: Acceptance integrity (Arbeitsstand)

- **Status (historisch zum Zeitpunkt der Arbeit):** Branch `fix/T-966-acceptance-integrity` (abgezweigt von `master` bei `eaac398`) wurde gepusht; PR [#80](https://github.com/st0rax/webagent-rs/pull/80) war zu diesem Zeitpunkt offen. Zwei Commits: `cac7c29` (Taskboard/Gates), `b0628a2` (Belege/Doku). **Inzwischen gemergt** als `493e9a6`, siehe Abschnitt oben.
- **Ausgangslage:** `GOALS.md` behauptete ÔÇ×Projekt abgeschlossenÔÇ£ und G-002 done, obwohl `docs/TASKBOARD.json` auf einen 1-Task-Stub reduziert war, `docs/proofs/T-961/environment.txt` `BLOCKED_BY_MISSING_EVIDENCE` meldet und PR #69 offene Live-Belege hat.
- **Korrigiert:**
  - Vollst├ñndiges Taskboard aus `dd78128` wiederhergestellt (91 Tasks, 77 done, 14 free); T-961 bleibt `free`. Kein History-Rewrite, kein Force-Push.
  - `docs/TASKBOARD.md` um einen maschinenlesbaren Spiegelblock erg├ñnzt; `src/taskboard.rs` pr├╝ft Taskzahl, IDs und Spiegelabgleich im normalen `cargo test` und damit auch in CI.
  - T-961-Belege bereinigt: keine PASS-Behauptung, keine nicht existierenden SHAs, `before.jsonl`/`after.jsonl` als ausdr├╝ckliche `not_run`-Platzhalter.
  - Statusdokumente auf eine ehrliche Version gebracht; `README.md` nennt den realen Tag `v0.11.4` mit Hinweis auf die offene Abnahme.
- **Gates (20.11.2026, Windows/GNU, Rust 1.100):**
  - `cargo fmt --all -- --check` Ô£à
  - `git diff --check` Ô£à
  - `cargo clippy --locked --no-default-features --all-targets -- -D warnings` Ô£à (nur zwei Cargo-Manifest-Hinweise zu ungenutzten optionalen Deps ohne `webview`; keine Rust-Warnungen)
  - `cargo test --locked --no-default-features` Ô£à ÔÇö Lib 1448 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
  - Negativprobe: `docs/TASKBOARD.md` wurde absichtlich auf `| T-001 | free |` ge├ñndert; `taskboard_invarianten_halten` schlug mit ÔÇ×nicht mit docs/TASKBOARD.json synchronÔÇ£ fehl und die Datei wurde sofort wiederhergestellt.
  - Ein erster Testlauf scheiterte an einer veralteten lokalen Linker-Umgebungsvariable sowie einem fehlenden C-Compiler; beides war Werkbank-Setup, kein Repo-Code. Die Defensive gegen Ersteres (`is_test_run`) ist jetzt Teil des Fixes.
  - `cargo clippy --locked --all-targets -- -D warnings` (Default-Features/WebView) Ô£à
  - `cargo test --locked` (Default-Features/WebView) Ô£à ÔÇö Lib 1454 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
- **GitHub-CI auf PR #80:** alle Checks gr├╝n in beiden L├ñufen (Push + PR-Event): `build & test`, `build & test (Windows, volle Features)`, `build (Linux, WebView via WebKitGTK)`, `aarch64`.
- **PR #69** ([Kommentar](https://github.com/st0rax/webagent-rs/pull/69#issuecomment-7048051133)): bewusst nicht gemergt, weil der PR selbst Live-Pi-Re-run und generisches curl/SDK Chat+Tools als `pending` f├╝hrt.
- **Offen / nicht ├╝bernommen:** Echte Live-Rezertifizierung (Eigent├╝mer muss anwesend sein; ein Agent liest keine Zugangsdaten oder Profile), PR #69 mit `pending`-Live-Belegen, kein neuer Tag/Release ohne Freigabe.
- **N├ñchste Aktion (damals):** Integrationsentscheidung zu PR #80 durch den Eigent├╝mer. PR #80 ist inzwischen gemergt; T-961 bleibt `free`, bis echte rohe Pi-Transkripte vor/nach dem Fix und eine Kontozustandspr├╝fung vorliegen. Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-10-15 ÔÇö T-965: English-first maintained documentation

- **Status:** `done`; PR [#76](https://github.com/st0rax/webagent-rs/pull/76) merged.
- **Merge commit:** `3778f31a83535690e3746501cd642c46b873c70e`.
- **Branch:** `docs/T-965-english-first-docs`; task status is `done` in `docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md`.
- **Policy:** English is the default for maintained documentation, task titles, agent instructions, issues, and pull requests. Preserve dated historical reports/evidence when rewriting would damage traceability; add English summaries where useful. Product-facing output language remains a separate runtime decision.
- **Changed scope:** `README.md` and `docs/NEWCOMER_GUIDE.md` are now in English; the README retains the quick start, architecture/module map, commands, profile and environment-variable guidance, release/platform boundaries, and historical provider context. The policy and T-965 status are recorded in the taskboard and handover.
- **Non-goals:** no Rust code, runtime behavior, release, tag, login-profile, or provider configuration changes. This does not claim that every maintained or historical document in the repository has already been translated; remaining maintained docs should be migrated in follow-up tasks.
- **Verification:** PR #76 merged; `docs/TASKBOARD.json` parsed successfully; README and newcomer-guide section headings are English; links to existing Markdown files and the README quick-start anchor were checked through GitHub. Local `git diff --check`, automated link-check tooling, and Rust tests were not run in this remote editing session.
---


## 2026-10-15 ÔÇö T-964: Kontextfreier Repository-Einstieg abgeschlossen

- **Status:** `done`; PR [#73](https://github.com/st0rax/webagent-rs/pull/73) gemergt.
- **Merge-Commit:** `76ba3b303d78135e8927b2bd7f1cd382732fdc8e`.
- **Branch:** `docs/T-964-newcomer-onboarding`; Taskboard-Abschluss in [PR #74](https://github.com/st0rax/webagent-rs/pull/74) dokumentiert.
- **Einstieg:** [`docs/NEWCOMER_GUIDE.md`](NEWCOMER_GUIDE.md) ist die zentrale Landkarte f├╝r neue Menschen und Coding-Agenten; die README verlinkt sie sichtbar und trennt Benutzung von Entwicklung.
- **Quellenklarheit:** Guide erkl├ñrt Projektmodell, Dokumentenrollen, Architekturbegriffe, historische vs. aktuelle Informationen, Bazaar-Claim und Sicherheitsgrenzen.
- **Scope:** nur README, Einstiegsdokument und Taskboard/Handover; keine Rust-Code-, Laufzeit-, Login-Profil-, Tag- oder Release-├änderungen.
- **Verifikation:** Taskboard-JSON geparst; relevante Guide-Zielpfade auf GitHub gepr├╝ft; Taskboard-Spiegel enth├ñlt T-964 und den Guide-Link. `git diff --check` und Rust-Gates wurden nicht lokal ausgef├╝hrt; f├╝r reine Dokumentation wurden keine Rust-Tests gestartet.


## 2026-10-15 ÔÇö T-963: Bazaar-konforme Integration abgeschlossen

- **Status:** `done`; PR [#72](https://github.com/st0rax/webagent-rs/pull/72) gemergt.
- **Merge-Commit:** `23ccca28dfd856d2bc90bcc9744aae136d4866b9`.
- **Branch:** `docs/T-934-bazaar-agent-prompt`; Taskboard-Claim und Handover sind dokumentiert.
- **Prompt:** [`docs/PI_INSPIRED_CODING_AGENT.md`](https://github.com/st0rax/webagent-rs/blob/master/docs/PI_INSPIRED_CODING_AGENT.md).
- **Repo-Workflow:** T-963 steht in `docs/TASKBOARD.json` (Quelle der Wahrheit) und `docs/TASKBOARD.md` (Spiegel). Der vorhandene Bazaar-Arbeitsprozess bleibt erhalten.
- **Scope eingehalten:** nur Dokumentation und Taskboard; keine Rust-Code-, Laufzeit-, Browserprofil-, Login-, Tag- oder Release-├änderungen.
- **Verifikation:** Taskboard-JSON erfolgreich geparst; Taskboard-Spiegel und Handover gepr├╝ft; keine abschliessenden Leerzeichen in den vier gepr├╝ften Dateien. Lokales `git diff --check` und Rust-Tests wurden in dieser Remote-Umgebung nicht ausgef├╝hrt.

---

> **Aktualisiert 2026-09-12:** **G-001 abgeschlossen** ÔÇö alle Zeilen der
> v1.0-Definition-of-Done verifiziert (`docs/proofs/G-001/g001-abschluss-2026-09-12.json`).
> - Voll-Gates am Abschluss-Stand (`2589b0a`) erneut gr├╝n: `cargo fmt --all -- --check`,
>   Clippy `-D warnings` headless **und** full-features, `cargo test --locked`
>   **1388 passed / 0 failed / 1 ignored + 7 bin**, `cargo test --locked
>   --no-default-features` 1382 + 7 bin, `git diff --check` sauber.
> - Toolchain-Norm auf rust 1.97 (`2589b0a`): rustfmt-Drift ├╝ber 13 Dateien
>   bereinigt, sechs neue Clippy-Lints behoben (u. a. `items_after_test_module`,
>   `single_match`, tote Konstante `MEMORY_CONTEXT_LIMIT`, nie gelesenes `wiki`-Feld).
> - **Release `v0.11.3` ver├Âffentlicht** ÔÇö
> https://github.com/st0rax/webagent-rs/releases/tag/v0.11.3 (Tag `7021959`,
> CI `34696249277` success, Assets: Windows exe + WebView2Loader.dll + sha256,
> Linux, Android aarch64):
> - **Web-UI vom T-203-Fake-Prototyp auf die echte `/api/*`-Anbindung gehoben**
>   (Phase-9 T-901ÔÇôT-906 done):
> - T-905 (`664f4e8`): Brain-Status, Sessions anlegen/wechseln, Chat-Stream
>   (`events?since=`), Stop, Quellen, Gruppen, Brain-Detail, Upload, Brain-
>   Fenster show/hide; Layout im T-203-Grok-Schema, Einzeldatei < 48 KiB,
>   Tests erzwingen `fetch(`-Anbindung.
> - T-906 (`5c42656`): T-905-Restl├╝cken geschlossen ÔÇö Gruppenlauf ├Âffnet die
>   erzeugte Session (`run_id`) und rendert die Events live (Runden-Stati,
>   TextDeltas, Synthese, Done), Datei-Upload im Composer
>   (`POST /sessions/{id}/upload`, 202 `accepted`), Brain-Fenster show/hide im
>   Detail; Beweis `docs/proofs/T-906/`.
> - **Code-Review (`44f7a1e`):** reviewer-Bot REQUEST_CHANGES ÔåÆ alle 11 Befunde
>   umgesetzt: XSS-Klasse behoben (echtes `escRow`, DOM-Selects, CSP-Meta),
>   `TextComplete`-String-Handling (live belegt: Backend sendet JSON-String),
>   Poll-Race-Guard, Gap-/404-Recovery, 409-`session_done`-Text, Backoff,
>   ehrlicher Upload-Text. Beweis `docs/proofs/T-906/webui-review-2026-09-12.json`.
> - Gates f├╝r 0.11.3: **1388 passed / 0 failed / 1 ignored**; `cargo check` gr├╝n;
>   Changelog `[0.11.3]` finalisiert; Beweise T-901..T-906 in `docs/proofs/`.
> - T-805/T-806/T-807 abgenommen: 46 frische Live-Verify-Belege (alle 9 Brains)
>   in `docs/proofs/T-501/live_verify_2026-09-12.json`, Matrix `as_of 2026-09-12`.
> - T-806 Receipt-E2E live belegt (negativ fail-closed + positiv done,
>   `docs/proofs/T-806/`); T-807 Windows-Prozessprobe + Release-Binary-E2E
>   (`docs/proofs/T-807/`), alle 26 Tasks im TASKBOARD `done`.
> - Release-Fix im CI: `build.rs` linkt `resource.o` nur noch unter GNU/mingw
>   (MSVC-CI nutzt winres `resource.lib`) ÔÇö behebt `LNK1181` im Release-Workflow.
> - Gates: `cargo test --lib` 1388 passed / 0 failed / 1 ignored;
>   `--features tui` und `--no-default-features` gr├╝n.
> - perplexity als Custom-Brain registriert (`custom_brains.json`, `%LOCALAPPDATA%\\webagent\\data`) ÔÇö diagnose/verify erreichbar
> - deepseek model/mode_switch removed by-design (Modus-Bar vom Anbieter entfernt)
> - effort-Spalte komplett vermessen: claude/qwen passed, alle anderen not_run/failed
>   (kimi: Pfad `['Denkaufwand Hoch','Hoch']` klickt, verify-Failed strukturell)
> - gemini/kimi/zai model_switch Frischbelege, chatgpt model_switch failed (Sofort-Pill)
> - chatgpt.json reasoning_effort_menu entfernt (komplett deckungsgleich mit model_menu)
> - Matrix-Stand: **130 Zellen, 10 Brains ├ù 13 Areas** vollst├ñndig, `as_of` 2026-09-11
> - restliche Restposten: streaming/attachment bereits 10/10 passed (PR #44/#45/#46);
>   managed_tools 10/10 failed by-design; nur `model/auto` und zai-Webchat-Flaky offen

## Fortsetzung G-001 / T-501 am 2026-09-07

Claim: `chatgpt-codex`, Branch `fix/T-501-model-proof`, Basis `28e9f1c`.
Der Nutzer hat die Fortsetzung von G-001 beauftragt. Erste Scheibe: den
Modellwechsel-Beleg korrigieren; `verify model_switch` prueft bisher nur den
generischen Zustand des Menueknopfs. Der Gemini-Beleg vom 2026-09-06 zeigt
`aria-expanded=false -> true` bei unveraendertem Modell `Flash`.
Das ist kein Modellwechsel. Matrix vor Audit: 109 passed, 10 failed,
4 unreachable, 7 not_run; keine aktuelle Live-Rezertifizierung.

Isolation: eigener Worktree. Der vorhandene Arbeitsbaum
`C:/Users/storax/projects/GitHub/webagent-rs` auf `feature/T-501-effort-rest`
enthaelt fremde, uncommittierte ChatGPT-Selektoren und Survey-Dateien; diese
bleiben erhalten. Die dort laufende WebAgent-Instanz wird nicht ersetzt.
Die Code-Scheibe ist jetzt lokal abgenommen: Modellwahl verlangt tatsaechlichen
Laufzeitwechsel plus Restore; alte Trigger-PASS sind per Hash-Version ungueltig.
Alle vier Gates gruen: `cargo test --locked --lib` (1331 passed, 1 ignored),
striktes Clippy, no-default-features und TUI-Check. Die Tests legten eine
unabhaengige Race in `benchmark::harvest` offen; die vier Tests teilen einen
Testdatenordner und sind nun per Test-Mutex isoliert. Offene Folge: sechs
Modellzellen erneut live messen, T-501 insgesamt bleibt offen.

> **Checkout-Hinweis (HEAD, 2026-09-07):** dieser Arbeitsbaum lief vor dem
> Rebase auf `feat/durable-transaction-log` (Basis `505d416`). Run-Events
> SHA-256-verknuepft + fsync; `cargo test --lib --no-default-features` 1278
> passed / 1 ignored. Release-Artefakt
> `webagent-0.11.1-transaction-log-repl-fix-x86_64-pc-windows-gnu.exe`.

**Aktualisiert:** 2026-09-08
**Zweck:** verbindlicher Wiedereinstieg und operative Wahrheit. Historische Befunde stehen in `docs/OVERVIEW.md` sowie in den datierten ├£bergaben; diese Datei ersetzt sie nicht, sondern h├ñlt nur den aktuellen Abschlusspfad fest.

**Stand 2026-09-05:** `master` auf `9793623` (PR #33 gemergt: Port-Vereinheitlichung ÔÇ×Ein Listener, zwei RollenÔÇ£ ÔÇö Web-UI + API-Bridge gemeinsam auf `8788`, `api serve` als Alias; `8787` ist historische Dead-Zahl; Details [`CLI_UI_REDESIGN.md`](CLI_UI_REDESIGN.md) ┬º8; `a934db2`/PR #32 `ask` war der Vorgaenger-Commit). **T-501-Nachzieh-Runde 2026-09-05 auf Port 8788** (Breaker-Cooldowns abgelaufen, Bridge mit echtem Login-Profil-Root): `api_responses` jetzt **9/9 passed** (mistral+zai nachgeholt; mistral-Deltas Provider-Timestamps, finale `.done`-Antwort sauber RESP_OK), `streaming` bleibt **6/9** (kimi Reasoning-Echo, mistral Timestamp, zai ÔÇ×Thinking...ÔÇ£), `attachment` 5 Brains frisch gemessen. **Nachtrag `attachment kimi` (2026-09-05, Fokus laut Eigentuemerentscheidung):** Der fruehere 502 `ABSENDEKNOPF_DEAKTIVIERT` lag am entarteten 1x1/70-Byte-Testbild, nicht an der Bridge - mit echtem 256x256-Bild (PIL, ~1,1 kB) laeuft der komplette Upload->Send->Antwort-Pfad durch: **kimi attachment jetzt `passed` (200 "RED", 36,6 s, frische Session)**. qwen/zai bleiben `failed` (auch mit echtem Bild "0 von 1 Dateien": echter SPA-Upload-Gap, Re-Run-Belege `attachment_{qwen,zai}_real_image_2026-09-05.json`), mistral Timestamp, auto Timeout 240s. `managed_tools` by-design 400 erneut bestaetigt. Matrix-Gesamtstand **98/130 passed, 17 failed, 6 unreachable, 9 not_run**; T-501 im TASKBOARD auf `claimed` zurueckgestellt (DoD weiterhin NICHT done: streaming 3, attachment 4, model/effort login-gebunden). **Web-UI-Feature (2026-09-05):** `web/index.html`-Composer zeigt bei leerer Eingabemaske einen Beispiel-Hinweis (`/quelle list`, `/quelle ... --save`, `@brain`) wie die opencode-Promptleiste; verschwindet beim Tippen (`syncHint()`), wirkt nach Neubau (eingebettet via `include_str!`). Belege `docs/proofs/T-501/*2026-09-05.*`, Harness `docs/proofs/T-501/rerun_2026-09-05.py`. Alle aelteren `127.0.0.1:8787`-Erwaehnungen in `docs/proofs/*` bleiben historische Belege und werden nicht angefasst. **Login-Realitaet (Messung 2026-09-05, DeepSeek zuerst):** isolierter No-Login-Smoke via `WEBAGENT_ROOT`-Temp-Root (frisches, leeres Profil; Breaker-Zustand im Temp ÔÇö echter Betrieb unberuehrt): `webagent ask --task "Antworte exakt und nur: DS_NOLOGIN_OK" --brain deepseek --chat --json --headless --no-memory` ÔåÆ `reason=session_state=LoginRequired`, `latency_ms=47561`, deterministische Breaker-Sperre 21600s (nur im Temp-Root), Exit 1. **Befund: deepseek funktioniert ohne Login NICHT** ÔÇö bestaetigt den frueheren Eintrag in `CLI_UI_REDESIGN.md` ┬º2 P2; der README-Login-Claim (ÔÇ×angemeldeter Browser bei dem Dienst, den du nutzen willstÔÇ£) bleibt damit korrekt neutral formuliert. Protokoll: `%LOCALAPPDATA%\Temp\opencode\wa-nologin\deepseek-smoke.log`.

**Historischer Kopf (2026-09-02):** Phase 4 komplett inkl. T-404 SDK-Blackbox (Python/JS-SDK + urllib/fetch, Dumps `docs/proofs/T-404/`). Live T-301/T-302/T-501 ohne Freigabe nicht gegen echte Brains. Naechste Code-Zelle war T-601.

## Verbindlicher Produkt-Neuschnitt vom 01.09.2026

Die bisherigen Teilziele rund um TUI, Browser-Inference-Bridge und einzelne
OpenAI-foermige Routen werden durch einen neuen, messbaren Produktvertrag
geordnet. Der vollstaendige Erkenntnisstand, die Zielarchitektur, Phasen und
Abnahmematrix stehen in [`WEB_UI_API_TOOL_RESET.md`](WEB_UI_API_TOOL_RESET.md).

Kurzfassung:

1. `webagent` erhaelt eine lokale, menschenfreundliche Web-UI als primaere
   Oberflaeche. Die rund 4.400 Zeilen TUI-Code werden vor dem Umbau durch einen
   GitHub-Archivbranch und ein Tag erhalten und bleiben bis zur UI-Abnahme als
   Legacy-Pfad baubar.
2. Claude ist der strengste Referenz-Brain fuer Chat, Modellwahl, Aufwand,
   Attachment und Streaming. Dieselbe Funktionsmatrix muss danach jeder
   beworbene Brain ueber Web-UI und API bestehen.
3. Der OpenAI-Endpunkt wird nicht mehr ueber einzelne HTTP-200-Smokes bewertet.
   Er bekommt versionierte Models-, Chat-Completions- und Responses-Profile,
   offizielle SDK-Black-box-Tests und explizite Fehler fuer nicht umgesetzte
   Semantik. Client-spezifische Sonderfaelle sind ausgeschlossen.
4. WebAgent implementiert selbst einen kleinen Managed-Tool-Kern nach dem
   Umfang von Pi 0.84.4: `read`, `bash`, `edit`, `write`. Pi bleibt reine
   Referenz und wird weder Abhaengigkeit noch Executor. Web-UI und API verwenden
   dieselbe Registry, Policy, Execution und Ereignisstrecke.
5. Der bisherige Prompt, der Systemkontext, Tool-Monster und Verlauf als
   vermeintliche Browseranweisung vermischt, wird verworfen. Reiner Chat zeigt
   nur einen gekennzeichneten Gespraechsverlauf; Managed Agent erklaert dem
   Brain die reale lokale Vermittlung offen und kompakt.
6. Streaming ist Pflicht: sofortiger sichtbarer Zustand, inkrementelle
   Textdeltas, Werkzeugereignisse, Stop und reconnectbarer SSE-Strom. Ein erst
   am Ende sichtbarer Text gilt als `buffered` und besteht das Streaming-Gate
   nicht.

Die folgenden Abschnitte bleiben als historische Teilbelege erhalten. Sie
duerfen insbesondere nicht als Nachweis fuer ein fertiges Web-UI, vollstaendige
OpenAI-Kompatibilitaet oder All-Brain-Toolparitaet gelesen werden.

## Virtuelles AutoRouter-Modell

**Aktualisiert:** 2026-09-01
**Branch:** `feat/browser-inference-provider`

Der OpenAI-Modellkatalog enthaelt nun an erster Stelle das virtuelle Modell
`webagent/auto`. Es wird nicht als statischer Alias behandelt: Der zentrale
Browser-Inference-Pfad klassifiziert Bildgenerierung, Audio- und Bild-Input,
Function-Tools, Coding, aktuelle Recherche sowie allgemeinen Text und waehlt
dafuer ein geeignetes reales Brain. Offene Circuit Breaker werden bei der
Auswahl uebersprungen; fuer zwingende Klassen wie Audio scheitert der Router
ehrlich, wenn kein geeignetes Brain verfuegbar ist. Die Entscheidung erscheint
als strukturierte Zeile im Serverlog.

Die Regeln und ihre Reihenfolge sind in `docs/API_BRIDGE.md` dokumentiert. Die
gleichen Regeln greifen fuer Chat Completions, Responses, Anthropic Messages,
OpenAI Audio und OpenAI Images, weil die Materialisierung direkt vor dem
gemeinsamen Browserturn erfolgt.
Zus├ñtzlich akzeptiert die Modellaufl├Âsung neben den kanonischen
`webagent/...`-IDs auch die sichtbaren ZCode-Aliase `wa/...`; die Antwort des
Modell-Endpunkts bleibt kanonisch und liefert weiterhin `webagent/...`.

| Gate | Ergebnis |
|---|---|
| fokussierte API-/Router-Tests | 39 bestanden |
| Vollfeature-Clippy `-D warnings` | bestanden |
| No-Default-Features-Clippy `-D warnings` | bestanden |
| vollstaendige No-Default-Features-Suite | 1.198 bestanden, 1 ignoriert; 7 Binaertests bestanden |
| Windows-GNU-Debug-Build | bestanden |
| Headless Katalog-Smoke | `webagent/auto` als erstes Modell mit Routingmetadaten |
| Headless Text-Smoke | zu ChatGPT geroutet, exakt `AUTO_ROUTE_OK`, 18,40 s |
| Headless WAV-Smoke | zu Gemini geroutet, `Die Pruefziffer lautet 789`, 22,33 s |

Nach dem Live-Befund, dass ChatGPT und Claude bei Audio teils HTTP 200 mit einer
reinen Absage liefern, klassifiziert die Bridge eindeutige
Transkriptionsabsagen jetzt als HTTP 502. Damit werden diese Provider nicht
mehr als scheinbar erfolgreiche Audio-Backends ausgegeben; die beworbene
Audio-Eingabe bleibt auf `gemini` und `webagent/auto` begrenzt.

### Frische headless Text-Rezertifizierung (2026-09-01)

Ein serieller Smoke mit eindeutigen Antwortmarkern lief gegen den aktualisierten
Endpoint auf `127.0.0.1:8787`; parallele Browserruns wurden vermieden.
ChatGPT (25,7 s), Claude (29,4 s), DeepSeek (14,3 s), Gemini (34,6 s), Kimi
(22,8 s), Mistral (48,8 s), Qwen (31,1 s) und Perplexity (16,7 s) antworteten
jeweils mit dem angeforderten exakten Marker und HTTP 200. Nach Ablauf des
providerbezogenen Circuit Breakers antwortete Z.ai im Recovery-Smoke ebenfalls
mit HTTP 200 in 20,7 s. Die Oberfl├ñche stellte dem Marker den UI-Titel
`Thought Process` voran; die API-Schicht entfernt diesen Titel jetzt
konservativ nur f├╝r Z.ai und l├ñsst den eigentlichen Antworttext unver├ñndert.

Ein ZCode-kompatibler Streaming-Smoke gegen `webagent/chatgpt` lieferte HTTP
200, Keep-Alive-Frames, inkrementelle `delta.content`-Chunks und einen
abschlie├ƒenden `finish_reason: stop` (18,4 s).

## Aktueller Fix: multimodaler Upload-Fallback f├╝r dynamische Browser-UIs

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`

Die bisherige Bild-/Audio-Strecke setzte ausschlie├ƒlich ein bereits vorhandenes
`input[type=file]` voraus. Genau das f├╝hrte bei DeepSeek, Gemini, Kimi und Mistral
zu `no_file_input`, sobald eine alte Bildnachricht im ZCode-Verlauf mitgesendet
wurde. `browser::send::attach_files` ├Âffnet jetzt zun├ñchst vorsichtig die
konfigurierte Attach-Oberfl├ñche (untrusted DOM-Klick), wartet auf ein dynamisch
gerendertes Datei-Input und f├ñllt danach auf Paste/Drop zur├╝ck. Der Fallback gilt
nur bei einer sichtbaren neuen Attachment-Vorschau als erfolgreich; andernfalls
kommt weiterhin ein erkl├ñrender 502. Native Dateidialoge werden nicht ge├Âffnet.

### Korrektur zum Providerbefund vom 31.08.2026

Die fr├╝here Formulierung ÔÇ×kein Datei-UploadÔÇ£ war f├╝r DeepSeek, Gemini, Kimi und
Mistral zu stark: Eine frische DOM-/Screenshot-Inventarisierung zeigt bei allen
vier eine Upload-Oberfl├ñche. DeepSeek rendert ein verstecktes
`input[type=file]` und eine B├╝roklammer; Gemini erzeugt nach ÔÇ×Uploads & ToolsÔÇ£
versteckte Inputs; Mistral rendert `input[name=file-upload]`; Kimi zeigt die
Toolkit-/+-Schaltfl├ñche und Attachment-Zust├ñnde. `no_file_input` war dort ein
Entdeckungsfehler eines vollst├ñndigen Verlaufs, kein Beleg f├╝r fehlende
Providerfunktion.

Der Windows-Treiber versucht deshalb neben dem bestehenden Paste/Drop-Weg einen
nativen WebView2-CDP-Upload (`DOM.setFileInputFiles`) und akzeptiert sowohl die
direkte WebView2-Antwortform als auch den ├╝blichen `result`-Wrapper. Dieser Weg
wurde live mit DeepSeek (23,33 s), Gemini (26,06 s), Kimi und Mistral als Bild-Request bis
zur Antwort `IMAGE_INPUT_OK` verifiziert. Kimi verwendet daf├╝r eine echte
Windows-Datei├╝bergabe (`CF_HDROP`) mit abgefangenem WebView2-Dateichooser und
den realen Pfeil-Button seines Lexical-Composers. Ein Submit gilt erst als
bewiesen, wenn Kimi den Composer tats├ñchlich konsumiert hat. Audio wurde mit
Gemini anhand einer synthetisierten WAV-Sprachphrase inhaltlich verifiziert.

Die generische Selektormaske enth├ñlt daf├╝r providerneutrale Attach-Signale
(`aria-label`, `data-testid`, `data-tooltip`, Datei-/Upload-Beschriftungen). Die
Text-Circuit-Breaker-Behandlung klassifiziert sowohl `no_file_input` als auch
`no_file_input_and_paste_not_confirmed` als request-spezifische F├ñhigkeitsgrenze;
Textturns eines betroffenen Brains bleiben davon unber├╝hrt.

| Gate | Ergebnis |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| fokussierte Upload-/Relay-Tests (`cargo test --locked --no-default-features send --lib`) | 12 bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo check --locked` (WebView2-Feature) | bestanden |
| Live-Bild-Input-Smoke `webagent/chatgpt` (1x1 PNG, exakte Antwort `IMAGE_INPUT_OK.`) | bestanden |
| Live-Bild-Input-Smoke `webagent/deepseek` (30 KB PNG, `IMAGE_INPUT_OK`) | bestanden (23,33 s) |
| Live-Bild-Input-Smoke `webagent/gemini` (30 KB PNG, `IMAGE_INPUT_OK`) | bestanden (26,06 s) |
| Live-Bild-Input-Smoke `webagent/kimi` (512x512 PNG, `IMAGE_INPUT_OK`) | bestanden (HTTP 200) |
| Live-Bild-Input-Smoke `webagent/mistral` (512x512 PNG, `IMAGE_INPUT_OK`) | bestanden (HTTP 200) |
| Live-Audio-Input-Smoke `webagent/gemini` (WAV-Sprachphrase, `AUDIO INPUT OK`) | bestanden (HTTP 200) |

Die Kimi-Korrekturen sind in `e668ff0` und `8f4e074` enthalten. Alte rote
Fehlerkarten werden entfernt bzw. der verunreinigte Draft wird einmalig
zurueckgesetzt. Der Upload faellt nicht mehr auf den rote Karten erzeugenden
synthetischen `DataTransfer`-Pfad zurueck. Der finale Smoke lief mit einem
512x512-PNG ueber `/v1/chat/completions` und lieferte HTTP 200 sowie exakt
`IMAGE_INPUT_OK`; Kimi-Bildinput ist damit freigegeben.

Der identische WAV-Request gegen ChatGPT Web wurde transportiert und mit HTTP
200 beantwortet, das dort ausgewaehlte Web-Modell verweigerte jedoch selbst die
Transkription. Das ist eine providerseitige Modellgrenze; der positive
Gemini-Smoke belegt den gemeinsamen API- und Uploadpfad.

ChatGPT-Bildausgabe ist inzwischen als eigener Headless-Pfad ├╝ber
`POST /v1/images/generations` implementiert. Ein extrahiertes Resultat wurde als
808.492-Byte-PNG mit korrekter Signatur und 1254x1254 Pixeln dekodiert und
visuell gepr├╝ft. Weitere Gegenproben deckten zwei False-Positive-Quellen auf und
schlossen sie: das 104x104-Toolicon wird durch die nach Toolaktivierung
aufgenommene Baseline ausgeschlossen; alte Estuary-Bilder bleiben anhand ihrer
stabilen `file_...`-ID ausgeschlossen, auch wenn ChatGPT `ts` und `sig` ├ñndert.
Der aktuell folgende atomare Neu-Generierungs-Smoke scheitert ehrlich am
providerseitigen Free-Plan-Kontingent (Reset laut UI am 01.09.2026 um 16:57)
und wird nicht als Erfolg gewertet. F├╝r Gemini wurde der zuvor fehlende
provider-spezifische Pfad erg├ñnzt: ÔÇ×Uploads & ToolsÔÇ£ wird nach dem asynchronen
`new_chat()`-Remount per trusted CDP-Pointer ge├Âffnet und ÔÇ×Bild erstellenÔÇ£
aktiviert. Headless waren Moduswechsel (ÔÇ×BilderÔÇ£), Submit und alternierende
Renderer-Wakeups belegt; Gemini blieb trotzdem bis zum absoluten 180- bzw.
300-Sekunden-Budget bei ÔÇ×Creating your imageÔÇ£, ohne neues Bild im DOM. Gemini
wird daher noch nicht mit Bildausgabe beworben. Alle anderen Brains melden
weiterhin nur Textausgabe. Eine Behauptung ÔÇ×100 % API-Kompatibilitaet fuer alle
BrainsÔÇ£ ist mit diesem Stand ausdr├╝cklich nicht zul├ñssig.

Die anschlie├ƒende headless Werkzeugmen├╝-Vermessung aller ├╝brigen Profile fand
keinen weiteren Bildgenerator: Claude und Qwen bieten Datei-Upload, Mistral
einen Medien-Upload einschlie├ƒlich verbreiteter Audioformate; DeepSeek, Kimi,
Perplexity und Z.ai zeigten keinen Bild-/Audio-Generator. Damit sind
`/v1/images/generations`-Backends aktuell nur f├╝r ChatGPT und Gemini sachlich
begr├╝ndet; Claude best├ñtigt offiziell, dass der Chat nur textbasierten Output
liefert und Bilder ausschlie├ƒlich als Input verarbeitet.

Die bislang fehlenden eigenst├ñndigen OpenAI-Audiorouten sind anschlie├ƒend
erg├ñnzt worden. `/v1/audio/transcriptions` und `/v1/audio/translations` parsen
den offiziellen bin├ñren Multipart-Upload, routen ihn als Audio-Anhang an ein
Web-Brain und liefern `json`, `text` oder ein konservatives `verbose_json`.
Bin├ñrbytes, Dateiname, MIME-Typ, Modell und Format sind durch Regressionstests
abgedeckt. `/v1/audio/speech` existiert als ehrliche fail-closed Grenze: Keines
der aktuell vermessenen Web-Brains liefert einen belegten, extrahierbaren
TTS-Audiostream, daher werden dort keine synthetischen Audioantworten behauptet.
Ein echter headless Multipart-Smoke gegen `webagent/gemini` transkribierte die
neu synthetisierte Kontrollaufnahme ÔÇ×Die Pr├╝fziffer lautet sieben acht neunÔÇ£ in
23,8 Sekunden korrekt zu `{"text":"Die Pr├╝fziffer lautet 789"}`. Ein erster
englischer Satz aus einer deutschen Windows-Stimme war phonetisch ungeeignet;
dieser Quellenfehler wird nicht als Provider- oder Endpoint-Erfolg gewertet.

## Aktueller Fix: gro├ƒe ZCode-Toolrequests im Browser-Provider

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`
**Commit:** `7bd1881` (`fix: compact large browser tool prompts`)

Ein ZCode-Request mit 53 Werkzeugen und rund 124 KiB JSON reproduzierte bei
ChatGPT die Web-UI-Antwort ÔÇ×Something went wrongÔÇ£. Die folgenden Claude- und
DeepSeek-Versuche liefen dadurch im Client in `ECONNREFUSED`, weil der lokale
Endpoint w├ñhrend des vorherigen Neustarts nicht erreichbar war; ein frischer
Textturn gegen beide Brains ist danach erfolgreich durchgelaufen.

`browser_inference::prompt_with_tools` kompaktisiert den Toolblock nun
stufenweise auf h├Âchstens 64 KiB. Werkzeugnamen und Parameterschemata bleiben
vollst├ñndig erhalten, Beschreibungstexte werden nur bei Bedarf gek├╝rzt.

| Gate | Ergebnis |
|---|---|
| Kompakter Unit-Test | bestanden |
| ZCode-Request mit 53 Tools, normal | HTTP 200, kein Web-UI-Fehler |
| Derselbe Request ├╝ber SSE | HTTP 200, g├╝ltiges `data: [DONE]` |
| Claude nach dem ChatGPT-Request | HTTP 200, `CLAUDE_CHECK_42` |
| DeepSeek nach dem ChatGPT-Request | HTTP 200, `deepseek_OK` |
| `cargo clippy --locked --all-targets --no-default-features -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.186 bestanden, 1 ignoriert |

Der laufende Release-Endpoint verwendet die neue Binary auf `127.0.0.1:8787`
und antwortet auf `/health` mit `status=ok`.

Die Browser-Einleitung wurde anschlie├ƒend neutralisiert: Sie beschreibt den
Inhalt als Gespr├ñchsverlauf eines API-Clients und vermeidet sichtbare
`Provider-Bridge`-/`WEBAGENT_INFERENCE/1`-Identit├ñtsdiskussionen. Der
Tool-Umschlag bleibt f├╝r echte Client-Werkzeuge strikt und fail-closed, ist aber
kurz als Client-Werkzeugliste formuliert. Das verhindert unn├Âtige Claude-
Metaantworten, ohne providerseitige Sicherheitsablehnungen zu ver├ñndern oder
zu umgehen.

Die F├ñhigkeitsgrenze bleibt bewusst dokumentiert: Bild-/Audio-Eingaben werden
hochgeladen, die Ausgabe wird derzeit ausschlie├ƒlich als Text extrahiert.
Generierte Bildartefakte aus der ChatGPT-Weboberfl├ñche sind noch kein
API-`image`-Output.

Ein konkreter Fehlerbefund aus dem ZCode-Test wurde getrennt: DeepSeek, Gemini
und Kimi hatten wegen eines mitgef├╝hrten Bildes `no_file_input` geliefert. Dieser
F├ñhigkeitsfehler wird in `relay` jetzt nicht mehr als allgemeiner Brain-Ausfall
gez├ñhlt, nicht wiederholt und nicht in den Text-Circuit-Breaker ├╝bernommen. Ein
sp├ñterer text-only Turn bleibt dadurch m├Âglich; die API meldet die
Multimodalit├ñtsgrenze weiterhin ehrlich mit 502.

## Browser-Inference-Provider-Scheibe vom 29.08.2026

Die aktuelle Arbeit liegt isoliert auf `feat/browser-inference-provider`, ausgehend von `master` bei `a0cd00b7`. `api_bridge` startet nicht mehr den vollst├ñndigen `AgentController`, sondern ruft die neue harnessfreie Grenze `browser_inference::complete()` auf. Dadurch sind WEBAGENT/1, Shell-/Dateiaktionen, Controller-Memory und der Plan-Act-Observe-Loop nicht mehr Teil eines Providerrequests. Der bestehende Rust-Harness bleibt unver├ñndert als separater Consumer erhalten.

Die Providertrennung liegt in `83bc90d`; die abgeschlossene Pi-0.84.4-Integration samt reproduzierbarem Tool-Smoke liegt in `c8b98ed`. Beide Commits gehoeren zum Branch `feat/browser-inference-provider`; diese Uebergabe wird als nachfolgender reiner Dokumentationscommit abgeschlossen.

Der OpenAI-Adapter normalisiert Function-Tools, `tool_choice`, Assistant-`tool_calls` und `role=tool`-Ergebnisse. Browsermodelle geben Tool Calls ├╝ber einen strikten `WEBAGENT_INFERENCE/1`-Umschlag zur├╝ck; unbekannte Tools, doppelte IDs und verletzte erzwungene Toolwahl scheitern fail-closed. Die Bridge f├╝hrt Tools ausdr├╝cklich nicht selbst aus. Anthropic Messages bleibt in dieser Scheibe textbasiert; SSE bleibt gepuffert statt tokeninkrementell.

| Gate | Ergebnis dieser Scheibe |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.170 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden |
| `cargo test --locked` | 1.209 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo build --locked --release` | bestanden; `webagent.exe` 7.005.184 Bytes (6,68 MiB) |
| Lokaler API-Smoke | `/health=ok`, gesch├╝tztes `/v1/models=webagent/chatgpt`, kontrollierter Shutdown bestanden |
| Live-Textturn ChatGPT | bestanden am 30.08.2026: `BRIDGE_OK` in 20,8 Sekunden |
| Pi 0.84.4 Textturn | bestanden: Provider entdeckt, `PI_BRIDGE_OK` ueber SSE, `stopReason=stop` |
| Live-Tool-Call | bestanden: Pi 0.84.4 fuehrte `read` auf einer Zufallsdatei aus, `role=tool` wurde zurueckgesendet und der exakte Nonce-Inhalt final ausgegeben |

`webagent relay` lieferte live `BRIDGE_OK` in 20,8 Sekunden; Pi 0.84.4 entdeckte den Beispielprovider und erhielt ueber gepuffertes SSE `PI_BRIDGE_OK` mit `stopReason=stop`. Der erste Pi-Toolversuch zeigte zusaetzlich, dass eine weiche `auto`-Instruktion das Browsermodell nicht sicher zum explizit verlangten Tool zwingt und externe Client-Tools mit eingebauten Browserwerkzeugen verwechselt. Die Browser-Inference-Anweisung trennt beides jetzt explizit und verlangt Tools fuer ausdrueckliche Toolauftraege sowie lokale/aktuelle/unbekannte Daten. Der erneute zufallsbasierte Pi-read-Smoke bestand den kompletten Loop: Tool Call, lokale Pi-Ausfuehrung, Tool-Ergebnis-Rueckgabe und finale Nonce-Antwort.

Die reproduzierbare Nutzerstrecke liegt in `examples/pi/models.json` und `scripts/test-pi-provider.ps1`. Sie nutzt die aktuelle Pi-Paketlinie `@earendil-works/pi-coding-agent`, isoliert `PI_CODING_AGENT_DIR`, prueft erst Text und dann einen echten Tool-Loop und veraendert weder die regulaere Pi-Konfiguration noch das Repository.

## Dynamischer Multi-Brain-Katalog vom 30.08.2026

Die Bridge ist nicht mehr auf das beim Serverstart angegebene Brain beschraenkt. `/v1/models` wird aus `config::brains()` erzeugt und enthaelt damit alle eingebauten sowie registrierten Custom-Brains. Jede OpenAI- oder Anthropic-Anfrage loest `webagent/<brain>` vor dem Browserstart auf und uebergibt dieses Brain an die harnessfreie Inference-Schicht; `webagent` allein bleibt Alias fuer `--brain`.

`examples/pi/extensions/webagent-models.ts` fragt den Katalog beim Pi-Start dynamisch ab und registriert den Provider. Der neue Pi-Befehl `/models` aktualisiert den Endpoint-Katalog und oeffnet die Brain-Auswahl; `/models <brain>` wechselt direkt. Die lokale Pi-0.84.4-Installation listete live `chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`, `qwen`, `zai` und das Custom-Brain `perplexity`. Die statische globale Ein-Modell-Konfiguration wurde auf diesem Rechner durch die dynamische Erweiterung ersetzt.

Die Release-Binary mit diesem Stand lieferte den dynamischen Katalog live aus. Pi lud die Repository-Erweiterung, listete alle neun Brains und bestand einen echten ChatGPT-Request mit der exakten Antwort `DYNAMIC_MODELS_OK`. Das direkte Kommando `/models claude` wurde ebenfalls ohne Fehler verarbeitet; ein produktiver Turn mit jedem weiteren Brain setzt dessen gueltiges Browser-Login voraus.

Die Implementierung ist als `1015586` (`feat: add dynamic pi brain switching`) auf `origin/feat/browser-inference-provider` gepusht.

## Latenz- und Isolation-Optimierung vom 30.08.2026

`api_bridge` serialisiert Browserruns jetzt nur noch pro Brain; unterschiedliche Brains koennen parallel arbeiten. In `relay` werden deterministische Sendefehler (fehlender Absende-Beweis, deaktivierter Knopf, erkannte Blockade, Login-/Cloudflare-/Limit-Hinweis) nicht mehr dreimal vollstaendig wiederholt; transiente CDP- und Navigationsfehler bleiben retry-faehig. Der identische Gemini-Fehler sank live von 95,351 s auf 33,408 s. Vollfeature-Clippy, fokussierte Relay-Tests und Release-Build bestanden.

## Aktueller Repositoryzustand

> **Historisch (Stand vor 2026-09-04).** Die nachfolgenden Branch-, Commit- und
> Gate-Angaben betreffen den damaligen Stand `task/v1-release-baseline` und sind
> mit dem aktuellen `master` (`505d416`) ├╝berholt. Aktueller Einstieg: der Kopf
> dieser Datei und `feature/docs-cli-ui-fixes`.

Die Arbeit l├ñuft auf `task/v1-release-baseline`, ausgehend von `origin/master` bei `1d214e8`. Die erste abgeschlossene Baseline-Scheibe ist als `2659baf` (`fix: restore headless release baseline`) committet; der Scope-Freeze und die Definition of Done folgen in `4fdb068` (`docs: define v1 release completion`). Beide Commits sind auf `origin/task/v1-release-baseline` gepusht. Die Baseline-Scheibe stellte den browserfreien Releasezustand wieder her: fehlende Bibliotheksmodule wurden registriert, der Root-Eintrag in `Cargo.lock` wurde auf `0.11.1` korrigiert, reine Kernmodule wurden von unn├Âtigen TUI-Gates entkoppelt, strikte Lint-/Testdrift wurde behoben und die alte Wilson-Dublette wird auf die gemeinsame Implementierung zur├╝ckgef├╝hrt.

Besonders wichtig ist die Korrektur von `capture_patch`: Eine globale Git-Einstellung wie `color.ui=always` lieferte ANSI-Sequenzen in `git diff` und machte die Scope- sowie Harvest-Pr├╝fung blind. Der maschinell verarbeitete Patch wird nun explizit ohne Farbe erzeugt; reale Gegenproben auf einem tempor├ñren Git-Repository belegen In-Scope-Harvest und fail-closed Out-of-Scope-Ablehnung.

| Gate | Ergebnis auf dem aktuellen Arbeitsstand |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | **1.157 Bibliotheks-, 7 Bin├ñrtests bestanden; 1 Test bewusst ignoriert** |
| `git diff --check` | bestanden |
| Reale Scope-/Harvest-Gegenprobe | bestanden; ANSI-farbige Git-Diffs werden sicher verarbeitet |
| `cargo clippy --locked --all-targets -- -D warnings` auf Linux/WebKitGTK | bestanden |
| `cargo test --locked` auf Linux/WebKitGTK | **1.196 Bibliotheks-, 7 Bin├ñrtests bestanden; 1 Test bewusst ignoriert** |
| Windows-CI `33001615724` auf `e1e5a74` | Formatcheck, Vollfeature-Clippy, Vollfeature-Build und Vollfeature-Tests vollst├ñndig bestanden |

## v1.0-Ziel und Definition of Done

> **v1.0 bedeutet nicht, dass jede Idee umgesetzt ist.** Webagent ist fertig, wenn der dokumentierte lokale Browser-Agent verl├ñsslich gebaut, getestet, betrieben und anhand aktueller Evidenz freigegeben werden kann. Ein gr├╝ner Unit-Test oder ein abgeschlossener Teil-Goal reicht daf├╝r nicht aus.

| Abschlussbereich | Verbindliches Abnahmekriterium |
|---|---|
| **Reproduzierbarer Kern** | Formatcheck, striktes Headless-Clippy und vollst├ñndige browserfreie Test-Suite sind auf dem Release-Candidate gr├╝n; der Lockfile-Stand ist konsistent. |
| **Plattformgrenze** | Die Vollfeature-Gates (`cargo clippy --all-targets -- -D warnings`, `cargo test`) sind auf Windows mit WebView2 oder durch nachvollziehbare Windows-CI-Evidenz gr├╝n. Linux bleibt ein unterst├╝tzter browserfreier Kern-/TUI-Pfad, ersetzt aber keine Windows-WebView-Abnahme. |
| **Providerf├ñhigkeit** | Jeder in der Releasekonfiguration aktivierte Provider besitzt eine frische, nachvollziehbare Send-/Antwort-Evidenz oder ist bewusst deaktiviert und mit Ursache dokumentiert. Keine historische Erfolgsmeldung gilt als Live-Beleg. |
| **Mehr-Brain-Betrieb** | Poolstart, Heartbeat, Recovery, Profil-Lease, Write-back, bereinigbarer Shutdown sowie Continuation und Cross-Brain-Handoff sind systemisch belegt. Ein abgebrochener Lauf darf weder das Master-Profil besch├ñdigen noch ungemeldete Klonreste hinterlassen. |
| **Harvest-Sicherheit** | Ein g├╝ltiger Patch wird nach Wiederanwendung erneut gebaut und getestet; Scope-Verst├Â├ƒe, L├Âschungen und nicht nachvollziehbare Patches bleiben fail-closed ausgeschlossen. |
| **Betriebsdokumentation** | `docs/OVERVIEW.md`, diese Datei, `CHANGELOG.md` und die Release-Checkliste stimmen mit Code, Tests und bekannten Grenzen ├╝berein. |
| **Releaseentscheidung** | Der Release Candidate ist auf einem sauberen Tree und ├╝ber GitHub erreichbar; erst nach dokumentierter Review- und Eigent├╝merfreigabe folgen Merge, Tag und GitHub-Release. |

## Scope-Freeze

Der v1.0-Kritische Pfad umfasst ausschlie├ƒlich die nachfolgenden Arbeiten. Jede Erweiterung, die keine Definition-of-Done-Zeile erf├╝llt, bleibt au├ƒerhalb des Releasepfads.

| Im v1.0-Scope | Bewusst nach v1.0 verschoben |
|---|---|
| Browserfreie Releasequalit├ñt, Windows-Vollfeature-Evidenz und CI-Reproduzierbarkeit | Terminal-Renderer und ARIA-zentrierte Vermittlungsschicht |
| Frische Provider-Rezertifizierung und klarer Umgang mit Drift bzw. Ausf├ñllen | CDP-/AX-Spike ├╝ber die heutige Browseranbindung hinaus |
| Shutdown-, Profil-, Lease- und Klonbereinigungsh├ñrtung | Reale Free-Cloud-Transportadapter oder kostenpflichtige Fallbacks |
| End-to-End-Abnahme f├╝r Pool, Continuation, Cross-Brain-Handoff und Harvest | Genius Council, zus├ñtzliche Agentenrollen und gr├Â├ƒere UX-Erweiterungen |
| Dokumentierte Release-Candidate- und Freigabevorbereitung | Neue Provider, neue Persistenz- oder Protokollmechanismen ohne Abschlussbezug |

## Providerbereitschaft und Desktop-Grenze

Die aktiven Standardprovider sind aktuell `chatgpt`, `deepseek`, `kimi`, `gemini`, `qwen`, `claude`, `mistral` und `zai`. Die ausgelieferten Selektoren f├╝r alle acht Anbieter werden eingebettet, sind valides JSON und bestehen die beiden providerfreien Vertr├ñge `test_embedded_selectors_cover_all_brains_and_parse` sowie `all_configured_brains_have_selectors`. `perplexity` besitzt eine ausgelieferte Selektordatei, ist jedoch nicht Teil des aktuellen statischen `BRAIN_TABLE` und damit kein v1.0-Standardprovider.

Eine manuelle Browser-Gegenprobe auf ChatGPT im verbundenen Browser lieferte den erwarteten Antworttext `BEREIT.`. Sie ist **keine Webagent-WebView-Evidenz**, weil sie nicht aus dem vom Nutzer getesteten Desktop-Arbeitsordner stammt. Der anschlie├ƒende DeepSeek-Aufruf leitete in diesem separaten Browserprofil auf `/sign_in` um; auch das ist kein Befund ├╝ber die projektseitigen Profile.

Der vom Nutzer getestete Windows-Desktop-Arbeitsordner ist in dieser Sitzung nicht erreichbar: Der Sandbox-Desktop enth├ñlt keinen Webagent-Ordner, die Browserverbindung blockiert lokale `file:///`-Pfade, der Remote enth├ñlt keinen neueren Desktop-Commit und es ist keine Desktop-/Computer-Verbindung konfiguriert. Das ist ein technischer Zugriffsblocker, keine fehlende Bereitschaft des Nutzers und keine Providerbewertung. Bis zu einem erreichbaren Desktop-Stand bleiben echte WebView-, Profil- und Providerbelege offen.

## Linux-WebView-Abnahme

Die Vollfeature-Abnahme auf Linux legte eine echte Konfigurationsregression offen: Die vorherige Headless-Bereinigung hatte `CloneGuard::path` entfernt, obwohl der WebView-Pfad sie bei der Runtime-├£bergabe ben├Âtigt. Zus├ñtzlich war `browser_args` unter WebKitGTK ungenutzt und zwei Linux-Stubs lagen hinter dem Testmodul. Commit `d32b8d0` grenzt die Windows-Argumente sauber ein, stellt den WebView-gateten Zugriff wieder her und ordnet die Plattformstubs lintkonform vor den Tests an. Nach Installation der notwendigen lokalen GTK-/WebKit-Entwicklungspakete sind die strikten Headless- und Vollfeature-Lints sowie beide vollst├ñndigen Test-Suiten gr├╝n.

Diese Evidenz best├ñtigt den Linux-WebKitGTK-Build und seine Tests. Erg├ñnzend hat die Windows-CI `33001615724` auf dem aktuellen Branch die Vollfeature-Gates vollst├ñndig bestanden. Beide Belegketten ersetzen jedoch keine Live-Sitzungs- oder Providerabnahme auf dem tats├ñchlichen Desktop-Arbeitsstand.

## Betriebsh├ñrtung: aktueller Stand

Die beiden historischen Betriebsbefunde sind im gegenw├ñrtigen Quellstand bereits strukturell adressiert. `main::run` ruft vor jedem regul├ñren Ablauf `sweep_stale_runtime_profiles()` auf; die Bereinigung betrachtet ausschlie├ƒlich die Wegwerf-Wurzeln `swarm` und `encapsulated`, verlangt eine lesbare Altersinformation und l├ñsst kanonische Login-Profile bewusst unangetastet. Der gezielte Test `test_sweep_stale_runtime_profiles_spares_fresh_and_logins` belegt genau diese Grenze.

F├╝r den kontrollierten TUI-Exit existiert `--run-secs`: Die Deadline f├╝hrt in denselben Cleanup-Pfad wie die Taste `q`. Dieser schreibt `PoolControl { stop: true }`, joint den Worker-Thread und ruft anschlie├ƒend `BrowserPool::shutdown_with_result()` auf, damit der geordnete Tab-Teardown und ein m├Âglicher Write-back stattfinden. Der Worker-Pool belegt mit `kill_all_children_is_idempotent_and_empties_the_pool`, dass der Stopvorgang wiederholbar und vollst├ñndig ist. Eine echte Windows-WebView-/Write-back-Gegenprobe bleibt jedoch Teil der sp├ñteren Systemabnahme; der browserfreie Test beweist nicht die vorhandenen Desktop-Sitzungen.

## Kritischer Pfad und Blocker

Die n├ñchste technische Scheibe ist die **Provider-Rezertifizierung**. Vor dem Live-Lauf werden die bestehenden Selektoren, Capability-Proofs und Fehlerbilder lokal inventarisiert. Die eigentliche Messung darf nur stattfinden, wenn der Eigent├╝mer anwesend ist: Browserfenster k├Ânnen sich ├Âffnen, aber Anmeldungen, Einmalcodes, CAPTCHAs und Zugangsdaten verbleiben vollst├ñndig beim Eigent├╝mer. Der Integrator protokolliert ausschlie├ƒlich die beobachteten Zust├ñnde und nutzt keine Geheimnisse.

Der aktuell bekannte spezifische Providerrest ist `zai`: Die Antwort kann einen `Thought Process` voranstellen; ein fr├╝herer ├£berlastungszustand verhinderte die belastbare Gegenprobe. Ein Provider mit frischer, nicht klassifizierbarer Antwort wird nicht still als releasebereit bewertet. F├╝r alle Provider gilt dieselbe Regel: frische Evidenz oder dokumentierte, bewusst deaktivierte Grenze.

| Abh├ñngigkeit | Status | Umgang |
|---|---|---|
| Eigent├╝mer f├╝r Live-Browser/Anmeldung | erforderlich | Vor der tats├ñchlichen Live-Rezertifizierung gezielt um Freigabe bitten; bis dahin nur lokale Vorbereitung. |
| Windows-WebView2-Gates | CI bestanden, Live-Abnahme offen | Windows-CI `33002148599` ist vollst├ñndig gr├╝n; reale WebView2-Sitzungen und Profile bleiben separat zu pr├╝fen. |
| Merge | abgeschlossen | Die gepr├╝ften Baseline- und Plattformkorrekturen wurden per Fast-Forward nach `master` integriert. |
| Tag/GitHub-Release | offen | Erst nach vollst├ñndiger Live-Evidenz und Eigent├╝merfreigabe vorbereiten; keine Ver├Âffentlichung vorab. |

## N├ñchste sichere Aktion

Die n├ñchste sichere Aktion ist die **Live-Abnahme am erreichbaren Desktop-Arbeitsstand**: je aktivem Provider denselben harmlosen Sende-/Antwortpfad pr├╝fen, Zust├ñnde klassifizieren und die Windows-WebView2-/Write-back-Gegenprobe dokumentieren. Der aktuelle Windows-Desktop-Arbeitsordner ist aus dieser Sitzung nicht erreichbar; bis eine erreichbare Verbindung besteht, werden weder unbefugte Logins noch Ersatzsitzungen als Projektevidenz verwendet.

## ├£bergabe

- **Branch:** `master`; G-001-Abschluss-Stand `2589b0a`, Release `v0.11.3` (Tag `7021959`) ver├Âffentlicht.
- **Abgeschlossene Scheiben:** `2659baf` (Headless-Baseline), `4fdb068` (v1.0-Definition), `d32b8d0` (Linux-WebView-Feature-Gates), `f51acd6` (Linux-/Windows-Abnahmest├ñnde), Phase-9 `T-901..T-906` inkl. Web-UI-Review-Fixes (`44f7a1e`), Release `v0.11.3`, Toolchain-Norm `2589b0a`.
- **Eigent├╝merschaft:** aktueller Integrator hat den v1.0-Abschluss bearbeitet; Live-Anmeldungen bleiben beim Eigent├╝mer.
- **Externe Freigaben:** f├╝r Release `v0.11.3` erteilt (Eigent├╝merwahl ÔÇ×Erst README/Bild, dann Release"); Tag und GitHub-Release sind erfolgt.
- **Arbeitsbaum:** nach diesem ├£bergabecommit erneut pr├╝fen; `master` direkt pushen, keine History umschreiben.

> **Hinweis zur Wahrheitspflege:** Historische Live-Befunde aus August 2026 sind n├╝tzlich f├╝r die Diagnose, aber kein aktueller Verf├╝gbarkeitsbeleg. Ma├ƒgeblich f├╝r v1.0 sind frische, reproduzierbare Belege am Release Candidate ÔÇö f├╝r `v0.11.3` in `docs/proofs/G-001/g001-abschluss-2026-09-12.json` zusammengef├╝hrt.

## T-1019 ÔÇö Provider-Chat fortsetzen (2026-11-27)

**Status:** done, Code-Commit `89605ba` auf `feature/T-1019-continue-provider-chat`.
**Belege:** `docs/proofs/T-1019/README.md`, `docs/proofs/T-1019/gates.txt`.

Mit `WEBAGENT_CONTINUE_PROVIDER_CHAT=1` schickt die Bridge nur die aktuelle
Nachricht in den Provider-Chat; der Default bleibt unveraendert. Shared-Tabs
werden nicht neu navigiert, der Relay macht genau einen Versuch und ueberspringt
`new_chat()` nur bei nachgewiesener `/c/...`-Konversation.

Gates: fmt/clippy/lib/arch/tui/no-default/diff-check/release alle gruen;
1562 Tests bestanden, 0 fehlgeschlagen, 1 ignoriert. Sauberer Release-Build
`89605ba7d`, EXE-SHA256 `CE6F874FBDAD3A8D1F479847621DBD28645AA93CB5461299750F524DF0118F2D`.

Live auf Test-Port 8789: Turn 1 HTTP 200, genau `T1019-TURN1-OK`; Turn 2 HTTP
200, genau `Otter`; beides `success=true`, `sends=1`, `prompt_chars` 236/231.
Der zweite Prompt enthaelt das Wort `Otter` nicht ÔÇö das Gedaechtnis lag beim
Provider, nicht im Bridge-Kontext. Ein zuvor gebautes bedingungsloses
`new_chat`-Skip wurde mit HTTP 502 und
`send_phase=insert: Composer-Feld nicht gefunden (Timeout)` verworfen und
nicht als PASS gezaehlt. Im finalen Lauf war die Tab-URL keine `/c/...`-URL;
der bedingte Skip-Pfad wurde nicht ausgeloest und wird nicht als Live-Beleg
behauptet.

Integration: Branch `feature/T-1019-continue-provider-chat` mit `89605ba` und
`b8b9401` gepusht und per Fast-Forward nach `master` (`b8b9401`) integriert;
`185dc35` dokumentiert die Integration. Danach ausdruecklich freigegebener
Deploy auf 8788: Backup
`webagent-deploy-target\backup-release-before-T1019-20261127\`, alte PID
`9008` gestoppt, neue PID `7236` mit identischer Command-Line, deployte EXE
`CE6F874FÔÇªF0118F2D`, DLL `465A7DDFÔÇªA0A6F`. Aktiv:
`WEBAGENT_USE_SHARED_BROWSER=1`, `WEBAGENT_PERSIST_TABS=1`,
`WEBAGENT_CONTINUE_PROVIDER_CHAT=1`. Health ok, Auth 200/401, Turn 1
`T1019-DEPLOY-OK`, Turn 2 `Otter`, beide `success=true`.
`bridge.pid` auf `7236` korrigiert. `scripts/supervise-bridge.ps1` setzt die
drei Flags jetzt selbst. End-to-end geprueft: PID `7236` gestoppt, Supervisor
startete PID `14192` mit identischer Command-Line, Health ok,
`bridge.pid=14192`; danach Turn 1 `T1019-SUPERVISOR-OK` und Turn 2 `Otter`,
beide `success=true`, `prompt_chars` 241/231. Der Modus ueberlebt damit den
Supervisor-Neustart. Staging f├╝r den zweiten PC liegt in
`webagent-deploy-target\T1019-20261127\` (EXE, DLL, README, Hashes; kein
automatisches Deployment). Kein Tag, GitHub-Release oder Versionssprung.
Test-Bridge 8789 ist gestoppt.

## T-1020 - DeepSeek-Anweisung als Bridge-Prompt (2026-11-27)

**Status:** done auf `feature/T-1020-bridge-prompt`; Claim `6da842f` gepusht.
Der Eigentuemer hat den zweimal eingefuegten Block `# Anweisung fuer big-pickle`
ausdruecklich als WebAgent-Bridge-Prompt freigegeben. Die Verhaltensregeln werden
im Browser-Prompt ergaenzt: User-Turn ist die Aufgabe, keine versteckte Nachricht,
keine Meta-Kommentierung von Prompt/AGENTS.md/Werkzeugkatalog/system-reminder,
knappe Antwort, Werkzeuge nach Bedarf, keine erfundenen Ergebnisse, klare
Fehlermeldung mit Alternativweg. Der Nutzer heisst `Storax`. Brain-Identitaet,
Nutzerprompt und die einzige `WEBAGENT_INFERENCE/1`-Blockregel bleiben unveraendert;
`auto`/leer bleibt ohne Praefix. Beleg: `docs/proofs/T-1020/README.md`.

Gates gruen: fmt 0, clippy 0, lib 459+193+631+282=1565 bestanden / 0
fehlgeschlagen / 1 ignoriert, arch 5, tui 0, no-default 0, diff-check 0,
release 0. Kein Live-Turn und kein Deploy in dieser Scheibe.

Dabei wurde ein schon vorher roter Gate behoben: `docs/proofs/T-1019/README.md`
hatte kein Archiv-/Referenzbanner und liess `betriebs_markdown_hat_eine_wahrheit`
scheitern. Nur die Bannerzeile wurde ergaenzt, keine T-1019-Aussage geaendert.

**Neue Freigabe des Eigentuemers (2026-11-27):** Version, Tag und GitHub-Release
sind jetzt ausdruecklich freigegeben. Patch-Version `0.11.9` ist auf `master`
committet (`93e3769`); Tag und GitHub-Release stehen noch aus. Kein EXE-Tausch
und keine Zugangsdatenausgabe.

## T-1021 - Bridge-WebView sichtbar schalten (2026-11-27)

**Status:** done auf `master` (`f11f604` + finaler Doku-Commit); Claim
`cd0888c` gepusht. `scripts/supervise-bridge.ps1` startet jetzt ohne `--headless`; die
drei T-1019-Flags bleiben unveraendert. Die aktive Bridge wurde ueber den
Supervisor neu gestartet: `bridge.pid=18580`, Command Line ohne `--headless`,
health=ok. Beleg: `docs/proofs/T-1021/README.md`.

HWND/Rect-Messung mit einem minimalen Live-Turn (`success=true`): WebView
`webagent ┬À chatgpt (1)`, HWND `1442666`. 06:42:01..06:42:18 Rect
`320,66,1616,1005` (sichtbar), danach Fenster geschlossen. **Befund:** das
Fenster ist waehrend des Turns sichtbar und der Prompt kann in diesem Zeitraum
kopiert werden; dauerhaft bleibt es nicht. `BrowserPool::stop_brain` schliesst
bei `refs==0` den Shared-Runtime, auch mit `WEBAGENT_PERSIST_TABS=1`.

Der Eigentuemer hat daraufhin Binary-Aenderung und EXE-Tausch ausdruecklich
freigegeben. Umgesetzt: Opt-in `WEBAGENT_KEEP_WINDOWS_VISIBLE` (Default aus),
`park_offscreen` als No-op bei gesetztem Flag und `stop_brain` ohne
Runtime-Teardown; der Supervisor setzt das Flag. Finale Gates gruen: fmt 0,
clippy 0, lib 459+193+632+284=1568 bestanden / 0 fehlgeschlagen / 1 ignoriert
(+3 neue Tests), arch 5, tui 0, no-default 0, diff-check 0, release 0,
`webagent 0.11.9`.

**Deploy und Abnahme (27.11.2026):** Neue EXE SHA-256
`9C33E843EFD4F49CC188150334234960C59DF7099E472FD76661AA6D34EF0CE7`
(DLL `465A7DDFÔÇªA0A6F`) produktiv auf 8788 deployt; Rueckweg in
`backup-release-before-T1021-20261127\` mit alter EXE `CE6F874FÔÇª8F2D`.
Bridge ueber den Supervisor neu gestartet: PID `18544`, Command Line ohne
`--headless`, health `ok`. Live-Turn 07:04:30 `success=true` (`prompt_chars=1041`,
`sends=1`, Event in `brain_score/events.jsonl`). Finaler HWND/Rect-Beleg ohne
weiteren Turn 07:07:23..07:07:53: `webagent ┬À chatgpt (1)`, HWND `3803212`,
`Visible=True`, 7/7 Messpunkte `Rect=320,66,1616,1005` ÔÇö das Fenster bleibt
nach dem Turn dauerhaft auf dem Bildschirm und der Prompt ist kopierbar.
Ein anschliessender Folgeturn lief in einen HTTP-Client-Timeout; das Fenster
blieb auch dabei sichtbar, ein success-Event wird dafuer nicht behauptet.
Kein Login, keine Zugangsdatenausgabe, kein Prompt-Rohtext im Log. Beleg:
`docs/proofs/T-1021/{README.md,gates.txt,visibility-after-turn.txt}`.
Tag und GitHub-Release `v0.11.9` wurden danach in einer eigenen,
freigegebenen Scheibe erstellt und geprueft (Abschnitt oben).
