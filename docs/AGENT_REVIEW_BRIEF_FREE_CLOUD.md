# WebAgent – unabhängiges Reviewbriefing

> **Referenz.** Dieses Dokument beschreibt die aktuelle Free-Cloud-Arbeitsscheibe und ist zusammen mit dem getesteten Repositorystand zu lesen.

> **Status:** Arbeitsdokument für vier unabhängige Read-only-Reviews.  
> **Arbeitsbaum:** `C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-harness-abnahme`  
> **Geltung:** Nur die neue Free-Cloud-Textchat-Scheibe und ihre direkte CLI-Anbindung.

## Verbindliche Grenzen

Jeder Agent arbeitet ausschließlich lesend. Keine Datei darf geändert, kein Commit erstellt, kein Browser geöffnet, kein Account berührt und kein externer Modell-/Provideraufruf vorgenommen werden. Der Arbeitsbaum enthält absichtlich uncommittete Änderungen; sie sind ausschließlich Gegenstand der Prüfung.

Der Review bezieht sich auf `src/free_cloud_chat.rs`, die neue `cloud`-CLI-Anbindung in `src/cli.rs` und `src/main.rs` sowie den Nutzerplan `docs/WEBAGENT_FREE_CLOUD_TEXTCHAT_PLAN.md`. Der bestehende `api`-Befehl und die Browser-Bridge dürfen nicht umgebaut oder als Cloud-Adapter missverstanden werden.

## Festes Produktziel

Die neue Scheibe liefert eine lokal ausführbare Modellregistry und eine transparente Profil-/Custom-Suche. Sie führt **keine** externe Inferenz und keine Browserautomation aus. Im Standardmodus `Free-only` dürfen nur technisch wiederholt nachgewiesene kostenlose Adapter automatisch geroutet werden. HuggingChat und öffentliche Spaces bleiben manuell. Hugging-Face Inference Providers gelten wegen begrenzter, veränderlicher Testguthaben als `ExplicitCredits` und dürfen im Free-only-Modus nicht automatisch ausgeführt werden.

## Bekannter Implementierungsstand

| Baustein | Erwarteter Zustand |
|---|---|
| Registry | `CloudModel` mit Quelle, Profilen, Sprachen, Tags, Zugriffsmodus, Adapterstatus und Prüfzeitpunkt. |
| Free-only-Policy | `VerifiedFree` ist die einzige automatische Route. `ExplicitCredits`, `ManualOnly` und `Unavailable` ergeben keine automatische Ausführung. |
| Custom-Suche | Rein lokale, erklärbare Metadatensuche mit Treffergründen und `manual_only`-Markierung. Nicht passende Begriffe dürfen kein Ergebnis allein wegen allgemeiner Metadaten erzeugen. |
| CLI | `webagent cloud list`, `cloud search` und `cloud decide`; alle Aufrufe sind lokal und JSON-basiert. |
| Startup | `cloud` umgeht den globalen TUI-/Konfigurationsstart, damit die lokale Registry ohne Browser-/Profilinitialisierung ausführbar bleibt. |
| Testbasis | Unit-Tests in `src/free_cloud_chat.rs`; bestehende Projekte enthalten vorbestehende Warnungen und globale Formatabweichungen, die nicht Bestandteil dieser Scheibe sind. |

## Rollen und konkrete Prüfaufträge

### Claude Opus – Architektur, Sicherheitsgrenzen und Adaptervertrag

Prüfe, ob die aktuelle Scheibe den Nutzerplan korrekt in einen sicheren MVP übersetzt. Suche insbesondere nach Kostenrisiken, irreführenden „kostenlos“-Zusagen, unklaren Trust-Grenzen, fehlenden Adapter-/Streaming-Verträgen und State-/Privacy-Risiken beim späteren Ausbau. Liefere maximal zehn Befunde als Tabelle mit **Schweregrad**, **Datei:Zeile**, **Begründung**, **minimaler Korrektur** und **Testidee**.

### Grok Build – Produkt-MVP und Nutzerführung

Prüfe, ob Profilwahl, Custom-Suche, Treffergründe, `manual_only` und die CLI-Ausgaben für einen Nutzer verständlich und korrekt differenziert sind. Bewerte auch, ob die MVP-Reihenfolge des Plans sinnvoll abgesichert wurde. Liefere maximal acht priorisierte Befunde, keine Implementierung.

### OpenCode – Rust-Korrektheit und Wartbarkeit

Prüfe Datenmodell, Suchfilter, Sortierung, Fehlerpfade, CLI-Argumente, JSON-Serialisierung, Root-Command-Dispatch und die gezielte Startup-Ausnahme. Suche nach Compiler-, Ownership-, Unicode-, Reihenfolge- und Testlücken. Liefere maximal zehn file-/line-belegte Befunde und konkrete Testfälle.

### Codex – adversariales QA-Review

Entwickle einen knappen Negativtestkatalog: Free-only-/Credit-Verwechslung, unbekannte IDs/Profiles, leere und nicht passende Suchen, manipulative Suchbegriffe, manuelle Quellen, unerwartete Startup-Nebeneffekte sowie JSON-Kompatibilität. Ordne jeden Test in **muss bestehen**, **muss sauber ablehnen** oder **späterer Ausbau** ein.

## Gemeinsame Akzeptanzkriterien

| Kriterium | Nachweis |
|---|---|
| Keine automatische Credit-/Kostenroute im Free-only-Modus | Unit-Test und `cloud decide`-Ausgabe. |
| Keine externe Inferenz oder Browserautomation | Codepfad-Review; CLI darf nur lokale Registry lesen. |
| Nicht passende nichtleere Suche liefert keine irreführenden Treffer | Unit-Test. |
| Jede Routingentscheidung erklärt den Zugriffsstatus | JSON-Ausgabe enthält Entscheidung und Begründung. |
| CLI startet ohne TUI-/Browser-Nebeneffekt | Lokaler `cloud search`-Smoke-Test. |
| Bestehende Browser-API-Bridge bleibt getrennt | Diff-/Architekturreview. |

## Antwortformat

Antworte ausschließlich mit dem Review. Keine Dateien ändern, keine Shell-/Browseraktion ausführen und keine Webrecherche anstoßen. Beginne mit einem Ein-Satz-Urteil (`freigabefähig mit ...`, `nicht freigabefähig wegen ...`) und liefere anschließend die verlangte priorisierte Tabelle.
