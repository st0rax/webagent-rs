**Referenz.** Lokaler Semantik-Erhaltungsbeleg zum Refactor T-914, kein Live-Nachweis. Operativer Arbeitsstand: `docs/CURRENT_WORK.md`; aktuelle Abnahme: `docs/CAPABILITY_MATRIX.json`.

# T-914 — API-Bridge-Medienhandler in `media.rs` isolieren

Datum: 2026-11-21  
Branch: `refactor/T-914-media-module` (ab `master` `e71a375`)  
Scope: `src/api_bridge.rs`, `src/api_bridge/media.rs` (neu)  
Freigabe: Nutzer hat den USER-SKIP vom 2026-09-13 am 2026-11-21 ausdrücklich aufgehoben (siehe `docs/CURRENT_WORK.md`).  
Belegart: statischer Blockvergleich gegen `HEAD` plus bestehende Audio-/Bild-Tests. **Kein Live-Anspruch**, kein Tag/Release.

## DoD 1 — Medienhandler liegen in `media.rs`

Folgende zehn Blöcke wurden aus `src/api_bridge.rs` in `src/api_bridge/media.rs` übernommen:

| Block | Funktion |
|---|---|
| `handle_image_generation` | OpenAI-Bildgenerierung, `n=1`, `b64_json`/`url` |
| `handle_audio_transcription` | Transkription/Übersetzung aus Multipart, Attachment an Brain |
| `is_audio_capability_refusal` | Erkennung höflicher Audio-Absagen |
| `handle_audio_speech` | fail-closed 502 für TTS |
| `MultipartPart` | Datenstruktur inkl. `name`, `file_name`, `content_type`, `data` |
| `multipart_text` | Feldwert aus geparsten Teilen |
| `multipart_parts` | **die Multipart-Schleife** mit `find_bytes`/Delimiter/Header |
| `run_image_generation_blocking` | Auto-Routing, Brain-Sperre, Relay-Aufruf |
| `ImageGenerationRequest` | Anfrageform der Bildgenerierung |
| `audio_mime` | MIME-Zuordnung wav/mp3/m4a/ogg/flac/webm |

Nur die Sichtbarkeit wurde geändert (`fn`/`struct` → `pub(super)`), damit Root und Tests die Funktionen weiter erreichen. Es gab **keine** Änderung an MIME-Grenzen, Base64- oder Upload-Semantik (T-914 non-goals). Die Root-Datei re-exportiert die Handlers für das Routing und die Tests.

## DoD 2 — Semantik-Erhaltung als Messung

Verglichen wurden die zehn Blöcke aus `git show HEAD:src/api_bridge.rs` mit den Blöcken in der neuen Datei. Normalisiert wurden ausschließlich

- das Sichtbarkeits-Schlüsselwort `pub(super) `,
- Whitespace (inkl. der von `rustfmt` erzeugten Zeilenumbrüche in den längeren
  `pub(super)`-Signaturen) und
- das trailing comma einer mehrzeiligen Parameterliste.

```text
fn handle_image_generation(                   IDENTICAL
fn handle_audio_transcription(                IDENTICAL
fn is_audio_capability_refusal(               IDENTICAL
fn handle_audio_speech(                       IDENTICAL
struct MultipartPart {                        IDENTICAL
fn multipart_text<'a>(                        IDENTICAL
fn multipart_parts(                           IDENTICAL
fn run_image_generation_blocking(             IDENTICAL
struct ImageGenerationRequest {               IDENTICAL
fn audio_mime(                                IDENTICAL
ALL_IDENTICAL=True
SHA256_NORMALIZED=338C4B572E51F2E9E75FEA319E5D0795FD4E4F7D933AD7E8FF8B2BE0B079CCB0
```

Der Hash ist über die konkatenierten normalisierten Blöcke in der Reihenfolge der Tabelle berechnet. Das Skript liegt temporär unter `%TEMP%\opencode\t914_verify.ps1`; der Vergleich wurde mit UTF-8-kodiertem `git show`-Output ausgeführt (die erste Fassung scheiterte an einer Konsolcodepage und an `rustfmt`-Whitespace — beide Artefakte sind im Ergebnis korrigiert, nicht als Funktionsunterschied gewertet).

## DoD 3 — Root hat keine Multipart-Schleife mehr

```text
MULTIPART_LOOP_OLD=1        (while let Some(relative) = find_bytes in HEAD:src/api_bridge.rs)
MULTIPART_LOOP_NEW_ROOT=0   (dieselbe Schleife jetzt in src/api_bridge/media.rs)
```

Damit ist T-921s Zustand erhalten: die Root-Datei enthält keinen Multipart-Parser mehr.

## DoD 4 — Bestehende Audio/Bild-Tests

```text
cargo test --features webview --lib api_bridge::tests::   54 passed; 0 failed
cargo test --lib                                          1483 passed; 0 failed; 1 ignored
```

Darunter die direkten Medien-Tests: `multipart_*`-Parsing inklusive `voice.wav`/`audio/wav`/Bytes, `is_audio_capability_refusal`-Marker sowie die Bild-/Audio-Routen über den SDK-Blackbox-Test.

## Gates

```text
cargo fmt --all -- --check                                  ok
cargo clippy --features webview --all-targets -- -D warnings ok
cargo test --features webview --lib api_bridge::tests::     54 passed; 0 failed
cargo test --lib                                           1483 passed; 0 failed; 1 ignored
git diff --check                                           ok
```

Lokale Umgebung: MinGW (`windres.exe`, `ar.exe`) aus einem WinGet-Paket wurde nur für die Cargo-Läufe an `PATH` gehängt; keine Repo-Datei wurde dafür geändert.

## Nicht erledigt / bewusst draußen

- Kein Live-Aufruf der Bild-/Audio-Endpunkte gegen ein echtes Web-Brain; dieser Beleg deckt nur die Modulgrenze und die vorhandenen Tests ab.
- Keine Verschiebung von Prompt-/Tool-Normalizern oder Response-Store (T-914 non-goals).
- Kein Tag, kein GitHub-Release.
