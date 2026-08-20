//! Einstellungen: was gilt, woher es kommt, und wie man es aendert.
//!
//! Bis 07.08.2026 lebten alle Stellschrauben ausschliesslich in
//! Umgebungsvariablen. Das hat zwei Folgen, die uns denselben Tag gekostet
//! haben: man sieht nicht, was gerade gilt, und ein vergessenes Flag beim Start
//! aendert das Verhalten stumm — `WEBAGENT_USE_SHARED_BROWSER` entschied
//! unbemerkt darueber, aus WELCHEM Profil der Lauf klont.
//!
//! Deshalb zeigt dieses Modul nicht nur den Wert, sondern auch die **Herkunft**
//! ([`Source`]). Ein Wert ohne Herkunft ist die Haelfte der Antwort: „steht auf
//! an" und „steht auf an, weil DU es gesetzt hast" sind verschiedene Aussagen.
//!
//! Rangfolge, bewusst so herum:
//!
//! 1. Umgebungsvariable — wer sie beim Start setzt, meint es ausdruecklich.
//! 2. Gespeicherte Einstellung (`data_dir()/settings.json`).
//! 3. Eingebaute Vorgabe.
//!
//! Die gespeicherte Einstellung wirkt, weil [`apply_persisted`] sie beim Start
//! in die Prozessumgebung legt — dort, wo der uebrige Code ohnehin liest. Eine
//! Datei, die nur die TUI kennt, waere eine Attrappe: man stellt etwas ein und
//! nichts passiert.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Woher der gerade geltende Wert stammt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Beim Start gesetzt — schlaegt alles andere.
    Umgebung,
    /// In `settings.json` gespeichert.
    Gespeichert,
    /// Eingebaute Vorgabe.
    Vorgabe,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Umgebung => "Umgebung",
            Source::Gespeichert => "gespeichert",
            Source::Vorgabe => "Vorgabe",
        }
    }
}

/// Art der Einstellung — bestimmt, was ein Klick tut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An/aus. Ein Klick schaltet um.
    Schalter,
    /// Sekunden. Ein Klick geht die Stufen durch.
    Sekunden(&'static [u64]),
    /// Anzahl. Ein Klick geht die Stufen durch.
    Anzahl(&'static [u64]),
}

/// Eine Stellschraube.
#[derive(Debug, Clone, Copy)]
pub struct Setting {
    /// Name der Umgebungsvariable — zugleich der Schluessel in settings.json.
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub default: &'static str,
    /// Wofuer das gut ist. Steht in der Oberflaeche, nicht nur im Code:
    /// eine Einstellung, deren Wirkung man raten muss, wird nicht benutzt.
    pub help: &'static str,
}

/// Die einstellbaren Werte.
///
/// Bewusst eine kurze, kuratierte Liste statt aller `WEBAGENT_*`: hier stehen
/// die, deren Wirkung ein Mensch im Betrieb tatsaechlich abwaegen will. Der
/// Rest bleibt Umgebungssache.
pub const SETTINGS: &[Setting] = &[
    Setting {
        key: "WEBAGENT_USE_SHARED_BROWSER",
        label: "Geteilter Browser",
        kind: Kind::Schalter,
        default: "0",
        help: "An: ein Browser fuer alle Brains, geklont aus profiles/shared — \
               Voraussetzung fuer die Kachelansicht. Aus: jedes Brain nutzt sein \
               eigenes Profil aus profiles/<brain>.",
    },
    Setting {
        key: "WEBAGENT_MAX_RUN_SECONDS",
        label: "Zeitdeckel je Lauf",
        kind: Kind::Sekunden(&[300, 600, 900, 1800, 3600]),
        default: "600",
        help: "Nach dieser Zeit bricht ein Lauf mit wall_timeout ab. Gemessen \
               sind 889 von 900 Sekunden reine Antwortzeit der Brains — zu knapp \
               gesetzt schneidet man ihnen das Denken ab.",
    },
    Setting {
        key: "WEBAGENT_BLOCK_COOLDOWN_S",
        label: "Sperrdauer nach hartem Block",
        kind: Kind::Sekunden(&[900, 3600, 10800, 21600]),
        default: "21600",
        help: "So lange bleibt ein Brain nach Login/Quota/Cloudflare draussen. \
               Am 07.08.2026 nahm eine EINZIGE Fehlerkennung alle acht Brains \
               fuer sechs Stunden aus dem Feld.",
    },
    Setting {
        key: "WEBAGENT_STALE_HEARTBEAT_S",
        label: "Stillstand melden nach",
        kind: Kind::Sekunden(&[300, 600, 900, 1800]),
        default: "600",
        help: "Ab wann ein Lauf ohne Ereignis als still gilt. Zu frueh erzieht \
               zum Wegsehen, zu spaet uebersieht drei Stunden Stillstand.",
    },
    Setting {
        key: "WEBAGENT_MAX_OBSERVATION_CHARS",
        label: "Beobachtung je Schritt",
        kind: Kind::Anzahl(&[2000, 8000, 20000, 60000]),
        default: "8000",
        help: "Wie viel Terminal-Ausgabe ein Brain je Schritt zurueckbekommt. \
               Gemessen vertragen alle acht Brains ueber 2 Mio. Zeichen Eingabe \
               — der Deckel schuetzt hier den Ueberblick, nicht das Brain.",
    },
    Setting {
        key: "WEBAGENT_PERSIST_TABS",
        label: "Tabs offen halten",
        kind: Kind::Schalter,
        default: "",
        help: "Haelt Browser-Tabs zwischen Auftraegen offen, statt sie neu zu \
               oeffnen. Ohne eigene Angabe: an, wenn der geteilte Browser an ist.",
    },
];

/// Datei mit den gespeicherten Einstellungen.
pub fn settings_path() -> PathBuf {
    crate::config::data_dir().join("settings.json")
}

/// Liest die gespeicherten Einstellungen. Fehlt oder bricht die Datei, gilt
/// „nichts gespeichert" — eine kaputte Datei darf den Start nicht verhindern.
pub fn load_stored() -> BTreeMap<String, String> {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("[settings] {path:?} nicht lesbar ({e}) — Vorgaben gelten");
        BTreeMap::new()
    })
}

fn store(values: &BTreeMap<String, String>) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(values).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Legt die gespeicherten Werte in die Prozessumgebung — aber nur, wo nichts
/// gesetzt ist.
///
/// Muss frueh in `main` laufen, VOR dem ersten Lesen. Die Umgebung gewinnt:
/// wer beim Start ein Flag setzt, meint es ausdruecklich, und eine gespeicherte
/// Einstellung darf ihm nicht in den Ruecken fallen.
pub fn apply_persisted() {
    for (key, value) in load_stored() {
        if !key.starts_with("WEBAGENT_") {
            continue; // fremde Schluessel ignorieren
        }
        if std::env::var_os(&key).is_some() {
            continue; // Umgebung schlaegt Datei
        }
        std::env::set_var(&key, &value);
    }
}

/// Was fuer eine Einstellung gerade gilt, und woher.
pub fn effective(setting: &Setting, stored: &BTreeMap<String, String>) -> (String, Source) {
    if let Ok(v) = std::env::var(setting.key) {
        // Eine gespeicherte Einstellung landet per `apply_persisted` ebenfalls
        // in der Umgebung. Sie deshalb als „Umgebung" auszuweisen waere
        // irrefuehrend — der Mensch will wissen, ob ER sie gesetzt hat.
        if stored.get(setting.key) == Some(&v) {
            return (v, Source::Gespeichert);
        }
        return (v, Source::Umgebung);
    }
    if let Some(v) = stored.get(setting.key) {
        return (v.clone(), Source::Gespeichert);
    }
    (setting.default.to_string(), Source::Vorgabe)
}

/// Deutet einen Wert als Schalterstellung — gleiche Regel wie
/// [`crate::config::use_shared_browser`].
pub fn is_on(value: &str) -> bool {
    matches!(value.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Der naechste Wert beim Weiterschalten.
///
/// Reine Funktion, damit die Stufenlogik ohne Datei und ohne Terminal pruefbar
/// ist. Ein unbekannter Ausgangswert landet auf der ersten Stufe, statt stecken
/// zu bleiben.
pub fn next_value(setting: &Setting, current: &str) -> String {
    match setting.kind {
        Kind::Schalter => {
            if is_on(current) {
                "0".to_string()
            } else {
                "1".to_string()
            }
        }
        Kind::Sekunden(stufen) | Kind::Anzahl(stufen) => {
            let now: u64 = current.trim().parse().unwrap_or(0);
            let next = stufen
                .iter()
                .find(|s| **s > now)
                .copied()
                .unwrap_or(stufen[0]);
            next.to_string()
        }
    }
}

/// Schaltet eine Einstellung weiter: speichert sie UND setzt sie in der
/// laufenden Prozessumgebung.
///
/// Beides, weil sonst eines von zwei Aergernissen entsteht — die Aenderung
/// wirkt erst nach einem Neustart, oder sie ist nach dem Neustart wieder weg.
pub fn cycle(setting: &Setting) -> Result<String, String> {
    let mut stored = load_stored();
    let (current, _) = effective(setting, &stored);
    let next = next_value(setting, &current);
    stored.insert(setting.key.to_string(), next.clone());
    store(&stored)?;
    std::env::set_var(setting.key, &next);
    Ok(next)
}

/// Setzt eine Einstellung auf „nicht gesetzt" zurueck.
pub fn reset(setting: &Setting) -> Result<(), String> {
    let mut stored = load_stored();
    stored.remove(setting.key);
    store(&stored)?;
    std::env::remove_var(setting.key);
    Ok(())
}

/// Zeile fuer die Anzeige.
pub struct Row {
    pub label: &'static str,
    pub value: String,
    pub source: Source,
    pub help: &'static str,
    /// Aenderungen wirken erst beim naechsten Lauf/Start, nicht rueckwirkend.
    pub key: &'static str,
}

/// Alle Einstellungen mit ihrem geltenden Wert.
pub fn rows() -> Vec<Row> {
    let stored = load_stored();
    SETTINGS
        .iter()
        .map(|s| {
            let (value, source) = effective(s, &stored);
            Row {
                label: s.label,
                value: if value.is_empty() {
                    "(nicht gesetzt)".to_string()
                } else {
                    value
                },
                source,
                help: s.help,
                key: s.key,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setting(kind: Kind, default: &'static str) -> Setting {
        Setting {
            key: "WEBAGENT_TESTWERT",
            label: "Test",
            kind,
            default,
            help: "",
        }
    }

    #[test]
    fn schalter_kippt_hin_und_zurueck() {
        let s = setting(Kind::Schalter, "0");
        assert_eq!(next_value(&s, "0"), "1");
        assert_eq!(next_value(&s, "1"), "0");
        assert_eq!(next_value(&s, "true"), "0");
        assert_eq!(next_value(&s, ""), "1");
    }

    #[test]
    fn stufen_laufen_um() {
        let s = setting(Kind::Sekunden(&[300, 600, 900]), "600");
        assert_eq!(next_value(&s, "300"), "600");
        assert_eq!(next_value(&s, "600"), "900");
        assert_eq!(
            next_value(&s, "900"),
            "300",
            "nach der letzten Stufe von vorn"
        );
    }

    #[test]
    fn unbekannter_wert_landet_auf_der_ersten_stufe_statt_zu_klemmen() {
        let s = setting(Kind::Sekunden(&[300, 600]), "300");
        assert_eq!(next_value(&s, "quatsch"), "300");
        // Ein Wert oberhalb aller Stufen darf nicht stecken bleiben.
        assert_eq!(next_value(&s, "99999"), "300");
    }

    #[test]
    fn zwischenwert_springt_auf_die_naechsthoehere_stufe() {
        let s = setting(Kind::Sekunden(&[300, 600, 900]), "600");
        assert_eq!(next_value(&s, "450"), "600");
    }

    #[test]
    fn is_on_folgt_derselben_regel_wie_die_konfiguration() {
        assert!(is_on("1") && is_on("true") && is_on("YES"));
        assert!(!is_on("0") && !is_on("") && !is_on("nein"));
    }

    #[test]
    fn umgebung_schlaegt_gespeichert_und_vorgabe() {
        let s = setting(Kind::Schalter, "0");
        let mut stored = BTreeMap::new();
        stored.insert(s.key.to_string(), "0".to_string());
        std::env::set_var(s.key, "1");
        let (value, source) = effective(&s, &stored);
        assert_eq!(value, "1");
        assert_eq!(
            source,
            Source::Umgebung,
            "vom Menschen gesetzt, nicht gespeichert"
        );
        std::env::remove_var(s.key);
    }

    #[test]
    fn gespeichert_bleibt_gespeichert_auch_wenn_es_in_der_umgebung_steht() {
        // apply_persisted legt gespeicherte Werte in die Umgebung. Wuerden sie
        // danach als „Umgebung" gelten, koennte niemand mehr unterscheiden, was
        // er selbst gesetzt hat.
        let s = setting(Kind::Schalter, "0");
        let mut stored = BTreeMap::new();
        stored.insert(s.key.to_string(), "1".to_string());
        std::env::set_var(s.key, "1");
        let (_, source) = effective(&s, &stored);
        assert_eq!(source, Source::Gespeichert);
        std::env::remove_var(s.key);
    }

    #[test]
    fn ohne_alles_gilt_die_vorgabe() {
        let s = setting(Kind::Sekunden(&[300]), "600");
        std::env::remove_var(s.key);
        let (value, source) = effective(&s, &BTreeMap::new());
        assert_eq!(value, "600");
        assert_eq!(source, Source::Vorgabe);
    }

    #[test]
    fn jede_einstellung_hat_eine_erklaerung() {
        // Eine Stellschraube, deren Wirkung man raten muss, wird nicht benutzt.
        for s in SETTINGS {
            assert!(!s.help.trim().is_empty(), "{} ohne Erklaerung", s.key);
            assert!(
                s.key.starts_with("WEBAGENT_"),
                "{} ist keine Umgebungsvariable",
                s.key
            );
        }
    }

    #[test]
    fn vorgaben_passen_zu_ihrer_art() {
        // Eine Stufen-Einstellung, deren Vorgabe keine Zahl ist, koennte nie
        // sinnvoll weitergeschaltet werden.
        for s in SETTINGS {
            if let Kind::Sekunden(stufen) | Kind::Anzahl(stufen) = s.kind {
                assert!(!stufen.is_empty(), "{}: keine Stufen", s.key);
                assert!(
                    s.default.parse::<u64>().is_ok(),
                    "{}: Vorgabe {:?} ist keine Zahl",
                    s.key,
                    s.default
                );
            }
        }
    }
}
