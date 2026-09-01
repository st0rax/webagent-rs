# Web-UI-, API- und Tool-Neuschnitt

**Status:** verbindlicher Umsetzungsplan

**Stand:** 2026-09-01

**Branch:** `feat/browser-inference-provider`

## Entscheidung

WebAgent wird nicht mehr ueber die bestehende Terminal-UI definiert. Das
Produktziel besteht aus drei gleichrangigen, pruefbaren Flaechen:

1. einer menschenfreundlichen lokalen Web-UI als primaerer Bedienoberflaeche;
2. einem clientunabhaengigen OpenAI-kompatiblen Inference-Endpunkt;
3. einem WebAgent-eigenen lokalen Werkzeuglauf, den jeder Brain sowohl aus der
   Web-UI als auch ueber den API-Pfad verwenden kann.

Claude ist der Referenz-Brain, weil seine Weboberflaeche konkurrierende oder
verdeckte Anweisungen besonders streng behandelt. Claude ist aber nicht der
einzige Abnahmegegenstand: Dieselbe Matrix muss anschliessend fuer jeden im
Katalog angebotenen Brain bestanden sein.

Die folgenden Begriffe sind ab jetzt strikt getrennt:

- **Brain:** ein Web-Chat wie Claude, ChatGPT, Gemini oder Kimi;
- **Web-UI:** die lokale WebAgent-Bedienoberflaeche;
- **Provider-API:** die OpenAI-kompatible HTTP-Grenze fuer beliebige Clients;
- **Managed Tools:** vier von WebAgent selbst implementierte und ausgefuehrte
  Werkzeuge;
- **Client Tools:** Function-Definitionen, deren Ausfuehrung dem API-Client
  gehoert. Sie sind nicht dasselbe wie Managed Tools.

Pi dient ausschliesslich als kleine, bewaehrte Referenz fuer Toolumfang und
Loop-Semantik. WebAgent importiert, startet oder paketiert weder Pi noch ein
`@earendil-works/*`-Paket.

## Recherchierter Ist-Zustand

### Bedienoberflaeche

Das Repository besitzt noch keine lokale Web-UI. Die vorhandene Ratatui-/ANSI-
Oberflaeche umfasst zusammen rund 4.400 Zeilen und greift auf bereits getrennte
Zustands-, Ereignis-, Controller- und Browsermodule zu. Dieser Code wird nicht
gepurgt.

Vor dem Umbau wird der letzte vollstaendige TUI-Stand auf GitHub durch
`archive/tui-ui` und ein annotiertes Tag gesichert. Die TUI bleibt zunaechst
hinter ihrem Cargo-Feature und als Legacy-Subcommand baubar. Erst nachdem die
Web-UI ihre Abnahme bestanden hat, darf sie aus dem primaeren Startpfad entfernt
werden. Branch, Tag und Git-Historie bleiben dauerhaft wiederherstellbar.

### Claude als Referenz

Die aktuelle Claude-Weboberflaeche bietet die hier geforderte Referenzflaeche:

- normalen Mehrturn-Chat;
- Laufzeit-Auswahl der fuer das Konto sichtbaren Modelle;
- modellabhaengige Aufwand-/Thinking-Einstellungen;
- Datei- und Bildanhaenge.

Modelle und Aufwandstufen sind plan-, konto-, organisations- und rollout-
abhaengig. WebAgent darf sie deshalb nicht als feste Liste kodieren, sondern
muss sie bei geoeffnetem Menue zur Laufzeit ermitteln und die tatsaechlich
gewaehlte Einstellung nachpruefen. Dasselbe Prinzip gilt fuer alle anderen
Brains.

Anthropics aktuelle Consumer Terms untersagen automatisierten Zugriff auf den
Consumer-Dienst ohne API-Key oder ausdrueckliche Erlaubnis. Das ist eine
externe Betriebs- und Releasegrenze, keine technische Unklarheit. Automatisierte
Mocks und lokale Fixtures bleiben davon unberuehrt; reale unbeaufsichtigte
Claude-Web-Abnahmen benoetigen eine zulaessige Nutzungssituation.

Quellen:

- [Claude: Modelle, Aufwand und Thinking](https://support.claude.com/en/articles/8664678-change-the-model-effort-and-thinking-settings)
- [Claude: Dateien hochladen](https://support.claude.com/en/articles/8241126-upload-files-to-claude)
- [Claude: Einstieg in den Chat](https://support.claude.com/en/articles/8114491-get-started-with-claude)
- [Anthropic Consumer Terms](https://www.anthropic.com/legal/consumer-terms)

### OpenAI-API-Vertrag

Die offizielle OpenAI Platform API ist kein einzelner Chat-Endpunkt. Der
aktuelle Referenzindex enthaelt 265 Methodenseiten, darunter Admin-, Realtime-,
Fine-Tuning-, Eval-, Vector-Store-, Container-, Video- und Abrechnungsressourcen.
Ein browserbasierter Modellprovider kann diese Plattform nicht ehrlich
nachbilden.

Der verbindliche Produktbegriff lautet deshalb:

> **OpenAI-kompatibler lokaler Inference-Provider mit einem versionierten und
> vollstaendig getesteten Models-, Chat-Completions- und Responses-Profil.**

Innerhalb dieses veroeffentlichten Profils bedeutet kompatibel tatsaechlich
kompatibel: keine clientbezogenen Sonderfaelle, keine still ignorierten Felder,
keine erfundenen Usage-Werte und keine OpenAI-foermige JSON-Huelle ohne die
zugehoerige Semantik. Ausserhalb des Profils kommen strukturierte
`unsupported_*`-Fehler statt Scheinerfolg.

Der heutige Adapter erfuellt diesen Massstab nicht. Seine Request-Strukturen
lesen nur einen kleinen Teil der offiziellen Felder; Serde ignoriert weitere
Felder still. Response-Objekte enthalten teils Platzhalter, Responses-SSE fehlt
unter anderem die konsequente Sequenznummern-Semantik, und Browser-Usage wird
als null beziehungsweise 0 dargestellt. Diese Punkte muessen feldweise gegen
die aktuelle offizielle Referenz korrigiert werden.

Verbindliche Profilflaechen:

| Profil | Routen | Verpflichtung |
|---|---|---|
| `openai-models-v1` | `GET /v1/models`, `GET /v1/models/{id}` | offizieller Listen-/Objektvertrag, lokale IDs, 404 fuer Unbekanntes |
| `openai-chat-text-v1` | `POST /v1/chat/completions` | Rollen, Text-Content-Parts, ein Choice, Fehler, echtes oder klar markiertes emuliertes SSE |
| `openai-responses-text-v1` | `POST /v1/responses` | String-/`input_text`-Input, vollstaendiges Response-Objekt und kanonischer Event-Lifecycle |
| `openai-local-state-v1` | Retrieve/Delete/Input-Items und `previous_response_id` | erst nach durablem, mandantengetrenntem Store und Restart-Test |

Jedes semantiktragende Feld wird einer von drei Klassen zugeordnet:

1. **unterstuetzt:** weitergereicht oder lokal vollstaendig umgesetzt und
   positiv getestet;
2. **adaptiert:** bewusst lokal nachgebildet und als solche dokumentiert;
3. **nicht unterstuetzt:** vor dem Browserstart mit passendem Parameter,
   Fehlercode und HTTP-Status abgelehnt.

Beispiele fuer Felder, die niemals still akzeptiert werden duerfen, sind
`n > 1`, `seed`, `logprobs`, `service_tier`, strikte Structured Outputs,
Samplerparameter ohne steuerbaren Upstream, exakte Usage sowie nicht vorhandene
Modalitaeten.

Quellen:

- [OpenAI API Reference Overview](https://developers.openai.com/api/reference/overview)
- [Create Chat Completion](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
- [Create a Response](https://developers.openai.com/api/reference/resources/responses/methods/create)
- [Responses Streaming Events](https://developers.openai.com/api/reference/resources/responses/streaming-events)
- [Function Calling](https://developers.openai.com/api/docs/guides/function-calling)
- [API Errors](https://developers.openai.com/api/docs/guides/error-codes)

### Werkzeugreferenz

Die lokal installierte Referenz ist Pi 0.84.4. Dessen normaler Coding-Modus
verwendet `read`, `bash`, `edit` und `write`. WebAgent uebernimmt den kleinen
Umfang und die bewaehrten Eigenschaften, implementiert sie aber eigenstaendig
in Rust:

| Managed Tool | Kernschema | WebAgent-Verpflichtung |
|---|---|---|
| `read` | `path`, optional `offset`, `limit` | begrenztes Textlesen, Fortsetzung, Bildresultat, Workspace-Grenze |
| `bash` | `command`, optional `timeout` | echte Bash, Streaming, Timeout/Abort, Exitcode, Outputlimit, Shell-Policy |
| `edit` | `path`, `edits[{oldText,newText}]` | atomare, eindeutige, nicht ueberlappende Ersetzungen plus Patch |
| `write` | `path`, `content` | Elternverzeichnisse, vollstaendiges Schreiben, Workspace-Grenze |

Die vorhandenen WebAgent-Aktionen `shell`, `edit`, `edit_batch` und `write`
werden nicht einfach umbenannt. Ein zentraler `ToolRegistry`-Vertrag erzeugt
Schema, validiert Argumente, fragt die Policy, fuehrt genau einmal aus und
normalisiert Ergebnis und Fehler. Web-UI und API verwenden dieselbe Registry,
dieselbe Execution und denselben Ereignisstrom.

Quelle: [Pi 0.84.4 Tool Registry](https://github.com/earendil-works/pi/blob/b79e4cc834970cca69daebffab7df1da7d1e52c4/packages/coding-agent/src/core/tools/index.ts)

## Prompt- und Kontextgrenze

Der bisherige API-Prompt ist zu verwerfen. Er hat Systemtext, Tool-Schemas,
Anhaenge und Verlauf in einen vermeintlich priorisierten Browserprompt
zusammengezogen. Das fuehrte nachweislich zu Verweigerungen, Identitaetsbrei,
alten Anhaengen und der Ausgabe interner Kontexte.

Es gibt kuenftig zwei explizite Modi:

### Reiner Chat

- keine Tooldefinition;
- kein `WEBAGENT_INFERENCE/1`;
- keine Behauptung einer System- oder Developer-Rolle gegenueber dem
  Browser-Brain;
- fruehere Nutzer- und Brain-Beitraege erscheinen als klar markiertes Zitat
  eines bisherigen Gespraechs mit `[brain]`;
- die letzte Nutzernachricht steht separat und unveraendert;
- nur Anhaenge der aktuellen Nutzernachricht werden hochgeladen;
- nicht verlustfrei darstellbare Rollen oder Content-Typen werden abgelehnt,
  nicht verschluckt.

### Managed Agent

WebAgent sagt dem Brain offen, was tatsaechlich passiert: Es fuehrt den Chat
ueber einen lokalen Vermittler, besitzt keinen direkten Rechnerzugriff und kann
genau vier kompakt beschriebene WebAgent-Werkzeuge anfordern. Eine Anforderung
ist noch kein Ausfuehrungsbeleg; nur das danach zurueckgesendete Ergebnis ist
einer. Es gibt keine vorgetaeuschte Systemrolle und keine als Historie
getarnte Handlungsanweisung.

Client-eigene Function-Tools werden davon getrennt behandelt. WebAgent fuehrt
niemals einen beliebigen vom Client gelieferten Funktionsnamen lokal aus.

## Quellen und manueller Hybrid (Free-Provider)

Jeder Brain hat genau eine aktive Quelle pro Session. Die Standardquelle ist
der Browser-Chat. Zusaetzlich kann jeder Brain eine manuell gewaehlte,
OpenAI-kompatible Free-Tier- oder offizielle Provider-API als Quelle erhalten.
Es gibt kein automatisches Routing und keinen versteckten Fallback: Quellen
wechseln nur auf ausdruecklichen Nutzerwunsch (Web-UI-Schalter oder Befehl),
und eine explizit vom Nutzer definierte Kette bleibt sichtbar und
nachvollziehbar.

Moegliche API-Quellen sind beispielsweise OpenRouter (`:free`-Modelle), NVIDIA
NIM, Groq, Cerebras, Mistral-Experiment, das Gemini-API-Free-Kontingent sowie
jeder beliebige OpenAI-kompatible Endpunkt (llama.cpp, vLLM, lokales Gateway).
FreeLLMAPI dient als Beleg, dass sich die freien Kontingente vieler Anbieter
hinter einem einzigen OpenAI-kompatiblen Vertrag buendeln lassen; sein
Node-/Docker-Stack wird nicht uebernommen.

### Source-Auswahl

- `data/providers.json` (gitignored, analog `custom_brains.json`): je Brain eine
  Quelle oder `default` (= Browser). Keys nur als Verweis auf Umgebungsvariablen
  oder den vorhandenen Key-Mechanismus, niemals als Klartext in der Datei.
- Session-Scope: Die gewaehlte Quelle gilt fuer die laufende Session. Befehl
  `/quelle <brain> <quelle|list|default>`, persistiert nur mit ausdruecklichem
  `--save`.
- Web-UI: kompakter Quellen-Schalter in der Kopfzeile (Brain, erkanntes Modell,
  Quelle, Modus).

### Transport und Groessenbudget

- Eigener minimaler HTTPS-Client in Rust (kein `reqwest`/Hyper): HTTP/1.1 ueber
  `tokio::net::TcpStream` + `tokio-rustls`, CA-Buendel via `webpki-roots`. Kein
  System-OpenSSL, keine nachzuruestende Abhaengigkeit; laeuft auf Windows,
  Linux und Android aus einer einzelnen Binary.
- Budget: Die ausgelieferte Binary bleibt deutlich unter 10 MB (Zuwachs fuer TLS
  circa 2 MB). Das Budget wird am Release-Artefakt kontrolliert und ist ein
  Kriterium der Release-Abnahme.

### Vertrag

- API-Quellen sind echte OpenAI-kompatible Endpunkte: Sie durchlaufen dieselben
  drei veroeffentlichten Profile (Models, Chat Completions, Responses) nativ und
  ohne erfundene Usage-Werte.
- Client-Tools werden nie lokal ausgefuehrt; Werkzeugausfuehrung bleibt allein
  den WebAgent-Managed Tools vorbehalten.
- Free-Tier-Kontingente, Ratelimit-Kontexte und Modellrotation sind
  providerabhaengig und werden wie die Model-Auswahl zur Laufzeit ermittelt,
  nicht als feste Liste kodiert.
- Rate-/Breaker-Politik je Quelle und Key (RPM/RPD/TPM-Zaehler, fail-offen,
  Cooldown) analog der bestehenden `FreeCloudHealthPolicy` und dem
  `circuit_breaker`; der Zustand erscheint im Health-Dashboard und nie als
  Modelltext.

### Grenze

Offizielle Provider-APIs sind die zulaessige Nutzungssituation fuer einen
Wechsel weg vom Browser-Chat. Die Anthropic-Consumer-Terms-Grenze fuer den
unbeaufsichtigten Claude-Browser bleibt davon unberuehrt; Claude bleibt
Referenz-Brain mit Browser als vorgesehener Quelle.

Quellen:

- [FreeLLMAPI](https://github.com/tashfeenahmed/freellmapi)
- [OpenRouter free Models](https://openrouter.ai/models?q=:free)
- [NVIDIA NIM / build.nvidia.com](https://build.nvidia.com)
- [Groq Console](https://console.groq.com)
- [Cerebras Cloud](https://cloud.cerebras.ai)
- [Mistral Console](https://console.mistral.ai)
- [Gemini API](https://ai.google.dev/gemini-api)

## Zielarchitektur

```text
Lokaler Browser
  -> WebAgent Web-UI
       -> SessionService
            -> BrainAdapter (Claude zuerst, danach alle Brains)
            -> ManagedAgentLoop
                 -> ToolRegistry (read, bash, edit, write)
                 -> Policy + Executor

Beliebiger OpenAI-Client
  -> OpenAICompatibilityAdapter
       -> derselbe SessionService / ManagedAgentLoop
       -> derselbe EventStream

BrainAdapter
  -> WebView2/WebKitGTK
       -> jeweilige Web-Chat-Oberflaeche
```

Die Transport-, Sitzungs- und Produktlogik wird aus `api_bridge.rs`, REPL und
TUI herausgezogen. Keine Oberflaeche erhaelt eine eigene Toolimplementierung.

## Web-UI und User Experience

`webagent` startet standardmaessig einen Loopback-Webserver und oeffnet die
lokale Web-UI. Die Seite ist Desktop-first, funktioniert aber auch in einem
schmalen Fenster.

### Informationsarchitektur

Das Layout orientiert sich an der Grok-Bot-Oberflaeche: eine inhaltlich
strukturierte linke Leiste, ein ruhiger Chat in der Mitte und ein globaler
Statusbalken oben. Es gibt drei feste Zonen:

- **oberer Statusbalken (Health-Balken):** relative Balkenanzeige aller
  verfuegbaren und nicht verfuegbaren Quellen [gruen|rot], zusaetzlich
  Prozentanteil und Quellenliste. Ein Klick oeffnet die Backend-Detailansicht
  (Doctor-Report je Brain, Breaker-/Canary-Zustand, naechste Aktion) — der
  Zustand stammt aus `GET /api/health/brains`, nicht aus Modelltext.
- **linke, einklappbare Leiste mit System-Kategorien:** alles Systemrelevante
  ist in sinnvollen Kategorien erreichbar — `Sitzungen`, `Brains & Quellen`,
  `Gruppen`, `Laeufe`, `System` (Workspace, Profile, Selektoren, Logs). Jede
  Kategorie ist aufklappbar und versammelt die zugehoerigen Eintraege; der
  Chat ist nie der einzige Zugang zu einem Systemzustand.
- **ruhige Hauptspalte:** Nutzer-/Brain-Nachrichten und laufende Antwort.

Darueber hinaus gelten weiterhin:

- **kompakte Kopfzeile:** Brain, tatsaechlich erkanntes Modell, Aufwand,
  Quelle und Modus `Chat`/`Systemzugriff`; der Quellen-Schalter haengt hier an
  (manueller Hybrid, siehe oben);
- **Composer:** mehrzeilige Eingabe, Drag-and-drop, Dateiauswahl,
  Attachment-Vorschau, Senden/Stoppen;
- **einklappbare Aktivitaetsleiste:** echte Schritte, Werkzeuganforderung,
  Freigabe, Argumente, Live-Ausgabe, Ergebnis und Dauer.

Technische Rohdaten bleiben erreichbar, dominieren aber nicht den normalen
Chat. Fehler nennen Ursache, betroffene Schicht und naechste Aktion; sie werden
nicht als Modelltext in die Unterhaltung gemischt.

### Streaming ist Pflicht

Die UI zeigt unmittelbar nach dem Absenden einen echten Laufzustand. Sobald die
Brain-Oberflaeche wachsenden Text zeigt, werden ausschliesslich neue Deltas per
SSE uebertragen. Werkzeug- und Uploadereignisse laufen ueber denselben
geordneten Eventstream.

Ein Turn besteht nur, wenn:

- sofort ein sichtbarer Fortschritt beginnt;
- Text ohne Duplikate inkrementell waechst;
- Markdown waehrend des Streams stabil bleibt;
- Auto-Scroll nur aktiv ist, solange der Nutzer am Ende steht;
- Stop Browsergenerierung, Agentenloop und Werkzeugprozess beendet;
- SSE nach Verbindungsverlust anhand einer Event-ID fortsetzen kann;
- der Endzustand eindeutig `completed`, `cancelled` oder `failed` ist.

Ein Brain, dessen UI Text erst am Ende freigibt, wird als `buffered` markiert und
besteht das Streaming-Gate nicht. Status- und Toolereignisse bleiben trotzdem
sofort sichtbar.

### Health-Dashboard (Gesundheit)

Der obere Statusbalken zeigt fuer alle konfigurierten und registrierten Brains,
welche Quellen verfuegbar und welche nicht verfuegbar sind, ohne einen Browser
zu starten.

- **Quelle:** das bestehende `doctor`-Modul (`DoctorReport`/`BrainCheck`,
  dateisystembasiert) plus Breaker-Zustand aus `circuit_breaker`/`canary`.
  `GET /api/health/brains` liefert den Report als JSON mit Zeitstempel und
  Abrufalter; kein Browserstart, kein Live-Lauf pro Poll.
- **Uebersicht:** relative Balkenanzeige in der oberen Statusleiste — gruener
  Anteil = gesunde Quellen, roter Anteil = nicht verfuegbare; zusaetzlich
  Prozentwert und Quellenliste. Nie Farbcodierung allein
  (Barrierefreiheit). Pro Brain eine Zeile mit Balken.
- **Klick auf einen Balken oder Eintrag:** oeffnet die Backend-Detailansicht
  (zweite Ebene statt Popup-Stapel) mit den Doctor-Feldern je Brain
  (Selektoren, Profil und Locks, Login-Zustand, letzter abgeschlossener Lauf
  inkl. Alter, Recovery-Hinweis) und Breaker-/Canary-Status. Fehler nennen
  Ursache, Schicht und naechste Aktion.
- **Quelle waehlen (Klick auf die Quelle):** derselbe Session-Source-Scope wie
  `/quelle`; die gewaehlte Quelle wird als Aktiv-Zustand markiert. Detail und
  Auswahl sind getrennte Bedienelemente.
- **Aktualisierung:** beim Laden der Seite und ueber eine explizite
  „Neu pruefen"-Aktion; automatische Pruefungen hoechstens in langen
  Canary-Intervallen, nie innerhalb eines Chat-Streams.

### Bedien- und Barrierefreiheitsregeln

- komplette Tastaturbedienung mit sichtbarem Fokus;
- semantische Controls statt klickbarer `div`-Elemente;
- Live-Regionen fuer Status, aber kein Screenreader-Spam pro Textdelta;
- keine alleinige Farbcodierung fuer Status oder Gefahr;
- destruktive beziehungsweise workspace-aendernde Tools zeigen eine klare,
  konsistente Policy- und Freigabeanzeige;
- reduzierte Animation bei `prefers-reduced-motion`;
- responsive Panels ohne horizontales Zwangsscrollen;
- keine automatische Fokusentfuehrung waehrend eines Streams.

Vor der Controller-Integration entsteht ein lokal renderbarer, klickbarer
Prototyp mit Fake-Events. Layout und Interaktion werden daran abgenommen, bevor
Backendkopplung die UX verfestigt.

UX-Referenzen:

- [W3C WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- [WAI-ARIA Authoring Practices](https://www.w3.org/WAI/ARIA/apg/patterns/)
- [VS Code Chat](https://code.visualstudio.com/docs/chat/chat-overview)
- [VS Code Toolfreigaben](https://code.visualstudio.com/docs/agents/run/approvals)

## Implementierungsreihenfolge

## Aufgabentafel (wer übernimmt was)

Damit mehrere nichtmenschliche Entwickler parallel arbeiten können, sind die
Phasen unten nach Eignung markiert. Die einzelne, verlösbare Arbeit liegt auf
`docs/TASKBOARD.md`; maschinenlesbar und claim-relevant ist `docs/TASKBOARD.json`.
Verbindliche Arbeitsregeln: `docs/WORK_CONTRACT.md`.

| Phase | bevorzugte Entwickler | Grund |
|---|---|---|
| 0 Bestand | local/opencode, chief | Doku/Bestand, Matrix, Handover |
| 1 Kern | chatgpt-codex, claude-code, local | strukturierte Extraktion + deterministische Tests |
| 2 Web-UI | claude-code, manus | UI-Prototyp, A11y, eingebettete Assets |
| 3 Claude-Referenz | claude-code (Live) | Claude-Web-Livebetrieb; Freigabegrenze beachten |
| 4 OpenAI-Kern | chatgpt-codex (Idealfall) | tiefe OpenAI-Referenztreue, SDK-Blackbox |
| 5 Alle Brains | claude-code, grok-agent | Live-Provider-Matrix, providerkundig |
| 6 Health+Quellen | chatgpt-codex, grok-agent | rustls-Client, `/quelle`, Rate-Tracking |
| 7 Gruppen | chatgpt-codex, claude-code | Runden-/Handoff-Logik auf bestehender Infra |

Claim-Regel: Eine Aufgabe bearbeitet immer genau ein Entwickler; die Übernahme
wird nur durch den Eintrag in `docs/TASKBOARD.json` wirksam (owner, branch,
claimed_at). Beim Abschluss: Belegpfad + Handover-Status aktualisieren, Zelle
auf `done`.

### Phase 0 - Bestand sichern und Wahrheit korrigieren

1. TUI-Stand als GitHub-Branch und annotiertes Tag erhalten.
2. Alte unqualifizierte Aussagen zu `100 % API-Kompatibilitaet` entfernen.
3. Eine maschinenlesbare Capability-/Konformitaetsmatrix anlegen.
4. Alle bisherigen positiven Belege als historische Teilbelege markieren.

### Phase 1 - Gemeinsamer Kern und deterministische Tests

1. `SessionService`, `EventStream` und `ToolRegistry` als UI-neutrale Module
   herausloesen.
2. Die vier Managed Tools in Rust mit Workspace-, Symlink-/Junction-, Timeout-
   und Shell-Policy-Grenzen implementieren.
3. Einen Fake-Brain fuer Textdelta-, Toolloop-, Abort-, Retry- und
   Exactly-once-Tests bauen.
4. Reinen Chat und Managed Agent als getrennte Promptbuilder implementieren.

### Phase 2 - Web-UI-Prototyp und lokaler Server

1. Eingebettete statische Assets, damit die einzelne portable Binary erhalten
   bleibt.
2. Session-, Capability-, Health-, Upload-, Chat-, Stop- und Event-Endpunkte.
3. Klickbaren Fake-Prototyp im Grok-Bot-Layout (Health-Balken oben, System-
   Kategorien links, Chat-Mitte) mit responsivem Layout und A11y-Gates abnehmen.
4. Defaultstart auf lokale Web-UI umstellen; TUI bleibt Legacy.

### Phase 3 - Claude-Referenz komplettieren

1. Reiner Mehrturn-Chat mit echtem Delta-Streaming.
2. Modelle zur Laufzeit auflisten, waehlen und nach dem naechsten Turn
   verifizieren.
3. Aufwand/Thinking zur Laufzeit erkennen, waehlen und verifizieren.
4. Kleine TXT- und PNG-Anhaenge samt sichtbarer Vorschau und inhaltlichem
   Antwortbeleg.
5. Stop, Fehler, Login-required, Limit und Session-Wiederaufnahme sichtbar und
   getrennt behandeln.
6. Managed `read`-Nonce-Loop aus Web-UI und API exakt einmal ausfuehren.

### Phase 4 - OpenAI-Konformitaetskern

1. DTOs feldvollstaendig fuer die drei veroeffentlichten Profile modellieren.
2. Unbekannte JSON-Felder fuer vorwaertskompatible Daten tolerieren, bekannte
   aber nicht umsetzbare semantiktragende Felder explizit ablehnen.
3. IDs, Zeitstempel, Header, `x-request-id`, Fehler und Statuscodes korrigieren.
4. Chat-SSE und Responses-SSE auf denselben geordneten Deltastrom setzen;
   Responses-Ereignisse erhalten monotone `sequence_number`.
5. State nur auf einem echten persistenten Store anbieten.
6. Offizielle OpenAI-Python- und JavaScript-SDKs als Black-box-Tests verwenden;
   danach mindestens zwei verschiedene reale Clients ohne clientbezogenen Code.

### Phase 5 - Alle Brains

Fuer jeden zur Laufzeit angebotenen Brain und `auto` werden beide Oberflaechen
getestet:

1. Chat und Mehrturn-Isolation;
2. inkrementelles Streaming und Stop;
3. Modellauswahl;
4. native Aufwandswahl oder offen ausgewiesene WebAgent-Aufwandsabbildung;
5. Datei-/Bildanhang entsprechend der beworbenen Capability;
6. Managed `read`, `bash`, `edit`, `write` mit einer reversiblen Fixture;
7. identisches Ergebnis- und Fehlerverhalten in Web-UI, Chat Completions und
   Responses.

Ein Katalogeintrag wird erst beworben, wenn seine Matrix gruen ist. Ein
providerseitig nicht vorhandenes UI-Feature wird nicht erfunden; eine
WebAgent-Abbildung muss im UI als solche erkennbar sein.

### Phase 6 - Health-Dashboard und manuelle Quellen

1. `GET /api/health/brains` auf dem bestehenden Doctor-Report aufsetzen, ohne
   Browserstart.
2. Health-Balken und Detailansicht in der Web-UI (relative Balken, Detail,
   Session-Auswahl, A11y-Gates).
3. Minimalen rustls-HTTPS-Client und `data/providers.json` einfuehren.
4. `/quelle`-Befehl, Web-UI-Quellen-Schalter und Session-Source-Scope.
5. Pro-Quelle-Rate- und Breaker-Politik mit Test; Verifikation des
   Groessenbudgets an der Release-Binary.

### Phase 7 - Grok-Bot-Modus (Gruppen)

Der Grok-Bot-Parallelmodus wird Produkt: Gruppen aus 2-6 Brains, die eine
Aufgabe in Runden bearbeiten und eine Synthese an den Nutzer zurueckgeben.

1. Gruppen als persistente Kategorie in der linken Leiste (`Gruppen`);
   Erstellen und Benennen einer Gruppe aus 2-6 registrierten Brains.
2. Runden-Mechanik auf Basis der bestehenden `/swarm`-Abfrage und der
   `HandoffQueue`: je Runde eine Nachricht pro Brain, `@Brain`-Erwaehnung fuer
   gezielten Handoff, Leader-Synthese am Ende.
3. Gruppenlauf als normale Session: gleicher Eventstream, gleiche Stop-,
   Fehler- und Anhangregeln wie Einzel-Chats.
4. Abnahme: eine Gruppe mit Fake-Brains deterministisch getestet, danach eine
   reale Gruppe (z. B. `claude`+`perplexity`) end-to-end belegt.

Routinen (`when`) und Skills (`how`, Lernen aus Live-Demo) bleiben bewusst
ausgeklammert, bis der Gruppenmodus abgenommen ist — sie waeren eine eigene
groessere Scheibe (Scheduler + Skill-Store), kein Einschub in Phase 7.

## Abnahmematrix

Jede Zelle hat einen Zustand `not_run`, `passed`, `failed`, `unsupported` oder
`blocked` sowie Datum, Brain, sichtbares Modell, Selektorhash, Latenz und
Belegpfad.

| Bereich | Claude-Gate | Alle-Brains-Gate |
|---|---|---|
| Web-UI Chat | Mehrturn, New Chat, Resume | pro Katalogeintrag |
| Streaming | echte Deltas, Stop, Reconnect | pro Katalogeintrag, kein stilles Buffered-Pass |
| Modell | Runtime-Liste, Wahl, Nachpruefung | native Liste oder dokumentierte Grenze |
| Aufwand | Runtime-Liste, Wahl, Nachpruefung | native oder sichtbar gemappte WebAgent-Stufe |
| Attachment | TXT und PNG inhaltlich belegt | jede beworbene Modalitaet |
| Managed Tools | vier Tools, Policy, Exactly-once | vier Tools je Brain ueber UI und API |
| API Models | offizielles SDK | clientunabhaengig |
| API Chat | nonstream + stream + Negativfelder | je Brain |
| API Responses | Objekt + Event-Lifecycle + Fehler | je Brain |
| Gesundheit | Health-Balken, relative Balken, Backend-Detail, Session-Auswahl, keine Farballeincodierung | pro Katalogeintrag |
| Quellen | `/quelle` manuell, drei Profile nativ, <10-MB-Transport | pro Quelle |
| Gruppen | 2-6 Brains, Runden, `@Brain`, Leader-Synthese, deterministisch + echte Gruppe | pro Gruppe |
| Sicherheit | Workspace, Symlink/Junction, Shell, Abort | identische zentrale Policy |

## Aussagegrenze

Bis alle Pflichtzellen bestanden sind, lautet der Status nicht `100 %`, sondern
konkret beispielsweise `openai-chat-text-v1 passed for claude`. Ein einzelner
HTTP-200-Turn, ein SDK-Parserfolg oder ein erfolgreiches Tool mit ChatGPT ist
kein Gesamtbeleg.

Der Abschluss dieses Plans bedeutet:

- Web-UI ist die primaere und menschenfreundlich abgenommene Oberflaeche;
- TUI ist auf GitHub erhalten;
- die veroeffentlichten OpenAI-Inference-Profile sind semantisch und
  clientunabhaengig getestet;
- alle beworbenen Brains haben Chat, Streaming, ihre Modell-/Aufwandsabbildung,
  Anhaenge und alle vier Managed Tools ueber Web-UI und API bestanden.
