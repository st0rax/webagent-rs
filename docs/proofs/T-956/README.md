# T-956: Werkzeugargumente verlustfrei aus dem Codeblock

**Referenz:** Live-Nachweis zu T-956, Stand 2026-09-14. Verbindlich sind START_HERE.md und docs/TASKBOARD.json.

## Messung

Einzelanfrage je Brain an `/v1/chat/completions` mit einem Werkzeug `write`
und der Bitte, diesen `content` zeichengenau zu uebergeben:

```text
A=*x* und *y*
B="$HOME" und $PATH und $X$
C=C:\Users\a\b
D=`code` _unter_ **fett**
E=Zeile5
```

Gezaehlt wurde je Zeichenklasse im Argument, das die Bridge als `tool_calls`
zurueckgab.

| Stand | Brain | `*` (8) | `$` (4) | `\` (3) | Backtick (2) | `_` (2) | Leerzeichen |
|---|---|---|---|---|---|---|---|
| vorher, Build 84dd57f | deepseek | 4 | 0 | 2 | 0 | 0 | normal |
| vorher, Build 84dd57f | qwen | 0 | 0 | 2 | 0 | 0 | normal |
| Codeblock, ohne NBSP-Fix | qwen | 8 | 4 | 3 | 2 | 2 | NBSP |
| Codeblock mit NBSP-Fix | deepseek | 8 | 4 | 3 | 2 | 2 | normal |
| Codeblock mit NBSP-Fix | qwen | 8 | 4 | 3 | 2 | 2 | normal |

Vorher angekommen, deepseek: `B="PATH und X`, `D=code unter fett`, und `\b`
wurde als JSON-Escape zum Steuerzeichen Backspace. qwen: `A=x und y`.

Nachher bei beiden: exakt der gesendete Text plus ein abschliessender
Zeilenumbruch, den beide Modelle bereits in der Vorher-Messung anhaengten.

## Grenzen

- Liefert ein Modell den Umschlag nicht im Codeblock, liest die Bridge ihn
  weiter aus dem gerenderten Text und Zeichen koennen fehlen. Fail-closed
  dafuer ist T-958.
- Geprueft nur deepseek und qwen.
