**Referenz.** Lokaler Red-/Green-Beleg zu T-957, kein Betrieb und kein Live-Nachweis. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`.

# T-957 — DSML-Werkzeugformat fail-closed

Datum: 2026-11-21  
Branch: `fix/T-957-dsml-fail-closed`  
Scope: `src/browser_inference.rs`, `docs/API_BRIDGE.md`  
Belegart: lokale Red-/Green-Tests und statische Grenzstelle. **Kein Live-Anspruch**, kein Tag/Release.

## Reales Beispiel

Aus `docs/TASKBOARD.json`, T-957 (deepseek über Pi, 14.09.2026 ab 14:51, viermal beobachtet):

```text
<´¢£´¢£DSML´¢£´¢£ calls><´¢£´¢£DSML´¢£´¢£ invoke name="write"><´¢£´¢£DSML´¢£´¢£ parameter name="path" string="true">...</´¢£´¢£DSML´¢£´¢£ parameter></´¢£´¢£DSML´¢£´¢£ invoke></´¢£´¢£DSML´¢£´¢£ calls>
```

Die Zeichenfolge `´¢£´¢£` ist die im Taskboard gespeicherte, durch eine
Zeichencodierung verformte Form des DSML-Markers `<｜｜DSML｜｜>`. Der Parser
erkennt beide Formen.

## Entscheidung

**Keine Übersetzung**, sondern benannter Protokollfehler
`DSML_TOOL_FORMAT_UNSUPPORTED`. Die DSML-Argumente kämen aus derselben
verformenden Lesestrecke, die T-956 nur für den JSON-Umschlag (Codeblock,
`pre`-Element) abgesichert hat; eine Übersetzung würde Tool-Calls mit
möglicherweise verstümmelten Parametern erfinden. Der Fehler läuft über die
bestehende Inference-Fehlerklasse (HTTP 502) und wird nie als normale
Textantwort an den Client gegeben. Ein gültiger
`WEBAGENT_INFERENCE/1`-Umschlag bleibt unverändert; die Prüfung greift nur,
wenn `envelope_payload` keinen Umschlag findet.

## Vorher (Red)

`cargo test --lib browser_inference::tests::real_dsml_example_never_becomes_a_text_answer -- --exact`

```text
called `Result::unwrap_err()` on an `Ok` value:
BrowserInferenceResponse { text: Some("<´¢£´¢£DSML´¢£´¢£ calls>..."), tool_calls: [] }
test ... real_dsml_example_never_becomes_a_text_answer ... FAILED
```

Das ist genau der gemeldete Fehler: die DSML-Antwort wurde als Textantwort
durchgereicht, `tool_calls` blieb leer.

## Nachher (Green)

Neue Tests in `src/browser_inference.rs`:

- `real_dsml_example_never_becomes_a_text_answer` — das obige reale Beispiel
  liefert `Err` mit `DSML_TOOL_FORMAT_UNSUPPORTED` und nennt den erwarteten
  Umschlag; kein Text, keine Tool-Calls.
- `dsml_with_fullwidth_marker_and_chatter_fails_closed` — Form mit
  Vollbreiten-Marker und vorangestelltem Text, geprüft für `Auto` und
  `Required`.

Beide Tests zuerst gegen den ungeänderten Parser rot, dann gegen den Fix grün.
Bestehende Umschlagtests (`strict_tool_envelope_is_normalized`,
`chatter_before_envelope_is_tolerated`, `fenced_envelope_with_leading_chatter_is_tolerated`,
`inline_marker_mention_is_not_a_tool_call`, Reparatur- und Fail-Closed-Tests)
bleiben grün.

Gates (2026-11-21, Windows/GNU, Rust 1.100):

```text
cargo fmt --all -- --check                     ok
cargo clippy --all-targets -- -D warnings      ok
cargo test --lib                               1468 passed; 0 failed; 1 ignored
git diff --check                               ok
```

Hinweis zur lokalen Umgebung: MinGW (`windres.exe`, `ar.exe`) liegt in einem
WinGet-Paket, nicht im Standard-PATH; für die Cargo-Läufe wurde dessen `bin`
vorübergehend an `PATH` gehängt. Keine Repo-Datei wurde dafür geändert.

`docs/API_BRIDGE.md` beschreibt die Entscheidung und den Fehlernamen im
Abschnitt „OpenAI-Tool-Calling“.
