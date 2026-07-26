//! design_vote — Swarm entwirft, stimmt im Ausscheidungsverfahren ab, Gewinner
//! wird umgesetzt.
//!
//! Ablauf (Storax-Wunsch 2026-07-21, „kick vote jede Runde ein Vorschlag"):
//! 1. **Sammeln:** jedes Brain liefert EIN konkretes Design zum Thema.
//! 2. **Ausscheiden:** Runde für Runde nennt jedes lebende Brain den Vorschlag,
//!    der RAUS soll; der meistgenannte fliegt ([`crate::knockout::Bracket`]).
//! 3. **Gewinner:** der letzte Überlebende. Ein Brain darf ihn umsetzen (der
//!    Live-Teil in `main`, hier nur die Auswahl).
//!
//! Die Brain-Abfragen kommen als `query`-Closure herein — dieselbe Trennung wie
//! bei [`crate::self_research`], damit die Orchestrierung ohne Browser testbar
//! ist.

use crate::knockout::{parse_kick, Bracket};

/// Konfiguration eines Design-Votes.
#[derive(Debug, Clone)]
pub struct DesignVoteConfig {
    /// Teilnehmende Brains.
    pub brains: Vec<String>,
    /// Worum es geht (z.B. „das Worker-Pool-Dashboard der TUI").
    pub topic: String,
    /// Optionaler Kontext (aktuelles Layout, Randbedingungen).
    pub context: String,
    /// Art des Konsenses: TUI-Entwurf oder Bauplan fuer den Benchmark.
    pub mode: VoteMode,
}

/// Was die Brains vor der Abstimmung einreichen sollen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteMode {
    Design,
    ImplementationPlan,
}

/// Ergebnis eines Design-Votes.
#[derive(Debug, Clone)]
pub struct DesignVoteReport {
    /// `(brain, design)` je gesammeltem Vorschlag, in Sammelreihenfolge.
    pub proposals: Vec<(String, String)>,
    /// Reihenfolge des Ausscheidens `(vorschlag_index, in_runde)`.
    pub eliminated: Vec<(usize, usize)>,
    /// Gewinner-Index in `proposals`, falls einer feststeht.
    pub winner: Option<usize>,
}

impl DesignVoteReport {
    /// Gewinner als `(brain, design)`.
    pub fn winning_design(&self) -> Option<&(String, String)> {
        self.winner.and_then(|i| self.proposals.get(i))
    }
}

/// Prompt für die Sammelphase.
pub fn build_collect_prompt(topic: &str, context: &str) -> String {
    let ctx = if context.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nKontext:\n{}", context.trim())
    };
    format!(
        "Entwirf EIN konkretes Terminal-UI-Design (TUI) fuer {topic}.{ctx}\n\n\
         Beschreibe in EINEM Absatz: Gesamt-Layout und Panel-Aufteilung, was in \
         jedem Panel steht, Farb-/Statuskodierung, und die wichtigsten \
         Tastenbindungen/Interaktionen. Konkret und umsetzbar, KEIN Code, keine \
         Einleitung — nur das Design."
    )
}

/// Prompt fuer die Planungsphase des Benchmarks.
pub fn build_plan_collect_prompt(topic: &str, context: &str) -> String {
    let ctx = if context.trim().is_empty() {
        String::new()
    } else {
        format!("\n\nKontext:\n{}", context.trim())
    };
    format!(
        "Erstelle EINEN konkreten, begrenzten Implementierungsplan fuer {topic}.{ctx}\n\n\
         Nenne genau EINE Zieldatei aus der erlaubten Liste, die exakte Änderung/API, \
         die nötigen Tests und Risiken fuer Scope oder Kompatibilitaet. KEIN Code, \
         keine Einleitung, keine Architektur-Erweiterung — nur den umsetzbaren Plan."
    )
}

/// Prompt für eine Ausscheidungsrunde über die noch lebenden Designs.
pub fn build_kick_prompt(topic: &str, alive: &[(usize, &str)]) -> String {
    let mut list = String::new();
    for (display_nr, (_orig, text)) in alive.iter().enumerate() {
        list.push_str(&format!(
            "{}. {}\n\n",
            display_nr + 1,
            crate::char_prefix(text, 600)
        ));
    }
    format!(
        "Ausscheidungsrunde fuer den besten Implementierungsplan ({topic}). Verbleibende \
         Pläne:\n\n{list}\nNenne die EINE Nummer, die AUSSCHEIDEN soll (der \
         schwächste Plan), und begruende kurz. Antworte mit der Nummer zuerst, \
         z.B. '3 - zu ueberladen, ...'."
    )
}

/// Prompt fuer eine Ausscheidungsrunde über konkrete Implementierungspläne.
pub fn build_plan_kick_prompt(topic: &str, alive: &[(usize, &str)]) -> String {
    let mut list = String::new();
    for (display_nr, (_orig, text)) in alive.iter().enumerate() {
        list.push_str(&format!(
            "{}. {}\n\n",
            display_nr + 1,
            crate::char_prefix(text, 600)
        ));
    }
    format!(
        "Ausscheidungsrunde fuer den besten Implementierungsplan ({topic}). Verbleibende \
         Plaene:\n\n{list}\nNenne die EINE Nummer, die AUSSCHEIDEN soll (der \
         schwaechste Plan), und begruende kurz. Antworte mit der Nummer zuerst, \
         z.B. '3 - zu ueberladen, ...'."
    )
}

/// Aufgaben-Prompt für die Umsetzung des Gewinner-Designs.
pub fn build_implement_prompt(design: &str) -> String {
    format!(
        "Setze folgendes TUI-Design im Rust-Projekt webagent-rs um (ratatui, \
         Dateien src/tui_render.rs und src/tui_state.rs). Nutze das Rohformat \
         (WEBAGENT/1 EDIT/WRITE). Ergaenze Tests wo sinnvoll; `cargo test --lib` \
         muss gruen bleiben. Halte dich an das Design, erfinde keine neuen \
         Abhaengigkeiten.\n\nDesign:\n{design}"
    )
}

/// Fährt Sammeln + Ausscheiden. `on_round` meldet Fortschritt (Runde, Ereignis).
pub fn run_design_vote<Q>(
    config: &DesignVoteConfig,
    on_round: &dyn Fn(&str),
    query: Q,
) -> DesignVoteReport
where
    Q: Fn(&str, &str) -> Result<String, String>,
{
    // ---- Sammeln ----
    on_round(&format!(
        "Sammeln: {} Brains entwerfen",
        config.brains.len()
    ));
    let collect = match config.mode {
        VoteMode::Design => build_collect_prompt(&config.topic, &config.context),
        VoteMode::ImplementationPlan => build_plan_collect_prompt(&config.topic, &config.context),
    };
    let mut proposals: Vec<(String, String)> = Vec::new();
    for b in &config.brains {
        match query(b, &collect) {
            Ok(text)
                if !text.trim().is_empty() && !crate::brain::is_retryable_empty_response(&text) =>
            {
                on_round(&format!(
                    "  {b}: Entwurf ({} Zeichen)",
                    text.trim().chars().count()
                ));
                proposals.push((b.clone(), text.trim().to_string()));
            }
            Ok(_) => on_round(&format!("  {b}: kein verwertbarer Entwurf")),
            Err(e) => on_round(&format!("  {b}: Fehler — {e}")),
        }
    }

    if proposals.len() <= 1 {
        let winner = if proposals.is_empty() { None } else { Some(0) };
        return DesignVoteReport {
            proposals,
            eliminated: Vec::new(),
            winner,
        };
    }

    // ---- Ausscheiden ----
    let mut bracket = Bracket::new(proposals.len());
    let mut eliminated: Vec<(usize, usize)> = Vec::new();
    let mut round = 0usize;
    while !bracket.is_decided() {
        round += 1;
        // Anzeige-Liste der lebenden Designs (Anzeige-Nr → Original-Index).
        let alive_orig: Vec<usize> = bracket.alive().to_vec();
        let alive_view: Vec<(usize, &str)> = alive_orig
            .iter()
            .map(|&i| (i, proposals[i].1.as_str()))
            .collect();
        on_round(&format!(
            "Runde {round}: {} Designs, {} Brains kicken",
            alive_orig.len(),
            config.brains.len()
        ));
        let prompt = match config.mode {
            VoteMode::Design => build_kick_prompt(&config.topic, &alive_view),
            VoteMode::ImplementationPlan => build_plan_kick_prompt(&config.topic, &alive_view),
        };

        let mut kicks: Vec<usize> = Vec::new();
        for b in &config.brains {
            if let Ok(resp) = query(b, &prompt) {
                // Anzeige-Nummer (1-basiert über die lebende Liste) → Original-Index.
                if let Some(display_idx) = parse_kick(&resp, alive_orig.len()) {
                    kicks.push(alive_orig[display_idx]);
                }
            }
        }

        match bracket.eliminate(&kicks) {
            Some(loser) => {
                on_round(&format!(
                    "  ausgeschieden: #{loser} ({}, {})",
                    proposals[loser].0,
                    crate::char_prefix(&proposals[loser].1, 60)
                ));
                eliminated.push((loser, round));
            }
            None => {
                // Keine gültige Stimme — Patt. Um nicht endlos zu drehen, den
                // Kandidaten mit dem höchsten Original-Index entfernen (stabil).
                if let Some(&last) = alive_orig.last() {
                    on_round(&format!(
                        "  keine gueltige Stimme — entferne #{last} ({}, Patt)",
                        proposals[last].0
                    ));
                    let _ = bracket.eliminate(&[last]);
                    eliminated.push((last, round));
                } else {
                    break;
                }
            }
        }
    }

    let winner = bracket.winner();
    DesignVoteReport {
        proposals,
        eliminated,
        winner,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn collect_prompt_includes_topic_and_optional_context() {
        let p = build_collect_prompt("das Dashboard", "aktuell 3 Panels");
        assert!(p.contains("das Dashboard"));
        assert!(p.contains("aktuell 3 Panels"));
        assert!(p.contains("KEIN Code"));
        // Ohne Kontext kein leerer Kontextblock.
        assert!(!build_collect_prompt("x", "").contains("Kontext:"));
    }

    #[test]
    fn kick_prompt_numbers_alive_designs_from_one() {
        let alive = [(0usize, "Design A"), (3usize, "Design B")];
        let p = build_kick_prompt("die TUI", &alive);
        assert!(p.contains("1. Design A"));
        assert!(
            p.contains("2. Design B"),
            "Anzeige-Nummern, nicht Original-Indizes"
        );
    }

    #[test]
    fn full_vote_eliminates_down_to_one_winner() {
        // 3 Brains, jedes liefert ein Design; dann kicken sie bis einer bleibt.
        let brains = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let calls = RefCell::new(0usize);
        let query = |_b: &str, prompt: &str| -> Result<String, String> {
            if prompt.contains("Entwirf EIN") {
                // Sammelphase: jedes Brain ein unterscheidbares Design.
                let n = *calls.borrow();
                *calls.borrow_mut() += 1;
                Ok(format!(
                    "Design Nummer {n} mit Panels und Farben und Tasten fuer die TUI"
                ))
            } else {
                // Kick-Phase: alle kicken die Anzeige-Nummer 1 (das jeweils
                // erste lebende Design) -> die kleineren Original-Indizes fallen
                // zuerst, Gewinner ist der hoechste Index (c, Index 2).
                Ok("1 — schwaechstes".to_string())
            }
        };
        let report = run_design_vote(
            &DesignVoteConfig {
                brains,
                topic: "die TUI".to_string(),
                context: String::new(),
                mode: VoteMode::Design,
            },
            &|_| {},
            query,
        );
        assert_eq!(report.proposals.len(), 3);
        assert_eq!(report.eliminated.len(), 2, "n-1 Ausscheidungen");
        assert_eq!(report.winner, Some(2));
        assert!(report
            .winning_design()
            .unwrap()
            .1
            .contains("Design Nummer 2"));
    }

    #[test]
    fn a_single_valid_proposal_wins_without_voting() {
        let brains = vec!["a".to_string(), "b".to_string()];
        let query = |b: &str, _p: &str| -> Result<String, String> {
            if b == "a" {
                Ok(
                    "Ein vollstaendiges Design mit Layout, Panels, Farben und Tastenbindungen"
                        .to_string(),
                )
            } else {
                // b liefert nur eine UI-Ausfallmeldung -> zaehlt nicht.
                Ok("No response, Please try again later.".to_string())
            }
        };
        let report = run_design_vote(
            &DesignVoteConfig {
                brains,
                topic: "x".to_string(),
                context: String::new(),
                mode: VoteMode::Design,
            },
            &|_| {},
            query,
        );
        assert_eq!(report.proposals.len(), 1, "Ausfallmeldung ausgefiltert");
        assert_eq!(report.winner, Some(0));
        assert!(report.eliminated.is_empty());
    }

    #[test]
    fn no_proposals_means_no_winner() {
        let brains = vec!["a".to_string()];
        let query = |_b: &str, _p: &str| -> Result<String, String> { Ok(String::new()) };
        let report = run_design_vote(
            &DesignVoteConfig {
                brains,
                topic: "x".to_string(),
                context: String::new(),
                mode: VoteMode::Design,
            },
            &|_| {},
            query,
        );
        assert!(report.proposals.is_empty());
        assert_eq!(report.winner, None);
    }
}
