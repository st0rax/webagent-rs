//! capability — was ein Brain im Webchat kann, als Level und als Questlog.
//!
//! Heute fährt der Agent jedes Brain als reines Text-rein/Text-raus. Die
//! Oberfläche bietet aber weit mehr per Maus: Reasoning umschalten, Modell
//! wechseln, Websuche, Anhänge. Jede Schaltfläche ist eine Fähigkeit, die
//! webagent noch nicht nutzt.
//!
//! # Warum der Nenner pro Brain gilt
//!
//! Nicht jede Oberfläche bietet alles. DeepSeek hat deutlich weniger Knöpfe
//! als ChatGPT. Ein globaler Nenner würde ein karges UI für seine Kargheit
//! bestrafen und wäre nie erreichbar. Deshalb ist das Maximum eines Brains die
//! Zahl der Optionen, die **dieses** Brain anbietet:
//!
//! ```text
//! deepseek [1/5]     <- 5 Knoepfe vorhanden, 1 davon fahrbar
//! chatgpt  [1/11]    <- reichere Oberfläche, gleicher Stand
//! ```
//!
//! Ein Brain auf `[5/5]` ist damit *fertig ausgereizt*, auch wenn ein anderes
//! einen höheren absoluten Zähler hat.
//!
//! # Was zählt
//!
//! Nur **fahrbare** Fähigkeiten: eine Option zählt, wenn der Code sie bedienen
//! kann UND die Selektoren dafür hinterlegt sind. Ein Eintrag in der JSON, den
//! niemand ansteuert, ist kein Können — sonst misst das Level Absichten statt
//! Fähigkeiten. Alles, was das Brain anbietet aber noch nicht fahrbar ist,
//! landet im Questlog.

use crate::capability_proof::selector_hash_for;
use crate::config::load_selectors;

/// Wie eine Fähigkeit zu belegen ist — steuert auch, **was** `verify` auslöst
/// (§8 des Capability-Proof-Plans). Die Beleg-Form gehört in dieselbe Zeile wie
/// `needs`, `driveable` und `attainable`.
///
/// `None` nur für Katalog-Einträge ohne Beleg-Form: nicht `driveable` oder
/// nicht `attainable`. Der Vollständigkeits-Test unten erzwingt den
/// Zusammenhang: genau die fahrbaren und erreichbaren Fähigkeiten haben eine
/// Beleg-Form, alle anderen `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofKind {
    /// Probe senden, Antwort abwarten (Dreier-ODER wie `wait_response`).
    Generation,
    /// Auf chats Generierung aufgesattelt: Stop sichtbar → Klick → weg.
    Induced,
    /// URL-Paar vorher/nachher (`get_conversation_ref`, `open_section`).
    Navigation,
    /// Zustandswechsel mit Rückkehr, wie `operations::verify_surface`.
    RoundTripToggle,
    /// `list_menu` → Eintrag ≠ aktuell → hin → zurück.
    RoundTripMenu,
    /// Segmentleiste, in der alle Stellungen dauerhaft sichtbar sind.
    RoundTripSegment,
    /// Kein Beleg definiert — Fähigkeit ist nicht fahrbar oder nicht erreichbar.
    None,
}

/// Eine bekannte Webchat-Fähigkeit (das Universum, nicht das Angebot).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capability {
    /// Stabiler Schlüssel; auch der Name im `ui_options`-Feld der Selektor-JSON.
    pub key: &'static str,
    /// Kurzbeschreibung für Anzeige und Questlog.
    pub label: &'static str,
    /// Selektor-Schlüssel, die alle vorhanden sein müssen, damit die Fähigkeit
    /// fahrbar ist.
    pub needs: &'static [&'static str],
    /// Beleg-Form dieser Fähigkeit (§5 des Plans). `None` nur bei nicht
    /// fahrbaren oder nicht erreichbaren Einträgen.
    pub proof: ProofKind,
    /// Ob der Agent die Fähigkeit heute bedienen kann. `false` = Katalogeintrag
    /// ohne Code; zählt nie zum Level, erscheint aber als Quest.
    pub driveable: bool,
    /// Ob die Fähigkeit für diesen Harness überhaupt erreichbar ist.
    ///
    /// `false` heißt: nicht „noch nicht gebaut", sondern „mit den Mitteln
    /// dieses Agenten nicht nachweisbar fahrbar". Solche Einträge fallen aus
    /// dem Nenner heraus, statt als ewige Quest zu stehen — ein Maximum, das
    /// niemand erreichen kann, ist kein Maßstab, sondern eine Schikane.
    /// Sie bleiben im Katalog, damit die Begründung nicht verlorengeht und
    /// jemand sie widerlegen kann.
    pub attainable: bool,
}

/// Katalog aller bekannten Optionen. Neue Entdeckungen kommen hier dazu.
pub const CATALOG: &[Capability] = &[
    Capability {
        key: "chat",
        label: "Text senden und Antwort lesen",
        needs: &["composer", "send_button", "assistant_message"],
        proof: ProofKind::Generation,
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "new_chat",
        label: "Neuen Chat beginnen",
        needs: &["new_chat_button"],
        proof: ProofKind::Navigation,
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "stop_generation",
        label: "Laufende Antwort abbrechen",
        needs: &["stop_button"],
        proof: ProofKind::Induced,
        driveable: true,
        attainable: true,
    },
    // Seit 2026-07-28 fahrbar (`WebBrainBackend::toggle_option`), live belegt
    // an deepseek: DeepThink aria-pressed false->true, Websuche true->false,
    // jeweils mit Zustandsvergleich vorher/nachher.
    Capability {
        key: "reasoning_toggle",
        label: "Reasoning/Thinking umschalten",
        needs: &["reasoning_toggle"],
        proof: ProofKind::RoundTripToggle,
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "web_search",
        label: "Websuche zuschalten",
        needs: &["web_search_toggle"],
        proof: ProofKind::RoundTripToggle,
        driveable: true,
        attainable: true,
    },
    // Seit 2026-07-28 fahrbar (`WebBrainBackend::switch_model`), live belegt an
    // claude (Sonnet 5 -> Haiku 4.5), zai (GLM-5.1 -> GLM-5-Turbo) und qwen
    // (Plus -> Max), jeweils mit Nachpruefung der Menue-Beschriftung.
    //
    // Der Wechsel haelt nur INNERHALB einer Sitzung: jeder neue Browserstart
    // faellt auf das Standardmodell zurueck. Er muss also in derselben Sitzung
    // passieren wie die Frage — nicht vorab einmal gesetzt werden.
    Capability {
        key: "model_switch",
        label: "Modell wechseln",
        needs: &["model_menu", "model_option"],
        proof: ProofKind::RoundTripMenu,
        driveable: true,
        attainable: true,
    },
    // Getrennt von `model_switch`: eine Segmentleiste, in der alle Stellungen
    // dauerhaft sichtbar sind (deepseek: Instant | Expert | Vision). Es gibt
    // kein Menue zum Aufklappen und keine gemeinsame Beschriftung, an der man
    // den Wechsel gegenlesen koennte.
    //
    // Der Antrieb (`select_segment`) steht, bleibt aber driveable: false —
    // am 2026-07-28 gemessen tragen deepseeks Segmente auf dem klickbaren
    // Vorfahren KEINE Auswahlmarkierung: weder aria-selected/-pressed noch
    // data-state noch eine Klasse. Der Zustandsstring kam leer zurueck, und
    // der Rueckweg auf "Instant" meldete korrekt "kein Zustandswechsel
    // feststellbar". Ein Klick, dessen Wirkung man nicht nachweisen kann, ist
    // kein Koennen — sonst zaehlt das Level Absichten. Sobald ein belastbarer
    // Marker gefunden ist (z.B. am Elternelement statt am Knopf), kippt das.
    Capability {
        key: "mode_switch",
        label: "Modus umschalten (Segmentleiste)",
        needs: &["mode_option"],
        // Beleg-Form folgt beim Fahren: sobald ein Marker gefunden ist und
        // `driveable` auf `true` kippt, kommt hier `RoundTripSegment` dazu.
        proof: ProofKind::None,
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "deep_research",
        label: "Deep Research starten",
        needs: &["deep_research_toggle"],
        proof: ProofKind::None,
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "file_attach",
        label: "Datei anhängen",
        needs: &["attach_button"],
        proof: ProofKind::None,
        driveable: false,
        // Nicht erreichbar, nicht bloss ungebaut: der Knopf oeffnet einen
        // Dateidialog des Betriebssystems. JavaScript kann keine File-Objekte
        // erzeugen, und ohne CDP gibt es keinen Weg, einem input[type=file]
        // eine Datei unterzuschieben. Als ewige Quest wuerde das jeden Nenner
        // dauerhaft unerreichbar machen.
        attainable: false,
    },
    Capability {
        key: "canvas",
        label: "Canvas/Artifact öffnen",
        needs: &["canvas_button"],
        proof: ProofKind::None,
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "regenerate",
        label: "Antwort neu erzeugen",
        needs: &["regenerate_button"],
        proof: ProofKind::None,
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "temporary_chat",
        label: "Temporären Chat nutzen",
        needs: &["temporary_chat_button"],
        proof: ProofKind::None,
        // Antrieb steht seit 2026-08-02 (`WebBrainBackend::toggle_temporary_chat`,
        // Vorher/Nachher-Beleg wie bei `toggle_option`, gegen den Mock getestet).
        // Bleibt trotzdem `false`: kein Beleg, kein Level — es fehlt der Klick am
        // echten qwen, der nachweislich einen Zustand geaendert hat. Erst danach
        // hier auf `true` stellen, mit Datum und Brain im Kommentar.
        driveable: false,
        attainable: true,
    },
    // Am 2026-07-27 auf Screenshots der acht Oberflächen gesichtet, aber vom
    // Katalog bis dahin nicht gekannt. Ein nicht gekannter Knopf faellt aus dem
    // Nenner heraus und laesst ein Brain reicher aussehen als es ist — deshalb
    // stehen sie hier, obwohl noch kein Code sie faehrt.
    Capability {
        key: "voice_input",
        label: "Spracheingabe per Mikrofon",
        needs: &["voice_input_button"],
        proof: ProofKind::None,
        driveable: false,
        // Anklickbar, aber nicht belegbar: ein laufendes Mikrofon aendert
        // keinen pruefbaren Zustand. Nach dem eigenen Massstab — kein Beleg,
        // kein Level — darf es nicht zaehlen; dann gehoert es auch nicht in
        // den Nenner. Seit `webview_runtime.rs::apply_fake_audio_args` kann
        // eine WAV-Datei die Mikrofon-Freigabe ersetzen und die Transkription
        // landet belegbar im Composer (`--use-file-for-fake-audio-capture`,
        // opt-in via `WEBAGENT_FAKE_AUDIO`). Bleibt `false`, bis ein
        // End-to-End-Lauf bestanden ist, dann `ProofKind::Generation` und
        // `attainable: true` (§13: voice_* verschoben, eigener grill-me).
        attainable: false,
    },
    // Bewusst getrennt von `voice_input`: das Mikrofon diktiert in den Composer,
    // der Sprachdialog uebernimmt die ganze Unterhaltung. Wer beides in einen
    // Eintrag wirft, kann spaeter nicht sagen, was ein Brain wirklich anbietet.
    Capability {
        key: "voice_mode",
        label: "Sprachdialog-Modus",
        needs: &["voice_mode_button"],
        proof: ProofKind::None,
        driveable: false,
        // Wie voice_input: der Sprachdialog laesst sich starten, sein
        // Gelingen aber nicht aus dem DOM ablesen.
        attainable: false,
    },
    // Nicht dasselbe wie `reasoning_toggle`: Claude zeigt neben dem Modell
    // ("Sonnet 5") einen zweiten Waehler ("Mittel"). Der schaltet Reasoning
    // nicht an oder aus, sondern dosiert die Denktiefe — eine eigene Fähigkeit
    // mit eigenem Menue.
    Capability {
        key: "reasoning_effort",
        label: "Denkstufe waehlen",
        // Der Stufen-Pfad liegt in einem Untermenue ("Aufwand > Hoch"); ohne
        // konfigurierten `reasoning_effort_path` kann `verify` die Fähigkeit
        // nicht ausfuehren und meldet ehrlich NeedsSelectors (Nacharbeit §13).
        needs: &["reasoning_effort_menu", "reasoning_effort_path"],
        proof: ProofKind::RoundTripMenu,
        // Seit 2026-07-28 fahrbar (`select_in_menu_path`), live belegt an claude:
        // "Sonnet 5 Mittel" -> "Sonnet 5 Hoch" -> zurueck. Die Stufe liegt in
        // einem Untermenue ("Aufwand > Hoch"), ein einstufiger Klick erreicht
        // sie nicht; der Beleg bleibt die Beschriftung des Menuebuttons.
        driveable: true,
        attainable: true,
    },
    // Projekte sind persistenter Kontext ueber Chats hinweg. Fuer webagent
    // interessant, weil sich damit Dauerauftraege ablegen liessen, statt jeden
    // Chat bei null zu beginnen.
    Capability {
        key: "projects",
        label: "Projekte/Arbeitsbereiche",
        needs: &["projects_button"],
        proof: ProofKind::Navigation,
        // Seit 2026-07-28 fahrbar (`open_section`), live belegt: chatgpt
        // navigiert nach /projects, claude nach claude.ai/projects. Der Beleg
        // ist die URL — am Knopf selbst gibt es keinen Zustand.
        driveable: true,
        attainable: true,
    },
];

/// Stichworte, an denen eine Schaltfläche einer Fähigkeit zugeordnet wird.
/// Geprüft wird gegen aria-label, title, data-testid und Beschriftung.
///
/// Bewusst eng gehalten: ein falsch erkannter Knopf blaeht den Nenner auf und
/// erzeugt eine Quest, die es nicht gibt. Lieber eine Option uebersehen (sie
/// faellt beim naechsten Durchgang auf) als eine erfinden.
const MATCHERS: &[(&str, &[&str])] = &[
    (
        "reasoning_toggle",
        &[
            "deepthink",
            "deep think",
            "extended thinking",
            "reasoning",
            "think longer",
        ],
    ),
    (
        "web_search",
        &["web search", "search the web", "websuche", "im web suchen"],
    ),
    (
        "model_switch",
        &[
            "choose model",
            "model selector",
            "modell wählen",
            "switch model",
        ],
    ),
    ("deep_research", &["deep research", "tiefenrecherche"]),
    (
        "file_attach",
        &["attach", "upload file", "datei anhängen", "hochladen"],
    ),
    ("canvas", &["canvas", "artifact", "artefakt"]),
    (
        "regenerate",
        &["regenerate", "neu generieren", "erneut generieren"],
    ),
    (
        "temporary_chat",
        &["temporary chat", "temporärer chat", "incognito"],
    ),
    ("new_chat", &["new chat", "neuer chat", "neuen chat"]),
    (
        "stop_generation",
        &["stop response", "stop generating", "antwort stoppen"],
    ),
    // "voice" allein waere zu weit: es steckt auch in Beschriftungen, die den
    // Sprachdialog meinen. Deshalb nur Wortpaare bzw. das eindeutige Mikrofon.
    (
        "voice_input",
        &["mikrofon", "microphone", "spracheingabe", "voice input"],
    ),
    (
        "voice_mode",
        &[
            "sprachmodus",
            "voice mode",
            "sprachdialog",
            "sprachmodus starten",
        ],
    ),
    // Bewusst ohne das nackte "reasoning": das gehoert dem An/Aus-Schalter.
    // Hier zaehlt nur, was eine Stufe benennt.
    (
        "reasoning_effort",
        &[
            "reasoning effort",
            "denkstufe",
            "denktiefe",
            "thinking effort",
        ],
    ),
    (
        "projects",
        &["projekte", "projects", "new project", "neues projekt"],
    ),
];

/// Ordnet die Schaltflächen eines DOM-Berichts den bekannten Fähigkeiten zu.
///
/// # Grenze der Methode
///
/// Sie findet nur, was im DOM einen Namen trägt. Am 2026-07-27 gemessen:
/// deepseek rendert 107 Bedienelemente als `div.ds-button--icon` mit SVG —
/// ohne aria-label, title, id, `data-*`, Text oder Eltern-Label. Dort ist
/// nichts zu holen; solche Oberflächen brauchen Icon- oder Positionsanalyse.
///
/// Deshalb sind die Ergebnisse **Untergrenzen** und werden nicht automatisch
/// als `ui_options` festgeschrieben. Wer es besser weiß, trägt sie von Hand in
/// `<stable_root>/selectors/<brain>.json` ein — diese Datei schlägt die
/// mitgelieferte (siehe `config::resolve_selectors_path`).
///
/// `buttons` ist die `buttons`-Liste aus `WebBrainBackend::dom_report` — je
/// Eintrag die Felder `al` (aria-label), `ti` (title), `dt` (data-testid) und
/// `tp` (Textanfang). `chat` ist immer dabei: wer ein Eingabefeld hat, kann
/// chatten, und genau das prueft `dom_report` ueber `counts`.
pub fn detect_ui_options(buttons: &[serde_json::Value], has_composer: bool) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    if has_composer {
        found.push("chat".to_string());
    }
    let haystacks: Vec<String> = buttons
        .iter()
        .map(|b| {
            // `cls` mitlesen: manche Oberflaechen (deepseek) rendern reine
            // Icon-Divs ohne aria-label, title oder Text — dort ist der
            // Klassenname der einzige Anhaltspunkt.
            // `ex` traegt die Ersatzquellen fuer Icon-only-Knoepfe (SVG-title,
            // id, data-*, Eltern-Label), `cls` den Klassennamen — bei manchen
            // Oberflaechen der einzige Anhaltspunkt.
            ["al", "ti", "dt", "tp", "cls", "ex"]
                .iter()
                .filter_map(|k| b.get(*k).and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
        })
        .collect();
    for (key, needles) in MATCHERS {
        if haystacks
            .iter()
            .any(|h| needles.iter().any(|n| h.contains(n)))
        {
            found.push((*key).to_string());
        }
    }
    // Katalogreihenfolge, keine Dubletten.
    CATALOG
        .iter()
        .filter(|c| found.iter().any(|f| f == c.key))
        .map(|c| c.key.to_string())
        .collect()
}

/// Katalogeintrag zu einem Schlüssel.
pub fn capability(key: &str) -> Option<&'static Capability> {
    CATALOG.iter().find(|c| c.key == key)
}

/// Fähigkeits-Schlüssel, den eine Antriebs-Route bedient.
///
/// Eine Route kann ein Fähigkeitsname sein (`temporary_chat`, `voice_input`)
/// oder ein Selektor-Schlüssel (`web_search_toggle`, `model_menu`). Beides
/// muss denselben Katalog-Eintrag treffen, damit Beweise aus verschiedenen
/// Kommandos an derselben Fähigkeit ankommen. Nicht-fahrbare Fähigkeiten
/// (und Routen, deren Selektor zu einer nicht-fahrbaren Fähigkeit gehört)
/// liefern `None`: ein Beweis für etwas, das nie ins Level zählt, wäre ein
/// Fake-Level.
pub fn capability_for_route(route: &str) -> Option<&'static str> {
    if let Some(c) = capability(route) {
        if c.driveable {
            return Some(c.key);
        }
    }
    CATALOG
        .iter()
        .find(|c| c.driveable && c.needs.contains(&route))
        .map(|c| c.key)
}

/// Neuen Messfund mit dem bereits bekannten Angebot vereinigen.
///
/// Eine Messung ist eine **Untergrenze** (siehe [`detect_ui_options`]): sie
/// findet nur, was gerade im DOM steht und einen Namen traegt. Weniger zu
/// finden belegt nicht, dass eine Option weg ist — ausgeloggt, eingeklappt
/// oder icon-only reicht schon. Deshalb darf ein Lauf `ui_options` nur heben,
/// nie kuerzen; Streichen bleibt eine Handarbeit an der Datei.
///
/// Genau das fehlte: ein Durchgang ohne erkannten Composer hat `chat` aus der
/// Datei geworfen, und das Brain galt danach als stumm.
pub fn union_ui_options(known: &[String], found: &[String]) -> Vec<String> {
    CATALOG
        .iter()
        .filter(|c| known.iter().any(|k| k == c.key) || found.iter().any(|f| f == c.key))
        .map(|c| c.key.to_string())
        .collect()
}

/// Eine offene Aufgabe: das Brain bietet die Option an, webagent kann sie nicht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quest {
    pub brain_id: String,
    pub key: String,
    pub label: String,
    /// Was konkret fehlt — der eigentliche Wert des Questlogs.
    pub blocker: QuestBlocker,
}

/// Woran eine Quest hängt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestBlocker {
    /// Selektoren da, aber kein Code, der die Option bedient.
    NeedsCode,
    /// Code da, aber für dieses Brain fehlen die Selektoren.
    NeedsSelectors,
    /// Beides fehlt.
    NeedsBoth,
    /// Code + Selektoren da, aber nie verifiziert — es fehlt ein Beleg.
    NeedsProof,
    /// Lief einmal, Beleg aber verfallen (TTL abgelaufen oder Selektoren geändert).
    ProofExpired,
}

impl QuestBlocker {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuestBlocker::NeedsCode => "Code fehlt",
            QuestBlocker::NeedsSelectors => "Selektoren fehlen",
            QuestBlocker::NeedsBoth => "Code + Selektoren fehlen",
            QuestBlocker::NeedsProof => "nie verifiziert",
            QuestBlocker::ProofExpired => "Beleg verfallen",
        }
    }
}

/// Fähigkeitsstand eines Brains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrainLevel {
    pub brain_id: String,
    /// Wurde die Oberfläche je durchgezählt (`ui_options` gepflegt)?
    /// `false` = leerer PoKIdex-Eintrag: gesichtet, aber nicht vermessen.
    pub surveyed: bool,
    /// Optionen, die dieses Brain laut UI anbietet (der Nenner).
    pub available: Vec<String>,
    /// Davon fahrbar (der Zähler).
    pub have: Vec<String>,
    /// Der Rest, als Aufgaben.
    pub quests: Vec<Quest>,
    /// Bewiesene Fähigkeiten: `(Fähigkeit, Zeitpunkt des Belegs)`. Der
    /// Grund, warum ein Brain `[n/…]` steht, obwohl n = 0 ist, wird hier
    /// sichtbar statt verschwiegen.
    pub verified: Vec<(String, String)>,
    /// Vom Brain angeboten, aber fuer diesen Harness prinzipiell nicht
    /// nachweisbar fahrbar — bewusst aus dem Nenner genommen und hier
    /// sichtbar gehalten.
    pub out_of_reach: Vec<String>,
}

impl BrainLevel {
    pub fn level(&self) -> usize {
        self.have.len()
    }
    /// `None` = unvermessen; ein Maximum zu behaupten waere geraten.
    pub fn max_level(&self) -> Option<usize> {
        if self.surveyed {
            Some(self.available.len())
        } else {
            None
        }
    }
    /// `deepseek [1/5]` bzw. `deepseek [1/?]` solange unvermessen. Sind
    /// Fähigkeiten vorhanden, aber unbewiesen, trägt das Wort die Ehrlichkeit
    /// (CONVENTIONS.md:106): `deepseek [0/7 · 5 unbewiesen]`.
    pub fn label(&self) -> String {
        let unproven = self
            .quests
            .iter()
            .filter(|q| matches!(q.blocker, QuestBlocker::NeedsProof | QuestBlocker::ProofExpired))
            .count();
        match self.max_level() {
            Some(m) => {
                if unproven > 0 {
                    format!(
                        "{} [{}/{} · {} unbewiesen]",
                        self.brain_id,
                        self.level(),
                        m,
                        unproven
                    )
                } else {
                    format!("{} [{}/{}]", self.brain_id, self.level(), m)
                }
            }
            None => format!("{} [{}/?]", self.brain_id, self.level()),
        }
    }
    /// Alles ausgereizt, was dieses Brain hergibt. Unvermessen ist nie maxed.
    pub fn maxed(&self) -> bool {
        matches!(self.max_level(), Some(m) if m > 0 && self.level() == m)
    }
    /// Rang-Titel — reine Anzeige, aus dem Anteil abgeleitet. 0 ist
    /// „unbewiesen", nicht „stumm": alles da, nur nie verifiziert
    /// (CONVENTIONS.md:106 — unbekannt, nicht schlecht).
    pub fn rank(&self) -> &'static str {
        let max = match self.max_level() {
            Some(m) if m > 0 => m,
            Some(_) => return "leer",
            None => return "unvermessen",
        };
        match self.level() * 100 / max {
            100 => "gemeistert",
            67..=99 => "fortgeschritten",
            34..=66 => "angelernt",
            1..=33 => "Anfänger",
            _ => "unbewiesen",
        }
    }
}

/// Prüft, ob eine Selektor-JSON einen nicht-leeren Eintrag für `key` hat.
/// Akzeptiert Liste (übliche Form) und Einzelstring.
fn has_selector(sel: &serde_json::Value, key: &str) -> bool {
    match sel.get(key) {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .any(|v| v.as_str().map(|s| !s.trim().is_empty()).unwrap_or(false)),
        Some(serde_json::Value::String(s)) => !s.trim().is_empty(),
        _ => false,
    }
}

/// Welche Optionen bietet dieses Brain an?
///
/// Erste Quelle ist das Feld `ui_options` der Selektor-JSON — eine bewusste
/// Aussage darüber, was die Oberfläche hergibt, unabhängig davon, ob schon
/// jemand Selektoren dafür geschrieben hat. Fehlt das Feld, fällt die Zählung
/// auf „alles, wofür Selektoren da sind" zurück; sonst stünde jedes bisher
/// nicht vermessene Brain bei `[n/0]`.
pub fn available_options(sel: &serde_json::Value) -> Option<Vec<String>> {
    let list = match sel.get("ui_options") {
        Some(serde_json::Value::Array(l)) => l,
        _ => return None,
    };
    let declared: Vec<&str> = list
        .iter()
        .filter_map(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty() && capability(s).is_some())
        .collect();
    if declared.is_empty() {
        return None;
    }
    // Reihenfolge des Katalogs, keine Dubletten.
    Some(
        CATALOG
            .iter()
            .filter(|c| declared.contains(&c.key))
            .map(|c| c.key.to_string())
            .collect(),
    )
}

/// Stand eines Brains aus seiner bereits geladenen Selektor-JSON bestimmen.
/// Optionen, die das Brain zwar anbietet, die dieser Harness aber prinzipiell
/// nicht nachweisbar fahren kann. Werden getrennt ausgewiesen statt still
/// verschwiegen — sonst sieht es aus, als gaebe es sie nicht.
pub fn out_of_reach(available: &[String]) -> Vec<String> {
    available
        .iter()
        .filter(|k| capability(k).map(|c| !c.attainable).unwrap_or(false))
        .cloned()
        .collect()
}

pub fn level_from_selectors(brain_id: &str, sel: &serde_json::Value) -> BrainLevel {
    level_from_selectors_with(brain_id, sel, &|b, cap, hash| {
        crate::capability_proof::proof_state(b, cap, hash)
    })
}

/// Stand eines Brains aus seiner bereits geladenen Selektor-JSON bestimmen —
/// mit injiziertem Beleg-Nachschlag. Die IO bleibt damit aus dem reinen
/// Teil; `lookup(brain_id, fähigkeit, selektor_hash)` liefert den Belegzustand
/// (Tests spielen hier einen Fake ein, siehe `shipped_brains_can_all_at_least_chat`).
///
/// **Beide Wege zu `have` sind gegatet** — der offensichtliche (Fähigkeit im
/// `ui_options`-Angebot) wie der Überschreib-Pfad bei unvermessenem Brain.
/// Sonst behielte ein Brain ohne `ui_options` sein selektor-basiertes Level.
pub fn level_from_selectors_with(
    brain_id: &str,
    sel: &serde_json::Value,
    lookup: &dyn Fn(&str, &str, u32) -> crate::capability_proof::ProofState,
) -> BrainLevel {
    // Kein `ui_options` = niemand hat die Oberfläche je durchgezählt. Dann ist
    // der Nenner unbekannt, NICHT "das, wofür zufällig Selektoren da sind" —
    // sonst meldet jedes Brain "ausgereizt", obwohl nur Text funktioniert.
    let surveyed = available_options(sel);
    let offered = surveyed.clone().unwrap_or_default();
    let unreachable = out_of_reach(&offered);
    // Der Nenner enthaelt nur, was dieser Harness ueberhaupt nachweisbar
    // fahren KANN. Ein Maximum, das niemand erreichen kann, ist kein Massstab.
    // Die ausgeschlossenen bleiben in `out_of_reach` sichtbar.
    let available: Vec<String> = offered
        .iter()
        .filter(|k| !unreachable.contains(k))
        .cloned()
        .collect();
    let mut have = Vec::new();
    let mut quests = Vec::new();
    let mut verified = Vec::new();
    for key in &available {
        let cap = match capability(key) {
            Some(c) => c,
            None => continue,
        };
        let has_sel = cap.needs.iter().all(|k| has_selector(sel, k));
        let proof = (cap.driveable && has_sel)
            .then(|| lookup(brain_id, cap.key, selector_hash_for(cap, sel)));
        match proof {
            Some(crate::capability_proof::ProofState::Proven { at }) => {
                have.push(cap.key.to_string());
                verified.push((cap.key.to_string(), at));
            }
            Some(crate::capability_proof::ProofState::Expired { .. }) => {
                quests.push(quest(brain_id, cap, QuestBlocker::ProofExpired));
            }
            Some(_) => quests.push(quest(brain_id, cap, QuestBlocker::NeedsProof)),
            None if has_sel => quests.push(quest(brain_id, cap, QuestBlocker::NeedsCode)),
            None if cap.driveable => {
                quests.push(quest(brain_id, cap, QuestBlocker::NeedsSelectors))
            }
            None => quests.push(quest(brain_id, cap, QuestBlocker::NeedsBoth)),
        }
    }
    // Fahrbares zählt auch bei unvermessenem Brain — nur der Nenner fehlt.
    // Auch hier gilt das Gate: ohne Beleg kein `have`, sonst ist gerade das
    // unvermessene Brain die Hintertür (gemini hat leere `ui_options`).
    if surveyed.is_none() {
        have.clear();
        verified.clear();
        quests.clear();
        for cap in CATALOG {
            if !cap.driveable || !cap.needs.iter().all(|k| has_selector(sel, k)) {
                continue;
            }
            if let crate::capability_proof::ProofState::Proven { at } =
                lookup(brain_id, cap.key, selector_hash_for(cap, sel))
            {
                have.push(cap.key.to_string());
                verified.push((cap.key.to_string(), at));
            }
        }
    }
    BrainLevel {
        brain_id: brain_id.to_string(),
        surveyed: surveyed.is_some(),
        available,
        out_of_reach: unreachable,
        have,
        quests,
        verified,
    }
}

fn quest(brain_id: &str, cap: &Capability, blocker: QuestBlocker) -> Quest {
    Quest {
        brain_id: brain_id.to_string(),
        key: cap.key.to_string(),
        label: cap.label.to_string(),
        blocker,
    }
}

/// Stand eines Brains bestimmen (lädt die Selektoren von Platte/Embedded).
/// Unlesbare Selektoren = Level 0 ohne Angebot, kein Fehler: ein kaputtes Brain
/// soll die Anzeige der anderen nicht verhindern.
pub fn level_of(brain_id: &str) -> BrainLevel {
    match load_selectors(brain_id) {
        Ok(sel) => level_from_selectors(brain_id, &sel),
        Err(_) => BrainLevel {
            brain_id: brain_id.to_string(),
            surveyed: false,
            available: Vec::new(),
            out_of_reach: Vec::new(),
            have: Vec::new(),
            quests: Vec::new(),
            verified: Vec::new(),
        },
    }
}

/// Stand aller bekannten Brains (sortiert nach ID).
pub fn levels_all() -> Vec<BrainLevel> {
    crate::config::available_brain_ids()
        .into_iter()
        .map(|id| level_of(&id))
        .collect()
}

/// Alle offenen Quests über alle Brains, gebündelt nach Fähigkeit.
///
/// Nach Häufigkeit sortiert: eine Option, die sechs Brains anbieten, bringt
/// beim Implementieren sechs Level — das ist die Reihenfolge, in der sich
/// Arbeit lohnt.
pub fn quest_log() -> Vec<(String, Vec<Quest>)> {
    let mut grouped: Vec<(String, Vec<Quest>)> = Vec::new();
    for lvl in levels_all() {
        for q in lvl.quests {
            match grouped.iter_mut().find(|(k, _)| k == &q.key) {
                Some((_, list)) => list.push(q),
                None => grouped.push((q.key.clone(), vec![q])),
            }
        }
    }
    grouped.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability_proof::ProofState;
    use serde_json::json;

    fn text_only() -> serde_json::Value {
        json!({
            "composer": ["#x"],
            "send_button": ["#y"],
            "assistant_message": ["#z"],
        })
    }

    /// Fake-Beleg: jede fahrbare Fähigkeit gilt als bewiesen. Damit prüfen
    /// Tests, die nur den selektor-Teil messen, genau das — und bleiben ohne
    /// Store und Browser lauffähig (§11 des Plans).
    fn always_proven(
        _brain: &str,
        _cap: &str,
        _hash: u32,
    ) -> crate::capability_proof::ProofState {
        ProofState::Proven {
            at: "2026-01-01T00:00:00Z".into(),
        }
    }

    fn never_proven(
        _brain: &str,
        _cap: &str,
        _hash: u32,
    ) -> crate::capability_proof::ProofState {
        ProofState::Never
    }

    #[test]
    fn detect_ui_options_reads_buttons_not_self_report() {
        let buttons = vec![
            json!({"al": "DeepThink (R1)", "ti": "", "dt": "", "tp": "DeepThink"}),
            json!({"al": "Search the web", "ti": "", "dt": "", "tp": ""}),
            json!({"al": "", "ti": "", "dt": "new-chat-button", "tp": "Neuer Chat"}),
            json!({"al": "Kontoeinstellungen", "ti": "", "dt": "", "tp": ""}),
        ];
        let got = detect_ui_options(&buttons, true);
        assert_eq!(
            got,
            vec!["chat", "new_chat", "reasoning_toggle", "web_search"]
        );
        // Nicht erkannt heisst nicht erfunden: canvas taucht nirgends auf.
        assert!(!got.contains(&"canvas".to_string()));
    }

    #[test]
    fn detect_ui_options_without_composer_has_no_chat() {
        let got = detect_ui_options(&[json!({"al": "Canvas"})], false);
        assert_eq!(got, vec!["canvas"]);
    }

    #[test]
    fn detect_ui_options_ignores_unrelated_buttons() {
        let buttons = vec![
            json!({"al": "Einstellungen"}),
            json!({"al": "Abmelden"}),
            json!({"al": "Feedback geben"}),
        ];
        assert!(detect_ui_options(&buttons, false).is_empty());
    }

    #[test]
    fn newly_catalogued_options_are_quests_not_levels() {
        // Frisch aufgenommene Fund-Optionen haben Selektoren, aber keinen Code.
        // Sie muessen den Nenner heben und im Questlog stehen — nie den Zaehler.
        let buttons = vec![
            json!({"al": "Mikrofon", "ti": "", "dt": "", "tp": ""}),
            json!({"al": "", "ti": "Sprachmodus", "dt": "", "tp": ""}),
            json!({"al": "Canvas", "ti": "", "dt": "", "tp": ""}),
            json!({"al": "Projekte", "ti": "", "dt": "", "tp": ""}),
        ];
        let got = detect_ui_options(&buttons, true);
        assert_eq!(
            got,
            vec!["chat", "canvas", "voice_input", "voice_mode", "projects"]
        );

        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "voice_input_button": ["#mic"],
            "voice_mode_button": ["#vm"],
            "canvas_button": ["#cv"],
                        "projects_button": ["#pr"],
            "ui_options": got,
        });
        let lvl = level_from_selectors_with("claude", &sel, &always_proven);
        // Mikrofon und Sprachdialog sind angeboten, aber nicht
        // nachweisbar fahrbar (attainable: false) — sie fallen aus dem Nenner
        // und stehen in `out_of_reach`. Bliebe ein unerreichbarer Eintrag im
        // Nenner, koennte kein Brain je "ausgereizt" werden.
        //
        // Als Beispiel fuer "Selektor ohne Code" dient jetzt `canvas`: der Test
        // stand nacheinander auf reasoning_toggle, projects und
        // reasoning_effort — alle drei wurden am 2026-07-28 fahrbar. Er wandert
        // mit dem Code; seine Aussage bleibt unveraendert.
        assert_eq!(
            lvl.label(),
            "claude [2/3]",
            "chat und projects sind fahrbar"
        );
        assert_eq!(lvl.out_of_reach, vec!["voice_input", "voice_mode"]);
        assert_eq!(lvl.quests.len(), 1, "nur canvas fehlt noch");
        assert_eq!(lvl.quests[0].key, "canvas");
        assert_eq!(lvl.quests[0].blocker, QuestBlocker::NeedsCode);
    }

    #[test]
    fn unreachable_options_leave_the_denominator_but_stay_visible() {
        // Dateianhang oeffnet einen Dialog des Betriebssystems; JavaScript kann
        // keine File-Objekte erzeugen. Als ewige Quest haette das jeden Nenner
        // dauerhaft unerreichbar gemacht.
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "attach_button": ["#a"],
            "ui_options": ["chat", "file_attach"],
        });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.label(), "t [1/1]");
        assert!(
            lvl.maxed(),
            "ohne den unerreichbaren Posten ist das ausgereizt"
        );
        assert_eq!(lvl.out_of_reach, vec!["file_attach"]);
        assert!(
            lvl.quests.is_empty(),
            "unerreichbar ist keine Aufgabe, sondern eine Grenze"
        );
    }

    #[test]
    fn reasoning_effort_is_not_the_reasoning_toggle() {
        // Claude zeigt beides: den An/Aus-Schalter und den Stufen-Waehler.
        // Wuerden sie zusammenfallen, verschwaende eine Fähigkeit im Nenner.
        let got = detect_ui_options(&[json!({"al": "Denkstufe"})], false);
        assert_eq!(got, vec!["reasoning_effort"]);
        assert!(!got.contains(&"reasoning_toggle".to_string()));
    }

    #[test]
    fn catalog_keys_are_unique_and_nonempty() {
        let mut seen = std::collections::HashSet::new();
        for cap in CATALOG {
            assert!(!cap.key.trim().is_empty(), "leerer Key");
            assert!(!cap.label.trim().is_empty(), "leeres Label bei {}", cap.key);
            assert!(seen.insert(cap.key), "doppelter Key: {}", cap.key);
        }
    }

    #[test]
    fn denominator_is_per_brain_not_global() {
        // Karges UI: 2 Optionen, davon 1 fahrbar -> [1/2], nicht [1/CATALOG].
        let karg = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat", "reasoning_toggle"],
        });
        let lvl = level_from_selectors_with("deepseek", &karg, &always_proven);
        assert_eq!(lvl.label(), "deepseek [1/2]");

        // Reiches UI, gleicher Code-Stand -> gleicher Zaehler, groesserer Nenner.
        let reich = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat", "reasoning_toggle", "web_search", "canvas", "regenerate"],
        });
        let lvl2 = level_from_selectors_with("chatgpt", &reich, &always_proven);
        assert_eq!(lvl2.label(), "chatgpt [1/5]");
        assert_eq!(
            lvl.level(),
            lvl2.level(),
            "gleicher Stand, anderer Anspruch"
        );
        assert!(lvl.max_level() < lvl2.max_level());
    }

    #[test]
    fn declared_selector_without_code_is_a_quest_not_a_level() {
        // Beispiel bewusst `canvas`: der Test stand frueher auf
        // `reasoning_toggle`, das am 2026-07-28 fahrbar wurde — und faellt
        // damit als Beispiel fuer "Selektor ohne Code" aus. Die Aussage bleibt
        // dieselbe, sie braucht nur einen Eintrag, den noch kein Code bedient.
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "canvas_button": ["#c"],
            "ui_options": ["chat", "canvas"],
        });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.level(), 1, "Selektor ohne Code ist kein Koennen");
        assert_eq!(lvl.quests.len(), 1);
        assert_eq!(lvl.quests[0].key, "canvas");
        assert_eq!(lvl.quests[0].blocker, QuestBlocker::NeedsCode);
    }

    #[test]
    fn quest_blocker_distinguishes_missing_code_from_missing_selectors() {
        // new_chat ist fahrbar; ohne Selektor fehlt genau der.
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat", "new_chat", "canvas"],
        });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        let by = |k: &str| lvl.quests.iter().find(|q| q.key == k).map(|q| q.blocker);
        assert_eq!(by("new_chat"), Some(QuestBlocker::NeedsSelectors));
        assert_eq!(by("canvas"), Some(QuestBlocker::NeedsBoth));
    }

    #[test]
    fn without_ui_options_the_maximum_stays_unknown() {
        // Frueher galt hier "Nenner = vorhandene Selektoren". Das meldete jedes
        // Brain als ausgereizt, obwohl nur Text laeuft — ein Teilnahmepokal.
        // Unvermessen heisst jetzt `?`, und `?` ist nie "gemeistert".
        let lvl = level_from_selectors_with("t", &text_only(), &always_proven);
        assert!(!lvl.surveyed);
        assert_eq!(lvl.max_level(), None);
        assert_eq!(lvl.label(), "t [1/?]");
        assert_eq!(lvl.level(), 1, "Fahrbares zaehlt trotzdem");
        assert!(!lvl.maxed(), "unvermessen darf nie ausgereizt heissen");
        assert_eq!(lvl.rank(), "unvermessen");
        assert!(lvl.quests.is_empty(), "ohne bekanntes Angebot keine Quests");
    }

    #[test]
    fn unknown_ui_option_keys_are_ignored() {
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat", "teleportation", ""],
        });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.available, vec!["chat"], "Unfug zaehlt nicht zum Nenner");
    }

    #[test]
    fn partial_requirements_do_not_count_as_driveable() {
        // model_switch braucht beide Selektoren — einer allein reicht nicht.
        let sel = json!({ "model_menu": ["#m"], "ui_options": ["model_switch"] });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.level(), 0);
        assert_eq!(lvl.max_level(), Some(1));
        assert_eq!(lvl.rank(), "unbewiesen");
    }

    #[test]
    fn empty_or_blank_selector_lists_do_not_count() {
        let sel = json!({ "new_chat_button": [], "stop_button": ["  "] });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.level(), 0);
        assert!(lvl.available.is_empty());
        assert!(!lvl.maxed(), "0/0 ist nicht gemeistert");
    }

    #[test]
    fn unreadable_selectors_yield_level_zero_not_panic() {
        let lvl = level_of("gibt-es-nicht");
        assert_eq!(lvl.level(), 0);
        assert_eq!(lvl.max_level(), None);
        assert_eq!(lvl.rank(), "unvermessen");
    }

    #[test]
    fn shipped_brains_can_all_at_least_chat() {
        // Bewusst NICHT ueber `levels_all()`: das laedt die Selektoren von der
        // Platte, also samt lokalem Overlay unter <stable_root>/selectors und
        // samt selbst registrierter Brains. Ein Test ueber AUSGELIEFERTE Daten,
        // der am Zustand der Maschine haengt, misst nicht die Auslieferung —
        // er faellt irgendwann bei irgendwem um (hier: ein `probe --write` aus
        // einer ausgeloggten Sitzung hatte `chat` aus kimi.json entfernt).
        for (id, raw) in crate::config::shipped_selector_table() {
            let sel: serde_json::Value =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("{id}: kaputtes JSON: {e}"));
            let lvl = level_from_selectors_with(id, &sel, &always_proven);
            assert!(
                lvl.have.contains(&"chat".to_string()),
                "{id} kann nicht mal Text senden"
            );
        }
    }

    #[test]
    fn capability_for_route_maps_names_and_selector_keys() {
        // Faehigkeitsname und Selektor-Schluessel muessen dieselbe Faehigkeit
        // treffen, sonst landen Beweise aus verschiedenen Kommandos am falschen
        // Eintrag.
        assert_eq!(capability_for_route("chat"), Some("chat"));
        assert_eq!(capability_for_route("web_search_toggle"), Some("web_search"));
        assert_eq!(capability_for_route("reasoning_toggle"), Some("reasoning_toggle"));
        assert_eq!(capability_for_route("model_menu"), Some("model_switch"));
        assert_eq!(capability_for_route("reasoning_effort_menu"), Some("reasoning_effort"));
        // Nicht-fahrbar: kein Beweis fuer etwas, das nie ins Level zaehlt.
        assert_eq!(capability_for_route("temporary_chat"), None);
        assert_eq!(capability_for_route("canvas_button"), None);
        assert_eq!(capability_for_route("gibt-es-nicht"), None);
    }

    #[test]
    fn a_survey_may_raise_ui_options_never_shrink_them() {
        // Der Fall, der kimi stumm gemacht hat: ein Durchgang ohne erkannten
        // Composer meldet weniger als bekannt. Was er nicht gesehen hat, ist
        // damit nicht widerlegt — `chat` und `stop_generation` bleiben stehen.
        let known = vec![
            "chat".to_string(),
            "stop_generation".to_string(),
            "new_chat".to_string(),
        ];
        let found = vec!["new_chat".to_string(), "model_switch".to_string()];
        assert_eq!(
            union_ui_options(&known, &found),
            vec!["chat", "new_chat", "stop_generation", "model_switch"],
            "Katalogreihenfolge, nichts verloren, Neufund dazu"
        );
        // Unfug faellt raus, Dubletten auch.
        assert_eq!(
            union_ui_options(&["chat".into(), "teleportation".into()], &["chat".into()]),
            vec!["chat"]
        );
    }

    #[test]
    fn quest_log_sorts_by_reach() {
        // Die Option, die die meisten Brains anbieten, steht oben — dort
        // bringt eine Implementierung den groessten Hebel.
        let log = quest_log();
        for w in log.windows(2) {
            assert!(
                w[0].1.len() >= w[1].1.len(),
                "Questlog nicht nach Reichweite sortiert"
            );
        }
    }

    #[test]
    fn never_verified_is_needs_proof_not_have() {
        // Code + Selektoren da, aber kein Beleg: die Fähigkeit zählt nicht,
        // sie steht als Quest mit dem Grund "nie verifiziert".
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat"],
        });
        let lvl = level_from_selectors_with("t", &sel, &never_proven);
        assert_eq!(lvl.level(), 0, "ohne Beleg ist nichts bewiesen");
        assert_eq!(lvl.quests.len(), 1);
        assert_eq!(lvl.quests[0].key, "chat");
        assert_eq!(lvl.quests[0].blocker, QuestBlocker::NeedsProof);
        assert!(lvl.verified.is_empty());
    }

    #[test]
    fn proven_is_have_with_timestamp() {
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat"],
        });
        let lvl = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl.level(), 1);
        assert_eq!(lvl.verified, vec![("chat".to_string(), "2026-01-01T00:00:00Z".into())]);
    }

    #[test]
    fn expired_proof_is_a_quest_not_have() {
        // Der Beleg lief mal, ist aber verfallen — anderes Urteil als "nie
        // verifiziert", denn es bedeutet: nochmal verifizieren, nicht neu
        // lernen.
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat"],
        });
        let lvl = level_from_selectors_with("t", &sel, &|_, _, _| ProofState::Expired {
            at: "2026-01-01T00:00:00Z".into(),
            reason: crate::capability_proof::ExpiryReason::TtlElapsed,
        });
        assert_eq!(lvl.level(), 0);
        assert_eq!(lvl.quests[0].blocker, QuestBlocker::ProofExpired);
        assert!(lvl.verified.is_empty());
    }

    #[test]
    fn unmeasured_brain_is_gated_too() {
        // Pfad 2 aus §7: ohne `ui_options` wird `have` per Selektor-Scan
        // bestimmt — auch dort gilt der Beleg. Gemessen am
        // `never_proven`-Fall: das kaputte Brain (leere `ui_options`) darf
        // nicht sein selektor-basiertes Level behalten.
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
        });
        let lvl = level_from_selectors_with("t", &sel, &never_proven);
        assert_eq!(lvl.level(), 0, "unvermessen heisst nicht ungegated");
        assert!(lvl.quests.is_empty());
        assert!(lvl.verified.is_empty());

        let lvl_proven = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(lvl_proven.level(), 1);
        assert_eq!(lvl_proven.verified.len(), 1);
    }

    #[test]
    fn label_counts_unproven_faithfully() {
        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "reasoning_toggle": ["#r"],
            "ui_options": ["chat", "reasoning_toggle"],
        });
        let lvl = level_from_selectors_with("t", &sel, &never_proven);
        assert_eq!(lvl.label(), "t [0/2 · 2 unbewiesen]");
        let proven = level_from_selectors_with("t", &sel, &always_proven);
        assert_eq!(proven.label(), "t [2/2]");
    }

    #[test]
    fn proof_kind_is_complete() {
        // Vollständigkeits-Gate: genau die fahrbaren UND erreichbaren
        // Fähigkeiten haben eine Beleg-Form, alle anderen `None`. Wer eine
        // neue Fähigkeit einträgt, muss hier Farbe bekennen — eine fahrbare
        // ohne Beleg-Form würde `verify` nie messen, und die stille Lücke
        // bliebe unsichtbar.
        for cap in CATALOG {
            let needs_proof = cap.driveable && cap.attainable;
            if needs_proof {
                assert!(
                    cap.proof != ProofKind::None,
                    "{} ist fahrbar und erreichbar, hat aber keine Beleg-Form",
                    cap.key
                );
            } else {
                assert_eq!(
                    cap.proof,
                    ProofKind::None,
                    "{} ist nicht (fahrbar UND erreichbar), darf keine Beleg-Form haben",
                    cap.key
                );
            }
        }
    }
}
