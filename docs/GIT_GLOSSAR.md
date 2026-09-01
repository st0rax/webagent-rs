# Git-Glossar

> **Referenz-Kurzliste.** Die hier gesammelten Begriffe sind die im Repo gültigen
> technischen Ausdrücke — verwendet sie in Doku, Commits und beim Reden mit
> Menschen/Agents. Es gibt nur eine sichtbare Schiene (`master`), alles Weitere
> sind temporäre Seitenäste oder verwaltete Kopien.

## Das Wichtigste in einem Satz

**Stamm = `master`.** Das ganze Projekt liegt auf *einem* sichtbaren Branch
(`master`). Von dort können temporäre Seitenäste (Branches) abzweigen und
fließen vor der Abgabe zurück in den Stamm.

## Tabelle

| Begriff | Bild | Bedeutung | Wann wir es tun |
|---|---|---|---|
| **Stamm / `master`** | der Hauptstand | die eine sichtbare Basis, auf der alles leben | alles landet dort |
| **Branch** | ein Ast | eine vom Stamm abgehende Arbeitsversion | nur temporär für isolierte Arbeit; läuft vor Abgabe zurück |
| **Fork** | eigenständige Kopie | das komplette Projekt in ein *eigenes* Repo kopieren | selten; nicht hier |
| **Clone** | Steckling | Projekt lokal auf deinen Rechner kopieren | beim Einrichten |
| **Commit** | Schnappschuss | Stand lokal festschreiben | jede sinnvolle Änderungseinheit |
| **Push** | hochladen | lokale Commits zum `origin` (GitHub) übertragen | nach jedem fertigen Schritt (Direktive #9) |
| **Pull** | runterladen | neuesten Stand vom `origin` holen | vor dem Aufnehmen von Arbeit |
| **Fetch** | nur ansehen | Stand vom `origin` holen, ohne zu mergen | selten zum Prüfen |
| **Merge** | zurück in den Stamm | zwei Linien zu einer vereinen | wenn ein Branch fertig ist |
| **Rebase** | Ast umtopfen | Branch neu auf den aktuellen Stamm setzen | optional, zum Reinigen |
| **Checkout** | Stand setzen | Working Tree auf einen Branch/Commit-stand umschalten | selten nötig |
| **Staging / `git add`** | vormerken | Dateien für den Commit markieren (Staging-Area) | vor jedem Commit |
| **Working Tree** | Arbeitskopie | die gerade ausgecheckten Dateien auf der Festplatte | immer relevant |
| **Origin** | Kanon. Remote | der standard-Name für den Remote-Server (= GitHub) | push/pull-Ziel |
| **Remote** | Gegenstelle | ein ferner Repo-Server (z. B. `origin` = GitHub) | bestimmt push/pull |
| **HEAD** | aktuelle Position | der Stand, auf dem du gerade bist | zeigt beim jederzeit |
| **Tag** | Etikett / Meilenstein | einen Commit mit Namen markieren (z. B. `v0.11.0`) | bei Releases |

## Wichtigste Sicherheitsregel

Ein **Commit ist nur lokal** — erst ein **Push** überträgt ihn zum `origin`.
Ohne Push ist ein lokaler Stand bei Systemausfall potentiell verloren. Deshalb
immer: klein committen, **regelmäßig pushen** (Direktive #9).

## Wortschatz-Regeln für uns

- Verwende die Fachausdrücke aus dieser Tabelle konsequent.
- Kein „Ästeln/ästen" oder „verzweigen" als Ersatz für `merge`/`branch`.
- „Brach" gibt es nicht als Git-Begriff — es heißt **Branch**.

## Branch-Namensschema (verbindlich pro Arbeits-Zweig)

Jeder Arbeits-Zweig (kein `main`) wird nach diesem Muster benannt:

```
<typ>/<kurzbeschreibung>
```

| Typ | wann nutzen | Beispiel |
|---|---|---|
| `feature/` | neue Fähigkeit | `feature/T-102-tool-registry` |
| `fix/` | Fehlerbehebung | `fix/rollback-panic` |
| `docs/` | Doku/Wissensarbeit | `docs/glossar` |
| `chore/` | Wartung, einrichten, Meta | `chore/ci` |
| `refactor/` | Umbau ohne Verhalten | `refactor/session-extract` |
| `test/` | Tests/Verifikation | `test/provide-status` |

Regeln:
- Kürze + Kleinbuchstaben (+ Bindestriche); optional `T-<id>`-Ref davor.
- **Nie am Ende einen „Riesen-Branch" pushen.** Sobald der Zweig eine grüne,
  in sich abgeschlossene Einheit ist, klein in `master` mergen und pushen.
- `master` bleibt immer grün und ist der einzige dauerhafte Zweig.