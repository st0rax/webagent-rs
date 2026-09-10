# START_HERE — Einstieg für neue Entwickler (auch KI)

## AKTUELLER ARBEITSSTAND — 2026-09-10

Dieser Block ist vor dem historischen Projektkontext zu lesen und ist die
maßgebliche Einstiegslage für laufende Arbeit:

- Checkout: `C:\Users\storax\Documents\Codex\2026-08-27\roadmap-zusammenfassen-chatgpt-conversation-6a90695d-b1e4\work\webagent-github-audit-20260829`
- Branch dieses Dokumentationsstands: `feature/T-801-brain-contract`
- HEAD dieses Dokumentationsstands: siehe `git log -1`; die Arbeitskopie muss vor jeder Implementierung erneut geprüft werden.
- Der aktuelle technische Fahrplan steht in `docs/BRAIN_UNIFICATION_PLAN.md`. Er vereinheitlicht Vertrag, Senden, Antwortstream, Profil-Lease, Probe/Proof und Taskabschluss für alle Brains.
- Umsetzungsstand laut `docs/CURRENT_WORK.md`: **T-801 (Scheibe 1, Brain-Vertrag) und T-802 (Scheibe 2, gemeinsames Senden ohne Doppelversand) sind als Code umgesetzt**; T-802 ist im `docs/TASKBOARD.json` `done` (Beleg `docs/proofs/T-802/RESULT.md`). Als naechste umsetzbare Scheiben bieten sich **T-803** (Antwortstream) und **T-804** (Profil-Lease) an; genau eine davon ist vor Aenderungen zu claimen.
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

**Aktuelle Vereinheitlichung:** Beginne mit `T-801` aus
`docs/BRAIN_UNIFICATION_PLAN.md` oder beginne unabhängig mit `T-808` für das
Run-Ledger. Die Aufgaben `T-802`–`T-807` sind durch Abhängigkeiten gesperrt,
bis ihre Vorgänger abgeschlossen sind. Ältere freie
Aufgaben dürfen nur nach erneutem Lesen der aktuellen JSON-Quelle übernommen
werden.

**Neue freie Aufgaben auf einen Blick:**

| ID | Phase | Aufgabe | Typ |
|---|---|---|---|
| T-801 | 8 | Gemeinsamer Brain-Vertrag und Konformitätsfixtures | code |
| T-808 | 8 | Dauerhaftes Run-Ledger und Crash-Recovery | code |
| T-802–T-807 | 8 | Vereinheitlichung, Proof und Live-Abnahme; abhängig | code |

Vollständige Liste: `docs/TASKBOARD.md`.

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

Bekannter Stand: 1238 passed / 1 ignored (Stand 2026‑09‑02).

## 5. Nächste offene Scheiben

- **T-101** und **T-102** sind erledigt (`local/opencode`).
- **T-103** und **T-104** sind erledigt (`chatgpt-codex`); **T-201** … bleibt frei.
  Siehe `docs/TASKBOARD.md`.

## 6. Grenzen (nicht überschreiten)

- `C:\Users\storax\.zcode\v2\config.json` **nicht anfassen**.
- Keine Secrets/Tokens auslesen, kopieren oder committen.
- Keine Force-Pushes / History-Rewrites; Rücknahmen als neue Commits.
- Live-Claude-Web-Tests nur im Rahmen zulässiger Nutzung (Anthropic Consumer
  Terms) und nur nach Freigabe.
- Keine unbelegten `100 %`-Aussagen; Belege gehören in die Capability-Matrix.
