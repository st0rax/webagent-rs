//! scoring — gemeinsame Scoring-Funktionen für code_score und brain_score
//!
//! Extrahiert die doppelte Wilson-Score-Implementierung, die in beiden Modulen
//! identisch war (siehe docs/ARCHITECTURE.md → "Bekannte Redundanzen").

/// 95%-Konfidenz-Z-Wert für den Wilson-Score (identisch in beiden Original-Modulen).
pub const Z: f64 = 1.96;

/// Wilson-Score-Lower-Bound für `successes` von `n` Versuchen.
///
/// `n == 0` liefert 0.5 (völlige Unsicherheit) — ein Brain ohne Daten ist nicht
/// "schlecht", sondern unbekannt (gleiche Konvention wie brain_score/code_score).
///
/// Die Formel ist die standardisierte Wilson-Score-Gleichung für binäre
/// Erfolg/Fehlschlag-Ereignisse mit 95%-Konfidenzintervall.
pub fn wilson_lower_bound(successes: usize, n: usize) -> f64 {
    if n == 0 {
        return 0.5;
    }
    let n = n as f64;
    let p = successes as f64 / n;
    let z2 = Z * Z;
    let denom = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let margin = Z * ((p * (1.0 - p) + z2 / (4.0 * n)) / n).sqrt();
    ((center - margin) / denom).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_no_data_is_uncertain_not_zero() {
        assert_eq!(wilson_lower_bound(0, 0), 0.5);
    }

    #[test]
    fn wilson_prefers_more_evidence_at_same_ratio() {
        // 90% aus 10 Versuchen ist weniger sicher als 90% aus 100 — der Score
        // muss das widerspiegeln (weniger Daten -> vorsichtigerer, niedrigerer
        // Lower Bound), sonst wäre ein früher Zufallstreffer genauso viel wert
        // wie eine belastbare Historie.
        let few = wilson_lower_bound(9, 10);
        let many = wilson_lower_bound(90, 100);
        assert!(many > few, "many={many} sollte > few={few} sein");
    }

    #[test]
    fn wilson_perfect_score_is_near_one() {
        // Perfekter Score ist nicht exakt 1.0, sondern knapp darunter aufgrund
        // des Konfidenzintervalls (Wilson-Score Eigenschaft)
        let perfect = wilson_lower_bound(100, 100);
        assert!(perfect > 0.95, "perfect={perfect} sollte > 0.95 sein");
        assert!(perfect < 1.0, "perfect={perfect} sollte < 1.0 sein");
    }

    #[test]
    fn wilson_zero_score_is_zero() {
        assert_eq!(wilson_lower_bound(0, 100), 0.0);
    }

    #[test]
    fn wilson_half_score_is_below_half() {
        // 50% Erfolgsrate liegt unter 0.5 aufgrund des Konfidenzintervalls
        // (statistische Vorsicht bei mittelmäßigen Ergebnissen)
        let medium = wilson_lower_bound(50, 100);
        assert!(medium < 0.5, "medium={medium} sollte < 0.5 sein");
        assert!(medium > 0.4, "medium={medium} sollte > 0.4 sein");
    }
}
