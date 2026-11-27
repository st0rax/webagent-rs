# Audit-Fixes 2026-11-27 — Isolation, Denylist, UI-Härtung, Doku

**Referenzstatus:** Branch `fix/audit-isolation-and-denylist`; der laufende
Stand steht in `docs/CURRENT_WORK.md`. Erste Scheibe `0216eb3` ist gepusht,
diese Folgescheibe wird nach den Gates committet.
**Datum:** 2026-11-27
**Branch:** `fix/audit-isolation-and-denylist` (Basis `origin/master` `6e4ca4f`)
**Eigentümer:** local/opencode (Audit auf ausdrückliches „los jetzt“)

## Anlass

Projektprüfung gegen `master` `6e4ca4f`. Die P1-Befunde wurden praktisch
reproduziert und behoben; der `/api/*`-Befund wurde nach ausdrücklicher
Entscheidung des Eigentümers mit Host/Origin-Prüfung (statt Token-Plumbing)
gehärtet. Dazu kommt ein rein redaktioneller Doku-Abgleich. Keine
Versionsänderung, kein Tag, kein Release, kein Deployment.

## Befund 1 — Testläufe schrieben in Produktivdaten

**Beleg vor dem Fix:** `cargo test -p webagent-api --lib relay` erhöhte die
Zeilen mit `"brain_id":"mock…"` in
`%LOCALAPPDATA%\webagent\data\brain_score\events.jsonl` und setzte den echten
`circuit_breaker`-Eintrag `mock` auf 122 Fehlschläge / `open_until`.

**Ursache:** `config::paths::is_test_run()` erkannte nur Testbinaries mit dem
Präfix `webagent-` oder Elternordner `deps`. Auf dieser Werkbank liegen die
Testbinaries unter
`target/debug/build/<crate>/<hash>/out/<target>-<16 Hex>.exe`; bei Untercrates
lautet der Name `webagent_api-…`, `webagent_core-…`, `arch_layers-…`.
`cfg!(test)` ist in der Dependency-Crate ebenfalls false.

**Fix:** `is_cargo_test_binary_name()` erkennt den 16-Hex-Suffix unabhängig vom
Präfix. Die produktive `webagent.exe` hat keinen Hex-Suffix und bleibt damit
Nicht-Testlauf. `WEBAGENT_FORCE_REAL_DATA=1` überschreibt weiterhin.

**Beleg nach dem Fix:** gleicher Testlauf, `prod_mock` 430 → 430,
`test_mock` 1311 → 1318 (Zählung der `mock`-Events vor/nach dem Lauf).

## Befund 2 — Shell-Denylist mit sechs bestätigten Umgehungen

Mit einem temporären Test gegen `shell_policy::evaluate` (danach gelöscht)
waren erlaubt: `rm -fr /`, `rm -rf --no-preserve-root /`,
`bash -c "rm -rf /"`, `cmd /c rd /s /q C:\Windows\Temp`, `format.com C:`,
`del /s /q C:\*.*`.

**Fix in `crates/webagent-core/src/shell_policy.rs`:**

- `rm_root_denial()`: rekursives `rm` plus absolutes Pfad-, Home- oder
  Wildcard-Ziel — unabhängig von Flag-Reihenfolge und -Gruppierung.
- `del_mass_denial()`: `/s` und `*.*` in jeder Reihenfolge.
- `wrapped_command_denial()`: `bash/sh/zsh/dash/cmd/wsl/powershell/pwsh`
  prüft das eingebettete Kommando rekursiv (Tiefe maximal 3).
- `format`-Regex akzeptiert jetzt auch `format.com` / `format.exe`.

`PFLICHT_DENY` wurde nicht verändert; `pflicht_denys_bleiben` bleibt grün.
Neue Regressionstests decken die zwölf Umgehungsvarianten und sieben
harmlose Kontrollbefehle ab.

## Befund 3 — `/api/*` ohne Herkunftsprüfung

**Beleg vor dem Fix:** `GET /api/sessions` antwortete ohne Token mit HTTP 200;
dieselbe Kette bedient auch `POST .../chat`, `/quelle` und `/groups/.../run`.
`/v1/*` war dagegen korrekt mit Bearer-Token geschützt.

**Entscheidung des Eigentümers:** Origin/Host-Prüfung, kein Token für die UI.
Damit bleiben vorhandene curl-/CLI-Aufrufe (`Host: 127.0.0.1`) funktionsfähig;
ein Token-Plumbing hätte `webagent show/hide` und Skripte gebrochen.

**Fix:** `src/web_ui.rs` prüft vor dem `/api/*`-Dispatch den Host auf Loopback,
einen vorhandenen Origin auf dieselbe Loopback-UI und denselben Port sowie
`Sec-Fetch-Site` auf `none`/`same-origin`/`same-site`. Bei Verstoß: HTTP 403,
kein Dispatch. `HttpRequest::header()` in `crates/webagent-api/src/api_bridge.rs`
macht die normalisierten Header lesbar, ohne die Map offenzulegen. `/v1/*`
behält seinen Bearer-Schutz; `/health` bleibt bewusst offen.

**Regressionstests:** lokaler Origin/Host → 200; fremder Origin,
Rebinding-Host und `cross-site` → 403; fremder Origin bei `POST /api/sessions`
→ 403 ohne `run_id`; direkte Port-/Host-Pruefungen inklusive IPv6.

## Doku-Abgleich

`README.md` (v0.11.9 statt v0.11.4, Deployment-Fakten), `docs/RELEASE.md`
(v0.11.9 veröffentlicht, lokale EXE ≠ CI-EXE), `docs/OVERVIEW.md`
(27.11., `6e4ca4f`, 127 Tasks), `docs/PRODUCTION_READINESS.md`
(als historischer v0.11.7-Beleg markiert), `docs/SECURITY.md`
(Crate-Pfade statt `src/...`) und `docs/TASKBOARD.json` (`as_of`) wurden
gegen den verifizierten Stand abgeglichen. Keine neue Freigabe behauptet.

## Weitere P3-Fixes (Folgescheibe, gleicher Branch)

- **HTTPS-Body-Limit:** `crates/webagent-api/src/https_client.rs` liest jetzt
  maximal `MAX_RESPONSE_BYTES = 8 MiB` (ein Byte mehr zur
  Ueberschreitungserkennung) und lehnt groessere Antworten ab. Der alte
  30-Sekunden-Timeout begrenzte nur die Zeit, nicht den Speicher. Neuer
  Grenzwert-Test: genau 8 MiB ok, 8 MiB + 1 Fehler.
- **CI-Advisory-Gate:** `.github/workflows/ci.yml` hat einen eigenen Job
  `audit` mit `cargo install cargo-audit --locked` und `cargo audit`. Der Job
  ist eigenstaendig, damit ein Advisory sichtbar blockiert und den Build-Job
  nicht durch eine Tool-Installation verzoegert.
- **`CURRENT_WORK.md`:** Eintraege bis 25.11.2026 (66 Abschnitte, 1616 Zeilen)
  unveraendert nach `docs/archive/CURRENT_WORK-2026-11-25.md` verschoben;
  oben steht ein Zeiger. operative Wahrheit bleibt die aktuelle Datei. Der
  Startup-Guard (`src/startup.rs`) verbietet ein Archiv-Banner in
  `docs/CURRENT_WORK.md` und verlangt es im Archiv — nach dem Split hatte der
  erste Lauf genau hier einen Testfehler; beide Seiten sind korrigiert.

## Abhaengigkeits-Advisories

`cargo audit` (v0.23.0, Advisory-DB 1571 Eintraege) meldete im ersten Lauf
8 Vulnerabilities: `cookie` 0.18.1 (RUSTSEC-2026-0594), `rustls` 0.23.43
(RUSTSEC-2026-0285, -0346, -0348, -0349), `rustls-webpki` 0.103.15
(RUSTSEC-2026-0382), `url` 2.5.8 (RUSTSEC-2026-0592) und `webpki-roots`
1.0.9 (RUSTSEC-2026-0381). `Cargo.lock` wurde patchweise angehoben auf
cookie 0.18.3, rustls 0.23.46, rustls-webpki 0.103.16, url 2.5.9 und
webpki-roots 1.0.10 (form_urlencoded 1.2.3 zog mit). Zweiter Lauf:
**0 Vulnerabilities**, 11 allowed warnings. Die Warnungen betreffen
unmaintained/unsound/yanked Crates ohne Patch oder nur mit Major-Upgrade
(`glib`, `lru`, `rand`, `rustix`, `tokio`, `fxhash`, `instant`, `paste`,
`proc-macro-error`, `clipboard-win`); sie sind bewusst nicht Teil dieser
Scheibe.

## Gates (Endstand, 2026-11-27)

Alle Exit-Codes 0: `cargo fmt --all -- --check`, `git diff --check`,
`cargo clippy --features webview --all-targets --locked -- -D warnings`,
`cargo test --workspace --lib --locked` (463 + 194 + 636 + 284 = **1577
passed**, 0 failed, 1 ignored), `cargo test --test arch_layers --locked`
(5/5), `cargo check --features tui --locked`,
`cargo check --no-default-features --locked`,
`cargo build --release --locked`, `cargo audit` (0 Vulnerabilities, 11
allowed warnings). Log:
`C:\Users\storax\AppData\Local\Temp\opencode\audit-fix-gates.log`; die
Zwischenläufe mit fmt-/Clippy-/Testfehlern sind durch diesen Lauf ersetzt.
Zielgruppe der Regressionstests: 2 `paths`, 2 `shell_policy`, 4 `web_ui`.

## Nicht Teil dieser Scheibe

`Turn-Reveal fehlgeschlagen … Tab 2 existiert nicht` und die 10-%-Toleranz im
Composer-Vergleich. Der Eigentuemer hat beide Fixes freigegeben; sie folgen
als eigene Scheibe. Ein Tag/Release/Deployment wurde nicht ausgelöst.
