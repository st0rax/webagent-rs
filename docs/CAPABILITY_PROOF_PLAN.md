> **Archiv.** Kein Soll-Zustand. Aktuell: docs/OVERVIEW.md, TUI-Betrieb: AGENTS.md §6.

# Capability-Proof — Implementierungsplan

> **STATUS: UMGESETZT — alle Phasen gebaut, erste Läufe gemessen (2026-08-09).**
> `src/capability_proof.rs`, das Gate in `src/capability.rs`,
> `src/browser/verify.rs` und `webagent verify` (`commands/ops.rs`) stehen.
> 860 Tests grün, `cargo clippy --all-targets -- -D warnings` sauber.
> **Nichts committet** — vor dem Weiterbauen `git status` prüfen.
>
> Erarbeitet in vierzehn `/grill-me`-Durchgängen, 52 Entscheidungen, alle in §10
> mit Begründung. **Der Wert dieses Dokuments sind die Entscheidungen und ihre
> Begründungen, nicht seine Zeilennummern.**
>
> ⚠ **Alle Zeilenverweise beziehen sich auf `evolution/supervised-harvest`
> @ `11d8619` (2026-08-08) und sind seither veraltet.** Danach wurden
> `config.rs` → `src/config/` und `protocol.rs` → `src/protocol/` aufgeteilt und
> 22 weitere Dateien geändert. Beispiele: `fnv1a` liegt jetzt in
> `src/config/selectors.rs` (und ist wie gefordert `pub(crate)`), das Gate ist
> nicht mehr `capability.rs:561`. Verweise als Fundstelle-zum-Zeitpunkt lesen,
> nicht als Sprungmarke.
>
> Was aus dem Code beantwortbar war, wurde gemessen statt gefragt.

---

## 1. Idee in einem Satz

Ein Level zählt eine Fähigkeit erst, wenn ein **bestandener Live-Lauf** sie
belegt hat — nicht schon, wenn ein Selektor-Eintrag in einer JSON steht.

## 2. Herkunft

Aus `docs/UEBERGABE_2026-07-28.md`, Abschnitt „Selbsttäuschung, die im Modell
steckt":

> Das Level prüft „Selektor vorhanden", nicht „Selektor funktioniert." Zai stand
> kurz auf `[6/6] ausgereizt`, weil ich beim Suchen Kandidaten in die Datei
> geschrieben hatte, die auf den `+`-Knopf zeigen und nachweislich nichts
> schalten. Selbst gefangen, aber nur zufällig. **Wer hier weiterbaut: ein Level
> sollte einen bestandenen Live-Lauf voraussetzen, nicht einen JSON-Eintrag.**
> Das ist die wichtigste offene Designaufgabe.

## 3. Was schon da ist — und was wirklich fehlt

**Der Beweis wird bereits erbracht und dann weggeworfen.** Das ist keine
Metapher, sondern der gemessene Zustand:

`brain_probe.rs:503-518` definiert einen `Verdict` mit `capability_key`,
`selector_key`, `before`, `after`, `proven: bool`, `restored: Option<bool>` und
`note`. `operations::verify_surface` fährt damit **den vollständigen
Round-Trip** — Zustand lesen, klicken, Zustand lesen, zurückschalten, Rückweg
belegen (`brain_probe.rs:569`), getestet in `brain_probe.rs:1022-1024`. Die
Notiz im Nicht-belegt-Fall lautet wörtlich:

> Klick kam an, Zustand unveraendert — kein Beleg, kein Level
> (`brain_probe.rs:565`)

Der Maßstab dieses Plans ist also bereits implementiert. Was fehlt:

| Lücke | Wo | Folge |
|---|---|---|
| Verdicts werden nicht gespeichert | `commands/ui.rs:616` gesammelt, `:740-746` als Zähler gedruckt, dann weg | jeder Beleg verfällt mit dem Prozess |
| Verdicts gaten das Schreiben nicht | Write-Pfad `commands/ui.rs:641-680` konsultiert `verdicts` nie | ein `FAIL`-Selektor landet trotzdem in der Nutzerdatei |
| Das Level fragt nicht nach Belegen | `capability.rs:561` und `:576-583` | ein JSON-Eintrag ist weiterhin ein Level |
| Verifikation deckt nur Namensmuster ab | Filter `commands/ui.rs:621`: `ends_with("toggle") \|\| contains("switch")` | `reasoning_effort_menu`, `projects_button`, `new_chat_button` werden nie geprüft |

Es fehlt also kein Subsystem und nicht einmal der Round-Trip — es fehlen
**Persistenz, Gate und Abdeckung**.

### Wer schreibt eigentlich Selektoren

Nicht `brain_probe.rs` — das Modul enthält kein einziges `write`. Geschrieben
wird an zwei Stellen in `commands/ui.rs`:

1. **`probe --write`** (`:641-680`): merged in die Nutzerdatei per
   `.or_insert_with()` — bestehende Schlüssel werden **nie** überschrieben, nur
   fehlende ergänzt. Schreibt auch `ui_options`, also den Nenner des Levels.
2. **`survey --write`** (`:880-903`): setzt `ui_options` per
   `union_ui_options`, kann also nur heben. Abgesichert durch die Regression
   `survey_schreibt_ui_options_nur_dazu` (`:927`) — Anlass war der Vorfall aus
   `capability.rs:881-886`, bei dem ein Lauf aus einer ausgeloggten Sitzung
   `chat` aus `kimi.json` entfernt hatte.

Beide Wege heben heute unmittelbar das Level. Der Nenner ist gegen Schrumpfen
geschützt, **der Zähler gegen nichts**.

## 4. Die Invariante

```
have = driveable (Code)  ∧  has_sel (Config)  ∧  proven (Laufzeit, frisch, ungebrochen)
```

Entscheidend: **ein Beleg lässt sich nicht durch Editieren erzeugen.** Wer
Selektoren umschreibt, ändert den Hash über die `needs`-Schlüssel und entwertet
damit genau den Beleg, den er sich erschleichen wollte. Das macht den
Zai-`[6/6]`-Fall strukturell unmöglich.

## 5. Beleg-Formen

`Capability` (`capability.rs:35`) bekommt ein Feld `proof: ProofKind`. Die
Beleg-Form gehört in dieselbe Zeile wie `needs`, `driveable` und `attainable`.
Sie steuert außerdem, **was überhaupt verifiziert wird** — statt wie heute ein
Namensmuster auf `selector_key` (§3).

| Fähigkeit | `ProofKind` | Beleg | heute abgedeckt? |
|---|---|---|---|
| `chat` | `Generation` | Probe senden, Dreier-ODER wie `wait_response` (s.u.) | nein |
| `stop_generation` | `Induced` | auf chats Generierung aufgesattelt: Stop sichtbar → Klick → weg | nein |
| `new_chat` | `Navigation` | `get_conversation_ref` vorher ≠ nachher — **nach** dem chat-Beleg, sonst gibt es nichts zu verlassen (s.u.) | nein |
| `reasoning_toggle` | `RoundTripToggle` | `operations::verify_surface` | **ja** |
| `web_search` | `RoundTripToggle` | `operations::verify_surface` | **ja** |
| `model_switch` | `RoundTripMenu` | `list_menu` → Eintrag ≠ aktuell → hin → zurück | **ja** (Namensfilter trifft `switch`) |
| `reasoning_effort` | `RoundTripMenu` | `select_in_menu_path` mit konfiguriertem Pfad (s.u.) | nein — Filter greift nicht |
| `projects` | `Navigation` | `open_section` URL-Paar; Rückkehr ist Aufräumen, nicht Beleg | nein |
| `mode_switch` | `RoundTripSegment` | `select_segment`; bleibt Quest ohne Marker (`capability.rs:125`) | **ja** |
| `deep_research`, `canvas`, `regenerate`, `temporary_chat` | — | `driveable: false` | — |
| `file_attach` | — | `attainable: false` | — |
| `voice_input`, `voice_mode` | — | `attainable: false`; Beleg-Form erst nach einem bestandenen Fake-Audio-Lauf bestimmen, §13 | — |

### Was belegt wird: die Fähigkeit wie konfiguriert

`Proposal.selector` (`brain_probe.rs:86`) ist **ein** Selektor, die Konfiguration
speichert pro Schlüssel eine Fallback-Liste. Der Verifier löst deshalb **erst wie
im Betrieb auf** und belegt den Gewinner.

Sonst entstünde die Unwahrheit in die andere Richtung: ein Brain, das produktiv
einwandfrei läuft, weil der dritte Fallback greift, bekäme ein `FAIL` für den
ersten. Das Level behauptet „der Harness kann das fahren" — und der Harness
benutzt Fallbacks.

**Der Produktiv-Resolver ist `js_scan`** (`browser/js.rs:58`), nicht
`select_with_fallback`. Letzteres matcht Strings gegen einen DOM-Schnappschuss
(`html.contains(selector)` plus Substring-Heuristik) und ist nicht der Weg, auf
dem geklickt wird. `js_scan` erzeugt eine JS-Schleife über `S[i]`, die beim
ersten Treffer zurückkehrt — der Index liegt vor, wird aber nicht mitgegeben.

Dafür braucht es eine kleine Erweiterung: **`js_scan_indexed`** liefert
`{i: <index>, v: <wert>}` statt nur den Wert. Damit hält der `ProofRecord` fest,
welcher Fallback wirklich getragen hat. Ohne dieses Feld bliebe unsichtbar, wenn
ein Brain nur noch über den letzten Eintrag der Kette läuft — und genau das ist
das Frühwarnzeichen für Selektor-Drift.

### Der Untermenü-Pfad ist Konfiguration, kein Beleg

`select_in_menu_path` (`browser/ui.rs:51`) nimmt `path: &[&str]` wie
`["Aufwand", "Hoch"]` und klickt sich Stufe für Stufe durch. Es gibt **keine
Pfad-Entdeckung**: `list_menu` listet nur die erste Ebene, Untermenü-Einträge
existieren im DOM erst nach dem Elternklick.

Der Pfad kommt deshalb als Selektor-Schlüssel **`reasoning_effort_path`** in die
JSON und wird in `cap.needs` aufgenommen. Drei Gewinne auf einmal:

- fehlt er, meldet `has_sel` ehrlich `NeedsSelectors` statt still durchzufallen
- er geht in den Selektor-Hash ein — eine Pfadänderung entwertet den Beleg
- er ist ein **Parameter**, kein Beleg; die Regel „kein JSON-Eintrag wird zum
  Level" bleibt unberührt, weil der Round-Trip trotzdem gefahren werden muss

Zur Laufzeit entdecken wäre die Alternative, hieße aber, sich blind durch fremde
Menüs zu klicken — was ungewollt echte Einstellungen ändern kann. Und fest im
`CATALOG` ginge nicht: „Aufwand" ist claudes deutschsprachige Beschriftung, qwen
benennt seine Stufen anders.

### Warum Round-Trip

`select_in_menu_path` (`browser/ui.rs:63`) und `select_in_menu`
(`browser/ui.rs:448`) geben bei bereits gesetztem Ziel `Ok("… bereits aktiv,
kein Wechsel noetig")` zurück. Der Kommentar bei `browser/ui.rs:446`
dokumentiert den Vorfall: ChatGPT meldete „umgestellt", obwohl der Name schon
dastand.

Der Round-Trip löst das **strukturell**: der Verifier wählt einen Eintrag, der
nachweislich ≠ aktuell ist — „bereits aktiv" kann auf dem Hinweg nicht
auftreten. Meldet der **Rückweg** „bereits aktiv", war der Hinweg wirkungslos:
`Failed`.

### Misslungener Rückweg ist kein Beweisfehler

`brain_probe::Verdict` kann `proven: true` bei `restored: Some(false)` liefern.
Das zählt als **`Passed`**, und der Restore-Fehler geht sichtbar ins
`evidence`-Feld.

Der Hinweg hat einen Zustandswechsel nachgewiesen — die Fähigkeit ist damit
gezeigt. Ein misslungener Rückweg ist ein Aufräumfehler, kein Beweisfehler;
daraus „kann nicht" zu machen wäre falsch. Sichtbar bleiben muss er trotzdem,
weil das Konto verändert zurückbleibt.

### Die Generation-Sequenz — drei Belege aus einem Lauf

```text
1. Probe senden, dann selbst pollen:
       (count, text, stop) = probe_generation(assistant_js, stop_js, -1)
   Beleg fuer `chat`, sobald EINES zutrifft (wie backend.rs:181):
       count > baseline  ||  (has_stop && stop)  ||  text != baseline_text
2. waehrend die Antwort laeuft: stop sichtbar -> Klick
   -> stop weg UND text waechst nicht weiter               -> belegt `stop_generation`
   alle ~2s detect_block_banner (backend.rs:186-190)       -> Unreachable, s.u.
3. JETZT new_chat klicken, get_conversation_ref vorher/nachher -> belegt `new_chat`
   (fehlt der Selektor: uebersprungen, new_chat bleibt Quest)
```

Eine Sequenz, ein Junk-Eintrag, drei Belege.

> **Korrektur nach der ersten Messung (2026-08-09).** Ursprünglich stand
> `new_chat` an Position 1 — „damit die Probe in einem frischen Gespräch landet".
> Das war falsch: **8 von 8 Brains meldeten `Failed`**, immer mit
> „neuer Chat ohne URL-Wechsel — kein Beleg", und zwar ohne einen einzigen
> Selektorfehler (chatgpt `a[href='/']`, claude `a[href='/new']`, mistral
> `a[href='/chat']`, zai `#sidebar-new-chat-button` wurden alle sauber
> getroffen).
>
> Der Grund: `get_conversation_ref` ist schlicht `driver.current_url()`
> (`operations.rs:94-102`). Die Verify-Sitzung startet auf `brain_url` — das
> **ist** bereits der neue Chat. Es gibt keine Konversation zu verlassen, also
> ändert sich die URL nicht. Der Beleg wurde zum einzigen Zeitpunkt erhoben, an
> dem er nicht funktionieren kann.
>
> Nach dem `chat`-Beleg existiert dagegen eine Konversation mit eigener URL, und
> der Wechsel ist echt. Kriterium und Code bleiben unverändert — nur die Position
> wandert. Nebeneffekt: das Konto bleibt danach auf einem leeren neuen Chat
> stehen statt im Junk-Gespräch.

**Warum das Dreier-ODER und nicht nur der Zähler.** `wait_response`
(`backend.rs:173-181`) bricht bei *einem* von drei Signalen ab; der Kommentar
dort sagt, Trigger (c) fange Brains ab, „deren Zähler nicht inkrementiert
(Container-Selektor / bestehende Konversation)". Genau dafür existiert
`baseline_text` (`browser/mod.rs:310-312`). Ein Beleg allein auf
`assistant_count` würde diese Brains fälschlich auf `Failed` setzen — was
produktiv als Antwortbeginn gilt, muss auch als Beleg gelten.

**Warum ein eigener Poll statt `wait_response`.** Dessen Phase 2 wartet bis zur
Vollständigkeit — danach ist nichts mehr zu stoppen, und der `Induced`-Beleg
entfiele. Der Beleg braucht Phase-1-Semantik plus einen Eingriff mittendrin.
`probe_generation` liefert `(count, text, stop)` in einem einzigen Roundtrip,
der Poll ist also billig.

**Die Baseline gibt es geschenkt.** `prepare_send_baseline()`
(`browser/mod.rs:1279-1288`) liest `assistant_count()` und legt den Text der
letzten Nachricht in `baseline_text` ab; der Send-Pfad gibt den Zähler zurück
(`mod.rs:1270`). Der Beleg braucht also nur `let baseline = backend.send(probe)?`
— beide Vergleichswerte des Dreier-ODER stehen danach bereit.

**Schlägt `send` fehl, ist `chat` `Failed`.** `send` liefert
`Err(submit_failed_error(4))`, wenn nach vier Versuchen (Klick auf
`send_button` und Enter im Wechsel) kein Absende-Beweis vorliegt
(`mod.rs:1274-1276`). Absenden **ist** die Fähigkeit; ohne Beweis sind
`composer` oder `send_button` kaputt. Das ist ein Selektorbefund, kein externer
Zustand — der Kommentar dort nennt es selbst eine „Vergiftungsquelle", wenn man
es durchgehen lässt.

**Timeout:** `resolve_timeout("wait_response", brain_id, probe, None)`, dieselbe
Auflösung wie produktiv (`relay.rs:36`). Sie kennt brain- und promptabhängige
Werte und die gemessenen p95 aus `timeouts.rs`. Praktisch greift sie selten, weil
der Poll beim **ersten** Signal abbricht, nicht bei Vollständigkeit. Eine eigene
Beleg-Frist wäre eine zweite Zahl, die auseinanderläuft.

**Blockade schlägt Timeout.** Im Poll alle ~2 s `detect_block_banner` prüfen,
wie es `wait_response` tut. Ein Tageslimit wird damit `Unreachable` mit
`blocked` bzw. `rate_limit` im evidence — nicht `Failed`. Ohne das sähe ein
limitierter Brain aus, als sei `chat` kaputt, und der bestehende Beleg würde
entzogen (`CONVENTIONS.md:99`: extern Blockiertes flaggen, nicht als Fehler
werten).

**`stop_generation`: zwei Befunde auseinanderhalten.**

| Beobachtung | Urteil | Warum |
|---|---|---|
| `stop` war über die ganze Generierung **nie** sichtbar | `Failed` | der Selektor ist falsch — `backend.rs:286-287` nennt das selbst „Dekoration" |
| `stop` war sichtbar, Antwort endete vor dem Klick | `Unreachable` | Aussage über den Lauf, nicht über die Fähigkeit |

Das verfeinert die frühere Entscheidung, die beides als `Unreachable` geführt
hätte — ein nachweislich toter Stop-Selektor bliebe dabei unentdeckt.

Der Probe-Prompt:

```
webagent capability probe <stamp> — zaehle langsam von 1 bis 100, je Zahl eine Zeile.
```

Deterministisch lang: der `chat`-Beleg braucht eine wachsende Antwort, der
`Induced`-Beleg eine, die lange genug läuft, um sie zu unterbrechen. Ein
„antworte nur mit OK" endete zu schnell für den Stop-Klick. Der Eintrag
**bleibt stehen** — nachträgliches Aufräumen wäre ein Eingriff ohne Beleg,
`temporary_chat` deklariert nur qwen und ist nicht `driveable`. Deshalb ist der
Prompt selbstbeschreibend und trägt einen Stempel.

## 6. Neues Modul: `src/capability_proof.rs`

Rein rechnender Teil: Store, Hash, Frist, Zustand. Kein Browser. Die
browserfahrende Verifikation liegt getrennt (§8) — dieselbe Trennung, die
`brain_probe.rs` im Modulkopf für sich beschreibt.

**`brain_probe::Verdict` ist die Messung, dieser Typ das Urteil.** Deshalb heißt
das Enum hier `ProofOutcome` und nicht `Verdict` — der Name ist vergeben, und
die beiden meinen Verschiedenes: die Messung sagt, was die Oberfläche tat; das
Urteil sagt, was das für das Level bedeutet.

```rust
/// Ein Verifikationsbefund. Append-only, eine Zeile JSON pro Lauf.
pub struct ProofRecord {
    pub brain_id: String,
    pub capability: String,
    pub ts: String,                  // crate::now_rfc3339()
    pub outcome: ProofOutcome,
    /// Hash über die `needs`-Selektoren dieser Fähigkeit zum Zeitpunkt des Laufs.
    pub selector_hash: u32,
    /// Welcher Eintrag der Fallback-Kette getragen hat — aus `js_scan_indexed` (§5).
    /// `None` bei Beleg-Formen ohne Selektorauflösung.
    pub winning_selector: Option<String>,
    /// Klartext aus der `Measurement` (before -> after, restored) bzw.
    /// bei Unreachable der externe Zustand. **Festes Vokabular**, damit es
    /// auszählbar bleibt — dieselben Wörter, die `backend_status` führt:
    /// `blocked`, `rate_limit`, `cloudflare`, `logged_out`, `start_failed`,
    /// `circuit_open`.
    pub evidence: String,
    pub latency_ms: u64,
}

/// Was an der Oberfläche gemessen wurde — quellenunabhängig.
///
/// Alle sechs Beleg-Formen liefern diesen Typ, damit es **einen** Weg in den
/// Store gibt. `brain_probe::Verdict` bekommt ein `From<&Verdict>`; die
/// Round-Trip-Messung wird dadurch konvertiert, nicht nachgebaut.
///
/// Bewusst hier und nicht in `brain_probe`: dieses Modul ist als rein rechnend
/// zugesichert (§6) und darf nicht von einem Typ des Browser-Umfelds abhängen.
pub struct Measurement {
    pub capability_key: String,
    pub before: String,
    pub after: String,
    pub proven: bool,
    /// Ausgangszustand wiederhergestellt? `None`, wenn nichts zu widerrufen war.
    pub restored: Option<bool>,
    pub note: String,
    /// Welcher Eintrag der Fallback-Kette getragen hat, falls aufgelöst wurde.
    pub winning_selector: Option<String>,
}

pub enum ProofOutcome {
    Passed,
    Failed,
    /// Keine Aussage über die Fähigkeit möglich — entzieht nie einen Beleg.
    Unreachable,
}

pub enum ProofState {
    Proven { at: String },
    Expired { at: String, reason: ExpiryReason },
    Failed { at: String, evidence: String },
    Never,
}

pub enum ExpiryReason { TtlElapsed, SelectorsChanged }

/// Hash über genau die Selektorlisten, die diese Fähigkeit laut CATALOG braucht.
/// Kanonische Reihenfolge (`cap.needs`), damit die Reihenfolge in der JSON egal ist.
pub fn selector_hash_for(cap: &Capability, sel: &serde_json::Value) -> u32;

/// Baut einen Record aus einer Messung. **Der einzige Weg in den Store** —
/// für `webagent verify` wie für `probe --verify` (§8).
pub fn record_measurement(brain_id: &str, m: &Measurement, outcome: ProofOutcome, hash: u32, ms: u64);

pub fn record_proof(rec: ProofRecord);
fn record_proof_at(rec: ProofRecord, path: &PathBuf);   // Tests

pub fn proof_state(brain_id: &str, capability: &str, current_hash: u32) -> ProofState;
fn proof_state_at(brain_id: &str, capability: &str, current_hash: u32, path: &PathBuf) -> ProofState;
```

`ProofOutcome::Unreachable` beschreibt die **Beweislage**, das `evidence`-Feld
den **externen Zustand**. Der Wert heißt bewusst nicht `Blocked`:
`CONVENTIONS.md:99` führt „blocked" für extern Blockiertes
(Tageslimit/Login/Cloudflare), unser Fall umfasst zusätzlich „Browserstart
fehlgeschlagen" und „Generierung war zu kurz". Das feste Vokabular im
`evidence`-Feld bleibt trotzdem auszählbar — das ist die Drift-Historie.

Store: `data_dir().join("capability").join("proofs.jsonl")`, `WRITE_LOCK` wie in
`brain_score.rs`.

**Der Store liegt nicht im Repo** und braucht deshalb auch keinen
`.gitignore`-Eintrag: `data_dir()` (`config.rs:83-88`) löst auf
`webagent_root_stable().join("data")` auf, also `%LOCALAPPDATA%\webagent\data`.
Das ist ohnehin richtig so — ein Beleg gilt für DIESEN Account in DIESER
Browserumgebung; committete Belege wären fremde Behauptungen über die eigene
Maschine.

Unter `cargo test` zeigt `data_dir()` auf `temp_dir()/webagent_test_data`
(`config.rs:84-86`). Tests sind damit **von Haus aus** vom Betrieb getrennt —
der Kommentar dort nennt den Anlass: im Score-Log standen real Einträge mit
`brain_id: "a"` neben den echten Brains. `record_proof_at(path)` bleibt trotzdem
sinnvoll, aber für Determinismus innerhalb eines Tests, nicht für die Isolation.

### Lese-Semantik (`proof_state_at`)

```text
records = alle Zeilen für (brain_id, capability), in Schreibreihenfolge
records = records ohne Unreachable                 # keine Aussage über die Fähigkeit
last = records.last() else return Never

match last.outcome:
    Failed  -> Failed { at, evidence }             # letztes Urteil gewinnt
    Passed  ->
        if last.selector_hash != current_hash -> Expired { SelectorsChanged }
        if age(last.ts) > ttl()                -> Expired { TtlElapsed }
        else                                   -> Proven { at }
```

**Letztes Urteil gewinnt, unabhängig von der TTL.** Alles andere hieße, ein
Level zu halten, dessen Bruch man gerade beobachtet hat.

### Verfall

- **Selektor-Hash** über die `needs`-Schlüssel entwertet handgeschriebene
  Ansprüche sofort. Bewusst nicht die ganze Datei: ein neuer Fallback für
  `canvas_button` darf den `chat`-Beleg nicht wegwerfen.
- **TTL** fängt Drift der Website, die im Selektor-File keine Spur hinterlässt.
  `WEBAGENT_PROOF_TTL_DAYS`, Default **14**, nach dem Muster von
  `circuit_breaker::cooldown_secs()`.

`fnv1a` liegt heute privat in `config.rs:1553` (`u32`). Für den Beleg-Hash auf
`pub(crate)` heben — **nicht kopieren**.

## 7. Änderungen an `capability.rs`

```rust
pub struct Capability {          // heute capability.rs:35
    pub proof: ProofKind,        // neu — steuert auch die Verify-Auswahl (§8)
}

pub enum QuestBlocker {          // heute capability.rs:405
    NeedsCode, NeedsSelectors, NeedsBoth,
    NeedsProof,      // neu: Code + Selektoren da, nie verifiziert
    ProofExpired,    // neu: lief mal, TTL abgelaufen oder Selektoren geändert
}
```

**`level_from_selectors` (`capability.rs:538`) bleibt rein.** Die IO wandert in
einen injizierten Lookup, analog zum `_at(path)`-Muster:

```rust
pub fn level_from_selectors_with(
    brain_id: &str,
    sel: &serde_json::Value,
    lookup: &dyn Fn(&str, &str, u32) -> ProofState,
) -> BrainLevel;

pub fn level_from_selectors(brain_id: &str, sel: &serde_json::Value) -> BrainLevel;
```

### Beide Wege zu `have` gaten — nicht nur den offensichtlichen

1. `capability.rs:561` `match (cap.driveable, has_sel)` — der Normalfall.
2. `capability.rs:576-583` — bei **unvermessenem** Brain (kein `ui_options`)
   wird `have` *überschrieben* mit einem Selektor-Scan über den ganzen `CATALOG`,
   und `quests` wird geleert.

Pfad 2 ist keine Randnotiz: `selectors/gemini.json` hat leere `ui_options`.
Würde nur Pfad 1 gegatet, behielte ausgerechnet das kaputte Brain sein
selektor-basiertes Level.

**Beide Pfade bekommen dieselbe Bedingung.** `quests.clear()` in Pfad 2
**bleibt** — ohne `ui_options` weiß niemand, was fehlt; der Grund wird über das
neue `verified`-Feld sichtbar.

### Anzeige: die Zahl bleibt 0, das Wort trägt die Ehrlichkeit

`CONVENTIONS.md:106` verlangt „unbekannt, nicht schlecht". Ein nacktes `[0/7]`
sagt aber „kann nicht". Auflösung ohne Umbau: `level()` bleibt die Zahl der
**bewiesenen** Fähigkeiten — 0 ist dann faktisch wahr. Die Unterscheidung
tragen Label und Rang:

- `label()` (`capability.rs:458`) → `claude [0/7 · 5 unbewiesen]`
- `rank()` (`capability.rs:473`) → `"unbewiesen"` statt `"stumm"`

`rank()` kennt mit `"unvermessen"` bereits dasselbe Muster für den Nenner; das
ist die symmetrische Ergänzung für den Zähler. `BrainLevel` bekommt zusätzlich
`verified: Vec<(String, String)>`.

**Kein Verhaltensrisiko.** Alle Aufrufer von `level_of`/`levels_all` sind reine
Anzeige (`brains_health.rs:59`, `commands/ui.rs:751`). Nichts routet nach Level.

## 8. Verifikation

### Wo die Schleife lebt

`driver` (`browser/mod.rs:309`) und `selectors` (`:299`) sind `pub(crate)` —
crate-interner Code kommt also heran. Die Verify-Schleife wird trotzdem **nicht**
Teil von `capability_proof.rs`: dieses Modul trägt Store, Hash und Frist und
bleibt ohne Treiber testbar, wie §6 zusichert.

Stattdessen dasselbe Muster wie bei `verify_surface`: eine dünne Methode
`WebBrainBackend::verify_capabilities(...)` auf dem Backend, die Logik in einem
neuen Nachbarmodul **`src/browser/verify.rs`**. Die Schleife hält damit **eine**
Sitzung und kommt an Treiber und Selektoren. `commands/ops.rs` nimmt die
`Measurement`s entgegen und macht über `capability_proof::record_measurement`
Records daraus.

### Wiederverwenden, nicht nachbauen

Der Round-Trip existiert. Der Verifier baut aus der **konfigurierten** Fähigkeit
ein `Proposal` (`brain_probe.rs:80`) — `capability_key`, `selector_key`, und als
`selector` den nach §5 aufgelösten Gewinner der Fallback-Kette — und ruft
dasselbe Gerät.

**Wichtig: das innere `operations::verify_surface(driver, proposal)` aufrufen**
(so wie `browser/mod.rs:967`), **nicht** den Wrapper
`WebBrainBackend::verify_surface` (`browser/mod.rs:954`). Der macht `start()`
und `stop()` pro Aufruf — bei 8 Brains × ~6 Fähigkeiten wären das 48
Browserstarts statt 8. Sichtbarkeit von `operations::verify_surface` ggf. auf
`pub(crate)` heben.

Zwei Implementierungen desselben Round-Trips wären genau das, was dieser Plan
bei `fnv1a` ausdrücklich verbietet.

### Auswahl über `ProofKind`, nicht über Namensmuster

Der heutige Filter (`commands/ui.rs:621`) matcht auf `selector_key`. Der
`Verdict` trägt aber bereits `capability_key` — also die Fähigkeit im `CATALOG`
nachschlagen und ihre `ProofKind` entscheiden lassen. Neue Fähigkeiten sind
damit automatisch abgedeckt, und der heutige stille Ausfall von
`reasoning_effort`, `projects` und `new_chat` fällt sofort auf.

### Ablauf

```text
fn verify(brain_id, caps, headless) -> Vec<ProofRecord>:
    if let Some(rest) = circuit_breaker::check(brain_id):        # nur LESEN
        for cap in targets: record(Unreachable, "circuit_open")
        return

    diag = backend.live_diagnose(headless)          # browser/mod.rs:1001
    if diag.is_err() or !diag.logged_in or diag.cloudflare:
        for cap in targets: record(Unreachable, grund_im_vokabular)   # entzieht nichts
        return                                       # EINE Sitzung, dann Schluss

    sel = load_selectors(brain_id)
    for cap in targets in Katalogreihenfolge:
        if !cap.driveable or !has_sel(cap): continue
        winner = resolve_fallback(sel, cap)          # wie im Betrieb
        hash   = selector_hash_for(cap, &sel)
        m: Measurement = match cap.proof {
            RoundTripToggle | RoundTripMenu | RoundTripSegment
                             => operations::verify_surface(driver, &proposal_from(cap, winner))
                                .into(),          # From<&brain_probe::Verdict>
            Navigation       => navigation_proof(driver, cap),
            Generation       => generation_proof(driver),   # liefert auch stop_generation
            Induced          => aus dem Generation-Lauf übernommen,
        }
        record_measurement(brain_id, &m, outcome_from(&m), hash, elapsed)
```

### `Generation` selbst bauen, nicht `relay_single_turn` rufen

`relay.rs:19` `relay_single_turn` macht genau „senden und Antwort abwarten" —
und ist als Baustein trotzdem falsch:

- eigener `WebBrainBackend::from_config` + `start()` (`relay.rs:34-38`) → eine
  **zweite** Browsersitzung neben der, die verify ohnehin offen hält
- bis zu **drei volle Turns** mit `new_chat` dazwischen (`relay.rs:63`) → drei
  Junk-Einträge statt einem
- `circuit_breaker::check` kann den Lauf still verweigern (`relay.rs:27-31`)
- schreibt bei **jedem** Ausgang in `circuit_breaker` und `brain_score`
  (`relay.rs:45, 94, 111, 143-144, 148`)

Stattdessen auf Treiber-Ebene in der bestehenden Sitzung: Composer füllen,
senden, `operations::assistant_count` (`browser/operations.rs:69`) beobachten.
Nur so lässt sich der Stop-Klick für `Induced` **mitten in** die laufende
Generierung setzen — relays Turns sind von außen nicht adressierbar.

Das ist kein Widerspruch zum `verify_surface`-Argument (§8, „wiederverwenden
statt nachbauen"): dort wird ein Round-Trip wiederverwendet, der auf dem
übergebenen Treiber arbeitet. `relay_single_turn` bringt seine eigene Sitzung
und ihre Buchhaltung mit.

### Reihenfolge und Seitenzustand

Die Belege sind nicht unabhängig voneinander — zwei von ihnen verändern, wo der
Browser steht.

**Reihenfolge innerhalb eines Brains:** erst Toggles und Menüs (billig,
zustandslokal, auf der Startseite), dann die Generation-Sequenz (legt einen
neuen Chat an, hinterlässt einen Eintrag), zuletzt Navigation (verlässt die
Seite). Ein abgebrochener Lauf hat damit die meisten Belege schon im Store, und
der teuerste Eingriff passiert zuletzt.

**`open_section` schaltet nicht zurück** (`browser/ui.rs:100-120`): es klickt,
prüft dass `get_conversation_ref` sich geändert hat, und gibt `(before, after)`
zurück. Die Seite steht danach auf `/projects`. Der Verifier navigiert deshalb
im Anschluss selbst zurück (`driver.navigate(brain_url)`).

Der Beleg ist mit dem URL-Paar bereits erbracht — die Rückkehr ist Aufräumen,
derselbe Maßstab wie beim misslungenen Round-Trip-Rückweg: `Passed`, Fehler ins
`evidence`. **Aber der Lauf muss abbrechen**, wenn die Rückkehr misslingt: alle
folgenden Belege liefen sonst auf der falschen Seite und wären wertlos.

### Ein Lauf ohne eine einzige Messung ist ein Fehlschlag

`Unreachable` entzieht nie einen Beleg — das ist richtig und hat sich in der
ersten Messung sofort bewährt (128 von 195 Einträgen waren `start_failed`, die
23 echten Belege darunter blieben unangetastet).

Aber wenn ein Lauf **ausschließlich** `Unreachable` produziert, hat er nichts
gemessen. Das muss laut heraus: deutliche Meldung und **Exitcode ≠ 0**. Sonst
sieht „fertig" aus wie „geprüft" — genau die Selbsttäuschung, gegen die dieser
ganze Plan gebaut ist.

### Verify speist die bestehenden Zähler nicht

`webagent verify` **liest** `circuit_breaker::check` und setzt bei offenem
Breaker `Unreachable` mit evidence `circuit_open` — ein bereits
rate-limitiertes Konto wird nicht auch noch von der Verifikation gehämmert.

Geschrieben wird ausschließlich nach `proofs.jsonl`. Nicht nach `brain_score`:
das misst Sitzungs-Zuverlässigkeit, ein Prüflauf ist keine Sitzung — dieselbe
Vermischung, die `AUTORESEARCH_PLAN.md` §10 gegen die brain_score-Anbindung
eingewandt hat. Und nicht nach `circuit_breaker`: sonst könnte ein
Selektorproblem an einer Nebenfähigkeit den Brain für echte Aufträge sperren.

**Ein-Eintrag-Menü ist `Failed`**, nicht `out_of_reach`: die `model_option`-
Selektoren sind generische Rollen (`[role=menuitem]`), die leicht danebentreffen.
Als `Failed` landet es im Questlog und wird untersucht.

**`Induced`: zwei Befunde, nicht einer.** Endet die Generierung, bevor der
Stop-Klick sitzt, ist das `Unreachable` — kein Beleg, aber auch kein Fehlschlag.
War `stop` dagegen über die **ganze** Generierung nie sichtbar, ist der Selektor
falsch und das Urteil `Failed` (`backend.rs:286-287` nennt so einen Selektor
„Dekoration"). Die vollständige Regel steht in §5.

**Ebenen-Disziplin.** `model_switch` nur erste Menüebene, `reasoning_effort` nur
über `select_in_menu_path`. Trifft der gewählte Eintrag nicht die erwartete
Ebene → `Failed`. Nötig, weil `selectors/claude.json` `model_menu` (Z. 76-81)
und `reasoning_effort_menu` (Z. 105-110) **byte-identisch** hält. Die Datei wird
**nicht vorab repariert** — der erste Lauf soll den Befund liefern.

Eine erzwungene Reihenfolge braucht es nicht: gemessen über alle acht
Selektordateien deklarieren nur `claude` und `qwen` beide Fähigkeiten, bei qwen
sind die Selektoren getrennt.

**Untermenü-Eltern als Falschziel.** Aus derselben Kollision folgt: bei claude
listet `list_menu` auf der ersten Ebene Modellnamen *und* den Untermenü-Eltern
(„Aufwand"). Wählt der Round-Trip den als Ziel, geht ein Untermenü auf statt
eines Modellwechsels — `menu_label` trägt danach nicht den Zielnamen, der Beleg
wird also ohnehin `Failed`. Die Mechanik greift; nötig ist nur eine lesbare
Begründung:

> `Eintrag 'Aufwand' geklickt, Beschriftung unveraendert — moeglicherweise ein
> Untermenue-Eltern statt einer Option`

Ohne diesen Hinweis sucht jemand den Fehler beim Modellwechsel statt bei der
Menüstruktur. Eine Vorab-Erkennung über `aria-haspopup` oder ein Chevron wäre
eine weitere Heuristik gegen DOM-Struktur — genau die Art Annahme, die in diesem
Projekt reihenweise gebrochen ist.

### Änderungen am Prober

- **`FAIL` wird nicht mehr geschrieben.** Ein Selektor, von dem gemessen wurde,
  dass er nichts schaltet, hat im operativen File negativen Wert — `sel()`
  probiert ihn zuerst. Schlimmer: `.or_insert_with()` (`commands/ui.rs:667`)
  überschreibt bestehende Schlüssel nie, ein kaputter Eintrag sperrt die Stelle
  also **dauerhaft** gegen bessere Funde. Er wird gemeldet, nicht abgelegt.
- **`probe --verify` schreibt in denselben Store.** Ein Beleg ist ein Beleg,
  unabhängig vom Befehl, der ihn erzeugt hat. Zusammen mit „FAIL nicht
  schreiben" ist der geschriebene Selektor genau der belegte — `probe --verify
  --write` sät damit sofort gültige Belege, und der erste `verify`-Lauf fängt
  nicht bei null an. Kein `source`-Feld: solange beide denselben Round-Trip
  fahren, ist die Herkunft ohne Bedeutung.

### `--cap` zieht Abhängigkeiten mit

`Induced` hat keinen eigenen Lauf — `stop_generation` wird während der
Generation-Sequenz belegt. `webagent verify --cap stop_generation` fährt deshalb
die ganze Sequenz und schreibt Belege für **beide** Fähigkeiten. `chat` wurde
dabei tatsächlich gemessen; den Beleg wegzuwerfen wäre dieselbe Verschwendung,
die dieser Plan behebt. Im Log sichtbar machen, dass mehr lief als angefordert.

### Brains nacheinander

Ein Lauf fährt ein Brain nach dem anderen, eine Browsersitzung zur Zeit.
Vorhersehbar, lesbare Ausgabe, keine Konkurrenz um Profile. `verify` ist ein
bewusst ausgelöster Diagnoselauf, kein Durchsatzproblem — Parallelität über den
`BrowserPool` wäre möglich (`start_brain_encapsulated` isoliert Profile bereits,
acht Worker liefen nachweislich gleichzeitig), brächte aber Pool-Zustands-
maschine, Heartbeats und Speicherlast in einen Pfad, der davon nichts braucht.
Nachrüsten, wenn die Laufzeit wirklich stört.

## 9. Auslösung

`webagent verify [--brain X] [--cap Y] [--headless]` in `commands/ops.rs` —
dort liegen die Operationen (`cmd_canary:68`, `cmd_doctor:661`);
`commands/ui.rs` trägt die Anzeige (`cmd_survey:325`, `cmd_quests:750`).

`quests`, `survey` und die TUI lesen nur den Store und melden `ProofExpired` —
sie starten **nie** einen Browser. Eine Anzeige, die als Nebenwirkung acht
Logins hochfährt, ist eine Falle.

## 10. Entschiedene Fragen

Keine offene Design-Frage mehr. Wer etwas ändert, ändert es gegen diese
Begründungen.

| Frage | Entscheidung | Warum |
|---|---|---|
| Was zählt als bestandener Lauf? | Round-Trip | schließt „bereits aktiv" strukturell aus |
| Verfällt ein Beleg? | TTL **und** Selektor-Hash | Handschrift vs. Website-Drift |
| Wo liegt der Beleg? | eigener JSONL-Store unter `data_dir()` | ein Beleg in der Selektordatei wäre wieder ein JSON-Eintrag; `data_dir()` liegt außerhalb des Repos, kein `.gitignore` nötig |
| Gilt die Pflicht für `chat`? | ja, für alle | zwei Klassen von Wahrheit wären keine |
| Wo steht die Beleg-Form? | Feld im `CATALOG` | ein Ort pro Fähigkeit |
| `stop_generation`? | auf chats Generierung aufsatteln | eine Generierung, ein Junk-Eintrag |
| Menü-Kollision claude? | hart fehlschlagen, nicht vorab fixen | der erste Lauf liefert den Befund |
| Wer löst aus? | nur `webagent verify` | Anzeige darf keine Browser starten |
| Fehlschlag bei gültigem Beleg? | letztes Urteil gewinnt | kein Level halten, dessen Bruch man sah |
| Hash-Umfang? | nur die `needs`-Schlüssel | präzise Entwertung |
| Belege committen? | nein, pro Maschine | fremde Accounts, fremde Modelle |
| `attainable` abschaffen? | nein | `file_attach` machte jeden Nenner unerreichbar |
| Ausgeloggt = Fehlschlag? | nein, `Unreachable` | deckt sich mit `CONVENTIONS.md:99` |
| QuestBlocker-Körnung? | `NeedsProof` + `ProofExpired` | verschiedene Handlungen |
| TTL-Wert? | 14 Tage, env-stellbar | kürzer wird abgeschaltet |
| Erste Stufe? | Store + Gate, ohne Verifier | wahrer Zustand vor Browsercode |
| Unvermessener Zweig? | mitgaten, `quests` leer | sonst Hintertür bei gemini |
| Chat-Junk? | stehen lassen, Probe kenntlich | Aufräumen wäre Eingriff ohne Beleg |
| `Induced` ohne Gelegenheit? | `Unreachable` — aber „nie sichtbar" ist `Failed` | Aussage über den Lauf vs. toter Selektor |
| Ein-Eintrag-Menü? | `Failed` mit eigenem Grund | verdeckt sonst zu engen Selektor |
| Anzeige unbewiesen? | Zahl 0, Wort „unbewiesen" | erfüllt `CONVENTIONS.md:106` ohne Umbau |
| Probe-Prompt? | deterministisch lang (1..100) | `chat` und `Induced` aus einer Generierung |
| `Unreachable`-Vokabular? | Wert bleibt, Grund im Bestandsvokabular | zwei Ebenen |
| Branch? | direkt auf `evolution/supervised-harvest` | vier Seitenäste hängen 61 Commits zurück |
| Typkollision `Verdict`? | Messung bleibt, Urteil heißt `ProofOutcome` | die beiden meinen Verschiedenes |
| Round-Trip nachbauen? | nein, `operations::verify_surface` wiederverwenden | zwei Implementierungen driften |
| `FAIL` schreiben? | nein | `.or_insert_with()` sperrt die Stelle dauerhaft |
| Verify-Auswahl? | über `ProofKind`, nicht Namensmuster | neue Fähigkeiten automatisch abgedeckt |
| Ein Selektor oder Fallback-Kette? | wie konfiguriert auflösen, Gewinner belegen | das Level behauptet, was der Harness tut |
| `probe --verify` in den Store? | ja, ein Store | ein Beleg ist ein Beleg |
| Rückweg misslungen? | `Passed`, Restore-Fehler ins evidence | Aufräumfehler ≠ Beweisfehler |
| Woher der Gewinner-Selektor? | `js_scan_indexed` ergänzen | der Index liegt in der Schleife schon vor |
| `Generation` über relay? | nein, Treiber-Ebene in eigener Sitzung | relay bringt zweite Sitzung, 3 Turns und eigene Buchhaltung |
| Verify und die Zähler? | Breaker lesen, nichts schreiben | ein Prüflauf ist keine Sitzung |
| `new_chat` mitbelegen? | ja, aber **nach** dem chat-Beleg | als Auftakt 8/8 `Failed` gemessen: auf der frischen Seite gibt es keine Konversation zu verlassen |
| Woran hängt der `chat`-Beleg? | Dreier-ODER wie `wait_response`, eigener Poll | Zähler allein setzt Container-Selektor-Brains falsch auf FAIL |
| Stop nie sichtbar? | `Failed` (Dekoration), zu spät = `Unreachable` | toter Selektor bliebe sonst unentdeckt |
| Rate-Limit während der Probe? | `detect_block_banner` → `Unreachable` | extern blockiert ist kein Tool-Defekt |
| Wo lebt die Verify-Schleife? | `browser/verify.rs` + Methode am Backend | `capability_proof.rs` bleibt ohne Treiber testbar |
| Untermenü-Pfad woher? | Selektor-Schlüssel `reasoning_effort_path` in `needs` | fehlt er, meldet `has_sel` ehrlich `NeedsSelectors` |
| Seite nach `open_section`? | selbst zurücknavigieren, sonst Lauf abbrechen | folgende Belege liefen sonst auf der falschen Seite |
| Reihenfolge im Brain? | Toggles/Menüs → Generation → Navigation | teuerster und seiteneffektreichster Eingriff zuletzt |
| Untermenü-Eltern als Ziel? | läuft in `Failed`, Meldung präzisieren | keine neue DOM-Heuristik; die Mechanik greift schon |
| `send` schlägt fehl? | `Failed` | Absenden ist die Fähigkeit, nicht ihr Umfeld |
| Timeout des Polls? | `resolve_timeout("wait_response", …)` | eine zweite Frist liefe auseinander |
| `--cap` mit Abhängigkeit? | Träger mitziehen, beide belegen | der Beleg wurde erbracht, wegwerfen wäre Verschwendung |
| Acht Brains parallel? | nein, nacheinander | Diagnose braucht keinen Durchsatz |
| Welcher Typ in den Store? | neutrales `Measurement`, `From<&Verdict>` | ein Weg in den Store; `capability_proof` bleibt frei von Browser-Typen |
| ProofKind für `voice_*`? | keine, bis Fake-Audio besteht | `attainable: false` braucht keine Beleg-Form |
| `stop_generation` 4× `Failed`? | echter Befund, Selektoren nacharbeiten | Icon-only-UIs tragen kein `aria-label`; genau dafür ist das Dekorations-Kriterium da |
| Lauf nur mit `Unreachable`? | laute Meldung, Exitcode ≠ 0 | „fertig" darf nicht wie „geprüft" aussehen |
| `ProofKind::None`? | ja, für `driveable: false` | im Plan übersehen, in der Umsetzung korrekt ergänzt |

Vom Code beantwortet statt gefragt: `list_menu` schließt das Menü selbst
(`ui.rs:397` drückt Escape vor dem Return), die Sequenz `list_menu` →
`select_in_menu` toggelt es also nicht versehentlich zu; erzwungene Reihenfolge zwischen
`model_switch`/`reasoning_effort` unnötig; `Induced`-Piggyback überall machbar
(alle acht Brains haben `stop_button`); Phase 2 ohne Verhaltensrisiko (alle
Level-Aufrufer sind Anzeige); den Wrapper `verify_surface` meiden (start/stop
pro Aufruf); `select_with_fallback` ist nicht der Produktivpfad (§5).

## 11. Teststrategie

- **Store**: Unit-Tests über `record_proof_at` / `proof_state_at` gegen ein
  `tempdir` — Muster `unique_path()` in `circuit_breaker.rs`/`code_score.rs`.
  Insbesondere „Passed, dann Failed" → `Failed`, „Passed, dann Unreachable" →
  weiterhin `Proven`, und „proven mit restored=Some(false)" → `Passed`.
- **TTL**: `WEBAGENT_PROOF_TTL_DAYS=0`, echte Uhr — so wie `circuit_breaker`
  seine Fristen testet.

  **Achtung beim `env_guard`-Muster** (`timeouts.rs:166-170`): das ist **nur ein
  Serialisierungs-Lock**, kein RAII-Guard, der die Variable zurücksetzt. Der
  Kommentar dort sagt es — „Alle env-empfindlichen Tests laufen daher über dieses
  Lock seriell." Setzen und Aufräumen bleibt Sache des Tests; wer das übersieht,
  lässt `WEBAGENT_PROOF_TTL_DAYS` stehen und vergiftet die übrigen. Und
  `ENV_LOCK` ist ein privates Static im Test-Modul von `timeouts.rs` —
  `capability_proof` braucht ein **eigenes**.
- **Hash**: gegen `json!`-Fixtures. Ein neuer Fallback in einem
  `needs`-Schlüssel ändert den Hash; ein Eintrag in einem fremden Schlüssel
  **nicht**. Schlüsselreihenfolge egal.
- **Gate**: `level_from_selectors_with` mit Fake-Lookup. Beide Pfade aus §7 —
  insbesondere ein Brain **ohne** `ui_options`, das trotz Selektoren auf 0 steht.
- **`shipped_brains_can_all_at_least_chat`** (`capability.rs:880`) wechselt auf
  `level_from_selectors_with(id, &sel, &always_proven)` und **behält seine
  Assertion**. Er prüft ausgelieferte Daten (`shipped_selector_table()`), nicht
  die Platte; mit injiziertem „alles bewiesen" misst er weiterhin genau das,
  wofür er gebaut wurde, und bleibt ohne Browser lauffähig.
- **`js_scan_indexed`**: reine String-Erzeugung, testbar ohne Browser — das
  erzeugte JS muss `{i,v}` liefern und bei keinem Treffer den Default. Gegen
  `MockPageDriver` zusätzlich: trifft erst der dritte Selektor, muss `i == 2`
  zurückkommen.
- **Adapter**: `proposal_from(cap, winner)` rein testbar — aus einer
  Selektordatei mit vier Fallbacks muss der aufgelöste Gewinner im Proposal
  landen, nicht der erste Eintrag.
- **Round-Trip**: nicht neu testen. `brain_probe.rs:1022-1024` deckt ihn ab.
- **Generation-Sequenz** gegen `MockPageDriver` (`src/mock_page.rs`): drei
  Belege aus einem Lauf, und der Fall „kein `new_chat`-Selektor" muss im
  laufenden Thread weiterlaufen statt abzubrechen. Dazu die drei Zweige des
  ODER einzeln: nur Zählerinkrement, nur Textänderung (Container-Selektor,
  Zähler bleibt stehen), nur Stop-Sichtbarkeit — **jeder für sich** muss `chat`
  belegen. Der mittlere ist der, den ein Beleg auf `assistant_count` verlöre.
- **`stop_generation`-Zweige**: Mock, bei dem `stop` nie sichtbar wird → muss
  `Failed` ergeben; Mock, bei dem `stop` sichtbar war und die Antwort vorher
  endet → `Unreachable`. Die beiden dürfen nicht zusammenfallen.
- **Blockade**: Mock liefert ein Limit-Banner statt einer Antwort → `Unreachable`
  mit `blocked` im evidence, **nicht** `Failed`, und der bestehende Beleg bleibt.
- **Seitenzustand**: nach einem `Navigation`-Beleg muss der Mock wieder auf der
  Start-URL stehen. Und der Gegentest: misslingt die Rückkehr, muss der Lauf
  abbrechen statt weiterzumessen — sonst entstehen Belege auf der falschen Seite,
  und die wären schlimmer als keine.
- **Fehlender Pfad**: eine Selektordatei ohne `reasoning_effort_path` muss
  `NeedsSelectors` ergeben, nicht `NeedsProof` — der Unterschied sagt, ob jemand
  konfigurieren oder verifizieren muss.
- **Sendefehler**: Mock, dessen `verify_submitted` nie greift → `chat` muss
  `Failed` werden, nicht `Unreachable`. Sonst verdeckt der Beleg genau den
  Selektorbruch, den zu finden sein Zweck ist.
- **`--cap` mit Abhängigkeit**: `--cap stop_generation` muss **zwei** Records
  schreiben. Der Test, der sonst niemandem auffällt: nur einer geschrieben heißt,
  ein gemessener Beleg wurde weggeworfen.
- **Verify schreibt keine Zähler**: ein Test, der nach einem simulierten
  Fehlschlag prüft, dass `brain_score` und `circuit_breaker` unverändert sind.
  Das ist die Zusicherung, die sonst still bricht, wenn jemand später
  `relay_single_turn` einbaut.
- **Livetest zuletzt**: `webagent verify --brain claude --cap reasoning_effort`.
  Erwarteter erster Befund ist die Menü-Kollision — ein grüner Lauf wäre
  verdächtig.

## 12. Phasenweise Umsetzung

Gearbeitet wird **direkt auf `evolution/supervised-harvest`**, Phase für Phase
committet. Das Repo hat vier Seitenäste (`refactor/thiserror`,
`refactor/tracing`, `refactor/module-split`, `fix/session-writeback`), die alle
61 Commits hinterherhängen — lange Nebenzweige verrotten hier nachweislich.

1. **`capability_proof.rs`**: `ProofRecord`, `ProofOutcome`, `ProofState`,
   Store, `selector_hash_for`, `Measurement`, `record_measurement`, TTL.
   Unit-getestet. Kein
   Browser, kein Diff an `capability.rs`.
2. **Gate scharf schalten**: `ProofKind` ins `CATALOG`, die beiden neuen
   `QuestBlocker`, `level_from_selectors_with`, **beide** Pfade aus §7, Label
   und Rang. Bestehende Tests auf den Fake-Lookup umstellen.

   > **Ergebnis dieser Stufe ist unangenehm und genau der Punkt:** alle acht
   > Brains fallen von zusammen 36 Punkten auf 0. Der wahre Zustand steht im
   > Repo, bevor die erste Zeile Verifier existiert. Verhalten ändert sich
   > nicht — alle Level-Aufrufer sind Anzeige.

3. **Prober anpassen** (`commands/ui.rs`): Auswahl über `ProofKind` statt
   Namensmuster, `FAIL` nicht mehr schreiben, Verdicts per
   `record_measurement` (über `From<&Verdict>`) in den Store. Ab hier erzeugt jeder
   `probe --verify`-Lauf echte Belege — das Level füllt sich wieder, ohne dass
   `verify` schon existiert.
4. **`js_scan_indexed`** in `browser/js.rs` — zwei Zeilen JS, ein Testfall.
   Danach `proposal_from(cap, winner)` mit Fallback-Auflösung. Der Round-Trip
   wird nur aufgerufen, nicht gebaut.
5. **`src/browser/verify.rs`** + Methode `WebBrainBackend::verify_capabilities`:
   die Schleife, die eine Sitzung hält. Darin die Generation-Sequenz auf
   Treiber-Ebene — `new_chat` → senden → eigener `probe_generation`-Poll mit
   Dreier-ODER → Stop-Klick mittendrin → Block-Banner alle ~2 s. Drei
   Beleg-Formen (`Navigation`, `Generation`, `Induced`) aus einem Lauf, gegen
   `MockPageDriver`.
6. **`webagent verify`** in `commands/ops.rs`: Breaker lesen,
   `live_diagnose`-Vorlauf, `Unreachable`-Pfad, Messungen über
   `capability_proof::record_measurement` in den Store, keine Schreibzugriffe
   auf `brain_score`/`circuit_breaker`.
7. **Anzeige**: `cmd_quests`/`cmd_survey`/TUI lesen `ProofState` und melden
   `ProofExpired`; kein Pfad startet einen Browser.
8. **Erster echter Lauf** über alle acht Brains. Befunde in
   `docs/PROVIDER_STATUS.md` **anhängen**, nicht überschreiben.

## 12b. Erste Messung (2026-08-09)

`proofs.jsonl`: 195 Einträge, 04:38–08:08. Wirksamer Stand nach
`proof_state`-Semantik (Unreachable übersprungen):

```
chatgpt 3   claude 4   zai 4   deepseek 3   qwen 3   kimi 2   mistral 2
gemini 1    perplexity 1                                   = 23 Belege
```

Was die Messung erbracht hat:

- **`new_chat` 8× `Failed`** — Beleg-Form war falsch positioniert, siehe §5.
  Kein Selektorproblem.
- **`stop_generation` 4× `Failed`** (deepseek, gemini, kimi, zai) gegen 3×
  `Passed` (claude, mistral, qwen). Echter Selektorbefund, siehe §13.
- **`Unreachable` rettete die Beleglage**: 128 `start_failed` aus einem
  gescheiterten Lauf ließen die 23 Belege darunter unberührt. Wären sie als
  `Failed` gewertet worden, wäre alles weg gewesen.
- **`winning_selector` zahlt sich aus**: sichtbar, dass claude über
  `div.font-claude-response` läuft und zai über
  `div.chat-assistant .markdown-prose` — die Drift-Frühwarnung funktioniert.
- **`perplexity` wurde mitgemessen**, obwohl es beim Planen noch nicht existierte.
  Die `ProofKind`-gesteuerte Auswahl (§8) hat es ohne Zutun aufgenommen.

## 12c. Bestätigung der `new_chat`-Korrektur (2026-08-09)

Stichprobe nach der Umstellung, `webagent verify --brain claude --cap chat
--cap new_chat --cap stop_generation`:

```
chat            = Passed  (141352ms) — chat belegt (stop sichtbar)
stop_generation = Unreachable        — Stop-Klick ohne belegbare Wirkung
new_chat        = Passed  (1048ms)   — URL-Wechsel
```

**`new_chat` ist belegt.** Vorher 8 von 8 `Failed`, ohne dass ein einziger
Selektor falsch war — allein durch die Position in der Sequenz. Die Diagnose aus
§5 trägt.

**Breitentest über sieben weitere Brains** (je eigener Prozess, 16:53–17:06):
`new_chat` ging von **9× `Failed` auf 4 Belege** — claude, chatgpt, zai,
deepseek. Die verbliebenen drei sind zwei verschiedene Fehler, siehe §13:
qwen (34 ms) und kimi (35 ms) durchlaufen nachweislich keinen der beiden Pfade
von `new_chat()` (Klick + 800 ms Schlaf, oder Ersatz-Navigation) — dort greift
der Selektor nicht. gemini brauchte 845 ms, also ein echter Versuch, dessen URL
sich nicht bewegt: `gemini.google.com/app` bleibt `/app`. Nur für diesen einen
Fall reicht das URL-Kriterium wirklich nicht.

Drei Nebenbefunde:

- Der `chat`-Beleg griff über den Trigger **„stop sichtbar"**, nicht über den
  Zähler. Genau der Zweig, den ein Beleg auf `assistant_count` allein verloren
  hätte (§5).
- `stop_generation` kam als `Unreachable` statt wie früher `Passed`. Nach
  „letztes Urteil gewinnt" entzieht das **keinen** Beleg — der frühere `Passed`
  bleibt wirksam, weil `Unreachable` übersprungen wird. Die Regel arbeitet.
- Die Probe (1..100) hält den Poll 141 s offen. Bei 9 Brains ist ein voller Lauf
  entsprechend lang; das ist der Preis einer Generierung, die zuverlässig lange
  genug für den Stop-Klick läuft.

## 13. Folgearbeiten

- **`selectors/claude.json`**: `model_menu` (Z. 76-81) und
  `reasoning_effort_menu` (Z. 105-110) entkoppeln — erst nach dem ersten Lauf.
- ~~**`new_chat_button` für qwen und kimi.**~~ **Erledigt 2026-08-10.**
  `probe --brain <id>` zeigte den Grund: falscher **Elementtyp**. qwen fährt
  ein `div[aria-label*='New Chat' i]`, kimi ein `a[aria-label*='Neuer Chat' i]`
  — konfiguriert waren nur `button`/`a`-Varianten. Beide jetzt `Passed`
  (1092 bzw. 1040 ms). Details in `PROVIDER_STATUS.md`.
  **Nebenbefund mit Folgen:** `%LOCALAPPDATA%\webagent\selectors\<brain>.json`
  überschattet die Repo-Datei. Bei kimi wirkte der Repo-Fix deshalb zunächst
  nicht. `gemini.json` und `perplexity.json` liegen dort ebenfalls — für diese
  beiden Brains greift Selektorpflege im Repo aktuell **nicht**.
- ~~**Zweites `new_chat`-Kriterium für gemini.**~~ **Erledigt 2026-08-10 — und
  die Begründung war falsch.** Angenommen war, `gemini.google.com/app` bleibe
  `/app`. Tatsächlich stand `a[href='/']` an dritter Stelle der Selektorliste,
  traf ein anderes Element und wurde geklickt (daher 845 ms *mit* Klick, *ohne*
  Wirkung). Mit dem gemessenen `a[aria-label*='Neuer Chat' i]` vorn und
  `a[href='/']` ganz hinten: `Passed — URL-Wechsel + Verlauf geleert (1 -> 0)`.
  gemini wechselt die URL sehr wohl.
  Das zweite Kriterium ist trotzdem eingebaut (`new_chat_outcome`,
  `browser/verify.rs`) — als unabhängiges Zweitsignal, nicht als der Fix.
  Schranke `count_before > 0`, sonst würde 0 → 0 jeden wirkungslosen Klick
  belegen.
- ~~**Regression prüfen: deepseek `chat`.**~~ **Erledigt: flüchtig.** Nachlauf
  `Passed` nach 11,4 s. Keine Nebenwirkung des Hygiene-Klicks. Ebenso mistral,
  dessen `start_failed` sich im Einzellauf nicht reproduzieren ließ.
- **Nachgemessen zur Schattendatei-Warnung:** `load_selectors` merged **pro
  Schlüssel**. Betroffen war nur `kimi.json` (12 Schlüssel). `gemini.json`
  führt allein `ui_options`, `perplexity.json` hat gar kein Repo-Gegenstück und
  ist damit die einzige Quelle — die gehört ins Repo, wenn perplexity bleiben
  soll.
- **`stop_button` für deepseek, gemini, kimi, zai.** Alle vier melden „Stop-Button
  nie sichtbar", obwohl plausible `aria-label`-Selektoren hinterlegt sind. Der
  Modulkopf von `capability.rs` nennt den Grund für deepseek: 107 Bedienelemente
  als reine Icon-`div`s **ohne** aria-label, title, id, `data-*` oder Text. Dort
  kann kein aria-label-Selektor greifen. `brain_probe` ist das Werkzeug, um
  Ersatz zu finden — die vier Befunde sind der Ertrag des Dekorations-Kriteriums,
  nicht sein Fehler.
- **`reasoning_effort_path` fehlt überall.** Mit der Aufnahme in `cap.needs`
  wird `reasoning_effort` bei claude und qwen sofort zu `NeedsSelectors` — heute
  zählt es dort als `have`, weil nur das Menü verlangt wird. Das ist die
  ehrliche Meldung: ohne Pfad kann der Harness die Denkstufe nicht ansteuern,
  auch wenn ein Selektor für das Menü existiert. Pfade nachtragen (claude:
  `["Aufwand", "Hoch"]`; qwen benennt seine Stufen anders und muss erst
  nachgesehen werden).
- **`selectors/gemini.json`** hat leere `ui_options` und ist unvermessen.
  Solange gemini ausgeloggt ist, liefert `verify` dort nur `Unreachable`. Das
  ist korrekt und braucht keine Sonderbehandlung, nur eine Anmeldung von Hand.
- **Level-Tabellen sind Maschinenstände.** Die Tabellen in `docs/PARITY.md` und
  `docs/UEBERGABE_2026-07-28.md` beschreiben Storax' Accounts, nicht das
  Projekt. Ein Satz dazu gehört an beide Stellen.
- **`_needs_review` aus `BRAIN_ANALYZE_ADD.md` §7** wurde nie implementiert
  (`git grep needs_review` trifft nur das Dokument). Mit „FAIL nicht schreiben"
  ist die dortige Frage anders beantwortet als damals vorgesehen — das Dokument
  gehört entsprechend nachgezogen.
- **Stale Status-Banner**: `AUTORESEARCH_PLAN.md` sagt „GEPLANT, NICHT
  IMPLEMENTIERT", obwohl `src/autoresearch.rs` mit 51,6 KB steht;
  `BRAIN_ANALYZE_ADD.md` sagt „DESIGN", obwohl `brain_probe.rs` gebaut ist.
  Genau die Falle aus `CONVENTIONS.md:109` („Pläne können stale sein").
- **`voice_input` / `voice_mode`** (`capability.rs:181`): die Begründung für
  `attainable: false` ist durch `webview_runtime.rs:774` `apply_fake_audio_args`
  überholt — der Doc-Kommentar dort sagt selbst, damit werde Spracheingabe
  „ueberhaupt erst **belegbar**". Bleibt `false` bis zu einem bestandenen
  End-to-End-Lauf, dann `ProofKind::Generation` und `attainable: true`.
- **`mode_switch`** (`capability.rs:125`) bleibt Quest: `driveable: false`,
  solange deepseeks Segmente keinen auslesbaren Marker tragen.

## 14. Entscheidungen zum §13-Folgeplan (2026-08-10)

Erarbeitet in einem weiteren `/grill-me`-Durchgang. Zwei §13-Items waren beim
Grill bereits erledigt — der Durchgang hat das aus `proofs.jsonl` + Code
nachgewiesen statt über offene Fragen (siehe unten). Reihenfolge der
**verbliebenen** Arbeit: Selektor-Hunting (`stop_button`, 4 Brains) →
`reasoning_effort` (claude/qwen) → Docs (`_needs_review`-Folgesatz + Level-
Tabellen) → `voice_*` (verschoben). Validierung gezielt pro Brain, voller
9-Brain-Sweep nur als Endvalidierung; thematische Commits direkt auf
`evolution/supervised-harvest`, je `cargo test` + `clippy` grün.

**Bereits erledigt (2026-08-10, uncommitted im Working Tree) — die zugehörigen
Grill-Aste wurden gegenstandslos:**
- **deepseek `chat`-Regression: flüchtig, kein Fix nötig.** `proofs.jsonl`:
  2026-08-09 14:55 `Failed` (108 s, `winning_selector: textarea`) → 2026-08-10
  07:25 `Passed` 11,4 s (`div.ds-markdown`). Keine Nebenwirkung des
  Hygiene-Klicks (so auch §13). Der Hygiene-Klick ist bereits frisch-guardig:
  `browser/verify.rs:566-568` klickt nur, wenn `assistant_count() > 0`.
- **gemini `new_chat`: erledigt.** Echter Fix war die Selektor-Reihenfolge
  (`a[aria-label*='Neuer Chat' i]` vorn, `a[href='/']` ganz hinten); der
  generelle ODER-Arm ist als Zweitsignal gebaut: `new_chat_outcome`
  (`browser/verify.rs:496-516`) = URL-Wechsel ODER
  `count_before > 0 && count_after == 0`, Zweige im `evidence` getrennt,
  Schranke `count_before > 0`. Tests `verify.rs:1010-1037`.

| Frage | Entscheidung | Warum |
|---|---|---|
| Icon-only-Discovery? | `selector_for` um `[class*='…' i]` + `[title*='…']` erweitern (niedrigste Priorität); Rohe-Dump um class/title erweitern | `Candidate`+`haystack` führen beide schon (`brain_probe.rs:46-49`), nur der Emitter (`selector_for`, `brain_probe.rs:341-373`) fehlt — der Pfad ist halbfertig, nicht neu |
| Gewinner in die Kette? | Mensch setzt ihn vorne (js_scan = first-match), nachweislich tote Einträge im selben Commit; Write-Pfad bleibt merge-over-write | Round-Trip bleibt das Gate; die „nie überschreiben"-Sicherheit der Nutzerdatei bleibt unangetastet |
| Stop-Discovery-Fenster? | generierungsbewusster Probe-Modus: Probe senden, beim ersten Stop-Sichtbar `collect()`, Kandidaten drucken | `probe_surface` sendet keine Nachricht; ein Stop-Button existiert nur während der Generierung |
| claude `reasoning_effort`? | Byte-Identität als strukturell korrekt akzeptieren (dasselbe Dropdown öffnet Modelle UND „Aufwand"), nur `reasoning_effort_path: ["Aufwand","Hoch"]` nachtragen, dann Lauf | es gibt keinen separaten Effort-Button auf claude; der Lauf liefert den Befund (§10) |
| qwen `reasoning_effort`? | Discovery, Lauf entscheidet; ist es ein Toggle statt Pfad-Menü → ehrlich `NeedsSelectors` + PROVIDER_STATUS-Notiz | `ProofKind` ist brain-unabhängig; kein erfundener Pfad, kein Katalog-Umbau |
| `voice_*` E2E bauen? | verschieben; nur überholten Katalog-Kommentar (`capability.rs:230`) auf Fake-Audio-Stand nachziehen + Notiz | einziger Brain mit Button (qwen), Cloud-STT-Flakiness, geringster Wahrheitsertrag; eigener grill-me, wenn dran |
| Level-Tabellen-Maschinenstände? | ein Satz in `docs/UEBERGABE_2026-07-28.md`, gebündelt mit den übrigen Doc-Notizen | PARITY ist bereits OBSOLETE; Banner- und `_needs_review`-Folge (siehe unten) fallen in denselben Doc-Commit |
| Validierung? | gezielter Lauf pro betroffenem Brain; voller Sweep nur am Ende | Batch-Artefakte (`start_failed`) verfälschen Diagnosen; gezielt ist billiger |
| Commits? | thematische Commits, direkt auf `evolution/supervised-harvest`, je `cargo test` + `clippy` grün | History bleibt brauchbar, Fehler kosten einen kleinen Diff |

Vom Code beantwortet statt gefragt: die Stale-Banner aller Pläne sind korrigiert
(2026-08-10; `BRAIN_ANALYZE_ADD.md:3`, `AUTORESEARCH_PLAN.md:3` u. a. — alle
dokumentieren „Banner korrigiert 2026-08-10"). **Noch offen ist dagegen der
`_needs_review`-Folgesatz:** `BRAIN_ANALYZE_ADD.md:12` räumt ein, dass
`_needs_review` nie gebaut wurde, aber §7 (`BRAIN_ANALYZE_ADD.md:109-110`)
beschreibt weiterhin das alte „schreiben, aber `_needs_review` markieren" — das
gehört auf „FAIL nicht schreiben" nachgezogen (§13). `reasoning_effort_path`
steht in keiner Selektor-Datei, `capability.rs:259` verlangt ihn aber schon.
Nur `qwen` hat `voice_input_button`/`voice_mode_button`; `apply_fake_audio_args`
(`webview_runtime.rs:774`) injiziert via `--use-file-for-fake-audio-capture`;
`reasoning_effort` war am 2026-07-28 an claude live belegt
(`capability.rs:261-264`). Für das stop_button-Hunting an gemini ist eine
Anmeldung von Hand nötig (auch zum Vermessen der leeren `ui_options`;
`selectors/gemini.json`).
