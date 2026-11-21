# Current Work

## 2026-11-21 — G-010: T-948 abgeschlossen (Preflight vor dem Senden)

- **PR #94 gemergt** (Squash `23879aa`, alle CI-Checks grün), Branch
  `fix/T-948-consent-preflight` gelöscht. `master` lokal und remote bei
  `23879aa`, Working Tree vor dieser Statusfortschreibung sauber.
- **Ursache (Messbefund 2026-09-14):** Sechs mistral-Anfragen endeten mit
  `timeout_no_message`, obwohl das Senden Erfolg meldete. `webagent shot`
  belegte eine abgemeldete Seite mit „Anmelden"/„Registrieren" und einem
  modalen „Vibe Nutzungsbedingungen"-Dialog. `login_indicator` zeigte auf den
  Composer (auch abgemeldet im DOM), `login_button` kannte „Anmelden" nicht.
- **Fix (rein lesend, nur für deklarierte Brains):** `send_generic` ruft
  `operations::pre_send_block_reason` vor `wake_renderer`/Fill. Sichtbarer
  Zustimmungsdialog → `terms_consent_required`, sichtbarer Anmeldeknopf →
  `login_required`; Meldung `send_phase=preflight: …`, HTTP 502 mit Marker im
  Text. Kein Zustimmungsknopf, keine Anmeldung, kein Prompt — beides bleibt beim
  Menschen. `selectors/mistral.json` korrigiert `login_indicator` auf
  Profil-/Konto-Knöpfe, ergänzt „Anmelden" und `send_preflight_guard`.
- **Belege:** `docs/proofs/T-948/measurement.md` mit Mock-DOM-Tests und echter
  Red-/Green-Demonstration (beide Guard-Tests ohne Guard:
  `send_phase=insert: Composer-Feld nicht gefunden (Timeout)`, Exit 101; danach
  grün). `docs/API_BRIDGE.md` nennt die Marker. **Kein Live-Anspruch:** die
  neuen Profil-/Anmelde-Selektoren sind nicht live vermessen; der `handoff`
  „Fehlermeldung nachher gegen den abgemeldeten Zustand" bleibt offen.
- **Gates lokal grün:** `cargo fmt --all -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --lib` (1483 grün,
  0 failed, 1 ignored), `git diff --check`.
- **Board:** T-948 im JSON und Spiegel auf `done`, Stand 82/91 `done`,
  9 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigentümer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdrückliche Freigabe.

## 2026-11-21 — G-010: T-946 abgeschlossen (verwaiste Anfrage / Brain-Sperre)

- **PR #93 gemergt** (Squash `ae9cb2c`, alle CI-Checks grün), Branch
  `fix/T-946-client-disconnect-lock` gelöscht. `master` lokal und remote bei
  `ae9cb2c`, Working Tree sauber.
- **Ursache:** Eine abgebrochene mistral-Anfrage lief am 2026-09-14 dreimal
  (~867/867/861 s) im Browser weiter, obwohl der Client längst weg war. Die
  per-Brain-Sperre hielt den verwaisten Turn; Folgeanfragen an dasselbe Brain
  warteten. Andere Brains liefen parallel — das war korrekt, aber
  `docs/API_BRIDGE.md` behauptete fälschlich keine parallelen Browserruns.
- **Fix (drei Ebenen, ohne globale Serialisierung):**
  1. `TcpClientPresence` in `api_bridge/transport.rs` beobachtet den Client über
     einen zweiten Socket-Handle (20 ms Peek, Lese-Timeout sofort restore).
  2. `with_browser_lock` in `api_bridge/inference.rs` prüft vor und nach dem
     Sperrerwerb; verwaisste Anfragen starten keinen Turn.
  3. `run_turn_loop` in `relay.rs` prüft zu Beginn jedes Turns und übergibt
     einen `cancelled`-Closure an `wait_response_streaming_with_cancel`
     (`browser/backend.rs`, Checks in beiden Poll-Phasen). Abbruch →
     `backend_status=client_disconnected`, `relay_stop`, brain_score-Grund
     `client_disconnected`, Sperre frei. Kein erneutes Senden eines bereits
     gesendeten Prompts.
- **Belege:** `docs/proofs/T-946/measurement.md` mit echtem TCP-Loopback-Test,
  vier Red-/Green-Tests, echten brain_score-Zeilen und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1479 grün, 1 ignored), `git diff --check`. Ein einmaliger
  `ConnectionReset` im unabhängigen Web-UI-One-Shot-Test war nicht
  reproduzierbar (5 Einzel- + 2 Gesamtwiederholungen grün). `docs/API_BRIDGE.md`
  beschreibt jetzt Serialisierung je Brain und Abbruchverhalten korrekt.
- **Board:** T-946 im JSON und Spiegel auf `done`, Stand 81/91 `done`,
  10 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) bleiben
  ohne Eigentümer-Entscheidung offen; 100 % sind ehrlich unerreicht. Kein
  Tag/Release ohne neue ausdrückliche Freigabe. G-010 bleibt in_progress.

## 2026-11-21 — G-010: T-953 abgeschlossen (Modellausgabe-Fehler)

- **PR #92 gemergt** (Squash `02b6987`, alle CI-Checks grün), Branch
  `fix/T-953-model-output-error` gelöscht. `master` lokal und remote bei
  `02b6987`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **DoD 1 — Fehlertyp:** Jeder `parse_response`-Fehler trägt jetzt den stabilen
  Marker `MODEL_OUTPUT_ERROR:`. `inference_failure_response()` in
  `provider_handlers.rs` meldet 502 mit OpenAI-`type: server_error` und
  `code: model_output_envelope_invalid` (Anthropic entsprechend) statt
  `invalid_request_error`. Andere Fehler behalten ihre alte Übersetzung; der
  DSML-Fehler aus T-957 ist nun ebenfalls als Modellausgabe gekennzeichnet.
- **DoD 2 — brain_score:** `relay::run_turn_loop` prüft die Ausgabe über
  `OutputCheck::validate`, bevor `answer` gesetzt wird. Ein ungueltiger
  Umschlag ist ein Fehlschlag mit dem eigenen Grund
  `model_output_envelope_invalid`; der Circuit-Breaker bleibt bewusst unberührt
  (Brain war erreichbar). Vorher wurde der Turn als Erfolg verbucht.
- **DoD 3 — Korrekturgrenze:** Höchstens eine Korrektur im selben Chat (kein
  `new_chat`) mit der Parserfehlermeldung; danach fail-closed. Das Zeitbudget
  gilt für die gesamte Anfrage inklusive Korrektur. Mock-Backend-Tests prüfen
  `new_chats=1`, `sends=2` und den brain_score-Eintrag.
- **Beleg:** `docs/proofs/T-953/measurement.md` mit Rot-/Green-Tests, echten
  Event-Zeilen aus dem Test-Data-Verzeichnis und Vorher/Nachher. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1474 grün), `git diff --check`. `docs/API_BRIDGE.md`
  beschreibt Fehlerklasse, Grund und Korrekturgrenze.
- **Board:** T-953 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  80/91 `done`, 11 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigentümer-Entscheidung nicht abschließbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 — G-010: T-957 abgeschlossen (DSML fail-closed)

- **PR #91 gemergt** (Squash `f2a5aac`, alle CI-Checks grün), Branch
  `fix/T-957-dsml-fail-closed` gelöscht. `master` lokal und remote bei
  `f2a5aac`, Working Tree vor der hiesigen Statusfortschreibung sauber.
- **Entscheidung fail-closed statt Übersetzung:** `parse_response` erkennt den
  DSML-Marker aus dem realen deepseek-Beispiel (Vollbreiten-Form und die im
  Taskboard gespeicherte verformte Form) und meldet
  `DSML_TOOL_FORMAT_UNSUPPORTED`. Keine Übersetzung in `tool_calls`, weil die
  Argumente aus derselben verformenden Lesestrecke stammen, die T-956 nur für
  den JSON-Umschlag abgesichert hat. Gültige `WEBAGENT_INFERENCE/1`-Umschläge
  bleiben unberührt. `docs/API_BRIDGE.md` beschreibt die Grenze.
- **Beleg:** `docs/proofs/T-957/measurement.md` mit Red-Test (vorher
  `Ok(Textantwort)`, `tool_calls: []`), Green-Tests und Gates. Lokal
  `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --lib` (1468 grün), `git diff --check`. Der erste CI-Lauf war
  rot, weil ein neues Beleg-Markdown das Referenz-Banner brauchte; mit Banner
  sind alle Checks grün. MinGW liegt lokal in einem WinGet-Paket und wurde nur
  für die Cargo-Läufe an `PATH` gehängt (keine Repo-Änderung).
- **Board:** T-957 nach dem Merge auf `done`, Spiegel synchron. Stand damit
  79/91 `done`, 12 `free`. Kein Tag/Release. G-010 bleibt in_progress.
- **Offen:** T-914 (expliziter USER-SKIP) und T-961 (Semantik-Freigabe) sind
  ohne Eigentümer-Entscheidung nicht abschließbar; 100 % bleiben ehrlich
  unerreicht.

## 2026-11-21 — T-961: auch erzwungener Tool-Call widerlegt — weiter NICHT abgenommen

- **Dritte Richtung geprüft (strukturell, ohne Code-Änderung):** ein direkter
  OpenAI-Client auf dem unveränderten `master` `bf0a65b` (frisch gebaut,
  SHA256 `3532478D...F710`) sendete auf fünf aufeinanderfolgenden Turns
  `tool_choice: "required"` — bereits vom Adapter unterstützt, also keine
  API-Semantik-Änderung und keine Freigabe nötig. Ergebnis aus 8 Läufen:
  **1/8 streng grün**. Sechs Läufe lieferten trotz Verpflichtung nur Text
  (fail-closed 502), zwei scheiterten an `ABSENDEKNOPF_DEAKTIVIERT`. Ein
  zusätzlicher Validierungslauf mit fünf sauberen Tool-Turns hatte einen
  abschliessenden Renderer-Timeout und ist nicht Teil der Vergleichsreihe.
  Rohbelege: `docs/proofs/T-961/required-mode-*`, Auswertung
  `required-mode-result.md`, 0 Secret-Treffer.
- **Einordnung:** `required` macht die Absage für Aufrufer sichtbar (502 statt
  Text), beseitigt aber die Fehlerklasse nicht. Damit sind drei Richtungen
  gemessen: Verfügbarkeitsregel 2/8, `[FUNKTIONEN]` 6/8, `required` 1/8.
  Eine serverseitige Auto→Required-Eskalation oder ein versteckter Retry
  würde API-Semantik ändern und braucht eine ausdrückliche Eigentümer-Freigabe
  samt eigener Abnahme; beides ist **nicht** implementiert.
- **Eigentümer-Entscheidung vom 21.11.2026:** Negativbefund dokumentieren,
  **T-961 offen lassen**. Kein PASS, kein `done`, kein Code-Merge, kein
  Tag/Release. G-009 bleibt offen. Die bekannte Grenze steht jetzt in
  `docs/API_BRIDGE.md` inklusive des `required`-Befunds.

## 2026-11-21 — T-961: zwei Prompt-Kandidaten widerlegt — weiter NICHT abgenommen

- **PR #88 gemergt** (`8693b99`, Squash, alle CI-Checks gruen): Kontobefund und
  der ehrliche 2/8-Negativbeleg zum Verfügbarkeits-Kandidaten liegen jetzt in
  `master`.
- **Zweiter Kandidat ebenfalls widerlegt:** `f899ed3` nannte den Schema-Block
  `[FUNKTIONEN]` statt `[Client-Werkzeuge]` und vermied jedes Meta-Wort zur
  Verfügbarkeit. Alle lokalen Gates gruen (fmt, clippy, 1467 Tests), aber
  8 echte Live-Laeufe lieferten **6/8 streng gruen** und zwei Laeufe mit exakt
  der T-961-Fehlerklasse („Das read-Werkzeug ist in dieser Sitzung nicht
  verfügbar"). Das ist nicht besser als die 3/4-Baseline von `master`. Die
  Code-Aenderung wurde auf dem Branch mit `a7eb7ef` zurueckgenommen; Rohtrans-
  kripte und Auswertung liegen unter `docs/proofs/T-961/function-*`.
- **Ergebnis der Prompt-Versuche:** ausdrückliche Verfügbarkeitsregel 2/8,
  reine Umbenennung 6/8 (Baseline 3/4). Reine Prompt-Wortwahl ist als Richtung
  widerlegt; der Kontozustand scheidet als Ursache aus. Die Ursache bleibt
  **ungeklärt** (modellseitige Stochastik ist eine Vermutung, kein Beleg).
- **Entscheidung:** kein PASS, kein `done`, kein Merge eines Kandidaten,
  kein Tag/Release. **T-961 bleibt `free`**, G-009 bleibt offen. Die Bridge
  dokumentiert die bekannte Grenze jetzt in `docs/API_BRIDGE.md`, damit
  Aufrufer unvollständige Werkzeugläufe anhand der Marker pruefen.

## 2026-11-21 — T-961: Kontobefund belegt, Kandidat widerlegt — weiter NICHT abgenommen

- **PR #87 gemergt** (`dfebc47`, Squash, CI komplett gruen), `master` lokal und remote
  bei `e4386f0`. Er dokumentiert ehrlich, dass der Rahmungsfix die Intermittenz
  nicht beseitigt hat.
- **Kontozustand autonom geprueft** (DoD-Punkt, bisher als Eigentuemer-Auftrag
  markiert): read-only DOM der angemeldeten ChatGPT-WebView auf
  `/settings/personalisation` und im geoeffneten Dialog
  „Erinnerungszusammenfassung". Erinnerung aktiv, Abschnitt
  „Benutzerdefinierte Anweisungen" vorhanden, Zusammenfassung aktuell
  („Gerade aktualisiert"/„Überblick"), **kein Eintrag, der Werkzeugzugriff
  verneint**. Keine Einstellung geaendert, keine Zugangsdaten/Profile/Cookies
  gelesen, temporaere Diagnose-Patches vollstaendig zurueckgerollt. Persönliche
  Inhalte bewusst nicht committet. Beleg: `docs/proofs/T-961/account-state.md`.
- **Modellwechsel nicht moeglich:** `webagent model --brain chatgpt` meldet
  `aktuell ''` und `'model_menu' nicht anklickbar`; sichtbarer Modus lief in eine
  Navigation timeout. Kein Menue- und kein Konto-Settings-Eingriff vorgenommen.
- **Kandidat widerlegt:** `0039cd4` ergaenzte den Browser-Prompt um eine neutral
  formulierte Regel (Werkzeuge seien aktiv und ausfuehrbar, ein Ergebnis
  verbrauche das Werkzeug nicht). Alle lokalen Gates waren gruen (1467 Tests),
  aber 8 echte Live-Laeufe mit diesem Binary (Port 8788, frische Tokens,
  identische Prompts) lieferten nur **2/8 streng gruen**. In fuenf Laeufen
  verneinte das Modell den Werkzeugzugriff — teils schon vor dem ersten Aufruf
  („ist in dieser Sitzung nicht ausführbar"), teils nach drei Reads; ein Lauf
  endete in `ABSENDEKNOPF_DEAKTIVIERT` + `circuit_open`. Die Formulierung hat
  das Meta-Thema „Verfuegbarkeit" also eher provoziert. Die Code-Aenderung
  wurde auf dem Branch mit `63a6f7a` zurueckgenommen; der Branch enthaelt nur
  noch die Belegdokumentation (`docs/proofs/T-961/availability-*`).
- **Entscheidung jetzt:** kein PASS, kein `done`, kein Merge des Kandidaten,
  kein Tag/Release. G-009 bleibt offen. **T-961 bleibt `free`.** Die
  Intermittenz ist nicht erklaert; der Kontozustand scheidet als Ursache aus.
  Ein naechster Versuch muss ohne jedes Verfügbarkeits-Meta im Prompt auskommen
  und braucht erneut eine belastbare Raw-Transkript-Reihe.

## 2026-11-20 — T-961: Rahmungsfix gemergt, Intermittenz bleibt — weiter NICHT abgenommen

- **PR #86 gemergt** (`eaed3d1`, Squash, CI gruen, Branch geloescht). Ursache:
  `conversation_prompt` labelte Tool-Ergebnisse im **Verlauf** mit
  `[Client-Werkzeug-Ergebnis]` + `[tool_call_id]`, rendering aber die **letzte**
  Nachricht (genau Pi's `toolResult`) als nackten Text unter „Aktuelle Nachricht
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
  call_1..call_4 erfolgreich, dann *„Das fünfte read-Werkzeug ist in dieser
  Sitzung nicht verfügbar."* (4/5, fehlender Marker 5). Zwei Vorab-Laeufe mit
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

## 2026-11-20 — T-961: drei Blocker behoben, gruener Lauf, aber intermittierend — NICHT abgenommen

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
  Ursprungssymptom: call_1 erfolgreich (`isError=false`), dann *„Der
  Rückgabewert von nonce-1.txt fehlt..."*, 1/5 Tool-Starts. Ein einzelner
  gruener Lauf beweist bei nachweislich intermittierendem Verhalten keine
  geheilte Ursache. **T-961 bleibt `free`**, kein PASS, kein `done`.
- **Offen:** Intermittenz eingrenzen (Tool-Ergebnis-Rahmung in
  `conversation_prompt` variieren; Konto-Memory nur durch Eigentuemer
  einsehbar), dann mehrere stabile gruene Laeufe fordern. G-008 bleibt offen;
  G-009 (Tag/Release) bleibt offen. Belege: `docs/proofs/T-961/analysis.md`,
  `environment.txt`, `README.md`, `after-summary.txt`,
  `after-negative-intermittent-summary.txt`.

## 2026-11-20 — G-006 abgeschlossen: echte T-961-Belege (negativ)

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
  erfolgreichen Run — Anmeldung **nicht bestaetigt**. Kein Agent hat Zugangsdaten
  oder Profildateien gelesen.
- **Konsequenz:** T-961 bleibt `free`. Das ist ein Negativbeleg, keine Abnahme.
  Rezertifizierung erst nach Anmeldung durch den Eigentuemer
  (`webagent login --brain chatgpt`) und erneutem Lauf. G-006 ist als
  "Belege beschafft" done; G-008/G-009 bleiben offen.
- **PR #81 (T-949) gemergt:** Squash `0f80973`, alle CI-Checks gruen; lokaler
  `master` auf `origin/master`, danach dieser Handover-Stand committed.

## 2026-11-20 — T-949 (Branch `fix/T-949-consent-guard`)

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

## 2026-11-20 — G-008 Triage der restlichen freien Tasks

Einordnung der freien Tasks nach dem, was **ohne** Zugangsdaten/Profile und ohne
Live-Anbieter-Sitzung ehrlich abgeschlossen werden kann. Diese Triage ist keine
Abnahme und kein Defer-Beschluss; sie bereitet die Entscheidungsvorlage fuer den
Eigentuemer vor (G-008 bleibt offen, G-009 bleibt offen).

| Kategorie | Tasks | Begruendung |
|---|---|---|
| A — lokal umgesetzt + geprueft | T-949 | Code-/Mock-DOM-Beleg, PR #81 offen; nach CI-Fix (Referenz-Banner im Proof) erneut in CI. |
| B — lokal umsetzbar, noch offen | T-926, T-940, T-946, T-953, T-957 | DoD mit `cargo test --lib` und/oder statischem Beleg erfuellbar; keine Live-Sitzung noetig. Kandidaten fuer weitere Scheiben. |
| C — braucht echte Live-Sitzung / Anmeldung | T-927, T-928, T-948, T-961 | DoD verlangt frische Live-Matrix-/Effort-Belege, einen echten mistral-Zustimmungsdialog bzw. rohe Pi-Transkripte vor/nach Fix. Integrator liest keine Zugangsdaten und keine Profile. T-961-Lauf siehe G-006. |
| D — teils Live-Messung noetig | T-942, T-944, T-950 | DoD verlangt zusaetzlich eine Messung je Anbieter (Quell-Markdown gegen ausgelieferten Text, Denkmodus-Turn, Websuche-Zustand vor dem Senden); nicht rein lokal belegbar. |
| E — explizit deferred | T-914 | Im Taskboard bereits `[DEFERRED]` markiert; kein aktiver Auftrag. |

Konsequenz: autonom weiter bearbeitbar sind Kategorie B; Kategorie C/D benoetigen
entweder eine Anmeldung durch den Eigentuemer oder einen ausdruecklichen
Defer-Beschluss. Ein solcher Beschluss wird nicht stellvertretend erfunden.

## 2026-11-20 — Zielstack G-005..G-007 (autonomer Fortschritt)

- **G-005 done:** PR [#80](https://github.com/st0rax/webagent-rs/pull/80) wurde als Squash-Commit `493e9a613d72dcf319151b813ed7ffc84662c657` in `master` gemergt; lokaler `master` ist auf `origin/master` fast-forwarded und war bei Beginn dieser Scheibe sauber.
- **G-006 in_progress:** Echte T-961-Live-Messung wurde vorbereitet und der Vor-Fix-Lauf ausgefuehrt. Frisch gebaute Binaries: `master` (`493e9a6`, `webagent 0.11.3`) und isolierter Worktree `dd78128` (`webagent 0.11.3`). Pi ist installiert als **1.5.2** (nicht mehr die im alten Plan genannte 0.84.4). Die Bridge startet auf `127.0.0.1:8788` mit frischem, nicht gespeichertem Prozess-Token; Port 8787 bleibt bewusst unberuehrt (dort laeuft ein fremder Free-Router-Prozess).
  - Vor-Fix-Rohtranscript: `C:\Users\storax\AppData\Local\Temp\webagent-t961-harness\before.jsonl` (29.021 Bytes, frei von Token-/Cookie-/Key-Mustern). Beide Pi-Turns endeten mit `502: Browser-Inference fehlgeschlagen: session_state=Unbestimmt`; `tool_execution_start` = 0/5. Der Circuit Breaker fuer `chatgpt` waren daraufhin 900 s offen.
  - Wichtiger Befund: Das Repo-Skript `scripts/test-pi-provider.ps1` erkennt `PI_BRIDGE_OK` im echoenden User-Prompt und meldet den Textturn daher falsch als erfolgreich. Die temporaere Harness-Kopie prueft jetzt nur noch die Assistant-Nachricht. Live-Eingaben (Prompts, Modell, Port) blieben unveraendert.
  - Der Nach-Fix-Lauf wurde nach Ablauf des Breakers ausgefuehrt; echte rohe Belege und der Kontozustand sind inzwischen committed (siehe Abschnitt G-006 oben). Ergebnis negativ, T-961 bleibt `free`.
- **G-007 done:** PR [#69](https://github.com/st0rax/webagent-rs/pull/69) wurde am 20.11.2026 19:32 UTC bewusst als **ueberholt geschlossen**, nicht gemergt. Sein Code (Systemkontext, `content:null` + `tool_calls`, Tool-Fortsetzung) liegt bereits via PR #78/`864e346` und PR #80/`493e9a6` in `master`; gegenueber `master` verblieben nur T-934-Referenzbelege. Die offen gebliebenen Live-Gates auf dem veralteten Head-SHA `f361402` wurden nicht fabriziert.
- **Offen:** G-006 Nach-Fix-Ergebnis, G-008 Triagierung der restlichen 14 freien Tasks, G-009 Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-11-20 — T-966: Acceptance integrity (Arbeitsstand)

- **Status (historisch zum Zeitpunkt der Arbeit):** Branch `fix/T-966-acceptance-integrity` (abgezweigt von `master` bei `eaac398`) wurde gepusht; PR [#80](https://github.com/st0rax/webagent-rs/pull/80) war zu diesem Zeitpunkt offen. Zwei Commits: `cac7c29` (Taskboard/Gates), `b0628a2` (Belege/Doku). **Inzwischen gemergt** als `493e9a6`, siehe Abschnitt oben.
- **Ausgangslage:** `GOALS.md` behauptete „Projekt abgeschlossen“ und G-002 done, obwohl `docs/TASKBOARD.json` auf einen 1-Task-Stub reduziert war, `docs/proofs/T-961/environment.txt` `BLOCKED_BY_MISSING_EVIDENCE` meldet und PR #69 offene Live-Belege hat.
- **Korrigiert:**
  - Vollständiges Taskboard aus `dd78128` wiederhergestellt (91 Tasks, 77 done, 14 free); T-961 bleibt `free`. Kein History-Rewrite, kein Force-Push.
  - `docs/TASKBOARD.md` um einen maschinenlesbaren Spiegelblock ergänzt; `src/taskboard.rs` prüft Taskzahl, IDs und Spiegelabgleich im normalen `cargo test` und damit auch in CI.
  - T-961-Belege bereinigt: keine PASS-Behauptung, keine nicht existierenden SHAs, `before.jsonl`/`after.jsonl` als ausdrückliche `not_run`-Platzhalter.
  - Statusdokumente auf eine ehrliche Version gebracht; `README.md` nennt den realen Tag `v0.11.4` mit Hinweis auf die offene Abnahme.
- **Gates (20.11.2026, Windows/GNU, Rust 1.100):**
  - `cargo fmt --all -- --check` ✅
  - `git diff --check` ✅
  - `cargo clippy --locked --no-default-features --all-targets -- -D warnings` ✅ (nur zwei Cargo-Manifest-Hinweise zu ungenutzten optionalen Deps ohne `webview`; keine Rust-Warnungen)
  - `cargo test --locked --no-default-features` ✅ — Lib 1448 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
  - Negativprobe: `docs/TASKBOARD.md` wurde absichtlich auf `| T-001 | free |` geändert; `taskboard_invarianten_halten` schlug mit „nicht mit docs/TASKBOARD.json synchron“ fehl und die Datei wurde sofort wiederhergestellt.
  - Ein erster Testlauf scheiterte an einer veralteten lokalen Linker-Umgebungsvariable sowie einem fehlenden C-Compiler; beides war Werkbank-Setup, kein Repo-Code. Die Defensive gegen Ersteres (`is_test_run`) ist jetzt Teil des Fixes.
  - `cargo clippy --locked --all-targets -- -D warnings` (Default-Features/WebView) ✅
  - `cargo test --locked` (Default-Features/WebView) ✅ — Lib 1454 passed / 0 failed / 1 ignored, Bin 7 passed, Doc 0
- **GitHub-CI auf PR #80:** alle Checks grün in beiden Läufen (Push + PR-Event): `build & test`, `build & test (Windows, volle Features)`, `build (Linux, WebView via WebKitGTK)`, `aarch64`.
- **PR #69** ([Kommentar](https://github.com/st0rax/webagent-rs/pull/69#issuecomment-7048051133)): bewusst nicht gemergt, weil der PR selbst Live-Pi-Re-run und generisches curl/SDK Chat+Tools als `pending` führt.
- **Offen / nicht übernommen:** Echte Live-Rezertifizierung (Eigentümer muss anwesend sein; ein Agent liest keine Zugangsdaten oder Profile), PR #69 mit `pending`-Live-Belegen, kein neuer Tag/Release ohne Freigabe.
- **Nächste Aktion (damals):** Integrationsentscheidung zu PR #80 durch den Eigentümer. PR #80 ist inzwischen gemergt; T-961 bleibt `free`, bis echte rohe Pi-Transkripte vor/nach dem Fix und eine Kontozustandsprüfung vorliegen. Tag/Release erst nach vollstaendiger Abnahme und Freigabe.

## 2026-10-15 — T-965: English-first maintained documentation

- **Status:** `done`; PR [#76](https://github.com/st0rax/webagent-rs/pull/76) merged.
- **Merge commit:** `3778f31a83535690e3746501cd642c46b873c70e`.
- **Branch:** `docs/T-965-english-first-docs`; task status is `done` in `docs/TASKBOARD.json` and mirrored in `docs/TASKBOARD.md`.
- **Policy:** English is the default for maintained documentation, task titles, agent instructions, issues, and pull requests. Preserve dated historical reports/evidence when rewriting would damage traceability; add English summaries where useful. Product-facing output language remains a separate runtime decision.
- **Changed scope:** `README.md` and `docs/NEWCOMER_GUIDE.md` are now in English; the README retains the quick start, architecture/module map, commands, profile and environment-variable guidance, release/platform boundaries, and historical provider context. The policy and T-965 status are recorded in the taskboard and handover.
- **Non-goals:** no Rust code, runtime behavior, release, tag, login-profile, or provider configuration changes. This does not claim that every maintained or historical document in the repository has already been translated; remaining maintained docs should be migrated in follow-up tasks.
- **Verification:** PR #76 merged; `docs/TASKBOARD.json` parsed successfully; README and newcomer-guide section headings are English; links to existing Markdown files and the README quick-start anchor were checked through GitHub. Local `git diff --check`, automated link-check tooling, and Rust tests were not run in this remote editing session.
---


## 2026-10-15 — T-964: Kontextfreier Repository-Einstieg abgeschlossen

- **Status:** `done`; PR [#73](https://github.com/st0rax/webagent-rs/pull/73) gemergt.
- **Merge-Commit:** `76ba3b303d78135e8927b2bd7f1cd382732fdc8e`.
- **Branch:** `docs/T-964-newcomer-onboarding`; Taskboard-Abschluss in [PR #74](https://github.com/st0rax/webagent-rs/pull/74) dokumentiert.
- **Einstieg:** [`docs/NEWCOMER_GUIDE.md`](NEWCOMER_GUIDE.md) ist die zentrale Landkarte für neue Menschen und Coding-Agenten; die README verlinkt sie sichtbar und trennt Benutzung von Entwicklung.
- **Quellenklarheit:** Guide erklärt Projektmodell, Dokumentenrollen, Architekturbegriffe, historische vs. aktuelle Informationen, Bazaar-Claim und Sicherheitsgrenzen.
- **Scope:** nur README, Einstiegsdokument und Taskboard/Handover; keine Rust-Code-, Laufzeit-, Login-Profil-, Tag- oder Release-Änderungen.
- **Verifikation:** Taskboard-JSON geparst; relevante Guide-Zielpfade auf GitHub geprüft; Taskboard-Spiegel enthält T-964 und den Guide-Link. `git diff --check` und Rust-Gates wurden nicht lokal ausgeführt; für reine Dokumentation wurden keine Rust-Tests gestartet.


## 2026-10-15 — T-963: Bazaar-konforme Integration abgeschlossen

- **Status:** `done`; PR [#72](https://github.com/st0rax/webagent-rs/pull/72) gemergt.
- **Merge-Commit:** `23ccca28dfd856d2bc90bcc9744aae136d4866b9`.
- **Branch:** `docs/T-934-bazaar-agent-prompt`; Taskboard-Claim und Handover sind dokumentiert.
- **Prompt:** [`docs/PI_INSPIRED_CODING_AGENT.md`](https://github.com/st0rax/webagent-rs/blob/master/docs/PI_INSPIRED_CODING_AGENT.md).
- **Repo-Workflow:** T-963 steht in `docs/TASKBOARD.json` (Quelle der Wahrheit) und `docs/TASKBOARD.md` (Spiegel). Der vorhandene Bazaar-Arbeitsprozess bleibt erhalten.
- **Scope eingehalten:** nur Dokumentation und Taskboard; keine Rust-Code-, Laufzeit-, Browserprofil-, Login-, Tag- oder Release-Änderungen.
- **Verifikation:** Taskboard-JSON erfolgreich geparst; Taskboard-Spiegel und Handover geprüft; keine abschliessenden Leerzeichen in den vier geprüften Dateien. Lokales `git diff --check` und Rust-Tests wurden in dieser Remote-Umgebung nicht ausgeführt.

---

> **Aktualisiert 2026-09-12:** **G-001 abgeschlossen** — alle Zeilen der
> v1.0-Definition-of-Done verifiziert (`docs/proofs/G-001/g001-abschluss-2026-09-12.json`).
> - Voll-Gates am Abschluss-Stand (`2589b0a`) erneut grün: `cargo fmt --all -- --check`,
>   Clippy `-D warnings` headless **und** full-features, `cargo test --locked`
>   **1388 passed / 0 failed / 1 ignored + 7 bin**, `cargo test --locked
>   --no-default-features` 1382 + 7 bin, `git diff --check` sauber.
> - Toolchain-Norm auf rust 1.97 (`2589b0a`): rustfmt-Drift über 13 Dateien
>   bereinigt, sechs neue Clippy-Lints behoben (u. a. `items_after_test_module`,
>   `single_match`, tote Konstante `MEMORY_CONTEXT_LIMIT`, nie gelesenes `wiki`-Feld).
> - **Release `v0.11.3` veröffentlicht** —
> https://github.com/st0rax/webagent-rs/releases/tag/v0.11.3 (Tag `7021959`,
> CI `34696249277` success, Assets: Windows exe + WebView2Loader.dll + sha256,
> Linux, Android aarch64):
> - **Web-UI vom T-203-Fake-Prototyp auf die echte `/api/*`-Anbindung gehoben**
>   (Phase-9 T-901–T-906 done):
> - T-905 (`664f4e8`): Brain-Status, Sessions anlegen/wechseln, Chat-Stream
>   (`events?since=`), Stop, Quellen, Gruppen, Brain-Detail, Upload, Brain-
>   Fenster show/hide; Layout im T-203-Grok-Schema, Einzeldatei < 48 KiB,
>   Tests erzwingen `fetch(`-Anbindung.
> - T-906 (`5c42656`): T-905-Restlücken geschlossen — Gruppenlauf öffnet die
>   erzeugte Session (`run_id`) und rendert die Events live (Runden-Stati,
>   TextDeltas, Synthese, Done), Datei-Upload im Composer
>   (`POST /sessions/{id}/upload`, 202 `accepted`), Brain-Fenster show/hide im
>   Detail; Beweis `docs/proofs/T-906/`.
> - **Code-Review (`44f7a1e`):** reviewer-Bot REQUEST_CHANGES → alle 11 Befunde
>   umgesetzt: XSS-Klasse behoben (echtes `escRow`, DOM-Selects, CSP-Meta),
>   `TextComplete`-String-Handling (live belegt: Backend sendet JSON-String),
>   Poll-Race-Guard, Gap-/404-Recovery, 409-`session_done`-Text, Backoff,
>   ehrlicher Upload-Text. Beweis `docs/proofs/T-906/webui-review-2026-09-12.json`.
> - Gates für 0.11.3: **1388 passed / 0 failed / 1 ignored**; `cargo check` grün;
>   Changelog `[0.11.3]` finalisiert; Beweise T-901..T-906 in `docs/proofs/`.
> - T-805/T-806/T-807 abgenommen: 46 frische Live-Verify-Belege (alle 9 Brains)
>   in `docs/proofs/T-501/live_verify_2026-09-12.json`, Matrix `as_of 2026-09-12`.
> - T-806 Receipt-E2E live belegt (negativ fail-closed + positiv done,
>   `docs/proofs/T-806/`); T-807 Windows-Prozessprobe + Release-Binary-E2E
>   (`docs/proofs/T-807/`), alle 26 Tasks im TASKBOARD `done`.
> - Release-Fix im CI: `build.rs` linkt `resource.o` nur noch unter GNU/mingw
>   (MSVC-CI nutzt winres `resource.lib`) — behebt `LNK1181` im Release-Workflow.
> - Gates: `cargo test --lib` 1388 passed / 0 failed / 1 ignored;
>   `--features tui` und `--no-default-features` grün.
> - perplexity als Custom-Brain registriert (`custom_brains.json`, `%LOCALAPPDATA%\\webagent\\data`) — diagnose/verify erreichbar
> - deepseek model/mode_switch removed by-design (Modus-Bar vom Anbieter entfernt)
> - effort-Spalte komplett vermessen: claude/qwen passed, alle anderen not_run/failed
>   (kimi: Pfad `['Denkaufwand Hoch','Hoch']` klickt, verify-Failed strukturell)
> - gemini/kimi/zai model_switch Frischbelege, chatgpt model_switch failed (Sofort-Pill)
> - chatgpt.json reasoning_effort_menu entfernt (komplett deckungsgleich mit model_menu)
> - Matrix-Stand: **130 Zellen, 10 Brains × 13 Areas** vollständig, `as_of` 2026-09-11
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
**Zweck:** verbindlicher Wiedereinstieg und operative Wahrheit. Historische Befunde stehen in `docs/OVERVIEW.md` sowie in den datierten Übergaben; diese Datei ersetzt sie nicht, sondern hält nur den aktuellen Abschlusspfad fest.

**Stand 2026-09-05:** `master` auf `9793623` (PR #33 gemergt: Port-Vereinheitlichung „Ein Listener, zwei Rollen“ — Web-UI + API-Bridge gemeinsam auf `8788`, `api serve` als Alias; `8787` ist historische Dead-Zahl; Details [`CLI_UI_REDESIGN.md`](CLI_UI_REDESIGN.md) §8; `a934db2`/PR #32 `ask` war der Vorgaenger-Commit). **T-501-Nachzieh-Runde 2026-09-05 auf Port 8788** (Breaker-Cooldowns abgelaufen, Bridge mit echtem Login-Profil-Root): `api_responses` jetzt **9/9 passed** (mistral+zai nachgeholt; mistral-Deltas Provider-Timestamps, finale `.done`-Antwort sauber RESP_OK), `streaming` bleibt **6/9** (kimi Reasoning-Echo, mistral Timestamp, zai „Thinking...“), `attachment` 5 Brains frisch gemessen. **Nachtrag `attachment kimi` (2026-09-05, Fokus laut Eigentuemerentscheidung):** Der fruehere 502 `ABSENDEKNOPF_DEAKTIVIERT` lag am entarteten 1x1/70-Byte-Testbild, nicht an der Bridge - mit echtem 256x256-Bild (PIL, ~1,1 kB) laeuft der komplette Upload->Send->Antwort-Pfad durch: **kimi attachment jetzt `passed` (200 "RED", 36,6 s, frische Session)**. qwen/zai bleiben `failed` (auch mit echtem Bild "0 von 1 Dateien": echter SPA-Upload-Gap, Re-Run-Belege `attachment_{qwen,zai}_real_image_2026-09-05.json`), mistral Timestamp, auto Timeout 240s. `managed_tools` by-design 400 erneut bestaetigt. Matrix-Gesamtstand **98/130 passed, 17 failed, 6 unreachable, 9 not_run**; T-501 im TASKBOARD auf `claimed` zurueckgestellt (DoD weiterhin NICHT done: streaming 3, attachment 4, model/effort login-gebunden). **Web-UI-Feature (2026-09-05):** `web/index.html`-Composer zeigt bei leerer Eingabemaske einen Beispiel-Hinweis (`/quelle list`, `/quelle ... --save`, `@brain`) wie die opencode-Promptleiste; verschwindet beim Tippen (`syncHint()`), wirkt nach Neubau (eingebettet via `include_str!`). Belege `docs/proofs/T-501/*2026-09-05.*`, Harness `docs/proofs/T-501/rerun_2026-09-05.py`. Alle aelteren `127.0.0.1:8787`-Erwaehnungen in `docs/proofs/*` bleiben historische Belege und werden nicht angefasst. **Login-Realitaet (Messung 2026-09-05, DeepSeek zuerst):** isolierter No-Login-Smoke via `WEBAGENT_ROOT`-Temp-Root (frisches, leeres Profil; Breaker-Zustand im Temp — echter Betrieb unberuehrt): `webagent ask --task "Antworte exakt und nur: DS_NOLOGIN_OK" --brain deepseek --chat --json --headless --no-memory` → `reason=session_state=LoginRequired`, `latency_ms=47561`, deterministische Breaker-Sperre 21600s (nur im Temp-Root), Exit 1. **Befund: deepseek funktioniert ohne Login NICHT** — bestaetigt den frueheren Eintrag in `CLI_UI_REDESIGN.md` §2 P2; der README-Login-Claim („angemeldeter Browser bei dem Dienst, den du nutzen willst“) bleibt damit korrekt neutral formuliert. Protokoll: `%LOCALAPPDATA%\Temp\opencode\wa-nologin\deepseek-smoke.log`.

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
Zusätzlich akzeptiert die Modellauflösung neben den kanonischen
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
mit HTTP 200 in 20,7 s. Die Oberfläche stellte dem Marker den UI-Titel
`Thought Process` voran; die API-Schicht entfernt diesen Titel jetzt
konservativ nur für Z.ai und lässt den eigentlichen Antworttext unverändert.

Ein ZCode-kompatibler Streaming-Smoke gegen `webagent/chatgpt` lieferte HTTP
200, Keep-Alive-Frames, inkrementelle `delta.content`-Chunks und einen
abschließenden `finish_reason: stop` (18,4 s).

## Aktueller Fix: multimodaler Upload-Fallback für dynamische Browser-UIs

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`

Die bisherige Bild-/Audio-Strecke setzte ausschließlich ein bereits vorhandenes
`input[type=file]` voraus. Genau das führte bei DeepSeek, Gemini, Kimi und Mistral
zu `no_file_input`, sobald eine alte Bildnachricht im ZCode-Verlauf mitgesendet
wurde. `browser::send::attach_files` öffnet jetzt zunächst vorsichtig die
konfigurierte Attach-Oberfläche (untrusted DOM-Klick), wartet auf ein dynamisch
gerendertes Datei-Input und fällt danach auf Paste/Drop zurück. Der Fallback gilt
nur bei einer sichtbaren neuen Attachment-Vorschau als erfolgreich; andernfalls
kommt weiterhin ein erklärender 502. Native Dateidialoge werden nicht geöffnet.

### Korrektur zum Providerbefund vom 31.08.2026

Die frühere Formulierung „kein Datei-Upload“ war für DeepSeek, Gemini, Kimi und
Mistral zu stark: Eine frische DOM-/Screenshot-Inventarisierung zeigt bei allen
vier eine Upload-Oberfläche. DeepSeek rendert ein verstecktes
`input[type=file]` und eine Büroklammer; Gemini erzeugt nach „Uploads & Tools“
versteckte Inputs; Mistral rendert `input[name=file-upload]`; Kimi zeigt die
Toolkit-/+-Schaltfläche und Attachment-Zustände. `no_file_input` war dort ein
Entdeckungsfehler eines vollständigen Verlaufs, kein Beleg für fehlende
Providerfunktion.

Der Windows-Treiber versucht deshalb neben dem bestehenden Paste/Drop-Weg einen
nativen WebView2-CDP-Upload (`DOM.setFileInputFiles`) und akzeptiert sowohl die
direkte WebView2-Antwortform als auch den üblichen `result`-Wrapper. Dieser Weg
wurde live mit DeepSeek (23,33 s), Gemini (26,06 s), Kimi und Mistral als Bild-Request bis
zur Antwort `IMAGE_INPUT_OK` verifiziert. Kimi verwendet dafür eine echte
Windows-Dateiübergabe (`CF_HDROP`) mit abgefangenem WebView2-Dateichooser und
den realen Pfeil-Button seines Lexical-Composers. Ein Submit gilt erst als
bewiesen, wenn Kimi den Composer tatsächlich konsumiert hat. Audio wurde mit
Gemini anhand einer synthetisierten WAV-Sprachphrase inhaltlich verifiziert.

Die generische Selektormaske enthält dafür providerneutrale Attach-Signale
(`aria-label`, `data-testid`, `data-tooltip`, Datei-/Upload-Beschriftungen). Die
Text-Circuit-Breaker-Behandlung klassifiziert sowohl `no_file_input` als auch
`no_file_input_and_paste_not_confirmed` als request-spezifische Fähigkeitsgrenze;
Textturns eines betroffenen Brains bleiben davon unberührt.

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

ChatGPT-Bildausgabe ist inzwischen als eigener Headless-Pfad über
`POST /v1/images/generations` implementiert. Ein extrahiertes Resultat wurde als
808.492-Byte-PNG mit korrekter Signatur und 1254x1254 Pixeln dekodiert und
visuell geprüft. Weitere Gegenproben deckten zwei False-Positive-Quellen auf und
schlossen sie: das 104x104-Toolicon wird durch die nach Toolaktivierung
aufgenommene Baseline ausgeschlossen; alte Estuary-Bilder bleiben anhand ihrer
stabilen `file_...`-ID ausgeschlossen, auch wenn ChatGPT `ts` und `sig` ändert.
Der aktuell folgende atomare Neu-Generierungs-Smoke scheitert ehrlich am
providerseitigen Free-Plan-Kontingent (Reset laut UI am 01.09.2026 um 16:57)
und wird nicht als Erfolg gewertet. Für Gemini wurde der zuvor fehlende
provider-spezifische Pfad ergänzt: „Uploads & Tools“ wird nach dem asynchronen
`new_chat()`-Remount per trusted CDP-Pointer geöffnet und „Bild erstellen“
aktiviert. Headless waren Moduswechsel („Bilder“), Submit und alternierende
Renderer-Wakeups belegt; Gemini blieb trotzdem bis zum absoluten 180- bzw.
300-Sekunden-Budget bei „Creating your image“, ohne neues Bild im DOM. Gemini
wird daher noch nicht mit Bildausgabe beworben. Alle anderen Brains melden
weiterhin nur Textausgabe. Eine Behauptung „100 % API-Kompatibilitaet fuer alle
Brains“ ist mit diesem Stand ausdrücklich nicht zulässig.

Die anschließende headless Werkzeugmenü-Vermessung aller übrigen Profile fand
keinen weiteren Bildgenerator: Claude und Qwen bieten Datei-Upload, Mistral
einen Medien-Upload einschließlich verbreiteter Audioformate; DeepSeek, Kimi,
Perplexity und Z.ai zeigten keinen Bild-/Audio-Generator. Damit sind
`/v1/images/generations`-Backends aktuell nur für ChatGPT und Gemini sachlich
begründet; Claude bestätigt offiziell, dass der Chat nur textbasierten Output
liefert und Bilder ausschließlich als Input verarbeitet.

Die bislang fehlenden eigenständigen OpenAI-Audiorouten sind anschließend
ergänzt worden. `/v1/audio/transcriptions` und `/v1/audio/translations` parsen
den offiziellen binären Multipart-Upload, routen ihn als Audio-Anhang an ein
Web-Brain und liefern `json`, `text` oder ein konservatives `verbose_json`.
Binärbytes, Dateiname, MIME-Typ, Modell und Format sind durch Regressionstests
abgedeckt. `/v1/audio/speech` existiert als ehrliche fail-closed Grenze: Keines
der aktuell vermessenen Web-Brains liefert einen belegten, extrahierbaren
TTS-Audiostream, daher werden dort keine synthetischen Audioantworten behauptet.
Ein echter headless Multipart-Smoke gegen `webagent/gemini` transkribierte die
neu synthetisierte Kontrollaufnahme „Die Prüfziffer lautet sieben acht neun“ in
23,8 Sekunden korrekt zu `{"text":"Die Prüfziffer lautet 789"}`. Ein erster
englischer Satz aus einer deutschen Windows-Stimme war phonetisch ungeeignet;
dieser Quellenfehler wird nicht als Provider- oder Endpoint-Erfolg gewertet.

## Aktueller Fix: große ZCode-Toolrequests im Browser-Provider

**Aktualisiert:** 2026-08-31
**Branch:** `feat/browser-inference-provider`
**Commit:** `7bd1881` (`fix: compact large browser tool prompts`)

Ein ZCode-Request mit 53 Werkzeugen und rund 124 KiB JSON reproduzierte bei
ChatGPT die Web-UI-Antwort „Something went wrong“. Die folgenden Claude- und
DeepSeek-Versuche liefen dadurch im Client in `ECONNREFUSED`, weil der lokale
Endpoint während des vorherigen Neustarts nicht erreichbar war; ein frischer
Textturn gegen beide Brains ist danach erfolgreich durchgelaufen.

`browser_inference::prompt_with_tools` kompaktisiert den Toolblock nun
stufenweise auf höchstens 64 KiB. Werkzeugnamen und Parameterschemata bleiben
vollständig erhalten, Beschreibungstexte werden nur bei Bedarf gekürzt.

| Gate | Ergebnis |
|---|---|
| Kompakter Unit-Test | bestanden |
| ZCode-Request mit 53 Tools, normal | HTTP 200, kein Web-UI-Fehler |
| Derselbe Request über SSE | HTTP 200, gültiges `data: [DONE]` |
| Claude nach dem ChatGPT-Request | HTTP 200, `CLAUDE_CHECK_42` |
| DeepSeek nach dem ChatGPT-Request | HTTP 200, `deepseek_OK` |
| `cargo clippy --locked --all-targets --no-default-features -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.186 bestanden, 1 ignoriert |

Der laufende Release-Endpoint verwendet die neue Binary auf `127.0.0.1:8787`
und antwortet auf `/health` mit `status=ok`.

Die Browser-Einleitung wurde anschließend neutralisiert: Sie beschreibt den
Inhalt als Gesprächsverlauf eines API-Clients und vermeidet sichtbare
`Provider-Bridge`-/`WEBAGENT_INFERENCE/1`-Identitätsdiskussionen. Der
Tool-Umschlag bleibt für echte Client-Werkzeuge strikt und fail-closed, ist aber
kurz als Client-Werkzeugliste formuliert. Das verhindert unnötige Claude-
Metaantworten, ohne providerseitige Sicherheitsablehnungen zu verändern oder
zu umgehen.

Die Fähigkeitsgrenze bleibt bewusst dokumentiert: Bild-/Audio-Eingaben werden
hochgeladen, die Ausgabe wird derzeit ausschließlich als Text extrahiert.
Generierte Bildartefakte aus der ChatGPT-Weboberfläche sind noch kein
API-`image`-Output.

Ein konkreter Fehlerbefund aus dem ZCode-Test wurde getrennt: DeepSeek, Gemini
und Kimi hatten wegen eines mitgeführten Bildes `no_file_input` geliefert. Dieser
Fähigkeitsfehler wird in `relay` jetzt nicht mehr als allgemeiner Brain-Ausfall
gezählt, nicht wiederholt und nicht in den Text-Circuit-Breaker übernommen. Ein
späterer text-only Turn bleibt dadurch möglich; die API meldet die
Multimodalitätsgrenze weiterhin ehrlich mit 502.

## Browser-Inference-Provider-Scheibe vom 29.08.2026

Die aktuelle Arbeit liegt isoliert auf `feat/browser-inference-provider`, ausgehend von `master` bei `a0cd00b7`. `api_bridge` startet nicht mehr den vollständigen `AgentController`, sondern ruft die neue harnessfreie Grenze `browser_inference::complete()` auf. Dadurch sind WEBAGENT/1, Shell-/Dateiaktionen, Controller-Memory und der Plan-Act-Observe-Loop nicht mehr Teil eines Providerrequests. Der bestehende Rust-Harness bleibt unverändert als separater Consumer erhalten.

Die Providertrennung liegt in `83bc90d`; die abgeschlossene Pi-0.84.4-Integration samt reproduzierbarem Tool-Smoke liegt in `c8b98ed`. Beide Commits gehoeren zum Branch `feat/browser-inference-provider`; diese Uebergabe wird als nachfolgender reiner Dokumentationscommit abgeschlossen.

Der OpenAI-Adapter normalisiert Function-Tools, `tool_choice`, Assistant-`tool_calls` und `role=tool`-Ergebnisse. Browsermodelle geben Tool Calls über einen strikten `WEBAGENT_INFERENCE/1`-Umschlag zurück; unbekannte Tools, doppelte IDs und verletzte erzwungene Toolwahl scheitern fail-closed. Die Bridge führt Tools ausdrücklich nicht selbst aus. Anthropic Messages bleibt in dieser Scheibe textbasiert; SSE bleibt gepuffert statt tokeninkrementell.

| Gate | Ergebnis dieser Scheibe |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | 1.170 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo clippy --locked --all-targets -- -D warnings` | bestanden |
| `cargo test --locked` | 1.209 Bibliotheks-, 7 Binaertests bestanden; 1 Test bewusst ignoriert |
| `cargo build --locked --release` | bestanden; `webagent.exe` 7.005.184 Bytes (6,68 MiB) |
| Lokaler API-Smoke | `/health=ok`, geschütztes `/v1/models=webagent/chatgpt`, kontrollierter Shutdown bestanden |
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
> mit dem aktuellen `master` (`505d416`) überholt. Aktueller Einstieg: der Kopf
> dieser Datei und `feature/docs-cli-ui-fixes`.

Die Arbeit läuft auf `task/v1-release-baseline`, ausgehend von `origin/master` bei `1d214e8`. Die erste abgeschlossene Baseline-Scheibe ist als `2659baf` (`fix: restore headless release baseline`) committet; der Scope-Freeze und die Definition of Done folgen in `4fdb068` (`docs: define v1 release completion`). Beide Commits sind auf `origin/task/v1-release-baseline` gepusht. Die Baseline-Scheibe stellte den browserfreien Releasezustand wieder her: fehlende Bibliotheksmodule wurden registriert, der Root-Eintrag in `Cargo.lock` wurde auf `0.11.1` korrigiert, reine Kernmodule wurden von unnötigen TUI-Gates entkoppelt, strikte Lint-/Testdrift wurde behoben und die alte Wilson-Dublette wird auf die gemeinsame Implementierung zurückgeführt.

Besonders wichtig ist die Korrektur von `capture_patch`: Eine globale Git-Einstellung wie `color.ui=always` lieferte ANSI-Sequenzen in `git diff` und machte die Scope- sowie Harvest-Prüfung blind. Der maschinell verarbeitete Patch wird nun explizit ohne Farbe erzeugt; reale Gegenproben auf einem temporären Git-Repository belegen In-Scope-Harvest und fail-closed Out-of-Scope-Ablehnung.

| Gate | Ergebnis auf dem aktuellen Arbeitsstand |
|---|---|
| `cargo fmt --all -- --check` | bestanden |
| `cargo clippy --locked --no-default-features --all-targets -- -D warnings` | bestanden |
| `cargo test --locked --no-default-features` | **1.157 Bibliotheks-, 7 Binärtests bestanden; 1 Test bewusst ignoriert** |
| `git diff --check` | bestanden |
| Reale Scope-/Harvest-Gegenprobe | bestanden; ANSI-farbige Git-Diffs werden sicher verarbeitet |
| `cargo clippy --locked --all-targets -- -D warnings` auf Linux/WebKitGTK | bestanden |
| `cargo test --locked` auf Linux/WebKitGTK | **1.196 Bibliotheks-, 7 Binärtests bestanden; 1 Test bewusst ignoriert** |
| Windows-CI `33001615724` auf `e1e5a74` | Formatcheck, Vollfeature-Clippy, Vollfeature-Build und Vollfeature-Tests vollständig bestanden |

## v1.0-Ziel und Definition of Done

> **v1.0 bedeutet nicht, dass jede Idee umgesetzt ist.** Webagent ist fertig, wenn der dokumentierte lokale Browser-Agent verlässlich gebaut, getestet, betrieben und anhand aktueller Evidenz freigegeben werden kann. Ein grüner Unit-Test oder ein abgeschlossener Teil-Goal reicht dafür nicht aus.

| Abschlussbereich | Verbindliches Abnahmekriterium |
|---|---|
| **Reproduzierbarer Kern** | Formatcheck, striktes Headless-Clippy und vollständige browserfreie Test-Suite sind auf dem Release-Candidate grün; der Lockfile-Stand ist konsistent. |
| **Plattformgrenze** | Die Vollfeature-Gates (`cargo clippy --all-targets -- -D warnings`, `cargo test`) sind auf Windows mit WebView2 oder durch nachvollziehbare Windows-CI-Evidenz grün. Linux bleibt ein unterstützter browserfreier Kern-/TUI-Pfad, ersetzt aber keine Windows-WebView-Abnahme. |
| **Providerfähigkeit** | Jeder in der Releasekonfiguration aktivierte Provider besitzt eine frische, nachvollziehbare Send-/Antwort-Evidenz oder ist bewusst deaktiviert und mit Ursache dokumentiert. Keine historische Erfolgsmeldung gilt als Live-Beleg. |
| **Mehr-Brain-Betrieb** | Poolstart, Heartbeat, Recovery, Profil-Lease, Write-back, bereinigbarer Shutdown sowie Continuation und Cross-Brain-Handoff sind systemisch belegt. Ein abgebrochener Lauf darf weder das Master-Profil beschädigen noch ungemeldete Klonreste hinterlassen. |
| **Harvest-Sicherheit** | Ein gültiger Patch wird nach Wiederanwendung erneut gebaut und getestet; Scope-Verstöße, Löschungen und nicht nachvollziehbare Patches bleiben fail-closed ausgeschlossen. |
| **Betriebsdokumentation** | `docs/OVERVIEW.md`, diese Datei, `CHANGELOG.md` und die Release-Checkliste stimmen mit Code, Tests und bekannten Grenzen überein. |
| **Releaseentscheidung** | Der Release Candidate ist auf einem sauberen Tree und über GitHub erreichbar; erst nach dokumentierter Review- und Eigentümerfreigabe folgen Merge, Tag und GitHub-Release. |

## Scope-Freeze

Der v1.0-Kritische Pfad umfasst ausschließlich die nachfolgenden Arbeiten. Jede Erweiterung, die keine Definition-of-Done-Zeile erfüllt, bleibt außerhalb des Releasepfads.

| Im v1.0-Scope | Bewusst nach v1.0 verschoben |
|---|---|
| Browserfreie Releasequalität, Windows-Vollfeature-Evidenz und CI-Reproduzierbarkeit | Terminal-Renderer und ARIA-zentrierte Vermittlungsschicht |
| Frische Provider-Rezertifizierung und klarer Umgang mit Drift bzw. Ausfällen | CDP-/AX-Spike über die heutige Browseranbindung hinaus |
| Shutdown-, Profil-, Lease- und Klonbereinigungshärtung | Reale Free-Cloud-Transportadapter oder kostenpflichtige Fallbacks |
| End-to-End-Abnahme für Pool, Continuation, Cross-Brain-Handoff und Harvest | Genius Council, zusätzliche Agentenrollen und größere UX-Erweiterungen |
| Dokumentierte Release-Candidate- und Freigabevorbereitung | Neue Provider, neue Persistenz- oder Protokollmechanismen ohne Abschlussbezug |

## Providerbereitschaft und Desktop-Grenze

Die aktiven Standardprovider sind aktuell `chatgpt`, `deepseek`, `kimi`, `gemini`, `qwen`, `claude`, `mistral` und `zai`. Die ausgelieferten Selektoren für alle acht Anbieter werden eingebettet, sind valides JSON und bestehen die beiden providerfreien Verträge `test_embedded_selectors_cover_all_brains_and_parse` sowie `all_configured_brains_have_selectors`. `perplexity` besitzt eine ausgelieferte Selektordatei, ist jedoch nicht Teil des aktuellen statischen `BRAIN_TABLE` und damit kein v1.0-Standardprovider.

Eine manuelle Browser-Gegenprobe auf ChatGPT im verbundenen Browser lieferte den erwarteten Antworttext `BEREIT.`. Sie ist **keine Webagent-WebView-Evidenz**, weil sie nicht aus dem vom Nutzer getesteten Desktop-Arbeitsordner stammt. Der anschließende DeepSeek-Aufruf leitete in diesem separaten Browserprofil auf `/sign_in` um; auch das ist kein Befund über die projektseitigen Profile.

Der vom Nutzer getestete Windows-Desktop-Arbeitsordner ist in dieser Sitzung nicht erreichbar: Der Sandbox-Desktop enthält keinen Webagent-Ordner, die Browserverbindung blockiert lokale `file:///`-Pfade, der Remote enthält keinen neueren Desktop-Commit und es ist keine Desktop-/Computer-Verbindung konfiguriert. Das ist ein technischer Zugriffsblocker, keine fehlende Bereitschaft des Nutzers und keine Providerbewertung. Bis zu einem erreichbaren Desktop-Stand bleiben echte WebView-, Profil- und Providerbelege offen.

## Linux-WebView-Abnahme

Die Vollfeature-Abnahme auf Linux legte eine echte Konfigurationsregression offen: Die vorherige Headless-Bereinigung hatte `CloneGuard::path` entfernt, obwohl der WebView-Pfad sie bei der Runtime-Übergabe benötigt. Zusätzlich war `browser_args` unter WebKitGTK ungenutzt und zwei Linux-Stubs lagen hinter dem Testmodul. Commit `d32b8d0` grenzt die Windows-Argumente sauber ein, stellt den WebView-gateten Zugriff wieder her und ordnet die Plattformstubs lintkonform vor den Tests an. Nach Installation der notwendigen lokalen GTK-/WebKit-Entwicklungspakete sind die strikten Headless- und Vollfeature-Lints sowie beide vollständigen Test-Suiten grün.

Diese Evidenz bestätigt den Linux-WebKitGTK-Build und seine Tests. Ergänzend hat die Windows-CI `33001615724` auf dem aktuellen Branch die Vollfeature-Gates vollständig bestanden. Beide Belegketten ersetzen jedoch keine Live-Sitzungs- oder Providerabnahme auf dem tatsächlichen Desktop-Arbeitsstand.

## Betriebshärtung: aktueller Stand

Die beiden historischen Betriebsbefunde sind im gegenwärtigen Quellstand bereits strukturell adressiert. `main::run` ruft vor jedem regulären Ablauf `sweep_stale_runtime_profiles()` auf; die Bereinigung betrachtet ausschließlich die Wegwerf-Wurzeln `swarm` und `encapsulated`, verlangt eine lesbare Altersinformation und lässt kanonische Login-Profile bewusst unangetastet. Der gezielte Test `test_sweep_stale_runtime_profiles_spares_fresh_and_logins` belegt genau diese Grenze.

Für den kontrollierten TUI-Exit existiert `--run-secs`: Die Deadline führt in denselben Cleanup-Pfad wie die Taste `q`. Dieser schreibt `PoolControl { stop: true }`, joint den Worker-Thread und ruft anschließend `BrowserPool::shutdown_with_result()` auf, damit der geordnete Tab-Teardown und ein möglicher Write-back stattfinden. Der Worker-Pool belegt mit `kill_all_children_is_idempotent_and_empties_the_pool`, dass der Stopvorgang wiederholbar und vollständig ist. Eine echte Windows-WebView-/Write-back-Gegenprobe bleibt jedoch Teil der späteren Systemabnahme; der browserfreie Test beweist nicht die vorhandenen Desktop-Sitzungen.

## Kritischer Pfad und Blocker

Die nächste technische Scheibe ist die **Provider-Rezertifizierung**. Vor dem Live-Lauf werden die bestehenden Selektoren, Capability-Proofs und Fehlerbilder lokal inventarisiert. Die eigentliche Messung darf nur stattfinden, wenn der Eigentümer anwesend ist: Browserfenster können sich öffnen, aber Anmeldungen, Einmalcodes, CAPTCHAs und Zugangsdaten verbleiben vollständig beim Eigentümer. Der Integrator protokolliert ausschließlich die beobachteten Zustände und nutzt keine Geheimnisse.

Der aktuell bekannte spezifische Providerrest ist `zai`: Die Antwort kann einen `Thought Process` voranstellen; ein früherer Überlastungszustand verhinderte die belastbare Gegenprobe. Ein Provider mit frischer, nicht klassifizierbarer Antwort wird nicht still als releasebereit bewertet. Für alle Provider gilt dieselbe Regel: frische Evidenz oder dokumentierte, bewusst deaktivierte Grenze.

| Abhängigkeit | Status | Umgang |
|---|---|---|
| Eigentümer für Live-Browser/Anmeldung | erforderlich | Vor der tatsächlichen Live-Rezertifizierung gezielt um Freigabe bitten; bis dahin nur lokale Vorbereitung. |
| Windows-WebView2-Gates | CI bestanden, Live-Abnahme offen | Windows-CI `33002148599` ist vollständig grün; reale WebView2-Sitzungen und Profile bleiben separat zu prüfen. |
| Merge | abgeschlossen | Die geprüften Baseline- und Plattformkorrekturen wurden per Fast-Forward nach `master` integriert. |
| Tag/GitHub-Release | offen | Erst nach vollständiger Live-Evidenz und Eigentümerfreigabe vorbereiten; keine Veröffentlichung vorab. |

## Nächste sichere Aktion

Die nächste sichere Aktion ist die **Live-Abnahme am erreichbaren Desktop-Arbeitsstand**: je aktivem Provider denselben harmlosen Sende-/Antwortpfad prüfen, Zustände klassifizieren und die Windows-WebView2-/Write-back-Gegenprobe dokumentieren. Der aktuelle Windows-Desktop-Arbeitsordner ist aus dieser Sitzung nicht erreichbar; bis eine erreichbare Verbindung besteht, werden weder unbefugte Logins noch Ersatzsitzungen als Projektevidenz verwendet.

## Übergabe

- **Branch:** `master`; G-001-Abschluss-Stand `2589b0a`, Release `v0.11.3` (Tag `7021959`) veröffentlicht.
- **Abgeschlossene Scheiben:** `2659baf` (Headless-Baseline), `4fdb068` (v1.0-Definition), `d32b8d0` (Linux-WebView-Feature-Gates), `f51acd6` (Linux-/Windows-Abnahmestände), Phase-9 `T-901..T-906` inkl. Web-UI-Review-Fixes (`44f7a1e`), Release `v0.11.3`, Toolchain-Norm `2589b0a`.
- **Eigentümerschaft:** aktueller Integrator hat den v1.0-Abschluss bearbeitet; Live-Anmeldungen bleiben beim Eigentümer.
- **Externe Freigaben:** für Release `v0.11.3` erteilt (Eigentümerwahl „Erst README/Bild, dann Release"); Tag und GitHub-Release sind erfolgt.
- **Arbeitsbaum:** nach diesem Übergabecommit erneut prüfen; `master` direkt pushen, keine History umschreiben.

> **Hinweis zur Wahrheitspflege:** Historische Live-Befunde aus August 2026 sind nützlich für die Diagnose, aber kein aktueller Verfügbarkeitsbeleg. Maßgeblich für v1.0 sind frische, reproduzierbare Belege am Release Candidate — für `v0.11.3` in `docs/proofs/G-001/g001-abschluss-2026-09-12.json` zusammengeführt.
