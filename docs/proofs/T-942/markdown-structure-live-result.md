> **Referenz.** Echter Vorher/Nachher-Livebeleg für T-942: Antworten mit
> Tabelle, Code-Fence und Liste verlieren beim DOM-Auslesen ihre Markdown-
> Struktur. Der Fix führt die Struktur aus echten DOM-Elementen zurück. Kein
> Tag, kein Release. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-942 — Markdown-Struktur beim Auslesen erhalten

## Freigabe und Rahmen

- T-942 ergänzt den in T-926 freigegebenen WebView-OAuth-Pfad. Es gilt weiterhin:
  **keine api keys, nur oauth**. Kein Token-, Cookie- oder Profilzugriff, keine
  Kontoeinstellung geändert, kein HTTP-Port.
- Datum: 2026-11-21, ca. 06:30–07:05 lokale Zeit.
- Binary Vorher: SHA256 `A0B5D0D1E87D4BCD6F6DF641BB23FAEBD0940F7185DE90DFECB47AD3493306CD`.
- Binary Nachher (final): SHA256
  `9EE80CE4E5D1DB477346FDB6D2E5A3B948A0CD6F011F4FDA559FF95773BFB760`.
- Während der Läufe wurden keine Repo-Dateien geändert. Rohdaten liegen außerhalb
  des Repos unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T942-markdown-structure-20261121\`
  und `…\T942-markdown-fix-20261121\`.

## Messmethode

Der Prompt enthält ein **bekanntes Quell-Markdown** (115 Zeichen nach
Zeilenenden-Normalisierung) und verlangt unveränderte Wiedergabe. Damit ist der
Quelltext exakt bekannt — es wird nichts aus der Antwort erraten:

```
| Name | Wert |
| --- | --- |
| A | 1 |
| B | 2 |

```rust
fn main() {}
```

- Punkt eins
- Punkt zwei
- Punkt drei
```

Jeder Lauf beginnt mit einem frischen GUID-Nonce; der Vergleich entfernt nur
diesen Nonce und normalisiert CRLF→LF. Befehl:
`webagent cloud web-view-stream --brain <brain> --prompt "<nonce> + Quelltext" --headless`

Der gemessene Pfad ist exakt der produktive Lesepfad: `browser_inference` →
`relay` → `WebBrainBackend::assistant_text` → DOM-Auslesen → Token-Event.

## Vorher — Strukturverlust (echter Negativbeleg)

| Brain | Nonce | Exit | Ausgelieferter Text (Auszüge) |
|---|---|---:|---|
| deepseek | `T942-05223CD7` | 0 | `Name\tWert\nModell\tdeepseek` — Tabellen als Tab-Text; `rust\nCopy\nDownload\nfn main(){}` — Fence weg, UI-Labels im Text; `Punkt 1\n\nPunkt 2` — Listenmarker weg |
| gemini | `T942-F228D3C9` | 0 | `Name\tWert\nIdentitaet\tgemini`; `Rust\nfn main(){}` — Fence weg; `Punkt eins\n\nPunkt zwei` — Listenmarker weg |

Damit sind alle drei in der DoD genannten Strukturen (Tabelle, Code-Fence,
Liste) live als verloren belegt. deepseek liefert zusätzlich die
Oberflächen-Labels `Copy`/`Download` als Antworttext — der gleiche Effekt, den
die Taskbeschreibung für Werkzeugargumente vermutet.

## Fix

| Element | Ort |
|---|---|
| `TXMD` — strukturelle DOM→Markdown-Rückführung im gemeinsamen Prelude | `src/browser/js.rs` (`JS_SEL_PRELUDE`) |
| Opt-in-Umschaltung `answer_markdown` (sonst unverändertes `TX`) | `src/browser/mod.rs` (`assistant_text`) |
| Selektordatei-Flag | `selectors/deepseek.json`, `selectors/gemini.json` |

`TXMD` ist **keine Heuristik aus Tab-getrenntem Text** (die der Task ausschließt),
sondern eine Traversierung echter DOM-Elemente:

- `<table>`/`<tr>`/`<td>` → Pipe-Tabelle mit Separatorzeile,
- `<pre>`/`<code>` → ```` ```<Sprache>\n…\n``` ````; Sprache aus
  `class*=language-` **oder** aus dem echten Code-Block-Header
  (`.md-code-block-banner` bei deepseek, `.code-block-decoration` bei gemini),
- `<ul>`/`<ol>` → `- ` bzw. `1. `, `<h1>`–`<h6>` → `#`…`######`,
- UI-Chrome wird verworfen: `button`, `svg`, `role=button`, `role=img`,
  `aria-hidden=true`, `aria-label` mit copy/download/kopier/herunterladen sowie
  die Klassen `ds-button`, `copy-button`, `download-button`, `mat-icon`,
  `gem-icon`.
- `TXENV` (Werkzeugumschlag, T-956) und KaTeX-Annotationen haben weiter
  Vorrang; ohne `answer_markdown` bleibt `TX` unverändert aktiv.

Die Header-Struktur wurde vorher nicht geraten, sondern mit einem temporären,
danach wieder entfernten Debug-Reader aus dem echten DOM gemessen.

## Nachher — strukturgleich (finaler Livebeleg)

| Brain | Nonce | Exit | Dauer | Roh-SHA256 (stdout) | Vergleich |
|---|---|---:|---:|---|---|
| deepseek | `T942H-23A8FEF4` | 0 | 11,3 s | `E1AC92517F52F6E524F2F64D02650A43DFB3F2038043C8B9EDF7A8B72A1E5956` | **STRUKTURGLEICH**, 115/115 Zeichen |
| gemini | `T942H-C9CE5F14` | 0 | 9,0 s | `6DE3D0BABB683CEA8537E05EB2C88CC0363F448D7B93EFD555C2B18B3A177C46` | **STRUKTURGLEICH**, 115/115 Zeichen |

Beide Läufe lieferten exakt:

```
| Name | Wert |
| --- | --- |
| A | 1 |
| B | 2 |

```rust
fn main() {}
```

- Punkt eins
- Punkt zwei
- Punkt drei
```

Kein `Copy`, kein `Download`, kein `?`-Icon, keine Tab-Trennung. Beide
stderr-Dateien sind leer (SHA256 der leeren Eingabe `E3B0C442…B855`).

Zwischenschritte, ehrlich ausgewiesen: `T942F-578DD4D4` (deepseek) und
`T942F-6BCE26EB` (gemini) zeigten nach der ersten Rückführung bereits Tabelle
und Listen, aber noch `Copy`/`Download` und `Rust` außerhalb des Fences sowie
ein `?`-Icon bei gemini. Die DOM-gestützte Header-/UI-Korrektur beseitigte das;
`T942G-*` und die finalen `T942H-*`-Läufe sind sauber.

## Ehrliche Einschränkungen

- Gemessen wurden **deepseek und gemini** — die beiden Sitzungen, die in T-926
  live bestätigt wurden. Für qwen, kimi, chatgpt und die übrigen Brains liegt
  **keine** Messung vor; das Opt-in `answer_markdown` ist dort bewusst **nicht**
  gesetzt, sodass sich ihr Verhalten nicht ändert.
- Die Rückführung rekonstruiert Markdown aus dem gerenderten DOM. Sie ist hier
  gegen ein bekanntes Quell-Markdown bytegenau belegt, aber kein Beweis dafür,
  dass jedes denkbare Oberflächen-DOM (z. B. verschachtelte Tabellen, versch.
  Code-Icon-Themen) identisch rekonstruiert wird. NeueDOM-Formen sind erneut zu
  messen.
- Der Vergleich normalisiert CRLF→LF; ohne Normalisierung unterscheiden sich
  die Zeilenenden, nicht der Inhalt.

Rohdatei-Hashes (SHA256), Vorher und Nachher:

```
T942-markdown-structure-20261121/deepseek.stdout.json   71D84476D57382D19B8C2E8E00C806E1E366625A1E1D28755D12FE788FAC5422  (483 B)
T942-markdown-structure-20261121/deepseek.stderr.log    E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
T942-markdown-structure-20261121/gemini.stdout.json     F65E09B7D3E2C31BA5338843C1EA2CE9093BD9D6460F039D21C157E5277F7906  (488 B)
T942-markdown-structure-20261121/gemini.stderr.log      D829233CE21CAEBE5AFC00C33336945C16A2D293A38B673B022636EA45C25900  (115 B)
T942-markdown-fix-20261121/deepseek-final.stdout.json   E1AC92517F52F6E524F2F64D02650A43DFB3F2038043C8B9EDF7A8B72A1E5956  (499 B)
T942-markdown-fix-20261121/deepseek-final.stderr.log    E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
T942-markdown-fix-20261121/gemini-final.stdout.json     6DE3D0BABB683CEA8537E05EB2C88CC0363F448D7B93EFD555C2B18B3A177C46  (497 B)
T942-markdown-fix-20261121/gemini-final.stderr.log      E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855  (leer)
```

Die nicht-leeren stderr-Dateien der ersten Läufe enthalten nur die übliche
WebView-/Profil-Ausgabe; die finalen Läufe haben leeres stderr.

## Gates

| Gate | Ergebnis |
|---|---|
| `cargo test --lib t942_` | 1 passed / 0 failed |
| `cargo test --lib` | 1499 passed / 0 failed / 1 ignored |
| `cargo fmt --all` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün (Binary `9EE80CE4…B760`) |
| `git diff --check` | grün |

## Einordnung

T-942 erfüllt seine DoD: Tabelle, Code-Fence und Liste kommen strukturgleich
beim Aufrufer an; der Vorher/Nachher-Vergleich liegt als Messung im
`proof_path`. **Kein Tag, kein GitHub-Release.**
