> **Referenz.** Echter DOM- und Vorher/Nachher-Livebeleg für T-944: der
> sichtbare Denkbereich des Anbieters wurde als Antwort ausgeliefert. Der Fix
> schliesst deklarierte Denkknoten aus beiden Lesepfaden aus. Kein Tag, kein
> Release. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-944 — Denkprozess wird nicht mehr als Antwort ausgeliefert

**Datum:** 2026-11-21
**Task:** T-944 „Denkprozess des Anbieters wird als Antwort ausgeliefert"
**Branch (geplant):** `feature/T-944-reasoning-exclude`, ausgehend von
`feature/T-942-markdown-structure` (`19e8c6f`). Die abgelehnte T-961-Scheibe
wurde **nicht** mitgezogen.

## Methode

Webview-OAuth-Session der angemeldeten Brains, ein Turn pro Anbieter via
`webagent cloud web-view-stream --brain <brain> --prompt <nonce-Prompt>
--headless`. Kein API-Key, kein Token-Argument, keine Zugangsdaten eingegeben,
keine Profil-/Cookie-/Token-Datei gelesen, keine dauerhafte Kontoeinstellung
geändert. Die Antworten tragen eine frische prozessonly Nonce (`T944Z-…`);
`api_key: false`, `oauth_session: true` in allen ausgewerteten Läufen.
Rohdaten ausschliesslich unter
`C:\Users\storax\AppData\Local\Temp\opencode\T944-reasoning-20261121\`, nicht
ins Repo kopiert. Während der Live-Läufe wurden keine Repo-Dateien geändert.

## DOM-Beleg — wo der Denkbereich tatsächlich sitzt

Vorübergehend war ein umgebungsgesteuerter DOM-Dump im Lesekopf eingebaut
(`WEBAGENT_T944_DOMDUMP`); er wurde vor dem finalen Build vollständig entfernt
(`rg WEBAGENT_T944_DOMDUMP src/` → keine Treffer). Das finale Binary enthält
keinen Debug-Code.

**DeepSeek (`T944D-D9359E91`, `T944E-04B1BB81`, Dump-Hash
`A62C877F1A46A60EFDAFE121D591F08CDD3F1990A812338012C7287382830C2A`):** die
Oberfläche rendert den Denkbereich als **Geschwister** des Antwortknotens:
`.ds-think-content` mit Textbeginn `We need answer...`. Der gewählte
Antwortknoten `div.ds-markdown.ds-assistant-message-main-content` enthält nur
die Antwort. Weil der Denkbereich ausserhalb des Antwortknotens liegt, war der
ursprüngliche Bug bei DeepSeek nicht durch den Selektor erklärbar — wohl aber
die Gefahr, dass ein breiterer Antwortknoten ihn miterntet.

**Gemini (`T944G-873A1EA8`, Dump-Hash
`3DD89CC001D9751B26E7887CAAF1A4BD95526A6F491883B2D0205291A94D353D`):** im
gemessenen Turn gab es **keinen separaten sichtbaren Denkbereich**; der
Antwortknoten `div.markdown.markdown-main-panel.md-content` enthält direkt die
Antwortabsätze. Ein `reasoning_exclude` für Gemini zu erfinden wäre eine
Behauptung ohne Beleg — deshalb ist dort keiner deklariert.

**ZAI:** der konfigurierte `reasoning_toggle` war nicht anklickbar
(`[toggle] zai/reasoning_toggle: 'reasoning_toggle' nicht anklickbar`, Exit 1,
kein Zustandswechsel). Ein Deep-Think-Turn konnte nicht erzwungen werden.

**ChatGPT:** `selectors/chatgpt.json` kennt keinen Reasoning-Selektor-Key.
**Claude:** Survey exit 1 mit `url=https://claude.ai/login?from=logout…` — ohne
Eigentümer-Login nicht vermessbar.

## Fix

- `selectors/deepseek.json`: neuer deklarierter Ausschluss
  `reasoning_exclude: [".ds-think-content", "[class*='ds-think-content' i]"]`.
  Nur DeepSeek, weil nur dort ein sichtbarer Denkbereich im DOM belegt wurde.
- `src/browser/mod.rs`: `answer_read_body(...)` und `probe_generation_js(...)`
  entfernen die deklarierten Knoten aus einer **DOM-Kopie**, bevor `TX`/`TXMD`
  liest. Beide Auslesepfade sind betroffen — ein Fix in nur einem wäre
  wirkungslos geblieben. Ohne Deklaration bleibt der Lesekörper bytegenau wie
  zuvor; Ausfälle beim Klonen/CSS-match fallen auf den ursprünglichen Text
  zurück. Kein Text-/Formulierungsfilter, keine inhaltliche Filterung (non_goals
  eingehalten).
- `src/browser/verify.rs`: Signaturanpassung mit leerem Ausschluss (`"[]"`).
- Zwei neue Unit-Tests belegen: ohne Ausschluss unveränderter Lesekörper; mit
  Ausschluss nur `cloneNode`/`querySelectorAll`/`removeChild` der deklarierten
  Knoten, kein Textfilter.

DeepThink war bei der Messung **eingeschaltet**: der erste Toggle-Lauf schaltete
`aria-pressed=true → false`, der sofortige Restore `false → true`; der
abschliessende Survey zeigt weiterhin `ds-toggle-button--selected` für
DeepThink. Kein dauerhafter Zustandswechsel. (DeepSeek `Search` war ebenfalls
`selected` — das betrifft T-950 und wurde nicht verändert.)

## Finaler Livebeleg

Binary: `target/debug/webagent.exe`, SHA256
`64ADA47A726A74513A7249A6993E87BDDDEEB7A0FAF1E5B1199511E8AEC8BC90`.
Prompt je Brain: „Beginne mit `<nonce>`. Denke ausfuehrlich nach, antworte aber
NUR mit dem Ergebnis: Was ist 17*23?"

| Brain | Nonce | Exit | ausgelieferte Antwort | Nonce ok | Reasoning-Marker |
|---|---|---:|---|---|---|
| deepseek | `T944Z-9FDB9E20` | 0 | `T944Z-9FDB9E20 391` | ja | keine |
| gemini | `T944Z-B2AC9CFF` | 0 | `T944Z-B2AC9CFF 391` | ja | keine |
| qwen | `T944Z-A651188A` | 0 | `T944Z-A651188A 391` | ja | keine |
| zai | `T944Z-FD92CE43` | 0 | `T944Z-FD92CE43` / `391` | ja | keine |
| chatgpt | `T944Z-E7002AD0` | 0 | `391` | **nein** | keine |

Geprüfte Marker: `We need answer`, `Thought for`, `Denkprozess`, `reasoning`,
`I need to`, `Ich muss`, `Thought Process`. Kein Treffer in einer Antwort.
Alle fünf Läufe liefen über `webview_oauth` mit `api_key: false`,
`oauth_session: true`.

**Ehrliche Grenzen:** bei chatgpt wurde die Nonce nicht mitgeliefert — die
Antwort `391` ist korrekt, belegt aber nicht die Nonce-Propagierung; dieser Lauf
zählt nur als „kein Reasoning im Text", nicht als vollständiger Nonce-Beleg.
kimi und mistral lieferten Exit 2 mit leerer stdout-Datei, claude war nicht
angemeldet — für diese drei wird **keine** Aussage über Reasoning-Ausschluss
getroffen. ZAI konnte nicht in den Deep-Think-Modus geschaltet werden; sein
Lauf belegt nur, dass bei diesem Turn kein Reasoning ausgeliefert wurde. Ein
Turn mit *erzwungenem* Denkmodus ist nur für DeepSeek lückenlos belegt.

## Gates

| Gate | Ergebnis |
|---|---|
| `cargo test --lib` | 1501 passed / 0 failed / 1 ignored |
| `cargo fmt --all` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün |
| `git diff --check` | grün |

## Rohdatei-Hashes (SHA256)

```
deepseek-domdump.stdout.json     52991AC20460B093C2EEA178B47610DF2FFC4C31A18B67F065E6EA3AE6A18A53
deepseek-domdump2.stdout.json    A62C877F1A46A60EFDAFE121D591F08CDD3F1990A812338012C7287382830C2A
gemini-domdump.stdout.json       3DD89CC001D9751B26E7887CAAF1A4BD95526A6F491883B2D0205291A94D353D
deepseek-toggle-on.stdout.txt    5595BC4279E06A59E19A89D48C9FA191BB110A9C03D680929DD6B8885BA89549
deepseek-toggle-restore-on.stdout.txt 8761DC08F2D1FAC1DA539B9E5B766B9653C2B2B29090EF3C4D44BE6E73262E48
deepseek-survey-final.txt        50FB9627501412AE2E06DBF115DADBF9FEDF30A674F26BFF250C2DB4D9EAC774
zai-toggle-on.stderr.txt         2D61A310693B848341FC08443FE868067A60F10AD89EE4C418FB67ADAABE062F
deepseek-final2.stdout.json      25F62526C353CA5A745B001967308C70ECBD7D0E4398380B1AF0AA3318ADDF41
gemini-final2.stdout.json        579C75449CED12123C04EA242409F44F89153D8312FB6FEBE599B0101B52A93B
chatgpt-final2.stdout.json       3FDE5153FAA707E8B2C962148534AA9A2AF192E34423578C43CCD3A5AAB964E4
qwen-final2.stdout.json          8A17534555CC122EAA2DC9EA9C003EB58275A6C443E57F78E4C4EEBAD0808A09
zai-final2.stdout.json           E5ED6A7E3E0AC19AC5BFA6CB6FAAD2D1975331F04AB3334D224C617FB7DA202B
```

Zusätzliche Survey-/DOM-Dumps und stderr-Dateien liegen im selben Temp-Verzeichnis;
die nicht-leeren stderr-Dateien enthalten nur die übliche WebView-/Profil-Ausgabe
(`Failed to unregister class Chrome_WidgetWin_0`).

## Fazit

DoD erfüllt: für den einzigen im DOM belegten Anbieter mit sichtbarem
Denkbereich (deepseek) ist ein Ausschluss-Selektor deklariert, und der Turn mit
aktivem DeepThink liefert ausschliesslich die Antwort. Für die übrigen Brains
wird keine Ausschluss-Behauptung aufgestellt, die nicht messbar ist. Kein Tag,
kein GitHub-Release, kein Merge ohne vollständige Abnahme und Freigabe.
