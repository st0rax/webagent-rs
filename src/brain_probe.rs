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
            "{} {} {} {} {}",
            self.aria_label, self.text, self.test_id, self.id, self.placeholder
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
}

const RULES: &[Rule] = &[
    Rule {
        capability_key: "chat",
        selector_key: "send_button",
        needles: &[
            "send", "senden", "absenden", "abschicken", "nachricht senden",
            "send prompt", "send message",
        ],
        // „Sprachnachricht senden" ist der Sprachdialog, nicht der Absendeknopf.
        excludes: &["sprach", "voice", "audio"],
    },
    Rule {
        capability_key: "stop_generation",
        selector_key: "stop_button",
        needles: &[
            "stop", "stopp", "stoppen", "abbrechen", "beenden",
            "antwort stoppen", "generierung beenden", "stop streaming",
        ],
        excludes: &["sprach", "voice"],
    },
    Rule {
        capability_key: "new_chat",
        selector_key: "new_chat_button",
        needles: &[
            "new chat", "neuer chat", "neue unterhaltung", "neuer thread",
            "new conversation", "neues gespraech", "neues gespräch",
        ],
        excludes: &["temporary", "temporaer", "temporär"],
    },
    Rule {
        capability_key: "temporary_chat",
        selector_key: "temporary_chat_button",
        needles: &[
            "temporary chat", "temporaerer chat", "temporärer chat",
            "temporary", "temporaer", "temporär", "incognito",
        ],
        excludes: &[],
    },
    Rule {
        capability_key: "deep_research",
        selector_key: "deep_research_toggle",
        needles: &[
            "deep research", "deepresearch", "tiefe recherche",
            "tiefenrecherche", "ausfuehrliche recherche", "ausführliche recherche",
        ],
        excludes: &[],
    },
    Rule {
        capability_key: "web_search",
        selector_key: "web_search_toggle",
        needles: &[
            "web search", "websuche", "web-suche", "im internet suchen",
            "search the web", "internetsuche",
        ],
        excludes: &["deep"],
    },
    Rule {
        capability_key: "reasoning_toggle",
        selector_key: "reasoning_toggle",
        needles: &["reason", "denken", "nachdenken", "think", "thinking"],
        // Die Denkstufe ist eine eigene Faehigkeit (reasoning_effort).
        excludes: &["stufe", "effort", "tiefe"],
    },
    Rule {
        capability_key: "regenerate",
        selector_key: "regenerate_button",
        needles: &[
            "regenerate", "neu erzeugen", "neu generieren", "erneut versuchen",
            "try again", "wiederholen",
        ],
        excludes: &[],
    },
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
    'button', 'a[href]', '[role=button]', '[role=menuitem]', '[role=switch]',
    '[role=tab]', 'textarea', 'input[type=text]', '[contenteditable=true]'
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
}
