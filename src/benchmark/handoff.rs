//! Handoff-Warteschlange der Phase B samt Fehlversuchs-Buchhaltung.

use super::bench_say;

/// Arbeitsschlange der Phase B samt Handoff-Buchhaltung.
///
/// Steckt in einer eigenen Struct, weil genau hier der Endlos-Pingpong sass:
/// `refined_cache` dedupliziert Aufgabentexte, deshalb tragen bei wenigen
/// Vorschlaegen MEHRERE Plan-Eintraege dieselbe Aufgabe — und `tried` ist nach
/// eben diesem Text gekeyt. Ohne Kill-Flag lief eine laengst ausgefallene
/// Aufgabe fuer jeden weiteren Eintrag erneut durch einen vollen Brain-Run.
/// Als reine Datenstruktur ist das ohne Netzwerk und ohne Brains testbar.
pub(crate) struct HandoffQueue {
    /// (Brain, Aufgabentext, abgebendes Brain — None = frischer Plan-Eintrag)
    queue: std::collections::VecDeque<(String, String, Option<String>)>,
    tried: std::collections::HashMap<String, Vec<String>>,
    dropped: std::collections::HashSet<String>,
    #[allow(dead_code)]
    brains: Vec<String>,
    #[allow(dead_code)]
    max_handoffs: usize,
}

impl HandoffQueue {
    pub(crate) fn new(plan: &[(String, String)], brains: &[String], max_handoffs: usize) -> Self {
        Self {
            queue: plan
                .iter()
                .map(|(b, t)| (b.clone(), t.clone(), None))
                .collect(),
            tried: std::collections::HashMap::new(),
            dropped: std::collections::HashSet::new(),
            brains: brains.to_vec(),
            max_handoffs,
        }
    }

    /// Naechster Auftrag. Ausgefallene Aufgaben werden hier verworfen, damit
    /// sie keinen weiteren Brain-Run kosten.
    pub(crate) fn next(&mut self) -> Option<(String, String, Option<String>)> {
        while let Some((brain, effective, from)) = self.queue.pop_front() {
            if self.dropped.contains(&effective) {
                bench_say!(
                    crate::bench_events::Level::Warn,
                    Some(brain.as_str()),
                    "{effective} bereits ausgefallen — ueberspringe Eintrag fuer {brain}."
                );
                continue;
            }
            // Handoffs sind bereits beim Einreihen vermerkt; nur frische
            // Plan-Eintraege muessen hier nachgetragen werden.
            if from.is_none() {
                let tried = self.tried.entry(effective.clone()).or_default();
                if tried.contains(&brain) {
                    bench_say!(
                        crate::bench_events::Level::Warn,
                        Some(brain.as_str()),
                        "{brain}: bereits für diese Aufgabe reserviert — Doppelstart übersprungen."
                    );
                    continue;
                }
                tried.push(brain.clone());
            }
            return Some((brain, effective, from));
        }
        None
    }

    /// Reicht eine steckengebliebene Aufgabe weiter. `Some(nb)` = uebernimmt
    /// `nb`, `None` = niemand mehr uebrig, Aufgabe faellt endgueltig aus.
    ///
    /// Die Reservierung passiert BEIM EINREIHEN, nicht erst beim Poppen —
    /// sonst waehlen zwei Stalls derselben Aufgabe dasselbe naechste Brain.
    #[allow(dead_code)]
    pub(crate) fn on_stall(&mut self, brain: &str, effective: &str) -> Option<String> {
        let already = self.tried.entry(effective.to_string()).or_default();
        let cap = self.max_handoffs.max(1) + 1;
        let next = if already.len() < cap {
            self.brains.iter().find(|b| !already.contains(b)).cloned()
        } else {
            None
        };
        match next {
            Some(nb) => {
                already.push(nb.clone());
                self.queue
                    .push_back((nb.clone(), effective.to_string(), Some(brain.to_string())));
                Some(nb)
            }
            None => {
                self.dropped.insert(effective.to_string());
                None
            }
        }
    }

    /// Nur fuer Tests: der Produktionspfad fragt den Zustand nicht ab, weil
    /// `next()` ausgefallene Aufgaben selbst wegwirft.
    #[cfg(test)]
    pub(crate) fn is_dropped(&self, effective: &str) -> bool {
        self.dropped.contains(effective)
    }
}

