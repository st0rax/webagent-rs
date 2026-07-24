//! knockout — Ausscheidungs-Abstimmung („kick vote").
//!
//! Anders als die Borda-Zählung in [`crate::self_research::tally`] (eine Runde,
//! gewichtete Rangliste) eliminiert der Knockout pro Runde genau einen
//! Kandidaten: jedes Brain nennt den Vorschlag, der RAUS soll, der meistgenannte
//! fliegt, der Rest geht in die nächste Runde. Nach `n-1` Runden bleibt einer —
//! der Gewinner (Storax-Wunsch 2026-07-21: TUI-Design per kick-vote).
//!
//! Hier steht nur die reine Auszähllogik; die Brain-Abfragen je Runde liefert
//! der Aufrufer (CLI/Orchestrator), damit das Kernstück ohne Browser testbar ist.

use std::collections::HashMap;

/// Zustand einer laufenden Ausscheidung: die noch lebenden Kandidaten als
/// Indizes in die ursprüngliche Vorschlagsliste, in stabiler Reihenfolge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bracket {
    alive: Vec<usize>,
}

impl Bracket {
    /// Startet mit `n` Kandidaten (Indizes `0..n`).
    pub fn new(n: usize) -> Self {
        Bracket {
            alive: (0..n).collect(),
        }
    }

    /// Die noch lebenden Kandidaten-Indizes.
    pub fn alive(&self) -> &[usize] {
        &self.alive
    }

    /// `true`, wenn nur noch einer übrig ist — die Ausscheidung ist vorbei.
    pub fn is_decided(&self) -> bool {
        self.alive.len() <= 1
    }

    /// Der Gewinner, falls entschieden.
    pub fn winner(&self) -> Option<usize> {
        if self.alive.len() == 1 {
            Some(self.alive[0])
        } else {
            None
        }
    }

    /// Wertet eine Kick-Runde aus und entfernt den meistgenannten lebenden
    /// Kandidaten. `kicks` sind die genannten Kandidaten-Indizes (in die
    /// URSPRÜNGLICHE Liste); Stimmen für bereits ausgeschiedene oder ungültige
    /// Indizes werden ignoriert. Gibt den eliminierten Index zurück, oder `None`,
    /// wenn nichts mehr zu tun ist bzw. keine gültige Stimme fiel.
    ///
    /// Gleichstand entscheidet der KLEINERE Index — rein für Determinismus, nicht
    /// als Wertung; der Aufrufer kann bei Gleichstand auch eine Stichrunde fahren.
    pub fn eliminate(&mut self, kicks: &[usize]) -> Option<usize> {
        if self.is_decided() {
            return None;
        }
        let mut counts: HashMap<usize, u32> = HashMap::new();
        for &k in kicks {
            if self.alive.contains(&k) {
                *counts.entry(k).or_insert(0) += 1;
            }
        }
        // Meiste Kicks; bei Gleichstand kleinster Index.
        let loser = counts
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            .map(|(&idx, _)| idx)?;
        self.alive.retain(|&x| x != loser);
        Some(loser)
    }

    /// Wie viele Runden noch bis zur Entscheidung.
    pub fn rounds_remaining(&self) -> usize {
        self.alive.len().saturating_sub(1)
    }
}

/// Extrahiert die EINE gekickte Nummer aus einer Brain-Antwort (1-basiert, wie
/// der Nutzer die Liste sieht) und gibt sie 0-basiert zurück. Nimmt die erste
/// im gültigen Bereich genannte Zahl — Brains schreiben oft „Ich kicke Nr. 3,
/// weil …".
pub fn parse_kick(line: &str, candidate_count: usize) -> Option<usize> {
    let mut cur = String::new();
    for ch in line.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(n) = cur.parse::<usize>() {
                if n >= 1 && n <= candidate_count {
                    return Some(n - 1);
                }
            }
            cur.clear();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eliminates_the_most_kicked_each_round() {
        let mut b = Bracket::new(4); // Kandidaten 0,1,2,3
                                     // Runde 1: 2 wird am häufigsten gekickt.
        assert_eq!(b.eliminate(&[2, 2, 0, 2, 1]), Some(2));
        assert_eq!(b.alive(), &[0, 1, 3]);
        // Runde 2: 3 raus.
        assert_eq!(b.eliminate(&[3, 3, 0]), Some(3));
        assert_eq!(b.alive(), &[0, 1]);
        // Runde 3: 1 raus -> 0 gewinnt.
        assert_eq!(b.eliminate(&[1, 1, 0]), Some(1));
        assert!(b.is_decided());
        assert_eq!(b.winner(), Some(0));
    }

    #[test]
    fn votes_for_dead_or_invalid_candidates_are_ignored() {
        let mut b = Bracket::new(3);
        b.eliminate(&[2, 2]); // 2 raus
        assert_eq!(b.alive(), &[0, 1]);
        // Stimmen fuer den bereits toten 2 und den nie existenten 9 zaehlen nicht;
        // nur die eine gueltige Stimme fuer 1 entscheidet.
        assert_eq!(b.eliminate(&[2, 9, 1]), Some(1));
        assert_eq!(b.winner(), Some(0));
    }

    #[test]
    fn a_tie_is_broken_deterministically_by_lower_index() {
        let mut b = Bracket::new(3);
        // 0 und 1 je einmal gekickt -> kleinerer Index (0) faellt.
        assert_eq!(b.eliminate(&[0, 1]), Some(0));
        assert_eq!(b.alive(), &[1, 2]);
    }

    #[test]
    fn no_valid_kick_eliminates_nobody() {
        let mut b = Bracket::new(3);
        assert_eq!(b.eliminate(&[7, 8, 9]), None);
        assert_eq!(b.alive(), &[0, 1, 2]);
        assert!(!b.is_decided());
    }

    #[test]
    fn a_single_candidate_is_already_the_winner() {
        let b = Bracket::new(1);
        assert!(b.is_decided());
        assert_eq!(b.winner(), Some(0));
        let mut b2 = Bracket::new(1);
        assert_eq!(b2.eliminate(&[0]), None, "nichts mehr zu eliminieren");
    }

    #[test]
    fn rounds_remaining_counts_down_to_the_winner() {
        let mut b = Bracket::new(5);
        assert_eq!(b.rounds_remaining(), 4);
        b.eliminate(&[0]);
        assert_eq!(b.rounds_remaining(), 3);
    }

    #[test]
    fn parse_kick_takes_the_first_valid_number() {
        // 1-basiert rein, 0-basiert raus.
        assert_eq!(
            parse_kick("Ich kicke Nummer 3, weil das Layout ueberladen ist.", 5),
            Some(2)
        );
        assert_eq!(parse_kick("3", 5), Some(2));
        // Ausserhalb des Bereichs wird uebersprungen, die naechste gueltige zaehlt.
        assert_eq!(parse_kick("Vorschlag 99 ist raus — also 2.", 5), Some(1));
        assert_eq!(parse_kick("keine zahl hier", 5), None);
    }
}
