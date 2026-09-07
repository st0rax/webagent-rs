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

1. T-801: Vertragsgrundlage und Fixtures. Reale beobachtete Ablaeufe als bereinigte MockPageDriver-Fixtures sichern. Typisierte Phasen, Fehler und Ereignisse an bestehende Schnittstellen anbinden. Kompatibilitaet fuer bestehende FakeBrain-Implementierungen erhalten. Abnahme: identische Zustandsfolgen fuer alle Adapter, ein Terminalereignis, Abbruch in jeder Phase.
2. T-802: Gemeinsames Senden. Eine Fill/Verify/Submit-Schleife; vollstaendigen Inhalt vergleichen, nur Editor-Leerraum normalisieren, nie Codezeichen oder Unicode pauschal veraendern. Nach konsumiertem Composer oder unklarem Submit nur beobachten; kein blindes Nachfuellen. Retries begrenzen und dokumentieren. Abnahme: Multiline, Unicode, abgeschnittener Prompt, deaktivierter Button, verspaetete Bestaetigung und Doppelversand-Gegenprobe.
3. T-803: Antwortstream bis zur Maske. Controller, Relay, Swarm, REPL, Web-UI und API konsumieren denselben Ereignisstrom. Rohsnapshot im Transkript erhalten; append-Deltas bei echtem Praefixwachstum, Replace-Ereignis bei Revision statt Texte zu verlieren. Reasoning/UI/Antwort und UI-Diagnosen wie Zai/HTML trennen nur bei belegter DOM-Struktur; Diagnose darf nie als Antwort oder erfolgreicher Repair gelten. Gepufferte Ausgabe nicht als Streaming zaehlen. Abnahme: erstes Delta vor Ende, keine Duplikate, Unicode, Revision, leere Antwort, Stop, Reconnect, Diagnosefragment und vollstaendige Endantwort.
4. T-804: Gemeinsame Profil-Lease und Blocker. Prozessuebergreifende Profilbelegung vor Kopieren/Start pruefen. Busy ist kein Providerlimit. Aktive Profile nicht blind kopieren; kontrolliert warten oder konsistent vorbereitete Isolation verwenden. Bestehende Circuit-Breaker-/Limit-Stores vereinheitlichen statt neue Stores anlegen. Resetzeit mit Herkunft speichern; unbekannter Reset bleibt unbekannt. Keine Probes waehrend belegter Sperre. Abnahme: zwei Prozesse, os error 32, Prozessabsturz, Wiederaufnahme, bekannter/unbekannter Reset, Navigationstimeout.
5. T-805: Probe und Capability-Nachweis. brain_probe nutzt denselben Send-/Observe-Vertrag. Kandidat -> Live-Zustandswechsel -> Measurement -> bestehender capability_proof. Composer, Submit, Output, Streaming, Stop, Modell, Effort, Anhang separat pruefen. Cache an Selektor-/Adapterversion und TTL binden, bei relevanter Aenderung invalidieren. Neue Brain-URL durch generische Discovery, unbekannte Mechanik als unverified ausweisen. Keine feste Brain-Liste im Kern.
6. T-806: Verifizierter Taskabschluss. Schwachen Datei-Existenzabschluss ersetzen. Explizite Task-/Owner-Bindung beim Runstart, keine fest codierte chatgpt-codex-Identitaet. Abnahmeanforderungen vor Run einfrieren; controllerseitig erfasste Tests/Capabilities mit Run-ID, Commit, Artefakthashes und Geltungsbereich pruefen. Brain-erzeugtes Manifest ist Antrag, kein eigener Beweis. Alle Pflichtkriterien muessen bestanden sein. Board unter Prozesslock erneut lesen, Claim/Dependencies vergleichen, eindeutige Tempdatei, flush/sync und Windows-sicher ersetzen; Crash-Recovery und idempotenten Replay testen. Ablehnung im Run-Eventlog persistieren. Alte/leere/fremde/manipulierte Belege und parallele Boardaenderungen duerfen nie done ergeben.
7. T-807: Endabnahme und Release. Gemeinsame Konformitaetssuite je beworbenem Brain und Einstiegspunkt; echte frische Providerbelege separat zu Mocktests. Canary bleibt Smoke. T-501-Matrix ohne Umdeuten von failed/unreachable aktualisieren. Windows-Build einmal nach Gates, Hash/Commit/Version im Manifest, sichtbarer End-to-End-Run, ZIP/DLLs pruefen und GitHub-Release erst mit eindeutigem Abnahmestatus bereitstellen.

## Gates und Migration

T-802/803/804 bauen auf T-801; T-805 benoetigt diese drei; T-806 benoetigt T-801/805; T-807 benoetigt alle. Erst gemeinsame Strecke einziehen, dann Provider schrittweise migrieren und alte Schleifen entfernen. Gegenprobe gegen bestehende reale Logs. Pflichtgates: cargo test --lib, cargo check --features tui, cargo check --no-default-features; zusaetzlich gezielte Windows-Prozess-/Crash-/Streamingtests. Eine gruene Testsuite ersetzt keine Live-Matrix.

## Ausfuehrung mit geringer Denktiefe

Je Scheibe: aktuellen Branch/Claim/Dateiscope pruefen, genau diese Scheibe implementieren, spezifizierte Gegenproben laufen lassen, kleinen neuen Commit pushen, Taskboard und CURRENT_WORK aktualisieren. Keine Force-Pushes, keine wiederholten Builds nach blossen Statuschecks. Bei API-Vertragsaenderungen oder widerspruechlichen Live-Befunden Architekturentscheidung explizit dokumentieren. T-501 bleibt bis Gesamt-Abnahme claimed; neue Aufgaben stehen zunaechst free, Abhaengigkeiten bestimmen ihre Ausfuehrbarkeit.
