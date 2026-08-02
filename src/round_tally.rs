//! round_tally — wie viele Brains einer Runde wirklich geantwortet haben.
//!
//! Der Benchmark faehrt acht Brains an und zeigt hinterher Ergebnisse. Was er
//! bisher nicht zeigte: wie viele davon ueberhaupt eine verwertbare Antwort
//! lieferten. Am 02.08.2026 meldete der Harness selbst „5 verworfene
//! Brain-Antwort(en), davon 1 mit erkennbarem Format — DAS ist der Harness,
//! nicht das Brain", und daraufhin hielt der Lauf an. In der Oberflaeche war
//! davon nichts zu sehen: acht Kacheln, scheinbar alles in Ordnung.
//!
//! Eine Zaehlung, die niemand sieht, ist keine Messung. Deshalb sind die vier
//! Ausgaenge hier getrennt und werden in der Kopfzeile angezeigt:
//!
//! - **gestartet** — angefragt
//! - **geantwortet** — verwertbare Antwort
//! - **verworfen** — Antwort kam, war aber nicht lesbar (Formatfehler)
//! - **Timeout/Fehler** — gar keine Antwort
//!
//! `verworfen` ist bewusst von `Timeout` getrennt: das eine ist ein Fehler des
//! Harness, das andere einer des Brains oder des Anbieters. Wer beides in einen
//! Topf wirft, sucht die Ursache an der falschen Stelle.

use std::sync::atomic::{AtomicU32, Ordering};

/// Ausgang einer einzelnen Brain-Anfrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Verwertbare Antwort.
    Answered,
    /// Antwort kam an, war aber nicht auswertbar (Format).
    Discarded,
    /// Keine Antwort (Timeout, Fehler, gesperrt).
    Failed,
}

static STARTED: AtomicU32 = AtomicU32::new(0);
static ANSWERED: AtomicU32 = AtomicU32::new(0);
static DISCARDED: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicU32 = AtomicU32::new(0);

/// Zaehlerstand einer Runde.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tally {
    pub started: u32,
    pub answered: u32,
    pub discarded: u32,
    pub failed: u32,
}

impl Tally {
    /// Anteil verwertbarer Antworten in Prozent. `None` ohne Anfragen —
    /// bewusst kein `0 %`, denn „nichts angefragt" ist nicht dasselbe wie
    /// „alles gescheitert".
    pub fn success_percent(&self) -> Option<u32> {
        if self.started == 0 {
            return None;
        }
        Some(self.answered * 100 / self.started)
    }

    /// Kurzform fuer die Kopfzeile.
    pub fn label(&self) -> String {
        match self.success_percent() {
            None => "Brains: —".to_string(),
            Some(pct) => format!(
                "Brains {}/{} ({pct}%)  ✗{} verworfen  ⏱{}",
                self.answered, self.started, self.discarded, self.failed
            ),
        }
    }

    /// Verdient die Runde einen Warnhinweis?
    ///
    /// Schwelle bei „weniger als die Haelfte verwertbar". Der Fall vom
    /// 02.08.2026 (1 von 6) faellt damit klar darunter, ein einzelner
    /// Ausreisser bei acht Brains dagegen nicht.
    pub fn is_alarming(&self) -> bool {
        matches!(self.success_percent(), Some(pct) if pct < 50)
    }
}

/// Setzt die Zaehlung fuer eine neue Runde zurueck.
pub fn reset() {
    STARTED.store(0, Ordering::Relaxed);
    ANSWERED.store(0, Ordering::Relaxed);
    DISCARDED.store(0, Ordering::Relaxed);
    FAILED.store(0, Ordering::Relaxed);
}

/// Meldet eine gestartete Anfrage.
pub fn note_started() {
    STARTED.fetch_add(1, Ordering::Relaxed);
}

/// Meldet den Ausgang einer Anfrage.
pub fn note_outcome(outcome: Outcome) {
    match outcome {
        Outcome::Answered => ANSWERED.fetch_add(1, Ordering::Relaxed),
        Outcome::Discarded => DISCARDED.fetch_add(1, Ordering::Relaxed),
        Outcome::Failed => FAILED.fetch_add(1, Ordering::Relaxed),
    };
}

/// Aktueller Stand.
pub fn snapshot() -> Tally {
    Tally {
        started: STARTED.load(Ordering::Relaxed),
        answered: ANSWERED.load(Ordering::Relaxed),
        discarded: DISCARDED.load(Ordering::Relaxed),
        failed: FAILED.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Zaehler sind prozessglobal — alle Faelle in EINEM Test, damit
    /// parallele Tests sich nicht gegenseitig die Staende umbauen.
    #[test]
    fn zaehlt_die_vier_ausgaenge_getrennt() {
        reset();
        assert_eq!(snapshot(), Tally::default());
        assert_eq!(snapshot().success_percent(), None, "nichts angefragt ist nicht 0%");
        assert!(!snapshot().is_alarming(), "ohne Anfragen kein Alarm");

        // Der echte Fall vom 02.08.2026: 6 angefragt, 1 verwertbar,
        // 5 wegen Format verworfen.
        for _ in 0..6 {
            note_started();
        }
        note_outcome(Outcome::Answered);
        for _ in 0..5 {
            note_outcome(Outcome::Discarded);
        }
        let t = snapshot();
        assert_eq!(t.started, 6);
        assert_eq!(t.answered, 1);
        assert_eq!(t.discarded, 5);
        assert_eq!(t.failed, 0);
        assert_eq!(t.success_percent(), Some(16));
        assert!(t.is_alarming(), "1 von 6 muss auffallen");
        assert!(t.label().contains("1/6"), "{}", t.label());
        assert!(t.label().contains("✗5"), "{}", t.label());

        // Verworfen und Timeout bleiben getrennt: das eine ist ein Fehler des
        // Harness, das andere einer des Brains.
        reset();
        note_started();
        note_outcome(Outcome::Failed);
        let t = snapshot();
        assert_eq!(t.failed, 1);
        assert_eq!(t.discarded, 0);

        // Gesunde Runde schlaegt keinen Alarm.
        reset();
        for _ in 0..8 {
            note_started();
        }
        for _ in 0..7 {
            note_outcome(Outcome::Answered);
        }
        note_outcome(Outcome::Failed);
        assert_eq!(snapshot().success_percent(), Some(87));
        assert!(!snapshot().is_alarming());
        reset();
    }
}
