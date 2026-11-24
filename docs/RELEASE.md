> **Referenz.** Verbindliche Reihenfolge für Release-Tags. Operativer Stand:
> `docs/CURRENT_WORK.md`. Kein Tag und kein GitHub-Release durch T-967.

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
   `Cargo.lock` auf dieselbe nächste Patch-Version setzen (aktuell `0.11.5`).
2. **Lokal prüfen:**
   `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` → Exit 0.
   Gegen `v0.11.4` muss das Skript Exit 1 liefern, gegen `0.11.5` Exit 2.
3. **Gates:** `cargo fmt --all -- --check`,
   `cargo clippy --all-targets -- -D warnings`, `cargo test --lib`,
   `git diff --check`.
4. **Merge:** den geprüften Stand nach `master` mergen. Erst danach taggen.
5. **Tag:** `vX.Y.Z` ausschließlich auf dem gemergten Commit setzen, dessen
   Manifest und Lockfile genau diese Version tragen.
6. **Publish:** der Workflow `release.yml` führt zuerst den `verify`-Job aus
   (Versionsprüfung für Tag-Refs plus `cargo test --lib --locked`); alle Build-
   und Publish-Jobs hängen davon ab. ohne grünen `verify` kein Artefakt.

## Grenzen

- T-967 erzeugt **keinen** Tag, **kein** GitHub-Release und kein Deployment.
- `workflow_dispatch` auf einem Branch hat keinen Release-Tag; dort läuft nur
  der Testteil von `verify`, die Versionsprüfung wird übersprungen.
- Das Skript liest nur `Cargo.toml` und `Cargo.lock`; es prüft keine
  Git-Historie, keine Signatur und keine installierte Binary-Version.
- Die Versionsprüfung ist fail-closed: fehlende oder unlesbare Dateien,
  Mismatch und falsches Tag-Format führen zu einem nicht-null Exit-Code.
