# Gesamtübergabe von Codex an Claude

> **Archiv seit 2026-08-22.** Der dauerhafte Einstieg ist
> [`../START_HERE.md`](../START_HERE.md), der laufende Stand steht in
> [`CURRENT_WORK.md`](CURRENT_WORK.md). Dieses Dokument bleibt als datierter
> Übergabebeleg erhalten und wird nicht fortgeschrieben.

**Datum:** 2026-08-22
**Projekt:** `webagent-rs`
**Status:** konsolidierter lokaler Übergabestand; Gesamtprojekt nicht fertig
**Neuer schreibender Integrator:** Claude

## 1. Autoritativer Einstiegspunkt

- Worktree: `C:\Users\storax\Documents\Codex\2026-08-21\man\work\webagent-codex-takeover`
- Branch: `codex/project-takeover`
- Inhaltlicher Integrations-HEAD vor diesem Übergabedokument: `2cf952e`
- Upstream-Basis: `origin/master` bei `9bf57f5`
- Der Branch war vor dem Übergabedokument fünf Commits vor `origin/master` und
  sauber. Der Commit dieses Dokuments ist anschließend der neue Branch-Tip.
- Es wurde nicht gepusht, gemergt, getaggt, released oder deployed.

Claude übernimmt ab diesem Dokument allein die schreibende Integration. Der
alte Claude-Branch `claude/writeback-restore-tests` muss nicht weitergeführt
oder erneut rebased werden; sein einziger relevanter Commit ist bereits als
`2cf952e` integriert.

## 2. Was das Produkt ist

Webagent ist ein lokaler, provider-agnostischer Rust-Harness für autonome
Web-Chat-Brains. Der Harness stellt Session, Kontext, Werkzeuge, Persistenz,
Plan–Act–Observe-Steuerung, Sicherheitsgrenzen, Capability-Evidenz,
Multi-Brain-Orchestrierung sowie Benchmark-/Selbstverbesserungspfade bereit.
Das produktive Browserbackend steuert angemeldete Web-Sessions über Embedded
WebView2; Kern, REPL und TUI besitzen zusätzlich Headless-/Nicht-WebView-Pfade.

Die aktuelle Produktwahrheit steht in `docs/OVERVIEW.md`. Historische Plan-,
Status- und Übergabedokumente sind keine automatische Soll-Quelle. Vor jeder
Änderung `AGENTS.md` vollständig lesen.

## 3. Wichtige Klarstellung: Das Projekt ist nicht fertig

Der zuvor abgeschlossene Goal-Datensatz belegt nur eine begrenzte
Harness-Härtungsscheibe. Er ist keine Gesamtproduktabnahme. Der lokale Kern ist
umfangreich getestet, aber folgende Abschlussbelege fehlen:

1. aktuelle Live-Rezertifizierung der heutigen Provider-/WebView-Oberflächen;
2. integrierter realer Mehr-Brain-Lauf mit Pool, Heartbeat, Continuation,
   Cross-Brain-Handoff, Profil-Lease/Write-back und geordnetem Shutdown;
3. reproduzierbarer Benchmark-/Repair-/Harvest-Systemlauf auf sauberem Git-
   Worktree;
4. bewusste Upstream-Integration und Releaseentscheidung.

Reale Provider-, Login-, Account-, Browser-, Kosten- oder Credentialpfade nur
nach ausdrücklicher Nutzerfreigabe ausführen.

## 4. Von Codex konsolidierte Commits

| Commit | Inhalt |
|---|---|
| `3ef693b` | UI-only Flächen in `transcript.rs` und `browser_pool.rs` sauber nach `tui`/`webview`/`test` begrenzt; strikter Headless-Produktionsbuild warnungsfrei |
| `7568e49` | Manus-Ende, Übernahmezweig und damalige Gates dokumentiert |
| `e8658b8` | drei aktuelle Vollfeature-Clippy-Befunde behoben: Tail-Expression, explizit nicht-trunkierende Lockdatei und Testmodul hinter Produktionsitems |
| `27e3540` | Goal-Abschluss klar von Projektabschluss getrennt; aktuelle Abschlussmatrix und Roadmap in `docs/OVERVIEW.md` |
| `2cf952e` | Claudes Commit `3437bfd` integriert: zwei mutationsverifizierte Write-back-Durability-Tests für Post-Verify-Rollback und fehlgeschlagenen Restore |

`2cf952e` wurde zusätzlich unabhängig read-only geprüft. Urteil: sinnvoll,
konfliktfrei und nicht redundant. Nicht blockierender Rest: Die Assertion
`error.contains("injected")` trennt Original- und Restore-Fehlertext nicht
einzeln, weil beide Testhooks denselben Text verwenden. Der spezifische
Restore-Fehlerzweig wird durch die zusätzliche Meldungsassertion dennoch klar
belegt.

## 5. Exakte lokale Evidenz am Übergabestand

| Gate | Ergebnis |
|---|---|
| fokussierte Write-back-Durability-Tests | 9 bestanden, 0 fehlgeschlagen |
| `cargo test --no-default-features` | 1.102 Bibliotheks- und 7 Binärtests bestanden, 0 fehlgeschlagen |
| `cargo test` | 1.168 Bibliotheks- und 7 Binärtests bestanden, 0 fehlgeschlagen |
| `cargo clippy --no-default-features -- -D warnings` | Exit 0 |
| `cargo clippy --all-targets -- -D warnings` | Exit 0 |
| `cargo fmt --all -- --check` | Exit 0 |
| `git diff --check` | Exit 0 |

Die Headless-Testkompilierung druckt weiterhin 20 test-only Dead-Code-Warnungen
aus TUI/Wall/Testflächen. Das strikte Headless-Clippy-Gate ist dennoch sauber;
es existiert keine entsprechende Produktionswarnung.

## 6. Relevante Worktrees und Quarantäne

### Maßgeblich

`webagent-codex-takeover` ist der einzige neue Integrationsstand. Hier
weiterarbeiten und vor jeder Scheibe `git status --short --branch` prüfen.

### Nicht erneut integrieren

- `webagent-claude-writeback-tests`: Branch `claude/writeback-restore-tests`,
  Commit `3437bfd`. Bereits als `2cf952e` integriert.
- `webagent-codex-headless-warnings`: alter Vorläufer auf Basis vor den
  Manus-Commits. Sein relevanter Inhalt ist bereits `3ef693b`.

### Bewusst unangetastete Forensik

`C:\Users\storax\Documents\Codex\2026-08-12\kann\work\webagent-final-verification`
steht detached bei `9bf57f5` und enthält einen uncommittierten
`src/repl/pool.rs`-Diff mit 31 Ergänzungen und 5 Löschungen. Er extrahiert die
bereits vorhandene Circuit-Breaker-Prüfung in einen Testhelper. Der Testname
behauptet stärker, Backend-Konstruktion zu belegen, als seine Assertions
tatsächlich nachweisen. Nicht resetten, löschen oder blind übernehmen; der
Worktree bleibt nur als forensische Sicherung.

Weitere historische/detached Worktrees sind keine Kandidaten, solange kein
neuer expliziter Commit- und Evidenzvergleich sie qualifiziert.

## 7. Empfohlene nächste Arbeitsscheibe

Die kleinste providerfreie Scheibe, die den echten Projektabschluss voranbringt,
ist eine Benchmark-/Harvest-Systemabnahme.

### Befund

`run_benchmark` kann die Planungsphase injizieren, ruft in Phase B aber den fest
verdrahteten `bench_run`-/WebView-Pfad auf. `WorkPackage` und
`evaluate_work_package` sind vorhanden und unit-getestet, aber nicht vollständig
in diese Pipeline integriert. Deshalb beweisen die vielen Einzeltests noch
keinen kompletten Auftrag–Repair–Handoff–Harvest-Ablauf.

### Enger Scope

1. Phase-B-Ausführung hinter einen crate-privaten injizierbaren Runner legen.
   Eingänge: Fresh, Continuation mit identischer Run-ID und Cross-Brain-Handoff-
   Envelope. Produktionsadapter bleibt semantisch unverändert.
2. Einen internen Einstieg für ein geparstes, mit
   `evaluate_work_package == Ready` geprüftes Work-Package extrahieren.
3. Git-Capture, Reset, Apply, Eval und Harvest real auf einem temporären kleinen
   Git-Repository ausführen; nur Brain-Ausführung/Eval kontrolliert injizieren.
4. Einen End-to-End-Test bauen:
   - Brain A erzeugt zuerst einen roten Patch;
   - Same-Brain-Continuation erhält dieselbe Run-ID und echte Gate-Ausgabe;
   - nach Stall erfolgt ein begrenztes Cross-Brain-Handoff ohne Conversation-
     Übernahme;
   - Brain B erzeugt einen technisch grünen Patch außerhalb `allowed_paths`;
   - Harvest verwirft ihn fail-closed mit Scope-Grund;
   - `report.harvested` bleibt leer, HEAD unverändert und Worktree sauber.

Ein korrekt verworfener Kandidat ist ein gültiger Systemnachweis. BrowserPool,
Profile, Liveprovider und Release gehören nicht in diese Scheibe.

Danach als eigene zweite Scheibe: lokale integrierte Pool-/Lease-/Write-back-
Abnahme. Erst danach Live-Rezertifizierung mit Nutzerfreigabe.

## 8. Weitere offene Produktgrenzen

- Providerstatus in `docs/PROVIDER_STATUS.md` ist historisch. Der öffentliche
  8/8-Relay-Nachweis stammt vom 2026-07-16 und ist keine heutige Garantie.
- Multi-Brain-, Handoff- und TUI-Komponenten sind lokal getestet, ihr aktueller
  gemeinsamer Livebetrieb bleibt experimentell.
- Free-Cloud besitzt Registry, Policy, Mock-Stream, Metadaten- und Breaker-
  Verträge, aber keinen realen HTTP-/Provideradapter. Dieser Pfad ist optional
  und freigabepflichtig.
- API-Bridge ist als lokaler token-geschützter Loopback-Dienst implementiert;
  dokumentierte Streaming-, Tool-Call- und Multimodalgrenzen bleiben bestehen.
- Push, Merge, Tag und Release sind menschliche Entscheidungen. Nicht aus dem
  Vorhandensein eines Workflows ableiten.

## 9. Verbindliche Arbeitsregeln für Claude

1. Nur ein schreibender Integrator: ab jetzt Claude.
2. Kleine, getestete Scheiben; eine Architekturmechanik statt paralleler
   Ersatzpfade.
3. Vor Build erst bestehende Mechanismen und Aufrufer vollständig lesen.
4. Capability-Evidenz ausschließlich über `capability_proof`; keinen zweiten
   Store einführen.
5. Fremde oder historische Worktree-Diffs nie durch Reset, Checkout oder
   pauschales Kopieren „bereinigen“.
6. Kein Push/Release und kein externer Provider-/Browserzugriff ohne explizite
   Nutzerfreigabe.
7. Nach jeder Scheibe Status und Evidenz sichtbar aktualisieren; „Goal complete“
   niemals wieder mit „Projekt fertig“ gleichsetzen.

## 10. Wiederaufnahmebefehle

```powershell
Set-Location 'C:\Users\storax\Documents\Codex\2026-08-21\man\work\webagent-codex-takeover'
git status --short --branch
git log --oneline --decorate origin/master..HEAD
git diff --check
cargo fmt --all -- --check
cargo clippy --no-default-features -- -D warnings
cargo clippy --all-targets -- -D warnings
cargo test --no-default-features
cargo test
```

Optional kann der bereits gefüllte Buildcache unter
`C:\Users\storax\Documents\Codex\2026-08-21\man\work\webagent-codex-headless-warnings\target`
als `CARGO_TARGET_DIR` verwendet werden. Das ändert keine Projektdateien.

## 11. Prozess- und Eigentumsstatus

- Manus-Prozess: nicht mehr vorhanden.
- Claude-App/-Prozesse: vorhanden.
- Codex beginnt nach Übergabe keine neue Implementierungsscheibe.
- Aktiver schreibender Integrator nach Übergabe: Claude.

Vor dem ersten neuen Commit muss Claude den exakten aktuellen Branch-Tip und
einen sauberen Worktree selbst erneut bestätigen.
