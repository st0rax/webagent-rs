# T-935: Composer blockweise befuellen

**Referenz:** Live-Nachweis zu T-935, Stand 2026-09-14. Verbindlich sind START_HERE.md und docs/TASKBOARD.json.

## Herkunft

Commit d7918b2 von local/opencode (04:12), per cherry-pick mit erhaltener
Autorschaft als 709d350 auf `fix/T-935-chunked-fill-v2` uebertragen und von
claude-code uebernommen. Konflikt in `provider_handlers.rs`: nur die
Log-Zeile uebernommen, nicht die dort erneut eingefuegte Pruefung
`require_clean_text_tools`, die der T-934-Port entfernt hat.

## Vorher

Pi-Session mit chatgpt ueber die Bridge, am selben Tag: alle 14 Turns bis
25.264 Zeichen bestanden; ab 29.608 Zeichen scheiterten 5 von 11 Turns mit
`Composer-Feld nicht gefunden (Timeout)`, spaeter drei in Folge bei 83.651
und einer bei 303.995 Zeichen.

## Nachher

Build aus `fix/T-935-chunked-fill-v2` (8d18a16), Bridge PID 15700.
Einzelanfragen an `webagent/chatgpt`, neutraler Fuelltext, am Ende eine
Kontrollzahl:

| Zeichen | HTTP | Dauer | Composer-Fehler | Kontrollzahl |
|---:|---|---:|---|---|
| 40.172 | 200 | 28 s | nein | genannt |
| 40.172 | 200 | 25 s | nein | genannt |
| 60.172 | 200 | 25 s | nein | genannt |
| 60.172 | 200 | 25 s | nein | genannt |

## Grenzen

- Kein kontrollierter Vorher-Nachher-Vergleich: Die Fehlschlaege stammen aus
  einer Pi-Session mit Werkzeugliste und langem Verlauf, der Nachweis nutzt
  Einzelanfragen mit neutralem Text. Vier bestandene Laeufe schliessen seltene
  Fehlschlaege nicht aus.
- Groessen ueber 60.000 Zeichen sind hier nicht geprueft.
