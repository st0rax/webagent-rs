**Referenz.** Lokaler Beleg der freigegebenen Semantikänderung Auto→Required vom 21.11.2026. **Kein Abnahmebeleg** — T-961 bleibt `free`, bis stabile grüne Raw-Live-Transkripte mit neuer Abnahme vorliegen; jeder Negativlauf blockiert. Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — Serverseitige Eskalation `tool_choice: auto` → `required`

Datum: 2026-11-21  
Branch: `fix/T-961-auto-required-escalation` (ab `master` `21afb42`)  
Freigabe: Nutzer hat am 2026-11-21 „Auto→Required freigeben" gewählt — bewusste Semantikänderung mit neuer Abnahme und frischer Live-Matrix.  
Scope: `src/api_bridge/content.rs`, `src/api_bridge/provider_handlers.rs`, `src/api_bridge.rs`, `src/browser_inference.rs`, `src/cli.rs`, `src/main.rs`, Tests, `docs/API_BRIDGE.md`  
Belegart: lokale Red-/Green-Tests, kein Live-Anspruch. Kein Tag/Release.

## Was sich ändert — und was nicht

`openai_tool_choice`, `responses_tool_choice` und `anthropic_tool_choice` behalten ihre clientseitige Bedeutung unverändert (`auto`/`none`/`required`/Function). Neu ist `content::effective_tool_choice(choice, tools, auto_tool_choice_required)`:

| Eingabe | Schalter aus | Schalter an |
|---|---|---|
| `Auto`, Tools vorhanden | `Auto` | **`Required`** |
| `Auto`, keine Tools | `Auto` | `Auto` |
| `None` | `None` | `None` |
| `Required` | `Required` | `Required` |
| `Function(name)` | `Function(name)` | `Function(name)` |

Die Eskalation wird an allen fünf Parser-Aufrufstellen in `provider_handlers.rs` angewandt (OpenAI/Anthropic/Responses, je synchron und inkrementell). Der Schalter ist **opt-in und standardmäßig aus**: `--auto-tool-choice-required` an `webagent api serve` und an der Web-UI mit `--api`, alternativ `WEBAGENT_AUTO_TOOL_CHOICE_REQUIRED=1` in der Prozessumgebung. Beim Einschalten schreibt die Bridge eine deutliche Startmeldung.

**Begründung für opt-in:** `required` nimmt dem Modell die Möglichkeit, einen Werkzeugloop mit einer Textantwort abzuschließen. Ein globaler, stiller Default würde jeden bestehenden Tool-Call-Client brechen, der nach dem letzten `role=tool`-Ergebnis eine Textantwort erwartet. Der Betreiber, der den Schalter setzt, akzeptiert diese Semantik; für den Schlussturn muss er `tool_choice: "none"` senden oder `tools` weglassen.

Zusätzlich wurde ein bestehender Widerspruch im Prompt behoben: `prompt_with_tools` bot auch bei `Required`/`Function` den Satz „Wenn kein Werkzeug erforderlich ist, antworte normal ohne Maschinenumschlag" an, obwohl `parse_response` eine Textantwort dort fail-closed zurückweist. Der Hinweis wird jetzt nur noch bei `Auto` eingefügt.

## Red vor Green

Test `api_bridge::tests::auto_tool_choice_required_eskaliert_nur_auto_mit_tools` prüft beide Schalterzustände und alle Wahlvarianten. Mit temporär deaktivierter Eskalation (`if false && auto_tool_choice_required`, Backup unter `%TEMP%\opencode\content.rs.T961.backup`):

```text
assertion `left == right` failed
  left: Auto
 right: Required
api_bridge::tests::auto_tool_choice_required_eskaliert_nur_auto_mit_tools ... FAILED
RED_EXIT=101
```

Nach Wiederherstellung und erzwungenem Rebuild (Timestamp des Quellfiles aktualisiert, damit Cargo den zwischenzeitlich zwischengespeicherten Testbinary nicht wiederverwendet):

```text
api_bridge::tests::auto_tool_choice_required_eskaliert_nur_auto_mit_tools ... ok
GREEN_EXIT=0
browser_inference::tests::required_prompt_bietet_keine_textantwort_an ... ok
PROMPT_EXIT=0
```

Der zweite Test belegt, dass der Text-Hinweis nur bei `Auto` im Prompt steht und bei `Required`/`Function` fehlt.

## Gates

```text
cargo fmt --all -- --check                       ok
cargo clippy --all-targets -- -D warnings         ok (0 warnings)
cargo test --lib                                 1485 passed; 0 failed; 1 ignored
cargo test --lib taskboard_invarianten_halten    ok
git diff --check                                 ok
```

Zwei neue Tests gegenüber dem Ausgangsstand 1483. Lokale Umgebung: MinGW (`windres.exe`, `ar.exe`) aus einem WinGet-Paket wurde nur für die Cargo-Läufe an `PATH` gehängt; keine Repo-Datei wurde dafür geändert.

## Nicht erledigt / bewusst draußen

- **Keine Live-Abnahme.** Die vorherige Messreihe `required-mode-negative-{1..8}.jsonl` (clientseitiges `required`, 1/8 grün) bleibt ein Negativbefund und ist kein Beleg für diesen Code. Die frische Live-Matrix mit dem neuen Binary und gesetztem Schalter steht noch aus.
- **T-961 bleibt `free`.** Kein PASS, kein `done`, kein Tag/Release. Erst stabile aufeinanderfolgende grüne Rohtranskripte mit mindestens fünf Werkzeugturns und ohne Negativlauf erlauben die Abnahme.
- Kein versteckter Retry, keine Kontoeinstellung, keine Zugangsdaten. Die Eskalation ist rein serverseitig und protokolliert nicht den Token.
