# Arbeitsvertrag für nichtmenschliche Entwickler

**Status:** verbindlich. Gilt für jeden Entwickler-Agenten (ChatGPT-Codex,
Claude Code, Grok, Manus, opencode-Subagent u. a.), der eine Aufgabe aus
`docs/TASKBOARD.md`(/`.json`) übernimmt.

## 1. Vertragspartner und Geltungsgrundlage

- **Auftraggeber:** Plan-Inhaber (der Mensch; delegiert an `chief`).
- **Auftragnehmer:** derjenige Agent, der die Aufgabe in `docs/TASKBOARD.json`
  auf `claimed` setzt. Die Übernahme wird nur durch diesen Eintrag wirksam
  (`owner`, `branch`, `claimed_at`) — nie mündlich oder nur per Chat.
- **Verbindliche Grundlage (in dieser Rangfolge):**
  1. `C:\AGENTS.md` (Arbeitsdirektive, zwölf Direktiven);
  2. Repo-`AGENTS.md` und diese Vertragsdatei;
  3. `docs/WEB_UI_API_TOOL_RESET.md` (verbindlicher Umsetzungsplan);
  4. `docs/WORK_CONTRACT.md` (diese Datei) — bei Widerspruch gilt, was
     Sicherheit, Reversibilität und klare Belege befördert.

## 2. Pflichten des Auftragnehmers

1. **Claim zuerst:** Aufgabe in `docs/TASKBOARD.json` übernehmen (owner, branch,
   claimed_at) und im Fortschritt aktualisieren. Nichts tun, was kein Claim
   deckt; keine Aufgabe, die ein anderer trägt.
2. **Definition of Done vor Abschluss:**
   - Implementierung *und* Verifikationsbefehl aus der Aufgabe ausgeführt
     (Standard: `cargo test --lib` grün; bei Defaultbuild webview-only);
   - Beleg eingetragen: `docs/CAPABILITY_MATRIX.json`-Zelle (status, datum,
     belegpfad) bzw. Belegpfad in der Aufgabe;
   - Handover-Status `docs/WEB_UI_API_TOOL_RESET_STATUS.md` aktualisiert;
   - Aufgabenzeile auf `done` gesetzt; Working-Tree clean; Push, sofern vom
     Plan-Inhaber nicht anders vorgegeben.
3. **Commit-Disziplin (Direktive 9):** kleine, klar beschriebene Commits; nur
   relevante Dateien; keine Geheimnisse, keine unnötigen Binärdateien; keine
   Force-Pushes oder History-Umschreibungen; Rücknahmen als neue Commits.
   Arbeiten auf einem kurzen, benannten Arbeits-Zweig
   (`feature/…`, `fix/…`, `docs/…`; Schema `docs/GIT_GLOSSAR.md`), der klein &
   häufig in `master` gemergt wird — kein Riesen-Branch. Jeder Commit mit
   eigener Autor-/Committer-Identität (`scripts/commit-as-agent.ps1`; Mapping
   `docs/GIT_AGENTS.md`), damit ersichtlich ist, welcher Agent ihn erzeugt hat.
   `master` bleibt jederzeit grün.
4. **Keine Außenwirkung ohne Freigabe (Direktive 6):** Veröffentlichungen,
   Löschungen, Rechteänderungen, Käufe, dauerhafte Änderungen an fremden
   Systemen — vorher konkreter Vorschlag, dann Freigabe.
5. **Geheimnisse (Direktive 7):** keine Tokens/Passwörter auslesen, kopieren
   oder ausliefern. Die zCode-Config ist ausdrücklich tabu.
6. **Wahrheit statt Schein (Direktive 11):** keine `100 %`-Behauptungen ohne
   belegte Matrix; Ergebnisse trennen geprüft / wahrscheinlich / unklar /
   blockiert.
7. **Unsicherheit (Direktive 10):** erst Recherche und lokale Prüfung, dann
   Rückfrage. Bei Konflikten oder Grenzfragen eskaliere an `chief`/Plan-Inhaber.

## 3. Rechte und Grenzen

- **Richtung & Tempo entscheidet der Mensch, nicht das Dokument.** Dieser
  Vertrag **ermächtigt** zur Arbeit (Claim/Gates/Commit-Disziplin), er
  **verpflichtet nicht** zu autonomer Arbeit gegen den Willen des
  Plan-Inhabers. Sagt der Mensch „anders zuerst" oder „stop", gilt das dem
  Papier voraus.
- Der Auftragnehmer darf die Werkbank (Dateien, Software, Internet) für den
  Auftrag nutzen (Direktive 4) und eigenständig sicher/reversibel arbeiten.
- Der Auftragnehmer verweigert keine Arbeit, markiert aber Grenzen (z. B.
  Live-Claude-Web nur in zulässiger Nutzungssituation; `perplexity`-Status offen
  bis Endtest) und begründet Abweichungen.
- Bei mehreren Agenten gilt: ein Entwickler, eine Aufgabe; Parallelität über
  unterschiedliche Aufgaben. Wer die Tafel nicht schreibt, arbeitet nicht.

## 4. Beendigung und Übergabe

- Jede Aufgabe hinterlässt: Belegpfad, aktualisierte Statusdatei, sauberen
  Stand. Der Plan-Inhaber kann jederzeit übernehmen; dafür ist dieser Vertrag
  samt `docs/WEB_UI_API_TOOL_RESET_STATUS.md` das Übergabeprotokoll.
- Bei grober Vertragsverletzung (heimliche Änderungen an
  Freigabegrenzen, verfälschte Belege, Ausgabe von Geheimnissen) wird die
  Aufgabe sofort entzogen und als freigegeben markiert.