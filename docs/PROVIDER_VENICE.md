# Venice als Browser-Brain

> **Referenz.** Venice wird hier als Browser-Oberflaeche integriert, nicht
> über seine separate API. Die Primärquellen beschreiben Venice als privaten
> Web-Chat mit Modellwahl sowie Text-, Datei- und weiteren Funktionen:
> [Venice-Startseite](https://venice.ai/) und
> [Venice Text Generation](https://venice.ai/text-generation).

## Gelieferter Integrationsumfang

- Brain-ID: `venice`
- Start-URL: `https://venice.ai/chat`
- Separates Profil: `profiles/venice` nach der bestehenden Brain-Konvention
- Selektoren: `selectors/venice.json`, zusätzlich in die Binary eingebettet
- Katalog: `/v1/models` veröffentlicht `webagent/venice`; der Pi-Fallback
  kennt dieselbe Modell-ID, solange die Bridge nicht erreichbar ist

Die Selektoren decken Composer, Senden, Stop, neuen Chat, Login, Google-SSO,
Antwortcontainer, Modellmenü und Upload als konservative Startkandidaten ab.
Sie sind keine Live-Behauptung. Der selektorunabhängige Fallback bleibt aktiv,
falls Venice sein DOM ändert.

## Evidenzgrenze und Abnahme

Venice erhält keine Fähigkeit aus bereits bestehenden Brains. Die
Capability-Matrix enthält deshalb für Venice 13 Zellen mit `not_run`:
Web-Chat, Streaming, Modellwahl, Aufwand, Anhang, Managed Tools, drei
API-Pfade sowie Health, Sources, Groups und Security.

Vor einer Beförderung auf `passed` wird je betroffener Zelle ein frischer
Browser- oder Bridge-Lauf mit Beleg unter `docs/proofs/T-501/` benötigt. Für
`model` verlangt der gemeinsame Prüfer einen echten Wechsel, unabhängiges
Nachlesen und Wiederherstellen in derselben Sitzung. Für Medien werden die
Modalitäten im API-Katalog bis zu einem bestätigten Live-Smoke weiterhin nur
als `text` angekündigt.

## Ausstehender erster Lauf

1. `webagent diagnose --brain venice --headless`: URL, Profil, Login- und
   Composer-Zustand erfassen.
2. `webagent survey --brain venice --dump --headless`: aktuelle DOM-Kandidaten
   mit `selectors/venice.json` vergleichen und nur bei Evidenz nachschärfen.
3. Einen textuellen Chat-Smoke ausführen und erst danach Streaming, Modellwahl
   und Upload getrennt vermessen.
4. `/v1/models`, `/v1/chat/completions` und `/v1/responses` gegen
   `webagent/venice` prüfen; Managed Tools bleiben bis zu einem eigenen Test
   ausdrücklich offen.
