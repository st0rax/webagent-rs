# START_HERE — tu genau das

> Du bist neu (Mensch oder Agent). **Diese Datei ist die Handlungsanweisung.**
> Andere Docs vertiefen; bei Widerspruch gilt **diese Datei**, außer eine
> **Schutzregel** (Direktiven 6–8, Geheimnisse, Freigabe) ist strenger.
> Die zwölf Direktiven (§1) gelten ab der ersten Aktion — nicht erst nach dem Claim.
> Nicht raten. Nicht die Kette überspringen. Nicht ohne Claim arbeiten.

## 0. In 30 Sekunden

1. `git pull origin master`
2. `docs/TASKBOARD.json` öffnen (nicht die Markdown als Claim-Quelle)
3. **Eine** Zelle mit `"status": "free"` nehmen, in der **du** in `"suitable"` stehst — außer der Mensch hat dir eine `id` genannt
4. Claim **in der JSON** schreiben, dann erst Code
5. Kurzer Zweig `feature/T-<id>-<kurz>`, Gates grün, klein nach `master` mergen, **einmal** pushen

Ziel (G-001): das Projekt **fertig** — lokale Web-UI + OpenAI-kompatible API + Managed Tools. Richtung und Tempo bestimmt der **Mensch**. Du lieferst eine Zelle.

Leitbild: **erst Resonanz, dann Aktion; ambitioniert denken, sauber prüfen, hartnäckig bleiben.**

## 1. Die zwölf Direktiven (verbindlich)

Dieselben Gebote wie `C:\AGENTS.md` und Repo-`AGENTS.md`. Vor jeder Arbeit. Bei Widerspruch mit einer späteren Datei gilt die **strengere Schutzregel**. Projektspezifisches (Claim, Zweige, Tabus) darf sie nicht aufweichen.

| Nr. | Direktive |
|---:|---|
| 1 | **Klären vor Beschleunigen.** Richtung, Ton und Prioritäten stehen, bevor du umfangreich loslegst. |
| 2 | **„Geht nicht“ ist ein Zwischenstand.** Alternativen, Hilfsmittel, Zwischenschritte und vertretbaren Aufwand prüfen. |
| 3 | **Antizipiere die dritte Stufe.** Neben dem nächsten Schritt: Folgen, Abhängigkeiten, Risiken, Rückweg. |
| 4 | **Die Werkbank darf benutzt werden.** Lokale Dateien, Software, Netz — wenn sie dem Auftrag dienen. |
| 5 | **Sicher und reversibel zuerst.** Nachvollziehbar, testbar, rückgängig machbar; Zwischenstände halten. |
| 6 | **Nicht blind herumschrauben.** Löschen, Sicherheit, Kosten, Veröffentlichung, Rechte, rechtliche Bindung: vorher Freigabe. |
| 7 | **Zugänge dienen nur dem Auftrag.** Logins für die Aufgabe ja. Passwörter, Tokens, Geheimnisse nicht lesen, kopieren, ausgeben. |
| 8 | **Kostenfreie Tests sind möglich.** Kostenlose Testkonten mit klarem Zweck. Zahlung, Probeabo mit Kostenrisiko, Ausweis, OTP, CAPTCHA, Mail-Verify: Übergabe an den Menschen. |
| 9 | **GitHub ist ein Dev-Log.** Kleine Commits, regelmäßig pushen. Keine Secrets, keine unnötigen Binaries. Kein Force-Push, keine History-Umschreibung. Rücknahme = neuer Commit. |
| 10 | **Rückfragen sind der letzte Ausweg.** Erst suchen, lokal prüfen, Alternative. Fragen nur, wenn Entscheidung, Geheimnis oder Freigabe **nur** vom Menschen kommen kann. |
| 11 | **Klarheit vor Show.** Getrennt: geprüft / wahrscheinlich / unklar / blockiert. Grenzen nicht verschleiern. |
| 12 | **MacGyver-Geist mit Rückwärtsgang.** Unkonventionell ja — mit Inventar, nachvollziehbarem Weg und Kurskorrektur. |

Praktisch: interne, sichere Arbeit selbst tun. Unklares, das du prüfen kannst, erst selbst prüfen. Außenwirkung: Vorschlag (Ziel, Umfang, Folge), dann Freigabe.

## 2. Wer du bist

Beim Commit und im Claim nur diese Schlüssel:

| Du kommst als | `owner` / `-Agent` |
|---|---|
| Grok | `grok-agent` |
| ChatGPT / Codex | `chatgpt-codex` |
| Claude | `claude-code` |
| Manus | `manus` |
| opencode / lokal | `opencode` |

Fremde Identität = Regelverstoß. Mapping: `docs/GIT_AGENTS.md`.

## 3. Welche Aufgabe (nicht raten)

Quelle der Wahrheit: **`docs/TASKBOARD.json`**. `docs/TASKBOARD.md` ist nur Spiegel.

Eine Aufgabe ist für dich erlaubt, wenn **alle** gelten:

- `"status": "free"` (oder der Mensch hat sie dir zugewiesen)
- deine `owner`-Id steht in `"suitable"` **oder** der Mensch hat die `id` ausdrücklich genannt
- du hast **keine** zweite Zelle offen
- du brichst die Phasenreihe nicht: nimm die **nächste freie Zelle der kleinsten Phase**, nicht T-701 weil sie in einer Tabelle stand

**Aktuell (Phase 2, Stand nach T-104):**

| ID | Was | `suitable` |
|---|---|---|
| T-201 | Eingebettete Assets + Loopback-Server | `claude-code`, `manus` |
| T-202 | HTTP-Endpunkte Session/Health/Chat/… | `chatgpt-codex`, `claude-code` |
| T-203 | Klickbarer Fake-Prototyp (Grok-Layout, A11y) | `claude-code`, `manus`, `local/opencode` |

Stehst du nicht in `suitable` und hat der Mensch keine `id` genannt: **nicht arbeiten**. Sag das. Erfinde keine Zelle.

T-101–T-104 sind `done`. Live-Zellen (T-301+) brauchen den Menschen (Login/Freigabe) — nicht von allein anfangen.

## 4. Claim zuerst

In **derselben** JSON-Zelle:

```json
"status": "claimed",
"owner": "grok-agent",
"branch": "feature/T-202-api-endpoints",
"claimed_at": "2026-09-02"
```

Ohne diesen Eintrag gilt die Übernahme nicht — auch nicht, wenn du im Chat „ich mach T-202“ gesagt hast. Vor dem Claim: `git pull`, JSON nochmal lesen (kein Lock; wer zuerst committed/pushed, hat die Zelle).

Vertrag akzeptieren: `docs/WORK_CONTRACT.md`.

## 5. Git (ein Modell)

`master` ist der Stamm. Arbeit **nicht** direkt auf `master`.

```pwsh
git pull origin master
git switch -c feature/T-202-api-endpoints
# ... Arbeit ...
cargo fmt --all -- --check
cargo test --lib
pwsh -File scripts/commit-as-agent.ps1 -Agent grok-agent -Message "T-202: session health endpoints"
git switch master
git pull origin master
git merge --ff-only feature/T-202-api-endpoints   # wenn ff nicht geht: merge ohne History umzuschreiben
git push origin master
```

- Zweigname: `feature|fix|docs|chore|refactor|test/` + Kurzname, oft `T-<id>-…` (`docs/GIT_GLOSSAR.md`)
- `scripts/commit-as-agent.ps1` committet auf `master` **oder** so einem Zweig
- **Ein** Push der fertigen Einheit nach `master`. Kein Force-Push, keine History-Umschreibung
- `archive/tui-ui` ist Archiv, kein Arbeitszweig

Wenn CI auf `master` rot ist (Formatcheck ist ein bekanntes Gate): zuerst `cargo fmt` / die rote Ursache, nicht die nächste Feature-Zelle auf einem roten Stamm stapeln — außer der Mensch sagt etwas anderes.

## 6. Fertig = DoD, nicht „sieht gut aus“

Vor `"status": "done"`:

1. Verifikationsbefehl der Zelle (meist `cargo test --lib`) **und** `cargo fmt --all -- --check`
2. `proof_path` in der JSON-Zelle
3. `docs/WEB_UI_API_TOOL_RESET_STATUS.md` an den **aktuellen** HEAD anpassen (Commit-SHA nicht liegen lassen)
4. `"status": "done"`, `"done_at"` setzen
5. Working Tree sauber, Push

Keine unbelegten `100 %`. Belege: `docs/CAPABILITY_MATRIX.json`.

## 7. Grenzen

- `C:\Users\storax\.zcode\v2\config.json` nicht anfassen
- Keine Secrets/Tokens lesen, kopieren, committen
- Keine Außenwirkung (Release, Tag, Rechte, Bezahltes) ohne Freigabe
- Live-Claude-Web nur nach Freigabe (Anthropic Consumer Terms)
- Parallel: eine Zelle, ein Agent; sonst getrennte Worktrees und disjunkte Dateien (`AGENTS.md`)

## 8. Nach dem Ablauf lesen (nicht vorher 40 Seiten)

Erst Abschnitt 0–7 **tun** (Direktiven in §1 gelten ab der ersten Aktion). Dann, falls nötig:

| Datei | Wozu |
|---|---|
| `AGENTS.md` | Projektspezifisches (Bestand vor neuen Modulen, TUI, Beleg-Gate) — die Zwölf stehen schon hier |
| `GOALS.md` | Nordstern G-001 |
| `docs/WORK_CONTRACT.md` | Claim, Inspektor, DoD |
| `docs/WEB_UI_API_TOOL_RESET.md` | Plan Phasen 0–7 |
| `docs/WEB_UI_API_TOOL_RESET_STATUS.md` | Handover (kann hinter der JSON liegen — JSON gewinnt für Claims) |
| `docs/CAPABILITY_MATRIX.json` | Belegzellen |

Lokal: `git status --short --branch` vor dem ersten Edit. Unbekannte Änderungen nicht überschreiben.
