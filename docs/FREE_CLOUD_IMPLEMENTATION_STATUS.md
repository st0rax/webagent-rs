# Free-Cloud-Textchat: Implementierungs- und Abnahmestatus

> **Referenz.** Dieses Dokument beschreibt die aktuelle Free-Cloud-Arbeitsscheibe und ist zusammen mit dem getesteten Repositorystand zu lesen.

> **Stand:** 20. August 2026  
> **Arbeitsbaum:** `C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-harness-abnahme`  
> **Status:** Die lokale Registry-/Policy-Scheibe und ihr deterministischer Adaptervertrag sind implementiert, getestet und Ã¼ber die lokale CLI nachgewiesen. Sie ist **noch keine vollstÃ¤ndige Cloud-Chat-ProduktionseinfÃ¼hrung**, weil reale externe Inferenz, Browser-/Space-Adapter und Credential-Routen bewusst noch nicht implementiert sind.

## Umgesetzte Scheibe

Die erste sichere Umsetzung des Nutzerplans besteht aus `src/free_cloud_chat.rs`, der Modulregistrierung in `src/lib.rs` sowie einer lokalen `cloud`-CLI-Anbindung in `src/cli.rs` und `src/main.rs`. Der Code verwaltet ausschließlich normalisierte Metadaten, Profilzuordnung, erklärbare Custom-Suche und Routingentscheidungen. Er ruft **keine** Inferenzschnittstelle auf, automatisiert keine Weboberfläche und überträgt keine Chatnachricht an externe Quellen.

| Element | Umgesetzt | Abnahmebeleg |
|---|---:|---|
| Versionierte Modellregistry | Ja | `CloudModel`, `AccessMode`, `ModelProfile`, `SearchResult` |
| Free-only-Policy | Ja | Nur `VerifiedFree` kann bei `free_only=true` automatisch routen |
| Credit-/Manual-/Unavailable-Sperre | Ja | `ExplicitCredits`, `ManualOnly`, `Unavailable` ergeben `ManualOnly` bzw. `Unavailable` |
| Erklärbare Custom-Suche | Ja | Treffergründe, Score, `manual_only`-Markierung |
| Unicode-Tokenisierung | Ja | Test mit Rust-Unicode-Escapes für `Ü`, `ü`, `ä` |
| Mehrbegriff-Ranking | Ja | `u32`-Score und Test für zusätzliche reale Treffer |
| Lokale CLI-Struktur | Ja, kompiliert | `cloud list`, `cloud search`, `cloud decide` |
| Externe Inferenz, Streaming, Spaces-Adapter | Nein, bewusst zurückgestellt | Kein Token, keine Browserautomation, keine Kostenroute |

## Kosten- und Sicherheitsentscheidung

Hugging Face dokumentiert für freie Konten ein monatliches Testguthaben von 0,10 US-Dollar; nach dessen Verbrauch ist nutzungsabhängige Bezahlung möglich. Deshalb wird Hugging-Face-Inference in der Registry als `ExplicitCredits` geführt und im Standardmodus **nicht** automatisch geroutet. Öffentliche Spaces und HuggingChat sind aktuell `ManualOnly`. Erst ein wiederholt technisch belegter und aktuell geprüfter Adapter darf in den Status `VerifiedFree` wechseln.[1]

> **Verbindliche Invariante:** Im Modus `free_only=true` führt nur `VerifiedFree` **und** `adapter_compatible=true` zu `RouteDecision::Auto`. Jeder andere Status wird vor einer Adapterausführung zurückgewiesen.

Die Custom-Suche ist vorerst rein lokal. Der spätere Hub-Metadatenadapter kann die offizielle Modell-/Space-Suche verwenden, muss aber vor jeder automatischen Ausführung weiterhin Kostenstatus, Adaptervertrag, Healthcheck und Datenschutzangaben prüfen.[2] [3]

## Unabhängige Prüfung

| Prüfinstanz | Auftrag | Ergebnis | Übernahme |
|---|---|---|---|
| Claude Opus | Root-Cause-Review zum Cloud-Startpfad | Erkannte die eager TUI-Fallback-Konstruktion und anschließend Pre-Dispatch-Nebenwirkungen; gab ein positives Free-only-Micro-Review | `unwrap_or_else`; Startpfadtrennung dokumentiert; Policy positiv gegengeprüft |
| OpenCode | Read-only-Rust-Review | Empfahl größere Scores, Quell-URL im Suchhaystack und einen Unicode-Test | `u32`-Score, `source_url`-Suche, Unicode- und Mehrbegriffstests übernommen |
| Grok Build | Read-only-RCA | Sitzung lieferte innerhalb zweier Wartefenster keine Ergebnisantwort | Nicht als Befund verwertet |
| Codex | Read-only-Uncommitted-Review | Lokaler Client scheiterte vor dem Review, weil sein konfiguriertes Modell nicht für die vorhandene ChatGPT-Anmeldung unterstützt ist | Nicht als Befund verwertet |

Claude Opus hat die aktuelle Kerninvariante explizit geprüft und bestätigt: `Unavailable` wird zuerst abgewiesen; die Free-only-Gate akzeptiert ausschließlich `VerifiedFree`; anschließend bleibt `adapter_compatible` Pflicht. Die neue `u32`-Bewertung vermeidet die frühe Sättigung des ehemaligen `u8`-Scores. Die Unicode-Testdaten verwenden Rust-Escapes und sind damit unabhängig von Windows-Konsolenencoding.

## Testnachweise

Die vollstÃ¤ndige PrÃ¼fung lief mit dem Windows-GNU-Linker und ergab ein erfolgreiches `cargo test --no-default-features`: **1.047 Tests bestanden**. `rustfmt --check src\\free_cloud_chat.rs` sowie `cargo clippy --no-default-features` liefen ebenfalls erfolgreich; vorbestehende Warnungen in nicht verwandten TUI-/Brain-Wall-Modulen wurden nicht ausgeweitet.

Der vormals blockierende Baselinefehler wurde durch eine isolierte Ursachenanalyse behoben: Das Generieren der groÃŸen Clap-Command-Struktur Ã¼berlief im Windows-GNU-Main-Thread, ist aber auf einem ausdrÃ¼cklich 64 MiB groÃŸen Rust-Worker-Stack vollstÃ¤ndig terminierend. Der Nicht-WebView-Programmeinstieg startet `run()` daher auf diesem Worker-Stack; der WebView-Pfad bleibt auf dem Hauptthread. Danach liefen `webagent --version`, `webagent cloud search --profile custom --query "huggingface api"` und `webagent cloud decide --model-id huggingface/inference-providers` erfolgreich. Die letzte Ausgabe bestÃ¤tigt dabei `manual_only` fÃ¼r den kreditpflichtigen Provider.

## Lokaler Adaptervertrag

`src/free_cloud_chat.rs` enthÃ¤lt jetzt den providerneutralen `TextStreamAdapter`-Vertrag, `TextStreamRequest`, `TextStreamEvent` und klar typisierte Fehler. Als einzige Implementierung existiert `DeterministicMockAdapter` fÃ¼r `webagent/local-mock-stream`. Der Adapter verwendet ausschlieÃŸlich In-Memory-Strings und hat weder HTTP-, Browser-, Credential- noch Providerzugriff. Vor jedem Event erzwingt er `decide_route`; eine verweigerte Free-only-Route liefert einen Fehler und keine Teilereignisse.

Der explizite CLI-Nachweis lautet `webagent cloud mock-stream --prompt "Hallo Vertrag"`. Die Ausgabe benennt `deterministic_local_mock` und `network_access: false`. Ein Leerzeichenprompt endet mit Code 2. Unit-Tests decken den erfolgreichen Stream, die verweigerte Credit-Route und den leeren Prompt ab. Claude Opus und OpenCode bewerteten die Scheibe unabhÃ¤ngig ohne Hoch- oder Mittelrisikofund.
## Versionierter Hub-Metadatenvertrag

`src/free_cloud_chat.rs` enthÃ¤lt nun den versionierten, **lokal testbaren** Hub-Metadatenvertrag. `HubMetadataRequest` trÃ¤gt ausschlieÃŸlich die Schema-Version, die Modellkennung und die explizite Zugriffsrichtlinie; er enthÃ¤lt und speichert niemals Credentials. `HubMetadataTransport` ist eine injizierbare Nahtstelle, deren einzige bereitgestellte Implementierung `StaticHubMetadataTransport` ist. Diese Fixture arbeitet ausschlieÃŸlich im Prozess und ruft weder HTTP noch Browser noch einen Hub-Provider auf.

`HubMetadataAdapter<T>` implementiert den bestehenden `TextStreamAdapter`. Vor jedem Transportzugriff und vor jedem Event erzwingt er `decide_route()`. AnschlieÃŸend validiert er Adaptermodell, angefragte Hub-Modellkennung und positive Maximalaltergrenze. Nach der Fixture-Antwort validiert er Schema, ModellidentitÃ¤t und `last_verified_at`; abgelaufene, extrem alte und aus der Zukunft stammende Zeitstempel werden ohne Teilereignisse abgewiesen. Die Erfolgsausgabe verwendet explizit `TextStreamEvent::Metadata` statt eines Inferenz-Tokens, sodass Metadatenauskunft und Textgenerierung im Protokoll nicht verwechselt werden.

Die Zugriffsrichtlinie ist standardmÃ¤ÃŸig `public_only`. `with_user_credential_opt_in()` setzt nur eine deklarative spÃ¤tere Transportrichtlinie und enthÃ¤lt weiterhin keinen Token-, Browser- oder Netzwerkpfad. Lokale Tests decken eine frische Fixture, eine Credit-Route mit absichtlich panischem Transport, einen abgelaufenen Snapshot, den i64-Extremwert, einen Zukunftszeitstempel sowie das explizite Credential-Opt-in ab. Claude Opus und OpenCode prÃ¼ften die Scheibe unabhÃ¤ngig; ein von Opus gefundener arithmetischer Grenzfall wurde mit `saturating_sub` samt Regressionstest korrigiert.
## Lokaler Healthcheck- und Circuit-Breaker-Vertrag

Der Free-Cloud-Pfad besitzt jetzt einen separaten, deterministischen Health- und Circuit-Breaker-Vertrag. Er ist bewusst **nicht** der vorhandene `src/circuit_breaker.rs`: Dieser bleibt fÃ¼r Brain-/Relay-Sitzungen zustÃ¤ndig, wÃ¤hrend `FreeCloudCircuitBreaker` ausschlieÃŸlich einen kÃ¼nftigen externen Free-Cloud-Adapter absichert. `FreeCloudHealthPolicy` ist versioniert, validiert Schwelle und Cooldown vor jeder Probe und erhÃ¤lt die Zeit als expliziten i64-Wert. Dadurch sind Tests und ein spÃ¤terer Produktionsclock identisch nachvollziehbar.

`AdapterHealthProbe` ist nur eine injizierbare Nahtstelle; die bereitgestellte `StaticAdapterHealthProbe` arbeitet ausschlieÃŸlich im Prozess. Der Breaker durchlÃ¤uft `closed`, `open` und `half_open`. Eine offene Schaltung unterbindet die Probe vollstÃ¤ndig und liefert `CircuitOpen` mit deterministischem `retry_at`; nach dem Cooldown lÃ¤sst genau eine Half-Open-Probe die RÃ¼ckkehr zu `closed` oder das erneute Ã–ffnen zu. Zeitberechnungen verwenden sÃ¤ttigende Addition, Failure-ZÃ¤hler sÃ¤ttigende Addition. Die erfolgreiche, gesperrte, Half-Open-erfolgreiche, Half-Open-fehlerhafte und ungÃ¼ltige Policy werden per lokalen Unit-Tests abgedeckt.

Claude Opus und OpenCode prÃ¼ften die Zustandsgrenzen unabhÃ¤ngig. Der von OpenCode hervorgehobene ZÃ¤hlerfortschritt nach einem Half-Open-Fehler wurde mit einer expliziten Assertion korrigiert. Die Scheibe enthÃ¤lt keine Persistenz, keine Timeout-/Panikkapselung und keinen Live-Health-Probe; diese Eigenschaften gehÃ¶ren erst zu einem ausdrÃ¼cklich freigegebenen I/O-Adapter.
## NÃ¤chste Abnahmeschritte

Der nÃ¤chste **externe** Schritt wÃ¤re ein echter Hub-Transport mit ausdrÃ¼cklicher Nutzerfreigabe, einer I/O-sicheren Health-Probe Ã¼ber den vorhandenen Breaker-Vertrag, parserseitigen Schema-/DatenschutzprÃ¼fungen und dokumentierten Kosten-/Freiheitsnachweisen. Dieser Schritt ist nicht Teil der aktuellen Scheibe und wird weder automatisch noch ohne ausdrÃ¼ckliche Freigabe ausgefÃ¼hrt. Erst danach kann ein realer Streaming-Adapter geprÃ¼ft werden. Kostenpflichtige Fallbacks bleiben im Free-only-Modus deaktiviert.

## Quellen

[1] Hugging Face, „Pricing and Billing“: <https://huggingface.co/docs/inference-providers/en/pricing>  
[2] Hugging Face, „Search the Hub“: <https://huggingface.co/docs/huggingface_hub/en/guides/search>  
[3] Hugging Face, „Hub API Endpoints“: <https://huggingface.co/docs/hub/en/api>
