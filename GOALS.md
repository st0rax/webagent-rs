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
- **Taskboard:** 127 Einträge / 127 `done` / 0 `free` / 0 `claimed` (Stand 27.11.2026, `docs/TASKBOARD.json` ist Quelle der Wahrheit). `origin/master` steht bei `a916039`; der aktuelle Release ist `v0.11.10` (Tag-Commit `629009b`, Workflow `46067339957`, sieben SHA-256-geprüfte Assets). Der bestehende Tag `v0.11.4` und das GitHub-Release bleiben unangetastet. Die Audit-Fixes aus `fix/audit-isolation-and-denylist` sind vollständig gemergt, getaggt, veröffentlicht und lokal deployt; Beleg: `docs/proofs/release-v0.11.10-2026-11-27/README.md`.

Status: **Operativ abgeschlossen.** Alle Code-, Test- und CI-Gates sind grün; T-967 ist über PR #102 (`c0890d5`), T-968 über PR #103 (`a2a144a`) und T-961 über PR #104 (`ea88203`) auf `master` gemergt. G-002/T-961 wurde mit zwei kausalen Retry-Läufen abgenommen; eine erneute Anmeldung wurde nicht verlangt und nicht durch Agenten bedient. Die Abnahme gilt nur für die erkannte semantische Verweigerungsklasse, nicht für die Ursache des Modellverhaltens oder andere Fehlerklassen. Der aktuelle Release ist [`v0.11.10`](https://github.com/st0rax/webagent-rs/releases/tag/v0.11.10) (Tag-Commit `629009b`, Workflow `46067339957`, sieben geprüfte Assets); `v0.11.5` bis `v0.11.9` sind historische Patch-Releases. Die Windows-Artefakte von `v0.11.10` sind in den lokalen Zielordner deployt (EXE-SHA-256 `7EF813D5…D4CC`, `canary` 8/8 `ok`, Bridges 8788/8789 gesund); Rückweg ist `backup-release-before-v0.11.10-20261127\`. Ein Fern-/Produktiv-Deployment bleibt eine gesonderte Entscheidung und ist ohne schriftliche Risikoakzeptanz No-Go (`docs/PRODUCTION_READINESS.md`).
