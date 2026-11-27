> Fruehere Eintraege (bis 25.11.2026): `docs/archive/CURRENT_WORK-2026-11-25.md`.

## 2026-11-27 - v0.11.10 in Arbeit: Merge, Tag, Release, Deployment

- **Owner/Freigabe:** local/opencode auf
  `fix/audit-isolation-and-denylist` (`53768e3`). Der Eigentuemer hat am
  27.11.2026 „immer sofort" fuer Merge, Tag, GitHub-Release und Deployment
  erteilt; es wird nicht erneut nachgefragt. Keine Force-Pushes, keine
  History-Umschreibung, keine Zugangsdaten, keine Live-Logins.
- **Umfang:** die drei Audit-Scheiben `0216eb3`, `b7ff548` und `53768e3`
  (Test-Isolation, Denylist, `/api/*`-Origin-Guard, HTTPS-Limit,
  `cargo audit`-CI, Lockfile-Patches, Archiv, Reveal-Tab, Composer-Puffer).
- **Versionsbump:** `0.11.9` → `0.11.10` in Root-`Cargo.toml`, den drei
  Crate-Manifesten und allen vier `webagent*`-Eintraegen in `Cargo.lock`;
  `CHANGELOG.md` und `docs/RELEASE.md` nachgezogen.
- **Stand:** Gate-Relauf, Commit/Push, Fast-Forward-Merge, Tag, Release und
  lokales Deployment mit Backup/Rollback laufen in dieser Scheibe; Ergebnisse
  werden hier nachgetragen, bevor die Scheibe als done gilt.

## 2026-11-27 - Audit-Schlussscheibe: Reveal-Tab und Composer-Toleranz

- **Owner/Freigabe:** local/opencode, gleicher Branch
  `fix/audit-isolation-and-denylist`, auf `b7ff548`. Der Eigentuemer hat beide
  Eingriffe ausdruecklich freigegeben. Kein Tag, kein Release, kein
  Deployment, kein Merge, keine Versionsaenderung.
- **Reveal-Tab:** `crates/webagent-transport/src/webview_runtime.rs` hat jetzt
  die lesende Runtime-Abfrage `page_exists` (`RuntimeMessage::PageExists`).
  `browser_pool.rs` prueft vor der Wiederverwendung eines gecachten
  `PooledTab`, ob der Page-Slot noch lebt (`pooled_tab_is_live`); wenn nicht,
  wird der Eintrag verworfen und in `start_brain` neu geoeffnet. `reveal_brain`
  verwirft einen veralteten Eintrag ebenfalls und meldet benannt
  „Tab ist geschlossen; Brain neu starten" statt „Tab 2 existiert nicht".
  Dafuer `reveal_brain(&mut self)` und zwei angepasste Aufrufer
  (`src/web_ui_api.rs`, `crates/webagent-transport/src/browser/mod.rs`).
- **Composer:** `composer_length_reserve` misst den Unterschied zwischen Roh-
  und normalisiertem Text, geklemmt auf 16..256 Zeichen. Die alte pauschale
  10-%-Toleranz ist aus `long_text_holds`, `ensure_composer_full` und der
  Submit-Diagnose entfernt; alle drei Pfade nutzen denselben Puffer. Kopf und
  Ende werden weiter exakt verglichen.
- **Regressionstests:** ungültige/fehlgeschlagene Runtime-Antwort gilt nicht
  als live, veralteter Pool-Eintrag wird entfernt; gemessener Puffer, Luecke
  knapp jenseits der Grenze, 5-%-Luecke mit intakten Raendern sowie
  ContentCheck genau an der/reserve-plus-eins-Grenze.
- **Gates:** fmt 0, diffcheck 0, clippy 0, lib 1582 passed / 0 failed /
  1 ignored, arch 5/5, tui 0, no-default 0, release 0, cargo audit 0
  (Log `C:\Users\storax\AppData\Local\Temp\opencode\audit-fix-gates.log`).
- **Bekanntes Risiko:** eine fehlgeschlagene `page_exists`-Abfrage wird als
  veraltet gewertet und kann eine noch live Tab neu oeffnen (Verlust des
  laufenden Chat-Verlaufs). Das ist die vom Eigentuemer freigegebene
  Kurskorrektur gegenueber dem stillen Reveal-Fehler.

## 2026-11-27 - Audit-Folgescheibe: HTTPS-Limit, CI-Audit, Lockfile, Archiv

- **Owner/Pfad:** local/opencode, gleicher Branch
  `fix/audit-isolation-and-denylist`, auf `0216eb3`. Kein Tag, kein Release,
  kein Deployment, kein Merge, keine Versionsaenderung.
- **HTTPS-Limit:** `crates/webagent-api/src/https_client.rs` begrenzt manuelle
  Provider-Antworten auf 8 MiB (`take(MAX_RESPONSE_BYTES + 1)`, danach
  `check_response_size`). Der 30-Sekunden-Timeout begrenzte vorher nur die Zeit,
  nicht den Speicher. Neuer Test: genau 8 MiB ok, 8 MiB + 1 Fehler.
- **CI-Advisory-Gate:** `.github/workflows/ci.yml` hat einen eigenen Job
  `audit` (`cargo install cargo-audit --locked`, `cargo audit`). Erster Lauf:
  8 Vulnerabilities (cookie 0.18.1, rustls 0.23.43 x4, rustls-webpki 0.103.15,
  url 2.5.8, webpki-roots 1.0.9). `Cargo.lock` wurde patchweise angehoben auf
  cookie 0.18.3, rustls 0.23.46, rustls-webpki 0.103.16, url 2.5.9,
  webpki-roots 1.0.10 (plus form_urlencoded 1.2.3). Zweiter Lauf: 0
  Vulnerabilities, 11 allowed warnings (unmaintained/unsound/yanked ohne
  Patch bzw. nur mit Major-Upgrade — bewusst nicht in dieser Scheibe).
- **Doku:** 66 historische Abschnitte (1616 Zeilen) unveraendert nach
  `docs/archive/CURRENT_WORK-2026-11-25.md`. Der Startup-Guard verlangt in
  archivierten Markdowns ein `**Archiv**`-Banner und verbietet es im
  Betriebs-Dokument; beide Seiten sind entsprechend gesetzt, der Test
  `betriebs_markdown_hat_eine_wahrheit` ist gruen.
- **Gates:** fmt 0, diffcheck 0, clippy 0, lib 1577 passed / 0 failed /
  1 ignored, arch 5/5, tui 0, no-default 0, release 0, cargo audit 0
  (Log `C:\Users\storax\AppData\Local\Temp\opencode\audit-fix-gates.log`).
- **Freigaben:** Owner hat den Reveal-Tab-Fix und die Composer-Toleranzbegrenzung
  freigegeben; beide sind in der vorstehenden Scheibe umgesetzt.

## 2026-11-27 - Audit-Fixes: Isolation, Denylist, UI-Haertung, Doku

- **Owner/Pfad:** local/opencode, Worktree
  `C:\Users\storax\Documents\Standardprojekt\webagent-rs-audit-fixes`,
  Branch `fix/audit-isolation-and-denylist` auf Basis `origin/master` `6e4ca4f`.
  Der alte Worktree `webagent-rs-t1008` bleibt unberuehrt (27 Commits hinterher).
- **Zweck:** reproduzierte P1-Befunde der Projektpruefung beheben: (1)
  Untercrate-Testbinaries schrieben in `%LOCALAPPDATA%\webagent\data`
  (Leaderboard/Circuit-Breaker), (2) sechs Denylist-Umgehungen (`rm -fr /`,
  `bash -c …`, `cmd /c rd /s …`, `format.com`, `del /s …`), (3) `/api/*` ohne
  Herkunftspruefung. Der Eigentuemer hat fuer (3) ausdruecklich Host/Origin statt
  Token-Plumbing gewaehlt. Kein Tag, kein Release, kein Deployment, keine
  Versionsaenderung.
- **Stand:** done. Alle Gates gegen den Endstand gruen: fmt 0, diffcheck 0,
  clippy 0, lib 1576 passed / 0 failed / 1 ignored, arch 5/5, tui 0,
  no-default 0, release 0 (Log
  `C:\Users\storax\AppData\Local\Temp\opencode\audit-fix-gates.log`).
  Belege: `docs/proofs/audit-2026-11-27/README.md`. Kein Tag, kein Release,
  kein Deployment, kein Merge.
- **Nicht enthalten:** Reveal-Tab-Bug, HTTPS-Body-Limit, `cargo audit` in CI,
  10-%-Composer-Toleranz — bleiben bewusst offen.

## 2026-11-27 - v0.11.9 final: Tag und GitHub-Release verifiziert

- **Status:** done. Annotierter Tag `v0.11.9` auf `master` `87d0153`
  (Tag-Objekt `82cae8f`) gepusht; GitHub-Release
  `https://github.com/st0rax/webagent-rs/releases/tag/v0.11.9` ist public,
  kein Draft, kein Pre-Release. Release-Workflow `45950140641` success:
  verify 0, windows 0, linux 0, android 0, github-release 0.
- **Sieben Assets geprueft** (alle heruntergeladen und mit den `.sha256`-
  Dateien abgeglichen, 3/3 OK): `webagent-windows-x86_64.exe`
  (`4C01B44C…73D`), `.sha256`, `WebView2Loader.dll`
  (`465A7DDF…A0A6F`), `webagent-linux-x86_64` (`7A2FC4D0…1000`),
  `.sha256`, `webagent-aarch64-linux-android` (`FF39FDFC…AB7`), `.sha256`.
- **Unterschied bewusst dokumentiert:** das CI-Release-EXE hat eine andere
  Hash-Summe als die lokal deployte EXE (`9C33E843…0CE7`); beide stammen aus
  derselben Source (`f11f604`, danach nur Doku-Commit `87d0153`), aber aus
  unterschiedlichen Build-Umgebungen. Die `WebView2Loader.dll` ist identisch.
  Produktiv laeuft weiter die deployte EXE mit dauerhaft sichtbarem WebView
  (PID `18544`, HWND `3803212`, `Rect=320,66,1616,1005`).
- Kein Zugangsdatenauslesen, kein Prompt-Rohtext im Log; Tag/Release waren
  ausdruecklich freigegeben.

## 2026-11-27 - T-1018 final: Composer-Beweis kurz deployt und live belegt

- **Finaler Stand:** `master` `23ac4e9` (gepusht), Release-Build
  `webagent 0.11.8 (23ac4e9b3)`, EXE-SHA-256
  `BBB9C714A0074C6AC975582BB337CB2720B1F189703419A7C5711ECBF35EDA3B`
  (DLL `465A7DDF…A0A6F`). Aktiv als PID `9008` mit identischer
  Command-Line, Health `ok`, `/v1/models` 200 mit Header / 401 ohne.
  Backup/Rückweg: `backup-release-before-T1018-20261127\`.
- **Fix:** `composer_holds_text` verlangt jetzt auch bei Prompts ≤ 1.000
  Zeichen einen echten Composer-Treffer. Vorher lieferte die Kurzschaltung
  ohne Vergleich `true`, womit ein leerer Editor als „gefüllt" und ein
  nicht konsumierter Prompt als „gesendet" gelten konnte. Die 10-%-Toleranz
  für lange Prompts bleibt unverändert.
- **Live-Abnahme `chatgpt`, 12 Tools:** Kurzprompt → HTTP 200, exakt
  `T1018-SHORT-OK`, 20,6 s, `success=true`, `prompt_chars=2677`.
  85.000 Zeichen Request (Event 87.643) → 0,102 s, 502 `Maximum 83000`,
  `events_delta=0`. 80.000 Zeichen Realtext (Event 82.643) → HTTP 200,
  exakt `T1018-REAL80-OK`, 35,1 s, `success=true`, `sends=1`.
- **Gates:** fmt 0; clippy 0; lib 459+187+630+282 = 1558 passed / 0 failed /
  1 ignored; arch 5 passed; tui 0; no-default 0; diff-check 0.
  Beleg: `docs/proofs/T-1018/{README.md,gates.txt}`.
- Für den zweiten PC liegt die finale EXE im lokalen Staging-Ordner
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\T1018-20261127\`
  (EXE, DLL, README, Hashes geprüft). Kein automatisches Deployment dort.
- Kein Nutzerprompt-Text gekürzt, kein Login, kein Tag/Release/
  Versionssprung; `WEBAGENT_API_KEY` nur als Header verwendet.

## 2026-11-27 - T-1017 final: Guard 83k deployt und live abgenommen

- **Finaler Stand:** `master` `039a00d` (gepusht), Release-Build
  `webagent 0.11.8 (039a00daf)`, EXE-SHA-256
  `B990DA8B219036DD4C6BD5692E709DC3CC67F15E6969763A64E888F9F1F8FB93`
  (DLL `465A7DDF…A0A6F`). Aktiv als PID `1144` mit identischer
  Command-Line, Health `ok`, `/v1/models` 200 mit Header / 401 ohne.
  Backup/Rückweg:
  `backup-release-before-T1017-guard83k-20261127\`.
- **Kalibrierung:** Ein `tools=12`-Turn mit 132.906 Zeichen wurde gesendet,
  blieb aber 351 s ohne Antwort; ohne Tools bei 130.215 Zeichen 353 s.
  Synthetischer Fülltext antwortete bei 102.743 Zeichen in 33,3 s, Realtext
  gleicher Länge nicht. Realtext 82.643 Zeichen → `success=true` (44,3 s);
  87.643 und 92.643 Zeichen → 341 s / 340 s ohne Antwort. Deshalb
  `MAX_BROWSER_PROMPT_CHARS = 83_000` (zwischen 82.643 und 87.643).
- **Live-Abnahme `chatgpt`, 12 Tools:** 85.000 Zeichen Request (Event
  87.643) → 0,121 s, HTTP 502 mit `Maximum 83000`, `events_delta=0`, kein
  Browserlauf. 80.000 Zeichen Realtext (Event 82.643) → HTTP 200, exakt
  `T1017-GUARD83-REAL80-OK`, 48,3 s, `success=true`, `sends=1`,
  `finish_reason=stop`.
- **Gates finale Scheibe:** fünfte Serie grün: fmt 0; clippy 0; lib
  459+187+630+280 = 1556 passed / 0 failed / 1 ignored; browser_inference
  32 passed; arch 5 passed; tui 0; no-default 0; diff-check 0.
- **Ehrliche Grenze:** Kein 100%-PASS für beliebige große Prompts. Belegt
  ist der sichere Bereich bis 83.000 Zeichen finaler Promptlänge für den
  getesteten Realtext; darüber schneller benannter Fehler statt 5–6 Minuten
  Brain-Sperre. Kein Nutzerprompt-Text gekürzt, kein Login, kein
  Tag/Release/Versionssprung; `WEBAGENT_API_KEY` nur als Header verwendet.
- Für den zweiten PC liegt die finale EXE im lokalen Staging-Ordner
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\T1017-guard83k-20261127\`
  (EXE, DLL, README, Hashes geprüft). Kein automatisches Deployment dort.
- Beleg: `docs/proofs/T-1017/{README.md,gates.txt}`. Die Abschnitte
  „Build/Deploy noch offen" weiter unten sind historische Zwischenstaende.

## 2026-11-27 - T-1017 final deployt und live abgenommen (Stand)

- **Finaler Stand:** `master` `e326001` (gepusht), Release-Build
  `webagent 0.11.8 (e326001d3)`, EXE-SHA-256
  `59B94B061B686999E8A392D0FF4B589876D60D88AF1B42F055905EC73AC2046B`
  (DLL `465A7DDF…A0A6F`). Aktiv als PID `14960` mit identischer
  Command-Line, Health `ok`, `/v1/models` 200 mit Header / 401 ohne.
  Backup/Rueckweg:
  `backup-release-before-T1017-descriptions-20261127\`.
- **Live-Abnahme `chatgpt`:** 12 Tools + 99.935 Zeichen Kontext → HTTP 200,
  Antwort exakt `T1017-MEDIUM-OK`, 39,7 s; Event `success=true`,
  `sends=1`, `prompt_chars=102806`. Kleiner Turn mit 12 Tools → HTTP 200,
  exakt `T1017-DESC0-OK`, 22,6 s, `prompt_chars=2917`. Damit ist die
  Längengrenze fuer den bisherigen ~196k-Fehler behoben; die externen
  `tools=12`-Turns landen jetzt bei ~134k statt ~196k.
- **Grenze ehrlich — nicht 100 %:** 134.851 Zeichen wurden gesendet
  (`sends=1`, kein `ABSENDEKNOPF_DEAKTIVIERT`), aber ohne Antwort. Ein
  Realtext-Turn mit 132.906 Zeichen (Dokumentation statt Fülltext) wurde
  ebenfalls gesendet und lief 351 s in
  `timeout_no_text: renderer may be unresponsive after wake,
  generation_complete=false, raw_chars=0` — kein Antwort-PASS. Danach war
  ein 12-Tool-Kurzturn wieder grün (`T1017-POSTBIG-OK`, 20,4 s,
  `success=true`). Ueber 140.000 Zeichen scheitert der Turn in 0,15 s mit
  benanntem Budgetfehler. Die Längenablehnung ist behoben, die Antwort bei
  ~132k dagegen nicht garantiert. Kein PASS ohne `success=true`-Event
  behauptet.
- **Code-Entscheid:** Tool-Beschreibungen werden komplett entfernt
  (`TOOL_DESCRIPTION_LIMITS=[0]`), Namen/Parameter/Typen bleiben; damit
  schrumpft das 12-Tool-Schema von ~9 KiB auf ~2,4 KiB. Kein
  Nutzerprompt-Text gekürzt, kein Login, keine Zugangsdatenausgabe, kein
  Tag/Release/Versionssprung. Board 123/123 `done`.
- Für den zweiten PC liegt dieselbe EXE zusätzlich im lokalen Staging-Ordner
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\T1017-20261127\`
  (`webagent.exe`, DLL, README.txt, Hashes geprueft). Kein Tag/Release und
  kein automatisches Deployment auf dem zweiten PC.
- Beleg: `docs/proofs/T-1017/{README.md,gates.txt}`. Die Abschnitte
  „Build/Deploy noch offen" weiter unten sind historische Zwischenstaende.

## 2026-11-27 - T-1017 live kalibriert: echte UI-Grenze ~140k statt 200k

- Der erste T-1017-Build (`03dbe14be`, EXE `D4D54039…5B1`) wurde mit
  Backup `backup-release-before-T1017-20261126\` und identischer
  Command-Line aktiviert (PID 5688, Health ok, 9 Modelle).
- **PASS:** `chatgpt`, `tools=12`, HTTP 200, Antwort exakt
  `T1017-LIVE-OK`, 29,2 s; Event 23:44:52 `+01:00` `success=true`,
  `sends=1`, `prompt_chars=27432`.
- **Guard-PASS:** 192.439 Zeichen → 0,13 s, benannter Fehler
  `Prompt zu gross fuer die Browser-Oberflaeche … (Maximum 180000)`,
  kein Browserlauf, kein Breaker-Eintrag.
- **Kalibrierung:** 159.505 und 147.459 Zeichen scheitern weiterhin an
  `ABSENDEKNOPF_DEAKTIVIERT` (je ~90 s); 127.459 und 138.561 Zeichen
  werden gesendet (`sends=1`), liefern aber auf synthetische Füllprompts
  keine Antwort innerhalb 200 s / 260 s. Die alte Annahme „200.000
  Zeichen gehen" ist falsch.
- **Anpassung:** `MAX_TOOL_SCHEMA_BYTES` 32 KiB → 12 KiB,
  `TOOL_DESCRIPTION_LIMITS = [256, 0]`, `MAX_BROWSER_PROMPT_CHARS`
  180.000 → 140.000. Kein Nutzerprompt-Text gekürzt. Gates erneut grün
  (fmt/clippy/lib/arch/tui/no-default/diff-check 0; 1556 passed).
  Danach auf `[0]` verschärft: mit `[256, 0]` blieb das 12-Tool-Schema bei
  ~9 KiB (Event `prompt_chars=9073`) und der externe ~132k-Prompt hätte
  140k überschritten; `[0]` bringt das Schema auf ~2,4 KiB.
  Beleg: `docs/proofs/T-1017/{README.md,gates.txt}`.
- Breaker wurde für die Messungen mehrfach mit Backup zurueckgesetzt
  (`state-before-T1017-live-20261126.json`, `-size-120k`, `-size-100k`,
  `-size-132k`). Kein Login, keine Zugangsdatenausgabe, kein Tag/Release.
- **Offen:** angepassten Build erneut bauen/deployen und verifizieren.

## 2026-11-26 - T-1017 auf master (Build/Deploy noch offen)

- Branch `bd0b350` (opencode-Identität) gepusht und per Fast-Forward nach
  `master` `bd0b350` gemergt; `master` gepusht. Board 123/123 `done`,
  Spiegel nachgezogen. Der finale Release-Build und der EXE-Tausch stehen
  noch aus; die aktive EXE bleibt bis dahin `D96F4999…EEAB` (PID 16972).

## 2026-11-26 - T-1017 umgesetzt (Prompt-Budget; Deploy/Live noch offen)

- **Anlass:** Auf „kümmer dich drum" hin wurde die `chatgpt`-Längengrenze
  bearbeitet, ohne Nutzerprompt-Text zu kürzen. Die aktive EXE blieb dabei
  zunächst unverändert (`D96F4999…EEAB`, PID 16972, Health ok).
- **Änderung auf `fix/T-1017-browser-prompt-budget` (vorige Basis
  `e75c895`):** `MAX_TOOL_SCHEMA_BYTES` 64 KiB → 32 KiB; neu
  `MAX_BROWSER_PROMPT_CHARS = 180_000` mit `check_browser_prompt_budget`
  direkt vor dem Browserlauf. Namen/Parameter aller Tools bleiben erhalten;
  überlange Prompts scheitern benannt statt nach ~4 Minuten mit
  `ABSENDEKNOPF_DEAKTIVIERT`. Zwei Regressionstests neu.
- **Gates:** fmt 0; clippy 0; lib 459+187+630+280 = 1556 passed / 0 failed /
  1 ignored; arch 5 passed (`modules=92, edges=282, cycles=3,
  upward_edges=26, crate_violations=0`); tui 0; no-default 0;
  `git diff --check` 0. Beleg: `docs/proofs/T-1017/{README.md,gates.txt}`.
- **Offen:** Branch committen/pushen, nach `master` mergen, Release-Build,
  EXE-Tausch mit Backup und identischer Bridge-Command-Line, danach
  `chatgpt`-Turn mit `tools=12` belegen. Kein PASS ohne `success=true`-Event
  oder belegten, benannten Budgetfehler. Kein Tag/Release/Versionssprung,
  keine Zugangsdatenausgabe.
- **Betriebslage bei Beginn:** `chatgpt`-Breaker offen bis 23:44:38 Uhr
  lokal; externer `opencode-cli` (PID 4944) sendet weiter `tools=12`-Turns.

## 2026-11-26 - Live-PASS auf der aktiven T-1016-EXE

- `POST /v1/chat/completions` auf `webagent/gemini` (`stream=false`,
  `temperature=0`, `Antworte exakt mit T1016-LIVE-OK.`): **HTTP 200**,
  Antwort exakt `T1016-LIVE-OK`, Event 22:27:26 UTC mit `success=true`,
  `sends=1`, `pasted_chars=202/202`. Der erste Anlauf wurde im
  ContentCheck gekuerzt (167/202) und vom bestehenden Relay-Retry
  korrigiert. Damit ist der komplette Backend-Pfad auf der neuen Binary
  live belegt.
- `chatgpt` bleibt **nicht** live abgenommen: Ein externer, verbundener
  `opencode-cli.exe serve --service` schickt fortlaufend `tools=12`-Turns
  mit ~196.028 Zeichen; die Brain-Sperre serialisiert diese rund
  4 Minuten langen Laeufe, sodass kurze Kontroll-Turns verhungern
  (150 s / 420 s Client-Timeout, ein `client_disconnected`-Event mit
  `pasted_chars=1`). Die grossen Turns scheitern an der bekannten
  UI-Laengenablehnung `ABSENDEKNOPF_DEAKTIVIERT`, nicht an Selektoren.
  Kein Rueckfall von T-1016; kein PASS fuer `chatgpt` behauptet.
- Der `chatgpt`-Breaker wurde auf „MACH JETZT" zweimal mit Backup
  zurueckgesetzt (`state-before-T1016-live-20261126.json`,
  `state-before-T1016-isolated-20261126.json`). Health `ok`, PID `16972`,
  aktive EXE unveraendert `D96F4999…EEAB`.
- Zusatzbefund ohne Code-Aenderung: `composer_holds_text` gilt bei
  Prompts ≤ 1.000 Zeichen ungeprueft als erfolgreich
  (`crates/webagent-transport/src/browser/composer.rs:384-391`), was
  kurzen Turns den Absende-Beweis ueber den Composer nimmt. Dokumentiert
  im Deploy-Beleg; keine API-Semantik- oder Release-Aenderung ohne
  Freigabe.
- Beleg und Rueckweg:
  `docs/proofs/deploy-T1016-bridge-restart-2026-11-26/README.md`.

## 2026-11-26 - Abgleich: Bridge-Zweige und Doku-Stand

- `git branch --no-merged origin/master` listet viele Zweige, deren Arbeit
  **bereits auf `master` liegt** — sie wurden per Squash/PR oder Cherry-Pick
  uebernommen, nicht vergessen. Belege: `chore/T-974-bridge-autostart` → PR
  #114 (`c9050f3`), `chore/T-975-pi-full-coverage` → PR #115 (`93d33a3`),
  `chore/actions-node24` → PR #105 (`c1f01f0`),
  `fix/T-1014-gemini-prompt-input` → `a6c8b97`, `refactor/T-1000-…` →
  `52b41f8`. Die payload-Dateien sind bytegleich auf `master`. **`Nicht`
  erneut mergen und nicht loeschen** (Loeschen braucht Freigabe).
- Board-Spiegel `docs/TASKBOARD.md` hatte T-301 noch `claimed`, obwohl die JSON
  (Quelle der Wahrheit) `done` meldet; korrigiert. Living Docs `GOALS.md`,
  `docs/OVERVIEW.md`, `docs/RELEASE.md` und `docs/arch/CRATE_SPLIT.md` hatten
  veraltete Stand-/Release-Aussagen (93 Tasks, `v0.11.6/7`, „noch ein
  Paket"); auf 122 Tasks / `v0.11.8` / vier Crates aktualisiert bzw. als
  historisch markiert. Dated Log-Eintraege bleiben unveraendert Historie.
- Goal-Stack (`~/.agents/goal/stack.json`): G-010 und G-011 mit Belegen
  abgeschlossen; kein Tag und kein Release erzeugt (G-009 gilt weiter).
- Re-Gates nach dem Abgleich: fmt 0; clippy 0; lib 459+185+630+280 = 1554
  passed / 0 failed / 1 ignored; Architektur 5 passed (`modules=92,
  edges=282, cycles=3, upward_edges=26, crate_violations=0`); tui 0;
  `--no-default-features` 0; `git diff --check` 0.

## 2026-11-26 - T-1016-EXE aktiv deployt (auf Eigentuemer-Anweisung)

- **Anweisung:** Der Eigentuemer schrieb „kannst du bitte als
  projektverantwortlicher dran arbeiten ohne alle 5 minuten aufzuhlren" und
  „und ich warte immer noch auf ne neue binarsy". Das wurde als konkrete
  Anweisung gelesen, die neue Binary in den aktiven Betrieb zu bringen; die
  fruehere T-1016-Grenze „kein lokaler Restart" ist damit ersetzt.
- **Tausch:** Backup in
  `backup-release-before-T1016-20261126\` (alte T-1015-EXE
  `E35A7092…80C8`, DLL, SHA-Datei, Breaker-State; Hashes nach Kopieren
  geprueft). PID `2564` gestoppt, Port 8788 frei, neue aktive EXE
  `D96F4999…EEAB` (`webagent 0.11.8 (6ccec0e34)`), DLL `465A7DDF…A0A6F`.
  Neue PID `16972`, identische Command Line, Health `ok`, `/v1/models` 200
  mit Header / 401 ohne.
- **Damalige Live-Grenze (spaeter durch den Live-PASS-Abschnitt oben
  ergaenzt):** Ein kontrollierter Chat-Turn lief 150 s und wurde ohne
  eigenes `events.jsonl`-Event abgebrochen; parallel scheiterte eine
  `tools=12`-Anfrage mit `prompt_chars=196027` an
  `ABSENDEKNOPF_DEAKTIVIERT` (Event 21:53:36 UTC, `success=false`). Das
  ist eine UI-Sende-Fehlerklasse, kein Selektoren-Fehler. Details und
  Rueckweg: `docs/proofs/deploy-T1016-bridge-restart-2026-11-26/README.md`.
- **Grenze:** Kein Tag, Release, Versionssprung, Login oder
  Zugangsdatenausgabe (Key nur als Header verwendet).

## 2026-11-26 - T-1016 umgesetzt: Selektoren-Fallback bei I/O-Fehler

- **Auslöser:** Der Eigentuemer testete die Binary auf einem zweiten PC und
  meldete „backend fehler selektronre nicht ladbar das gerät ist nicht bereit
  os error 21". Die Meldung stammt aus `Browser::from_config` →
  `load_selectors` → `read_selector_file`: der Compile-Zeit-Pfad
  `CARGO_MANIFEST_DIR/selectors` kann auf einem fremden PC auf einem nicht
  bereiten Laufwerk liegen; `os error 21` war bisher ein harter Fehler,
  obwohl die Selektoren eingebettet in der Binary stecken.
- **Claim/Branch:** `fix/T-1016-selector-io-fallback` von `origin/master`
  (`1accbe3`), Board-Eintrag vor den Source-Edits. Beleg:
  `docs/proofs/T-1016/README.md`, `gates.txt`.
- **Geliefert:** `read_selector_file` behandelt jetzt jeden I/O-Fehler als
  „Datei nicht vorhanden" und warnt sichtbar mit Pfad und Fehler; kaputtes
  JSON bleibt `InvalidData`. Damit greift bei den mitgelieferten Dateien die
  eingebettete Kopie, bei der Nutzerdatei die Basis ohne Overlay. Zwei neue
  Regressionstests (I/O-Fehler, kaputtes JSON). Keine Aenderung an
  Merge-Semantik oder Selektor-Inhalten.
- **Finaler Build/Staging:** Nach dem Merge nach `master` (`6ccec0e34`)
  `cargo build --release --locked` 0, `--version` → `webagent 0.11.8
  (6ccec0e34)`, Exit 0. Die EXE liegt (zusammen mit `WebView2Loader.dll` und
  `README.txt`) nur im Staging-Ordner
  `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\T1016-20261126\`
  — SHA-256 `D96F4999693AE24F69DEE20A010483A55863B1CA8E813E32956E8781E573EEAB`
  (DLL `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F`).
  Die aktive lokale `release\`-EXE bleibt bei T-1015 (`E35A7092…`).
- **Damalige Grenze (durch den nachfolgenden Deploy-Abschnitt ersetzt):** Der
  lokale Bridge-Prozess (T-1015, PID `2564`) wurde zunaechst nicht angefasst;
  der spaetere Tausch erfolgte erst nach der oben dokumentierten
  Eigentuemer-Anweisung. Auf dem zweiten PC muss der Eigentuemer die Binary
  weiterhin selbst einsetzen. Kein Tag, Release, Versionssprung oder Login;
  keine Zugangsdatenausgabe.

## 2026-11-26 - T-1015 live deployt und verifiziert

- **Freigabe:** Der Eigentuemer schrieb „autonom auf fertigstellung hinarbeiten";
  auf die konkrete Frage nach EXE-Tausch und Neustart antwortete er
  „Ja, deployen + restarten". Danach wurden Backup, Tausch und Restart ausgefuehrt.
- **Build/Stand:** `cargo build --release --locked` 0 aus `573719d11`;
  EXE-SHA-256 `E35A709265019B66E8D1E18C6A2FDA7389A67C5DBB99B2BA8CAD4B40795780C8`,
  DLL unveraendert (`465A7DDF…A0A6F`). Architektur-Guard 5 passed
  (`modules=92, edges=282, crate_violations=0`), `git diff --check` 0.
- **Sicherung/Restart:** Neuer Rollback-Ordner
  `backup-release-before-T1015-20261126\` mit geprueften Hashes der bisherigen
  T-1014-EXE (`6CC16E34…E390B72B`), DLL, SHA-Datei und Breaker-Zustand; der
  aeltere T-1014-Ordner bleibt. PID `4484` gestoppt, Port frei, neue PID `2564`
  mit identischer Command Line, Health `ok`. `/v1/models`: neun Modelle,
  Gemini `12000/4000`, ohne Header 401.
- **Live-Belege:** Turn 1 (43.386 ms, HTTP 200) lieferte genau `chatgpt bereit.`
  und die Logzeilen `[T-1015] Begruessungs-Stub erkannt` sowie
  `[T-1015] Retry erschoepft … kein PASS` — genau ein Frisch-Chat-Retry, kein
  falscher Erfolg. Turn 2 (15.072 ms, HTTP 200) lieferte
  `finish_reason=tool_calls` mit `read_file {"path":"docs/API_BRIDGE.md"}` und
  keinen `model_output_envelope_invalid`; beide Events `success=true`, `sends=1`.
  Der zweite Rohtext wird nicht persistiert — die Trailer-Klasse ist zusaetzlich
  per Unit-Test belegt.
- **Grenze:** Kein Login, keine Zugangsdatenausgabe (Header nur verwendet),
  kein Tag, Release, Versionssprung oder neues oeffentliches Artefakt.
  Beleg/Rueckweg: `docs/proofs/deploy-T1015-bridge-restart-2026-11-26/README.md`.

## 2026-11-26 - T-1015 umgesetzt: Begruessungs-Stubs und Umschlag-Trailer

- **Auslöser/Auswertung:** Der Eigentuemer verlangte „session opencode „greeting"
  auswerten fixes implementiern". Die Session `ses_de6ca134effeoxqH4z2uBVf4yx`
  (Titel `Greeting`) zeigte auf Auftraege mit `tools=12` wiederholt nur
  `chatgpt`, `Hallo.` oder `chatgpt bereit.` als Endantwort; genau ein
  `write`-Call erzeugte `C:\Users\storax\Desktop\notizbuch.md` (4.011 Byte,
  SHA-256 `39B99C3BC37FD4D6D58F4E18990168E8B6642A097E86AE556C5F2D94A9F3BFA1`).
  Um 21:25:39 UTC scheiterte ein vollständiger Umschlag an `\n``` ` danach
  (`trailing characters at line 2 column 1`, `model_output_envelope_invalid`).
- **Claim/Branch:** `fix/T-1015-greeting-stub-retry` von `origin/master`
  (`4dc5f8c`), Board-Eintrag vor den Source-Edits angelegt. Belege:
  `docs/proofs/T-1015/README.md` und `gates.txt` (mit den Rohbeobachtungen).
- **Geliefert:** `is_greeting_only_response` in `webagent-core` erkennt nur die
  ganze Antwort als Stub (eng begrenzt, Protokoll und mehrsaetzige Antworten
  ausgenommen). Bei aktiven Client-Tools und vorhandenem Ursprungsauftrag gibt
  es genau EINEN Frisch-Chat-Retry mit diesem Auftrag plus kurzem Hinweis;
  ein wiederholter Stub wird ehrlich zurueckgegeben. `envelope_json_slice`
  begrenzt die Parser-Eingabe auf das balancierte JSON-Objekt; Trailer aus
  Markdown/Prosa sind erlaubt, eine fehlende Klammer bleibt fail-closed.
  `docs/API_BRIDGE.md` dokumentiert das Verhalten.
- **Gates auf dem finalen Baum:** fmt 0; clippy 0; workspace lib 459 root +
  185 api + 628 core + 280 transport = 1552 passed / 0 failed / 1 ignored;
  `cargo check --features tui` 0; `--no-default-features` 0; `git diff --check` 0.
  Vier neue Regressionstests. Ein erster Clippy-Lauf mit `-q` hinter `--`
  scheiterte nur am Flag, ein Workspace-Lauf an einem transienten
  `ConnectionReset` im Web-UI-Test; beide beim Rerun gruen.
- **Integration:** Implementierungscommit `573719d` auf
  `origin/fix/T-1015-greeting-stub-retry` gepusht, Merge-Commit `7b61afc` auf
  `master` gepusht. Der gemergte Baum ist inhaltlich identisch mit dem
  geprüften Branch (`git diff 573719d 7b61afc` leer); kein Tag, Release oder
  Versionssprung.
- **Grenze:** Die aktive Bridge laeuft weiter mit der T-1014-EXE (`84ba99fb5`),
  PID `4484`, Port `8788`, Health `ok`. Kein EXE-Tausch, kein Neustart, kein
  Live-Provider-Turn, kein Login, keine Zugangsdatenausgabe, kein Tag,
  GitHub-Release, Versionssprung oder Deployment. Der Rollback-Ordner
  `backup-release-T1014-20261126` bleibt unveraendert. Ein T-1015-Restart
  braucht eine neue ausdrueckliche Freigabe.

## 2026-11-26 - T-1014 live am laufenden Bridge verifiziert

- **Auslöser:** Der Eigentuemer schrieb „MACH ENDLICH", nachdem ich den Live-Test zurückgehalten hatte. Damit ist der kontrollierte Provider-Turn freigegeben; ich habe ihn gegen die bereits laufende Bridge PID `4484` ausgeführt.
- **Gemini:** 2.041-Zeichen-Prompt → HTTP 200, Antwort exakt `T1014-GEMINI-OK`, 18,5 s, Event `pasted_chars=2210/2210`, `sends=1`, `success=true`. Der fruehere Zeichen-fuer-Zeichen-Fallback und die 69–667 s Laufzeiten sind nicht mehr aufgetreten.
- **ChatGPT:** 48.041-Zeichen-Prompt → HTTP 200, Antwort `chatgpt  T1014-LANG-OK`, 35,5 s, Event `pasted_chars=48216/48213`, `sends=1`, `success=true`, `phase=settle`. Der alte `content_check`-Timeout bei ~45,6k Zeichen ist damit ebenfalls behoben.
- **Over-Cap:** 25.000 Zeichen an Gemini → HTTP 502 `prompt_zu_lang` in 177 ms, Event mit `latency_ms=0` und ohne `turn`-Objekt, kein Browserstart, Breaker unverändert. Kein stilles Kuerzen.
- **Weiteres Monitoring:** Elf echte Turns zwischen 19:22 und 19:35 UTC (zehn `chatgpt`, einer `gemini`) alle `success=true`, `sends=1`. Die `T-961`-Frisch-Chat-Retrys im Log sind dokumentiertes, einmaliges Verhalten eines abgeschlossenen Tasks, kein Defekt; `Retry erschoepft` kam nicht vor. `/v1/models` ohne Token weiter 401, Health `ok`, PID `4484` und Port `8788` stabil.
- **Logs:** `bridge.err.restart-20261126.log` zeigt nur die bekannten `not_isolatable`-Hinweise und den bewusst ausgeloesten Over-Cap-Abbruch; kein Composer-Timeout, kein Browserabsturz. Kein Login, keine Zugangsdatenausgabe, kein Tag/Release.
- **Beleg:** `docs/proofs/T-1014/live-verification.md`.

## 2026-11-26 - Bridge mit T-1014-Binary neu gestartet

- **Freigabe:** Der Eigentuemer hat nach dem Hinweis, dass EXE-Tausch und Neustart Freigabe brauchen, „start die bridge bitte nue" gesagt. Alter PID `15908` wurde gestoppt, die aktive EXE ersetzt und die Bridge mit identischer Command Line als PID `4484` neu gestartet.
- **Aktiv:** `...\webagent-deploy-target\release\webagent.exe` hat jetzt SHA-256 `6CC16E34E9B2E58E45E81E9BDB80F08EB0A916FB2BC9E32F4395CA58E390B72B` und meldet `webagent 0.11.8 (84ba99fb5)`; DLL-Hash unveraendert. Port `127.0.0.1:8788` hoert auf PID `4484`, Health liefert `status=ok`. `/v1/models` meldet neun Modelle und fuer Gemini `context_window=12000`, `max_tokens=4000` — der T-1014-Katalog ist damit live.
- **Sicherung/Rollback:** `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\backup-release-T1014-20261126\` enthaelt die bisherige EXE (Hash geprueft), DLL, SHA-Datei und den Breaker-Zustand vor dem Eingriff. Der noch offene `chatgpt`-Breaker-Eintrag wurde mit Sicherung entfernt; alle anderen Eintraege sind abgelaufen.
- **Grenze dieses Schritts:** Beim Restart selbst gab es noch keinen Provider-Turn, kein Login, keine WebView-Oeffnung und keine Zugangsdatenausgabe; kein Tag, Release oder Versionssprung. Die zwei spaeteren Live-Turns sind im Abschnitt oben dokumentiert. Neue Logs: `bridge.err.restart-20261126.log` / `bridge.out.restart-20261126.log`. Beleg: `docs/proofs/deploy-T1014-bridge-restart-2026-11-26/README.md`.

## 2026-11-26 - T-1014 implemented: Gemini prompt cap, fail-fast and tolerant composer proof

- **Branch/claim:** `fix/T-1014-gemini-prompt-input` from `c44b166`, claim commit `bbd95da`, owner `local/opencode`. T-1014 is `done` in `docs/TASKBOARD.json` and the mirror. Proof: `docs/proofs/T-1014/README.md` and `gates.txt`.
- **Integration:** merged into `master` as `a6c8b97` and pushed; `origin/master` is in sync. No tag, GitHub release or deployment was created or moved.
- **Befund:** Gemini live turns with 38.561–150.325 Zeichen ran 69–667 s with `pasted_chars=0`, `sends=0`; successful turns topped out at 26.523 Zeichen. `send_gemini` fell back to `type_text_char_by_char` — one PageDriver round-trip per character. A chatgpt turn at 18:09 UTC showed 45.399/45.567 Zeichen pasted but failed the strict content check, so the tolerant proof is now used for fill, consumed check and send verification too.
- **Delivered:** `webagent-core` owns a measured Gemini prompt cap of 24.000 chars plus per-brain catalog limits of 12.000/4.000 tokens, all env-overridable. `relay_turn` rejects over-limit prompts before breaker check, backend creation and browser start with `prompt_zu_lang` and records a failed event; it never truncates. `/v1/models` advertises the usable Gemini capacity so Pi compacts earlier. Long prompts no longer use the per-character fallback (only <=256 chars keep it); `composer_holds_text` accepts head, tail and a 10-% length band only when the strict Rich-Editor comparison fails, and rejects truncation or wrong edges. `docs/API_BRIDGE.md` documents the per-brain limits.
- **Gates on the final source tree:** fmt 0; clippy 0; workspace lib 466 root + 182 api + 627 core + 280 transport = 1555 passed / 0 failed / 1 ignored; arch 5 passed with `modules=92, edges=281, cycles=3, upward_edges=26, crate_violations=0`; all-targets 0 (plus 7 bin-unit and 5 integration); no-default 0 (transport 274); release build 0 with `WebView2Loader.dll` and `webagent 0.11.8 (bbd95da76+dirty)` before the commit; `git diff --check` 0.
- **Live-Grenze:** Bridge PID `15908` still runs the deployed `v0.11.8` EXE from `webagent-deploy-target\release\`. Breaker state still shows open cooldowns for chatgpt, gemini, claude, deepseek and kimi; that is why further turns are skipped. A fresh release binary exists only in `target\release\` and was **not** deployed. No credentials, login, live provider turn or browser session was used. Stopping PID `15908`, replacing the active EXE and restarting needs explicit owner approval; afterwards `events.jsonl`, `bridge.err.log` and the breaker state must be monitored.

## 2026-11-26 - Phase 16 crate-extraction chain merged into master; board closed

- **Integration:** local `master` was fast-forwarded from `93d33a3` to released `origin/master` `fb7b886`, then the four reviewed branches were merged in order with explicit merge commits: `9227572` (T-1010), `83f6516` (T-1011), `7bed096` (T-1012), `c1771dbf4` (T-1013). All merges were conflict-free; working tree clean before the board update. No force-push, no history rewrite, no branch deleted.
- **Board:** T-1010..T-1013 are `done` with `done_at=2026-11-26` in `docs/TASKBOARD.json` and the `docs/TASKBOARD.md` mirror. Each keeps its claim, branch and proof path; no proof file was removed. Phase 16 now has no `claimed` task.
- **Gates on merged master:** fmt 0; clippy 0; arch 5 passed with `modules=92, edges=280, cycles=3, upward_edges=26, crate_violations=0` and `api → transport` still rejected; workspace lib 466 root + 181 api + 625 core + 277 transport = 1549 passed / 0 failed / 1 ignored; all-targets adds 7 bin-unit + 5 integration; no-default 466 + 181 + 625 + 271 passed / 1 ignored; offline check 0; debug and release builds 0 with `WebView2Loader.dll`, `--version`/`--help` 0 and a clean `webagent 0.11.8 (c1771dbf4)`; relay bad-brain smoke exit 2 with the real provider installed; `git diff --check` 0.
- **Release reconciliation:** annotated tag `v0.11.8` points at the released commit `fb7b886`, the exact base of this chain. GitHub release `v0.11.8` is published (not draft/prerelease, 2026-11-26T13:33:49Z) with seven assets (Windows EXE + DLL, Linux and Android binaries and three `.sha256` files). It was not moved, deleted or replaced; the merged extraction is a post-release refactor and all crate versions remain `0.11.8`.
- **Deployment reconciliation (facts):** `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\v0.11.8\webagent.exe` reports `webagent 0.11.8 (fb7b88607)`; the active `...\webagent-deploy-target\release\webagent.exe` still reports `webagent 0.11.7 (93d33a3f2+dirty)`. Rollback folders `v0.11.5-backup`, `v0.11.6-backup` and `v0.11.7-backup` exist. No deployment file was changed by the integration. Whether to promote the staged `v0.11.8` assets into the active folder and/or publish a new version for the extracted code remains an explicit owner decision.
- **Deployment preflight (2026-11-26):** the published Windows asset was downloaded into `C:\Users\storax\AppData\Local\Temp\opencode\v0118-assets\` and its SHA-256 matches the release `.sha256` (`a58d319482e8cb652a692f854d2dbad8514e20fb80d5f8669ad406c30f09719f`). The staged `v0.11.8\webagent.exe` is a **different local build** (`f5970f7d4237864ffe8aa3125b645c7c76c4204dbbc5b0953232df82d4045f0e`) that reports `fb7b88607`. The active `release\webagent.exe` is **in use**: PID `17372`, listening on `127.0.0.1:8788`. No deployment file was touched; activating v0.11.8 would require stopping that process and has not been done.
- **Owner decision (2026-11-26):** activate the published v0.11.8 assets with a rollback backup; keep `v0.11.8` as the delivery version — no version bump, no new tag and no new GitHub release for the crate extraction merged after the release.
- **Deployment finished:** PID `17372` was stopped and port 8788 verified free; the three active files were backed up to `...\webagent-deploy-target\v0.11.7-active-20261126-backup\` with hashes verified. The downloaded **published** assets were activated in `...\webagent-deploy-target\release\`: EXE SHA-256 `a58d319482e8cb652a692f854d2dbad8514e20fb80d5f8669ad406c30f09719f` (matches the release `.sha256`), DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`. Deployed `--version` → `webagent 0.11.8 (fb7b88607)`, exit 0; `--help` exit 0. Proof and rollback: `docs/proofs/deploy-v0.11.8-2026-11-26/README.md`.
- **Grenze / offen:** local deployment only; the running bridge was stopped and intentionally **not** restarted, so no live provider/browser session was started. No credentials were read, no tag or GitHub release was created or moved, and no new public artifact was published for the extracted code. Rollback is the verified `v0.11.7-active-20261126-backup\` folder.

## 2026-11-26 - T-1013 implemented: api modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1013-api-extraction`, claim commit `4fc4e1e`, owner `local/opencode`. T-1013 was claimed in board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1013/README.md`.
- **Delivered:** 23 source files (nine top-level modules) moved with `git mv` into `crates/webagent-api/src/`: `api_bridge` (+12 children), `browser_inference`, `comms`, `free_cloud_chat`, `https_client`, `oobe`, `relay`, `session` (+2 children), `source_scope`. Root re-exports `webagent_api::*`, so all existing paths stay valid. `crate::<core>` was rewritten to explicit `webagent_core::` paths in 14 files; compile-driven `pub(crate)` → `pub` widenings in `api_bridge` only. The t404 SDK test now resolves `../../scripts/t404` because its manifest moved. New core module `backend_factory` stores `fn` pointers (the trait is not dyn-compatible) and is installed once by the binary as `install::<webagent::WebBackendFactory>()`, removing the last `api → transport` blocker without a new dependency or exception.
- **Guard/docs:** `layers.toml` places `backend_factory` at L0 rank 1; rank 0 was rejected by the guard (`new upward edge backend_factory (rank 0) -> brain (rank 1)`). `CRATE_SPLIT.md` §4.3 and `layer_exceptions.txt` record the measured state; no exception line was added or removed. Guard `modules=92, edges=280, cycles=3, upward_edges=26, crate_violations=0`; `api → transport` still rejected.
- **Gates:** fmt 0; clippy 0; arch 5 passed; workspace lib tests 466 root + 181 api + 625 core + 277 transport = 1549 passed / 0 failed / 1 ignored (claim-time 1547, +2 new provider tests); all-targets adds 7 bin-unit + 5 integration; no-default 0; offline check 0; debug/release builds 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; relay bad-brain smoke exit 2 in both profiles, proving the provider is installed without a live call; `git diff --check` 0.
- **Grenze / offen:** no merge, tag, release or deployment, no credentials, provider call, browser login or live brain/image session. Extraction chain complete; final integration into `master`, task completion and release reconciliation are a separate approved step.

## 2026-11-26 - T-1013 claimed: api extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror carry T-1013 as `claimed`, owner `local/opencode`, branch `refactor/T-1013-api-extraction`, 2026-11-26. It branches from the pushed T-1012 implementation commit `1910fec` and stays `claimed` until merged. No source file was moved before this claim.
- **Claim-time baseline:** workspace lib tests 647 root + 623 core + 277 transport = 1547 passed / 0 failed / 1 ignored; guard `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0; debug and release smoke 0. Known blocker: `relay.rs` calls the concrete `WebBackendFactory` (transport), which api must not reference. It needs a core-side factory provider installed by the binary.

## 2026-11-26 - T-1012 implemented: transport modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1012-transport-extraction`, claim commit `3c4388d`, owner `local/opencode`. T-1012 was claimed in board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1012/README.md`.
- **Delivered:** 22 source files (ten top-level modules) moved with `git mv` into `crates/webagent-transport/src/`: `browser` (11 files), `brain_probe`, `browser_pool`, `login`, `mock_page`, `page_driver`, `panel_sink`, `upload_spa_js`, `webview_reveal`, `webview_runtime` (plus its two child files). Root re-exports `webagent_transport::*`, so all existing paths stay valid. `crate::<core>` was rewritten to explicit `webagent_core::` paths in 13 files; `WebBackendFactory` and its `BackendFactory` impl moved together to satisfy the orphan rule. `panel_sink` geometry helpers are exposed under a new `test-support` feature used by root dev-dependencies. No runtime behavior, dependency version or `[profile.*]` change.
- **Guard/docs:** `CRATE_SPLIT.md` §4.2 and `layer_exceptions.txt` record the measured state; `layers.toml` and the exception lines needed no edit. Guard still `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; `api → transport` still rejected.
- **Gates:** fmt 0; clippy 0; arch 5 passed; workspace lib tests 647 root + 623 core + 277 transport = 1547 passed / 0 failed / 1 ignored (identical to claim-time total); all-targets adds 7 bin-unit + 5 integration; no-default 647 + 623 + 271 passed / 1 ignored; offline check 0; debug/release builds 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; `git diff --check` 0.
- **Grenze / offen:** no api module moved, no merge, tag, release or deployment, no credentials, provider call or browser login. Next: T-1013 api extraction from this branch.

## 2026-11-26 - T-1012 claimed: transport extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror carry T-1012 as `claimed`, owner `local/opencode`, branch `refactor/T-1012-transport-extraction`, 2026-11-26. It branches from the pushed T-1011 implementation commit `2635336` and stays `claimed` until merged. No source file was moved before this claim.
- **Claim-time baseline:** workspace lib tests 924 root + 623 core = 1547 passed / 0 failed / 1 ignored; guard `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0; debug and release smoke 0. Known blocker to resolve first: `panel_sink` is a root module used by transport candidates, and the concrete `WebBackendFactory` marker must move with its `BackendFactory` impl.

## 2026-11-26 - T-1011 implemented: core modules extracted (branch pushed)

- **Branch/claim:** `refactor/T-1011-core-extraction`, claim commit `6e421e6`, owner `local/opencode`. T-1011 was claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. Proof: `docs/proofs/T-1011/README.md`.
- **Delivered:** 34 platform-neutral modules and the root `lib.rs` helper block moved into `crates/webagent-core` (helpers in new `support` module). Root re-exports the crate with `pub use webagent_core::*`; no public path changed. `counting` deliberately stays in root because it calls `WebBackendFactory::create_brain_backend` (impl in root browser); the factory marker moved to root `browser/backend.rs` to satisfy the orphan rule. Selector JSON assets moved with `config/selectors.rs`. Compile-driven `pub(crate)` widenings and three source-path test fixes only; no runtime behavior change.
- **Guard/docs:** `layers.toml` gained `support` at rank 0; `layer_exceptions.txt` records `modules=91, edges=278, cycles=3, upward_edges=26, crate_violations=0` with no exception line changed; `CRATE_SPLIT.md` records the actual extraction and the `counting` deviation.
- **Gates:** fmt 0; `cargo clippy --workspace --features webview --all-targets -- -D warnings` 0; arch 5 passed; unit tests 924 root + 623 core = 1547 passed / 0 failed / 1 ignored (identical to the claim-time total); all-targets 924 lib + 7 bin-unit + 5 integration + 623 core lib; no-default 918 + 7 + 5 + 623 passed / 1 ignored; debug build and `--version`/`--help` exit 0; release build 0 with `--version`/`--help` exit 0 and `WebView2Loader.dll` present; `git diff --check` 0. A late path-semantics fix (`root_dir()`/`selectors_dir()` and a benchmark test plan path) was applied during final verification; all gates were then re-run green on the final tree, including the release rebuild.
- **Grenze / offen:** no merge into master, no new tag/release/deployment, no credentials, live provider call or browser login. Next: T-1012 transport extraction from this branch.

## 2026-11-26 - T-1011 claimed: core extraction (no source edits yet)

- **Claim:** `docs/TASKBOARD.json` and mirror now carry T-1011 as `claimed`, owner `local/opencode`, branch `refactor/T-1011-core-extraction`, 2026-11-26. It builds directly on the caught-up T-1010 branch (`cb1e51b`) and stays `claimed` until merged. No module was moved before this claim.
- **Claim-time baseline:** lib 1547 passed / 0 failed / 1 ignored; guard `modules=90, edges=277, cycles=3, upward_edges=26, crate_violations=0`; fmt/clippy 0. The extraction must preserve these counts (modulo moved-module test relocation) and re-export the existing public API from the root crate.

## 2026-11-26 - T-1010 branch caught up with released v0.11.8 master

- **Lagebild:** Another session merged phase 16 through T-1009 into `origin/master` and published tag `v0.11.8` (`fb7b886`), explicitly leaving crate extraction to this session. `refactor/T-1010-workspace-guard` was based on the pre-release `92011a5`; `origin/master` was merged into it as a normal merge commit (no force-push, no history rewrite).
- **Konfliktauflösung:** Board conflicts were resolved in favour of the released board: T-997..T-1009 stay `done`, T-976/T-977 remain present, T-1010 stays `claimed`. `Cargo.toml`/`Cargo.lock` kept released version `0.11.8` and gained the workspace skeleton. `docs/TASKBOARD.json` parses with 116 tasks; no conflict markers remain.
- **Re-gates:** fmt 0; clippy 0; arch 5 passed with `modules=90, edges=277, cycles=3, upward_edges=26, crate_violations=0` (two new edges from released Play/T-976/T-977 code, no new exception); lib 1547 passed / 0 failed / 1 ignored (was 1531 before the master merge); all-targets 1547 lib + 7 bin-unit + 5 integration; no-default 1541 lib + 7 + 5; `git diff --check` 0. The T-1010 proof records both measurement points.
- **Grenze / offen:** no crate extracted yet; this branch is still not merged into master. No new tag, release, deployment, credentials, live provider call or browser login. Next: T-1011 core extraction from this caught-up branch.

## 2026-11-26 - Produktstand ausgeliefert: Play-Code, Phase 16, Release-Vorbereitung

- **Auftrag:** fertig machen. Das heisst den vorhandenen Produktstand auf `master` bringen und ausliefern, nicht die naechste Architekturbaustelle oeffnen.
- **Enthalten:** Phase 16 bis T-1009, Play-Kommando (`src/repl/play.rs`), sichtbare API-Turns (T-976), zentrale Modellgrenzen und Timeouts (T-977). `relay` deckt geparkte WebViews vor dem Turn auf (`relay_reveal_for_turn`).
- **Nicht geoeffnet:** Play-v0.3-Spec-Tasks T-978..T-996 (kein Code, 0 Tests). Crate-Extraktion T-1010 bleibt in der anderen Session und ist nicht Teil dieser Auslieferung.
- **Release:** `Cargo.toml` und `Cargo.lock` stehen auf `0.11.8`. Tag `v0.11.8` wird nach dem Versionsgate auf diesen Commit gesetzt. Kein Produktiv-Deployment.

## 2026-11-26 - Phase 16 landed on master through T-1008

- **Freigabe:** Der Eigentuemer hat die offene Arbeit aus der Session-Auswertung
  bestaetigt: Release-Absturz schliessen und die ungemergte Phase-16-Kette nach
  `master` legen. Kein Tag, kein GitHub-Release, kein Deployment, keine
  Crate-Extraktion, kein Live-Lauf.
- **Merge:** Fast-Forward `origin/master` `93d33a3` → `fa7c37d`
  (`fix/T-1008-webview-loader-smoke`). 29 Commits, T-997 bis T-1008.
- **Absturz, selbst geprueft:** Exit `-1073741515` ist `0xC0000135`
  (`STATUS_DLL_NOT_FOUND`), nicht `0xC0000005`. Frischer Worktree auf `fa7c37d`,
  `cargo build --release --locked` Exit 0 in 11m 12s, `WebView2Loader.dll`
  (165336 Bytes) lag ohne manuelles Skript neben der EXE.
  `target/release/webagent.exe --version` → `webagent 0.11.7 (fa7c37dda)`, Exit 0.
  `--help` Exit 0, 54 Zeilen. Kein Login, kein Browserfenster.
- **Board:** T-997..T-1008 nach dem Merge auf `done` gesetzt (`done_at`
  2026-11-26).

## 2026-11-26 - T-1010 implemented: workspace skeleton and multi-crate guard

- **Branch/claim:** `refactor/T-1010-workspace-guard`, claim commit `142f964`, owner `local/opencode`. T-1010 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This slice adds only the workspace skeleton and guard coverage; no production module moved.
- **Delivered:** root `Cargo.toml` gains `[workspace]` with `members = [".", "crates/webagent-core", "crates/webagent-transport", "crates/webagent-api"]`, `resolver = "2"`, `default-members = ["."]`; one shared `Cargo.lock`/`target/`. Three empty library members exist with their own manifests and no `[profile.*]`. All `[profile.*]` values and dependency versions are unchanged; two stale `0xC0000005` comments were corrected to the measured `0xC0000135` (T-1008). Proof: `docs/proofs/T-1010/README.md`.
- **Guard upgrade:** `tests/arch_layers.rs` now discovers modules from every workspace `lib.rs`, records each module's owning crate, counts `crate::` plus explicit `webagent_core::`/`webagent_transport::`/`webagent_api::` paths, and enforces an allowed crate-pair matrix (core has no library outgoing edge; transport and api are siblings reaching only core; only root reaches all three). The conceptual layer/cycle/upward/stale-exception checks are unchanged. A new negative test rejects `api → transport`. `docs/arch/layer_exceptions.txt` records `crate_violations=0`; no exception line changed.
- **Gates measured:** `cargo metadata --no-deps` lists all four packages. Claim lib 1531 passed / 0 failed / 1 ignored; final lib 1531 / 0 / 1 (52.06 s), unchanged. `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 5 passed with `modules=90, edges=275, cycles=3, upward_edges=26, crate_violations=0`; `cargo test --all-targets --quiet` 1531 lib + 7 bin-unit + 5 integration; `cargo test --locked --no-default-features --quiet` 1525 lib + 7 bin-unit + 5 integration; debug/release build and `--version`/`--help` smoke exit 0; `git diff --check` 0. The root path deps are intentionally not added until T-1011 so clippy does not report unused dependencies.
- **Grenze / offen:** no module moved, no dependency version changed, no merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`. Next slice is T-1011: move the core modules into `webagent-core` with root re-exports, add the path dep, and keep the guard and all gates green.

## 2026-11-26 - T-1009 implemented: ImageBackend boundary for relay

- **Branch/claim:** `refactor/T-1009-image-backend`, claim commit `0ede5dc`, owner `local/opencode`. Trait-Grenze, keine Crate-Extraktion.
- **Delivered:** `ImageGenerationBackend` plus `BackendFactory::create_image_backend` in `brain.rs`; `WebBrainBackend` implementiert das Trait; `relay_image_generation` geht ueber die Factory und `relay_image_turn`. Lifecycle, Traces, Timeouts und Fehlerstrings unveraendert. Proof: `docs/proofs/T-1009/README.md`.
- **Nachgemessen auf dem Merge nach `6969da3`:** `rg crate::browser src/relay.rs` leer. `cargo test --lib image_turn_uses_the_core_trait_lifecycle` 1 passed. `cargo test --test arch_layers` 4 passed, `modules=90, edges=275, cycles=3, upward_edges=26`. `taskboard_invarianten_halten` 1 passed. `git diff --check` 0.
- **Merge:** `origin/refactor/T-1009-image-backend` (`92011a5`) nach `master`. T-1009 danach `done`. Kein Tag, kein Release, keine Crate-Extraktion. T-1010 bleibt bei der anderen Session. Play-Mode-Branch `fix/T-976-api-turn-reveal` ist nicht enthalten.

## 2026-11-26 - T-1008 implemented: local runnable WebView2 build

- **Branch/claim:** `fix/T-1008-webview-loader-smoke`, claim commit `2d64df9`, owner `local/opencode`. T-1008 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This is a local build fix, not crate extraction.
- **Diagnosis correction:** every phase-16 note that reads exit `-1073741515` as `0xC0000005` was wrong. `-1073741515` is `0xC0000135` (`STATUS_DLL_NOT_FOUND`); `gdb` showed `During startup program exited with code 0xc0000135.` with no stack, i.e. the loader failed before `main` because `WebView2Loader.dll` was missing beside the EXE. The correction is recorded in the proofs for T-1004/T-1006/T-1007, `docs/arch/CRATE_SPLIT.md`, `docs/arch/CLI_GROUPS.md` and `docs/proofs/T-1008/README.md`.
- **Delivered:** the existing root `build.rs` now copies `WebView2Loader.dll` from `target/<profile>/build/webview2-com-sys[-<hash>]/out/x64` to `target/<profile>/` for Windows + `webview` builds. It handles both the Cargo 1.100 `build/webagent/<hash>/out` layout and the older `build/webagent-<hash>/out` layout by locating the profile directory through the sys-crate build folder. `scripts/copy-webview2-loader.ps1` stays as a documented manual fallback and for CI. No `Cargo.toml`, `[profile.*]`, dependency or workflow change.
- **Smoke proven:** with the DLL deleted first, `cargo build --locked` and `cargo build --release --locked` each recreated it; debug and release `--version` and `--help` all exit 0. Browser-free `cargo build --no-default-features --locked` also exits 0 and emits no missing-DLL warning.
- **Gates measured:** claim-time lib 1530 passed / 0 failed / 1 ignored (55.72 s); final lib 1530 / 0 / 1 (49.43 s), unchanged. `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed with `modules=90, edges=276, cycles=3, upward_edges=26`; `cargo test --all-targets --quiet` 1530 lib + 7 bin-unit + 4 integration; `git diff --check` 0.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`. The owner has approved the remaining work; next slice is the image-backend trait for `relay → WebBrainBackend`, then crate extraction and merges. Proof: `docs/proofs/T-1008/README.md`.

## 2026-11-26 - T-1007 implemented: PanelSink boundary for transport

- **Branch/claim:** `refactor/T-1007-panel-sink`, claim commit `6fec681`, owner `local/opencode`. T-1007 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. It is follow-up 3 from `docs/arch/CRATE_SPLIT.md`, not crate extraction.
- **Delivered:** new dependency-free `src/panel_sink.rs` owns `Rect`, the pure grid/reveal geometry and the read-only `PanelSink` trait. `brain_grid` keeps the old re-exports and implements `DesktopPanelSink`; `main.rs` installs it before dispatch. `browser_pool`, `webview_reveal` and `webview_runtime` now reference `panel_sink` only; no `crate::brain_grid` remains in those three modules. Parking, docking and focus behavior is unchanged. Proof: `docs/proofs/T-1007/README.md`.
- **Documented limit:** without installation (library users and unit tests), `panel_sink()` returns `NoPanelSink` and `reveal_rect()` uses the 80/80/1280×900 fallback. The binary installs the desktop sink, so production geometry is unchanged.
- **Guard delta:** deleted `browser_pool→brain_grid`, `webview_reveal→brain_grid` and `webview_runtime→brain_grid` from the ratchet after the guard proved them stale; added `panel_sink` to L0. Measured 90 modules / 276 edges / 3 cycles / 26 upward edges (was 89 / 275 / 3 / 29). The `brain_probe,browser,browser_pool` cycle is unchanged; no exception was added.
- **Tests re-measured:** claim 1528 total / 1527 passed / 0 failed / 1 ignored (50.71 s); final 1531 total / 1530 passed / 0 failed / 1 ignored (52.40 s). Three new unit tests in `panel_sink`. Gates green: `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1530 lib + 7 bin-unit + 4 integration; `cargo test --locked --no-default-features --quiet` final 1524 lib + 7 bin-unit + 4 integration; `git diff --check` 0.
- **Flake, not hidden:** two parallel browser-free runs failed intermittently with `ConnectionReset` in the untouched `web_ui.rs` one-shot helper (two different tests). Isolated retry, full serial run and a third parallel run passed; the claim commit passed in parallel in a clean worktree. No regression from the changed modules was reproducible.
- **Binary smoke blocked, not a regression:** `cargo build` succeeded, but default `webview` `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time. `Cargo.toml`, `[profile.*]` and dependencies were not touched.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`; crate extraction still needs fresh explicit owner approval. Next owner choice: review/push this branch, or approve the image-backend follow-up explicitly.

## 2026-11-26 - T-1006 implemented: public core traits and shared inference types

- **Branch/claim:** `refactor/T-1006-core-traits`, claim commit `9bf1841`, implementation commit `7f87e5c`, owner `local/opencode`. T-1006 was created and claimed in the board/mirror before source edits and stays `claimed`; no merge, no `done`, no PR. This is F1 from the T-1004 follow-up list, not crate extraction.
- **Delivered:** new dependency-free `src/inference_types.rs` owns the shared inference/tool types, `ClientPresence`/`AlwaysConnected` and `WebSearchObservation`; old paths keep working via re-exports. `brain.rs` publishes `RelayBackend`, `BackendFactory`, `PageProbe` and `WebBackendFactory`; the browser module implements them. `relay_single_turn_checked`, `counting` and `contract` now use the new boundaries. Proof: `docs/proofs/T-1006/README.md`.
- **Correction to T-1004:** `PageProbe` alone does not remove `contract → browser` or `counting → browser`. `contract` used protocol helpers merely re-exported by `browser`; `counting` needed `BackendFactory`. `docs/arch/CRATE_SPLIT.md` now says so. The image-generation relay path still constructs `WebBrainBackend` directly and needs a separate image-backend trait before extraction.
- **Guard delta:** removed three stale `edge:` lines and replaced the six-module browser cycle with `brain_probe,browser,browser_pool`; no exception was added. `browser_inference`/`free_cloud_chat` moved to L3 and `contract` to L6 so remaining edges are downward. Measured 89 modules / 275 edges / 3 cycles / 29 upward edges (was 88 / 274 / 3 / 32).
- **Tests re-measured:** claim 1524 total / 1523 passed / 0 failed / 1 ignored (50.53 s); final 1528 total / 1527 passed / 0 failed / 1 ignored (50.20 s). Four new unit tests (three in `inference_types`, one `PageProbe` test in `brain`). All gates green: `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1527 lib + 7 bin-unit + 4 integration passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** default `webview` `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, identical to the T-1004 baseline. `Cargo.toml`, `[profile.*]` and dependencies were not touched.
- **Grenze / offen:** no crate extraction, merge, PR, tag, release, deployment, credentials, live provider call or browser login. All phase-16 branches remain `claimed`; crate extraction still needs a fresh explicit owner approval. Next owner choice: review/push this branch, or approve the next follow-up (PanelSink/image backend) explicitly.

## 2026-11-26 - T-1004 delivered: crate split decision (design only)

- **Branch/claim:** `docs/T-1004-crate-split`, claim commit `f27928b`, owner `local/opencode`. T-1004 stays `claimed`; no merge, no `done`, no PR. The owner phrase `mach fertig den senf` approved finishing the design document only, not crate extraction. Formal `depends_on` is satisfied in substance by T-998..T-1003, but those branches are also still unmerged.
- **Delivered:** `docs/arch/CRATE_SPLIT.md` with a re-measured baseline, the naive and recommended group graphs, per-crate module/Cargo trees, seven required trait boundaries, the workspace-root `[profile.*]` policy, nine ordered follow-up proposals and an explicit no-go list. Proof: `docs/proofs/T-1004/README.md`. No `src/`, Cargo, profile or test file was touched; no follow-up task ID was created.
- **Decision:** three libraries (`webagent-core`, `webagent-transport`, `webagent-api`) plus the existing root binary crate. Recommended grouping measured at 34 core / 9 transport / 9 api / 36 root modules. Six cross-crate blockers remain before extraction: `contract`/`counting → browser`, `browser → browser_inference`, three `brain_grid` hooks, and `relay → WebBrainBackend`. They need `PageProbe`, shared inference types, `PanelSink` and `BackendFactory`; `RelayBackend` already exists privately in `relay.rs` and should become a public core trait.
- **Baseline re-measured, not quoted:** `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (53.09 s); guard 88 modules / 274 edges / 3 cycles / 32 upward edges; clean lib test build+link 65.98 s with a 162,341,370-byte test executable; clean default binary build+link 40.47 s with a 150,514,926-byte `webagent.exe`; 21 direct Cargo dependencies. A scratch copy of the guard sanitizer under `%TEMP%\opencode\` produced the group matrices without touching repo files.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; startup markdown-banner test 1 passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** default `webview` `--help` still exits `-1073741515` (`0xC0000005`) before `main` at the claim commit. No binary-affecting change was made, and `[profile.*]` (`codegen-units = 16`) was not touched.
- **Grenze / offen:** no crate extraction, code change, credentials, live call, release, tag, deployment, PR or merge. Next extraction step requires a fresh, explicit owner approval; stop here.

## 2026-11-26 - T-1005 delivered: CLI grouping design (no code changes)

- **Branch/claim:** `docs/T-1005-cli-groups`, claim commit `afd11c9`, owner `local/opencode`. T-1005 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Delivered:** `docs/arch/CLI_GROUPS.md` with the verbatim before-help snapshot, 43-command inventory, proposal for `brain`/`run`/`check`/`api`/`dev` groups, illustrative after-help, default-help policy and an alias table keeping every old name callable. Proof: `docs/proofs/T-1005/README.md`. No `src/`, Cargo or test file was touched.
- **Snapshot source:** browser-free binary at `938bbf1` (`--no-default-features`), because the default `webview` binary crashes before `main` with `0xC0000005`; the command enum is not feature-gated, so help text is identical.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `git diff --check` 0; startup markdown-banner test 1 passed.
- **Grenze / offen:** design only, no command removed, no behavior change, no credentials, release, tag, deployment, PR or merge. Open for implementation: whether `serve` maps to `api` or `ui --api`, and whether hidden root aliases get a `help --all`. T-1004 remains free and still needs explicit owner approval; no further task was claimed.

## 2026-11-26 - T-1003 implemented: Windows GUI dependencies behind `webview`

- **Branch/claim:** `chore/T-1003-feature-hygiene`, claim commit `a76d79d`, owner `local/opencode`. The initial `build/` name was renamed before the claim because the board invariant and commit script only allow `feature|fix|docs|chore|refactor|test`. T-1003 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten".
- **Bestand zuerst geprüft:** `windows` 0.58 is used only behind `cfg(all(windows, feature = "webview"))` (JobObjects in `worker_pool.rs`, ToolHelp in `brain_grid.rs`/`brain_wall.rs`, console handle in `lib.rs`/`main.rs`), so the browser-free build does not need it. `windows-sys` 0.59 is used by non-GUI code (`config/profiles.rs`, `config/writeback.rs`) and stays non-optional. Proof: `docs/proofs/T-1003/README.md`.
- **Changes:** `Cargo.toml` only — `webview2-com`, `windows`, `arboard`, `clipboard-win`, `image` are optional and wired into the `webview` feature via `dep:`; `windows-sys` moved into the Windows target table. No `[profile.*]` line changed; `src/lib.rs` and `Cargo.lock` needed no edit.
- **Measured:** Windows `cargo tree --no-default-features -e normal --depth 1` went from five GUI crates plus `windows-sys` to core crates plus `windows-sys`; Linux tree has no `windows-sys`. After `cargo clean -p webagent`, lib test binary 95,086,256 → 94,481,201 bytes (−605,055); wall-clock build+link 46.69 s → 47.66 s (no improvement, within noise). Default binary size unchanged at 150,514,926 bytes; browser-free binary 82,625,882 bytes.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --all-targets --quiet` 1523 lib + 7 bin-unit + 4 integration passed; `cargo test --locked --no-default-features --quiet` 1517 lib + 7 bin-unit + 4 integration passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Binary smoke:** default `webview` binary still crashes before `main` with `0xC0000005` on `--help` and `diagnose --brain deepseek --headless` (pre-existing, blocked DoD item). Browser-free binary: `--help` exit 0; `diagnose --brain deepseek --headless` exit 1 with the intended `page-unavailable: WebView-Feature nicht aktiviert` message. Linux cross-check blocked: `x86_64-linux-gnu-gcc` is not installed; the Linux dependency tree was checked instead.
- **Grenze / offen:** no credentials, live provider call, release, tag, deployment, PR or merge. T-1004..T-1005 remain free; T-1004 still needs explicit owner approval — stop here.

## 2026-11-26 - T-1002 implemented: tui_load/tui_state cycle removed, legacy TUI cycle documented

- **Branch/claim:** `refactor/T-1002-tui-cycles`, claim commit `06f5f2d`, owner `local/opencode`. T-1002 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Bestand zuerst geprüft:** the only `tui_state -> tui_load` production edge was the `pub use crate::tui_load::load_state;` re-export, consumed solely by `tui.rs`. Removed it and imported `load_state` directly in `tui.rs`. The `bin_hooks,repl,tui,tui_ansi` cycle spans four modules and slash-command/`PersistentQueryPool`/`run_tui`/ANSI helpers; per the task's low-priority rule it stays with a written reason in `docs/arch/layer_exceptions.txt` (legacy TUI, no behavior gain, no feature work). Proof: `docs/proofs/T-1002/README.md`.
- **Guard delta:** 88 modules / 274 edges (unchanged: one edge moved from `tui_state` to `tui`) / 3 cycles (was 4) / 32 upward edges (unchanged). Removed exception `cycle:tui_load,tui_state`; the remaining cycle line now carries its justification comment.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time. `Cargo.toml` and `[profile.*]` untouched.
- **Grenze / offen:** no login, credentials, live provider call, TUI feature work, release, tag, deployment, PR or merge. T-1003..T-1005 remain free; T-1004 still needs explicit owner approval. Per the standing instruction, the next free slice (T-1003) may be claimed; stop before T-1004.

## 2026-11-26 - T-1001 implemented: capability <-> capability_proof cycle removed

- **Branch/claim:** `refactor/T-1001-capability-cycle`, claim commit `2de15c5`, owner `local/opencode`. T-1001 stays `claimed`; no merge, no `done`, no PR. Formal `depends_on: T-997` was overridden by the owner's standing "autonom weiterarbeiten"; T-997 stays `claimed`.
- **Bestand zuerst geprüft:** `src/capability_types.rs` did not exist; the leaf named in the board scope was created. `capability_proof` remains the single proof gate and store — no second gate. `ProofKind`, `Capability`, `CATALOG`, `capability()` and `capability_for_route()` moved verbatim into the dependency-free leaf; `capability.rs` re-exports them, so all public paths and callers stay identical. `capability_proof.rs` imports from `capability_types`; no `crate::capability` reference remains there. Proof: `docs/proofs/T-1001/README.md`.
- **Guard delta:** the `capability,capability_proof` cycle is gone. Measured graph: 88 modules (new leaf) / 274 edges (was 273) / 4 cycles (was 5) / 32 upward edges (unchanged). `docs/arch/layer_exceptions.txt` lost the cycle line; no `edge:` line changed because the three modules share L4.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (unchanged from claim baseline, no tests added); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Deviation:** `src/lib.rs` (module declaration) and `docs/arch/layers.toml` (one L4 entry) are outside the three-file scope but required for a new top-level module to exist and be ranked. No behavior change to CLI, protocol, API, shell policy or proof-file format. `Cargo.toml` and `[profile.*]` untouched.
- **Follow-up already published:** the claim-time baseline was red because the T-1000 proof lacked the mandatory reference banner. Fixed on `refactor/T-1000-config-brain-limits-edge` at `27b38af` (pushed) and cherry-picked here at `52b41f8`; baseline then measured green.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as at claim time.
- **Grenze / offen:** no login, credentials, live provider call, release, tag, deployment, PR or merge. T-1002..T-1005 remain free; T-1004 still needs explicit owner approval. Per the standing instruction, the next free slice (T-1002) may be claimed; stop before T-1004.

## 2026-11-26 - T-1000 implemented: config no longer reaches brain_limits

- **Branch/claim:** `refactor/T-1000-config-brain-limits-edge`, claim commit `3d038ea`, owner `local/opencode`. T-1000 stays `claimed`; no merge, no `done`, no PR. Formally `depends_on: T-999`, but the owner's "autonom weiterarbeiten" explicitly overrode that gate just as the earlier "mach" did; T-999 stays `claimed` because it is not merged.
- **Bestand zuerst geprüft:** `src/config/limits.rs:47` called `crate::brain_limits::accepted_chars` while `brain_limits` reads `config::data_dir`. The inversion uses a small `AcceptedChars` trait: `config::limits` takes the resolved value by parameter (`Option<usize>` implements it) and `brain_limits` implements it for `&str`, delegating to the existing measurement-store lookup. The unchanged caller in `controller.rs` keeps compiling. `git grep -n "crate::brain_limits" -- src/config` is empty. Proof: `docs/proofs/T-1000/README.md`.
- **Guard delta:** the `brain_limits,config` cycle is gone. Measured graph: 87 modules / 273 edges (was 274) / 5 cycles (was 6) / 32 upward edges (unchanged). `docs/arch/layer_exceptions.txt` lost the `cycle:brain_limits,config` line; no `edge:` line changed. The ratchet initially failed on the stale entry, which confirmed the removal before the exception was deleted.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1523 passed / 0 failed / 1 ignored (claim-time baseline 1521 / 0 / 1, two tests added); `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1523 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0.
- **Deviation:** `src/config/mod.rs` is outside the board's two-file scope. The one-line `AcceptedChars` re-export is required because `mod limits;` is private; duplicating store parsing in `config` would create a second reader of the measurement file. No behavior change to CLI, WEBAGENT/1 protocol, API or shell policy. `Cargo.toml` and all `[profile.*]` settings are untouched.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`, as measured at the claim-time baseline on the earlier phase-16 branches.
- **Grenze / offen:** no login, credentials, live provider call, release, tag, deployment, PR or merge. T-1001..T-1005 remain free; T-1004 still needs explicit owner approval and was not touched. Per the owner's standing "autonom weiterarbeiten", the next free slice (T-1001) may be claimed; stop before T-1004.

## 2026-11-26 - T-999 implemented: config no longer reaches worker_pool

- **Branch/claim:** `refactor/T-999-config-worker-pool-edge`, claim commit `928ea11`, implementation commit `d6a4b37`, owner `local/opencode`. T-999 stays `claimed`; no merge, no `done`, no PR. Both commits are pushed and the worktree is clean. T-997 and T-998 also stay `claimed`; the owner's "mach" again overrode the formal dependency.
- **Bestand zuerst geprüft:** `src/fsutil.rs` does not exist and is not declared in `src/lib.rs`. The required leaf already exists in `src/pool_state.rs` (`atomic_write`, `RETRY_UNAVAILABLE_AFTER_SECS`) and `worker_pool.rs` already re-exports it. No new module was created; `src/lib.rs` and `docs/arch/layers.toml` were not touched. `src/config/brains.rs` now calls `crate::pool_state::…` directly, so `crate::worker_pool` has no match under `src/config`.
- **Guard delta:** the cycle `brain_limits,circuit_breaker,config,pool_failover,worker_pool` shrank to `brain_limits,config`. Measured graph is unchanged at 87 modules / 274 edges / 6 cycles / 32 upward edges. `docs/arch/layer_exceptions.txt` holds the shorter cycle; no `edge:` line was added or removed. Proof: `docs/proofs/T-999/README.md`.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored; `cargo test --test arch_layers -- --nocapture` 4 passed; `cargo test --all-targets --quiet` 1521 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. Claim-time baseline was already 1521 / 0 / 1, so the test count is unchanged.
- **Binary smoke blocked, not a regression:** `--help` and `diagnose` still exit `-1073741515` (`0xC0000005`) before `main`; binary size 150,498,536 bytes, identical to the T-998 branch head. The same crash was measured at claim commit `103967b` in a clean worktree. `Cargo.toml` and all `[profile.*]` settings remain untouched.
- **Grenze / offen:** no behavior change to CLI, protocol, API or shell policy; no login, credentials, live call, release, tag, deployment, PR or merge. T-1000..T-1005 remain free; T-1004 still needs owner approval. Stop here and wait; do not claim T-1000 implicitly.

## 2026-11-26 - T-998 implemented: protocol/brain no longer reach browser

- **Branch/claim:** `refactor/T-998-protocol-browser-edge`, claim commit `103967b`, implementation commit `5a24ddb`, owner `local/opencode`. T-998 stays `claimed` in `docs/TASKBOARD.json` — no merge, no `done`, no PR. T-997 also stays `claimed`; the owner's explicit "mach" overrode the formal dependency. Both commits are pushed and the worktree is clean.
- **Scope delivered:** `has_protocol_payload`, `block_phrase_in_text`, `BLOCK_PHRASES` and `BLOCK_BANNER_MAX_CHARS` now live in `src/protocol/mod.rs`; `src/brain.rs` and `src/protocol/parser.rs` call the protocol module directly; `src/browser/mod.rs` re-exports both helpers so `controller.rs` and `contract.rs` keep compiling unchanged. `rg` has no `crate::browser` hit in `src/protocol` or `src/brain.rs` (ripgrep-backed search: the `rg` executable is absent from this PowerShell). Proof: `docs/proofs/T-998/README.md`.
- **Guard delta:** the former eight-module cycle `api_bridge, brain, brain_probe, browser, browser_inference, browser_pool, protocol, relay` shrank to six modules (`brain` and `protocol` dropped out). Measured graph stayed at 87 modules / 274 edges / 6 cycles; upward edges rose from 30 to 32 because removing the cycle exposed `browser->protocol` and added `brain->protocol`. Both are explicit `edge:` exceptions in `docs/arch/layer_exceptions.txt`; the cycle line was shortened, not deleted. No behavior change.
- **Deviations:** `docs/arch/LAYERS.md` and `docs/proofs/T-997/README.md` received the `**Referenz.**` banner required by `startup::tests::betriebs_markdown_hat_eine_wahrheit`; both are T-997 documentation artifacts outside the T-998 scope and were red at claim time. The two new upward-exception lines are documented debt: the task requires the helper in the protocol layer while `brain` is ranked lower. No strict shrink of the total exception list.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored; `cargo test --test arch_layers -- --nocapture` 4 passed (guard: modules=87, edges=274, cycles=6, upward_edges=32); `cargo test --all-targets --quiet` 1521 lib + 7 binary-unit + 4 integration passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. Test count is unchanged from the T-997 baseline; the initial claim-time lib run was 1520 passed / 1 failed / 1 ignored solely because of the two missing banners.
- **Binary smoke blocked, not a regression:** `target/debug/webagent.exe --help` and `diagnose` exit `-1073741515` (`0xC0000005`) before `main`. A clean detached worktree at claim commit `103967b` builds a 150,498,625-byte binary with the same crash; this branch's binary is 150,498,536 bytes. `Cargo.toml` and every `[profile.*]` setting are untouched. No login, live brain call or provider request was made.
- **Grenze / offen:** no release, tag, deployment, PR or merge. T-999..T-1005 remain free; the crate-split decision (T-1004) still needs owner approval. Stop here and wait for review; do not claim T-999 implicitly.

## 2026-11-26 - T-997 implemented: module layer and cycle ratchet

- **Branch/claim:** `feature/T-997-arch-layers`, implementation commit `2b875b5`, owner `local/opencode`. T-997 remains `claimed` in `docs/TASKBOARD.json` — no merge, no `done`. The board/mirror/claim commit is `fdff65c`. No next task claimed.
- **Scope delivered:** `tests/arch_layers.rs` (source-based graph plus four tests), `docs/arch/layers.toml` (87 modules over `cross` + `L0..L7`), `docs/arch/layer_exceptions.txt` (6 cycles + 30 grandfathered upward edges), `docs/arch/LAYERS.md`, `docs/proofs/T-997/README.md`. Only the proof file and this handover are additional; no source module, CLI, protocol, API, `Cargo.toml` or `[profile.*]` change.
- **Measurement:** the guard derives 87 top-level modules, 274 `crate::<module>` edges, 6 cycles and 30 upward edges from `master 93d33a3`. The prompt's 89 modules / five cycles did not reproduce: `browser_pool`, `bin_hooks` and a sixth `benchmark`/`controller` cycle were missing from its list; raw lines were 92,649 (64,130 after removing `#[cfg(test)]`). Differences and reasons are in the proof.
- **Gates measured:** `cargo fmt --all -- --check` 0; `cargo clippy --features webview --all-targets -- -D warnings` 0; `cargo test --lib --quiet` 1521 passed / 0 failed / 1 ignored (unchanged from base); `cargo test --test arch_layers -- --nocapture` 4 passed / 0 failed; `cargo test --all-targets --quiet` lib 1521 + binary-unit 7 + integration 4 passed; `cargo test --locked --no-default-features --test arch_layers --quiet` 4 passed; `git diff --check` 0. The three extra integration tests prove rejection of a new cycle, a new upward edge and a stale exception.
- **Ratchet behavior:** every exception must still exist, so a removed edge/cycle makes the test fail until its line is deleted. Edges inside an excepted cycle are covered by the cycle line; the 30 remaining upward edges are explicit inventory, not hidden debt.
- **Grenze / offen:** no browser, network, login, secrets, capability-proof record, behavior change, release, tag, deployment, PR or merge. T-998..T-1005 remain free; the crate-split decision (T-1004) still needs owner approval before any extraction.

## 2026-11-26 - Phase 16 board prepared, T-997 claimed (bookkeeping only)

- **Branch:** `feature/T-997-arch-layers`, created from clean `master` `93d33a3` in worktree `C:\Users\storax\Documents\Standardprojekt\webagent-rs-phase16`. The initial `feat/…` name was renamed before any commit: `scripts/commit-as-agent.ps1` and `taskboard::tests::taskboard_invarianten_halten` only allow `feature|fix|docs|chore|refactor|test` prefixes for new claims. The Play branch `fix/T-976-api-turn-reveal` stays untouched at `3779d47` (pushed; no merge/PR/tag).
- **Board:** Added the nine phase-16 tasks `T-997..T-1005` to `docs/TASKBOARD.json` and to the machine-readable mirror plus a human-readable phase section in `docs/TASKBOARD.md`. Source: the JSON fragment pasted by the owner; `TASKBOARD.refactor.json` was absent from workspace and all git history. Owner-approved renumbering from `T-976..T-984` to `T-997..T-1005` covers `depends_on`, proof paths and verification strings; the historical `T-975` baseline reference stays unchanged. Board now has 109 tasks.
- **Claim:** `T-997` (`Layer ratchet: guard test for module dependency direction and cycles`) is `claimed` by `local/opencode` on `feature/T-997-arch-layers` with `claimed_at=2026-11-26`. `T-998..T-1005` remain `free`.
- **Baseline (measured, not quoted):** `cargo test --lib --quiet` on the unmodified branch base: **1521 passed / 0 failed / 1 ignored** in 57.96 s. This matches the prompt's `T-975` number; the prompt's other figures (89 top-level modules, about 94k lines, five cycles) still need re-measurement inside T-997.
- **Gate for this bookkeeping commit:** `cargo test --lib taskboard::tests::taskboard_invarianten_halten -- --exact` 1 passed; `git diff --check` 0. No source code, Cargo.toml, CLI or protocol change in this commit.
- **Next:** implement only the T-997 scope (`tests/arch_layers.rs`, `docs/arch/layers.toml`, `docs/arch/layer_exceptions.txt`, `docs/arch/LAYERS.md`, `docs/proofs/T-997/README.md`); then run the full four gates, record before/after test counts, push and stop for review. No next task is claimed.

## 2026-11-26 - T-975: Pi-Abdeckung geschlossen

- **Branch:** `chore/T-975-pi-full-coverage`, Basis `master` `c9050f3`. T-975 ist `done` mit `done_at=2026-11-26`; Board und Markdown-Spiegel sind nachgezogen. Direkte Nutzeranforderung: „ja bitte auf 100% hinarbeiten". Keine Runtime-Änderung, kein Portwechsel, kein Eingriff in fremde Pi-Provider.
- **Alle Brains registriert:** Die dynamische Extension `examples/pi/extensions/webagent-models.ts` wurde nach `%USERPROFILE%\.pi\agent\extensions\webagent-models.ts` installiert (Pfad aus der Pi-Dokumentation für auto-discovery/hot-reload). `pi --list-models webagent` liefert exit 0 mit neun Modellen: `auto`, `chatgpt`, `claude`, `deepseek`, `gemini`, `kimi`, `mistral`, `qwen`, `zai`; die letzten beiden ohne Bildunterstützung.
- **Neun echte Turns:** Jedes registrierte Modell wurde einzeln mit `pi --offline --provider webagent --model webagent/<brain> --no-tools --no-session --no-context-files --no-skills --no-prompt-templates --mode json` gegen die dauerhaft laufende Bridge gefahren. Alle neun: exit 0, Marker `WEBAGENT-<BRAIN>-OK-20261126` exakt gefunden.
- **Dialog über die echte Verknüpfung:** `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk` real per `Start-Process` gestartet. Sichtbares Fenster `WebAgent Bridge Start` mit `hwnd=23791118`, `visible=True`, `rect=122,130,1251,765` (Win32 `GetWindowRect`/`IsWindowVisible`). `n{ENTER}` → `optin=False`, `port=False`, `prompt.log` `Antwort=aus`, Fenster nach 15 s automatisch zu. `j{ENTER}` → `optin=True`, `port=True`, `Antwort=start`, `supervisor.log` `Bridge gestartet PID=7000` und gesund.
- **Beleg:** `docs/proofs/T-975/pi-volle-abdeckung.md` mit Katalogtabelle, allen neun Turn-Ergebnissen und den Dialog-Messwerten. Kein Tokenwert, kein `Bearer `, keine Cookies in Datei oder Ausgabe; `auth.json` blieb unberührt, es wurde keine Anmeldung durchgeführt.
- **Gates:** `cargo fmt --all -- --check` 0; `cargo clippy --all-targets --locked -- -D warnings` 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Grenze:** Der Anmelde-Dialog wurde über die echte Verknüpfung und das echte Skript geprüft, nicht durch einen echten Reboot/Re-Login – dafür ist eine gesonderte ausdrückliche Freigabe nötig. Kein Last-, Dauerlast- oder Parallelitätstest, keine neue Browser-Anmeldung, kein Release, kein Tag, kein Deployment.

## 2026-11-26 - T-974: Bridge-Dauerbetrieb mit Anmelde-Abfrage

- **Branch:** `chore/T-974-bridge-autostart`, Basis `master` `d824b10`. T-974 ist `done` mit `done_at=2026-11-26`. Direkte Nutzerentscheidung: "bitte bei windows start terminalfenster mit abfrage ob bridge gestartet werden soll".
- **Bausteine:** `scripts/bridge-start-prompt.ps1` zeigt beim Anmelden ein sichtbares Terminalfenster mit `Bridge jetzt starten? [J/N]`. `J` setzt das Opt-in und startet den Supervisor einmal; `N` entfernt das Opt-in und stoppt eine noch laufende eigene Bridge. `scripts/supervise-bridge.ps1` ist ein idempotenter Health-Check auf `127.0.0.1:8788/health` und startet die Bridge nur bei Bedarf neu; mit `-RequireOptIn` tut er ohne Zustimmung nichts. `scripts/setup-bridge-autostart.ps1` installiert und entfernt Verknuepfung sowie zyklische Aufgabe. Bedienung und Grenzen: `docs/LOCAL_BRIDGE_DAEMON.md`.
- **Installierter Endzustand:** Verknuepfung `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\WebAgent Bridge Start.lnk`, Aufgabe `WebAgent Bridge Health (5min)` (alle 5 Minuten, Interactive/Limited, nur aktueller Benutzer). Opt-in gesetzt, Bridge laeuft, Task-Lauf `LastTaskResult=0`. Kein Admin, kein Fernzugriff, kein Session-0-Dienst.
- **Real geprueft:** Supervisor erster Start und Zweitlauf (`keine Aenderung`) exit 0; Antwort `e` (Nein) stoppte die eigene Bridge und die zyklische Pruefung blieb no-op bei freiem Port; Antwort `j` (Ja) setzte das Opt-in und startete gesund; nach Stopp der eigenen PID startete die zyklische Pruefung die Bridge neu; `setup -Remove` entfernte Verknuepfung, Aufgabe, Opt-in und Bridge und wurde danach neu installiert. Ein echter Pi-Turn gegen die Daemon-Bridge lieferte exit 0 und exakt `WEBAGENT-DAEMON-OK-20261126`. Beleg `docs/proofs/T-974/daemon-abnahme.md`; Secret-Scan: kein Tokenwert, kein `Bearer `.
- **Rueckweg:** `pwsh -NoProfile -ExecutionPolicy RemoteSigned -File scripts/setup-bridge-autostart.ps1 -Remove`. Logs unter `%LOCALAPPDATA%\webagent\bridge\` bleiben als Beleg erhalten und koennen separat geloescht werden. Der Supervisor stoppt ausschliesslich die eigene, per `bridge.pid` und Prozessname verifizierte PID; fremde Prozesse auf Port 8788 fuehren zu Exit 3.
- **Gates erneut gruen:** `cargo fmt --all -- --check` 0; `cargo clippy --all-targets --locked -- -D warnings` 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` 0.
- **Grenze:** Der Anmeldedialog wurde anhand von Verknuepfung und Skript geprueft, nicht durch einen echten Reboot/Re-Login. Kein Release, kein Tag, kein Deployment, kein Last-/Paralleltest und kein Monitoring ausser dem 5-Minuten-Health-Check. `auth.json` blieb unberuehrt, es wurde kein bestehendes Secret gelesen oder ausgegeben.
## 2026-11-26 - T-973: lokaler Pi-Provider nutzbar

- **Branch:** `fix/T-973-pi-provider-port-8788`, Basis `master` `4df9d3d`. T-973 ist `done` mit `done_at=2026-11-26`; Board und Markdown-Spiegel sind nachgezogen. Kein Runtime-Code, kein Portwechsel der Bridge, kein neues Protokoll.
- **Zwei echte Fehler behoben:** Die aktiven Pi-Dateien nutzten noch den historisch toten Port `8787` (`examples/pi/models.json`, `examples/pi/extensions/webagent-models.ts`, `scripts/test-pi-provider.ps1`); jetzt überall `8788`. Das Modell-Schema enthielt `"input": ["text","image","audio"]`, das Pi 1.5.2 ablehnt (`("text" | "image")[]`). Korrigiert in der Vorlage und in beiden Snippets in `docs/API_BRIDGE.md`; die Extension filtert Katalog-Modalitys jetzt auf `text`/`image` und fällt auf `["text"]` zurück, damit `auto`/`gemini` nicht registrierungsunfähig werden.
- **Isolierter Smoke:** Mit deploytem `v0.11.7` auf `127.0.0.1:8788` und frischem, nur prozessbezogenem Token: `pwsh -File scripts/test-pi-provider.ps1` → `PASS: Textturn und mindestens fünf Tool-Aufrufe in einem Pi-Auftrag sind grün.` (`PI_BRIDGE_OK`, fünf sequenzielle `read`-Calls). Rohtranskript `docs/proofs/T-973/pi-tool-loop.json`.
- **Nutzerkonfiguration:** `%USERPROFILE%\.pi\agent\models.json` wurde vorher nach `models.json.bak-T973-20261126` gesichert und um den Provider `webagent` ergänzt (`baseUrl=http://127.0.0.1:8788/v1`, `api=openai-completions`, `apiKey=$WEBAGENT_API_KEY`, ein Modell `webagent/chatgpt`). Ein frischer lokaler Loopback-Token wurde als Benutzer-Umgebungsvariable `WEBAGENT_API_KEY` gesetzt, damit Pi und Bridge ihn teilen; der Wert wurde weder ausgegeben noch in einer Datei gespeichert. `auth.json` wurde nicht geöffnet, kein bestehendes Secret gelesen oder kopiert, keine Anmeldung durchgeführt.
- **Echter Nutzer-Turn:** `pi --list-models webagent` zeigt `webagent/chatgpt`. Mit derselben Konfiguration lieferte ein echter Textturn gegen die Bridge exit 0 und exakt `WEBAGENT-PI-USER-OK-20261126`; Beleg `docs/proofs/T-973/pi-user-config-turn.md`. Secret-Scan der beiden Belegdateien: kein Tokenwert, kein `Bearer `, keine Cookies; der Name `WEBAGENT_API_KEY` erscheint nur als Variablenname.
- **Gates erneut grün:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets --locked -- -D warnings` exit 0; `cargo test --lib --locked` 1521 passed / 0 failed / 1 ignored; `git diff --check` exit 0. Beide Bridges wurden gestoppt, Port `8788` ist frei.
- **Grenze / offen:** Kein Release, kein Tag, kein Deployment aus dieser Scheibe, kein Dauerbetrieb/Autostart und kein Fernzugriff. Der Provider ist manuell startbar; der Befehl `webagent api serve --brain chatgpt --headless --port 8788` benötigt die gesetzte `WEBAGENT_API_KEY`. Für alle acht Brains bleibt die dynamische Extension `examples/pi/extensions/webagent-models.ts` der dokumentierte Weg; sie wurde nicht in die Nutzerinstallation kopiert.
## 2026-11-26 - Release v0.11.7, lokales Deployment und Live-Turn-Abnahme

- **Freigabe:** Der Eigentümer hat PR/Merge/Deployment und einen zusätzlichen Live-Turn freigegeben; die Anmeldung lag bereits vor. Der Agent hat keine Zugangsdaten gelesen, keine Anmeldung durchgeführt und keine Profil-/Token-Dateien geöffnet.
- **Release:** Branch `release/v0.11.7-hardening` mit Versionsbump `0.11.6` → `0.11.7` in `Cargo.toml`/`Cargo.lock`, CHANGELOG und `done_at`-Korrektur für T-969..T-972 (der Board-Invariantentest war nach dem `done`-Setzen kurz rot). PR [#112](https://github.com/st0rax/webagent-rs/pull/112) mit vier grünen Checks squash-gemergt (`d54d347`). `verify-release-version.ps1 -Tag v0.11.7` → Exit 0, `-Tag v0.11.6` → Exit 1. Tag `v0.11.7` gepusht; Workflow [`45725669798`](https://github.com/st0rax/webagent-rs/actions/runs/45725669798) vollständig grün inkl. `github-release`, sieben Assets.
- **Gates:** `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --lib --locked` (1521 passed / 0 failed / 1 ignored) und `git diff --check` grün.
- **Deployment:** Exakt die Release-Assets aus `v0.11.7` nach `webagent-deploy-target\release\` kopiert; EXE-SHA-256 `0f45d96377857220ab7d25eda44bfe3f6e8886e6bb01136136d5d2dcfd973e84`, DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`, alle sieben Asset-Digests geprüft. Die `v0.11.6`-Dateien liegen unter `v0.11.6-backup\` als Rückweg (`v0.11.5-backup\` bleibt erhalten). `--version` → `webagent 0.11.7 (d54d34755)`, `canary` 8/8 `ok`, `brains-health` 8/8 `selectors=ok` (`bot2bot=MISSING`, informativ).
- **Provider-Rolle:** `api serve --brain chatgpt --headless --port 8788` mit prozessonly Token: `GET /health` 200, `GET /v1/models` mit Bearer 9 Modelle (`webagent/auto` zuerst), ohne Bearer 401. Ein echter `POST /v1/chat/completions` lieferte `PROVIDER-OK-20261126` (`finish_reason=stop`, `chatcmpl-18eb7029b20cca24`); Beleg in `docs/proofs/live-turn-2026-11-26/provider-chat-turn.md`. Prozess gestoppt, Port 8788 frei.
- **Live-Turn-Abnahme:** Deploytes `v0.11.7`, sichtbares WebView2-Fenster, `cloud web-view-stream --brain chatgpt --prompt … --net-trace` → Exit 0, Token exakt `LIVE-OK-20261126-V0.11.7`, `oauth_session=true`, `api_key=false`, 200-SSE-`provider_response` (602 ms). Rohbeleg plus Secret-Scan und Grenzen unter `docs/proofs/live-turn-2026-11-26/`. Ein gleicher Preflight mit `v0.11.6` und `--headless` lieferte `LIVE-OK-20261126`.
- **Bewertung:** `docs/PRODUCTION_READINESS.md` auf `v0.11.7` und Zielbetrieb umgeschrieben: Go für den lokalen Rechner mit Loopback-Provider, No-Go für Fern-/Produktivbetrieb ohne Monitoring/Last/Risikoakzeptanz. Restrisiken sind fünf Punkte, darunter ein Turn/ein Brain und die interne `Buffer()`-Deprecation im Publish-Job.
## 2026-11-26 - Vier Härtungsscheiben auf `master` zusammengeführt

- **Freigabe:** Der Eigentümer hat am 2026-11-25 „absolute freigabe für alle sinnvollen vorgänge wie PR und merge etc" erteilt. Deployment und Live-Turn wurden später separat freigegeben.
- **PRs und Checks:** [#108](https://github.com/st0rax/webagent-rs/pull/108) (T-969) wurde als Squash `d60a6c0` über GitHub gemergt. Für [#109](https://github.com/st0rax/webagent-rs/pull/109), [#110](https://github.com/st0rax/webagent-rs/pull/110) und [#111](https://github.com/st0rax/webagent-rs/pull/111) verweigerte GitHub den direkten Squash-Merge wegen Doku-Konflikten. Alle acht Check-Jobs auf den Retry-SHAs `17c0adb`, `e3ddb12`, `1c71e87`, `b3b23da7` waren zuvor `completed/success`.
- **Lokale Squash-Commits:** `a161c8b` (T-970), `9d2692d` (T-971), `c2ff017` (T-972), jeweils mit PR-Referenz in der Commit-Message, auf `master` gepusht. GitHub erkennt manuell gepushte Squashes nicht als PR-Merge; die drei PRs wurden mit einem nachvollziehbaren Kommentar geschlossen. Kein Force-Push, keine History-Umschreibung.
- **Konfliktauflösung:** Nur `docs/CURRENT_WORK.md` und `docs/TASKBOARD.json` waren betroffen. Alle vier Doku-Abschnitte bleiben erhalten; die Tafel behält die Master-Version und übernimmt pro Task nur den branchspezifischen Diff. Kein Code-Konflikt, kein Logikverlust.
- **Gates auf dem zusammengeführten Stand:** `cargo fmt --all -- --check` exit 0; `cargo clippy --all-targets -- -D warnings` exit 0; `cargo test --lib` 1521 passed / 0 failed / 1 ignored; `git diff --check` exit 0. `master` und `origin/master` stehen auf `c2ff017`.
- **Tafel:** T-969 bis T-972 sind jetzt `done`, der Markdown-Spiegel ist nachgezogen. Offen bleiben die Release-/Deploy-Scheibe und die Live-Turn-Abnahme mit vorhandener Nutzer-Sitzung.
