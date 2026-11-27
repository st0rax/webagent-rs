> **Referenz.** Verbindliche Reihenfolge für Release-Tags. Operativer Stand:
> `docs/CURRENT_WORK.md`. T-967 hat selbst keinen Tag und kein GitHub-Release
> erzeugt. Der Eigentümer hat am 2026-11-25 den Release `v0.11.5` nach PR,
> Merge und grüner CI freigegeben; dieser Vorgang läuft separat von T-967.
> Der Patch-Release `v0.11.6` wurde am selben Tag über PR #107 (`1149dff`)
> nach grüner CI getaggt und mit Workflow `45702611313` veröffentlicht.
> Der Patch-Release `v0.11.7` folgte am 26.11.2026 über PR #112 (`d54d347`)
> mit vollständig grünem Workflow `45725669798`, sieben geprüften Assets und
> lokalem Deployment; der Live-Turn-Beleg liegt unter
> `docs/proofs/live-turn-2026-11-26/`.
> Der Patch-Release `v0.11.8` (Tag-Commit `fb7b886`) wurde am 26.11.2026
> veröffentlicht. Die Commits danach (T-1007..T-1018) waren zunächst bewusst
> **nicht** neu getaggt; G-009 verlangte dafür eine neue ausdrückliche Freigabe.
> Diese Freigabe wurde am 27.11.2026 erteilt. Der Patch-Release `v0.11.9`
> umfasst T-1007..T-1021 und wurde am 27.11.2026 nach grünen Gates und Merge
> auf `master` mit Workflow `45950140641` veröffentlicht (Tag-Commit
> `87d0153`, sieben geprüfte Assets). Die lokal deployte EXE stammt aus
> `f11f604` (SHA-256 `9C33E843…0CE7`) und läuft mit dauerhaft sichtbarem
> WebView; sie ist nicht das CI-Release-EXE (`4C01B44C…73D`). Ein
> Fern-Deployment fand nicht statt. Belege: `docs/CURRENT_WORK.md`,
> `docs/proofs/T-1019/`, `docs/proofs/T-1020/`, `docs/proofs/T-1021/`.
> Der Patch-Release `v0.11.10` enthält die Audit-Fixes aus
> `fix/audit-isolation-and-denylist`. Der Eigentümer hat am 27.11.2026
> ausdrücklich „immer sofort" für Merge, Tag, GitHub-Release und Deployment
> freigegeben; alle vier Schritte wurden ohne erneute Rückfrage ausgeführt:
> annotierter Tag `63ed6c9` auf `629009b`, Workflow `46067339957` vollständig
> grün, Release public, sieben SHA-256-geprüfte Assets und lokales Deployment
> mit Backup/Rollback. Belege: `docs/CURRENT_WORK.md` und
> `docs/proofs/release-v0.11.10-2026-11-27/README.md`.

# Release-Vorgang (T-967)

## Historischer Befund

Der Tag `v0.11.4` wurde am 28.10.2026 von einem Commit erzeugt, dessen
`Cargo.toml` noch `0.11.3` enthielt. Der Release-Workflow baute und veröffentlichte
bei jedem `v*` ohne Versionsprüfung und ohne Tests. Ein Rebuild dieses Tags
meldet daher die falsche Version; ein künftiger falsch benannter Tag würde
still veröffentlicht werden. Der bestehende Tag bleibt historisch und wird
**nicht** verschoben, gelöscht oder force-gepusht. Ein erneuter Lauf des neuen
Workflows gegen `v0.11.4` scheitert absichtlich an der Versionsprüfung.

## Reihenfolge

1. **Bump:** `Cargo.toml` `[package].version` und den `webagent`-Eintrag in
   `Cargo.lock` auf dieselbe nächste Patch-Version setzen (aktuell `0.11.10`).
2. **Lokal prüfen:**
   `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.10` → Exit 0.
   Gegen `v0.11.4` muss das Skript Exit 1 liefern.
3. **Gates:** `cargo fmt --all -- --check`,
   `cargo clippy --all-targets -- -D warnings`, `cargo test --lib`,
   `git diff --check`.
4. **Merge:** den geprüften Stand nach `master` mergen. Erst danach taggen.
5. **Tag:** `vX.Y.Z` ausschließlich auf dem gemergten Commit setzen, dessen
   Manifest und Lockfile genau diese Version tragen.
6. **Publish:** der Workflow `release.yml` führt zuerst den `verify`-Job aus
   (Versionsprüfung für Tag-Refs plus `cargo test --lib --locked`); alle Build-
   und Publish-Jobs hängen davon ab. ohne grünen `verify` kein Artefakt.

## Windows-DLL bei aktueller Cargo-Toolchain

Seit Rust/Cargo 1.100 liegt das Build-Skript-Verzeichnis unter
`target/<profile>/build/<crate>/<hash>/out`, ältere Versionen nutzten
`build/<crate>-<hash>/out`. `scripts/copy-webview2-loader.ps1` prüft beide
Layouts. Der erste Lauf für `v0.11.5` scheiterte 2026-11-25 an genau diesem
Unterschied, bevor ein GitHub-Release entstand; der Eigentümer hat danach
freigegeben, den Tag auf den korrigierten Commit zu legen.

## Grenzen

- T-967 erzeugt **keinen** Tag, **kein** GitHub-Release und kein Deployment.
- `workflow_dispatch` auf einem Branch hat keinen Release-Tag; dort läuft nur
  der Testteil von `verify`, die Versionsprüfung wird übersprungen.
- Das Skript liest nur `Cargo.toml` und `Cargo.lock`; es prüft keine
  Git-Historie, keine Signatur und keine installierte Binary-Version.
- Die Versionsprüfung ist fail-closed: fehlende oder unlesbare Dateien,
  Mismatch und falsches Tag-Format führen zu einem nicht-null Exit-Code.
