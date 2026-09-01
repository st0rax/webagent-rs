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

- **linke, einklappbare Leiste:** Sitzungen, neuer Chat, klarer Status;
- **ruhige Hauptspalte:** Nutzer-/Brain-Nachrichten und laufende Antwort;
- **kompakte Kopfzeile:** Brain, tatsaechlich erkanntes Modell, Aufwand und
  Modus `Chat`/`Systemzugriff`;
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
2. Session-, Capability-, Upload-, Chat-, Stop- und Event-Endpunkte.
3. Klickbaren Fake-Prototyp mit responsivem Layout und A11y-Gates abnehmen.
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
