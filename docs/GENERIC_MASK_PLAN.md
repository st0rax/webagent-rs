> **Archiv.** Kein Soll-Zustand. Aktuell: docs/OVERVIEW.md, TUI-Betrieb: AGENTS.md §6.

# Generische Brain-Maske — Designplan

> **STATUS: ENTWURF — nichts gebaut.**
> Ziel: aus den deterministisch stabilisierten Ziel-Brains eine generische
> Maske ableiten, die neue Brains sofort trägt — der Multiplikator für das
> Featureset des Harness.
>
> Erarbeitet aus der Vision des Nutzers: *„erst deterministisch ausgewählte
> Brains produktiv machen — für einfache Prompts einsetzbar —, daraus eine
> generische Methode (heuristisch) ableiten, die hoffentlich einen Multiplikator
> im gesamten Featureset bringt."* Analogie: JDownloader-Plugin-Erkennung —
> ein generischer Extrakor für alle Hosts, Per-Host-Overrides nur wo nötig.
>
> **Die Basis ist gegeben (2026-08-11):** der volle headless-Lauf hat alle 8
> Brains samt realer Antworten ohne Eingriff durchlaufen (RUN_FULL_DONE,
> `verify-headless-full.log`). Die Brains sind damit für einfache Prompts
> produktiv — der Rohstoff für die Ableitung existiert.
>
> **Korrektur nach Bewertung:** „100% Featureset" war nie als striktes Gate
> gemeint. Die Maske **wächst mit** — Phase 2 ist der erste Schnitt, kein
> Endzustand. Vorbedingung ist die Produktivität, nicht die Vollständigkeit.
>
> Der Wert dieses Dokuments sind die Entscheidungen und ihre Begründungen,
> nicht seine Zeilennummern.

---

## 1. Idee in einem Satz

Eine eingebettete, **generische Selektor-Maske** ersetzt die `NotFound`-Lücke
für neue Brains: fehlende Brain-selektoren fallen per Key auf die Maske zurück,
die **bottom-up aus den belegten Ziel-Brains abgeleitet** ist — nicht top-down
erfunden.

## 2. Das Grundprinzip: deterministisch zuerst, dann ableiten

Die Reihenfolge ist Teil des Designs, kein Stilmittel:

1. **Deterministisch (Phase 1):** Die bewusst ausgewählten Brains auf ein
   **100%-Featureset** stabilisieren. Jeder vorhandene Key ist durch einen
   **bestandenen Live-Lauf** belegt (capability-proof-Prinzip: ein JSON-Eintrag
   ist kein Beweis). Deterministisch = exakte, gepflegte Selektoren.
2. **Ableitung (Phase 2):** Die generische Maske wird **aus den belegten
   Dateien abgeleitet** — pro Key die Fallback-Ketten vereinigt, die tatsächlich
   bei mehreren Brains funktionieren. Keine handgeschriebenen „könnte ja gehen"-
   Selektoren in die Maske.
3. **Multiplikator (Phase 3):** Die Maske ist die unterste Auflösungsstufe. Ein
   neues Brain (perplexity) erbt die Kern-Keys sofort; `probe` verfeinert
   Abweichungen als Overlay.

Warum diese Reihenfolge: Eine top-down erfundene Maske wäre ungeprüftes Raten
auf 8+ Oberflächen. Eine bottom-up abgeleitete Maske trägt nur das, was sich
**bereits bewährt** hat — der Hebel ist ein Aggregat des Bewiesenen, nicht ein
zusätzliches Risiko.

## 3. Befundlage (gemessen, nicht vermutet)

### 3.1 Der Kern ist real: 8/8

Vergleich aller 8 mitgelieferten Selektor-Dateien (2026-08-11):

| Schlüssel | Abdeckung | Bedeutung |
|---|---|---|
| `composer`, `send_button`, `stop_button`, `new_chat_button`, `login_button`, `login_indicator`, `google_sso_button`, `assistant_message` | **8/8** | Kern-Maske |
| `model_menu`, `model_option` | 5/8 | Modellwechsel |
| `consent_reject_button` | 4/8 | Cookie-Banner |
| `projects_button`, `reasoning_effort_menu`, `reasoning_toggle`, `web_search_toggle` | 2/8 | Brain-spezifisch |
| `attach_button`, `mode_option`, `dialog_dismiss_button`, `notice_close_button`, `reasoning_effort_path`, `temporary_chat_button`, `voice_input_button`, `voice_mode_button` | 1/8 | Einzelfälle |

Die 8 Kern-Keys existieren in **allen** Brains — die Maske ist also kein
Wunschbild, sondern die Verallgemeinerung eines vorhandenen Musters.

### 3.2 Die Lücke

`load_selectors` (src/config/selectors.rs:103) endet bei fehlender Datei mit
`Err(NotFound)` (Zeile 115). Ein registriertes Custom-Brain ohne Selektor-Datei
ist funktionslos, bis `probe --write` eine Datei erzeugt hat.

Beleg: **perplexity** ist als Custom-Brain registriert
(`custom_brains.json`, via `register_custom_brain` — der add-brain-Mechanismus
existiert). Ein früherer Probe-Lauf legte eine **partielle** Datei in die
Nutzer-Schicht (`%LOCALAPPDATA%\webagent\selectors\perplexity.json`): nur 7 Keys,
kein `stop_button`, kein `assistant_message`. Folge im verify-Lauf: nur
`projects` bestanden — nicht weil perplexity unfähig ist, sondern weil die
Maske für die fehlenden Keys schlicht nicht greift.

### 3.3 Was schon da ist

- **Registrierung:** `register_custom_brain` + `custom_brains.json`
  (src/config/brains.rs:294) — `/add_new_brain`-Backend existiert.
- **Erkennung:** `probe` (src/cli.rs:329) scannt das DOM generisch, klassifiziert
  Kandidaten nach Aria-Label/Beschriftung/Position (`brain_probe.rs:366-513`),
  prüft Vorschläge live (`verify`, Zeile 660). Deutsche/englische Beschriftungen,
  Perplexity-DOM inkl. Cookie-Banner sind als Tests eingebacken.
- **Overlay:** `load_selectors` merge_t Nutzer-Overlay über die Basis
  (`merge_selectors`, src/config/selectors.rs:82) — **pro Oberschlüssel
  vollständig**. Diese Semantik ist exakt der Baustein für die Maske.

### 3.4 Fangbereich: was die Maske fängt

Der Harness ist browserbasiert — fangbar ist jedes **Chat-UI im Browser**,
gleich welcher Anbieter:

- Klassische Chat-UIs (chatgpt, claude, deepseek, kimi, gemini, qwen, mistral,
  zai — die ersten acht des Pokédex).
- **OpenRouter** (openrouter.ai/chat): entgegen einer früheren Annahme doch ein
  Browser-Chat-UI — der „AI Chat Playground" zum Modell-Vergleich — also ein
  Maske-Kandidat, nicht nur eine API.
- **Hugging Face Chat** (hf.co/chat): Login via Google SSO, greift auf den
  8/8-Key `google_sso_button` zurück.
- **Nicht fangbar:** reine API-Endpunkte ohne Browser-UI. Die bräuchten einen
  anderen Backend-Typ (REST statt Selektoren) — für die ist die Maske
  irrelevant, sie ist ein Browser-Konzept.

## 4. Die drei Phasen

### Phase 1 — Deterministische Stabilisierung (jedes Brain auf sein Level 100)

**PoKImon-Framing:** Jedes Brain ist ein eigenes Pokémon mit eigenem,
UI-basiertem Featureset. „100%" ist **keine feste globale Liste**, sondern
**Level 100 dieses Brains**: alle Fähigkeiten, die seine Oberfläche bietet,
sind belegt (Level-UP pro verifizierter Fähigkeit). chatgpt war das erste
gefangene Pokémon (erster Eintrag in `BRAIN_TABLE`, src/config/brains.rs:184);
kimi gab dem Fork den Namen.

**Ziel:** Für jede der 8 Ziel-Brains ist jeder vorhandene, sinnvolle Key durch
einen bestandenen Live-Lauf belegt. Lücken sichtbar machen (verify-Läufe pro
Brain, `stop_generation`, `new_chat`, `model_switch`, …), fehlende Selektoren
per `probe`/Hand nachpflegen, erneut belegen.

**Definition of Done (Phase 1):** Der verify-Lauf jedes Brains zeigt genau die
Keys, die die Oberfläche kann — ohne `Unreachable`-Zufälle und ohne
„Selektor steht, aber nichts schaltet".

**Wichtig:** Phase 1 kennt noch keine Maske. Sie ist der rohstoff-liefernde
Schritt für Phase 2 — je belegter ein Key über mehrere Brains, desto
berechtigter sein Platz in der Maske.

### Phase 2 — Ableitung der Maske (bottom-up)

**Quelle:** die belegten Selektor-Dateien aus Phase 1.

**Mechanik:** Pro Key eine **bevorzugte Fallback-Kette** bilden — die
Reihenfolge der Selektoren, die über die meisten Brains hinweg den belegten
Treffer ergeben (z. B. `data-testid` vor generischen `aria-label`-Treffern,
siehe git-log `d7b25b7`: „data-testid vor die generischen Treffer").

**Aufnahme-Regel in die Maske:**

1. Der Key gehört zu den Kern-/verbreiteten Keys (8/8 oder 5/8).
2. Jeder aufgenommene Selektor stammt aus einer **belegten** Datei — nicht aus
   einem unbestätigten Kandidaten.
3. Falls mehrere Ketten konkurrieren, gewinnt die Reihenfolge aus den Dateien
   mit bestandenem verify-Beweis.

**Ausdrücklich nicht in die Maske:** 1/8-Keys (`voice_input`, `attach_button`,
`mode_option`, …) — das sind Oberflächen-Sonderfälle. Sie bleiben Brain-spezifisch.

**Form:** `selectors/_generic.json`, als `include_str!` in die Binary eingebettet
— analog zu `EMBEDDED_SELECTORS` (src/config/selectors.rs:11). Zusätzlich als
Datei im Repo, damit sie diffbar/curierbar bleibt.

### Phase 3 — Generischer Fallback + Multiplikator

**Auflösungskette** (statt `NotFound`):

```
user selectors  (Overlay, per Key vollständig)
  → brain selectors  (Overlay, per Key vollständig)
      → GENERISCHE MASKE  (Basis-Fallback, pro Key)
```

Die bestehende `merge_selectors`-Semantik trägt das ohne Umbau: Maske als Basis,
Brain-Datei als Overlay, Nutzer-Datei als Overlay. **Fehlt ein Key in der
Brain-Datei, fällt er per Key auf die Maske zurück** — fehlt die ganze Datei,
trägt die Maske allein.

**Multiplikator:** Jeder generische Key hebt das Featureset aller Brains, die
keinen eigenen Selektoren dafür haben. Ein neues Brain (perplexity) hat nach
Registrierung sofort die 8 Kern-Keys; `probe` ergänzt nur die Abweichungen
(Overlay). Der Rest des Featuresets ist geerbt, nicht gebaut.

## 5. Designentscheidungen

| # | Entscheidung | Begründung |
|---|---|---|
| D1 | Maske wird **bottom-up abgeleitet**, nie top-down erfunden | Ungeprüfte Selektoren wären ein zweites Risiko statt eines Hebels |
| D2 | Maske ist **unterste** Stufe, nie stärkstes | Der Brain (und der Mensch) bleiben immer Herr über die Maske |
| D3 | Fallback ist **pro Key**, nicht pro Datei | Ein Brain mit eigener Datei soll trotzdem von fehlenden Masken-Keys profitieren |
| D4 | **8/8- und 5/8-Keys** in die Maske, 1/8-Keys nicht | Maske = Verallgemeinerung, nicht Sammelsurium von Einzelfällen |
| D5 | Maske belegt **nichts von selbst** | Das capability-proof-Prinzip gilt weiter: erst ein bestandener Live-Lauf zählt ein Level. Die Maske bietet Fallbacks an, `verify` beweist sie pro Brain |
| D6 | Form: `_generic.json` eingebettet **und** im Repo | Download-Exe funktioniert ohne selectors-Ordner, Dev-Edits bleiben diffbar |
| D7 | `probe` bleibt die Verfeinerungs-Schleife | Neue Brains: Maske als Sofort-Basis, probe/verify hebt einzelne Keys in die Brain-Datei |

## 6. Testfall: perplexity

1. `perplexity` ist bereits registriert (custom_brains.json).
2. Nach Phase 3 greift die Maske — kein Hand-Selektor nötig.
3. verify-Lauf: `composer`, `send`, `stop`, `new_chat`, `login`, `assistant_message`
   müssen greifen; `model_switch` ist ein 5/8-Kandidat (bestehende partielle Datei
   hat bereits `model_menu`).
4. Abweichungen (z. B. `consent_reject_button`-Wortlaut „Nur notwendige") bleiben
   als Brain-Overlay liegen — die bestehende Datei wird nicht überschrieben,
   sondern nur ergänzt.

## 7. Risiken und Abwägung

- **Falschfreunde:** Ein generischer Selektor (z. B. `button[aria-label*='Send' i]`)
  kann in einer fremden UI etwas anderes treffen. Gegenmittel: Die Maske erbt nur
  belegte Ketten; jeder neue Brain durchläuft `verify`, bevor ein Level zählt.
- **UI-Churn:** Anbieter ändern HTML. Gegenmittel: Nutzer-Overlay über der Maske,
  Repair ohne Neubau (bestehende Semantik).
- **Masken-Vertrauen als Falle:** Die Gefahr, dass eine „funktionierende" Maske
  ein fehlendes Brain-Featureset überdeckt. Gegenmittel: D5 — die Maske ist nie
  ein Beweis, nur ein Angebot.
- **Pflege:** Die Maske altert, wenn Phase-1-Dateien weiter verbessert werden.
  Gegenmittel: Phase 2 wiederholbar machen (Ableitung als Skript/Kommando statt
  Einmal-Handarbeit) — als Ausbau nach dem ersten manuellen Entwurf.

## 8. Abgrenzung

Nicht Teil dieses Plans: `file_attach` (attainable: false, Dateidialog — eigener
Weg, siehe capability.rs:184), die UI-Erkennung selbst (`brain_probe` bleibt
unverändert), die Beweis-Mechanik (`capability_proof` bleibt unverändert).
Die Maske berührt nur **eine Stelle**: die Auflösungskette in `load_selectors`.
