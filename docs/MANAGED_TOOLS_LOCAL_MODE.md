# Lokaler Managed-Tools-Modus

## Warum die zehn Zellen rot sind

Die zehn `managed_tools`-Zellen stehen für alle bisherigen Brains und `auto`
auf `failed`, weil die Bridge Client-Function-Tools absichtlich mit HTTP 400
ablehnt. Der Standardpfad erzeugt sauberen Browsertext: weder OpenAI-/
Anthropic-Tool-Schemas noch Tool-Call-Verläufe oder versteckte Systemtexte
werden in externe Web-Chats geschrieben.

Das ist kein Providerfehler. Es ist eine Produktgrenze, die erst T-504 bewusst
und überprüfbar erweitern darf.

## Vorgeschlagener Vertrag

Ein neuer lokaler Modus wird nur über ein explizites API-Feld aktiviert,
beispielsweise `webagent_managed_tools: true`. Ohne dieses Feld bleibt jede
aktive Client-Tooldefinition fail-closed wie heute.

Im Opt-in-Modus führt die lokale Bridge ausschließlich Tools aus der
bestehenden `ToolRegistry` aus (`read`, `bash`, `edit`, `write`). Jeder Aufruf
braucht eine Action-ID, passiert die bestehende Policy und wird exactly-once
registriert. Der Stream enthält die Entscheidungen und Resultate in einem
dokumentierten lokalen Eventformat.

Der Browser erhält weiterhin nur die unveränderte sichtbare Nutzeraufgabe.
Er bekommt weder ein Tool-Schema noch ein verstecktes Steuerprotokoll. Ein
Browsermodell kann daher nicht eigenständig ein lokales Tool auslösen. Der
lokale Orchestrator entscheidet auf Basis der explizit angeforderten,
strukturierten Managed-Operationen.

## Abnahmevertrag für T-504

1. Der alte Standardpfad mit `tools` bleibt 400 und wird als Negativtest
   erhalten.
2. Der neue Modus akzeptiert nur die vier registrierten Toolnamen und lehnt
   unbekannte Felder, Pfade und gefährliche Befehle ab.
3. Identische Action-IDs führen keinen zweiten Seiteneffekt aus.
4. Ein Integrationstest belegt Request, Policy, Exactly-once, Event-Reihenfolge
   und Resultatdarstellung ohne Browser-Protokollinjektion.
5. Erst danach folgt ein separater Live-Beleg für eine echte Providerzelle;
   bis dahin bleiben alle zehn Matrixzellen `failed`.
