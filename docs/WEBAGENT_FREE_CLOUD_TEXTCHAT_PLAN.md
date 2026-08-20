# WebAgent: Plan für kostenlose Cloud-Textchats mit offenen Modellen

> **Referenz.** Dieses Dokument beschreibt die aktuelle Free-Cloud-Arbeitsscheibe und ist zusammen mit dem getesteten Repositorystand zu lesen.

> **Finale Fassung — Stand: 20. August 2026**

## Ziel

WebAgent erhält einen Chatbereich, in dem Nutzer zwischen mehreren kostenlosen Cloud-Modellen wählen oder automatisch routen lassen können. Der Schwerpunkt liegt auf HuggingChat und Hugging-Face-kompatiblen offenen Modellen. Die Modelle werden nach **Fähigkeitsprofilen** angezeigt, nicht als vermeintlich „filterfrei“ beworben: schnell, deutschsprachig, Reasoning/Code, kreativ und datensparsam.

Die Integration soll keine Sicherheitsfilter umgehen und keine Zusage machen, dass ein Modell niemals ablehnt. Sie soll jedoch verhindern, dass der gesamte WebAgent an ein einziges Modell mit einem einzigen Antwortstil gebunden ist.

## Cloud-Optionen

| Option | Rolle in WebAgent | Kostenlosigkeit | Integrationsweg |
|---|---|---|---|
| [HuggingChat](https://huggingface.co/chat) | Manuelle Referenzoberfläche und Modellkatalog; die Seite zeigt aktuell viele auswählbare Modelle und einen automatischen Omni-Modus | Als Chatoberfläche kostenlos nutzbar; konkrete Limits und Modellverfügbarkeit können sich ändern | Nicht als versteckte Browserautomation integrieren. Für WebAgent entweder Deep Link anbieten oder offizielle Inference-Schnittstelle verwenden. |
| [Hugging-Face Inference Providers](https://huggingface.co/docs/inference-providers/index) | API-Schicht für Chat Completion und mehrere offene Modelle | Kostenlose Konten erhalten laut offizieller Preisseite ein kleines monatliches Testguthaben; danach kostenpflichtig | **Für „nur gratis“ standardmäßig deaktivieren**, außer der Nutzer stellt eigene kostenlose Provider-Credentials bereit. |
| Öffentliche Hugging-Face-Chat-Spaces | Einfache Web-Demos und experimentelle Modellzugänge | Häufig kostenlos, aber Space-, Queue- und Rate-Limits sind nicht zentral garantiert | Nur über explizit geprüfte Gradio-Endpunkte und mit manueller Fallback-Option. |
| [OpenRouter](https://openrouter.ai/pricing) | Vergleichs- und Fallbackschicht für Cloud-Chat | Kostenlose Stufe mit kostenlosen Modellen und Tageslimit; nicht Hugging-Face-only | Nur als optionaler späterer Adapter; im „HF-only“-MVP deaktiviert. |

## Modellprofile

WebAgent sollte keine unklare Liste von Modellnamen anzeigen, sondern Fähigkeiten und Herkunft transparent machen. Ein Modell kann mehreren Profilen zugeordnet werden.

| Profil | Priorität | Typische Modellfamilien | Geeignet für |
|---|---:|---|---|
| Schnell | 1 | kleine Qwen-, Gemma-, Mistral- oder Llama-Varianten | kurze Fragen, UI-Assistenz und schnelle Entwürfe |
| Deutsch | 1 | Qwen-, Mistral- und Llama-Varianten mit guter mehrsprachiger Leistung | deutsche Konversation, Übersetzung und Zusammenfassung |
| Reasoning/Code | 1 | DeepSeek-, Qwen- oder vergleichbare Reasoning-Modelle | Planung, Debugging, technische Analyse |
| Kreativ | 2 | Qwen-, Mistral- oder Llama-Varianten mit längerer Ausgabe | Ideation, Rollenspiel im zulässigen Rahmen und Textentwürfe |
| Datensparsam | 1 | Provider/Space mit dokumentierter kurzer Speicherung oder lokaler Übergabe | sensible, aber zulässige Textinhalte |
| Custom | 1 | vom Nutzer gesuchte Hugging-Face-Modelle oder Spaces | gezielte Suche nach Aufgabe, Sprache, Modellfamilie, Lizenz oder sonstigen Metadaten |

Die konkrete Modellliste wird beim Start aus einer versionierten Registry geladen und nicht dauerhaft im Frontend festgeschrieben. Für jedes Modell werden `model_id`, Quelle, Provider, Sprache, Kontextfenster, erwartete Latenz, Free-Tier-Status, letzte Prüfung und Ausfallstatus gespeichert.

Im **Custom-Profil** zeigt WebAgent zunächst ein Suchfeld an. Der Nutzer kann beispielsweise `deutsch code schnell`, `Qwen reasoning`, `roleplay creative`, `long context` oder einen konkreten Modellnamen eingeben. Die Suchbegriffe werden gegen Hugging-Face-Metadaten abgeglichen: Modellname, Tags, Pipeline- beziehungsweise Aufgabentyp, Modellbeschreibung, Sprache, Lizenz, Quantisierung, Provider-/Space-Status und dokumentierte Inferenzmethode. Die Suche liefert keine automatische Ausführung, sondern eine Ergebnisliste mit Trefferbegründung.

Jeder Treffer enthält mindestens Modell- oder Space-Name, Autor, Kurzbeschreibung, Aufgabenart, unterstützte Sprachen, Lizenz, geschätzte Zugänglichkeit, kostenlosen Status, zuletzt geprüften Zeitpunkt, Link zur Originalseite und einen Button **„Auswählen“**. Erst nach dieser expliziten Auswahl wird der Eintrag für die Chat-Session aktiviert. Nicht eindeutig kostenlose, nicht erreichbare oder technisch nicht adapterkompatible Treffer werden als `manual_only` angezeigt und nicht automatisch geroutet.

## Custom-Suche und Treffer-Ranking

Die Custom-Suche läuft serverseitig über die offizielle Hugging-Face-Such- beziehungsweise Hub-Schnittstelle oder über eine periodisch aktualisierte Metadatenkopie. WebAgent speichert dabei nur normalisierte Metadaten und keine fremden Modellartefakte. Ein einfaches transparentes Ranking kann wie folgt berechnet werden: 35 Prozent semantische oder textuelle Übereinstimmung mit den Suchbegriffen, 20 Prozent Aufgaben-/Pipeline-Übereinstimmung, 15 Prozent Sprachübereinstimmung, 10 Prozent Adapterkompatibilität, 10 Prozent nachweisbarer Free-only-Status und 10 Prozent aktuelle Verfügbarkeit. Die Gewichte sollen in der Registry versioniert und in der UI erklärbar sein.

Die Suche darf auch Suchbegriffe wie `uncensored` oder `weniger restriktiv` als reine Metadatenanfrage behandeln, aber nicht als Zusage eines Sicherheits- oder Policy-Bypasses. Das Ergebnis wird deshalb nach technischer Eignung, Lizenz, Verfügbarkeit und dokumentierter Modellbeschreibung angezeigt; WebAgent setzt weiterhin seine eigenen Sicherheitsgrenzen für illegale, nicht-einvernehmliche oder gefährliche Inhalte durch.

## Zielarchitektur

```text
Browser
  │  SSE/WebSocket oder Polling
  ▼
WebAgent Chat API
  ├── Session- und Nachrichtenvalidierung
  ├── Profil-/Modellauswahl
  ├── Free-only-Policy
  ├── Quoten- und Tageslimitprüfung
  ├── Modellrouter
  ├── Streaming-Adapter
  └── Fehlerklassifikation und Fallback
  │
  ▼
Hugging-Face-Adapter
  ├── HuggingChat: Deep Link/Referenz, nicht automatisierte UI-Steuerung
  ├── HF Inference: nur explizit freigegebene kostenlose Route
  └── geprüfte öffentliche Gradio-Chat-Spaces
  │
  ▼
Browser: tokenweises Streaming, Modellname und Status sichtbar
```

Der Browser darf keine geheimen API-Schlüssel erhalten. Wenn eine offizielle Inference-Route benötigt wird, ruft das Backend sie auf. Für öffentliche Spaces ohne Authentifizierung sollte WebAgent nur die minimal erforderlichen Daten übertragen und den Space-Namen sichtbar anzeigen.

## Adaptervertrag

Jede Cloud-Quelle erhält einen einheitlichen Adapter:

```text
list_capabilities() -> ModelCapabilities
start_chat(session, messages, parameters) -> ChatJob
stream(chat_job) -> TokenEvents
cancel(chat_job) -> CancelResult
healthcheck() -> HealthReport
```

Die normalisierte Antwort enthält `model_id`, `provider`, `started_at`, `first_token_ms`, `finish_reason`, `usage_if_available` und eine technische `error_class`. Provider-spezifische Fehler werden nicht als Modellinhalt an den Nutzer vermischt.

## Routing und Fallback

Der Nutzer kann ein Profil auswählen, „Auto“ verwenden oder im **Custom-Profil** einen Treffer aus der Suchliste auswählen. Im Auto-Modus wird zunächst nach Profil, Sprache, Kontextlänge und aktueller Gesundheit gefiltert. Im Custom-Modus wird ausschließlich der vom Nutzer bestätigte Eintrag verwendet. Danach wird nur ein Modell angefragt. Bei `timeout`, `queue_unavailable`, `provider_error` oder `schema_error` ist höchstens ein definierter technischer Fallback erlaubt. Bei einer inhaltlichen Ablehnung wird nicht automatisch mit aggressiverem Prompt, anderem Systemprompt oder einer langen Kette von Modellen weiterprobiert.

Ein Circuit Breaker setzt einen fehlerhaften Space zeitweise auf `degraded` oder `manual_only`. Die UI zeigt dann beispielsweise: „Dieser kostenlose Space ist momentan überlastet; bitte anderes Profil wählen oder später erneut versuchen.“

## Streaming und Nutzeroberfläche

Die Chatoberfläche zeigt neben jeder Antwort das verwendete Modell und einen Status an. Der Nutzer kann zwischen Auto, Schnell, Deutsch, Reasoning/Code und Kreativ wechseln. Während des Streams werden Tokens inkrementell über SSE oder WebSocket ausgegeben. Bei öffentlichen Gradio-Spaces, die kein zuverlässiges Streaming anbieten, verwendet der Adapter einen asynchronen Jobstatus mit Polling und einer klaren Wartemeldung.

Die Oberfläche sollte außerdem eine Schaltfläche „Modell wechseln“ und einen Export der Unterhaltung anbieten. Ein Modellwechsel darf die bisherige Unterhaltung übernehmen, muss aber das neue Modell deutlich markieren.

## Quoten und nur kostenlose Nutzung

Da kostenlose Cloud-Angebote schwankende Limits besitzen, führt WebAgent eigene Schutzgrenzen ein: maximale Nachrichtenlänge, maximales Kontextfenster, tägliche Anfragen pro Nutzer und ein globales Budget an parallelen Jobs. Kostenpflichtige Provider werden im Backend nicht als Fallback konfiguriert. Wenn ein kostenloses Kontingent nicht sicher feststellbar ist, wird der Adapter deaktiviert oder als manueller Link angezeigt.

Ein täglicher Healthcheck aktualisiert die Modellregistry, aber nicht bei jeder Nutzeranfrage. Dadurch werden unnötige Provideraufrufe vermieden. Die konkrete kostenlose Verfügbarkeit muss beim Deployment erneut geprüft werden, weil Anbieter ihre Quoten und Modelle ändern können.

## Datenschutz und Sicherheit

Nachrichten werden standardmäßig nur so lange gespeichert, wie die Chat-Session aktiv ist. Für persistente Historien ist eine gesonderte Nutzerentscheidung erforderlich. Provider, Space-URL und mögliche Speicherung werden vor dem ersten externen Versand angezeigt. Tokens werden ausschließlich serverseitig als Secrets verwaltet.

Die Policy-Schicht dient nicht dazu, Inhalte künstlich zu verschärfen, sondern um WebAgent zuverlässig von illegalen oder nicht-einvernehmlichen Anfragen fernzuhalten. Insbesondere Minderjährige, nicht-einvernehmliche Sexualisierung, Identitätsmissbrauch, Doxxing und gefährliche Anleitungen werden unabhängig vom gewählten Modell blockiert. Legale allgemeine Kreativ-, Technik-, Übersetzungs- und Rollenspielanfragen können an das ausgewählte offene Modell weitergeleitet werden.

## Benchmark und Abnahme

Vor dem Livegang wird ein nicht-sensitives Testset mit deutschen Fragen, Codeaufgaben, Zusammenfassungen, langen Kontexten und kreativen Schreibaufgaben ausgeführt. Gemessen werden:

| Kriterium | Ziel |
|---|---:|
| Erfolgreiche Antwort | mindestens 90 % bei wiederholten Standardtests |
| Time-to-first-token | Median und p95 pro Modellprofil protokollieren |
| Deutschqualität | Blindbewertung auf Verständlichkeit und Grammatik |
| Codequalität | feste Unit-Test-Aufgaben statt subjektiver Bewertung |
| Kontexttreue | Fakten aus einem kontrollierten Dokument korrekt wiedergeben |
| Routingtreffer | gewähltes Profil führt überwiegend zum erwarteten Modelltyp |
| Fallbackverhalten | kein doppeltes Senden und maximal ein technischer Fallback |
| Kostenkontrolle | keine kostenpflichtigen Provider im Free-only-Modus |

Die Registry erhält außerdem einen `last_verified_at`-Zeitpunkt. Ein Modell wird aus dem automatischen Routing entfernt, wenn es wiederholt ausfällt, seine API ändert oder sein kostenloser Status nicht mehr nachweisbar ist.

## MVP-Reihenfolge

**Phase 1:** Chat-UI mit Profilwahl, Modellname, Streaming-Schnittstelle und einem Mock-Adapter.  
**Phase 2:** Ein offizieller kostenloser Hugging-Face-kompatibler Chat-Zugang mit serverseitigem Secret und harten Free-only-Grenzen.  
**Phase 3:** Zweites und drittes Modellprofil, Healthchecks, Circuit Breaker und ein technischer Fallback.  
**Phase 3b:** Custom-Suchfeld, Hugging-Face-Metadatenadapter, transparentes Ranking, Ergebnisdetails und expliziter Auswahl-Button.  
**Phase 4:** Benchmark-Dashboard, Export, Session-Löschung und transparente Providerinformationen.  
**Phase 5:** Optionaler OpenRouter-Adapter, nur wenn der Nutzer ausdrücklich akzeptiert, dass dies nicht mehr „Hugging Face only“ ist.

## Wichtige Entscheidung vor der Implementierung

Da WebAgent als Projektname genannt wurde, muss vor dem Bauen geklärt werden, ob bereits ein bestehendes WebAgent-Projekt geöffnet werden soll oder ob ein neues Webprojekt angelegt werden darf. Diese Entscheidung beeinflusst die tatsächliche Backend- und Hostingstruktur. Der Plan selbst setzt noch keine konkrete Framework- oder Deployment-Architektur voraus.

## Quellen

[1]: https://huggingface.co/chat "HuggingChat"  
[2]: https://huggingface.co/docs/inference-providers/index "Hugging Face Inference Providers"  
[3]: https://huggingface.co/docs/inference-providers/pricing "Hugging Face Inference Providers: Pricing and Billing"  
[4]: https://huggingface.co/docs/hub/spaces-zerogpu "Hugging Face Spaces ZeroGPU"  
[5]: https://openrouter.ai/pricing "OpenRouter Pricing"
