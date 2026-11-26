# **Referenz.** Play-Mode v0.3 — Pokiman als Protocol Detective

Status: Konzept und Soll-Zustand,Stand 2026-11-26. Verbindliche
Umsetzungsgrundlage ist [`docs/PLAY_MODE_SPEC.md`](PLAY_MODE_SPEC.md); die
Tasks liegen in [`docs/TASKBOARD.json`](TASKBOARD.json) als `T-978..T-996`
(Schema v2). Der echte heutige Code steht in `src/repl/play.rs`, nicht in
`src/play/` — siehe [`docs/play/RECON.md`](play/RECON.md).

## Leitidee

Verifizierbarkeit ist die Ressource im Spiel. Ein billiger Jetstream-Event
ist ein Hinweis, kein Beweis. Ein autoritativer PDS-, PLC- oder
Handle-Resolution-Proof ist ein Beleg. Ein Tagebucheintrag darf nur dann
„verifiziert" heißen, wenn er auf einen autoritativen, erfolgreichen und
passenden Proof zeigt. Das wird maschinell geprüft, nicht vom Brain
behauptet. XP gibt es weiterhin ausschließlich für echte Observations des
vorhandenen `ObservationHook`; es gibt keinen zweiten Beleg-Speicher.

Der Spielkern bleibt das Textadventure mit den Räumen `prompt#` → `bridge>` →
`breaker/` → `werkbank/` → `arena/`, geladenen Kernen und Geistern. Neu ist,
dass Geister nicht mehr nur einen Zähler hochsetzen, sondern eine
**diagnostizierte Ursache** mit `Outcome` (`Confirmed`/`Refuted`) haben.
`Save.ghosts` wird künftig aus dem ungebundenen `ghost_log` abgeleitet.

## Korrigierte Simulation (`case001`)

Die alte v0.2-Simulation behauptete, der Handle stehe im
`app.bsky.actor.profile`-Record. Das ist falsch. Der Beweis läuft über
`com.atproto.repo.describeRepo` mit den Feldern `handle`, `didDoc` und
`handleIsCorrect`, ergänzt um Handle-Resolution und PLC-Audit-Log.
`jetstream` ist `wss://<host>/subscribe`, nicht der Firehose-XRPC-Endpunkt.

Synthetischer Fall: DID `did:plc:ksn4wq3zbxe7rtgp2vhf5ydj`, Handle
`annie.example.test`, PDS `https://pds.example.test`. `describeRepo` meldet
`handleIsCorrect=false`; die Handle-Resolution liefert 404; ein Jetstream-
`identity`-Event behauptet den alten Handle.

Ablauf:

1. Failsafe 001 nennt den überholten Handle (bewusst `Wrong`) und fordert die
   Auflösung der DID.
2. PLC-Proof `P-0001` → Diary `D-0001` (`HandleUnbound`) wird `Verified`.
3. Jetstream-Proof `P-0002` erzeugt Geist `G-0001` (`Identity`); Diary
   `D-0002` (`HandleBindsDid`) ist zunächst `Speculative`.
4. `describeRepo` (`P-0003`) und fehlgeschlagene Handle-Resolution (`P-0004`)
   binden `G-0001` mit `Refuted`; `D-0002` wird `Refuted`.
5. Endzustand: 2 Diary-Einträge (1 `Verified`, 1 `Refuted`), kein
   ungebundener Geist, `bridge>` frei.

Alle Daten sind synthetisch. Kein echter Account, keine echte DID, kein
Netzwerkzugriff in Tests. Live-Zugriffe wären ausschließlich Aufgabe von
T-994 hinter dem Feature `play-live`.

## Umsetzungsstand und Grenzen

- Der heutige Spielstand unter `%LOCALAPPDATA%\webagent\data\play\save.json`
  ist `version=1`; Save v2 mit `diary`/`ghost_log` ist Aufgabe T-983.
- `Save::write` ist heute **nicht** atomar (`std::fs::write`); tmp+rename ist
  Teil von T-983.
- `ghosts: u32` ist heute ein Zähler, keine Liste; das Ghost-Modell kommt in
  T-984.
- Die Testfilter `play::proof` … `play::live` finden Stand 2026-11-26 **0
  Tests**. Sie sind Ziele der jeweiligen Tasks, keine aktuellen Belege;
  `cargo test --lib play` findet 9 reale Tests.
- Nach `TUTORIAL-ABGESCHLOSSEN` wird gewartet. Weitere Aufgaben kommen vom
  Nutzer, nicht aus dem Repo erfunden.

## Offene Punkte

- Werkbank/Constellation-Korrelation: nur Design (T-995).
- Firehose-Proofs mit CAR/MST-Prüfung: eigene Ausbaustufe.
- Weitere Fälle `case002` … und Fallauswahl im Menü.
- T-993 prüft `case001` als Golden-Test mit Mock-Brain und Fixtures.
