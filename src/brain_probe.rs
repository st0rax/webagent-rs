//! brain_probe — Oberflaechen-Analyse wie die Link-Analyse in JDownloader.
//!
//! Statt fuer jedes Brain die Selektoren von Hand zu pflegen, bekommt webagent
//! eine Chat-URL und klopft die Seite selbst ab: welche Bedienelemente gibt es,
//! welcher Faehigkeit aus [`crate::capability::CATALOG`] gehoert jedes davon,
//! und welcher Selektor trifft es stabil?
//!
//! # Warum das die drei offenen Faehigkeiten loest
//!
//! `deep_research` (kimi) fehlt der Selektor, `mode_switch` (deepseek) fehlt ein
//! nachweisbarer Zustandsmarker. Beides sind Suchaufgaben an der lebenden
//! Oberflaeche — genau das, was hier automatisiert wird, statt es pro Brain
//! einmal von Hand zu machen und beim naechsten UI-Umbau wieder zu verlieren.
//!
//! # Aufbau
//!
//! Das Sammeln braucht einen Browser, das Bewerten nicht. Deshalb dieselbe
//! Trennung wie in [`crate::design_vote`]: [`PROBE_SCRIPT`] holt rohe
//! Kandidaten aus dem DOM, [`classify`] ist reine Rechnung darauf und damit
//! ohne Browser testbar.

use serde::Deserialize;

use crate::page_driver::{PageDriver, Result};

/// Ein im DOM gefundenes Bedienelement, noch ohne Deutung.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Candidate {
    #[serde(default)]
    pub tag: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub aria_label: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub placeholder: String,
    #[serde(default)]
    pub test_id: String,
    #[serde(default)]
    pub id: String,
    /// Klassennamen — fuer Discovery von Bedienelementen ohne Text/role (z.B.
    /// kimi's Modell-Picker, ein `div[class*=...]` ohne aria-label).
    #[serde(default)]
    pub class: String,
    /// `title`-Attribut (Tooltip), oft die einzige Beschriftung eines Icon-Buttons.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub contenteditable: bool,
    /// Sichtbar und bedienbar — unsichtbare Treffer sind Rauschen.
    #[serde(default)]
    pub visible: bool,
}

impl Candidate {
    /// Alle Beschriftungsquellen zusammen, kleingeschrieben.
    ///
    /// Bewusst zusammengefasst: mal steht die Bedeutung im `aria-label`, mal im
    /// sichtbaren Text, mal nur im `data-testid`. Wer nur eine Quelle prueft,
    /// findet je nach Brain die Haelfte nicht.
    fn haystack(&self) -> String {
        format!(
            "{} {} {} {} {} {} {}",
            self.aria_label, self.text, self.test_id, self.id, self.placeholder, self.class, self.title
        )
        .to_lowercase()
    }
}

/// Vorschlag: dieser Selektor bedient diese Faehigkeit.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// Schluessel aus [`crate::capability::CATALOG`], z.B. `deep_research`.
    pub capability_key: &'static str,
    /// Schluessel in der Selektor-JSON, z.B. `deep_research_toggle`.
    pub selector_key: &'static str,
    /// Vorgeschlagener Selektor.
    pub selector: String,
    /// 0..100. Ein `data-testid` ist verlaesslicher als ein Textfund.
    pub confidence: u8,
    /// Woran es erkannt wurde — fuer die Nachpruefung durch einen Menschen.
    pub evidence: String,
}

/// Ein Suchmuster fuer eine Faehigkeit.
struct Rule {
    capability_key: &'static str,
    selector_key: &'static str,
    /// Treffer in irgendeiner Beschriftungsquelle. **Mehrsprachig**: die
    /// Oberflaechen laufen hier auf Deutsch. Ein rein englisches Muster greift
    /// nie — claudes Stopp-Knopf heisst „Antwort stoppen", nicht „stop".
    needles: &'static [&'static str],
    /// Diese Woerter schliessen einen Treffer aus (Falschfreunde).
    excludes: &'static [&'static str],
    /// Nur Kandidaten mit einer dieser `role`-Werte sind Kandidaten; `None`
    /// = jede Rolle erlaubt. Fuer Menue-Eintraege (`model_option`) zwingend,
    /// sonst matcht z.B. „Projekte" auf die Modell-Option, weil der Teilstring
    /// „pro" im aria-label steckt.
    roles: Option<&'static [&'static str]>,
}

macro_rules! rule {
    ($cap:expr, $key:expr, $needles:expr, $excludes:expr $(,)?) => {
        Rule {
            capability_key: $cap,
            selector_key: $key,
            needles: $needles,
            excludes: $excludes,
            roles: None,
        }
    };
    ($cap:expr, $key:expr, $needles:expr, $excludes:expr, $roles:expr $(,)?) => {
        Rule {
            capability_key: $cap,
            selector_key: $key,
            needles: $needles,
            excludes: $excludes,
            roles: Some($roles),
        }
    };
}

const RULES: &[Rule] = &[
    rule!(
        "chat",
        "send_button",
        &[
            "send", "senden", "absenden", "abschicken", "nachricht senden",
            "send prompt", "send message",
        ],
        &["sprach", "voice", "audio"],
    ),
    rule!(
        "stop_generation",
        "stop_button",
        &[
            "stop", "stopp", "stoppen", "abbrechen", "beenden",
            "antwort stoppen", "generierung beenden", "stop streaming",
        ],
        &["sprach", "voice"],
    ),
    rule!(
        "new_chat",
        "new_chat_button",
        &[
            "new chat", "neuer chat", "neue unterhaltung", "neuer thread",
            "new conversation", "neues gespraech", "neues gespräch",
        ],
        &["temporary", "temporaer", "temporär"],
    ),
    rule!(
        "temporary_chat",
        "temporary_chat_button",
        &[
            "temporary chat", "temporaerer chat", "temporärer chat",
            "temporary", "temporaer", "temporär", "incognito", "inkognito",
        ],
        &[],
    ),
    rule!(
        "deep_research",
        "deep_research_toggle",
        &[
            "deep research", "deepresearch", "tiefe recherche",
            "tiefenrecherche", "ausfuehrliche recherche", "ausführliche recherche",
        ],
        &[],
    ),
    rule!(
        "web_search",
        "web_search_toggle",
        &[
            "web search", "websuche", "web-suche", "im internet suchen",
            "search the web", "internetsuche",
        ],
        &["deep"],
    ),
    rule!(
        "reasoning_toggle",
        "reasoning_toggle",
        &["reason", "denken", "nachdenken", "think", "thinking"],
        &["stufe", "effort", "tiefe"],
    ),
    rule!(
        "regenerate",
        "regenerate_button",
        &[
            "regenerate", "neu erzeugen", "neu generieren", "erneut versuchen",
            "try again", "wiederholen",
        ],
        &[],
    ),
    rule!(
        "chat",
        "login_button",
        &[
            "anmelden", "log in", "sign in", "signin", "einloggen",
            "login", "se connecter", "sich anmelden",
        ],
        &["ab", "aus", "logout", "abmelden"],
    ),
    rule!(
        "model_switch",
        "model_menu",
        &[
            "modell", "model selector", "choose model", "switch model",
            "modell wählen", "change model", "model",
        ],
        &["option", "einstellung", "vereinbarung", "dienste", "richtlinie", "bedingungen", "nutzung"],
    ),
    rule!(
        "model_switch",
        "model_option",
        &[
            "gpt", "o3", "o4", "claude", "sonnet", "opus", "haiku",
            "deepseek", "reasoner", "kimi", "moonshot", "mistral", "mixtral",
            "gemini", "flash", "qwen", "max", "plus", "turbo", "glm",
            "llama", "perplexity", "sonar",
        ],
        &["modell wählen", "model selector", "switch model", "change model"],
        &["option", "menuitem", "radio"],
    ),
    rule!(
        "projects",
        "projects_button",
        &[
            "projekte", "projects", "arbeitsbereiche", "workspaces",
            "projektübersicht", "projektuebersicht",
        ],
        &[],
    ),
    rule!(
        "file_attach",
        "attach_button",
        &[
            "datei", "dateien", "anhängen", "anhaengen", "hinzufügen",
            "hinzufuegen", "attach", "upload", "hochladen",
        ],
        &["tool"],
    ),
    rule!(
        "voice_input",
        "voice_input_button",
        &[
            "mikrofon", "spracheingabe", "diktiermodus", "voice input",
            "diktieren",
        ],
        &[],
    ),
    rule!(
        "voice_mode",
        "voice_mode_button",
        &[
            "sprachmodus", "voice mode", "sprachdialog", "sprachmodus verwenden",
        ],
        &[],
    ),
    rule!(
        // Kein Capability-Key im Katalog: `consent_reject_button` ist ein
        // reiner Dialog-Selektor fuer `dismiss_consent`, kein Level-Faehigkeit.
        // Der capability_key bleibt dennoch ein gueltiges Katalog-Mitglied,
        // damit die Deutung typgleich bleibt.
        "chat",
        "consent_reject_button",
        &[
            "nur notwendige", "ablehnen", "reject all", "tout refuser",
            "nur notwendige cookies", "nicht akzeptieren",
        ],
        &["alle", "zulassen", "akzeptieren", "accept"],
    ),
];

/// JS, das die Bedienelemente der Seite einsammelt.
///
/// Sammelt bewusst grob und filtert erst in Rust: was ein Knopf bedeutet,
/// entscheidet sich an Beschriftungen in mehreren Sprachen, und diese Logik
/// gehoert an eine testbare Stelle — nicht in einen String, den kein Test je
/// ausfuehrt.
pub const PROBE_SCRIPT: &str = r#"
(() => {
  const out = [];
  const sel = [
    'button', 'a[href]', '[role=button]', '[role=menuitem]', '[role=option]',
    '[role=switch]', '[role=radio]', '[role=tab]', 'textarea',
    'input[type=text]', '[contenteditable=true]'
  ].join(', ');
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    out.push({
      tag: el.tagName.toLowerCase(),
      role: el.getAttribute('role') || '',
      aria_label: el.getAttribute('aria-label') || '',
      text: (el.innerText || el.textContent || '').trim().slice(0, 80),
      placeholder: el.getAttribute('placeholder') || '',
      test_id: el.getAttribute('data-testid') || '',
      id: el.id || '',
      contenteditable: el.getAttribute('contenteditable') === 'true',
      visible: r.width > 0 && r.height > 0
    });
  }
  return out;
})()
"#;

/// Baut den stabilsten Selektor fuer ein Element.
///
/// Reihenfolge nach Haltbarkeit: `data-testid` ueberlebt Umbauten am ehesten,
/// eine `id` meist auch; `aria-label` ist sprachabhaengig und faellt beim
/// naechsten Sprachwechsel um; reiner Text ist der letzte Ausweg.
fn selector_for(candidate: &Candidate) -> Option<(String, u8, String)> {
    if !candidate.test_id.is_empty() {
        return Some((
            format!("[data-testid='{}']", candidate.test_id),
            95,
            format!("data-testid={}", candidate.test_id),
        ));
    }
    if !candidate.id.is_empty() {
        return Some((
            format!("#{}", candidate.id),
            85,
            format!("id={}", candidate.id),
        ));
    }
    if !candidate.aria_label.is_empty() {
        // `i` = ohne Ruecksicht auf Gross-/Kleinschreibung; die Oberflaechen
        // sind darin nicht konsistent.
        return Some((
            format!("{}[aria-label*='{}' i]", candidate.tag, candidate.aria_label),
            70,
            format!("aria-label={}", candidate.aria_label),
        ));
    }
    if !candidate.text.is_empty() {
        return Some((
            format!("{}:has-text('{}')", candidate.tag, candidate.text),
            50,
            format!("text={}", candidate.text),
        ));
    }
    None
}

/// Findet den Composer: das Eingabefeld fuer die Nachricht.
fn classify_composer(candidates: &[Candidate]) -> Option<Proposal> {
    let field = candidates.iter().find(|c| {
        c.visible && (c.contenteditable || c.tag == "textarea") && !c.haystack().contains("such")
    })?;
    let (selector, confidence, evidence) = selector_for(field).or_else(|| {
        // Ein Composer ohne jede Beschriftung ist trotzdem einer.
        Some((
            if field.contenteditable {
                "[contenteditable='true']".to_string()
            } else {
                "textarea".to_string()
            },
            40,
            "Formfeld ohne Kennzeichnung".to_string(),
        ))
    })?;
    Some(Proposal {
        capability_key: "chat",
        selector_key: "composer",
        selector,
        confidence,
        evidence,
    })
}

/// Deutet die eingesammelten Kandidaten.
///
/// Reine Rechnung — kein Browser, kein Netz. Genau deshalb pruefbar.
pub fn classify(candidates: &[Candidate]) -> Vec<Proposal> {
    let mut proposals: Vec<Proposal> = Vec::new();
    if let Some(composer) = classify_composer(candidates) {
        proposals.push(composer);
    }

    for rule in RULES {
        // Bester Treffer je Regel: hoechste Selektor-Guete gewinnt, damit ein
        // beschriftungsloser Zufallstreffer keinen data-testid verdraengt.
        let mut best: Option<Proposal> = None;
        for candidate in candidates.iter().filter(|c| c.visible) {
            if let Some(roles) = rule.roles {
                if !roles.contains(&candidate.role.as_str()) {
                    continue;
                }
            }
            let haystack = candidate.haystack();
            if haystack.trim().is_empty() {
                continue;
            }
            if rule.excludes.iter().any(|bad| haystack.contains(bad)) {
                continue;
            }
            if !rule.needles.iter().any(|n| haystack.contains(n)) {
                continue;
            }
            let Some((selector, confidence, evidence)) = selector_for(candidate) else {
                continue;
            };
            let proposal = Proposal {
                capability_key: rule.capability_key,
                selector_key: rule.selector_key,
                selector,
                confidence,
                evidence,
            };
            if best.as_ref().is_none_or(|b| b.confidence < proposal.confidence) {
                best = Some(proposal);
            }
        }
        if let Some(found) = best {
            proposals.push(found);
        }
    }
    proposals
}

/// Faehrt die Analyse gegen eine offene Seite.
///
/// Der Browser-Teil ist absichtlich duenn: einsammeln und deuten lassen. So
/// bleibt die Logik im testbaren Teil.
pub fn probe(driver: &mut dyn PageDriver) -> Result<Vec<Proposal>> {
    let raw = driver.evaluate(PROBE_SCRIPT)?;
    let candidates: Vec<Candidate> = serde_json::from_value(raw).unwrap_or_default();
    Ok(classify(&candidates))
}

/// Ergebnis der Nachpruefung eines [`Proposal`]s an der lebenden Oberflaeche.
///
/// Ein Vorschlag ist ein Fund, keine Faehigkeit. Erst wenn ein Klick
/// nachweislich einen lesbaren Zustand geaendert hat, gilt sie als fahrbar —
/// „kein Beleg, kein Level".
#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub capability_key: &'static str,
    pub selector_key: &'static str,
    pub selector: String,
    /// Zustand vor dem Klick (Ausgabe der gemeinsamen Zustandsauslese).
    pub before: String,
    /// Zustand nach dem Klick.
    pub after: String,
    /// Nur `true`, wenn sich der Zustand messbar geaendert hat.
    pub proven: bool,
    /// Wurde der Ausgangszustand wiederhergestellt? `None`, wenn gar nichts zu
    /// widerrufen war (kein Wechsel).
    pub restored: Option<bool>,
    /// Klartext fuer Menschen — auch und gerade im Nicht-belegt-Fall.
    pub note: String,
}

/// Klickt einen Vorschlag an und prueft, ob sich ein lesbarer Zustand aendert.
///
/// Wichtig: **kein** Zustandswechsel ist ein gueltiges Ergebnis, kein Fehler.
/// Genau dieser Fall ist bei deepseeks `mode_switch` real aufgetreten — die
/// Segmente tragen keinerlei Auswahlmarkierung —, und er ist der Grund, warum
/// die Faehigkeit dort `driveable:false` steht. Ein `Err` gibt es nur, wenn die
/// Seite selbst nicht antwortet.
///
/// Nebenwirkungsarm: hat der Klick gewirkt, wird zurueckgeklickt und vermerkt,
/// ob der Rueckweg gelang. Ein Test, der die Oberflaeche anders hinterlaesst,
/// als er sie vorfand, ist eine Nebenwirkung — keine Messung.
pub fn verify(driver: &mut dyn PageDriver, proposal: &Proposal) -> Result<Verdict> {
    // Dieselben JS-Bausteine wie der Umschaltpfad des Backends
    // (`browser::ui::toggle_state_expr` / `click_toggle_expr`); nur die Quelle
    // der Selektorliste ist eine andere. Eine zweite handgepflegte Kopie waere
    // der doppelte Auslesepfad, an dem hier schon Fixes vorbeigelaufen sind.
    let selectors = vec![proposal.selector.clone()];
    let state_expr = crate::browser::js::toggle_state_expr_for(&selectors);
    let click_expr = crate::browser::js::click_toggle_expr_for(&selectors);

    let before = driver.eval_string(&state_expr)?;
    let clicked = driver.evaluate(&click_expr)?.as_bool().unwrap_or(false);
    if !clicked {
        return Ok(Verdict {
            capability_key: proposal.capability_key,
            selector_key: proposal.selector_key,
            selector: proposal.selector.clone(),
            before: before.clone(),
            after: before,
            proven: false,
            restored: None,
            note: format!("Selektor '{}' war nicht anklickbar", proposal.selector),
        });
    }
    let after = driver.eval_string(&state_expr)?;

    if before == after {
        return Ok(Verdict {
            capability_key: proposal.capability_key,
            selector_key: proposal.selector_key,
            selector: proposal.selector.clone(),
            before,
            after,
            proven: false,
            restored: None,
            note: "Klick kam an, Zustand unveraendert — kein Beleg, kein Level".into(),
        });
    }

    // Zustand hat sich geaendert: zurueckschalten und den Rueckweg belegen.
    let mut restored = false;
    if driver.evaluate(&click_expr)?.as_bool().unwrap_or(false) {
        restored = driver.eval_string(&state_expr)? == before;
    }
    let note = if restored {
        "Zustandswechsel belegt, Ausgangszustand wiederhergestellt".to_string()
    } else {
        format!(
            "Zustandswechsel belegt, aber Rueckweg misslungen — Oberflaeche steht jetzt auf '{after}' statt '{before}'"
        )
    };
    Ok(Verdict {
        capability_key: proposal.capability_key,
        selector_key: proposal.selector_key,
        selector: proposal.selector.clone(),
        before,
        after,
        proven: true,
        restored: Some(restored),
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn button(aria: &str) -> Candidate {
        Candidate {
            tag: "button".into(),
            aria_label: aria.into(),
            visible: true,
            ..Default::default()
        }
    }

    #[test]
    fn deutsche_beschriftungen_werden_gefunden() {
        // Der Kern: die Oberflaechen laufen hier auf Deutsch. Ein rein
        // englisches Muster findet claudes Stopp-Knopf nie.
        let found = classify(&[button("Antwort stoppen")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].capability_key, "stop_generation");
        assert_eq!(found[0].selector_key, "stop_button");
    }

    #[test]
    fn englische_beschriftungen_ebenso() {
        let found = classify(&[button("Stop streaming")]);
        assert_eq!(found[0].capability_key, "stop_generation");
    }

    #[test]
    fn testid_schlaegt_aria_label() {
        // Zwei Kandidaten fuer dieselbe Faehigkeit: der stabilere gewinnt.
        let candidates = vec![
            button("Nachricht senden"),
            Candidate {
                tag: "button".into(),
                test_id: "send-button".into(),
                text: "Senden".into(),
                visible: true,
                ..Default::default()
            },
        ];
        let found = classify(&candidates);
        let send = found.iter().find(|p| p.selector_key == "send_button").unwrap();
        assert_eq!(send.selector, "[data-testid='send-button']");
        assert!(send.confidence >= 95);
    }

    #[test]
    fn unsichtbare_elemente_zaehlen_nicht() {
        let hidden = Candidate {
            visible: false,
            ..button("Antwort stoppen")
        };
        assert!(classify(&[hidden]).is_empty());
    }

    #[test]
    fn falschfreunde_werden_ausgeschlossen() {
        // „Sprachnachricht senden" ist der Sprachdialog, nicht der Absendeknopf.
        assert!(classify(&[button("Sprachnachricht senden")])
            .iter()
            .all(|p| p.selector_key != "send_button"));
        // Ein temporaerer Chat ist kein neuer Chat.
        let found = classify(&[button("Temporärer Chat")]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].capability_key, "temporary_chat");
    }

    #[test]
    fn findet_die_bisher_fehlenden_faehigkeiten() {
        // deep_research fehlte kimi als Selektor, temporary_chat qwen als Code.
        // Genau die soll die Analyse selbst finden.
        let found = classify(&[
            button("Deep Research"),
            button("Tiefe Recherche"),
            button("Temporärer Chat"),
        ]);
        let keys: Vec<&str> = found.iter().map(|p| p.capability_key).collect();
        assert!(keys.contains(&"deep_research"), "{keys:?}");
        assert!(keys.contains(&"temporary_chat"), "{keys:?}");
    }

    #[test]
    fn perplexity_deutsche_oberflaeche_wird_gedeutet() {
        // Real aus dem Perplexity-DOM geerntet (headless, 2026-07): deutsche
        // Beschriftungen, die die englischen Muster alle verfehlt haetten.
        let candidates = vec![
            Candidate {
                tag: "button".into(),
                aria_label: "Modell".into(),
                text: "Modell".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "a".into(),
                aria_label: "Projekte".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                text: "Anmelden".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                aria_label: "Inkognito verwenden: Anonyme Sitzungen erstellen".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                aria_label: "Diktiermodus".into(),
                visible: true,
                ..Default::default()
            },
            Candidate {
                tag: "button".into(),
                text: "Nur notwendige".into(),
                visible: true,
                ..Default::default()
            },
        ];
        let found = classify(&candidates);
        let keys: Vec<(String, String)> = found
            .iter()
            .map(|p| (p.capability_key.to_string(), p.selector_key.to_string()))
            .collect();
        assert!(keys.contains(&("model_switch".into(), "model_menu".into())), "{keys:?}");
        assert!(keys.contains(&("projects".into(), "projects_button".into())), "{keys:?}");
        assert!(keys.contains(&("chat".into(), "login_button".into())), "{keys:?}");
        assert!(keys.contains(&("temporary_chat".into(), "temporary_chat_button".into())), "{keys:?}");
        assert!(keys.contains(&("voice_input".into(), "voice_input_button".into())), "{keys:?}");
        assert!(keys.contains(&("chat".into(), "consent_reject_button".into())), "{keys:?}");
    }

    #[test]
    fn alle_zulassen_ist_kein_ablehnen() {
        // „Alle zulassen" (Perplexity-Cookie-Banner) ist Zustimmen — der
        // Ablehnen-Selektor muss es uebersehen.
        let found = classify(&[button("Alle zulassen")]);
        assert!(
            found.iter().all(|p| p.selector_key != "consent_reject_button"),
            "{found:?}"
        );
    }

    #[test]
    fn composer_wird_erkannt_auch_ohne_beschriftung() {
        let candidates = vec![Candidate {
            tag: "div".into(),
            contenteditable: true,
            visible: true,
            ..Default::default()
        }];
        let found = classify(&candidates);
        let composer = found.iter().find(|p| p.selector_key == "composer").unwrap();
        assert_eq!(composer.selector, "[contenteditable='true']");
    }

    #[test]
    fn suchfeld_wird_nicht_als_composer_verwechselt() {
        let candidates = vec![Candidate {
            tag: "textarea".into(),
            placeholder: "Suchen".into(),
            visible: true,
            ..Default::default()
        }];
        assert!(classify(&candidates)
            .iter()
            .all(|p| p.selector_key != "composer"));
    }

    #[test]
    fn probe_script_hat_keine_zeilenuebergreifenden_stringliterale() {
        // Diese Klasse Fehler faellt sonst durch JEDES Sieb: kein Unit-Test
        // fuehrt das JS aus, also meldet die Suite gruen, waehrend das Skript
        // im Browser an einem Syntaxfehler stirbt. Genau so ist es hier beim
        // ersten Wurf passiert — ein ueber zwei Zeilen gebrochenes
        // '...'-Literal. Der Test ist billig und faengt die Wiederholung.
        for (number, line) in PROBE_SCRIPT.lines().enumerate() {
            let code = line.split("//").next().unwrap_or(line);
            assert_eq!(
                code.matches('\'').count() % 2,
                0,
                "Zeile {}: unpaarige Anfuehrungszeichen — {line}",
                number + 1
            );
        }
    }

    #[test]
    fn probe_faehrt_ueber_den_page_driver() {
        // Beweist die Verdrahtung ohne Browser: der Mock liefert die Rohdaten,
        // die Deutung passiert in Rust.
        let dom = serde_json::json!([
            {"tag": "button", "aria_label": "Antwort stoppen", "visible": true},
            {"tag": "div", "contenteditable": true, "visible": true}
        ]);
        let state = crate::mock_page::MockPageState::new().on_eval(PROBE_SCRIPT, dom);
        let mut driver = crate::mock_page::MockPageDriver::new(state);
        let found = probe(&mut driver).expect("probe");
        let keys: Vec<&str> = found.iter().map(|p| p.selector_key).collect();
        assert!(keys.contains(&"stop_button"), "{keys:?}");
        assert!(keys.contains(&"composer"), "{keys:?}");
    }

    fn proposal() -> Proposal {
        Proposal {
            capability_key: "reasoning_toggle",
            selector_key: "reasoning_toggle",
            selector: "[data-testid='think']".into(),
            confidence: 95,
            evidence: "data-testid=think".into(),
        }
    }

    /// Baut den Mock so, wie `verify` fragt: Zustandsauslese als Antwortfolge,
    /// Klick als Festwert.
    fn mock(states: Vec<&str>, click: bool) -> crate::mock_page::MockPageDriver {
        let sels = vec![proposal().selector];
        let state_expr = crate::browser::js::toggle_state_expr_for(&sels);
        let click_expr = crate::browser::js::click_toggle_expr_for(&sels);
        let state = crate::mock_page::MockPageState::new()
            .on_eval_seq(
                state_expr,
                states.into_iter().map(|s| serde_json::json!(s)).collect(),
            )
            .on_eval(click_expr, serde_json::json!(click));
        crate::mock_page::MockPageDriver::new(state)
    }

    #[test]
    fn verify_belegt_zustandswechsel_und_raeumt_auf() {
        // vorher / nachher / nach dem Rueckklick wieder wie vorher.
        let mut driver = mock(vec!["aria-pressed=false|", "aria-pressed=true|", "aria-pressed=false|"], true);
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(verdict.proven);
        assert_eq!(verdict.restored, Some(true));
        assert_ne!(verdict.before, verdict.after);
    }

    #[test]
    fn verify_ohne_wechsel_ist_kein_fehler() {
        // deepseeks mode_switch: die Segmente tragen keine Auswahlmarkierung.
        // Der Klick kommt an, der Zustand bleibt — das ist ein sauberes
        // Ergebnis (proven=false), kein Err.
        let mut driver = mock(vec!["|seg"], true);
        let verdict = verify(&mut driver, &proposal()).expect("verify darf hier nicht scheitern");
        assert!(!verdict.proven);
        assert_eq!(verdict.restored, None);
        assert_eq!(verdict.before, verdict.after);
        assert!(verdict.note.contains("unveraendert"), "{}", verdict.note);
    }

    #[test]
    fn verify_meldet_misslungenen_rueckweg() {
        // Der dritte Wert bleibt beim geaenderten Zustand: der Rueckklick hat
        // nicht zurueckgeschaltet. Belegt ist die Faehigkeit trotzdem — aber
        // die Nebenwirkung muss sichtbar sein.
        let mut driver = mock(vec!["aria-pressed=false|", "aria-pressed=true|", "aria-pressed=true|"], true);
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(verdict.proven);
        assert_eq!(verdict.restored, Some(false));
        assert!(verdict.note.contains("Rueckweg misslungen"), "{}", verdict.note);
    }

    #[test]
    fn verify_meldet_nicht_anklickbaren_selektor() {
        let mut driver = mock(vec!["aria-pressed=false|"], false);
        let verdict = verify(&mut driver, &proposal()).expect("verify");
        assert!(!verdict.proven);
        assert!(verdict.note.contains("nicht anklickbar"), "{}", verdict.note);
    }

    #[test]
    fn verify_nutzt_dieselbe_zustandsauslese_wie_der_toggle_pfad() {
        // Der Kern der Extraktion: haette verify eine eigene Kopie des JS,
        // wuerde dieser Vergleich auseinanderlaufen — und ein Fix am einen
        // Pfad am anderen vorbei.
        let sels = vec!["[data-testid='think']".to_string()];
        let expr = crate::browser::js::toggle_state_expr_for(&sels);
        assert!(expr.contains("closest("), "{expr}");
        assert!(expr.contains("data-"), "{expr}");
        assert!(crate::browser::js::click_toggle_expr_for(&sels).contains("t.click()"));
    }
}
