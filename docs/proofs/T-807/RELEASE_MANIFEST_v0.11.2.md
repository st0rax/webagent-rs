# T-807 Release-Manifest v0.11.2 (Windows)

> **Archiv.** Release-Manifest/Beweis T-807; kein Betrieb. Lebend:
> docs/CAPABILITY_MATRIX.json + docs/PROVIDER_STATUS.md (Matrix),
> BRAIN_UNIFICATION_PLAN.md §7 (Gates).

Erstellt: 2026-09-11 nach Pflichtgates (cargo test --lib, --features tui,
beide cargo check).

## Build

| Feld | Wert |
|---|---|
| Version (Cargo.toml / FileVersion) | 0.11.2 |
| Commit (git HEAD) | `ab4112b632239370407a504674d90edc033f73db` |
| Branch | feature/T-807-live-release |
| Profil | release (optimized) |
| Host | win32, GNU-Toolchain (rustup override) |

## Artefakte

| Datei | Größe (Bytes) | SHA256 |
|---|---|---|
| target/release/webagent.exe | 8.380.928 | `96812C0680D565C690310B1D89C1AA23FB10161694804AC529D05ED3CC66FDBA` |
| target/release/WebView2Loader.dll | 165.336 | `465A7DDFB3A0DA4C3965DAF2AD6AC7548513F42329B58AEBC337311C10EA0A6F` |

WebView2Loader.dll liegt direkt neben webagent.exe (per
scripts/copy-webview2-loader.ps1; ohne den Kopierschritt startet die Binary
sonst mit 0xC0000135, DDL not found — siehe docs/PROVIDER_STATUS.md).

## Release-Zusammenstellung (CI-konform naming)

- `webagent-windows-x86_64.exe` (= webagent.exe)
- `webagent-windows-x86_64.exe.sha256`
- `WebView2Loader.dll`

GitHub-Release entsteht per Tag `v0.11.2` über `.github/workflows/release.yml`
(Linux-/Android-Assets baut die CI). Erst mit eindeutigem Abnahmestatus
veröffentlichen (BRAIN_UNIFICATION_PLAN §7).

## End-to-End-Run

Sichtbar ausgefuehrt mit dem frischen Release-Build v0.11.2
(`verify --brain mistral --cap chat --headless --skip-startup-reconcile`,
Standard-Profil ohne WEBAGENT_PROFILE_DIR-Override):

```
[verify] mistral: chat = Passed (11286ms) — chat belegt (count>baseline)
         (streaming: 5 appends, 1 replaces)
```

Hinweis: Der WEBAGENT_PROFILE_DIR-Override auf das Desktop-Shared-Profil
fuehrte bei mistral zu ABSENDEKNOPF_DEAKTIVIERT (falsches Profil fuer diese
Session); ohne Override (Standard-Profil `profiles/mistral`) Passed. Matrix/
live: docs/proofs/T-807/LIVE_ABNAHME_2026-09-11.md (chat 9/9 Passed).