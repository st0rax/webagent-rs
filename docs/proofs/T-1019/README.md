# T-1019 — Provider-Chat fortsetzen statt Gesamtkontext neu zu rahmen

**Status:** done (Code-Commit `89605ba`, Dokumentations-Commit folgt)
**Datum:** 2026-11-27
**Branch:** `feature/T-1019-continue-provider-chat`

## Auftrag und Grenze

Ohne Flag bleibt die bisherige Semantik unverändert: `/v1/chat/completions`
rahmt ab Turn 2 den gesamten `messages`-Verlauf als `[du]`/`[user]`/`[tool]`
in einen Prompt und startet pro Relay-Versuch ein frisches Provider-Chat.
Mit `WEBAGENT_CONTINUE_PROVIDER_CHAT=1` geht nur die aktuelle Nachricht in den
Provider-Chat; der Verlauf bleibt beim Anbieter bzw. im fortbestehenden
Shared-Browser-Tab. Kein Kürzen, Zusammenfassen oder Umformulieren des
Nutzerprompts. Kein Tag, GitHub-Release oder Versionssprung.

## Umsetzung

| Datei | Änderung |
|---|---|
| `crates/webagent-core/src/config/brains.rs`, `config/mod.rs` | Flag `continue_provider_chat()`, Default `false`, Test für `1`/`off` |
| `crates/webagent-api/src/api_bridge/content.rs` | `conversation_prompt_with_continuation()`; im Modus nur aktuelle Nachricht, inkl. Tool-Label |
| `crates/webagent-api/src/relay.rs` | `max_relay_turns()` → 1 Versuch im Modus; `skip_provider_new_chat()` überspringt `new_chat()` nur bei nachgewiesener Konversation |
| `crates/webagent-core/src/brain.rs`, `crates/webagent-transport/src/browser/backend.rs` | Neue Trait-Methode `relay_conversation_active()`; Shared-Browser meldet `/c/…` als fortsetzbar |
| `crates/webagent-transport/src/browser_pool.rs` | Vorhandenen Shared-Tab nicht neu navigieren — der Tab ist der Gedächtnisträger |

## Live-Beweis (sauberer Commit-Build)

- Binary: `webagent 0.11.8 (89605ba7d)`
- EXE-SHA256: `CE6F874FBDAD3A8D1F479847621DBD28645AA93CB5461299750F524DF0118F2D`
- Test-Bridge: `127.0.0.1:8789`, `--skip-startup-reconcile api serve --brain chatgpt --headless --port 8789 --timeout-secs 120`
- Environment: `WEBAGENT_USE_SHARED_BROWSER=1`, `WEBAGENT_PERSIST_TABS=1`,
  `WEBAGENT_CONTINUE_PROVIDER_CHAT=1`, `WEBAGENT_SHARED_DEBUG_PORT=9223`
- Health: `{"service":"webagent-provider-bridge","status":"ok"}`
- Auth: `/v1/models` mit Header 200, ohne Header 401

| Turn | Gesendete Nachricht (nur diese) | HTTP | Antwort | Dauer |
|---:|---|---:|---|---:|
| 1 | `Merke dir das Codewort Otter. Antworte exakt mit T1019-TURN1-OK.` | 200 | exakt `T1019-TURN1-OK` | 33,5 s |
| 2 | `Wie lautet das Codewort? Antworte exakt mit dem einen Wort.` | 200 | exakt `Otter` | 28,6 s |

`events_delta=2`. Beide ChatGPT-Events: `success=true`, `sends=1`,
`prompt_chars` 236 bzw. 231. Der zweite Prompt enthält das Wort `Otter`
**nicht**; die Antwort `Otter` stammt also aus der Provider-Konversation,
nicht aus erneut angeschlepptem Bridge-Kontext. Die LLVM-Logs zeigen je Turn
`submission proved`, kein `send_phase`-Fault und kein Retry.

Diagnose-Log: `C:\Users\storax\AppData\Local\Temp\opencode\t1019-final.err.log`.

## Erkenntnisgrenze — nicht behauptet

Ein **bedingsloses** Überspringen von `new_chat()` wurde zuerst gebaut und
live verworfen: beide Turns HTTP 502 mit
`send_phase=insert: Composer-Feld nicht gefunden (Timeout)` (~151 s je Turn).
Der frische Shared-Tab braucht den `new_chat`-Pfad, um überhaupt auf eine
Seite mit Composer zu gelangen. Deshalb ist der Skip jetzt an eine
nachgewiesene Konversations-URL (`/c/…`) gebunden.

Im obigen Live-Lauf war die Tab-URL keine `/c/…`-URL; die Meldung
`bestehende Provider-Konversation (/c/) wird fortgesetzt (kein new_chat)`
erscheint daher **nicht** im Log. Es wird nicht behauptet, dass `new_chat()`
in diesem Lauf entfiel. Belegt sind: nur die aktuelle Nachricht im Prompt,
kein neues WebView pro Request und Provider-Gedächtnis über zwei Turns.
Der Skip-Pfad ist eine defensische Bedingung für Umgebungen mit echter
`/c/…`-URL und durch `continue_chat_skippt_new_chat_nur_in_bestehender_konversation()`
getestet.

Die Warnung `Master kennt mistral, claude, gemini nicht, obwohl die
kanonischen Profile eingeloggt sind` stammt aus dem Shared-Profil und ist
unverändert zum Befund aus dem Shared-Only-Test mit `continue=0`
(`T1019-SHARED-OK`, HTTP 200). Sie ist keine Aussage über chatgpt.

## Deploy auf die aktive Bridge (2026-11-27)

- Backup: `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\backup-release-before-T1019-20261127\`
  mit EXE `BBB9C714A0074C6AC975582BB337CB2720B1F189703419A7C5711ECBF35EDA3B`,
  DLL `465A7DDF…A0A6F` und Breaker-State.
- Alte PID `9008` gestoppt; neue PID `7236`, identische Command-Line
  (`--skip-startup-reconcile api serve --brain chatgpt --headless --port 8788 --timeout-secs 120`).
  Der anschliessende Supervisor-Test restartete die Bridge als PID `14192`
  (siehe Supervisor-Abschnitt); das ist die jetzt aktive PID.
- Aktivierte Environment-Variablen im Prozess:
  `WEBAGENT_USE_SHARED_BROWSER=1`, `WEBAGENT_PERSIST_TABS=1`,
  `WEBAGENT_CONTINUE_PROVIDER_CHAT=1`. Ohne diese Flags bleibt das Feature
  per Design inaktiv.
- Deployte EXE-SHA256: `CE6F874FBDAD3A8D1F479847621DBD28645AA93CB5461299750F524DF0118F2D`
  (`webagent 0.11.8 (89605ba7d)`); DLL unverändert.
- Health `ok`, `/v1/models` 200 mit Header / 401 ohne.
- Live auf 8788: Turn 1 HTTP 200, exakt `T1019-DEPLOY-OK` (116,7 s);
  Turn 2 HTTP 200, exakt `Otter` (25,1 s); beide `success=true`,
  `sends=1`, `prompt_chars` 237/231.
- `bridge.pid` wurde zunaechst auf PID `7236` korrigiert und nach dem
  Supervisor-Test von diesem selbst auf `14192` gesetzt.

**Supervisor end-to-end:** `scripts/supervise-bridge.ps1` setzt die drei Flags
jetzt selbst. Kontrolliert getestet: PID `7236` gestoppt, Supervisor gestartet,
neue PID `14192` mit identischer Command-Line, Health `ok`, `bridge.pid=14192`.
Danach auf 8788: Turn 1 HTTP 200, exakt `T1019-SUPERVISOR-OK` (95,8 s);
Turn 2 HTTP 200, exakt `Otter` (51,1 s); beide `success=true`, `sends=1`,
`prompt_chars` 241/231. Der Modus überlebt also einen Supervisor-Neustart.
Der aufrufende Shell-Wrapper des Supervisor-Aufrufs lief in ein Handle-Timeout;
der Supervisor selbst protokollierte `Bridge gestartet PID=14192` und
`OK Bridge gesund PID=14192`. Kein Tag, Release oder Versionssprung.

## Rückweg / Nebenwirkungen

- Produktiv-Bridge `8788` (alt PID 9008, jetzt 7236) wurde nur mit Backup und
  identischer Command-Line neu gestartet.
- Breaker-Sicherung: `C:\Users\storax\AppData\Local\webagent\data\circuit_breaker\state-before-T1019-live-20261127.json`.
  Der `chatgpt`-Eintrag wurde vor dem Test entfernt und nach dem Erfolg neu
  als Success geschrieben.
- Test-Bridge `8789` wurde nach der Beweisaufnahme gestoppt.
- Rückweg Deploy: aktive PID `14192` stoppen, EXE/DLL aus
  `backup-release-before-T1019-20261127\` zurückkopieren, Breaker-State bei
  Bedarf wiederherstellen und mit der obigen Command-Line neu starten.
  Der Supervisor muss dabei mit dem alten Skriptstand laufen oder seine
  drei Flag-Zeilen entfernt werden.
- Version bleibt `0.11.8`; kein Tag, kein Release, kein Versionssprung.
