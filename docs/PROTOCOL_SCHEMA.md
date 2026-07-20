# Protokoll-Schema: `webagent/1`

Menschenlesbare Referenz des Aktionsprotokolls, das jedes Brain ausgeben
muss. Sie ist aus `src/protocol.rs` abgeleitet (Funktionen `parse`,
`action_from_value`, `allowed_fields`) und dient Prompt-Autoren und Reviewern
als einzige Quelle. Bei Änderungen am Parser diese Datei mitziehen.

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
| `type` | String | einer von: `shell`, `message`, `finish`, `edit`, `write` |

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
{ "id": "fix-1", "type": "edit", "path": "C:/pfad/app.py",
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
{ "id": "new-1", "type": "write", "path": "C:/tmp/neu.txt", "content": "zeile1\n" }
```

## Alternatives Rohskript-Format (`shell`)

Für komplexe PowerShell-Skripte mit vielen Anführungszeichen gibt es ein
JSON-freies Envelope, das intern in eine einzelne `shell`-Action übersetzt wird:

```
WEBAGENT/1 SHELL
id: report-1
timeout_seconds: 300
---SCRIPT---
<beliebiges Skript, keine JSON-Escapes nötig>
---END SCRIPT---
```

`timeout_seconds` unterliegt derselben Range-Prüfung (`0 < x <= 3600`).
