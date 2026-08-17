> **Archiv.** Kein Soll-Zustand. Aktuell: docs/OVERVIEW.md, TUI-Betrieb: AGENTS.md §6.

# Übergabe — webagent-rs, Stand 2026-07-28

Verfasst von Claude (Desktop-Agent) für Codex/ChatGPT und jede Session, die kalt einsteigt.
Stand: HEAD `5a9778922`, 615 Tests grün, Clippy `-D warnings` sauber, deployt und gepusht.

Alles hier Behauptete ist **live nachgeprüft**, nicht nur durch Tests. Wo etwas nur vermutet
ist, steht das ausdrücklich dabei.

---

## 1. Behobene Fehler

### 1.1 Worker-Pool stand seit dem 25./26.07. still

Der Cooldown war eine Einbahnstraße. Drei Wiederbelebungspfade existieren, und **keiner**
fasst `cooldown` an:

- `select_auto_recovery` filtert strikt auf `unavailable`
- `reset_orphaned_active` nur auf `active`
- `load_or_init` ergänzt nur fehlende Einträge

Eine Failover-Kaskade hatte alle acht Brains nacheinander in den Cooldown geschickt
(`blocked: reserve promoted`). Die Sperren liefen nach 10 Minuten ab, aber niemand las sie
mehr — es gab kein `available` mehr, aus dem der Pool hätte starten können. **Deadlock.**
Sichtbar nur an Details: Heartbeats 32–48 h alt, `cooldown_until` durchgehend in der
Vergangenheit, bei allen acht identischer `last_error`.

Behoben durch `select_expired_cooldowns` pro Tick. Ein Cooldown-Eintrag **ohne**
Ablaufzeitpunkt gilt als kaputt und wird ebenfalls freigegeben, sonst säße dieselbe Falle
eine Ebene tiefer. `retired` bleibt final. Der Regressionstest bildet exakt die beobachtete
Lage nach.

### 1.2 Claude lieferte seine Denk-Anzeige als Antwort

`relay --brain claude` gab reproduzierbar (2/2) `"Crystallizing"`, `"Triangulating"`,
`"Weighing"` zurück — mit `ok=true` nach ~9,8 s. Für den Swarm sah das aus wie ein gültiges
Ergebnis.

Commit `1cbc7ef` hatte den Streaming-Container aus `claude.json` entfernt; nötig, aber nicht
hinreichend. `is_transient_response_text` kannte nur eine **feste Wortliste**. Claudes
Denk-Anzeige rotiert durch offenen Wortschatz — eine Vokabelliste holt das prinzipiell nie
ein. Jetzt formbasiert: dasselbe Wort mehrfach hintereinander (das DOM liefert das Label
doppelt) und Zeichen aus der Private-Use-Area in kurzem Text.

Danach kam die echte Antwort, trug aber die Denk-Überschrift doppelt vor sich her. Zwei
Ursachen: es gibt **zwei Auslesepfade** (`assistant_text` und die kombinierte Poll-Funktion
der Warteschleife — zurückgegeben wird die der Warteschleife), und der Icon-Glyph U+E027
hängt nur an der **ersten** Kopie, weshalb der Zeilenvergleich zwei identische Zeilen für
verschieden hielt.

### 1.3 Gemini galt als eingeloggt, war es aber nicht

`diagnose --brain gemini` meldete `logged_in: true`, während der Screenshot die ausgeloggte
Marketing-Startseite mit „Anmelden" zeigte. Ursache: `login_indicator` war
`div[contenteditable=true]`, also der Composer — und Geminis ausgeloggte Startseite hat
ebenfalls einen („Frag Gemini"). Jeder anonyme Besuch galt als angemeldet, und
`brains-health`, `canary` und `doctor` führten das Brain als gesund. Der Pool hätte ihm
Aufträge gegeben, die nie ankommen.

Behoben: **ein sichtbarer Anmelden-Knopf schlägt jeden positiven Indikator.** Ein Test
erzwingt, dass jedes Brain `login_button`-Selektoren hat, sonst liefe die Sperre ins Leere.

> **Offen und nur von Hand lösbar:** Gemini ist real ausgeloggt.
> `webagent login --brain gemini` öffnet das Fenster. Login-Flows werden nicht automatisiert.

### 1.4 `canary` ist kein Live-Check

Meldete „8 Brains all ok" — die Probe prüft aber nur, ob die Selektor-*Datei* auf der Platte
liegt. Kein Browser, kein Login, kein Netz. Verraten hat es sich durch `latency_ms=0` bei
allen acht. Der Hilfetext ist korrigiert; für echte Login-Prüfung ist `diagnose` zuständig.

### 1.5 Verwaiste Wegwerf-Profile (33,6 GB)

`cleanup_swarm_profiles` lief nur im `Drop`-Guard und nur für die eigene `run_id`. Bei
Absturz oder Kill blieb das ~200 MB große Profil liegen; es hatten sich 161 Stück
angesammelt. `sweep_stale_runtime_profiles` räumt beim Start alles > 12 h weg, für
`swarm/` **und** `encapsulated/` (gleiche Bauart, gleiche Lücke).

---

## 2. Neue Fähigkeiten

| Kommando | Zweck |
|---|---|
| `webagent shot [--brain X] [--open KEY]` | Nimmt die eigene WebView als PNG auf (`ICoreWebView2::CapturePreview`), **headless**. `--open` klickt vorher ein Element, z.B. `model_menu`. |
| `webagent wall [--interval N] [--once]` | Kachelseite aller Brains, lädt sich selbst neu. |
| `webagent survey [--brain X] [--dump] [--open KEY]` | Vermisst die Oberfläche im DOM. |
| `webagent model --brain X [--set NAME]` | Modelle listen/wechseln. |
| `webagent menu --brain X --key K [--set "A > B"]` | Beliebiges Menü, auch über Untermenü-Pfad. |
| `webagent toggle --brain X --option K` | Umschalter mit Zustandsbeleg. |
| `webagent mode --brain X --set S` | Segmentleiste. |
| `webagent section --brain X --key K` | Bereich öffnen, Beleg über URL. |
| `webagent quests [--json]` | PoKIdex: Level je Brain + Questlog. |

**Der Screenshot war der Durchbruch.** Die DOM-Vermessung fand bei DeepSeek *null* Optionen:
107 Bedienelemente als anonyme `div.ds-button--icon`, ohne `aria-label`, `title`, `id`,
`data-*` oder Text. Auf dem Bild sind dieselben Knöpfe sofort erkennbar. Wer hier
weitermacht: **erst gucken, dann Selektoren suchen.**

---

## 3. Das Level-Modell (`capability.rs`)

Ein Level zählt nur, wenn **beides** gilt: der Code kann die Option fahren (`driveable`)
UND für dieses Brain sind Selektoren hinterlegt.

Der **Nenner gilt pro Brain** — die Optionen, die *diese* Oberfläche anbietet (`ui_options`
in der Selektor-JSON). Ein globaler Nenner würde ein karges UI für seine Kargheit bestrafen.

Ohne gepflegtes `ui_options` gilt das Brain als **unvermessen** (`[3/?]`). Der erste Entwurf
nahm ersatzweise „die Selektoren, die da sind" — dann meldete jedes Brain „ausgereizt",
obwohl nur Text lief. Ein geratenes Maximum ist schlimmer als ein zugegebenes Fragezeichen.

`attainable: false` heißt **nicht** „noch nicht gebaut", sondern „mit den Mitteln dieses
Agenten nicht nachweisbar fahrbar". Solche Einträge fallen aus dem Nenner, bleiben aber als
`out_of_reach` sichtbar. Betroffen: `file_attach` (Dateidialog des Betriebssystems,
JavaScript kann keine File-Objekte erzeugen), `voice_input` und `voice_mode` (anklickbar,
aber ein laufendes Mikrofon ändert keinen prüfbaren Zustand).

### Jeder Antrieb prüft nach

Das ist der Kern und der Grund für die meisten Zeilen:

- **Modellwechsel:** Menü-Beschriftung muss danach den Zielnamen tragen.
- **Umschalter:** alle `data-*`/`aria-*` und die Klassenliste vorher gegen nachher.
- **Bereich:** die URL muss sich geändert haben.
- **Segmentleiste:** Zustand des gewählten Eintrags, notfalls Signatur der ganzen Leiste.

Vier Fallen, alle real aufgetreten:

1. **Zielwert stand schon da** → jede Prüfung trivial erfüllt. ChatGPT meldete „umgestellt
   auf ChatGPT", obwohl nichts passierte. Wird jetzt als „bereits aktiv" gemeldet.
2. **Verschwinden ist kein Beleg.** Kimis Chip-Leiste rendert nach dem Klick neu, der
   Zustand danach ist leer, also „verschieden" — der Screenshot zeigt aber, dass nichts
   aktiviert wurde. Leerer Zustand nach nicht-leerem gilt jetzt als Fehlschlag.
3. **`Q` liefert den innersten Treffer**, also den Textknoten statt der schaltbaren Pille.
   Deren Klassen ändern sich nie. Alle Klickpfade laufen per `closest()` hoch.
4. **Zu enge Attributliste.** Zais Deep-Think-Knopf hält seinen Zustand in `data-autoThink`,
   und die Klassenliste ändert sich nicht (Tailwind führt `data-[autoThink=true]:` statisch
   im `class`-Attribut).

### Selbsttäuschung, die im Modell steckt

**Das Level prüft „Selektor vorhanden", nicht „Selektor funktioniert."** Zai stand kurz auf
`[6/6] ausgereizt`, weil ich beim Suchen Kandidaten in die Datei geschrieben hatte, die auf
den `+`-Knopf zeigen und nachweislich nichts schalten. Selbst gefangen, aber nur zufällig.
**Wer hier weiterbaut: ein Level sollte einen bestandenen Live-Lauf voraussetzen, nicht
einen JSON-Eintrag.** Das ist die wichtigste offene Designaufgabe.

---

## 4. Stand der Level

```
chatgpt  [4/4] ausgereizt      claude [6/6] ausgereizt      zai [6/6] ausgereizt
deepseek [5/7]                 qwen   [5/7]
kimi     [3/5]                 mistral [3/5]
gemini   [3/?] unvermessen (ausgeloggt)
```

Gesamt 36. Zu Beginn des Ziels: 24.

> **Diese Level-Tabellen sind Maschinenstände, keine Projektaussagen.** Sie
> beschreiben Storax' Accounts (gemini ausgeloggt, deepseek mit 7 statt 9
> Optionen), nicht das Projekt. Lebende Quellen sind `docs/ARCHITECTURE.md` und
> `docs/PROVIDER_STATUS.md`; die Tabellen hier und in `docs/PARITY.md` (dort
> als OBSOLETE markiert) sind Momentaufnahmen vom Stand der jeweiligen Sitzung.

---

## 5. Offene Blocker — gemessen, nicht vermutet

| Brain | Blocker |
|---|---|
| gemini | Ausgeloggt. Ohne Anmeldung kein Nenner. Nur von Hand lösbar. |
| deepseek | Segmentleiste `Instant/Expert/Vision`: weder am Knopf, noch am nächsten Vorfahren mit Klasse (`_6c7e7df`), noch an der Gesamtsignatur der Leiste ändert sich etwas. Der Klick hinterlässt im DOM keine Spur. Möglich, dass es nur eine Ansicht der leeren Startseite ist und gar keinen Modus setzt. |
| deepseek | Modellwechsel: kein Aufklappmenü vorhanden. |
| qwen | Denkstufen-Menü („Auto") öffnet weder per `element.click()` noch per Koordinatenklick. Screenshot danach unverändert. Nächster Ansatz: vollständige Pointer-Sequenz (down/up/capture). |
| kimi | „Tiefgehende Recherche" existiert doppelt im DOM (Seitenleiste + Chip). Der auf den Container eingegrenzte Selektor trifft, bewirkt aber sichtbar nichts. |
| mistral | Modellwähler heißt „Schnell", noch kein belastbarer Anker. |

### Nicht gelöst, aber ein Weg existiert

`voice_input` gilt als nicht belegbar, weil ein Mikrofon keinen prüfbaren Zustand ändert.
**Chromium kann eine WAV-Datei als Mikrofon einspeisen** — WebView2 nimmt Flags über
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` entgegen:

```
--use-fake-ui-for-media-stream
--use-fake-device-for-media-stream
--use-file-for-fake-audio-capture=<datei.wav>
```

Damit landet die Transkription im Eingabefeld, und dessen Inhalt ist vergleichbar wie jeder
andere Beleg. Ein opt-in über `WEBAGENT_FAKE_AUDIO=<pfad.wav>` ist in
`webview_runtime.rs::apply_fake_audio_args` vorbereitet — **bewusst opt-in**, weil
`--use-fake-ui-for-media-stream` die Mikrofon-Freigabe ohne Rückfrage erteilt und als
Dauerzustand eine stille Absenkung der Browser-Sicherheit wäre. **Noch nicht end-to-end
erprobt.**

---

## 6. Fallen für die nächste Session

**Doppelte Auslesepfade.** Greift ein verifizierter Fix live nicht, NICHT die Logik weiter
ändern — erst nach dem Zwilling suchen. Heute dreimal getroffen: Antworttext
(`assistant_text` + Poll-Funktion), Selektoren (`webagent-rs/selectors/` ist wirksam,
`webagent/selectors/` ist eine veraltete Zweitkopie), Binary (`runtime-workers` vs.
`target/release`).

**Tests und Commit trennen.** Nie im selben Shell-Aufruf. Ich habe zweimal mit rotem Test
committet, weil das Ergebnis erst nach dem Commit sichtbar wurde — die Prüfung läuft dann
zwar, gatet aber nichts. Mindestens ein Test ist zudem flaky (ein Lauf rot, der nächste grün
ohne Änderung); bei einem einzelnen roten Test also erst wiederholen.

**Selbstauskunft der Brains ist als Messmethode unbrauchbar.** Auf die Frage nach seinen
Optionen gab DeepSeek die komplette abgefragte Liste zurück, inklusive Canvas und Temporary
Chat, die es nicht hat. Claude antwortete mit seiner Denk-Anzeige. Immer aus dem DOM oder
vom Bild messen.

**Nur im TUI starten.** Der Worker-Pool läuft ausschließlich über `webagent tui`, nie roh
`webagent workers` im Hintergrund.

**Deploy nur über `delivery/deploy_webagent_rs.ps1`.** `post_deploy_check.ps1` fängt stale
Binary und gebrochene CLI.

---

## 7. Nächste sinnvolle Schritte

1. Gemini anmelden (Storax), dann vermessen — danach sind sieben Brains erreichbar.
2. Das Level an einen bestandenen Live-Lauf koppeln statt an einen JSON-Eintrag (siehe 3.).
3. Qwens Menü mit vollständiger Pointer-Sequenz statt `.click()`.
4. `WEBAGENT_FAKE_AUDIO` end-to-end erproben — das entsperrt Spracheingabe als *belegbare*
   Fähigkeit.
5. Der Modellwechsel hält **nur innerhalb einer Sitzung**; jeder neue Browserstart fällt aufs
   Standardmodell zurück. Für den Swarm hieße das: `relay --model X`, damit Wechsel und Frage
   in derselben Sitzung passieren.
