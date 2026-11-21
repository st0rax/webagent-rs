**Referenz.** Echtes Live-Ergebnis der freigegebenen Auto→Required-Eskalation vom 21.11.2026. **Negativbeleg — kein Abnahmebeleg.** Kein Code ist durch diesen Lauf entstanden oder geändert worden; die Eskalation ist der gemergte Stand PR #96. **T-961 bleibt `free`.** Operativer Stand: `docs/CURRENT_WORK.md`.

# T-961 — Auto→Required live: NICHT ABGENOMMEN (0/7 saubere Läufe)

## Rahmenbedingungen

- Datum: 2026-11-21, 02:28–03:30 lokale Zeit.
- Binary: `target/debug/webagent.exe`, SHA256
  `61DC1281EA96EAC5B8B788C3D3CBE1418F2F4BB1E83FFAE1456949ABCDFDB69E`, gebaut aus
  `0d7d802` (Code identisch zum gemergten `f85010a`; `0d7d802` ändert nur die
  Handover-Doku).
- Schalter: `WEBAGENT_AUTO_TOOL_CHOICE_REQUIRED=1` in der Bridge-Prozessumgebung.
  Jede Bridge-Startmeldung bestätigt: `[api] auto_tool_choice_required aktiv
  (T-961): auto wird zu required eskaliert.`
- Port ausschließlich `8788`. Port `8787` blieb unberührt.
- Pro Lauf: frischer GUID-Token, nur in der Prozessumgebung; nie in einer Datei,
  einem Log oder einem Transkript gespeichert. Secret-Scan über allen hier
  abgelegten Rohdateien: 0 Treffer für `Bearer `, `WEBAGENT_API_KEY`, `apiKey`,
  `cookie`, `authorization`, `sk-`.
- Keine Repo-Datei wurde während eines Laufs geändert. Harness, `models.json` und
  Verifier lagen unter `%TEMP%\opencode\T961-live-20261121\` außerhalb des Repos.
- Kein Login, keine CAPTCHA, kein OTP, keine Kontoeinstellung, keine
  Profil-/Cookie-/Token-Datei. Der Textturn bestätigte die bestehende Anmeldung
  mit `PI_BRIDGE_OK` (21 s).
- Harness-Kompatibilität: die repo-eigene `examples/pi/models.json` enthält
  `"input": ["text","image","audio"]`; Pi 1.5.2 akzeptiert `audio` nicht. Die
  **temporäre** Kopie außerhalb des Repos wurde auf `["text","image"]` gesetzt.
  Das betrifft nur die Client-Konfiguration, nicht die API-Semantik.

## Zwei Messmethoden

1. **Pi-Smoke** (Kopie von `scripts/test-pi-provider.ps1`, nur Port 8788): Textturn
   plus fünf sequenzielle `read`-Aufrufe. Pi setzt `tool_choice` nicht pro Turn;
   der Schlussturn kann daher nicht auf `none` umgestellt werden.
2. **Direkter OpenAI-Client** (wie schon bei `required-mode-*`): sechs Requests
   gegen `/v1/chat/completions`. Turn 1–5 mit `tool_choice: "auto"` (wird vom
   Schalter zu `required` eskaliert) und lokalem `read`-Result; Turn 6 mit
   `tool_choice: "none"` als dokumentierte Aufrufer-Anpassung für die
   Textabschlussantwort.

## Ergebnisse

| Lauf | Methode | Ergebnis (streng) | Beleg |
|---|---|---|---|
| Pi 2 | Pi, `--debug` | **FAIL** — 0/5 Tool-Starts; vier `stopReason=error`, Navigationstimeout ~8 s | `auto-required-pi-2.jsonl`, `auto-required-pi-bridge-2.log` |
| Pi 3 | Pi, `--debug` | **FAIL** — 18 Tool-Starts, aber 15 Relay-Korrekturen, 20 `stopReason=error`, 6 Auto-Retries, **kein** `stopReason=stop` | `auto-required-pi-3.jsonl`, `auto-required-pi-bridge-3.log` |
| Direct 1 | direkter Client, `--debug` | **FAIL** — Turn 1 Navigationstimeout | `auto-required-direct-1.jsonl`, `auto-required-direct-bridge-1.log` |
| Direct 3 | direkter Client, headless | **FAIL** — Turn 3 HTTP 502 `model_output_envelope_invalid`: „lieferte trotz verpflichtendem Tool-Call nur Text"; auch nach Korrektur ungültig | `auto-required-direct-3.jsonl`, `auto-required-direct-bridge-3.log` |
| Direct 4 | direkter Client, headless | **FAIL** — Turn 4 HTTP 502; nach Korrektur unbekanntes Tool `files__search` | `auto-required-direct-4.jsonl`, `auto-required-direct-bridge-4.log` |
| Direct 5 | direkter Client, headless | **FAIL** — Turn 1 HTTP 502, identische Fehlerklasse | `auto-required-direct-5.jsonl`, `auto-required-direct-bridge-5.log` |
| Direct 6 | direkter Client, headless | **FAIL** — Turn 1–5 clientseitig sauber, aber 3 Relay-Korrekturen im Bridge-Log und Turn 6 (`none`) nach 180 s HttpClient-Timeout | `auto-required-direct-6.jsonl`, `auto-required-direct-bridge-6.log` |

**Saubere Abnahme-Läufe: 0/7.**

Ein weiterer Pi-Lauf (nummeriert 2 im temporären Verzeichnis, nicht verwechselt
mit der Tabelle) und ein Direct-Lauf 2 wurden beobachtet, aber ihre Rohdateien
wurden vor einer Harness-Korrektur nicht persistiert; sie zählen nicht als Beleg
und sind nicht in der Tabelle enthalten.

## Warum der Pi-Harness „PASS" meldete, obwohl der Lauf scheiterte

Der Harness prüft nur `exit_code == 0`, `tool_execution_start >= 5` und das
Vorhandensein der fünf Marker. In Pi 3 waren diese drei Bedingungen erfüllt,
obwohl das Rohtranskript 20 `stopReason=error`, sechs Auto-Retries und keine
erfolgreiche Textabschlussantwort enthielt. Ein strikter Verifier, der zusätzlich
`MODEL_OUTPUT_ERROR`, Relay-Korrekturen, Navigationstimeouts und `stopReason=error`
ablehnt, weist Pi 3 korrekt zurück. Die Harness-Meldung „PASS" ist daher **kein**
Abnahmebeleg.

## Befund

Die Eskalation ist funktional verdrahtet (der Schalter ist in jedem Bridge-Log
sichtbar, und die Turns werden mit Tools verarbeitet), aber sie macht den
Mehrschritt-Loop **nicht stabil**:

- Das Browsermodell liefert wiederholt trotz `required` eine reine Textantwort.
  Die vorbestehende T-953-Relay-Korrektur („eine Korrektur in derselben
  Konversation") rettet einzelne Turns, scheitert aber oft ihrerseits („Umschlag
  enthält keine Tool Calls", „unbekanntes Tool `files__search`").
- Selbst wenn fünf Tool-Turns clientseitig sauber durchlaufen (Direct 6), zeigen
  die Bridge-Logs Relay-Korrekturen; der Lauf ist damit nicht sauber grün.
- Der Schlussturn mit `tool_choice: "none"` ist die dokumentierte Anpassung,
  konnte aber nicht stabil bestätigt werden; in Direct 6 lief er in ein
  180-s-Timeout.

Damit bleibt der ursprüngliche Befund aus `required-mode-result.md` (1/8 grün
bei clientseitigem `required`) auch für die serverseitige Eskalation bestehen.
Ein Patch allein löst die Intermittenz nicht. **Kein PASS, kein `done`, kein
Tag/Release.** Weitere Läufe oder eine Änderung der Eskalationsstrategie
bräuchten eine neue ausdrückliche Freigabe des Eigentümers.

## Rohdatei-Hashes (SHA256, gekürzt auf 16 Zeichen)

| Datei | Bytes | SHA256 (16) |
|---|---:|---|
| `auto-required-pi-2.jsonl` | 22621 | `D75FED88C57B370D` |
| `auto-required-pi-3.jsonl` | 86915 | `F74B0CB4AD1ECF47` |
| `auto-required-direct-1.jsonl` | 1180 | `8BA6A2152EA30732` |
| `auto-required-direct-3.jsonl` | 4404 | `18CA4744CD1DEE7C` |
| `auto-required-direct-4.jsonl` | 6355 | `F05065A9ABBA48D0` |
| `auto-required-direct-5.jsonl` | 1284 | `4503DB7235B74041` |
| `auto-required-direct-6.jsonl` | 10666 | `4C689B0E6A0D2706` |
