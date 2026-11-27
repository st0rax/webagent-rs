# Release- und Deployment-Beleg: v0.11.10 (2026-11-27)

> **Referenz.** Vollständiger Nachweis für Merge, Tag, GitHub-Release und
> lokales Deployment der Audit-Fixes. Der operative Stand steht in
> `docs/CURRENT_WORK.md`.

## Freigabe und Umfang

- Der Eigentümer hat am 27.11.2026 „immer sofort" für Merge, Tag,
  GitHub-Release und Deployment erteilt. Es wurde nicht erneut nachgefragt.
- Keine Force-Pushes, keine History-Umschreibung, keine Zugangsdaten gelesen,
  keine Live-Logins, keine Provider-Turns.
- Inhalt: `0216eb3` (Test-Isolation, Denylist, `/api/*`-Guard), `b7ff548`
  (HTTPS-Cap, `cargo audit`-CI, Lockfile-Patches, Archiv), `53768e3`
  (Reveal-Tab, Composer-Puffer) und `629009b` (Versionsbump/Doku).

## Versionsgate und lokale Gates

| Prüfung | Ergebnis |
|---|---|
| `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.10` | 0 |
| gleicher Aufruf mit `-Tag v0.11.9` | 1 (fail-closed) |
| gleicher Aufruf mit `-Tag v0.11.4` | 1 (fail-closed) |
| `cargo fmt --all -- --check` | 0 |
| `git diff --check` | 0 |
| `cargo clippy --features webview --all-targets --locked -- -D warnings` | 0 |
| `cargo test --workspace --lib --locked` | 1582 bestanden / 0 fehlgeschlagen / 1 ignoriert (463 + 194 + 636 + 289) |
| `cargo test --test arch_layers --locked` | 5/5 |
| `cargo check --features tui --locked` | 0 |
| `cargo check --no-default-features --locked` | 0 |
| `cargo build --release --locked` | 0 |
| `cargo audit` | 0 Vulnerabilities, 11 bewusst offene Warnungen |

Authorityärer Log: `C:\Users\storax\AppData\Local\Temp\opencode\audit-fix-gates.log`.

## Merge und CI

- Branch: `fix/audit-isolation-and-denylist`, Head `629009b0c79b5364735540d05a7a6bea2fbcce8d`.
- Fast-Forward `origin/master` `6e4ca4f` → `629009b` per
  `git push origin fix/audit-isolation-and-denylist:refs/heads/master`;
  `git merge-base --is-ancestor origin/master <branch>` → 0. Kein Force-Push.
- Der lokale `master`-Branch ist im Worktree `webagent-rs-phase16`
  ausgecheckt und wurde nicht verändert; `origin/master` ist die Referenz.
- CI `46066913891` (`https://github.com/st0rax/webagent-rs/actions/runs/46066913891`):
  `success`, vier Jobs grün (Linux WebView, Windows volle Features,
  Linux Standard, dependency audit).

## Tag und GitHub-Release

- Annotierter Tag `v0.11.10`, Tag-Objekt
  `63ed6c9951ead2f8a895dbe8caa8115854eba7c3`, auf `629009b0c79b5364735540d05a7a6bea2fbcce8d`.
- Release-Workflow `46067339957`
  (`https://github.com/st0rax/webagent-rs/actions/runs/46067339957`):
  `success` für `verify tag, manifest, tests`, `android-aarch64`,
  `linux-x86_64`, `windows-x86_64`, `github-release`.
- Release `https://github.com/st0rax/webagent-rs/releases/tag/v0.11.10`:
  public, kein Draft, kein Pre-Release, veröffentlicht
  `2026-11-27T21:06:47Z`.

### Sieben geprüfte Assets

| Asset | SHA-256 |
|---|---|
| `webagent-windows-x86_64.exe` | `7ef813d57a42a99a5ae4e59008f3ce476a89334562013c01726b37d7209ad4cc` |
| `webagent-windows-x86_64.exe.sha256` | `9dbdc89b255e8dbb0ed5f579e8c99ce176e7618472d19f11abae3faaa7d4d135` |
| `WebView2Loader.dll` | `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f` |
| `webagent-linux-x86_64` | `0794ebaf4c727a7deae6208732b76c335312f7c40d5167f23877f864eb919a91` |
| `webagent-linux-x86_64.sha256` | `7232f01c2b690324b82fedc53c1b2ef8cef76fee3125cf3fe3ab1959d5e04a33` |
| `webagent-aarch64-linux-android` | `a664f6cafec28c458dcc08355d1b70d549c88b3abbf1fdf7f9d2c838b5ecd5f1` |
| `webagent-aarch64-linux-android.sha256` | `97f51a26e4a474c5c0c17106cac1d8ddcc6b4a9f3ad59ae27f56f6a456492616` |

Alle sieben Dateien wurden nach
`C:\Users\storax\AppData\Local\Temp\opencode\v01110-assets\` geladen, gegen
die Release-`digest`-Felder und die drei `.sha256`-Dateien geprüft:
`BAD_COUNT=0`.

## Lokales Deployment

- Ziel: `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\`.
- Vorher aktiv: `webagent.exe` `9c33e843efd4f49cc188150334234960c59df7099e472fd76661aa6d34ef0ce7`,
  DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`.
- Backup mit geprüften Hashes:
  `...\webagent-deploy-target\backup-release-before-v0.11.10-20261127\`
  (`webagent.exe`, `WebView2Loader.dll`,
  `webagent-windows-x86_64.exe.sha256`, `hashes-before.txt`).
- Alte Prozesse `16880` (8789) und `18112` (8788) wurden gestoppt; beide Ports
  waren vor dem Kopieren frei.
- Aktiviert: EXE `7ef813d57a42a99a5ae4e59008f3ce476a89334562013c01726b37d7209ad4cc`,
  DLL `465a7ddfb3a0da4c3965daf2ad6ac7548513f42329b58aebc337311c10ea0a6f`,
  SHA-Datei `9dbdc89b255e8dbb0ed5f579e8c99ce176e7618472d19f11abae3faaa7d4d135`
  (alle drei gleich den Release-Assets).
- `webagent.exe --version` → `webagent 0.11.10 (629009b0c)`, Exit 0;
  `--help` Exit 0.
- `webagent.exe canary` → Exit 0, 8/8 `ok` (chatgpt, claude, deepseek,
  gemini, kimi, mistral, qwen, zai), `latency_ms=0`.
- Neustart: 8788 über `scripts/supervise-bridge.ps1` (PID `11684`,
  `health=ok`) bewusst ohne `--headless` gemäß T-1021; 8789 mit der
  ursprünglichen Kommandozeile `api serve --port 8789` (PID `10804`,
  `health=ok`). Beide Ports lauschen auf `127.0.0.1`.

## Rollback

1. Laufende `webagent.exe`-Prozesse stoppen und prüfen, dass 8788/8789 frei sind.
2. Die drei Dateien aus
   `backup-release-before-v0.11.10-20261127\` nach `release\` zurückkopieren.
3. Hashes gegen `hashes-before.txt` prüfen; die EXE muss wieder der geprüften
   vorherigen Datei `9C33E843…0CE7` entsprechen.
4. Bridge wie zuvor starten (8788 per Supervisor, 8789 mit
   `api serve --port 8789`).

## Grenzen

Nur lokales Deployment. Kein Fern-/Produktivziel, kein echter Provider-Turn,
keine Anmeldung, keine Profil-/Token-Datei geöffnet. Die 11
`cargo audit`-Warnungen und das bekannte Risiko einer fehlgeschlagenen
`page_exists`-Abfrage bleiben bewusst offen und sind in
`docs/CURRENT_WORK.md` dokumentiert.
