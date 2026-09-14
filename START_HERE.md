# START_HERE — verbindlicher Einstieg (Mensch und Agent)

> Diese Datei ist **stabil**. Sie enthält keine Phasen-, Task- oder
> Versionsstände, weil die veralten und dann falsch anleiten. Der aktuelle
> Stand steht ausschließlich in `docs/CURRENT_WORK.md` (Prosa) und
> `docs/TASKBOARD.json` (maschinenlesbar). Widersprechen sich Dokumente,
> gewinnt die JSON.

---

## 0. Wo bin ich? — zuerst beantworten

Dieses Repo ist **in viele Arbeitsverzeichnisse gleichzeitig ausgecheckt**.
Sie sehen alle wie „das Repo" aus, stehen aber auf verschiedenen Branches.
Wer das nicht prüft, arbeitet im falschen Baum oder baut aus dem falschen
Stand. Das ist der häufigste Orientierungsfehler hier.

```bash
git rev-parse --show-toplevel     # in welchem Baum stehe ich?
git branch --show-current         # auf welchem Branch?
git worktree list                 # welche Bäume gibt es sonst, auf welchem Branch?
```

Drei Regeln dazu:

1. **`master` ist die einzige Referenz.** Ein anderer Baum ist Arbeitsstand,
   kein Wahrheitsstand — auch wenn dort etwas funktioniert.
2. **Funktioniert etwas nur in einem Nebenbaum, ist es nicht im Produkt.**
   Ein Branch, der nicht in `master` gemergt ist, verschwindet beim nächsten
   Build aus `master` spurlos. Niemand bemerkt es an der Quelle.
3. **Ein Binary weiß nicht, woher es kommt.** Wer ein `webagent.exe`
   vorfindet, kann ihm nicht ansehen, aus welchem Baum und Stand es gebaut
   wurde. Vor jeder Schlussfolgerung über Laufzeitverhalten prüfen:

```bash
stat -c '%y %n' target/debug/webagent.exe src/<geänderte Datei>   # Binary älter als Quelle?
grep -qa "<neuer Funktionsname>" target/debug/webagent.exe        # Symbol wirklich drin?
```

Kommentare und Zeichenketten aus Kommentaren stehen **nicht** im Binary; als
Probe taugen nur Namen, die der Compiler ausgibt (Funktionen, Typen). Eine
Gegenprobe mit einem Symbol, das vorher schon existierte, gehört dazu.

---

## 1. Was ist dieses Repo?

Ein lokaler, browserbasierter Agent (Rust): echte Chat-Sitzungen im
eingebetteten Browser sind das „Brain", die lokale Shell führt aus. Darüber
liegt eine **lokale, OpenAI-kompatible API** (`webagent api serve`), damit
beliebige Harnesses die angemeldeten Chats wie eine API ansprechen können.

---

## 2. Pflichtlese, in dieser Reihenfolge

| # | Datei | Worum es geht |
|---|---|---|
| 1 | `AGENTS.md` | Repo-Regeln, verbindlich |
| 2 | `GOALS.md` | Nordstern G-001, Richtung bestimmt der Mensch |
| 3 | `docs/WORK_CONTRACT.md` | Arbeitsvertrag für jeden, der eine Aufgabe übernimmt |
| 4 | `docs/CURRENT_WORK.md` | aktueller Stand und nächste sichere Aktion |
| 5 | `docs/TASKBOARD.json` | Claim-Quelle der Wahrheit |
| 6 | `docs/GIT_GLOSSAR.md` | Branch-Namensschema und git-Begriffe |

`docs/TASKBOARD.md` ist ein **Spiegel** zum Durchsehen, keine Quelle.
Dateien mit `*_PLAN.md`, `*_CONCEPT.md`, `PROGRESS.md`, `STATUS_LIVE.md` und
datierte Übergaben sind **Log oder Entwurf**, kein Soll-Zustand.

---

## 3. Eine Aufgabe übernehmen

Ein Task, ein Branch, ein Scope. Nichts davon ist optional.

1. Freien Task (`"status": "free"`) in `docs/TASKBOARD.json` wählen.
2. Dort eintragen: `status: "claimed"`, `owner`, `branch`, `claimed_at`.
3. **Genau den Branch anlegen, der im Board steht** — Namensschema in
   `docs/GIT_GLOSSAR.md`:
   ```bash
   git switch -c feature/T-102-tool-registry
   ```
4. Nur im `scope` des Tasks arbeiten. Das Board führt pro Task ein
   `scope`-Feld; ein Commit, der darüber hinausgeht, gehört zu einem anderen
   Task und macht beide unprüfbar.
5. Kleine Commits mit eigener Identität
   (`scripts/commit-as-agent.ps1`, Schlüssel in `docs/GIT_AGENTS.md`).

**Der Branch im Board und der Branch, auf dem du committest, müssen derselbe
sein.** Weichen sie ab, ist der Claim wertlos: niemand findet die Arbeit.

---

## 4. Fertig heißt gemergt

`done` ist **kein** Selbstbericht. Ein Task ist fertig, wenn sein Branch in
`master` steht — und das ist prüfbar:

```bash
git branch --merged master | grep <dein-branch>
```

Taucht er dort nicht auf, ist der Task höchstens `claimed`, egal wie grün die
Gates lokal waren. Ein Branch, der funktionierende Fähigkeiten enthält und
monatelang ungemergt liegt, ist eine Falle: Jeder Build aus `master` entfernt
diese Fähigkeiten wieder, ohne dass jemand eine Änderung sieht.

Erst nach dem Merge: `done` und `done_at` im Board setzen, Belegpfad
eintragen.

---

## 5. Gates

```bash
cargo test --lib                    # Default-Gate
cargo check --features tui          # TUI hinter Feature
cargo check --no-default-features   # CI-Zweig, ohne WebView
cargo clippy --all-targets -- -D warnings
```

Keine eingefrorenen Testzahlen in dieser Datei — sie wären binnen Tagen
falsch. Aktuelle Zahlen gehören in den jeweiligen Beleg.

---

## 6. Belege statt Behauptungen

Live-Fähigkeiten gelten nur über `docs/CAPABILITY_MATRIX.json` und
`data/capability/proofs.jsonl`. Ein grüner Exit-Code, eine Aussage eines
Brains oder eine ältere Statusdatei sind **keine** Belege. Ein Beleg altert:
Er verfällt und wird durch eine geänderte Selektordatei ungültig.

Keine unbelegten Prozentangaben, keine „funktioniert"-Aussagen ohne Messung
mit Datum.

---

## 7. Grenzen

- Keine Force-Pushes, keine History-Rewrites. Rücknahmen sind neue Commits.
- Keine Secrets oder Tokens lesen, kopieren oder committen.
- `data/` und `profiles/` enthalten Cookies und Sitzungen — niemals committen.
- `C:\Users\storax\.zcode\v2\config.json` nicht anfassen.
- Live-Tests gegen Anbieter nur im Rahmen deren Nutzungsbedingungen und nur
  nach ausdrücklicher Freigabe.
