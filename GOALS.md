# GOALS — Nordstern

## G-001 ✅ ABGESCHLOSSEN (12.09.2026, v0.11.3)
> Fertig: Webagent-Rust zu funktionierendem Bazaar-Projekt. Beleg: `docs/proofs/G-001/g001-abschluss-2026-09-12.json`.

## G-002 ⚠️ OFFEN (Stand 20.11.2026)
> Ziel: T-961 live evidence — chatgpt ruft nach einem erfolgreichen Werkzeugaufruf in derselben Sitzung weiter Werkzeuge auf, belegt durch rohe Pi-Transkripte vor und nach dem Fix und eine geprüfte Kontozustands-Notiz.
>
> Der Code-Fix ist in `master` enthalten (PR #78, Merge `864e346`), und `v0.11.4` wurde am 28.10.2026 getaggt. Die Dateien unter `docs/proofs/T-961/` sind jedoch ein Testplan mit `NOT-RUN`-Metadaten und Platzhaltern statt echter Pi-JSON-Modus-Ereignisse. Deshalb bleibt T-961 im vollständigen Taskboard `free` und G-002 offen.

## Offene Gates

- **T-961:** echter Live-Vergleich mit rohen Pi-Transkripten und Kontozustandsprüfung fehlt. Der Eigentümer muss anwesend sein; Anmeldungen, Einmalcodes und CAPTCHAs werden von keinem Agenten bedient.
- **PR #69** (`feature/T-934-pi-bridge-roundtrip`): Live-Pi-Rerun und generischer curl/SDK Chat+Tools sind dort ausdrücklich `pending`; nicht ohne Belege mergen.
- **Taskboard:** 14 freie Tasks im vollständigen `docs/TASKBOARD.json` (91 Einträge), darunter T-961.

Status: **Projekt nicht abgeschlossen.** Grüne lokale Build-/Test-Gates ersetzen keine Live-Abnahme. Ein neuer Tag oder ein GitHub-Release ohne vollständige Belege und Eigentümerfreigabe wird nicht erstellt.
