# START_HERE — Einstieg für neue Entwickler (auch KI)

## AKTUELLER ARBEITSSTAND — 2026-09-13

Dieser Block ist vor dem historischen Projektkontext zu lesen und ist die
maßgebliche Einstiegslage für laufende Arbeit:

- **Referenz ist ausschließlich der GitHub-Branch `master`.**
- Aktueller dokumentierter Master-Stand: `a2175c8`.
- Vor jeder Arbeit: `git pull origin master`, `git status` und
  `git log -1 --oneline` ausführen. Eine lokale Arbeitskopie mit uncommitteten
  Änderungen ist kein Master-Stand und darf nicht als solcher ausgegeben werden.
- **Release `v0.11.3` veröffentlicht** (Windows/Linux/Android, `https://github.com/st0rax/webagent-rs/releases/tag/v0.11.3`).
- **Phase-8 (Brain-Vereinheitlichung T-801–T-808) vollständig done**: gemeinsamer
  Brain-Vertrag, einheitliches Senden, Antwortstream, Profil-Lease, Probe/Proof,
  Taskabschluss-Verifikation (E2E belegt), Run-Ledger/Crash-Recovery und
  Live-Abnahme inkl. Windows-Prozessproben. Die bisherigen Phasen sind
  abgeschlossen. Der aktuelle neue Bedarf ist in Phase 10 als T-907–T-913
  beschrieben; diese Tasks sind zunächst `free` und dürfen erst nach Claim
  bearbeitet werden.
- Der aktuelle Stand gilt als v1.0-Anwärter: Der technische Fahrplan steht in
  `docs/BRAIN_UNIFICATION_PLAN.md`; Live-Matrix-Belegen in
  `docs/CAPABILITY_MATRIX.json` (`as_of` 2026-09-12, 46 frische Verify-Belege).
- Historische Branch-, Build-, Binary- und Run-Angaben aus älteren Übergaben sind kein aktueller Beleg. Aktuelle Runs und Artefakte müssen im jeweiligen Run-Verzeichnis neu nachgewiesen werden.

Das Dokument `docs/HANDOVER_TO_CODEX_2026-08-25.md` ist historische Übergabe
und keine aktuelle Wahrheitsquelle. Für den Einstieg gelten diese Datei und
`docs/CURRENT_WORK.md`.

> **Der dauerhafte Einstieg.** Du bist neu im Repo (Mensch oder Agent wie
> ChatGPT-Codex, Claude Code, Grok, Manus)? Dann starte hier. Diese Datei
> verweist auf das, was du konkret ansehen und tun sollst.

## 1. Was ist dieses Repo?

Ein **lokaler, browserbasierter Agent** (Rust-Port) mit eigener Provider-Bridge:
Er nutzt echte Chat-Brains (ChatGPT, Claude, Gemini, …) über Browser-Sessions
und bietet eine lokale, OpenAI-kompatible API. Aktueller Umbau („3 Flächen"):
lokale **Web-UI**, **OpenAI-kompatibler Endpunkt**, **Managed Tools** — siehe
`docs/WEB_UI_API_TOOL_RESET.md`.

## 2. Pflicht-Lese (in dieser Reihenfolge)

| Schritt | Datei | Worum es geht |
|---|---|---|
| 1 | `AGENTS.md` | Repo-Regeln, Mapping zur Bot-Architektur |
| 2 | `GOALS.md` | **Nordstern (G-001): das Projekt soll fertig werden** — für ALLE Agents, Richtung = Mensch |
| 3 | `docs/WEB_UI_API_TOOL_RESET.md` | **Verbindlicher Umsetzungsplan** (Phasen 0–7) |
| 4 | `docs/WORK_CONTRACT.md` | **Arbeitsvertrag** — verbindlich für jeden, der eine Aufgabe übernimmt |
| 4 | `docs/TASKBOARD.md` | Aufgabentafel (Spiegel); Claim-Quelle ist `docs/TASKBOARD.json` |
| 4a | `docs/BRAIN_UNIFICATION_PLAN.md` | Aktueller gemeinsamer Brain-Vertrag, Reihenfolge, Abnahme und Befundgrenzen |
| 5 | `docs/WEB_UI_API_TOOL_RESET_STATUS.md` | Aktueller Umsetzungsstand / Handover |
| 6 | `docs/CAPABILITY_MATRIX.json` | Beleg-Matrix (130 Zellen, Status je Fähigkeit) |

Außerhalb des Repos (nur lokal relevante Umgebung:
`C:\AGENTS.md` = Arbeitsdirektive mit den zwölf Direktiven; gilt für alle
Arbeiten unter `C:\Users`.)

> **Grundmodell (wichtig):** `master` ist der **Stamm / `main`** und bleibt
> **immer grün** (baut + testet). Sichtbare Arbeit läuft **nicht direkt** auf
> dem Stamm, sondern auf **kurzen, klar benannten Arbeits-Zweigen** (Branches):
> `feature/<T-…>-<kurz>`, `fix/…`, `docs/…`, `chore/…`, `refactor/…`, `test/…`.
> Regel: nie am Ende einen „Riesen-Branch" pushen — sobald ein Zweig eine
> **grüne, abgeschlossene Einheit** hat, wird er **häufig & klein** in `master`
> gemergt. `archive/tui-ui` ist nur ein lesbares Archiv des alten TUI-Stands,
> kein Entwicklungszweig.
>
> **Arbeitsweise je Schritt:**
> 1. Auf `master`: `git pull` (aktuell), eigenen Zweig anlegen:
>    `git switch -c feature/T-102-tool-registry`
> 2. Kleine Commits mit eigener Identität (`scripts/commit-as-agent.ps1`),
>    Gates vor jedem Commit grün.
> 3. Sobald eine Einheit grün & in sich abgeschlossen ist: zurück zu `master`
>    (`git switch master`), Zweig in `master` mergen, prüfen, und `git push`.
>
> **Fachbegriffe:** Wer unsicher ist, welche git-Ausdrücke gelten, findet in
> `docs/GIT_GLOSSAR.md` die verbindliche Kurzliste (Stamm/Branch/Push/Merge)
> **inkl. Branch-Namensschema**.

## 3. So übernimmst du eine Aufgabe (Claim zuerst)

**Die Aufgabentafel liegt unter `docs/`:**

| Datei | Zweck |
|---|---|
| `docs/TASKBOARD.md` | menschenlesbare Tabelle zum Durchsehen |
| `docs/TASKBOARD.json` | **Claim-Quelle der Wahrheit** — hier setzt du dich ein |

**So trägst du dich ein — konkret:**

1. Lies `docs/WORK_CONTRACT.md` und akzeptiere ihn.
2. Wähle eine freie Aufgabe (Status `"free"`) aus `docs/TASKBOARD.md`.
3. Öffne `docs/TASKBOARD.json` und setze bei deiner Aufgabe (`id`, z. B.
   `"T-102"`):
   ```json
   "status": "claimed",
   "owner": "claude-code",
   "branch": "feature/T-102-tool-registry",
   "claimed_at": "2026-09-02"
   ```
4. Lege einen **kurzen, benannten Arbeits-Zweig** an (Namensschema in
   `docs/GIT_GLOSSAR.md`), arbeite dort mit kleinen Commits, Gates grün
   (Abschnitt 4). Jeder Agent committet mit **eigener Identität** — ein Enum:
   ```pwsh
   git switch -c feature/T-102-tool-registry
   pwsh -File scripts/commit-as-agent.ps1 -Agent claude-code -Message "T-102: tools registry"
   ```
   (Agent-Schlüssel & Mapping: `docs/GIT_AGENTS.md`.) Sobald eine grüne,
   abgeschlossene Einheit steht: zurück zu `master`, kleinen Merge, prüfen und
   `git push origin master`. Danach separat
   `git push origin master`.
5. Beim Abschluss: Belegpfad (Matrix-Zelle) eintragen, Statusdatei
   `docs/WEB_UI_API_TOOL_RESET_STATUS.md` aktualisieren, in der JSON Zelle
   auf `"done"` setzen und `done_at` ergänzen.

**Aktueller Stand:** Phase-8 (T-801–T-808) ist abgeschlossen. Die freien
Phase-10-Tasks T-907–T-912 sind eigenständige Vorarbeiten; T-913 ist bis zum
Abschluss dieser Vorgänger blockiert. Vor der Übernahme immer die aktuelle
JSON-Quelle lesen (Claim = Quelle der Wahrheit). Lokale OpenCode-Entwürfe sind
kein Claim und werden nicht parallel von anderen Agenten bearbeitet.

**Regel:** Ein Entwickler, eine Aufgabe. Niemand arbeitet ohne Claim.

## 4. Verifikationskommandos

```pwsh
# Default-Gate (webview-only, TUI hinter Feature)
cargo test --lib

# TUI baut weiterhin hinter seinem Feature
cargo check --features tui

# Ohne Defaultfeatures (CI-Zweig)
cargo check --no-default-features

# (optional) Binärgewicht im Release-Artefakt für das <10-MB-Budget
```

Bekannter Master-Stand: 1388 passed / 0 failed / 1 ignored (Stand 2026‑09‑12).
Nach jeder Änderung ist der Teststand im Übergabebeleg neu zu erfassen.

## 5. Verbleibende Arbeit

- Phase-8 abgeschlossen (T-801–T-808 done), Release v0.11.3 veröffentlicht.
- Offene Matrix-Grenzen sind dokumentierte Befunde (`failed`/`unreachable`/
  `removed`/`not_run` laut `docs/CAPABILITY_MATRIX.json`), kein offener Task.
- Refactoring-Bedarf: T-907–T-913 in `docs/TASKBOARD.json`.
- Neuer Bedarf wird als freie Aufgabe im TASKBOARD eingetragen und nach dem
  Claim-Verfahren umgesetzt.

## 6. Grenzen (nicht überschreiten)

- `C:\Users\storax\.zcode\v2\config.json` **nicht anfassen**.
- Keine Secrets/Tokens auslesen, kopieren oder committen.
- Keine Force-Pushes / History-Rewrites; Rücknahmen als neue Commits.
- Live-Claude-Web-Tests nur im Rahmen zulässiger Nutzung (Anthropic Consumer
  Terms) und nur nach Freigabe.
- Keine unbelegten `100 %`-Aussagen; Belege gehören in die Capability-Matrix.
