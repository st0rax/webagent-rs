# Produktreife — historischer Beleg Stand 26.11.2026

> **Referenz.** Bewertung des damals veröffentlichten und lokal ausgelieferten
> Stands `v0.11.7`, keine neue Freigabe. Die Tabelle unten wurde nicht auf den
> aktuellen Stand (`v0.11.10`, `origin/master` `a916039`) neu bewertet;
> operative Wahrheit: `docs/CURRENT_WORK.md`. Das lokale Deployment läuft
> inzwischen mit `v0.11.10`; Beleg:
> `docs/proofs/release-v0.11.10-2026-11-27/README.md`.

**Urteil:** Release- und Installationsreife **ja**. Der definierte Zielbetrieb
auf diesem Windows-Rechner (Browser-Chat als Gehirn, lokale Provider-Rolle für
Pi/OpenCode) ist mit einem echten WebView2-Live-Turn **belegt**. Ein
Fern-/Produktivbetrieb mit Monitoring, Last und langfristiger Beobachtung bleibt
**nicht** belegt und ist nicht Gegenstand dieser Auslieferung.

## Geprüft

| Punkt | Beleg |
|---|---|
| Versionsgate | `pwsh -File scripts/verify-release-version.ps1 -Tag v0.11.7` → Exit 0; `Cargo.toml` = `Cargo.lock` = `0.11.7`; gegen `v0.11.6` → Exit 1 (Mismatch erwartet); Tag `v0.11.7` → `d54d347` |
| Qualitätsgates | `cargo fmt --all -- --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --lib --locked` (1521 passed / 0 failed / 1 ignored), `git diff --check` grün |
| CI/Release | PR [#108](https://github.com/st0rax/webagent-rs/pull/108)–[#112](https://github.com/st0rax/webagent-rs/pull/112) geprüft; #109–#111 wurden wegen Doku-Konflikten lokal gesquasht und geschlossen. Release-Workflow [`45725669798`](https://github.com/st0rax/webagent-rs/actions/runs/45725669798) vollständig grün inkl. `github-release`; sieben Assets, alle Asset-Digests und die `.sha256`-Datei geprüft; Release nicht Draft/Prerelease |
| Publish-Pfad mit neuen Actions | Real ausgeführt: `download-artifact@v8` + `action-gh-release@v3` im Publish-Job des Laufs `45725669798` grün. Keine `Node.js 20`-Action-Warnung; eine interne `[DEP0005] Buffer()`-Meldung des Node-Runtime (siehe Restrisiko 4) |
| Lokales Deployment | `C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release\` mit EXE (SHA-256 `0f45d963…d973e84`), `WebView2Loader.dll` (`465a7ddf…10ea0a6f`) und `.sha256` aus `v0.11.7`; Hashes nach dem Kopieren erneut geprüft. `v0.11.6-backup\` und `v0.11.5-backup\` sind der Rückweg |
| Version / Runtime | `webagent.exe --version` → `webagent 0.11.7 (d54d34755)`, Exit 0 |
| Provider-Rolle | `api serve --brain chatgpt --headless --port 8788` mit prozessonly Token: `GET /health` → 200, `GET /v1/models` mit Bearer → 9 Modelle (erste `webagent/auto`), ohne Bearer → 401; `POST /v1/chat/completions` → `PROVIDER-OK-20261126`, `finish_reason=stop`. Prozess gestoppt, Port 8788 frei; Beleg `docs/proofs/live-turn-2026-11-26/provider-chat-turn.md` |
| Echter WebView2-Live-Turn | deployed `v0.11.7`, sichtbares Fenster (ohne `--headless`), `cloud web-view-stream --brain chatgpt --net-trace`, Exit 0, Token exakt `LIVE-OK-20261126-V0.11.7`, `oauth_session=true`, `api_key=false`, 200-SSE-`provider_response` (602 ms). Rohbeleg und Grenzen: `docs/proofs/live-turn-2026-11-26/` |
| Offline-Preflight | `webagent.exe canary` → Exit 0, 8/8 `ok`; `webagent.exe brains-health` → Exit 0, `shared_profile=ok`, 8/8 `selectors=ok`; `bot2bot=MISSING` (informativ) |

## Bekannte Grenzen / Restrisiken

1. **Ein Turn, ein Brain, eine Sitzung.** Der Live-Turn belegt einen einzelnen
   chatgpt-Turn mit bereits vorhandener Anmeldung. Kein Last-, Dauer- oder
   Parallelitätstest, keine Häufigkeitsaussage. Vorzeitiger Textabschluss und
   `ABSENDEKNOPF_DEAKTIVIERT` bleiben bekannte andere Fehlerklassen.
2. **Kein Fern-/Produktivziel.** Deployt wurde ausschließlich der lokale Ordner
   auf diesem Rechner. Kein Monitoring, kein Alarm, kein remotely erreichbarer
   Dienst, kein langfristiges Rollback-Konzept jenseits der lokalen Backups.
   Die Provider-Rolle bindet bewusst nur an Loopback und verlangt einen Token.
3. **T-961 ist eng begrenzt.** Die Ursache des Providerverhaltens ist nicht
   isoliert; die Retry-Endabnahme stützt sich auf 14 strict-clean Läufe und
   zwei kausale Verweigerungs-Läufe, nicht auf den instabilen
   `--auto-tool-choice-required`-Schalter.
4. **Interne Deprecation im Publish-Job.** `download-artifact@v8` loggte auch
   2026-11-26 eine `[DEP0005] Buffer() is deprecated`-Meldung aus seinem
   Node-Runtime. Der Job war grün und das Artefakt vollständig; es ist keine
   Node-20-Action-Warnung.
5. **`bot2bot=MISSING`** im Preflight; für den Browser-Chat-/Provider-Pfad nicht
   blockerrelevant, aber für den Worker-Betrieb vorab zu klären.

## Go/No-Go

**Go** für den hier definierten lokalen Zielbetrieb: konkretes Ziel benannt
(dieser Windows-Rechner), Version und Hashes geprüft, UI-/Provider-Smoke grün,
echter WebView2-Turn mit vorhandener Sitzung belegt, Rückweg vorhanden.

**No-Go** für einen Fern-/Produktivbetrieb weiterhin, weil Monitoring und
Lasttests fehlen und kein konkretes Fernziel mit Zugangsweg definiert ist.
Eine allgemeine schriftliche Freigabe für weitere Operationen hat der
Eigentuemer am 27.11.2026 erteilt (`docs/CURRENT_WORK.md`); sie ersetzt
weder die technischen Nachweise noch die Dokumentation des konkreten Einsatzes.
Die technische Auslieferungsreife ist davon unabhängig gegeben.

## Rückbau des lokalen Deployments

Zur Wiederherstellung des vorherigen Stands:

```powershell
$target = 'C:\Users\storax\Documents\Standardprojekt\webagent-deploy-target\release'
Copy-Item -LiteralPath "$target\v0.11.6-backup\*" -Destination $target -Force
```

Oder eine ältere Sicherung (`v0.11.5-backup`) bzw. den gesamten Zielordner
verwenden. Sonst wurden keine Systemänderungen vorgenommen: keine
PATH-Änderung, kein dauerhaft laufender Prozess, keine Zugangsdaten gelesen oder
geschrieben.
