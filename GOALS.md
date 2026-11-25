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

Status: **Operativ abgeschlossen.** Alle Code-, Test- und CI-Gates sind grün; T-967 ist über PR #102 (`c0890d5`), T-968 über PR #103 (`a2a144a`) und T-961 über PR #104 (`ea88203`) auf `master` gemergt. G-002/T-961 wurde mit zwei kausalen Retry-Läufen abgenommen; eine erneute Anmeldung wurde nicht verlangt und nicht durch Agenten bedient. Die Abnahme gilt nur für die erkannte semantische Verweigerungsklasse, nicht für die Ursache des Modellverhaltens oder andere Fehlerklassen. Der Release `v0.11.5` ist nach dem Windows-DLL-Pfadfix als [GitHub-Release](https://github.com/st0rax/webagent-rs/releases/tag/v0.11.5) veröffentlicht (Workflow `45684483569` grün, sieben Assets, Sha256 geprüft). Die veröffentlichten Windows-Artefakte sind in einen lokalen Zielordner deployt und geprüft (`0.11.5` / `9b2ad7e6e`); ein Produktiv-Deployment bleibt eine gesonderte Entscheidung.
