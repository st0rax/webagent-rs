//! send_error — warum ein Absenden fehlschlug, als TYP statt als Text.
//!
//! Der Brain-Schwarm hat eine Fehlerhierarchie seit Runde 1 auf Platz 2 seiner
//! Top-10 (`verbesserungs-top10-2026-07`); bis zum 05.08.2026 stand `thiserror`
//! nicht einmal in der `Cargo.toml`.
//!
//! # Warum ausgerechnet hier angefangen wird
//!
//! Am 05.08.2026 wurde die Laengenmessung darauf angewiesen, einen Fehlschlag
//! zu unterscheiden: „die Oberflaeche verweigert das Absenden" ist eine
//! Ablehnung und damit ein MESSWERT, „der Beweis kam nicht rechtzeitig" ist ein
//! Harness-Problem und damit ein Abbruch. Weil es nur `Result<_, String>` gab,
//! wurde das mit einem Marker im Fehlertext geloest — `ABSENDEKNOPF_DEAKTIVIERT`
//! — und im Kommentar als „haesslich, richtig waere eine Fehlervariante"
//! vermerkt. Das ist diese Variante.
//!
//! Genau daran haengt mehr als Schoenheit: `benchmark.rs` klassifiziert Fehler
//! mit `low.contains("wall_timeout")`, und `looks_like_length_rejection` prueft
//! eine Wortliste, die am selben Tag um deutsche und chinesische Formulierungen
//! erweitert werden musste. Textvergleiche sind die Krankheit, der Typ ist die
//! Medizin.
//!
//! # Warum `Display` die alten Texte behaelt
//!
//! Die Meldungen landen in Run-Metadaten und Logs. Ein Umbau, der gespeicherte
//! Texte veraendert, macht alte Laeufe unlesbar — das waere ein Rueckschritt.
//! Die Texte sind deshalb woertlich uebernommen; NEU ist nur, dass man sie
//! nicht mehr lesen MUSS, um zu wissen, was passiert ist.

use thiserror::Error;

/// Warum eine Nachricht nicht abgeschickt werden konnte.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SendError {
    /// Die Oberflaeche hat die Eingabe gekuerzt oder gar nicht uebernommen.
    /// Keine Blockade — der Editor kam mit der Menge nicht zurecht.
    #[error(
        "Absenden fehlgeschlagen nach {attempts} Versuchen: der Composer enthaelt nur \
         {actual} von {intended} Zeichen — die Eingabe wurde von der Oberflaeche \
         gekuerzt oder gar nicht uebernommen, es ist KEINE Blockade"
    )]
    ComposerTruncated {
        attempts: u32,
        actual: usize,
        intended: usize,
    },

    /// Der Absendeknopf ist deaktiviert, obwohl der Text vollstaendig dasteht.
    ///
    /// Eine Laengenablehnung muss kein Text sein: manche Oberflaechen sagen gar
    /// nichts und legen nur den Knopf still. Fuer die Laengenmessung ist das
    /// eine ABLEHNUNG, kein Harness-Fehler.
    #[error(
        "Absendeknopf ist deaktiviert, obwohl der Text vollstaendig im Composer steht \
         ({attempts} Versuche) — die Oberflaeche verweigert das Absenden ohne Meldung"
    )]
    SendButtonDisabled { attempts: u32 },

    /// Ein Dialog liegt ueber dem Composer, dessen Text die Phrasenliste nicht kennt.
    #[error(
        "Absenden fehlgeschlagen: kein Absende-Beweis nach {attempts} Versuchen. \
         Unbekannter Dialog ueber dem Composer, Text: \"{text}\" -- falls das eine \
         Blockade ist, gehoert die Formulierung in BLOCK_PHRASES"
    )]
    UnknownDialog { attempts: u32, text: String },

    /// Bekannte Anbieter-Blockade (Kontingent, Auslastung).
    #[error("blockiert: kein Absende-Beweis nach {attempts} Versuchen -- Seite zeigt: {banner}")]
    ProviderBlocked { attempts: u32, banner: String },

    /// Alles sieht richtig aus, trotzdem kam kein Beweis.
    #[error(
        "Absenden fehlgeschlagen: kein Absende-Beweis nach {attempts} Versuchen \
         (Text steht vollstaendig im Composer, kein Dialog gefunden — moeglich \
         sind ein deaktivierter Absendeknopf oder eine stumm verworfene Eingabe)"
    )]
    NoProof { attempts: u32 },
}

impl SendError {
    /// Ist das eine ABLEHNUNG durch die Oberflaeche — also ein Messwert?
    ///
    /// Der Kern des Umbaus: die Laengenmessung fragt das hier, statt in einem
    /// Fehlertext nach einem Marker zu suchen.
    pub fn is_rejection(&self) -> bool {
        matches!(self, SendError::SendButtonDisabled { .. })
    }

    /// Lohnt ein erneuter Versuch? Eine Verweigerung der Oberflaeche nicht —
    /// sie faellt beim naechsten Mal genauso aus und kostet nur Zeit.
    pub fn is_retryable(&self) -> bool {
        !matches!(
            self,
            SendError::SendButtonDisabled { .. } | SendError::ProviderBlocked { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ablehnung_und_harness_fehler_sind_unterscheidbar_ohne_textvergleich() {
        // Genau die Unterscheidung, fuer die es vorher einen Marker im
        // Fehlertext brauchte.
        assert!(SendError::SendButtonDisabled { attempts: 5 }.is_rejection());
        assert!(!SendError::NoProof { attempts: 5 }.is_rejection());
        assert!(!SendError::ComposerTruncated {
            attempts: 5,
            actual: 12,
            intended: 400_000
        }
        .is_rejection());
    }

    #[test]
    fn eine_verweigerung_wird_nicht_wiederholt() {
        // Ein deaktivierter Knopf faellt beim naechsten Versuch genauso aus.
        // Vorher lief der Aufrufer trotzdem in zwei Wiederholungen plus
        // Circuit-Breaker — gemessen am 05.08.2026 bei claude und mistral.
        assert!(!SendError::SendButtonDisabled { attempts: 5 }.is_retryable());
        assert!(!SendError::ProviderBlocked {
            attempts: 5,
            banner: "usage limit".into()
        }
        .is_retryable());
        assert!(SendError::NoProof { attempts: 5 }.is_retryable());
    }

    #[test]
    fn texte_bleiben_woertlich_erhalten() {
        // Die Meldungen stehen in Run-Metadaten und Logs frueherer Laeufe. Wer
        // sie umformuliert, macht alte Laeufe unlesbar.
        let e = SendError::NoProof { attempts: 5 };
        let text = e.to_string();
        assert!(text.starts_with("Absenden fehlgeschlagen: kein Absende-Beweis nach 5 Versuchen"));
        assert!(text.contains("Text steht vollstaendig im Composer"));

        let d = SendError::SendButtonDisabled { attempts: 5 };
        assert!(d.to_string().contains("Absendeknopf ist deaktiviert"));

        let t = SendError::ComposerTruncated {
            attempts: 5,
            actual: 100,
            intended: 400_000,
        };
        assert!(t.to_string().contains("nur 100 von 400000 Zeichen"));
    }
}
