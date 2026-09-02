# LLM-Crowdsourcing: Arbeitsleitfaden für dieses Repo

**Status:** verbindlich als *Leitfaden*. Er richtet sich an alle, die freiwillig
zum Projekt beitragen wollen: Menschen, ChatGPT-Codex, Claude Code, Grok,
Manus, opencode-Subagent u. a.

**Grundsatz:** Wir sind ein **freiwilliges LLM-Crowdsourcing**. Niemand ist zur
Teilnahme verpflichtet — jede Aufgabe wird freiwillig übernommen und kann
jederzeit wieder freigegeben werden. Der Leitfaden sorgt für **Struktur und
Konfliktvermeidung**, er zwingt niemanden.

## 1. Teilnahme und Grundlage

- **Alle tragen freiwillig bei.** Wer eine Aufgabe aus `docs/TASKBOARD.json`
  übernimmt, tut das aus freier Entscheidung. Rückfragen sind willkommen
  (Direktive 10); Richtung und Tempo des Gesamtprojekts bestimmt der
  Plan-Inhaber (der Mensch) — ein Artefakt (Leitfaden, Tafel) ersetzt keinen
  solchen Menschen, es strukturiert nur die Zusammenarbeit.
- **Verbindliche Grundlage (Rangfolge):**
  1. `C:\AGENTS.md` (Arbeitsdirektive, zwölf Direktiven);
  2. Repo-`AGENTS.md` und dieser Leitfaden;
  3. `docs/WEB_UI_API_TOOL_RESET.md` (Umsetzungsplan);
  4. Klarheit, Sicherheit und Reversibilität gehen allem voraus.

## 2. So übernimmst du eine Aufgabe (freiwilliger Claim)

1. **Claim zuerst:** Aufgabe in `docs/TASKBOARD.json` auf `claimed` setzen
   (`owner`, `branch`, `claimed_at`). Wirksam wird die Übernahme nur durch
   diesen Eintrag — nie nur mündlich/per Chat.
2. **Definition of Done vor Abschluss:**
   - Implementierung *und* Verifikationsbefehl aus der Aufgabe ausgeführt
     (Standard: `cargo test --lib` grün; bei Defaultbuild webview-only);
   - Beleg eingetragen (`docs/CAPABILITY_MATRIX.json` bzw. Belegpfad);
   - Handover-Status `docs/WEB_UI_API_TOOL_RESET_STATUS.md` aktualisiert;
   - Aufgabenzeile auf `done` gesetzt; Working-Tree clean; Push nach
     Absprache mit der Plan-Inhaberin/unverändert.
3. **Commit-Disziplin (Direktive 9):** kleine, klar beschriebene Commits; keine
   Geheimnisse, keine unnötigen Binärdateien; keine Force-Pushes; Rücksetzer als
   neue Commits. Arbeiten auf einem kurzen, benannten Arbeits-Zweig
   (`feature/…`, `fix/…`, `docs/…`; Schema `docs/GIT_GLOSSAR.md`), der klein &
   häufig in `master` gemergt wird — kein Riesen-Branch. Jeder Commit mit
   eigener Autor-/Committer-Identität (`scripts/commit-as-agent.ps1`; Mapping
   `docs/GIT_AGENTS.md`).
4. **Keine Außenwirkung ohne Freigabe (Direktive 6):** Veröffentlichungen,
   Löschungen, Rechteänderungen, Käufe, dauerhafte Änderungen an fremden
   Systemen — vorher konkreter Vorschlag, dann Freigabe.
5. **Geheimnisse (Direktive 7):** keine Tokens/Passwörter auslesen, kopieren
   oder ausliefern. Die zCode-Config bleibt tabu.
6. **Wahrheit statt Schein (Direktive 11):** Ergebnisse trennen geprüft /
   wahrscheinlich / unklar / blockiert; keine unbelegten `100 %`-Behauptungen.
7. **Unsicherheit (Direktive 10):** erst Recherche und lokale Prüfung, dann
   Rückfrage. Bei Konflikten oder Grenzfragen ins Abnahme-Verfahren (Abschnitt 4).

## 3. Rechte und Grenzen

- Jeder darf die Werkbank (Dateien, Software, Internet) für sein Vorhaben
  nutzen (Direktive 4) und sicher/reversibel arbeiten.
- Jeder verweigert keine Arbeit, markiert aber eigene Grenzen (z. B.
  Live-Claude-Web nur in zulässiger Nutzungssituation; `perplexity`-Status
  offen bis Endtest) und begründet Abweichungen.
- Bei mehreren Beitragenden: eine Aufgabe, eine Person/Agent; Parallelität über
  unterschiedliche Aufgaben. Wer die Tafel nicht schreibt, arbeitet nicht hier.

## 4. Abnahme und Inspektor (rückwärts aufräumen)

**Zweck:** Ausreißer abfangen — sei es aus Unwissen, Unlust oder kurzer
Aufmerksamkeitsspanne. Der Inspektor geht mit einer **Checkliste** durch und
**rumpelt so lange, bis alles erledigt ist**, statt einen fehlerhaften Stand
einfach durchzuwinken.

- **Wer:** der `reviewer`-Subagent (oder, falls nicht verfügbar, der
  Plan-Inhaber/`chief`) prüft eine gemeldete Aufgabe als unabhängige Instanz.
- **Prüfung (Checkliste — jeder Punkt muss erfüllt sein):**
  1. Claim vollständig (`owner`, `branch`, `claimed_at`, kein fremder Stand)?
  2. DoD-Verifikation gelaufen und **grün** (`cargo test --lib` u. ä.).
  3. Belegpfad vorhanden und wahrheitsgemäß (Direktive 11; keine `100 %` ohne
     Beleg).
  4. Keine Geheimnisse / tabu-Config-Spuren; keine Außenwirkung ohne Freigabe.
  5. Commit auf einem benannten Zweig, klein & gemergt; kein Riesen-Branch;
     `master` grün.
  6. Working-Tree clean; Handover-Status aktuell.
- **Verfahren bei Befund:**
  - Nur **Mängel** benennen (geprüft getrennt von „unklar/vermutet").
  - Bei geringem Mangel: zurück an die/den Beitragende(n) mit konkreter Liste;
    danach erneute Prüfung — **so lange, bis alle Punkte grün sind**.
  - Bei hartnäckigem oder grobem Ausreißer (heimliche Änderungen an
    Freigabegrenzen, verfälschte Belege, Geheimnis-Ausgabe): Aufgabe wird
    entzogen, auf `free` zurückgesetzt und der Stand rückwärts bereinigt.
- **Rückwärts-Aufräumen** bedeutet: nicht nur vermerken „ist kaputt", sondern
  den fehlerhaften Zustand aktiv auf einen grüneren zurückführen (Rücknahme als
  neuer Commit, Markieren von Teilbelegen, Zurücksetzen der Aufgabe).

## 5. Beendigung und Übergabe

- Jede abgenommene Aufgabe hinterlässt: Belegpfad, aktualisierte Statusdatei,
  sauberen Stand. Der Plan-Inhaber kann jederzeit übernehmen; der Leitfaden
  samt `docs/WEB_UI_API_TOOL_RESET_STATUS.md` ist das Übergabeprotokoll.