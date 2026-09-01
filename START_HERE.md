# START_HERE — Einstieg für neue Entwickler (auch KI)

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
| 2 | `docs/WEB_UI_API_TOOL_RESET.md` | **Verbindlicher Umsetzungsplan** (Phasen 0–7) |
| 3 | `docs/WORK_CONTRACT.md` | **Arbeitsvertrag** — verbindlich für jeden, der eine Aufgabe übernimmt |
| 4 | `docs/TASKBOARD.md` | Aufgabentafel (Spiegel); Claim-Quelle ist `docs/TASKBOARD.json` |
| 5 | `docs/WEB_UI_API_TOOL_RESET_STATUS.md` | Aktueller Umsetzungsstand / Handover |
| 6 | `docs/CAPABILITY_MATRIX.json` | Beleg-Matrix (130 Zellen, Status je Fähigkeit) |

Außerhalb des Repos (nur lokal relevante Umgebung:
`C:\AGENTS.md` = Arbeitsdirektive mit den zwölf Direktiven; gilt für alle
Arbeiten unter `C:\Users`.)

> **Grundmodell (wichtig):** Dieses Repo hat **einen einzigen sichtbaren
> Branch für Arbeit: `master`.** Alle Docs, der Plan und der Codeleben dort.
> Wer klont, sieht alles — es gibt keine versteckte Wissens-Schiene.
> `archive/tui-ui` ist nur ein lesbares Archiv des alten TUI-Stands, kein
> Entwicklungszweig. Agents committen auf `master` (Gates grün), nie anderswo.
> Nur temporäre Nebenarbeiten (z. B. ein sauber zu isolierender Test) dürfen
> kurzfristig einen lokalen Branch verwenden, müssen aber vor der Abgabe in
> `master` landen.

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
   "branch": "feature/dein-zweig-oder-main",
   "claimed_at": "2026-09-02"
   ```
4. Arbeite auf eigenem Zweig vom `origin/feat/browser-inference-provider`
   (oder main), kleine Commits, Gates grün (Abschnitt 4).
5. Beim Abschluss: Belegpfad (Matrix-Zelle) eintragen, Statusdatei
   `docs/WEB_UI_API_TOOL_RESET_STATUS.md` aktualisieren, in der JSON Zelle
   auf `"done"` setzen und `done_at` ergänzen.

**Gerade offen für dich:** `T-102`, `T-103`, `T-104`, `T-201`, `T-202`,
`T-203`, `T-301`, `T-302`, `T-401`–`T-701` — freie Aufgaben ohne `owner`.

**Freie Aufgaben auf einen Blick (Stand heute):**

| ID | Phase | Aufgabe | Typ |
|---|---|---|---|
| T-102 | 1 | ToolRegistry + read/bash/edit/write mit Policy-Grenzen | code |
| T-103 | 1 | Fake-Brain (Textdelta/Toolloop/Abort/Retry/Exactly-once) | code |
| T-104 | 1 | Promptbuilder trennen (Reiner Chat vs. Managed Agent) | code |
| T-201 | 2 | Eingebettete Assets + Loopback-Serverstart | code |
| T-202 | 2 | Endpunkte: Session/Capability/Health/Upload/Chat/Stop/Event | code |
| T-701 | 7 | Gruppen-Modus (2–6 Brains, Runden, @Brain, Leader) | code |

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

Bekannter Stand: 1207 passed / 1 ignored (Stand 2026‑09‑01, vor dem
`session/`-Modul; aktuelle Zahl steht in der Statusdatei).

## 5. Nächste offene Scheiben

- **T-101** (läuft, `local/opencode`): `SessionService`+`EventStream`
  (`src/session/`) — Fundament steht, es fehlt die Api-Bridge-Anbindung.
- **T-102**, **T-103**, **T-104**, **T-201** … : freie Aufgaben für dich.
  Siehe `docs/TASKBOARD.md`.

## 6. Grenzen (nicht überschreiten)

- `C:\Users\storax\.zcode\v2\config.json` **nicht anfassen**.
- Keine Secrets/Tokens auslesen, kopieren oder committen.
- Keine Force-Pushes / History-Rewrites; Rücknahmen als neue Commits.
- Live-Claude-Web-Tests nur im Rahmen zulässiger Nutzung (Anthropic Consumer
  Terms) und nur nach Freigabe.
- Keine unbelegten `100 %`-Aussagen; Belege gehören in die Capability-Matrix.