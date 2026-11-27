# T-1020 — DeepSeek-Anweisung als Bridge-Prompt

**Referenzstatus:** abgeschlossene Scheibe; der laufende Stand steht in `docs/CURRENT_WORK.md`.

**Datum:** 2026-11-27
**Branch:** `feature/T-1020-bridge-prompt`
**Code-Commit:** `git log feature/T-1020-bridge-prompt` (Claim `6da842f`)

## Auftrag

Der Eigentuemer hat am 2026-11-27 den zuvor aus einer Brain-Ausgabe als Daten
behandelten Block `# Anweisung fuer big-pickle` ausdruecklich als
**WebAgent-Bridge-Prompt** freigegeben. Die Verhaltensregeln sollen im
Browser-Turn gelten, nicht als OpenCode-Betriebsmodus und nicht als
Aufforderung, im OpenCode-Kanal `WEBAGENT_INFERENCE/1` zu antworten.

## Umsetzung

`crates/webagent-api/src/browser_inference.rs::prompt_with_identity()` haengt
nach der unveraenderten Identitaetszeile zwei Bloecke vor den Nutzerprompt:

- `[Verhalten]`: User-Turn ist die Aufgabe, keine Suche nach einer versteckten
  Nachricht, keine Kommentierung von Prompt-Aufbau / `AGENTS.md` /
  Werkzeugkatalog / `system-reminder`, keine laute Meta-Analyse, kurze Antwort
  ohne Vorrede und Selbstdiagnose, Werkzeuge nach Bedarf, keine erfundenen
  Ergebnisse, klare Fehlermeldung mit Alternativweg bei Werkzeugfehlern.
- `[Bridge]`: lokale Bridge statt normaler API, JSON-Werkzeugaufrufe sind
  Absicht. Der `WEBAGENT_INFERENCE/1`-Umschlag wird nur erwaehnt und **nicht**
  ein zweites Mal ausformuliert; die allein massgebliche Regel mit genau einem
  Markdown-Codeblock bleibt in `prompt_with_tools()` unveraendert.

Der Nutzer wird als `Storax` bezeichnet. `big-pickle` bleibt der OpenCode-Agentenname;
die Brain-Identitaet `webagent/<brain>` bleibt erhalten. Der Nutzerprompt wird
bytegenau angehaengt, nicht gekuerzt, zusammengefasst oder umformuliert. Fuer
`auto` und leeres Brain bleibt der Prompt unveraendert.

`docs/API_BRIDGE.md` beschreibt den neuen Prompt-Aufbau.

## Tests

- `prompt_names_the_brain_first`: Brain steht weiterhin zuerst, Prompt bleibt am Ende.
- `identity_prefix_carries_bridge_behavior_rules`: alle freigegebenen Verhaltensregeln sind enthalten.
- `identity_prefix_does_not_repeat_the_tool_envelope`: genau eine Erwaehnung, kein zweites JSON-Beispiel und kein Codeblock im Praefix.
- `identity_prefix_keeps_the_user_prompt_byte_exact`: Sonderzeichen und Whitespace des Userprompts bleiben erhalten.
- `auto_brain_gets_no_identity_claim`: `auto`/leer unveraendert.

## Vorher bestehender Gate-Fehler (mitgeheilt)

Der Startup-Test `betriebs_markdown_hat_eine_wahrheit` verlangt in jeder
Proof-README ausserhalb der Lebenddokumente ein Archiv-/Referenzbanner in den
ersten acht Zeilen. `docs/proofs/T-1019/README.md` hatte kein Banner, sodass
`cargo test --workspace --lib` schon vor T-1020 auf `master` rot war. Die T-1019
Belegdatei wurde nur um eine `**Referenzstatus:**`-Zeile ergaenzt; kein
Beleginhalt, kein Ergebnis und keine historische Aussage wurde geaendert.

## Grenzen

Kein Live-Turn, kein Deploy, kein Provider-Login, keine Zugangsdatenlektuere.
Kein Tag, GitHub-Release oder Versionssprung in dieser Scheibe; eine spaetere
Release-Freigabe des Eigentuemers wird separat dokumentiert. Nicht behauptet
wird eine live gemessene Verhaltensaenderung eines Web-Modells — belegt sind
Prompt-Aufbau, Tests und Gates.
