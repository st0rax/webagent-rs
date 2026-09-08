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

## Messung 3 (Exakte Auswahl-API / ID-Roundtrip, 18:49–18:53 UTC)

Neue Selektoren-Keys in `selectors/{gemini,ZAI}.json`: `model_id_attr`,
`model_active_option`, `model_option_label` plus neue Verifier-Strecke
`model_roundtrip_exact` (ID-Roundtrip) in `src/browser/verify.rs` mit
Backend-Methoden `supports_exact_models` / `active_model_exact` /
`list_models_exact` / `select_model_exact` (`src/browser/ui.rs`). Auswahl laeuft
nun ueber exakte IDs statt Ganzlabels:

| Brain | Ergebnis | Latenz | Route | Beleg (Proof-Store, `selector_hash`) |
|---|---|---|---|---|
| gemini | **passed** (3/3) | 11.6 s | ID: `3.6 Flash -> 3.5 Flash-Lite` `[cf41b0e0dd7d53e5]`, Wechsel+Rueckweg=true | 311126731, ts 18:49:02/21/18:50:10 |
| zai | **passed** (3/3) | 11.0 s | ID: `GLM-5.3 -> GLM-5.3-Flash` `[x-preview-l]`, Wechsel+Rueckweg=true | 723949967, ts 18:50:30/46/18:51:34 |
| claude | failed | 1.2 s | Label-Fallback (kein `model_id_attr`) | ts 18:52 |
| perplexity | failed | 2.3 s | Label-Fallback (kein `model_id_attr`) | ts 18:53 |

DOM-Fakten je Brain:
- **gemini**: `gem-menu-item[data-mode-id=…]` als ID-Quelle; aktive Zeile =
  `class~="selected"` (Checkmark `aria-label="Ausgewählt"`, NICHT `data-active`
  — das ist Hover/Panel-Fokus); reines Label in `span.label`. Trigger `Flash`
  bleibt Teilstring, aber der ID-Roundtrip lauft daran vorbei.
- **zai**: `[data-testid='model-item']` mit `data-value` (`glm-5.3`,
  `glm-5.2`, `x-preview-l`), aktiv via `data-selected="true"`; Ganzlabel in
  `div.line-clamp-1` (erste Zeile). Substring-Paar `GLM-5.3` / `GLM-5.3-Flash`
  wird per ID eindeutig — in Messung 2 noch echte Mehrdeutigkeit.
- **claude**: Optionen tragen `data-model-id` (`claude-fable-5-1`,
  `claude-opus-5`), aber das aktive Modell (Sonnet 5) steht NICHT im Erstwohrh
  Menue (nur `Fable 5.1`, `Opus 5 Pro`, Untermenue `Aufwand Hoch`), und der
  Trigger kombiniert Modell+Effort. Strukturell nicht ueber den Roundtrip
  bestehbar → bewusst KEIN `model_id_attr`, Label-Fallback, failed.
- **perplexity**: keine exakten IDs, statischer Trigger `Modell` (kein
  Modellname im DOM). Kein `model_id_attr`, Label-Fallback, failed.

Dispatcher in `model_roundtrip` prueft zuerst `options_exact()` (≥2 distinct
IDs) → `model_roundtrip_exact`, sonst Label-Pfad. qwen/kimi/chatgpt unveraendert
via Label-Pfad (Testfixture-Defaults fuer die neuen Trait-Methoden liefern
`Ok(None)` ohne Seiteneffekt).

## Bewertung

- Deterministisches Fehlverhalten in Messung 1 ueber alle sechs Brains — kein
  Provider-Ausfall, kein flaky-Netz, kein Login-Problem (alle `diagnose` headless:
  `Ready`, `logged_in: true`, `composer: ok`, `cloudflare: false`).
- Ursache lag in der Selektoren-Ebene (Name+Desc gemischt). Fuer zwei Brains
  durch Namens-Knoten-Fixes behebbar.
- Zell-Uebergaenge: Baseline 6x `not_run`→`failed` (Messung 1); nach Nachzug
  2x →`passed`, 4x bleiben `failed` (dokumentierte UI-Grenzen). Messung 3
  (ID-Roundtrip): gemini + zai →`passed` (je 3/3), claude + perplexity bleiben
  `failed` — strukturelle Ausschlusskriterien, kein `model_id_attr` gesetzt.
  Gesamtstand nach Messung 3: **107 passed, 12 failed, 4 unreachable,
  7 not_run**.
- T-501 DoD ("Pro beworbenes Brain gruen") bleibt NICHT done: claude +
  perplexity sind strukturell nicht ueber exakte Auswahl oder Ganzlabel
  bestehbar (aktives Modell fehlt im Menue / kein Modellname im Trigger-DOM) —
  im Audit als dauerhaft dokumentierte Grenze erfasst.