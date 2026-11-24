> **Referenz.** Echter Livebeleg für T-950: anbieterseitige Websuche umgeht
> Client-Werkzeuge und blieb unsichtbar. Die Bridge schaltet die Websuche dort
> aus, wo sie sichtbar und bedienbar ist, kennzeichnet alle übrigen Brains in
> `/v1/models` als nicht isolierbar und protokolliert erkannte Quellenhinweise
> in `brain_score`. Kein Tag, kein Release. Operativer Stand:
> `docs/CURRENT_WORK.md`.

# T-950 — Websuche des Anbieters: Isolation und Sichtbarkeit

**Datum:** 2026-11-21
**Task:** T-950 „Websuche des Anbieters umgeht Client-Werkzeuge und ist unsichtbar"
**Branch (geplant):** `feature/T-950-websearch-isolation`, ausgehend von
`feature/T-944-reasoning-exclude` (`584f071`).

## Rahmen und Schutzregeln

- Es gilt weiterhin **keine api keys, nur oauth**: WebView-OAuth-Sessionen der
  angemeldeten Brains. Kein API-Key, kein Token-Argument, keine Zugangsdaten
  eingegeben, keine Profil-/Cookie-/Token-Datei gelesen.
- Keine dauerhaften Kontoeinstellungen geändert. Der Websuche-Umschalter ist
  eine Eigenschaft der jeweiligen Konversation; nach dem Lauf zeigt eine
  erneute Survey wieder `ds-toggle-button--selected` (Hash
  `50FB9627…4D9EAC774`) — kein dauerhafter Eingriff.
- Bridge-Livebeleg ausschließlich auf Port **8788**, Loopback
  `127.0.0.1:8788`. Frischer, prozess-only Token (32 Hex-Zeichen,
  `[guid]::NewGuid().ToString("N")`), nur in der Prozessumgebung des Bridge-
  Starts gesetzt, in keine Datei und nicht in das Repo geschrieben.
- Während aller Läufe wurden **keine Repo-Dateien geändert**. Rohdaten liegen
  ausschließlich unter
  `C:\Users\storax\AppData\Local\Temp\opencode\T950-websearch-20261121\`.
- Binary der DOM-/Toggle-Surveys: `64ADA47A…1199511E8AEC8BC90`.
- Finales Binary des Bridge-Belegs: `target/debug/webagent.exe`, SHA256
  `20DC755279D085C697BB218D52F82966954BBF289B5669F669BE5110A2D82FB8`.

## Was der Ausgangszustand war

Der Ursprungsfall (2026-09-14, Pi → `webagent/chatgpt`): der Client sandte nur
eine URL auf eine öffentliche Textdatei. ChatGPT rief kein Client-Werkzeug auf,
sondern öffnete die URL mit der eigenen Websuche; die Antwort trug den
Quellenhinweis „GitHub". Dieser Abruf erschien weder im Pi-Transkript noch in
`brain_score`. Vor dem Fix kannte die Bridge für keinen Brain eine
Websuche-Abschaltung, prüfte keinen Zustand und erkannte keine Quellenhinweise.

Reproduziert mit `web-view-stream` (`chatgpt`, Nonce `T950S-160D2F43`,
Rohdatei-Hash `C3DA6614…F994807F3B4D41F68F95D45A`): die Antwort endete mit
`Quelle: Offizielle Rust-README.md auf GitHub.` — ein explizit markierter
Hinweis, genau die Form, die der neue Detektor erkennt.

## Umsetzung

| Element | Ort |
|---|---|
| `WebSearchObservation` (`off_before_send`, `already_off`, `not_isolatable`, `toggle_failed`) und Puffer | `src/brain_score.rs` |
| Neue Event-Felder `web_search` und `source_hints` | `src/brain_score.rs` (`Event`) |
| `source_hints(text)` — nur explizit markierte Quellenhinweise, kein Raten aus Fließtext | `src/observer.rs` |
| `ensure_web_search_off()` — nach `new_chat`, vor dem Senden; bounded Wait für die SPA | `src/browser/ui.rs` |
| `relay_web_search_state()` am `RelayBackend`, Aufruf im Turn, Quellenhinweis-Logging | `src/relay.rs` |
| `web_search_isolation(brain)` und `/v1/models`-Feld `web_search_isolation` | `src/api_bridge/catalog.rs` |
| Katalog-Test `t950_nur_deepseek_wird_als_websearch_isolierbar_ausgegeben` | `src/api_bridge/tests.rs` |

Der Turn ist so aufgebaut: `new_chat` → Websuche-Zustand messen und
gegebenenfalls ausschalten → senden → Antwort → Quellenhinweise erkennen →
`record_event_with_sends`. Beide Puffer sind pro Turn und werden beim Schreiben
konsumiert; ein abgelehnter Turn schleppt seine Hinweise nicht in den nächsten.

Wichtig: es werden nur **explizit markierte** Hinweise erkannt (`Quelle:`,
`Quellen:`, `Sources:`, `Referenzen:` …) samt folgender Einträge. Ein bloßes
Erwähnen von „GitHub" im Fließtext ist kein Beleg und wird nicht geraten. Der
Antworttext wird nicht verändert — die Hinweise werden nur protokolliert.

## Je Brain: ist die Websuche abschaltbar?

| Brain | Abschaltbar | Beleg / Grund |
|---|---|---|
| **deepseek** | **ja** | `Search`-Toggle sichtbar, `ds-toggle-button--selected`; Klick `aria-pressed=true → false`, danach Survey wieder `selected`. Konfiguriert in `selectors/deepseek.json`. |
| zai | nein | `web_search_toggle` trifft einen Sidebar-Menüeintrag; synthetischer und Maus-Klick ohne Zustandswechsel (Exit 1). |
| chatgpt | nein | kein Websuche-Umschalter im DOM; „Suche" ist die Chatverlaufssuche. |
| gemini | nein | kein Websuche-Umschalter; „Chats durchsuchen" ist die Verlaufssuche. |
| kimi / mistral / qwen | nein | kein Websuche-Umschalter im DOM. |
| claude | nein | ohne Eigentümer-Login nicht vermessbar (`url=https://claude.ai/login?…`). |
| perplexity | nein | kein konfiguriertes Brain (Survey: „Unbekanntes Brain"). |
| auto | nein | virtuelles Routing-Ziel; Isolierung hängt vom gewählten Brain ab. |

Damit ist nur DeepSeek live als isolierbar belegt. Alle anderen Brains werden
in `/v1/models` und in `docs/API_BRIDGE.md` ausdrücklich als **nicht
isolierbar** gekennzeichnet.

`/v1/models` liefert je Brain ein Feld `web_search_isolation` mit `status` und
`reason`, z. B. deepseek `switchable_disabled_for_api`, chatgpt
`not_isolatable` („kein Websuche-Umschalter im live DOM; 'Suche' ist die
Chatverlaufssuche"), zai `not_isolatable` („web_search_toggle trifft einen
Sidebar-Menüeintrag; kein Zustandswechsel messbar"). Live-GET-Hash
`59F1E695…ED46797AD79BB36DE0D682`.

## Livebeleg über die Bridge (Port 8788)

Dienst: `webagent api serve --brain chatgpt --headless --port 8788
--timeout-secs 120`. `/health` → `{"service":"webagent-provider-bridge","status":"ok"}`.

| Turn | Modell | Nonce | Exit | Ergebnis im `brain_score`-Event |
|---|---|---|---:|---|
| 1 | `webagent/chatgpt` | `T950F-CDD54DDB` | OK | `web_search.status=not_isolatable` („kein web_search_toggle konfiguriert"), `source_hints=["Offizielles Rust-README.md auf GitHub"]` |
| 2 | `webagent/deepseek` | `T950G-C9645851` | OK | `web_search.status=off_before_send`, `before=aria-pressed=true\|…ds-toggle-button--selected`, `after=aria-pressed=false\|…ds-toggle-button` |

Beide Events: `success=true`, `sends=1`. Der ChatGPT-Antworttext endete
wörtlich mit `Quelle: Offizielle Rust-README.md auf GitHub.` (Completion-Hash
`72351F58…919E1B`); der Detektor hat genau diesen Hinweis in das Event
überführt. Der DeepSeek-Turn hat den aktiven `Search`-Schalter vor dem Senden
ausgeschaltet und den Zustandswechsel belegt (Completion-Hash
`CFC48083…8F13208F`). Das Bridge-Log zeigt
`[relay] chatgpt: Websuche nicht isolierbar (not_isolatable)`,
`[relay] chatgpt: 1 Quellenhinweis(e) in der Antwort erkannt` und
`[browser] deepseek: web_search_toggle ist aktiv, wird vor dem Senden
ausgeschaltet`.

### Ehrliche Zwischenbefunde

Ein erster Bridge-Lauf mit dem Zwischen-Binary `2D67A22…29078` meldete für
DeepSeek `not_isolatable` („web_search_toggle nicht im DOM gefunden"), obwohl
derselbe Umschalter in einer frischen CLI-Sitzung sofort erreichbar war
(Toggle-Hash `FC4869A0…4DB8AF3F`). Ursache war ein Rendertiming direkt nach
`new_chat`, kein fehlender Schalter. Der Fix ist ein bounded Wait (20 × 150 ms)
im `ensure_web_search_off`; der erneute Lauf mit dem Final-Binary liefert
`off_before_send`. Der negative Lauf ist nicht verwässert, sondern durch den
positiven Lauf mit demselben Belegpfad ersetzt.

## Grenzen

- Für alle Brains außer DeepSeek bleibt ein *stiller* Provider-Abruf ohne
  Quellenhinweis für die Bridge unsichtbar. Der Fix macht ihn **erkennbar, wenn
  die Oberfläche einen Hinweis ausgibt**, und kennzeichnet das Risiko in
  `/v1/models` + `API_BRIDGE.md`; er behauptet keine vollständige Isolation, wo
  keine Schaltfläche existiert.
- Claude blieb ohne Login nicht vermessbar, Perplexity ist kein konfiguriertes
  Brain, und beim ZAI-Toggle war kein Zustandswechsel feststellbar. Für diese
  wird nur die Nicht-Isolierbarkeit, keine positive Abschalt-Behauptung
  gemacht.
- Die Umschaltung betrifft nur die laufende Konversation; eine dauerhafte
  Kontenänderung wurde nicht vorgenommen und durch die Folge-Survey widerlegt.

## Gates

| Gate | Ergebnis |
|---|---|
| `cargo test --lib` | 1505 passed / 0 failed / 1 ignored |
| `cargo fmt --all` | grün |
| `cargo clippy --all-targets -- -D warnings` | grün |
| `cargo build` | grün |
| `git diff --check` | grün |
| `taskboard::tests::taskboard_invarianten_halten` | grün (90 done / 1 free / 91) |

## Rohdatei-Hashes (SHA256, Auszug)

```
BIN final                    20DC755279D085C697BB218D52F82966954BBF289B5669F669BE5110A2D82FB8
v1-models-final.json         59F1E69568ED05F9BF664D99771ECCFECB7950870AED46797AD79BB36DE0D682
chatgpt-request-final.json   90AD6C2FA3431737439497FB308133AF6AC48D84E6B7030AA3C1BD0E2FD8E5BF
chatgpt-completion-final.json 72351F587DC0ACAA9B03563418292A1857429A25527A775E7C9B974629919E1B
deepseek-request-final.json  31D0B8C6ED8FE005B674A5DBC5740B97420FD19E87362C7E41758E26577C82D1
deepseek-completion-final.json CFC480838DC4DBBB4125236C71E8EF3C4B30DBAE51DCDFD2D31FBD8E8F13208F
bridge-final.log             D88A997FE80CAEA536868337855FEDA2A2A7684C79A109C2BC9ACE90398F68F3
chatgpt-sourcehint.stdout.json C3DA6614A8AB340BFE2BDAEE25FD3EC2079AB187F994807F3B4D41F68F95D45A
deepseek-survey.txt          50FB9627501412AE2E06DBF115DADBF9FEDF30A674F26BFF250C2DB4D9EAC774
deepseek-survey-finalcheck.txt 50FB9627501412AE2E06DBF115DADBF9FEDF30A674F26BFF250C2DB4D9EAC774
deepseek-toggle-recheck.stdout.txt FC4869A0C9D86986F32F3E14B3FB4CBEF2CC3F7E9AECFA4666D775C04DB8AF3F
zai-web_search_toggle-off.stderr.txt 7DAC43605D748C334BCBF18B90A4FE91ADE73C799C7C926BB0430F37E7F42300
chatgpt-survey.txt           580A75311A0E0B25B233F950F90083397818E515DE0648A6F78C9D2588FA2D99
gemini-survey.txt            31539DC8B21A778AFE9D7FCB7FB836F9A0E1D86B8945A613146160F746643276
claude-survey.txt            F4A272522D446519375CAC75C915E1BFBC6DD6E0ACADB97F59010E550801C363
kimi-survey.txt              25DD90FC7E5405CFA4470F7ED66DDA4BD6D03987B470FF4BD12ED7A64ADD1A60
mistral-survey.txt           670666742D577EE11EE6BE4313CFCCB04C285AEDA5A70ED1D28B82C702520033
qwen-survey.txt              63272BCF941646F1202526F90E7E728434322FBAC3375E82422F749046D61FCC
zai-survey.txt               8CDB3176157B08B55EEC928D2D0D7B3DE272614689F528582EA9555EF2010E19
perplexity-survey.err        9339EE71EACACAF97FD95ECEB60040D6485B2E2B32162AF0AE4C92D095C00FEE
```

Alle Survey-, Toggle-, Completion- und Log-Dateien liegen im genannten
Temp-Verzeichnis; die nicht-leeren stderr-Dateien enthalten nur die übliche
WebView-/Profil-Ausgabe (`Failed to unregister class Chrome_WidgetWin_0`)
bzw. die genannten Nicht-Isolierbarkeits-/Login-Meldungen.

## Fazit

DoD erfüllt: je Brain ist dokumentiert, ob die Websuche abschaltbar ist; wo sie
es ist (nur DeepSeek), schaltet die Bridge sie für API-Anfragen nach
`new_chat` vor dem Senden aus und belegt den Zustand; wo nicht, kennzeichnet
`/v1/models` und `docs/API_BRIDGE.md` das Brain als nicht isolierbar; ein
erkannter Quellenhinweis wird in `brain_score/events.jsonl` protokolliert. Kein
Tag, kein GitHub-Release, kein Merge ohne vollständige Abnahme und Freigabe.
