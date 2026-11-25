# GOALS — Nordstern

## G-001 ✅ ABGESCHLOSSEN (12.09.2026, v0.11.3)
> Fertig: Webagent-Rust zu funktionierendem Bazaar-Projekt. Beleg: `docs/proofs/G-001/g001-abschluss-2026-09-12.json`.

## G-002 ✅ ABGESCHLOSSEN (25.11.2026, eng begrenzt)
> Ziel: T-961 live evidence — chatgpt ruft nach einem erfolgreichen Werkzeugaufruf in derselben Sitzung weiter Werkzeuge auf, belegt durch rohe Pi-Transkripte.
>
> Der Eigentümer hat am 2026-11-25 die autonome Fortführung bis zur Endabnahme freigegeben. Unter `docs/proofs/T-961/semantic-denial-retry/` liegen zwei kausale Live-Läufe: Die semantische Werkzeugverweigerung wurde ausgelöst, der einmalige Frisch-Chat-Retry griff, und beide Läufe endeten mit 5/5 echten Werkzeugturns und strict PASS. 14 strict-clean Läufe insgesamt. T-961 ist `done`. Die ursprünglich ebenfalls geforderte Kontozustands-Notiz wurde **nicht** nachgeliefert und keine Kontoeinstellung geändert; die Ursache des Modellverhaltens bleibt nicht isoliert. Andere Fehlerklassen (vorzeitiger Textabschluss, UI-Sendefehler) sind nicht adressiert. Merge, Tag, Release und Deployment sind **nicht** Teil der Abnahme.

## Offene Gates

- ~~**T-961:** echter Live-Vergleich mit rohen Pi-Transkripten und Kontozustandsprüfung fehlt.~~ Erledigt als enge, kausal belegte Abnahme des Retry-Pfads; die Kontozustandsprüfung bleibt bewusst ausgelassen, keine Anmeldung durch Agenten.
- **PR #69** (`feature/T-934-pi-bridge-roundtrip`): der PR ist inzwischen `CLOSED` ohne Merge; die dort offenen Live-Punkte (Pi-Rerun, generischer curl/SDK Chat+Tools) sind damit **nicht** erledigt und bleiben unbewiesen.
- **Taskboard:** 93 Einträge / 93 `done` / 0 `free` / 0 `claimed` nach der Endabnahme von T-961; `master` bleibt bei `7481b02`, T-967 und T-968 sind über PR #102 bzw. #103 gemergt. Der bestehende Tag `v0.11.4` und das GitHub-Release bleiben unangetastet.

Status: **Operativ abgeschlossen.** Alle Code-, Test- und CI-Gates sind grün; T-967 ist über PR #102 (`c0890d5`) und T-968 über PR #103 (`a2a144a`) auf `master` gemergt. G-002/T-961 wurde am 2026-11-25 auf dem Branch `fix/T-961-semantic-denial-retry` mit zwei kausalen Retry-Läufen abgenommen; eine erneute Anmeldung wurde nicht verlangt und nicht durch Agenten bedient. Die Abnahme gilt nur für die erkannte semantische Verweigerungsklasse, nicht für die Ursache des Modellverhaltens oder andere Fehlerklassen. Der Branch ist nicht gemergt; ein neuer Tag oder ein GitHub-Release ohne gesonderte Eigentümerfreigabe wird nicht erstellt.
