# GOALS — Nordstern

## G-001 ✅ ABGESCHLOSSEN (12.09.2026, v0.11.3)
> Fertig: Webagent-Rust zu funktionierendem Bazaar-Projekt. Beleg: `docs/proofs/G-001/g001-abschluss-2026-09-12.json`.

## G-002 ⚠️ OFFEN (Stand 25.11.2026)
> Ziel: T-961 live evidence — chatgpt ruft nach einem erfolgreichen Werkzeugaufruf in derselben Sitzung weiter Werkzeuge auf, belegt durch rohe Pi-Transkripte vor und nach dem Fix und eine geprüfte Kontozustands-Notiz.
>
> Der Code-Fix ist in `master` enthalten (PR #78, Merge `864e346`), und `v0.11.4` wurde am 28.10.2026 getaggt. Inzwischen liegen unter `docs/proofs/T-961/` echte rohe Pi-JSON-Modus-Transkripte für den Lauf vor und nach dem Fix vor; beide Läufe scheitern mit `session_state=Unbestimmt` und 0/5 Tool-Turns, und der chatgpt-Anmeldestatus ist unbestätigt. Das ist ein **negatives Ergebnis**, keine Abnahme. Deshalb bleibt T-961 im vollständigen Taskboard `free` und G-002 offen.

## Offene Gates

- **T-961:** echter Live-Vergleich mit rohen Pi-Transkripten und Kontozustandsprüfung fehlt. Der Eigentümer muss anwesend sein; Anmeldungen, Einmalcodes und CAPTCHAs werden von keinem Agenten bedient.
- **PR #69** (`feature/T-934-pi-bridge-roundtrip`): der PR ist inzwischen `CLOSED` ohne Merge; die dort offenen Live-Punkte (Pi-Rerun, generischer curl/SDK Chat+Tools) sind damit **nicht** erledigt und bleiben unbewiesen.
- **Taskboard:** nach PR #103 auf `master` 93 Einträge / 92 `done` / 1 `free` (T-961), kein `claimed`. T-967 über PR #102 und T-968 über PR #103 gemergt; der bestehende Tag `v0.11.4` und das GitHub-Release bleiben unangetastet.

Status: **Operativ abgeschlossen mit dokumentierter Ausnahme.** Alle Code-, Test- und CI-Gates sind grün; T-967 ist über PR #102 (`c0890d5`) und T-968 über PR #103 (`a2a144a`) auf `master` gemergt. G-002/T-961 bleibt auf ausdrücklicher Eigentümerentscheidung `free`; eine erneute Anmeldung wird nicht verlangt und nicht durch Agenten bedient. Das ist **kein** PASS für T-961 — grüne lokale Gates ersetzen keine Live-Abnahme. Ein neuer Tag oder ein GitHub-Release ohne vollständige Belege und Eigentümerfreigabe wird nicht erstellt.
