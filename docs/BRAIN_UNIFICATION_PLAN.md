# Einheitlicher Brain-Vertrag und gemeinsame Bedienmaske

Stand: 2026-09-07. Gepruefte Codebasis: 5e22f86. Dies ist der Umsetzungsplan, kein Abnahmebeleg.

## Ziel

Eine Web-UI/REPL/API bedient jedes registrierte Brain durch denselben Ablauf. Unterschiede sind Adapterdaten und begrenzte Browseroperationen, keine eigenen Retry-, Streaming- oder Abschlussregeln. Bestehendes BrainBackend, PageDriver, SessionEvent, RunStore und capability_proof erweitern; keinen parallelen Framework-/Proof-Store einfuehren.

## Verifizierte Luecken

- browser/backend.rs dispatcht auf send_generic/send_gemini/send_qwen; send_generic enthaelt Kimi-Zweige.
- Composer-Pruefungen unterscheiden sich: voller Text, acht Zeichen Praefix oder nur erfolgreicher Fill-Aufruf. Gemini/Qwen fuellen nach Submit-Versuchen erneut und riskieren Doppelversand.
- controller.rs verwendet wait_response; dessen Browserimplementierung verwirft Streaming-Callbacks. relay.rs verwendet wait_response_streaming. Ein gemeinsamer Poller allein ergibt deshalb keine gemeinsame Live-Ausgabe.
- brain_probe sammelt Kandidaten und verifiziert UI-Zustaende; er entdeckt bisher keinen vollstaendigen Laufzeitvertrag automatisch.
- taskboard.rs prueft is_file, owner, branch; es prueft weder Proof-Inhalt, Frische, Run-Bindung, Tests noch Matrix. Ein alter oder leerer Beleg kann somit einen falschen Abschluss verursachen. Die bisherigen Erfolgsaussagen sind entsprechend eingeschraenkt.
- START_HERE und CURRENT_WORK enthalten veraltete Branch-/Build-Angaben; Einstieg darf keine historischen Arbeitsstaende als aktuell injizieren.
- Zai lieferte im UI-Fall `No response, Please try again later.` zusammen mit `Unexpected token '<'` und HTML-artigem Inhalt. Das ist zunaechst ein UI-Diagnosebeleg, kein Beleg fuer einen bestimmten HTTP-Status. Dieselbe Diagnose muss vor Erfolg, Repair, Retry und Streaming-Abschluss zentral klassifiziert werden; Rohtext und Herkunft bleiben im Transkript erhalten.
- Die Profilverwaltung hat mehrere Eintrittspfade. Prozesslokale Mutexes, gemeinsam verwendete Tempdateien, altersbasierte Bereinigung und spaete Circuit-Breaker-Schreibvorgaenge reichen fuer parallele Runs nicht aus. Lease, OS-Lock, Generation, Reset-Herkunft und atomisches Read-Modify-Write muessen zusammengehoeren.
- Probe- und Proof-Ergebnisse muessen an Run, Entrypoint, Brain/Adapter, Commit und Messung gebunden sein. Ein Exit-Code oder ein Brain-erzeugtes Manifest darf keine fehlenden Kriterien als bestanden ausgeben; `failed`, `unreachable`, `blocked` und `not_run` bleiben getrennt.

## Vertrag

Gemeinsame Zustandsmaschine: AcquireProfile -> Navigate -> Ready -> Fill -> VerifyInput -> Submit -> VerifySubmit -> Observe -> Finished; seitlich Blocked, Failed, Cancelled. Operationen tragen run_id, turn_id, brain_id, Profil-Lease, Sequenz, Phase und monotone Dauer. Genau ein terminales Ereignis je Turn. Run fertig ist nicht gleich Task-DoD erfuellt.

Adapter liefern Selektoren, Editorfaehigkeiten, zulaessige Submit-Gesten und beobachtbare Zustandsmarker. Strategien koennen native InsertText, Rich-Multiline und DOM-Fallback sein. Keine pro-Brain-Schleifen im Controller. Nicht jede Website muss denselben Browsertrick verwenden.

## Umsetzung in pruefbaren Scheiben

1. T-801: Vertragsgrundlage und Fixtures. Reale beobachtete Ablaeufe als bereinigte MockPageDriver-Fixtures sichern. Typisierte Operationen tragen attempt_id, run_id, brain_id, Lease-Generation, Phase, Revision und Herkunft. `SurfaceOutcome` unterscheidet Inhalt, UI-Diagnose, Limit, Captcha, Challenge, Login und transienten Fehler, bevor irgendein Einstiegspunkt streamt, repariert oder Erfolg zaehlt. `OperationEvent` liefert started, heartbeat, retry, timeout, cancelled und terminalen Abschluss. Kompatibilitaet fuer bestehende FakeBrain-Implementierungen erhalten. Abnahme: identische Zustandsfolgen fuer Controller, REPL, Relay, Web-UI, API und Swarm; ein Terminalereignis; Abbruch in jeder Phase; Zai-HTML bleibt Rohbeleg und wird nie Textdelta oder Parser-Repair; blockierter PageDriver erzeugt sichtbar started, heartbeat und timeout.
2. T-802: Gemeinsames Senden. Eine Fill/Verify/Submit-Schleife; vollstaendigen Inhalt vergleichen, nur Editor-Leerraum normalisieren, nie Codezeichen oder Unicode pauschal veraendern. Nach konsumiertem Composer oder unklarem Submit nur beobachten; kein blindes Nachfuellen. Retries begrenzen und dokumentieren. Abnahme: Multiline, Unicode, abgeschnittener Prompt, deaktivierter Button, verspaetete Bestaetigung und Doppelversand-Gegenprobe.
3. T-803: Antwortstream bis zur Maske. Controller, Relay, Swarm, REPL, Web-UI und API konsumieren denselben Ereignisstrom. Rohsnapshot im Transkript erhalten; append-Deltas bei echtem Praefixwachstum, Replace-Ereignis bei Revision statt Texte zu verlieren. Reasoning/UI/Antwort und UI-Diagnosen wie Zai/HTML trennen nur bei belegter DOM-Struktur; Diagnose darf nie als Antwort oder erfolgreicher Repair gelten. Gepufferte Ausgabe nicht als Streaming zaehlen. Swarm gibt vor jedem Brain sichtbar aus, welchen Kontext er wirklich uebergibt: Ziel, Repository, Commit, Branch, Task, Run-ID und Belegpfade; fehlende Werte bleiben fehlend. Abnahme: erstes Delta vor Ende, keine Duplikate, Unicode, Revision, leere Antwort, Stop, Reconnect, Diagnosefragment und vollstaendige Endantwort.
4. T-804: Gemeinsame Profil-Lease und Blocker. Prozessuebergreifende Profilbelegung vor Kopieren/Start pruefen. Busy ist kein Providerlimit. Aktive Profile nicht blind kopieren; kontrolliert warten oder konsistent vorbereitete Isolation verwenden. Lease umfasst pid, Prozessstart, run_id, brain_id, Generation und Heartbeat und bleibt bis nach Browser-Abbau bestehen. Bestehende Circuit-Breaker-/Limit-Stores vereinheitlichen statt neue Stores anlegen. Resetzeit mit Herkunft speichern; unbekannter Reset bleibt unbekannt. Keine Probes waehrend belegter Sperre. Abnahme: zwei Prozesse, os error 32, Prozessabsturz, Wiederaufnahme, alter Worker darf neuen Lease nicht loeschen, bekannter/unbekannter Reset, Navigationstimeout.
5. T-805: Probe und Capability-Nachweis. brain_probe nutzt denselben Send-/Observe-Vertrag. Kandidat -> Live-Zustandswechsel -> Measurement -> bestehender capability_proof. Composer, Submit, Output, Streaming, Stop, Modell, Effort, Anhang separat pruefen. Cache an Selektor-/Adapterversion und TTL binden, bei relevanter Aenderung invalidieren. Neue Brain-URL durch generische Discovery, unbekannte Mechanik als unverified ausweisen. Keine feste Brain-Liste im Kern.
6. T-806: Verifizierter Taskabschluss. Schwachen Datei-Existenzabschluss ersetzen. Explizite Task-/Owner-Bindung beim Runstart, keine fest codierte chatgpt-codex-Identitaet. Abnahmeanforderungen vor Run einfrieren; controllerseitig erfasste Tests/Capabilities mit Run-ID, Commit, Artefakthashes und Geltungsbereich pruefen. Brain-erzeugtes Manifest ist Antrag, kein eigener Beweis. Alle Pflichtkriterien muessen bestanden sein. Board unter Prozesslock erneut lesen, Claim/Dependencies vergleichen, eindeutige Tempdatei, flush/sync und Windows-sicher ersetzen; Crash-Recovery und idempotenten Replay testen. Ablehnung im Run-Eventlog persistieren. Alte/leere/fremde/manipulierte Belege und parallele Boardaenderungen duerfen nie done ergeben.
7. T-807: Endabnahme und Release. Gemeinsame Konformitaetssuite je beworbenem Brain und Einstiegspunkt; echte frische Providerbelege separat zu Mocktests. Canary bleibt Smoke. T-501-Matrix ohne Umdeuten von failed/unreachable aktualisieren. Windows-Build einmal nach Gates, Hash/Commit/Version im Manifest, sichtbarer End-to-End-Run, ZIP/DLLs pruefen und GitHub-Release erst mit eindeutigem Abnahmestatus bereitstellen.
8. T-808: Dauerhaftes Run-Ledger und Crash-Recovery. Run-Meta, Eventkette und Transcript mit eindeutiger Run-/Attempt-/Lease-Bindung versoehnen. Prozessuebergreifend sequenzieren und sperren; keine gemeinsam verwendeten Tempnamen; Windows-sicher atomar ersetzen und flushen. Torn Tails werden erkannt, unveraendert quarantiniert und mit Recovery-Receipt statt stiller Reparatur fortgesetzt. Resume behandelt vor dem Crash moeglicherweise gesendete Aktionen als mehrdeutig und beobachtet zuerst. Abnahme: zwei Prozesse schreiben eine lueckenlose lineare Kette; kaputter Tail laesst den gueltigen Prefix lesbar; Fehler bei Rename/Write erzeugen nie halbes Meta; jede Persistenzunterbrechung ergibt fail-closed `interrupted` oder `recovery_required`, nie unbelegtes `done`.

## Gates und Migration

T-802/803/804 bauen auf T-801; T-805 benoetigt diese drei; T-808 kann parallel zu T-801 umgesetzt werden; T-806 benoetigt T-801/805/808; T-807 benoetigt alle. Erst gemeinsame Strecke einziehen, dann Provider schrittweise migrieren und alte Schleifen entfernen. Gegenprobe gegen bestehende reale Logs. Pflichtgates: cargo test --lib, cargo check --features tui, cargo check --no-default-features; zusaetzlich gezielte Windows-Prozess-/Crash-/Streamingtests. Eine gruene Testsuite ersetzt keine Live-Matrix.

## Ausfuehrung mit geringer Denktiefe

Je Scheibe: aktuellen Branch/Claim/Dateiscope pruefen, genau diese Scheibe implementieren, spezifizierte Gegenproben laufen lassen, kleinen neuen Commit pushen, Taskboard und CURRENT_WORK aktualisieren. Keine Force-Pushes, keine wiederholten Builds nach blossen Statuschecks. Bei API-Vertragsaenderungen oder widerspruechlichen Live-Befunden Architekturentscheidung explizit dokumentieren. T-501 bleibt bis Gesamt-Abnahme claimed; neue Aufgaben stehen zunaechst free, Abhaengigkeiten bestimmen ihre Ausfuehrbarkeit.

## Phase-8-Status (2026-09-12)

| Scheibe | Status | Belege |
|---|---|---|
| T-801 Vertrag + Fixtures | done | `96e83aa`; contract.rs Zustandsmaschine/OperationEvent/Ledger |
| T-802 Einheitliches Senden | done | `181cb38`; Fill+Verify+Submit-Schleife, Doppelversand-Gegenproben |
| T-803 Antwortstream | done | `1cf0cc5`,`133361a`,`41a6053`; Controller/REPL/Swarm/UI/API-Strom, Praefix-Delta vs Replace |
| T-804 Profil-Lease/Blocker | done | `7b328eb`,`9dd18a0`; siehe Abschnitt T-804-Status; Live-Prozessproben -> T-807 |
| T-805 Probe/Capability-Proof | teilweise | Kern vorhanden (brain_probe Verdict->Measurement, selector_hash+TTL-Invalidierung, verify). Store-Urteil: nicht belegte Oberflaechen-Probe = Unreachable, nie Failed. Live-Messungen -> T-807 |
| T-806 Verifizierter Taskabschluss | teilweise | `d5e88eb`; Taskboard unter Prozesslock, unique Temps, sync_all, Owner=Brain. Beleg-Verifikation -> T-807 |
| T-807 Endabnahme/Release | offen | benoetigt Live-Matrix mit echten Brains + Windows-Prozessproben |
| T-808 Run-Ledger/Crash-Recovery | done | `6b02cd0`; Torn-Tail-Quarantaene, Recovery-Receipt, Journal-Lock, fsync |

T-805-Rest (rein Rechnung/ohne Browser pruefbar): `brain_probe::verdict_outcome` ist
die einzige Uebersetzung Verdict->Store-Urteil; nicht belegte Messungen sind
Messluecken (`Unreachable`, entziehen nie einen Beleg), echte `Failed`-Befunde
entstehen nur in den reichen Verify-Pfaden (`generation_sequence` u.a.).
Keine feste Brain-Liste im Kern (API-Router-Batches in `api_bridge` sind
Routing-Tabellen, keine Kernliste; Capability-Katalog ist brain-agnostisch).
Cache an Selektor-/Adapterversion und TTL ist ueber `selector_hash_for` +
`proof_state` gebunden (`SelectorsChanged`/`TtlElapsed`).
Live-Messungen je Brain (Kandidat -> Live-Zustandswechsel -> Measurement ->
capability_proof) folgen in T-807.

T-806-Rest: Datei-Existenz-Abschluss ersetzt durch verifizierte Run-Belege
(Manifest = Antrag, kein Beweis), Run-ID/Commit/Artefakthashes controllerseitig
pruefen, Ablehnung im Run-Eventlog, Crash-/Replay-Tests — groesstenteils mit
echten Runs; die noch offenen Live-Teile laufen in T-807.

T-806-Rest (2026-09-12, Kern rechnerisch umgesetzt): `acceptance.rs` definiert
`CompletionReceipt` (brain-erzeugtes Manifest) und `ExpectedCompletion`
(controllerseitige Bindung an task_id, run_id, brain_id, Commit, Run-Status).
`acceptance::verify_completion` akzeptiert einen Antrag nur, wenn: Version passt,
alle Pflichtfelder nicht leer sind, task/run/brain/commit der Erwartung
entsprechen, der Run `done` ist, mindestens ein Pflichtkriterium reine `Passed`-
Ergebnisse liefert und Artefakthashes vorhanden sind. `Failed`/`Unreachable`/
`NotRun`-Kriterien und leere/differente Belege werden abgelehnt.
`taskboard::complete_claim_verified` schaltet bei Ablehnung nicht auf `done`,
sondern persistiert die Ursache via `RunStore::record_rejection` (Event-Typ
`task_completion_rejected`) in der lueckenlosen Run-Ereigniskette.
CLI: `run --completion-receipt <json>` bindet den Abschluss an
Run-ID/HEAD-Commit/Brain; ohne passendes Receipt kein `done`. Live-Loops
(Controller erzeugt Receipt aus verifizierten Capabilities) folgen in T-807.

## T-804-Status (2026-09-12)

Kernmodul umgesetzt und gepusht (`7b328eb`), Rest per Plan-Zuordnung:

- **Prozessuebergreifende Profilbelegung pruefen**: `acquire_swarm_profile(_in)` in `config/profiles.rs`; Default-Preparer des bot2bot-Workers nutzt es (120s Budget). `SwarmProfileOwner` traegt pid, Prozessstart, run_id, brain_id, Scope, `generation`, Heartbeat und Reset (`reset_origin`/`reset_at_ns`); Version bleibt 1 (additiv, Legacy-Marker ohne Lease-Felder werden nie angefasst).
- **Busy != Providerlimit**: `SwarmProfileLeaseState::Busy` ist vom Fehlerfall `Err(WouldBlock)` getrennt; Leases werden von `Stale` unterschieden, Lock-Dateien sind Advisory-locks, kein State-Store.
- **Verwaist = abgelaufener Heartbeat UND tote PID** (`pid_is_alive` via OpenProcess/GetExitCodeProcess, Windows); Reclaim steht unter fs2-Advisory-Sperre mit Re-Validierung des Markers (`actual == expected`) — ein alter Worker kann einen uebernommenen Lease nie loeschen, der Sweeper respektiert frische Heartbeats.
- **Kontrolliert warten / Isolation**: Acquire loop Free->prepare, Stale->reclaim, Busy->sleep(50ms), Deadline -> `Err(WouldBlock)`; gekapselte Fallback-Instanzen (encapsulated) kloeen weiterhin in eigene Verzeichnisse unter OS-Lock (`prepare_shared_profile_for_clone`).
- **Lease bis nach Browser-Abbau**: Worker released die Lease explizit erst nach dem kompletten `poll_once`-Durchlauf (browser teardown), nicht vorher.
- **Reset mit Herkunft / unbekannt bleibt unbekannt**: `record_reset` am Lease und `record_reset_in(profile_dir, origin)` freistehend; Navigationstimeout im isolierten Startpfad (`browser/backend.rs`) erfasst `navigation_timeout`, ein Profil ohne Marke bleibt unangetastet.
- **Keine Probes bei belegter Sperre / Store-Vereinheitlichung**: marke liegt im bestehenden `.webagent-swarm-owner.json`, Lock-Datei-Muster wie `.session-writeback.lock` (fs2); es entstehen keine neuen State-Stores. Probes/Verify gate zusätzlich ueber `circuit_breaker` (Vorbestand) — ein Proben-Hub waehrend belegter Lease bleibt fuer die Live-Abnahme.

Live-Abnahmepunkte (zwei Prozesse, os error 32, Absturz, Wiederaufnahme, Nav-Timesout) sind bewusst fuer T-807 als Windows-Prozessproben gegen reale Browser vorgesehen; die unit-getesteten Gegenproben decken Busy, Reclaim, Fremd-Release und Reset ab.
