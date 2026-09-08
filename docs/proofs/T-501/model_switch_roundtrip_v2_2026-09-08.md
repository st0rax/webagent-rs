> **Referenz.** Datierter Live-Reproof-Beleg. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`; aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

# T-501: Modellwahl Live-Reproof (roundtrip-v2) 2026-09-08

Datum: 2026-09-08. Arbeitsbranch `fix/T-501-model-proof` (rekonsolidiert auf
`origin/master` `a3036db` / Merge PR #54). Messbinary: `target/debug/webagent.exe`
(roundtrip-v2-Verifier aus `capability_proof.rs`).

## Messung 1 (Baseline, 06:37 UTC)

Seriell, headless, je Brain `webagent verify --cap model_switch --brain <id> --headless`:

| Brain | Ergebnis | Latenz | Beleg (Proof-Store `%LOCALAPPDATA%\webagent\data\capability\proofs.jsonl`) |
|---|---|---|---|
| zai | failed | 1310 ms | `model_switch` 2026-09-08T06:37:20Z, `button[aria-label='Select a model']` |
| claude | failed | 1163 ms | 2026-09-08T06:37:38Z, `button[data-testid='model-selector-dropdown']` |
| qwen | failed | 2283 ms | 2026-09-08T06:37:42Z, `[aria-label='Select Model']` |
| perplexity | failed | 2030 ms | 2026-09-08T06:37:48Z, `button[aria-label*='Modell' i]` |
| gemini | failed | 2485 ms | 2026-09-08T06:37:53Z, `button[aria-label*='Modusauswahl' i]` |
| kimi | failed | 1636 ms | 2026-09-08T06:37:57Z, `div.current-model` |

Einheitlicher Grund: **"Aktuelles Modell nicht eindeutig in der Laufzeitliste
erkennbar; kein Wechsel versucht"** — der roundtrip-v2-Verifier verlangt
tatsaechlichen Laufzeit-Wechsel + unabhaengiges Nachlesen + Restore in derselben
Sitzung; bei fehlender Eindeutigkeit bricht er fail-closed ab, ohne in die UI
einzugreifen. Keine Chatnachricht wurde gesendet, kein Profil veraendert, kein
Wechsel versucht.

## Ursachenanalyse (Root-Cause, per DOM-Diagnose)

Selektoren lasen den vollen `innerText` von Menue-Knopf und Optionen; die
aktuellen Modelle erschienen als Name+Beschreibung (bzw. Modell+Effort-Level)
gemischt und nie als reines Ganzlabel in der Laufzeitliste:

- **qwen**: Optionen = `div[role=option]` mit innerText `"Qwen3.7-Plus The
  high-performance..."`; exakter Name steht in `div.wms-list__name-text`.
- **kimi**: Trigger = `div.current-model` mit `"Schnell\nHoch"` (Modell +
  Effort), Optionen = `.model-item` mit Name+Beschreibung. Name exakt in
  `div.model-name span.name`.
- **zai**: Trigger `"GLM-5.3"` ok, aber Modellliste enthaelt `GLM-5.3-Flash`
  als Substring-Paar zu `GLM-5.3` → strikter Verifier verwirft Mehrdeutigkeit
  (unambiguous count=2) bewusst.
- **claude**: Trigger `"Sonnet 5 Hoch"` kombiniert Modell + Aufwand-Ebene;
  menuitemradio-Optionen sind getrennt (`Sonnet 5`, `Haiku 4.5`) → Triggerlabel
  kommt nie als Ganzlabel in der Liste vor.
- **gemini**: Trigger zeigt nur `"Flash"`, Optionen sind `"3.5 Flash-Lite"` /
  `"3.6 Flash"` (Modus+Version+Desc) → kein Ganzlabel-Match, Substring-Baum.
- **perplexity**: Trigger statisch `"Modell"`, aktueller Modellname wird im DOM
  nicht gerendert (bekannte v1.0-Grenze, DoD `perplexity-Entscheidung v1.0 vs.
  custom im Endtest on_record`).

## Messung 2 (Selector-Nachzug, 08:54–08:55 UTC)

`model_option`/`model_menu` auf die reinen Namens-Knoten verfeinert
(`selectors/qwen.json`, `selectors/kimi.json`):

| Brain | Ergebnis | Latenz | Beleg (Proof-Store, `selector_hash`) |
|---|---|---|---|
| qwen | **passed** | 5538 ms | `Qwen3.7-Plus -> Qwen3.8-Max`, Wechsel=true, Rueckweg=true; 179466165 |
| kimi | **passed** | 5537 ms | `Schnell -> K3 Swarm`, Wechsel=true, Rueckweg=true; 548048541 |
| zai | failed | 1135 ms | echte Mehrdeutigkeit (`GLM-5.3` vs `GLM-5.3-Flash`), 723949967 |
| claude | failed | 1152 ms | Triggerlabel Modell+Effort nicht als Ganzlabel, 3074261798 |
| gemini | failed | 2676 ms | Trigger `Flash` vs Optionen `3.5/3.6 Flash`, 311126731 |
| perplexity | failed | 2080 ms | statischer Button `Modell`, kein Modellname im DOM, 3864292236 |

qwen/kimi bestehen jetzt den vollstaendigen Roundtrip (Wechsel + unabhaengiges
Nachlesen + Restore) headless. zai/claude/gemini/perplexity bleiben ehrlich
fail-closed — jeweiks durch UI-Struktur bedingte Ganzlabel-Mehrdeutigkeit, kein
Selektor-Bug, kein UI-Eingriff versucht.

## Bewertung

- Deterministisches Fehlverhalten in Messung 1 ueber alle sechs Brains — kein
  Provider-Ausfall, kein flaky-Netz, kein Login-Problem (alle `diagnose` headless:
  `Ready`, `logged_in: true`, `composer: ok`, `cloudflare: false`).
- Ursache lag in der Selektoren-Ebene (Name+Desc gemischt). Fuer zwei Brains
  durch Namens-Knoten-Fixes behebbar.
- Zell-Uebergaenge: Baseline 6x `not_run`→`failed` (Messung 1); nach Nachzug
  2x →`passed`, 4x bleiben `failed` (dokumentierte UI-Grenzen). Gesamtstand nach
  Messung 2: **105 passed, 14 failed, 4 unreachable, 7 not_run**.
- T-501 DoD ("Pro beworbenes Brain gruen") bleibt NICHT done: zai/claude/gemini/
  perplexity sind nur ueber eine exakte Auswahl-API (statt Ganzlabel) oder
  UI-seitige Spezialisierung bestehbar — im Audit als kuenftige Grenze erfasst.