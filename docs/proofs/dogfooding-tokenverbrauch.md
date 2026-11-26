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

## Minimaler Start-Input (bevorzugter Weg für neue Sessions)

Eine neue Session startet ohne Verlauf. Statt Systemprompt und Anleitung vorab zu injizieren, bekommt das Brain nur die Quest und genau einen korrekt formatierten `read_file`-Aufruf; die Anleitung holt es sich selbst.

| Variante | Bytes | cl100k_base | o200k_base |
|---|---:|---:|---:|
| Alter Systemprompt allein | 5691 | 1565 | 1365 |
| Aktueller Systemprompt allein | 2777 | 748 | 666 |
| Quest + `read_file`-Block | 403 | **112** | **108** |
| Quest allein | 287 | 77 | 71 |
| Nur der `read_file`-Block | 114 | 35 | 37 |

Gegenüber dem alten Systemprompt spart der minimale Start 1453 Tokens (`cl100k_base`, −92,8 %) bzw. 1257 Tokens (`o200k_base`, −92,1 %). Gegenüber dem bereits gekürzten Systemprompt sind es 636 bzw. 558 Tokens. Der Block ist das Wire-Format aus `src/browser_inference.rs` (`WEBAGENT_INFERENCE/1` + `tool_calls` mit `read_file`/`path`); die Quest steht in `docs/DOGFOODING.md`.

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
- `docs/DOGFOODING.md` wird keinem Prompt injiziert und verändert daher keinen Tokenverbrauch; es ist nur eine auffindbare Anleitung. Die 88/83 Tokens oben sind der vom Nutzer/Client übergebene Quest-Text samt Block, nicht vom Harness erzeugter Systemprompt-Overhead.
- Der `read_file`-Aufruf setzt voraus, dass die Bridge das Tool im Turn anbietet ([Client-Werkzeuge] in `choice_text`). Ohne angebotenes Tool ist der Block ein Vorschlag, kein belegter Tool-Call.
- Keine Zugangsdaten gelesen oder ausgegeben; `auth.json` unberührt; kein neues Deployment, kein PR, kein Merge.
