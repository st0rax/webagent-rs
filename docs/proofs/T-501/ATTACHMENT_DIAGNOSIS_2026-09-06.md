# T-501 Attachment-Diagnose (mistral / qwen / zai / auto)

Stand: 2026-09-06. Owner der Board-Zelle bleibt `local/opencode` — diese Note
setzt **keine** Matrix-Zelle auf `passed` und markiert T-501 **nicht** done.
HomBot unberührt. Streaming kimi/mistral/zai und `managed_tools` hier nicht
bearbeitet (Absprache derfuhrer).

Quelle: bestehende Proofs unter `docs/proofs/T-501/attachment_*`, Matrix-Notes,
Upload-Pfad `src/browser/send.rs::attach_files`.

## Kurzmatrix

| Brain | Matrix | Symptom | Kategorie | Nächster Schritt |
|-------|--------|---------|-----------|------------------|
| mistral | failed | HTTP 200, Antwort nur Uhrzeit `4:01`, kein `RED` | Provider/UI verarbeitet Anhang nicht (oder Scraping liest Timestamp statt Chat) | Live mit `WEBAGENT_VERIFY_TRACE=1` + wake_renderer: Preview-Signal vs. Send; ggf. Absende-Beweis verschärfen |
| qwen | failed | `0 von 1 Dateien uebernommen` auch mit 256×256 PNG | SPA-Upload-Gap: synthetisches FileList/paste/drop wird verworfen; OSS-Pipeline braucht trusted FileChooser/Drag | Repo-Fix-Kandidat: CDP `Input.dispatchDragEvent` / FileChooser vor synthetischem DataTransfer |
| zai | failed | gleiches `0 von 1` mit echtem PNG | SPA-Upload Black Box; `selectors/zai.json` ohne stabile attach/file-Selektoren | Live-DOM-Vermessung (attach/paperclip/input[type=file]), dann Selektoren + gleicher trusted Upload-Pfad |
| auto | failed | Timeout ~240s, kein `RED` | AutoRouter+Attachment: langer Hang / falsches Ziel-Brain, kein klarer Upload-Fail | Router-Trace: welches Brain gewählt; Budget/Fehler weiterreichen statt stummem 240s |

Kontrast: **kimi** `passed` mit 256×256 (Clipboard-Paste + Vue-Uploader); der frühere
ABSENDEKNOPF-Fail lag am 1×1/70-B-Testbild, nicht an der Bridge
(`attachment_kimi_real_image_2026-09-05.json`).

## Code-Pfad (Ist)

`attach_files` versucht der Reihe nach:

1. Oberfläche öffnen (`attach_button`; mistral zusätzlich `file_upload_button`)
2. Alte Previews löschen
3. **Native Clipboard-Paste** für `kimi` **und** `mistral` (Bilder)
4. Synthetisches paste/drop (DataTransfer)
5. Native CDP/`set_file_input_files` + Input-Events
6. sonst Fehler `Browseroberflaeche hat nur N von M Dateien uebernommen`

Beweis für Übernahme: `attachment_signal_count()` bzw. `input.files` /
Send-Button (kimi).

## Einzelbefunde

### mistral — Timestamp statt Bildantwort

- Beleg: `attachment_mistral_2026-09-05.json` — `ok:false`, `http:200`,
  `reply:"4:01"`, Latency ~113s, 1×1 PNG.
- Matrix-Note: Attachment nicht verarbeitet.
- Code: mistral ist schon auf dem **selben Clipboard-Pfad wie kimi**. Wenn
  Paste „Ok“ meldet, aber die Antwort nur eine Uhrzeit ist, greift die
  Erfolgsbedingung zu früh (Signal/Send ohne Bildverständnis) **oder** die
  Antwort-Extraktion liest UI-Chrome (Wanduhr) statt Assistant-Text — verwandt
  mit dem bekannten mistral-Streaming-Timestamp-Problem, hier im Non-Stream-
  Attachment-Lauf.
- **Nicht** dasselbe wie qwen/zai (`0 von 1`).
- DoD-Implikation: Zelle bleibt `failed` bis Live belegt: sichtbare Preview +
  inhaltliche `RED`-Antwort (kein Timestamp-only).

### qwen — SPA lehnt File-Injection ab

- Belege: `attachment_qwen_2026-09-05.json` (1×1) und
  `attachment_qwen_real_image_2026-09-05.json` (256×256) — beide
  `0 von 1 Dateien`, HTTP 502 Bridge.
- Selektor: `attach_button` = Upload-aria; kein zweiter
  `file_upload_button`-Menüschritt wie mistral.
- Bewertung (unverändert ehrlich): **Brain-/SPA-Gap**, kein entartetes
  Testbild. Fix braucht trusted Drag/FileChooser, nicht nur größere PNGs.

### zai — gleicher Upload-Gap, dünne Selektoren

- Belege: `attachment_zai_*_2026-09-05.json` — `0 von 1`, auch 256×256.
- `selectors/zai.json`: keine belastbaren `attach_button` /
  `file_upload_button`-Einträge in der Attachment-Schlüsselmenge (anders als
  mistral/qwen).
- Ohne Live-DOM kein stabiler Öffnungs-Klick; Paste/native Input laufen ins
  Leere → `0 von 1`.
- Früherer Chat-Fail (Absendeknopf deaktiviert) ist ein **anderes** Symptom;
  hier scheitert schon die Dateiübernahme.

### auto — Timeout ohne RED

- Beleg: `attachment_auto_2026-09-05.json` — `timed out` ~240s, leere Reply.
- Kein eigenes `selectors/auto.json` (Router auf Ziel-Brains).
- Vermutung: Routing auf ein attachment-schwaches Brain (qwen/zai/mistral) oder
  Hänger ohne propagierten Upload-Fehler. Diagnose braucht Logzeile „auto →
  {brain}“ plus denselben Upload-Trace wie oben.

## Was wir absichtlich nicht tun

- Board-Claim nicht umschreiben; T-501 nicht `done`.
- Matrix-Status nicht auf `passed` setzen.
- `managed_tools` by-design failed nicht anfassen.
- Keine Live-Login-Session in dieser Note (Box ohne Provider-Logins) — Live-
  Nachweise auf dem Laptop mit wake_renderer, Absprache Streaming bleibt bei
  derfuhrer.

## Empfohlene Reihenfolge (Laptop)

1. qwen: VERIFY_TRACE + Screenshot nach attach_button; prüfen ob
   `input[type=file]` erscheint; dann DragEvent-PoC.
2. zai: DOM-Inventar für Paperclip/Upload; Selektoren ergänzen; dann gleicher
   PoC.
3. mistral: nach Paste Preview-Count + Assistant-Text; Timestamp-Filter in
   Antwortpfad prüfen.
4. auto: Routing-Trace eine Attachment-Anfrage; Budget an Ziel-Brain koppeln.
