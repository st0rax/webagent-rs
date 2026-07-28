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

use crate::config::load_selectors;

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
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "new_chat",
        label: "Neuen Chat beginnen",
        needs: &["new_chat_button"],
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "stop_generation",
        label: "Laufende Antwort abbrechen",
        needs: &["stop_button"],
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
        driveable: true,
        attainable: true,
    },
    Capability {
        key: "web_search",
        label: "Websuche zuschalten",
        needs: &["web_search_toggle"],
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
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "deep_research",
        label: "Deep Research starten",
        needs: &["deep_research_toggle"],
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "file_attach",
        label: "Datei anhängen",
        needs: &["attach_button"],
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
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "regenerate",
        label: "Antwort neu erzeugen",
        needs: &["regenerate_button"],
        driveable: false,
        attainable: true,
    },
    Capability {
        key: "temporary_chat",
        label: "Temporären Chat nutzen",
        needs: &["temporary_chat_button"],
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
        driveable: false,
        // Anklickbar, aber nicht belegbar: ein laufendes Mikrofon aendert
        // keinen pruefbaren Zustand. Nach dem eigenen Massstab — kein Beleg,
        // kein Level — darf es nicht zaehlen; dann gehoert es auch nicht in
        // den Nenner.
        attainable: false,
    },
    // Bewusst getrennt von `voice_input`: das Mikrofon diktiert in den Composer,
    // der Sprachdialog uebernimmt die ganze Unterhaltung. Wer beides in einen
    // Eintrag wirft, kann spaeter nicht sagen, was ein Brain wirklich anbietet.
    Capability {
        key: "voice_mode",
        label: "Sprachdialog-Modus",
        needs: &["voice_mode_button"],
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
        needs: &["reasoning_effort_menu"],
        driveable: false,
        attainable: true,
    },
    // Projekte sind persistenter Kontext ueber Chats hinweg. Fuer webagent
    // interessant, weil sich damit Dauerauftraege ablegen liessen, statt jeden
    // Chat bei null zu beginnen.
    Capability {
        key: "projects",
        label: "Projekte/Arbeitsbereiche",
        needs: &["projects_button"],
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
        &["deepthink", "deep think", "extended thinking", "reasoning", "think longer"],
    ),
    (
        "web_search",
        &["web search", "search the web", "websuche", "im web suchen"],
    ),
    ("model_switch", &["choose model", "model selector", "modell wählen", "switch model"]),
    ("deep_research", &["deep research", "tiefenrecherche"]),
    ("file_attach", &["attach", "upload file", "datei anhängen", "hochladen"]),
    ("canvas", &["canvas", "artifact", "artefakt"]),
    ("regenerate", &["regenerate", "neu generieren", "erneut generieren"]),
    ("temporary_chat", &["temporary chat", "temporärer chat", "incognito"]),
    ("new_chat", &["new chat", "neuer chat", "neuen chat"]),
    ("stop_generation", &["stop response", "stop generating", "antwort stoppen"]),
    // "voice" allein waere zu weit: es steckt auch in Beschriftungen, die den
    // Sprachdialog meinen. Deshalb nur Wortpaare bzw. das eindeutige Mikrofon.
    (
        "voice_input",
        &["mikrofon", "microphone", "spracheingabe", "voice input"],
    ),
    (
        "voice_mode",
        &["sprachmodus", "voice mode", "sprachdialog", "sprachmodus starten"],
    ),
    // Bewusst ohne das nackte "reasoning": das gehoert dem An/Aus-Schalter.
    // Hier zaehlt nur, was eine Stufe benennt.
    (
        "reasoning_effort",
        &["reasoning effort", "denkstufe", "denktiefe", "thinking effort"],
    ),
    ("projects", &["projekte", "projects", "new project", "neues projekt"]),
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
}

impl QuestBlocker {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuestBlocker::NeedsCode => "Code fehlt",
            QuestBlocker::NeedsSelectors => "Selektoren fehlen",
            QuestBlocker::NeedsBoth => "Code + Selektoren fehlen",
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
    /// `deepseek [1/5]` bzw. `deepseek [1/?]` solange unvermessen.
    pub fn label(&self) -> String {
        match self.max_level() {
            Some(m) => format!("{} [{}/{}]", self.brain_id, self.level(), m),
            None => format!("{} [{}/?]", self.brain_id, self.level()),
        }
    }
    /// Alles ausgereizt, was dieses Brain hergibt. Unvermessen ist nie maxed.
    pub fn maxed(&self) -> bool {
        matches!(self.max_level(), Some(m) if m > 0 && self.level() == m)
    }
    /// Rang-Titel — reine Anzeige, aus dem Anteil abgeleitet.
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
            _ => "stumm",
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
    for key in &available {
        let cap = match capability(key) {
            Some(c) => c,
            None => continue,
        };
        let has_sel = cap.needs.iter().all(|k| has_selector(sel, k));
        match (cap.driveable, has_sel) {
            (true, true) => have.push(cap.key.to_string()),
            (driveable, has_sel) => quests.push(Quest {
                brain_id: brain_id.to_string(),
                key: cap.key.to_string(),
                label: cap.label.to_string(),
                blocker: match (driveable, has_sel) {
                    (false, true) => QuestBlocker::NeedsCode,
                    (true, false) => QuestBlocker::NeedsSelectors,
                    _ => QuestBlocker::NeedsBoth,
                },
            }),
        }
    }
    // Fahrbares zählt auch bei unvermessenem Brain — nur der Nenner fehlt.
    if surveyed.is_none() {
        have = CATALOG
            .iter()
            .filter(|c| c.driveable && c.needs.iter().all(|k| has_selector(sel, k)))
            .map(|c| c.key.to_string())
            .collect();
        quests.clear();
    }
    BrainLevel {
        brain_id: brain_id.to_string(),
        surveyed: surveyed.is_some(),
        available,
        out_of_reach: unreachable,
        have,
        quests,
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
    grouped.sort_by(|a, b| {
        b.1.len()
            .cmp(&a.1.len())
            .then_with(|| a.0.cmp(&b.0))
    });
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_only() -> serde_json::Value {
        json!({
            "composer": ["#x"],
            "send_button": ["#y"],
            "assistant_message": ["#z"],
        })
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
        assert_eq!(got, vec!["chat", "new_chat", "reasoning_toggle", "web_search"]);
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
            json!({"al": "", "ti": "", "dt": "", "tp": "Denkstufe: Mittel"}),
            json!({"al": "Projekte", "ti": "", "dt": "", "tp": ""}),
        ];
        let got = detect_ui_options(&buttons, true);
        assert_eq!(
            got,
            vec!["chat", "voice_input", "voice_mode", "reasoning_effort", "projects"]
        );

        let sel = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "voice_input_button": ["#mic"],
            "voice_mode_button": ["#vm"],
            "reasoning_effort_menu": ["#eff"],
            "projects_button": ["#pr"],
            "ui_options": got,
        });
        let lvl = level_from_selectors("claude", &sel);
        // Mikrofon und Sprachdialog sind angeboten, aber nicht nachweisbar
        // fahrbar (attainable: false) — sie fallen aus dem Nenner und stehen
        // stattdessen in `out_of_reach`. Bliebe ein unerreichbarer Eintrag im
        // Nenner, koennte kein Brain je "ausgereizt" werden.
        //
        // `projects` wurde am 2026-07-28 fahrbar (open_section, URL-Beleg) und
        // zaehlt deshalb zum Zaehler statt zur Quest — der Test wandert mit
        // dem Code, seine Aussage bleibt: Selektor OHNE Code ist kein Koennen.
        assert_eq!(lvl.label(), "claude [2/3]", "chat und projects sind fahrbar");
        assert_eq!(lvl.out_of_reach, vec!["voice_input", "voice_mode"]);
        assert_eq!(lvl.quests.len(), 1, "nur die Denkstufe fehlt noch");
        assert_eq!(lvl.quests[0].key, "reasoning_effort");
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
        let lvl = level_from_selectors("t", &sel);
        assert_eq!(lvl.label(), "t [1/1]");
        assert!(lvl.maxed(), "ohne den unerreichbaren Posten ist das ausgereizt");
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
        let lvl = level_from_selectors("deepseek", &karg);
        assert_eq!(lvl.label(), "deepseek [1/2]");

        // Reiches UI, gleicher Code-Stand -> gleicher Zaehler, groesserer Nenner.
        let reich = json!({
            "composer": ["#x"], "send_button": ["#y"], "assistant_message": ["#z"],
            "ui_options": ["chat", "reasoning_toggle", "web_search", "canvas", "regenerate"],
        });
        let lvl2 = level_from_selectors("chatgpt", &reich);
        assert_eq!(lvl2.label(), "chatgpt [1/5]");
        assert_eq!(lvl.level(), lvl2.level(), "gleicher Stand, anderer Anspruch");
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
        let lvl = level_from_selectors("t", &sel);
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
        let lvl = level_from_selectors("t", &sel);
        let by = |k: &str| lvl.quests.iter().find(|q| q.key == k).map(|q| q.blocker);
        assert_eq!(by("new_chat"), Some(QuestBlocker::NeedsSelectors));
        assert_eq!(by("canvas"), Some(QuestBlocker::NeedsBoth));
    }

    #[test]
    fn without_ui_options_the_maximum_stays_unknown() {
        // Frueher galt hier "Nenner = vorhandene Selektoren". Das meldete jedes
        // Brain als ausgereizt, obwohl nur Text laeuft — ein Teilnahmepokal.
        // Unvermessen heisst jetzt `?`, und `?` ist nie "gemeistert".
        let lvl = level_from_selectors("t", &text_only());
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
        let lvl = level_from_selectors("t", &sel);
        assert_eq!(lvl.available, vec!["chat"], "Unfug zaehlt nicht zum Nenner");
    }

    #[test]
    fn partial_requirements_do_not_count_as_driveable() {
        // model_switch braucht beide Selektoren — einer allein reicht nicht.
        let sel = json!({ "model_menu": ["#m"], "ui_options": ["model_switch"] });
        let lvl = level_from_selectors("t", &sel);
        assert_eq!(lvl.level(), 0);
        assert_eq!(lvl.max_level(), Some(1));
        assert_eq!(lvl.rank(), "stumm");
    }

    #[test]
    fn empty_or_blank_selector_lists_do_not_count() {
        let sel = json!({ "new_chat_button": [], "stop_button": ["  "] });
        let lvl = level_from_selectors("t", &sel);
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
        for lvl in levels_all() {
            assert!(
                lvl.have.contains(&"chat".to_string()),
                "{} kann nicht mal Text senden",
                lvl.brain_id
            );
        }
    }

    #[test]
    fn quest_log_sorts_by_reach() {
        // Die Option, die die meisten Brains anbieten, steht oben — dort
        // bringt eine Implementierung den groessten Hebel.
        let log = quest_log();
        for w in log.windows(2) {
            assert!(w[0].1.len() >= w[1].1.len(), "Questlog nicht nach Reichweite sortiert");
        }
    }
}
