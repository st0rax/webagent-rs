# Zusammenarbeit über GitHub

GitHub ist das dauerhafte Kommunikations- und Übergabesystem dieses Projekts.
Chat und Bot2Bot dürfen auf neue Aktivität hinweisen, sind aber weder Auftrag
noch Projektgedächtnis.

## Zuständigkeiten der Kanäle

| Kanal | Verbindlicher Inhalt |
|---|---|
| GitHub Issue | Auftrag, Scope, Abnahmekriterien, Freigaben und laufender Status |
| Branch/Worktree | isolierte Umsetzung eines Issues, laufend nach GitHub gepusht |
| Pull Request | Diff, Review, Gateresultate, Risiken und Mergeentscheidung |
| `docs/CURRENT_WORK.md` | kurzer lokaler Wiederanlauf- und Notfallcheckpoint |
| Commit | unveränderlicher technischer Checkpoint |
| Chat/Bot2Bot | Benachrichtigung oder schnelle Abstimmung mit Link zum Issue/PR |

Das Repository ist öffentlich. In keinen GitHub-Kanal gehören Credentials,
Cookies, Browserprofile, private Transkripte, unbereinigte Laufzeitlogs,
personenbezogene Daten oder Inhalte aus `.env`.

## Ablauf eines Auftrags

1. Auftrag mit dem Issue-Formular **Entwicklungsauftrag** anlegen. Ergebnis,
   Scope, Nicht-Ziele, Abnahmekriterien und externe Freigaben müssen vor dem
   Start klar sein.
2. Einen Entwickler zuweisen. Dieser kommentiert Branch/Worktree, geplante
   Dateien und ersten Abnahmetest. Ein Issue hat genau einen schreibenden
   Integrator; parallele Helfer arbeiten in getrennten Worktrees und liefern
   eigene Commits.
3. Branch möglichst als `task/<issue>-<kurzname>` oder
   `fix/<issue>-<kurzname>` anlegen. Fremde Dirty Files bleiben unangetastet.
4. Substanziellen Fortschritt im Issue knapp dokumentieren: Commit, neue
   Evidenz, Blocker oder geänderte Entscheidung. Terminal-Rohprotokolle gehören
   nicht vollständig hinein.
5. Pull Request mit `Closes #<issue>` öffnen. Das PR-Template verlangt Scope,
   tatsächlich ausgeführte Gates, Risiken und Übergabestatus.
6. Erst nach Review und grünen erforderlichen Checks mergen; den Merge führt
   der Integrator selbst aus. Tag, Release und Deployment bleiben getrennte
   Entscheidungen des Eigentümers.

Arbeitet nur ein einziger Integrator am Projekt, gibt es kein zweites
Augenpaar. Dann ersetzt die vollständige, im PR belegte Gate-Matrix das Review
— nicht die Behauptung, es habe eines stattgefunden. Der PR bleibt auch dann
Pflicht: Er ist die nachlesbare Spur, was aus welchem Grund nach `master`
gelangt ist.

Der Arbeitsbranch wird dagegen laufend gepusht, spätestens am Ende jeder
Scheibe und vor jeder Unterbrechung. Der Eigentümer muss den aktuellen Stand
jederzeit über GitHub ziehen können, ohne auf eine Freigabe oder eine laufende
Sitzung angewiesen zu sein. Ein nur lokal liegender Stand ist bei Ausfall des
Entwicklers verloren — genau das ist am 2026-08-22 beinahe passiert.

## Unterbrechung oder Entwicklerwechsel

Vor der Übergabe im Issue kommentieren:

```text
Branch und Commit:
Ergebnis bisher:
Eigene/dirty Dateien:
Ausgeführte Gates und Ergebnis:
Offene Abnahmekriterien:
Blocker:
Genau eine sicherste nächste Aktion:
Noch erforderliche externe Freigaben:
```

Zusätzlich `docs/CURRENT_WORK.md` aktualisieren und einen lokalen Checkpoint-
Commit erstellen, sofern die Scheibe kohärent ist. Der neue Entwickler bestätigt
im Issue zuerst HEAD, Worktree-Status und übernommene Dateien. Damit ist eine
Übernahme ohne vorherigen Chat möglich.

## Entscheidungen und offene Ideen

Konkrete Arbeit gehört in Issues. Offene Architekturgespräche können später in
GitHub Discussions geführt und bei ausreichendem Scope in ein Issue überführt
werden. Discussions sind derzeit nicht aktiviert und für diesen Arbeitsablauf
nicht erforderlich.

## Offline-Fallback

Wenn GitHub nicht erreichbar ist, ist `docs/CURRENT_WORK.md` der lokale
Checkpoint. Sobald GitHub wieder verfügbar ist, wird der Zustand in das
zugehörige Issue/PR übertragen; es entsteht kein zweites dauerhaftes Log.
