> **Referenz.** Beleg für T-968 (kimi `reasoning_effort`). Operativer Stand:
> `docs/CURRENT_WORK.md`. Kein Merge, Tag, Release oder Deployment.

# T-968 — kimi `reasoning_effort` live belegt

**Datum:** 2026-11-25 · **Branch:** `fix/T-968-kimi-effort-roundtrip`
**Basis:** `b073d26` (Claim) · **Modus:** headless, bestehende angemeldete
kimi-Sitzung · **Kein** Login, kein Profil-, Cookie- oder Token-Zugriff,
keine Kontoaenderung, kein Tag/Release/Deployment.

## Ergebnis

`verify --brain kimi --cap reasoning_effort --headless` ist **passed**:

> Denkaufwand "Schnell Hoch" -> "Standard", unabhaengig gelesen "Schnell
> Standard"; Wechsel=true, Rueckweg=true (10054 ms)

Der Ausgangswert `Schnell Hoch` wurde in derselben Sitzung wiederhergestellt.
Die Matrix-Zelle `effort/kimi` ist ausschliesslich aus diesem rohen Lauf auf
`passed` gesetzt (`selector_hash=2705021080`).

## Rohe Belege

| Datei | Inhalt |
|---|---|
| `verify-live.txt` | Rohausgabe des erfolgreichen Live-Verify (stdout+stderr, ungekuerzt bis auf die WebView-Exit-Zeile) |
| `verify-live-failed-override.txt` | Rohausgabe des vorangegangenen Fehlversuchs mit altem User-Selektor-Override, inklusive Einordnung |
| `shots/survey-effort-submenu-hover.txt` | Entscheidender DOM-Survey: `aria-expanded=true`, `model-effort-option` mit `Standard` und `Hoch` |
| `shots/survey-model-menu.txt`, `shots/survey-effort-submenu.txt` | Aeltere Surveys, byte-gleich (SHA256 `8C2549A8…450CD`) — Untermenue war nicht geoeffnet |
| `shots/survey-effort-submenu-open.txt`, `shots/survey-effort-submenu-trusted.txt`, `shots/survey-effort-submenu-live.txt` | Zwischenschritte der Klick-Diagnose (synthetischer Klick, trusted CDP-Klick ohne Hover) |
| `shots/menu-effort-options-live.txt` | Diagnose: `effort_submenu` direkt ist im geschlossenen Modellmenue nicht anklickbar |
| `shots/kimi.png` | Screenshot der Oberflaeche |

Proof-Store (`%LOCALAPPDATA%\webagent\data\capability\proofs.jsonl`, letzter
Eintrag):

```json
{"brain_id":"kimi","capability":"reasoning_effort","ts":"2026-11-25T02:18:35.302666+00:00","outcome":"passed","selector_hash":2705021080,"winning_selector":null,"evidence":"Denkaufwand \"Schnell Hoch\" -> \"Standard\", unabhaengig gelesen \"Schnell Standard\"; Wechsel=true, Rueckweg=true","latency_ms":10054}
```

## Was der alte Befund wirklich war

1. `reasoning_effort_menu` las nur `div.current-model div.model-name span.name`
   und damit den Modellnamen `Schnell`, nicht den vollen Triggertext
   `Schnell Hoch`. Neuer erster Selektor: `[data-testid='model-select-trigger']`.
2. `reasoning_effort_option` enthielt `[role='menuitem']` und
   `#v-3-menu-trigger`; der zweite Pfadschritt traf damit erneut den
   Aufwand-Trigger statt eines Eintrags im Untermenue.
3. Das Untermenue oeffnet erst auf `pointerenter`. Ohne Hover bleibt
   `aria-expanded=false` und der DOM enthaelt nur 42 statt 44 Buttons. Der
   Hover-Pfad ist im Chain-Helfer und in `open_effort_submenu` nachgezogen.
4. Zusaetzlich lag ein Nutzer-Override ausserhalb des Repos obenauf:
   `%LOCALAPPDATA%\webagent\selectors\kimi.json` hatte
   `reasoning_effort_option = [data-testid='model-effort-item']`. Nur dieser
   Key wurde auf die echten Eintraege korrigiert. Original gesichert als
   `kimi.json.bak-T968-20261125` im selben Verzeichnis. Ohne diese lokale
   Korrektur bleibt der Agent ehrlich bei `failed` (siehe Fehlversuch oben).

Rechte Eintraege laut rohem DOM:
`[data-testid='model-effort-option'][role='menuitemradio']` mit `Standard`
und `Hoch`. Der Trigger ist `[data-testid='model-effort-item']` mit
`Denkaufwand Hoch`.

## Gates

- `cargo fmt` / `cargo build`: exit 0
- `cargo test --lib`: 1512 passed, 0 failed, 1 ignored
- `cargo clippy --all-targets -- -D warnings`: exit 0
- `git diff --check`: exit 0 (nur CRLF-Hinweis, keine Whitespace-Fehler)
- Fixture-Tests (`browser::verify::tests::effort_*`): 7 eigene Tests zu
  Label-Parsing, Alternativwahl, Roundtrip, Rueckwegfehler, fehlender
  Alternative, No-op-Antwort und Auswahlfehler mit Seiteneffekt; alle gruen.
- `brain_probe::tests::gegenprobe_schlaegt_nichts_falsches_vor`: gruen
  (`effort_submenu` als bewusste Ueberschneidung mit `model_menu` vermerkt).

## Grenzen

- Nur kimi wurde live nachgewiesen; andere Brains sind unveraendert.
- Der Beleg gilt fuer die zum Zeitpunkt des Laufs angemeldete Sitzung und die
  dort sichtbaren Werte (`Standard`, `Hoch`). Aendert die Oberflaeche ihre
  Struktur, muss der Verifier erneut laufen.
- T-961/G-002 bleibt `free`; daraus wird kein PASS erfunden.
- PR #103 ist mit Freigabe geoeffnet; kein Merge nach `master`, kein Tag,
  kein Release, kein Deployment ohne weitere ausdrueckliche Freigabe.
