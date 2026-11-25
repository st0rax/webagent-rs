# Produktreife — Stand 25.11.2026

> **Referenz.** Bewertung des veröffentlichten Stands `v0.11.6`, keine neue
> Freigabe. Operative Wahrheit: `docs/CURRENT_WORK.md`.

**Urteil:** Release- und Installationsreife **ja**. Produktivreife im Sinne eines
geprüften Zielbetriebs **nicht vollständig belegt** — sie hängt an einem
definierten Ziel, einem Live-Smoke dort und der bewussten Akzeptanz der unten
genannten Restrisiken.

## Geprüft

| Punkt | Beleg |
|---|---|
| Versionsgate | `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.6` → Exit 0; `Cargo.toml` = `Cargo.lock` = `0.11.6`; gegen `v0.11.5` → Exit 1 (Mismatch wie erwartet); Tag `v0.11.6` → `1149dff` |
| Qualitätsgates | `cargo fmt --all -- --check`, `cargo build --locked`, `cargo test --lib --locked`, `cargo clippy --all-targets -- -D warnings`, `git diff --check` grün |
| CI/Release | PR [#104](https://github.com/st0rax/webagent-rs/pull/104), [#105](https://github.com/st0rax/webagent-rs/pull/105), [#106](https://github.com/st0rax/webagent-rs/pull/106) und [#107](https://github.com/st0rax/webagent-rs/pull/107) grün; Release-Workflow [`45702611313`](https://github.com/st0rax/webagent-rs/actions/runs/45702611313) vollständig grün inkl. `github-release` (sieben Assets, alle SHA-256 gegen Release-Digests und `.sha256`-Dateien geprüft) |
| Publish-Pfad mit neuen Actions | Real ausgeführt, nicht nur statisch geprüft: `download-artifact@v8` + `action-gh-release@v3` im Publish-Job des Laufs `45702611313` grün. Keine `Node.js 20`-Action-Warnung; eine interne `Buffer()`-Deprecation des `download-artifact@v8`-Node-Runtime (siehe Restrisiko 4) |
| Node-20-Warnungen | Dispatch-Lauf [`45690875482`](https://github.com/st0rax/webagent-rs/actions/runs/45690875482) grün, 0 Treffer auf `Node.js 20`/`deprecated`; Release-Lauf `45702611313` nur die eine interne `Buffer()`-Meldung |
| T-961 | Zwei kausale Live-Läufe (`t961_fresh_chat_retries=1`, je 5/5 Tool-Calls, strict PASS), 14 strict-clean Läufe; Belege in `docs/proofs/T-961/semantic-denial-retry/` |
| Lokales Deployment | `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\` mit EXE, `WebView2Loader.dll` und `.sha256` aus `v0.11.6`; Hashes `568c5001…f8d5d3` / `465a7ddf…10ea0a6f` geprüft. Die `v0.11.5`-Dateien liegen als Rollback-Sicherung unter `v0.11.5-backup\` |
| Version / Runtime | `webagent.exe --version` → `webagent 0.11.6 (1149dff2f)`, Exit 0 |
| Runtime-Smoke | `webagent.exe ui --no-open --skip-startup-reconcile --port 8788`: Port nach 1 s Listen, `GET /` → 200 (38358 Byte), `GET /health` → 200 (31 Byte); Prozess gestoppt, Port danach frei |
| Offline-Preflight | `webagent.exe canary` → Exit 0, 8/8 `ok` (vor `v0.11.6`: 7/8 Fehlalarm `missing_selectors`); `webagent.exe brains-health` → Exit 0, `shared_profile=ok`, 8/8 `selectors=ok`; `bot2bot=MISSING` (informativ) |

## Bekannte Grenzen / Restrisiken

1. **Kein WebView2-Fensterstart im Release-Pfad.** Der Smoke testet den UI-/Health-Listener des deployten Binary, nicht das Öffnen eines echten Browserfensters oder einen Live-Browser-Relay-Turn.
2. **Kein Produktivziel.** Es existiert kein `delivery/deploy_webagent_rs.ps1`, kein `runtime-workers/` und kein Fernziel im Workspace (auch keine passenden Dienste, geplanten Tasks oder weiteren Laufwerke). Deployt wurde nur der lokale Ordner aus der Tabelle; kein Monitoring, keine Last-/Parallelitätstests, kein Prod-Rollback-Konzept.
3. **T-961 ist eng begrenzt.** Die Ursache des Providerverhaltens ist nicht isoliert; vorzeitiger Textabschluss und UI-Sendefehler mit Circuit-Breaker sind nicht adressiert. Die Live-Belege nutzten die bestehende chatgpt-Sitzung ohne neue Anmeldung.
4. **Interne Deprecation im Publish-Job.** `download-artifact@v8` loggte 2026-11-25 eine `[DEP0005] Buffer() is deprecated`-Meldung aus seinem Node-Runtime. Der Job war grün und das Artefakt vollständig; es ist keine Node-20-Action-Warnung, aber ein Hinweis auf künftige Action-Updates.
5. **`bot2bot=MISSING`** im Preflight; für den hier geprüften Webagent-Pfad nicht blockerrelevant, aber für den Worker-Betrieb vorab zu klären.

## Go/No-Go für einen Produktivbetrieb

**Go**, wenn ein konkretes Ziel mit Monitoring und Rückweg benannt ist, dort ein Live-Smoke (UI + ein echter Brain-Turn) durchgeführt wurde und die Punkte 1–5 als Restrisiko schriftlich akzeptiert sind.

**No-Go**, solange das Produktivziel fehlt oder ein Live-Smoke im Zielbetrieb nicht vorliegt. Die technische Auslieferungsreife ist davon unabhängig gegeben.

## Rückbau des lokalen Deployments

Zur Wiederherstellung des vorherigen Stands die gesicherten Dateien zurückkopieren:

```powershell
$target = 'C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release'
Copy-Item -LiteralPath "$target\v0.11.5-backup\*" -Destination $target -Force
```

Oder den Zielordner vollständig entfernen:

```powershell
Remove-Item -LiteralPath 'C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release' -Recurse -Force
```

Sonst wurden keine Systemänderungen vorgenommen: keine PATH-Änderung, kein dauerhaft laufender Prozess, keine Zugangsdaten gelesen oder geschrieben.
