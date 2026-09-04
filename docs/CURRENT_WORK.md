# Aktueller Arbeitsstand

**Aktualisiert:** 2026-09-04
**Zweck:** verbindlicher Wiedereinstieg und operative Wahrheit. Historische Befunde stehen in `docs/OVERVIEW.md` sowie in den datierten Übergaben; diese Datei ersetzt sie nicht, sondern hält nur den aktuellen Abschlusspfad fest.

**Stand 2026-09-04:** `master` auf `3b81603` (PR #30 „CLI-/UI-Schnittstellen & Doku" gemergt: Web-UI-Default ehrlich, `--headless`-Semantik, README/AGENTS/OVERVIEW konsolidiert, Archiv-Doc `CLI_UI_REDESIGN.md`). Repo-Pflege aufgeraeumt: 13 tote/verwaiste Branches entfernt, remote nur `master` + aktive Arbeit. Naechste aktive Arbeit: **`feature/cli-auto-brain`** — der Auto-Router (`api serve --brain auto`) ist jetzt direkt im CLI verfuegbar: `run`/`repl`/`relay` haben Default `--brain auto` und loesen ueber `select_auto_brain_for_cli` (gemeinsamer Router mit der Bridge) auf; importierte Auswahlkern-Injektion `first_available_auto_brain_in` ist deterministisch getestet. Details in [`CLI_UI_REDESIGN.md`](CLI_UI_REDESIGN.md) Abschnitt 6/„Auto-Router im CLI". Offene Design-Folgepunkte (ask-Befehl, Ports) in Abschnitt 7. Gates auf dem Branch: fmt, clippy `-D warnings`, komplette browserfreie Test-Suite (1279+7 gruen) und `git diff --check` gruen.

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

- **Branch:** `master` auf `f51acd6`; der frühere Arbeitsbranch `task/v1-release-baseline` zeigt auf denselben Commit.
- **Abgeschlossene Scheiben:** `2659baf` (Headless-Baseline), `4fdb068` (v1.0-Definition), `d32b8d0` (Linux-WebView-Feature-Gates) und `f51acd6` (Linux-/Windows-Abnahmestände); alle per Fast-Forward in `origin/master` integriert.
- **Eigentümerschaft:** aktueller Integrator bearbeitet den v1.0-Abschluss; Live-Anmeldungen bleiben beim Eigentümer.
- **Externe Freigaben:** erforderlich für Live-Browser, Logins, kostenpflichtige Providerpfade, Tag und GitHub-Release.
- **Arbeitsbaum:** nach diesem Übergabecommit erneut prüfen; `master` direkt pushen, keine History umschreiben.

> **Hinweis zur Wahrheitspflege:** Historische Live-Befunde aus August 2026 sind nützlich für die Diagnose, aber kein aktueller Verfügbarkeitsbeleg. Maßgeblich für v1.0 sind frische, reproduzierbare Belege am Release Candidate.
