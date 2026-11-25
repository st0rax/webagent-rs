# Produktreife — Stand 25.11.2026

> **Referenz.** Bewertung des veröffentlichten Stands `v0.11.5`, keine neue
> Freigabe. Operative Wahrheit: `docs/CURRENT_WORK.md`.

**Urteil:** Release- und Installationsreife **ja**. Produktivreife im Sinne eines
geprüften Zielbetriebs **nicht vollständig belegt** — sie hängt an einem
definierten Ziel, einem Live-Smoke dort und der bewussten Akzeptanz der unten
genannten Restrisiken.

## Geprüft

| Punkt | Beleg |
|---|---|
| Versionsgate | `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.5` → Exit 0; `Cargo.toml` = `Cargo.lock` = `0.11.5`; Tag `v0.11.5` → `9b2ad7e` |
| Qualitätsgates | `cargo fmt --all -- --check`, `cargo build --locked`, `cargo test --lib --locked`, `cargo clippy --all-targets -- -D warnings`, `git diff --check` grün |
| CI/Release | PR [#104](https://github.com/st0rax/webagent-rs/pull/104) und [#105](https://github.com/st0rax/webagent-rs/pull/105) grün; Release-Workflow [`45684483569`](https://github.com/st0rax/webagent-rs/actions/runs/45684483569) vollständig grün inkl. `github-release`; 7 Assets, alle SHA-256 geprüft |
| Node-20-Warnungen | Dispatch-Lauf [`45690875482`](https://github.com/st0rax/webagent-rs/actions/runs/45690875482) grün, 0 Zeilen mit `Node.js 20`/`deprecated` im Log |
| T-961 | Zwei kausale Live-Läufe (`t961_fresh_chat_retries=1`, je 5/5 Tool-Calls, strict PASS), 14 strict-clean Läufe; Belege in `docs/proofs/T-961/semantic-denial-retry/` |
| Lokales Deployment | `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\` mit EXE, `WebView2Loader.dll` und `.sha256` aus dem Release; Hashes `a1baa187…562c` / `465a7ddf…0ea0a6f` gegen Release-Digests geprüft |
| Version / Runtime | `webagent.exe --version` → `webagent 0.11.5 (9b2ad7e6e)`, Exit 0 |
| Runtime-Smoke | `webagent.exe ui --no-open --skip-startup-reconcile --port 8788`: Port nach 1 s Listen, `GET /` → 200 (38358 Byte), `GET /health` → 200 (31 Byte); Prozess gestoppt, Port danach frei |
| Offline-Preflight | `webagent.exe brains-health` → Exit 0, `shared_browser=false`, `shared_profile=ok`, 8/8 Brains `selectors=ok` (einschließlich eingebetteter Kopien); `bot2bot=MISSING` (informativ) |

## Bekannte Grenzen / Restrisiken

1. **Canary-Fehlalarm im ausgelieferten `v0.11.5`.** `webagent.exe canary` meldet 7/8 `missing_selectors`, weil die leichte Probe nur die Datei am Build-Pfad prüft. Das eigentliche `load_selectors` fällt auf eingebettete Selektoren zurück (`src/config/selectors.rs`), `brains-health` zählt sie korrekt mit. Die Korrektur in `src/canary.rs` wird mit diesem Scheiben nach `master` gemergt und ist **nicht** im veröffentlichten Binary.
2. **Publish-Pfad mit neuen Actions nicht erneut ausgeführt.** `github-release` läuft nur bei `v*`; der Dispatch auf `master` hat ihn übersprungen. Die Inputs (`generate_release_notes`, `files`, `merge-multiple`, `path`) wurden statisch gegen die `action.yml` der neuen Versionen geprüft. Der nächste Tag wird es real ausführen.
3. **Kein WebView2-Fensterstart im Release-Pfad.** Der Smoke testet den UI-/Health-Listener des deployten Binary, nicht das Öffnen eines echten Browserfensters oder einen Live-Browser-Relay-Turn.
4. **Kein Produktivziel.** Es existiert kein `delivery/deploy_webagent_rs.ps1`, kein `runtime-workers/` und kein Fernziel im Workspace. Deployt wurde nur der lokale Ordner aus der Tabelle; kein Monitoring, keine Last-/Parallelitätstests, kein Prod-Rollback-Konzept.
5. **T-961 ist eng begrenzt.** Die Ursache des Providerverhaltens ist nicht isoliert; vorzeitiger Textabschluss und UI-Sendefehler mit Circuit-Breaker sind nicht adressiert. Die Live-Belege nutzten die bestehende chatgpt-Sitzung ohne neue Anmeldung.
6. **`bot2bot=MISSING`** im Preflight; für den hier geprüften Webagent-Pfad nicht blockerrelevant, aber für den Worker-Betrieb vorab zu klären.

## Go/No-Go für einen Produktivbetrieb

**Go**, wenn ein konkretes Ziel mit Monitoring und Rückweg benannt ist, dort ein Live-Smoke (UI + ein echter Brain-Turn) durchgeführt wurde und die Punkte 1–6 als Restrisiko schriftlich akzeptiert sind.

**No-Go**, solange das Produktivziel fehlt oder ein Live-Smoke im Zielbetrieb nicht vorliegt. Die technische Auslieferungsreife ist davon unabhängig gegeben.

## Rückbau des lokalen Deployments

```powershell
Remove-Item -LiteralPath 'C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release' -Recurse -Force
```

Sonst wurden keine Systemänderungen vorgenommen: keine PATH-Änderung, kein dauerhaft laufender Prozess, keine Zugangsdaten gelesen oder geschrieben.
