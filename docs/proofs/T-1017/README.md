# T-1017 - Browser-Prompt-Budget statt stiller UI-Laengenablehnung

> **Referenz.** Task-Beleg für T-1017. Die Änderung begrenzt das
> Werkzeug-Schema und lehnt überlange Prompts vor dem Browserlauf ab. Sie
> kürzt keinen Nutzertext und beschreibt zunächst keinen Deploy.

- Task: T-1017
- Owner: local/opencode
- Branch: `fix/T-1017-browser-prompt-budget`
- Status: umgesetzt und gates-grün; Gates siehe `gates.txt`
- Basen: `origin/master` `e75c895`

## Befund

Die aktive T-1016-EXE (`D96F4999…EEAB`) lieferte für `webagent/gemini` einen
Live-PASS. Für `chatgpt` mit `tools=12` scheiterten die Turns dagegen
reproduzierbar in der Provider-Oberfläche:

- `brain_score/events.jsonl`, z. B. `2026-11-26T23:29:38+01:00`:
  `prompt_chars=196030`, `turn.pasted_chars=199811`,
  `reason=send_phase=submit: ABSENDEKNOPF_DEAKTIVIERT …`, `sends=0`,
  `latency_ms≈219000`.
- Der externe `opencode-cli.exe serve --service` (PID 4944) sendet laufend
  weitere `tools=12`-Turns. Die per-Brain-Sperre serialisiert sie; kurze
  Kontroll-Turns verhungern bzw. laufen in Client-Timeouts.
- Das ist eine UI-Längenablehnung nahe 200k Zeichen, **kein** Selektorenfehler
  und kein Rückfall von T-1016. Der Breaker öffnete mehrfach für 900 s.

## Änderung

`crates/webagent-core/src/inference_types.rs`:

- `MAX_TOOL_SCHEMA_BYTES`: `64 * 1024` → `32 * 1024`. Die Komprimierung
  (`TOOL_DESCRIPTION_LIMITS = [1024, 512, 256, 128, 0]`) kürzt nur
  Beschreibungen; **alle** Werkzeugnamen und Parameterformen bleiben erhalten.
- Neu: `MAX_BROWSER_PROMPT_CHARS = 180_000`. Der Wert liegt unter der
  beobachteten Ablehngrenze und lässt dem Pasted-Text (~3,8k Zeichen
  Overhead gegenüber `prompt_chars`) Reserven.

`crates/webagent-api/src/browser_inference.rs`:

- Neue `check_browser_prompt_budget(prompt)` direkt nach
  `prompt_with_tools` + `prompt_with_identity` in
  `complete_streaming_with_attachments`. Damit ist der Guard vor dem
  Browserlauf wirksam; `complete` und `api_bridge::inference` nutzen
  denselben Pfad.
- Fehlertext ist benannt: `Prompt zu gross fuer die Browser-Oberflaeche:
  <n> Zeichen (Maximum 180000); Werkzeugbeschreibungen sind bereits
  komprimiert`. Ein solcher Turn scheitert sofort statt nach ~4 Minuten
  und erzeugt keinen `ABSENDEKNOPF_DEAKTIVIERT`-Breakereintrag mehr.

Neue Regressionstests:

- `twelve_verbose_tools_stay_within_the_browser_schema_budget` - 12 Tools
  mit sehr langen Beschreibungen bleiben unter 32 KiB, Namen und
  `path`-Parameter überleben.
- `oversized_browser_prompt_is_rejected_before_the_browser_run` - 180.000
  Zeichen sind erlaubt, 180.001 werden mit benanntem Fehler abgelehnt.

## Erwartung und Grenze

- Erwartung für die externen `tools=12`-Turns: der komprimierte Prompt liegt
  deutlich unter 180k, der Browser sendet ihn und die Antwort wird belegt.
  Falls die Werkzeug-Schemata weniger als die angenommenen ~64k beigetragen
  haben, greift stattdessen der schnelle, benannte Budgetfehler; das ist
  immer noch besser als ein vierminütiger Leerlauf, aber **kein** Live-PASS.
- Kein Prompt-Text wird gekürzt, zusammengefasst oder umformuliert.
- Kein Tag, GitHub-Release, Versionssprung oder neues öffentliches Artefakt.
- Keine Zugangsdaten gelesen, angezeigt oder weitergegeben.
- Der Live-Nachweis steht erst nach Build, Deploy und einem belegten Turn;
  diese Datei wird dazu nach dem Merge ergänzt.
