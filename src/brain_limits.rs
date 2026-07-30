//! Gemessene Eingabelängen der Brain-Oberflächen — einmal ermitteln, dauerhaft
//! nutzen.
//!
//! Warum gemessen und nicht geschätzt: die Grenze ist nicht das Kontextfenster
//! des Modells, sondern die Eingabelänge der jeweiligen Weboberfläche. Die ist
//! je Anbieter verschieden, nirgends dokumentiert und ändert sich, wenn ein
//! Anbieter sein Frontend anfasst. Ein geratener Wert ist entweder zu klein
//! (dann liest sich ein Brain scheibchenweise durch Dateien) oder zu groß (dann
//! lehnt die Oberfläche ab und der Turn ist verloren).
//!
//! Die Messung läuft einmal je Brain und landet in
//! `<data>/brain_limits.json`. Kommt später ein Brain dazu, fehlt sein Eintrag
//! und es wird beim nächsten Lauf nachgemessen — ohne die bereits bekannten
//! erneut zu befragen.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Ein gemessener Eintrag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrainLimit {
    /// Größte Zeichenzahl, die nachweislich angenommen wurde.
    pub accepted_chars: usize,
    /// Kleinste Zeichenzahl, die nachweislich abgelehnt wurde (falls bekannt).
    #[serde(default)]
    pub rejected_chars: Option<usize>,
    /// Wann gemessen (`now_rfc3339`).
    pub measured_at: String,
    /// Womit die Ablehnung erkannt wurde — für die Nachvollziehbarkeit.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LimitStore {
    #[serde(default)]
    pub brains: BTreeMap<String, BrainLimit>,
}

pub fn store_path() -> PathBuf {
    crate::config::data_dir().join("brain_limits.json")
}

pub fn load() -> LimitStore {
    load_at(&store_path())
}

pub fn load_at(path: &PathBuf) -> LimitStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_at(store: &LimitStore, path: &PathBuf) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(store)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(path, body)
}

/// Gemessene Annahmegrenze eines Brains, falls vorhanden.
pub fn accepted_chars(brain_id: &str) -> Option<usize> {
    load().brains.get(brain_id).map(|l| l.accepted_chars)
}

/// Brains ohne Messwert — genau die, die noch befragt werden müssen.
pub fn unmeasured(brains: &[String]) -> Vec<String> {
    let store = load();
    brains
        .iter()
        .filter(|b| !store.brains.contains_key(*b))
        .cloned()
        .collect()
}

/// Ergebnis eintragen und sichern.
pub fn record(brain_id: &str, limit: BrainLimit) -> std::io::Result<()> {
    let path = store_path();
    let mut store = load_at(&path);
    store.brains.insert(brain_id.to_string(), limit);
    save_at(&store, &path)
}

/// Leiter der Probengrößen, größte zuerst.
///
/// Absteigend, damit der erste Erfolg die Messung beendet: ein Brain mit
/// grosszuegiger Oberflaeche kostet dann genau EINE Nachricht. Die Stufen sind
/// grob genug, dass keine Feinsuche noetig ist — fuer die Entscheidung
/// „wie viel Kontext passt in eine Observation" reicht die Groessenordnung.
pub const PROBE_LADDER: &[usize] = &[100_000, 50_000, 25_000, 12_000, 6_000];

/// Erkennt an der Antwort (oder am Fehler), ob die Oberflaeche die Eingabe
/// abgelehnt hat.
///
/// Storax' Hinweis vom 30.07.2026: die Oberflaechen schneiden nicht still ab,
/// sie melden eine ueberschrittene Zeichenlaenge. Genau diese Meldungen werden
/// hier erkannt — plus die Faelle, in denen das Senden gar nicht erst gelingt.
pub fn looks_like_length_rejection(text: &str) -> bool {
    let low = text.to_lowercase();
    [
        "zu lang",
        "too long",
        "maximum length",
        "max length",
        "character limit",
        "zeichenlimit",
        "zeichenbegrenzung",
        "exceeds",
        "überschritten",
        "ueberschritten",
        "message is too long",
        "prompt is too long",
        "reduce the length",
        "kürzen",
        "kuerzen",
    ]
    .iter()
    .any(|m| low.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> PathBuf {
        let n = std::process::id();
        std::env::temp_dir().join(format!("webagent_limits_test_{n}.json"))
    }

    #[test]
    fn leiter_ist_absteigend_und_endet_ueber_null() {
        let mut vorher = usize::MAX;
        for s in PROBE_LADDER {
            assert!(*s < vorher, "Leiter muss absteigen: {s} nach {vorher}");
            vorher = *s;
        }
        assert!(*PROBE_LADDER.last().unwrap() >= 1_000);
    }

    #[test]
    fn ablehnung_wird_an_der_meldung_erkannt() {
        assert!(looks_like_length_rejection(
            "Deine Nachricht ist zu lang. Bitte kürzen."
        ));
        assert!(looks_like_length_rejection(
            "Your message is too long. Please reduce the length."
        ));
        assert!(looks_like_length_rejection("Zeichenlimit überschritten"));
        // Eine normale Antwort darf nicht als Ablehnung gelten.
        assert!(!looks_like_length_rejection(
            "OK, ich habe die Datei gelesen und schlage folgende Aenderung vor."
        ));
    }

    #[test]
    fn speichern_und_lesen_ueberlebt_den_roundtrip() {
        let path = tmp_store();
        let _ = std::fs::remove_file(&path);
        let mut store = LimitStore::default();
        store.brains.insert(
            "deepseek".to_string(),
            BrainLimit {
                accepted_chars: 50_000,
                rejected_chars: Some(100_000),
                measured_at: "2026-07-30T18:00:00+00:00".to_string(),
                note: "Leiter".to_string(),
            },
        );
        save_at(&store, &path).expect("schreibbar");
        let gelesen = load_at(&path);
        assert_eq!(gelesen.brains["deepseek"].accepted_chars, 50_000);
        assert_eq!(gelesen.brains["deepseek"].rejected_chars, Some(100_000));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn kaputte_datei_ist_kein_harter_fehler() {
        let path =
            std::env::temp_dir().join(format!("webagent_limits_bad_{}.json", std::process::id()));
        std::fs::write(&path, "{ das ist kein json").expect("schreibbar");
        assert!(load_at(&path).brains.is_empty(), "leerer Store statt Panik");
        let _ = std::fs::remove_file(&path);
    }
}
