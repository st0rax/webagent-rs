# T-501: deepseek model_switch / mode_switch — removed by-design 2026-09-11

**Referenz.** Datierter Live-Beleg. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`;
> aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

Datum: 2026-09-11, lokale Zeit (UTC+2). Binary: `target/debug/webagent.exe`,
frisch aus `master` gebaut (Stand: 3a7d30d / PR #55 + d2248e0 + df76c88),
headless (`--skip-startup-reconcile`), Kommandos `survey --dump` und
`shot --out`.

## Befund

DeepSeek (`https://chat.deepseek.com/`) hat die bisherige
Instant/Expert/Vision-Segmentleiste **kommentarlos entfernt**. Der Nutzer
bestätigt die Konsolidierung der drei Modi; die automatisierte Vermessung
belegt denselben Stand.

| Instrument | Ergebnis |
|---|---|
| `survey --dump --headless` | 113 Buttons, davon nur **2 beschriftete Elemente**: `DeepThink`, `Search`. `mode_option` (text=Instant/Expert/Vision) trifft **nichts** (0 Matches) |
| `mode --set Expert` / `mode --set Instant` | `Expert nicht in 'mode_option' gefunden` / dto. `Instant` — Segment-Key ist tot |
| `shot --out … --headless` + OCR (rapidocr-onnxruntime) | Screenshot zeigt Composer-Zeile mit "Message DeepSeek", DeepThink-, @ Search-Toggle — **kein Modell-/Modusname**, kein Chevron, kein Menü in der Kopfzeile |
| Composer-DOM (31 Zeilen Mai 2026-09-11) | `media-controls`: DeepThink-Toggle (x=265/396), Search-Toggle (x=381/512, selected), Attach (x=937/1068), Senden (x=981/1112). Kopfzeile y=15: 3 unbeschriftete Icon-Buttons (x=63/99/135, Klasse `_4f3769f`, keine Text-/aria-/title-Merkmale) |

## Interpretation

- **model_switch**: kein Menü-/Modellauswahler mehr im DOM → als
  `mode_switch != model` entlassen; Zelle wechselt von `unreachable` auf
  **removed** (by-design).
- **mode_switch**: Die Segmentleiste (capability.rs `mode_switch`,
  `needs: ["mode_option"]`, driveable:false seit 2026-07-28) existiert im
  heutigen UI nicht mehr. `select_segment` kann den Umschalteffekt nicht
  erzeugen. Können entfällt strukturell, nicht nur mangels Beleg-Marker.
- Die beiden Capability-Konzepte bleiben im Projektregister, falls ein
  Anbieter Segmentleisten/Modellmenüs weiterhin anbietet; für deepseek sind
  sie am Messdatum nicht vorhanden.

## Einordnung

- `selectors/deepseek.json`: Hygiene-Kommentar `_hygiene_model_vs_mode`
  aktualisiert, `model_switch` aus `ui_options` entfernt, `mode_option`-Liste
  entfernt, temporärer `_probe_header`-Schlüssel entfernt.
- `capability.rs`: Kommentar um den 2026-09-11-Befund ergänzt.
- Matrix-Zelle `model|deepseek`: `removed`, Datum 2026-09-11,
  proof_path auf diese Datei.
- T-501-DoD unverändert NICHT done (offen: effort-Zellen, model/auto, claude/
  perplexity model strukturell; siehe `model_switch_roundtrip_v2_2026-09-08.md`).