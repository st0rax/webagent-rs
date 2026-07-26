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
    /// Einstimmig ratifizierter Endtext. Ohne ihn darf der Benchmark den
    /// Kick-Vote-Gewinner nicht automatisch umsetzen.
    pub approved: Option<String>,
    /// Konkrete Änderungswünsche aus der ersten Ratifikationsrunde.
    pub amendments: Vec<(String, String)>,
}

impl DesignVoteReport {
    /// Gewinner als `(brain, design)`.
    pub fn winning_design(&self) -> Option<&(String, String)> {
        self.winner.and_then(|i| self.proposals.get(i))
    }

    /// Nur ein einstimmig bestätigter Entwurf ist ein automatischer Bauauftrag.
    pub fn approved_text(&self) -> Option<&str> {
        self.approved.as_deref()
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

/// Eine Rangwahl ersetzt beim Implementierungsplan mehrere teure Browser-Wellen.
/// Die Nummern werden vom schwächsten zum stärksten Plan angegeben.
fn build_plan_rank_prompt(topic: &str, alive: &[(usize, &str)]) -> String {
    let mut list = String::new();
    for (display_nr, (_orig, text)) in alive.iter().enumerate() {
        list.push_str(&format!(
            "{}. {}\n\n",
            display_nr + 1,
            crate::char_prefix(text, 600)
        ));
    }
    format!(
        "Schnelle Rangwahl für den besten Implementierungsplan ({topic}). Pläne:\n\n{list}\n\
         Antworte ausschließlich mit ALLEN Nummern, vom schwächsten zum stärksten, \
         durch Kommas getrennt. Beispiel für drei Pläne: `3,1,2`."
    )
}

fn parse_ranking(response: &str, count: usize) -> Vec<usize> {
    let mut ranking = Vec::new();
    for token in response.split(|ch: char| !ch.is_ascii_digit()) {
        let Ok(number) = token.parse::<usize>() else {
            continue;
        };
        if (1..=count).contains(&number) && !ranking.contains(&(number - 1)) {
            ranking.push(number - 1);
        }
    }
    ranking
}

fn build_ratify_prompt(topic: &str, proposal: &str) -> String {
    format!(
        "Ratifikation des letzten Konsensvorschlags für {topic}:\n\n{proposal}\n\n\
         Antworte exakt mit `JA`, wenn du zustimmst. Andernfalls nenne genau \
         EINEN konkreten, begrenzten Änderungswunsch. Keine Diskussion."
    )
}

fn build_revision_prompt(
    topic: &str,
    proposal: &str,
    amendments: &[(String, String)],
    revision: usize,
    maximum_revisions: usize,
) -> String {
    let wishes = amendments
        .iter()
        .map(|(brain, wish)| format!("- {brain}: {}", crate::char_prefix(wish, 300)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Überarbeite diesen Konsensvorschlag für {topic} (Revision {revision} von maximal {maximum_revisions}):\n\n{proposal}\n\n\
         Geprüfte Änderungswünsche:\n{wishes}\n\n\
         Liefere nur den vollständigen, weiterhin begrenzten Endvorschlag. \
         Übernimm nur Wünsche, die Scope und Kompatibilität nicht verletzen."
    )
}

fn build_leader_decision_prompt(
    topic: &str,
    proposal: &str,
    remaining_amendments: &[(String, String)],
) -> String {
    let wishes = remaining_amendments
        .iter()
        .map(|(brain, wish)| format!("- {brain}: {}", crate::char_prefix(wish, 300)))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Du bist aufgrund des objektiven Code-Scoreboards der Tie-Breaker für {topic}.\n\n\
         Der Schwarm erreichte nach begrenzten, nachvollziehbaren Revisionen keine Einstimmigkeit.\n\n\
         Aktueller Plan:\n{proposal}\n\n\
         Noch offene Änderungswünsche:\n{wishes}\n\n\
         Entscheide jetzt verbindlich: Liefere ausschließlich einen vollständigen, kleinen und \
         umsetzbaren Endplan. Berücksichtige nur kompatible Wünsche; erweitere den Scope nicht."
    )
}

/// Führender, aktuell teilnehmender Brain nach objektivem Code-Score.
fn scoreboard_leader(brains: &[String]) -> Option<String> {
    crate::code_score::leaderboard()
        .into_iter()
        .find(|stats| brains.iter().any(|brain| brain == &stats.brain_id))
        .map(|stats| stats.brain_id)
}

fn is_yes(response: &str) -> bool {
    matches!(
        response.trim().to_uppercase().as_str(),
        "JA" | "YES" | "ZUSTIMMUNG"
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
    Q: Fn(&str, &str) -> Result<String, String> + Send + Sync,
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
    // Alle Entwuerfe sind voneinander unabhaengig. Parallel abfragen spart
    // Wartezeit, aber die Auswertung bleibt in Brain-Reihenfolge stabil.
    let collected: Vec<(&String, Result<String, String>)> = std::thread::scope(|scope| {
        let query_ref = &query;
        let jobs: Vec<_> = config
            .brains
            .iter()
            .map(|brain| {
                let prompt = &collect;
                scope.spawn(move || (brain, query_ref(brain, prompt)))
            })
            .collect();
        jobs.into_iter()
            .map(|job| job.join().expect("Design-Vote-Worker panicked"))
            .collect()
    });
    let mut proposals: Vec<(String, String)> = Vec::new();
    for (b, response) in collected {
        match response {
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
            approved: None,
            amendments: Vec::new(),
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
            VoteMode::ImplementationPlan => build_plan_rank_prompt(&config.topic, &alive_view),
        };

        // Jede Stimme bezieht sich auf denselben unveraenderlichen Katalog;
        // deshalb sind auch die Kick-Abfragen sicher parallelisierbar.
        let responses: Vec<Result<String, String>> = std::thread::scope(|scope| {
            let query_ref = &query;
            let jobs: Vec<_> = config
                .brains
                .iter()
                .map(|brain| {
                    let ballot = &prompt;
                    scope.spawn(move || query_ref(brain, ballot))
                })
                .collect();
            jobs.into_iter()
                .map(|job| job.join().expect("Design-Vote-Worker panicked"))
                .collect()
        });
        if config.mode == VoteMode::ImplementationPlan {
            // Borda-Auswertung der vollständigen Ranglisten. Wir entfernen etwa
            // die schwächere Hälfte pro Welle – 7 Pläne brauchen so drei statt
            // sechs Browser-Abstimmungen, ohne eine lokale Bauchentscheidung.
            let mut scores = vec![0usize; alive_orig.len()];
            let mut valid_rankings = 0usize;
            for response in responses
                .iter()
                .filter_map(|response| response.as_ref().ok())
            {
                let ranking = parse_ranking(response, alive_orig.len());
                if ranking.len() < 2 {
                    continue;
                }
                valid_rankings += 1;
                for (position, display_idx) in ranking.into_iter().enumerate() {
                    scores[display_idx] += position;
                }
            }
            if valid_rankings > 0 {
                let remove_count = (alive_orig.len() - 1).div_ceil(2);
                let mut order: Vec<usize> = (0..alive_orig.len()).collect();
                order.sort_by_key(|&display_idx| (scores[display_idx], alive_orig[display_idx]));
                for display_idx in order.into_iter().take(remove_count) {
                    let loser = alive_orig[display_idx];
                    let _ = bracket.eliminate(&[loser]);
                    on_round(&format!(
                        "  ausgeschieden: #{loser} ({}, Rangwahl)",
                        proposals[loser].0
                    ));
                    eliminated.push((loser, round));
                }
                continue;
            }
        }
        let mut kicks: Vec<usize> = Vec::new();
        for resp in responses.into_iter().flatten() {
            {
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
    let Some(winner_index) = winner else {
        return DesignVoteReport {
            proposals,
            eliminated,
            winner: None,
            approved: None,
            amendments: Vec::new(),
        };
    };

    // ---- Begrenzte Ratifikation ----
    // Ein nicht einstimmiger Plan startet ausdrücklich keinen neuen Kick-Vote:
    // derselbe Gewinner wird mit den konkreten Einwänden weiter verfeinert.
    // Sichtbarer Konsensfortschritt (weniger offene Wünsche) erweitert das
    // Budget, aber eine harte Obergrenze verhindert Endlosschleifen.
    let collect_amendments = |prompt: &str| -> Vec<(String, String)> {
        let query_ref = &query;
        let ratify: Vec<(String, Result<String, String>)> = std::thread::scope(|scope| {
            let jobs: Vec<_> = config
                .brains
                .iter()
                .map(|brain| scope.spawn(move || (brain.clone(), query_ref(brain, prompt))))
                .collect();
            jobs.into_iter()
                .map(|job| job.join().expect("Ratifikations-Worker panicked"))
                .collect()
        });
        ratify
            .into_iter()
            .filter_map(|(brain, response)| match response {
                Ok(text) if is_yes(&text) => None,
                Ok(text)
                    if !text.trim().is_empty()
                        && !crate::brain::is_retryable_empty_response(&text) =>
                {
                    Some((brain, text.trim().to_string()))
                }
                // Ausgefallene Provider sind weder Zustimmung noch Veto.
                _ => None,
            })
            .collect()
    };
    let mut proposal = proposals[winner_index].1.clone();
    let mut amendments = Vec::new();
    let mut approved = None;
    const BASE_REVISIONS: usize = 3;
    const MAX_REVISIONS: usize = 6;
    let mut revision = 0usize;
    let mut revision_limit = BASE_REVISIONS;
    let mut previous_amendment_count = None;
    let mut remaining_amendments = Vec::new();
    loop {
        on_round(if revision == 0 {
            "Ratifikation: alle Brains stimmen zu oder nennen einen Änderungswunsch"
        } else {
            "Ratifikation der überarbeiteten Fassung: Zustimmung oder ein Änderungswunsch"
        });
        let round_amendments = collect_amendments(&build_ratify_prompt(&config.topic, &proposal));
        if round_amendments.is_empty() {
            on_round("Ratifikation: einstimmig bestätigt");
            approved = Some(proposal.clone());
            break;
        }
        amendments.extend(round_amendments.iter().cloned());
        remaining_amendments = round_amendments.clone();
        let amendment_count = round_amendments.len();
        if previous_amendment_count.is_some_and(|previous| amendment_count < previous)
            && revision_limit < MAX_REVISIONS
        {
            revision_limit += 1;
            on_round(&format!(
                "Ratifikation: offene Wünsche sinken auf {amendment_count} — Budget auf {revision_limit} Revisionen erweitert"
            ));
        }
        previous_amendment_count = Some(amendment_count);
        if revision >= revision_limit {
            on_round(&format!(
                "Ratifikation: nach {revision_limit} Revisionen keine Einstimmigkeit — Runde wird beendet"
            ));
            break;
        }
        revision += 1;
        on_round(&format!(
            "Ratifikation: {amendment_count} Änderungswunsch/-wünsche — Revision {revision} von maximal {revision_limit}",
        ));
        let revision_prompt = build_revision_prompt(
            &config.topic,
            &proposal,
            &round_amendments,
            revision,
            revision_limit,
        );
        match query(&proposals[winner_index].0, &revision_prompt)
            .ok()
            .filter(|text| !text.trim().is_empty())
            .map(|text| text.trim().to_string())
        {
            Some(revised) => proposal = revised,
            None => {
                on_round("Ratifikation: Planautor lieferte keine Revision — Runde wird beendet");
                break;
            }
        }
    }
    if approved.is_none() {
        if let Some(leader) = scoreboard_leader(&config.brains) {
            on_round(&format!(
                "Ratifikation: keine Einstimmigkeit — Scoreboard-Leader {leader} entscheidet verbindlich"
            ));
            let prompt =
                build_leader_decision_prompt(&config.topic, &proposal, &remaining_amendments);
            match query(&leader, &prompt)
                .ok()
                .filter(|decision| !decision.trim().is_empty())
                .map(|decision| decision.trim().to_string())
            {
                Some(decision) => {
                    on_round(&format!(
                        "Ratifikation: verbindlicher Endplan von Scoreboard-Leader {leader}"
                    ));
                    approved = Some(decision);
                }
                None => on_round(&format!(
                    "Ratifikation: Scoreboard-Leader {leader} nicht verfügbar — keine automatische Umsetzung"
                )),
            }
        } else {
            on_round(
                "Ratifikation: kein teilnehmender Scoreboard-Leader — keine automatische Umsetzung",
            );
        }
    }
    DesignVoteReport {
        proposals,
        eliminated,
        winner: Some(winner_index),
        approved,
        amendments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
    fn ranking_parser_keeps_unique_valid_numbers_in_order() {
        assert_eq!(parse_ranking("3, 1, 3, 9, 2", 3), vec![2, 0, 1]);
    }

    #[test]
    fn full_vote_eliminates_down_to_one_winner() {
        // 3 Brains, jedes liefert ein Design; dann kicken sie bis einer bleibt.
        let brains = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let calls = AtomicUsize::new(0usize);
        let query = |_b: &str, prompt: &str| -> Result<String, String> {
            if prompt.contains("Entwirf EIN") {
                // Sammelphase: jedes Brain ein unterscheidbares Design.
                let n = calls.fetch_add(1, Ordering::SeqCst);
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
        assert_eq!(report.winning_design().unwrap().0, "c");
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
