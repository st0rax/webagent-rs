> **Referenz.** Gemessene Vorher/Nachher-Werte zur Prompt-Kürzung und Bash-Kompaktion. Kein Live-Turn, kein Deployment.

# Tokenverbrauch vorher/nachher — 2026-11-26

## Methode

- Prompt-Quellen: `git show HEAD:src/prompts.rs` (Vorher-Stand `f7cca14`) und Arbeitsstand `src/prompts.rs`. Aus beiden wurde der formatierte `autonomous_prefix()`-String extrahiert (`{ver}` → `webagent/1`, `{{`/`}}` → `{`/`}`), ohne umgebenden Rust-Quelltext.
- Tokenizer: lokal installiertes `tiktoken` 0.14.0 mit `cl100k_base` und `o200k_base`. Andere Provider (Claude, Gemini, DeepSeek, Qwen, …) zählen anders; die Werte sind Vergleichsgrößen, keine Rechnungsbeträge.
- Bash-Beispiel: `compactOutput()` aus `examples/pi/extensions/compact-bash-lib.mjs` mit den Env-Werten des Testfixtures und Standardgrenzen; 140 Zeilen mit langen Windows-Pfaden, doppelten Zeilen und einem Kappungsfall.
- Skripte (außerhalb des Repos, temporär): `C:\Users\storax\AppData\Local\Temp\opencode\token_compare.py`, `bash_tokens_input.mjs`, `bash_tokens_input.json`.

## Systemprompt (autonomer Pfad)

| Variante | Bytes | Zeichen | Zeilen | cl100k_base | o200k_base |
|---|---:|---:|---:|---:|---:|
| Vorher (`f7cca14`) | 5691 | 5626 | 125 | 1565 | 1365 |
| Nachher (Arbeitsstand) | 2777 | 2722 | 33 | 748 | 666 |
| Differenz | −2914 | −2904 | −92 | **−817 (−52,2 %)** | **−699 (−51,2 %)** |

Der Präfix wird einmal pro autonome Aufgabe bzw. in jedem Turn gesendet, der ihn erneut mitschickt. Die Einsparung skaliert mit der Anzahl solcher Turns; sie betrifft ausschließlich den autonomen Controller-Pfad.

## Bash-Output (Kompaktionslogik)

| Variante | Bytes | Zeilen | cl100k_base | o200k_base |
|---|---:|---:|---:|---:|
| Vorher (Rohtext) | 28839 | 140 | 10159 | 10379 |
| Nachher (kompaktiert) | 8155 | 51 | 3222 | 3219 |
| Differenz | −20684 | −89 | **−6937 (−68,3 %)** | **−7160 (−69,0 %)** |

Vom Harness gemeldete Statistik: `totalLines=140`, `outputLines=51`, `duplicates=19`, `capped=true`. Die kompakte Fassung enthält einen Zeiger auf die vollständig gespeicherte Ausgabe.

## Wichtige Grenzen

- **Pi-/Bridge-Pfad spart 0 Tokens durch die Prompt-Kürzung.** `src/api_bridge/content.rs::conversation_prompt` injiziert kein `<WEBAGENT_SYSTEM_PROMPT>`; in der ausgewerteten Pi-Session `01a2162c` war die System-Nachricht 0 Zeichen. Es wurde nur die Nutzer-Nachricht eingefügt.
- Die Bash-Zahl stammt aus einem synthetischen Fixture, nicht aus einem echten Pi-Turn. Die Erweiterung ist zum Zeitpunkt dieser Messung **nicht** in `%USERPROFILE%\.pi\agent\extensions\` installiert; es gibt noch keinen Live-Beleg für kompaktierten Output.
- `docs/DOGFOODING.md` wird keinem Prompt injiziert und verändert daher keinen Tokenverbrauch; es ist nur eine auffindbare Anleitung.
- Keine Zugangsdaten gelesen oder ausgegeben; `auth.json` unberührt; kein neues Deployment, kein PR, kein Merge.
