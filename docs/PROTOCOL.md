# Protokoll `webagent/1`

> **Referenz:** Der Parser unter `src/protocol/` ist maßgeblich. Dieses Dokument
> beschreibt den Stand vom 2026-11-25 und ist aus
> `src/protocol/{types,parser,mod,text}.rs` abgeleitet. Bei Widersprüchen gilt der
> getestete Parser. `docs/PROTOCOL_SCHEMA.md` ist die ältere, ausführlichere
> Referenz mit denselben Regeln.

## Zweck und Grenze

Das Protokoll ist die einzige erlaubte Sprache, in der ein Brain lokale Aktionen
anfordern darf. Der Parser **führt nichts aus**; er liefert ein validiertes
`ParseResult` mit `valid`, `actions` und einem maschinenlesbaren Fehlercode.
Erst `AgentController` (`src/controller.rs`) führt Aktionen nacheinander aus und
schickt Beobachtungen mit der `action_id` zurück. Reine Prosa ohne gültigen
Umschlag wird nie ausgeführt.

## Envelope

```json
{
  "protocol": "webagent/1",
  "actions": [
    { "id": "a1", "type": "shell", "command": "Get-ChildItem", "timeout_seconds": 30 },
    { "id": "a2", "type": "finish" }
  ]
}
```

| Feld | Regel |
|---|---|
| `protocol` | muss exakt `"webagent/1"` sein; `webagent/2` wird abgelehnt |
| `actions` | nicht-leer; `id`s innerhalb der Antwort eindeutig |

Zusätzliche Envelope-Regeln:

- Die Wurzel muss ein JSON-Objekt sein.
- `finish` muss allein stehen; Ausnahme: eine lückenlose
  `message_part`-Folge (`final-part-001`, `final-part-002`, …) wird im selben
  Umschlag von `finish` abgeschlossen.
- `message` muss allein stehen.
- Unbekannte Felder werden abgelehnt (`#[serde(deny_unknown_fields)]` in
  `src/protocol/types.rs` und die strikte Prüfung in `parser.rs`). Ein
  Tippfehler wie `comand` wird nicht still als leerer Befehl durchgelassen.
- Unvollständiges oder abgeschnittenes JSON ergibt `valid = false`; es gibt
  keine Ausführung eines Teils.

## Aktionen

| `type` | Erlaubte Felder | Bedeutung / Grenze |
|---|---|---|
| `shell` | `id`, `type`, `command`, `timeout_seconds` | `command` nicht-leer; `timeout_seconds` optional, Default 30, `0 < x <= 3600`, kein Bool/String. Vor der Ausführung läuft `shell_policy`. |
| `message` | `id`, `type`, `text` | nutzerlesbare Antwort, allein in der Antwort. |
| `message_part` | `id`, `type`, `text` | Teil eines langen Ergebnisstroms, führt keine Aktion aus; IDs lückenlos ab `final-part-001`, Abschluss mit `finish`. |
| `finish` | `id`, `type` | beendet den Run; keine weiteren Felder. |
| `edit` | `id`, `type`, `path`, `old_string`, `new_string` | `old_string` muss exakt und eindeutig stehen; `new_string != old_string`, `""` löscht den Anker. |
| `write` | `id`, `type`, `path`, `content` | legt eine neue Datei an; vorhandene Zieldatei → Fehler. `content` muss vorhanden sein, darf leer sein. |
| `edit_batch` | `id`, `type`, `edits` | nicht-leere Liste aus `path`/`old_string`/`new_string`; alle Anker werden zuerst validiert, bei einem Fehlschlag wird nichts geschrieben. |

Dateiaktionen laufen gegen den Workspace-Root (`src/file_actions.rs`):
Pfad-Traversal und Symlink-Escape werden abgewiesen, der Root wird kanonisiert.
`src/controller.rs` kann einen expliziten Workspace binden (Benchmark- und
isolierte Läufe).

## Alternative Rohformate

Für Inhalte mit vielen Quotes oder Zeilenumbrüchen akzeptiert der Parser genau
ein Top-Level-Rohformat, keine Treffer in Begleitprosa:

```text
WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
<Script ohne JSON-Escapes>
---END SCRIPT---
```

Weitere Header: `WEBAGENT/1 EDIT`, `WEBAGENT/1 EDIT_BATCH`, `WEBAGENT/1 WRITE`,
`WEBAGENT/1 MESSAGE`. Die exakten Delimiter stehen in `src/protocol/parser.rs`
und werden durch dessen Tests festgelegt. Die Rohformate werden intern in je
genau eine Action übersetzt.

## Fehler, Reparatur, Budgets

- Fehlercodes werden aus der Meldung abgeleitet (`src/protocol/types.rs`),
  u. a. `unknown_field`, `duplicate_id`, `invalid_protocol`, `prose`.
- `src/protocol/text.rs` erlaubt **zwei** Reparaturversuche:
  `PROTOCOL_REPAIR_MAX_FAILURES = 3`; Versuch 1 und 2 erhalten einen
  Reparatur-Prompt, ab dem dritten aufeinanderfolgenden Parse-Fehler bricht der
  Lauf als `protocol_error` ab.
- Zeitlimits liegen an der Action (`timeout_seconds`) und im Controller
  (`src/timeouts.rs`, `src/loop_guard.rs`); der Controller beobachtet Ergebnisse
  und begrenzt Wiederholungen.
- Beobachtungen tragen die `action_id` zurück. Doppelte Action-IDs werden vor
  der Ausführung abgewiesen, damit eine Aktion nicht zweimal läuft.

## Nachweis

Parser-, Controller-, Policy-, Datei- und Store-Tests liegen als Unit-Tests im
Quellcode (`src/protocol/mod.rs`, `src/protocol/parser.rs`, `src/controller.rs`,
`src/shell_policy.rs`, `src/file_actions.rs`, `src/run_store.rs`). Die CI ruft
keine Live-Provider auf (`ci.yml`); Live-Abnahmen liegen getrennt mit Datum und
Binary-Hash unter `docs/proofs/` und `docs/PRODUCTION_READINESS.md`.

## Bekannte Grenzen

- Der Prompt-Entwurf nannte das Shell-Feld `timeout_secs`; die Implementierung
  nutzt durchgehend `timeout_seconds`. Das ist eine Namensabweichung, keine
  andere Semantik.
- `finish` ohne vorherige Aktion ist gültig; es erzwingt keine inhaltliche
  Aufgabenerfüllung.
- Das Protokoll beschreibt keine Zugangsdaten, keine Anmeldung und keine
  Provider-API-Keys. Diese Pfade liegen außerhalb des Parsers.
