//! capability_types — der gemeinsame, abhängigkeitsfreie Kern des Katalogs.
//!
//! `capability` (Level, Quests, DOM-Erkennung) und `capability_proof`
//! (einziger Beleg-Gate) brauchen denselben Katalog und denselben
//! `Capability`-Typ. Liegen sie in einem der beiden Module, entsteht ein
//! Zyklus. Deshalb stehen Typen, Katalog und die reinen Katalog-Lookups hier:
//! die Abhängigkeit laeuft nur noch capability -> capability_proof ->
//! capability_types. Der Store bleibt unverändert in capability_proof.

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
    //
    // Am 2026-09-11 superzediert fuer deepseek: Die Instant/Expert/Vision-
    // Segmentleiste wurde vom Anbieter kommentarlos entfernt (User bestaetigt
    // Konsolidierung, OCR + Screenshot: Composer zeigt nur noch DeepThink/
    // Search-Toggles). Damit ist mode_switch im COUNTERPART — siehe
    // deepseek_mode_model_removed_2026-09-11.md — als nicht vorhanden belegt;
    // die Capability-Konzepte bleiben fuer Oberflaechen bestehen, die die
    // Leiste noch zeigen.
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
