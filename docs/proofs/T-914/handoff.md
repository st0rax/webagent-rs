<!-- **Referenz: Beleg der T-914-Abnahme; keine aktuelle Betriebsanweisung.** -->
# T-914 Handoff - API-Bridge-Medienhandler isolieren

- Task: T-914
- Owner: local/opencode
- Branch: `refactor/T-914-api-bridge-media`
- Base: `master` `4454a6b`
- Commit: `0200fcd`

## Ist-Zustand vor dem Commit

- `src/api_bridge.rs` (Root) enthielt Bild-/Audio-/Multipart-Handler inline
  (`handle_image_generation`, `handle_audio_transcription`, `handle_audio_speech`,
  `MultipartPart`, `multipart_text`, `multipart_parts`, `AudioTask`, `ImageGenerationRequest`).
- Fruehere T-914-Arbeit hatte T-933-Halb-Integration eingeschleppt (doppelte
  Imports in `provider_handlers.rs`, doppeltes `ImageGenerationRequest` in media.rs,
  Orphan-`mod response_protocol`) -> in dieser Session komplett revertiert.

## Aenderung

- Neues Modul `src/api_bridge/media.rs` (Verbatin-Uebernahme der Handler aus master).
- Aus Root entfernt: Zeilen 362-638 (Media-Handler-Block) und das 2. `ImageGenerationRequest`.
- Root: `mod media;`, `route_request` ruft `media::handle_*`, Re-Export nur fuer Tests:
  `#[cfg(test)] pub(crate) use media::{is_audio_capability_refusal, multipart_parts, multipart_text};`
- `audio_mime` bleibt in Root (wird von `content.rs` via `super::audio_mime` genutzt).
- Syntaxfehler repariert: `find_map(|line| { ... })` schloss mit `};` statt `});`.

## Kopplung / Kopplungsgrenzen

- media.rs greift auf private Root-Items via `super::` zu: `api_error, authorize,
  decode_json, find_bytes, model_id, resolve_model, run_image_generation_blocking,
  run_task_blocking, unix_seconds, ApiFlavor, BridgeConfig, HttpRequest, HttpResponse`.
- Root-Re-Export nur unter `#[cfg(test)]` (fuer `api_bridge::tests`) -> kein
  Produktions-Re-Export.

## Verifikation

Siehe `docs/proofs/T-914/gates.txt` (fmt, clippy -D warnings, 1423 Tests gruen, diff --check).