# Free-Cloud-Textchat: Implementierungs- und Abnahmestatus

> **Stand:** 20. August 2026  
> **Arbeitsbaum:** `C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-harness-abnahme`  
> **Status:** Die lokale Registry-/Policy-Scheibe ist implementiert und gezielt getestet. Sie ist **noch keine vollständige Cloud-Chat-Produktionseinführung**, weil ein bereits im Baseline-Commit vorhandener Windows-/GNU-Startfehler jeden Binary-Smoke-Test (`--version`) blockiert.

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

Die zielgerichtete Prüfung lief mit dem Windows-GNU-Linker und ergab ein erfolgreiches `cargo test --no-default-features free_cloud`; sechs Modultests bestanden. `rustfmt --check src\free_cloud_chat.rs` bestand ebenfalls. Vorbestehende Warnungen in nicht verwandten TUI-/Brain-Wall-Modulen wurden protokolliert, aber nicht als Teil dieser Scheibe geändert.

Der geplante CLI-Smoke-Test kann gegenwärtig nicht als Erfolg belegt werden. In einem **frisch erzeugten, unveränderten** Worktree auf Commit `b764d49` schlug bereits `cargo run --no-default-features -- --version` mit `STATUS_STACK_OVERFLOW (0xc00000fd)` fehl. Derselbe Fehler tritt im Abnahmearbeitsbaum auf. Damit ist der Ausfall unabhängig von der Cloud-Diff und muss als eigenständige Windows-/GNU-Harness-Lücke behandelt werden.

## Nächste Abnahmeschritte

Die nächste technische Scheibe ist nicht ein Live-Provider, sondern eine isolierte Reparatur des Baseline-Stack-Overflows mit einem minimalen `--version`-Regressionstest. Erst nach diesem Nachweis dürfen `cloud list`, `cloud search` und `cloud decide` als Binary-Smoke-Test freigegeben werden. Danach folgen ein versionierter Hub-Metadatenadapter, ein expliciter Opt-in für Nutzer-Credentials, Healthcheck/Circuit-Breaker und schließlich ein Streaming-Adapter. Kostenpflichtige Fallbacks bleiben im Free-only-Modus deaktiviert.

## Quellen

[1] Hugging Face, „Pricing and Billing“: <https://huggingface.co/docs/inference-providers/en/pricing>  
[2] Hugging Face, „Search the Hub“: <https://huggingface.co/docs/huggingface_hub/en/guides/search>  
[3] Hugging Face, „Hub API Endpoints“: <https://huggingface.co/docs/hub/en/api>
