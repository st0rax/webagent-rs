# Protokoll-Schema: `webagent/1`

> **Referenz mit historischem JSON-Schwerpunkt.** Der ausführbare Parser unter
> `src/protocol/` ist autoritativ. Er unterstützt zusätzlich strikte Rohformate
> für `SHELL`, `EDIT`, `EDIT_BATCH`, `WRITE` und `MESSAGE`; Beispiele in diesem
> Dokument sind keine Aufforderung, Actions in Prosa einzubetten.

Menschenlesbare Referenz des Aktionsprotokolls, das jedes Brain ausgeben
muss. Sie ist aus `src/protocol/{types,parser}.rs` abgeleitet und dient
Prompt-Autoren und Reviewern als Orientierung. Bei Änderungen am Parser diese
Datei mitziehen; bei einem Widerspruch gilt der getestete Parser.

Die Validierung ist **hand-gerollt und strikt** (kein externes Schema-Framework,
keine zusätzliche Dependency). Strikt heißt: unbekannte Felder in einem
Action-Objekt werden abgelehnt — ein Tippfehler wie `comand` statt `command`
fliegt sofort auf, statt als leerer Befehl durchzulaufen.

## Envelope (Wurzel-Objekt)

```json
{
  "protocol": "webagent/1",
  "actions": [ /* eine oder mehr Action-Objekte */ ]
}
```

| Feld       | Typ    | Constraint                          |
|------------|--------|-------------------------------------|
| `protocol` | String | muss exakt `"webagent/1"` sein      |
| `actions`  | Array  | nicht-leer                          |

Zusätzliche Envelope-Regeln:

- Die Wurzel muss ein JSON-Objekt sein.
- Action-`id`s müssen innerhalb der `actions`-Liste eindeutig sein.
- Eine `finish`-Action muss die **einzige** Action der Antwort sein.
- Eine `message`-Action muss die **einzige** Action der Antwort sein.
- `shell`- und `edit`-Actions dürfen in einer Antwort gemischt werden (serielle
  Ausführung).

## Actions

Jede Action ist ein Objekt. Gemeinsame Pflichtfelder für **alle** Typen:

| Feld   | Typ    | Constraint                                        |
|--------|--------|---------------------------------------------------|
| `id`   | String | Pflicht                                           |
| `type` | String | einer von: `shell`, `message`, `finish`, `edit`, `edit_batch`, `write` |

Über die unten je Typ gelisteten Felder hinaus sind **keine** weiteren Felder
erlaubt. Ein unbekanntes Feld (Tippfehler oder falscher Typ) → Antwort ungültig.

### `type: "shell"` — lokalen Befehl ausführen

Erlaubte Felder: `id`, `type`, `command`, `timeout_seconds`.

| Feld              | Typ    | Constraint                                    |
|-------------------|--------|-----------------------------------------------|
| `command`         | String | Pflicht, nicht-leer (nach Trim)               |
| `timeout_seconds` | Zahl   | optional, Default `30`; endlich, `0 < x <= 3600`; kein Bool/String |

```json
{ "id": "step-1", "type": "shell", "command": "Get-Location", "timeout_seconds": 30 }
```

### `type: "message"` — nutzerlesbare Antwort

Erlaubte Felder: `id`, `type`, `text`.

| Feld   | Typ    | Constraint                       |
|--------|--------|----------------------------------|
| `text` | String | Pflicht, nicht-leer (nach Trim)  |

```json
{ "id": "answer-1", "type": "message", "text": "Kurze Zusammenfassung." }
```

### `type: "finish"` — Run beenden

Erlaubte Felder: nur `id`, `type` (keine weiteren).

```json
{ "id": "done-1", "type": "finish" }
```

### `type: "edit"` — Anker-Ersatz in Bestandsdatei

Erlaubte Felder: `id`, `type`, `path`, `old_string`, `new_string`.

| Feld         | Typ    | Constraint                                                   |
|--------------|--------|-------------------------------------------------------------|
| `path`       | String | Pflicht, nicht-leer (nach Trim)                             |
| `old_string` | String | Pflicht, nicht-leer; muss exakt & eindeutig in der Datei stehen |
| `new_string` | String | optional (`""` löscht den Anker); muss `!= old_string` sein |

```json
{ "id": "fix-1", "type": "edit", "path": "src/app.rs",
  "old_string": "return 1", "new_string": "return 2" }
```

### `type: "write"` — neue Datei anlegen

Erlaubte Felder: `id`, `type`, `path`, `content`.

| Feld      | Typ    | Constraint                                              |
|-----------|--------|---------------------------------------------------------|
| `path`    | String | Pflicht, nicht-leer (nach Trim)                        |
| `content` | String | Pflicht (Schlüssel muss vorhanden sein; `""` erlaubt)  |

Schlägt fehl, wenn die Zieldatei bereits existiert — Bestandsdateien immer mit
`edit` ändern.

```json
{ "id": "new-1", "type": "write", "path": "src/neu.rs", "content": "zeile1\n" }
```

### `type: "edit_batch"` — mehrere Anker atomar ersetzen

Erlaubte Felder: `id`, `type`, `edits`. `edits` ist eine nicht-leere Liste aus
`path`, `old_string` und `new_string` mit denselben Ankerregeln wie `edit`.
Zuerst werden alle Pfade und Anker validiert; schlägt ein Eintrag fehl, wird
kein Teil des Batches geschrieben.

```json
{
  "id": "refactor-1",
  "type": "edit_batch",
  "edits": [
    { "path": "src/a.rs", "old_string": "old_a", "new_string": "new_a" },
    { "path": "src/b.rs", "old_string": "old_b", "new_string": "new_b" }
  ]
}
```

## Alternative Rohformate

Für Inhalte mit vielen Anführungszeichen oder Zeilenumbrüchen gibt es strikt
begrenzte JSON-freie Envelopes. Sie werden intern jeweils in genau eine Action
übersetzt. Der Parser akzeptiert nur ein top-level Envelope, nicht Treffer in
erklärender Begleitprosa.

Shell:

```
WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
<beliebiges Skript, keine JSON-Escapes nötig>
---END SCRIPT---
```

`timeout_seconds` unterliegt derselben Range-Prüfung (`0 < x <= 3600`).

Die kanonischen Header der übrigen Rohformate sind:

```text
WEBAGENT/1 EDIT
WEBAGENT/1 EDIT_BATCH
WEBAGENT/1 WRITE
WEBAGENT/1 MESSAGE
```

Die exakten Delimiter und Pflichtfelder stehen in `src/protocol/parser.rs` und
werden durch dessen Parser-Tests festgelegt.
