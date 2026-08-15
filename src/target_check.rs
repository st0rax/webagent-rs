//! target_check — prueft, ob eine Bauaufgabe auf existierenden Code zeigt.
//!
//! Der Schwarm stimmt ueber Verbesserungsvorschlaege ab und der Sieger wird
//! gebaut. Was dazwischen fehlt: die Behauptung, die in dem Vorschlag steckt,
//! wird nie gegen den Quelltext geprueft.
//!
//! Beobachtet am 12.08.2026: Sieger war *„src/benchmark/mod.rs: Fehlende
//! Fehlerbehandlung bei `bench_collapse_all` fuer leere Panel-Liste"*.
//! `bench_collapse_all` steht aber in `src/tui_state.rs`, und der ebenfalls
//! genannte Typ `BenchmarkUi` existiert im ganzen Projekt nicht. deepseek
//! durchsuchte daraufhin acht Minuten lang eine 1046-Zeilen-Datei nach einem
//! Symbol, das dort nie war, und endete in `max_cycles` ohne eine Zeile
//! Aenderung. Drei weitere Laeufe an dem Tag sahen genauso aus.
//!
//! **Nicht ablehnen, sondern hinweisen.** Ein Vorschlag darf voellig
//! berechtigt eine noch nicht existierende Funktion fordern — das ist der
//! Normalfall bei „implementiere X". Unterscheidbar ist nur der Fall, in dem
//! ein Symbol **anderswo existiert** als behauptet: dann ist die Dateiangabe
//! nachweislich falsch, und der Hinweis spart dem Brain die Suche.

use std::collections::BTreeMap;
use std::path::Path;

/// Was die Pruefung ueber ein genanntes Symbol sagen kann.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Befund {
    /// Symbol steht in der genannten Datei — alles in Ordnung.
    Passt,
    /// Symbol existiert, aber in anderen Dateien. Die Aufgabe zeigt falsch.
    AndereDatei {
        symbol: String,
        gefunden_in: Vec<String>,
    },
    /// Ohne genannte Zieldatei: bekannte Fundstellen als Orientierung.
    Gefunden {
        symbol: String,
        gefunden_in: Vec<String>,
    },
    /// Symbol kommt nirgends vor. Kein Fehler: vermutlich neu anzulegen.
    Unbekannt { symbol: String },
}

/// Ergebnis der Pruefung einer Bauaufgabe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pruefung {
    /// Zieldatei fehlt komplett.
    pub zieldatei_fehlt: Option<String>,
    pub befunde: Vec<Befund>,
}

impl Pruefung {
    /// Nur die Faelle, bei denen die Aufgabe nachweislich falsch zeigt.
    ///
    /// `Unbekannt` gehoert bewusst NICHT dazu: „implementiere eine neue
    /// Funktion" ist keine Fehlangabe.
    pub fn irrefuehrend(&self) -> bool {
        self.zieldatei_fehlt.is_some()
            || self
                .befunde
                .iter()
                .any(|b| matches!(b, Befund::AndereDatei { .. }))
    }

    /// Hinweistext fuer das bauende Brain — leer, wenn nichts zu melden ist.
    ///
    /// Bewusst als Hinweis und nicht als Ablehnung: der Vorschlag kann in der
    /// Sache richtig sein und nur die Datei verwechseln.
    pub fn hinweis(&self) -> String {
        let mut zeilen = Vec::new();
        if let Some(datei) = &self.zieldatei_fehlt {
            zeilen.push(format!(
                "Die genannte Zieldatei `{datei}` existiert nicht. Pruefe den Pfad, \
                 bevor du suchst."
            ));
        }
        for b in &self.befunde {
            match b {
                Befund::AndereDatei {
                    symbol,
                    gefunden_in,
                } => zeilen.push(format!(
                    "`{symbol}` steht nicht in der genannten Datei, sondern in: {}. \
                     Suche dort, nicht in der Zieldatei.",
                    gefunden_in.join(", ")
                )),
                Befund::Gefunden {
                    symbol,
                    gefunden_in,
                } => zeilen.push(format!(
                    "Fuer `{symbol}` wurde keine Zieldatei genannt. Das Symbol steht in: {}. \
                     Beginne die Aenderung dort.",
                    gefunden_in.join(", ")
                )),
                Befund::Passt | Befund::Unbekannt { .. } => {}
            }
        }
        if zeilen.is_empty() {
            return String::new();
        }
        format!(
            "HINWEIS ZUR AUFGABE (automatisch gegen den Quelltext geprueft):\n- {}",
            zeilen.join("\n- ")
        )
    }
}

/// Sieht ein Wort wie ein Rust-Bezeichner aus?
///
/// `streng` gilt fuer Fliesstext: dort muss ein Kandidat einen Unterstrich
/// tragen **oder** mindestens zwei Grossbuchstaben haben (`BenchmarkUi`,
/// `ControllerError`). Grund: die Kommentare in diesem Projekt sind deutsch,
/// und deutsche Substantive sind grossgeschrieben. Ohne diese Schranke haelt
/// „Behandlung" oder „Fehlerbehandlung" fuer einen Typ, findet das Wort in
/// irgendeinem Kommentar wieder und meldet dem Brain eine falsche Datei —
/// ein falscher Hinweis ist schaedlicher als gar keiner.
fn wie_bezeichner(s: &str, streng: bool) -> bool {
    if s.len() < 4 {
        return false;
    }
    // Echte Bezeichner bestehen nur aus Buchstaben, Ziffern und Unterstrichen.
    // Real beobachtet 2026-08-15: der Refiner schrieb `Backend-Implementierung`,
    // `Parse-Fehler`, `Symbole/Funktionen`, `Unit-Test-Bereich` in Backticks,
    // und die Pruefung meldete daraus Phantom- und Fehlweisungs-Funde.
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    if s.contains('_') {
        return s.chars().any(|c| c.is_ascii_alphabetic());
    }
    let grosse = s.chars().filter(|c| c.is_uppercase()).count();
    let gemischt = grosse >= 1 && s.chars().any(|c| c.is_lowercase());
    if !gemischt {
        return false;
    }
    // Akronym-Plural wie `APIs`, `IDs`, `URLs` (Grossbuchstaben vor genau einem
    // Kleinbuchstaben) ist deutsches Fliesstext-Wort, kein Bezeichner.
    let caps = s.chars().take_while(|c| c.is_uppercase()).count();
    let acronym_plural = caps > 0
        && s[caps..].len() == 1
        && s[caps..].chars().next().is_some_and(|c| c.is_lowercase());
    if acronym_plural {
        return false;
    }
    if streng {
        grosse >= 2
    } else {
        s.chars().next().is_some_and(|c| c.is_uppercase())
    }
}

/// Zieht Bezeichner aus einem Vorschlagstext.
///
/// In Backticks gilt die lockere Regel — wer etwas in Backticks setzt, meint
/// Code. Im Fliesstext gilt die strenge (siehe [`wie_bezeichner`]).
pub fn symbole(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str, streng: bool| {
        let s = s.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        // Funktionsklammern und Pfadreste abschneiden.
        let s = s.rsplit("::").next().unwrap_or(s);
        if out.iter().any(|x| x == s) || !wie_bezeichner(s, streng) {
            return;
        }
        out.push(s.to_string());
    };

    // 1. Alles in Backticks — das ist die verlaesslichste Quelle.
    let mut rest = text;
    while let Some(a) = rest.find('`') {
        let nach = &rest[a + 1..];
        let Some(b) = nach.find('`') else { break };
        for teil in nach[..b].split_whitespace() {
            push(teil, false);
        }
        rest = &nach[b + 1..];
    }

    // 2. Freistehende Kandidaten im Fliesstext — strenge Regel.
    for wort in text.split(|c: char| c.is_whitespace() || c == ',' || c == ';') {
        push(wort, true);
    }
    out
}

/// Prueft eine Bauaufgabe gegen den Quelltext unter `root`.
///
/// `dateien` ist die Liste der durchsuchbaren Quelldateien (relativ zu `root`),
/// damit die Funktion ohne Dateisystem testbar bleibt: der Aufrufer reicht
/// Pfad und Inhalt herein.
pub fn pruefe(zieldatei: &str, aufgabe: &str, dateien: &BTreeMap<String, String>) -> Pruefung {
    let mut p = Pruefung::default();

    let hat_ziel = !zieldatei.trim().is_empty();
    let ziel_inhalt = hat_ziel.then(|| dateien.get(zieldatei)).flatten();
    if hat_ziel && ziel_inhalt.is_none() {
        p.zieldatei_fehlt = Some(zieldatei.to_string());
    }

    for sym in symbole(aufgabe) {
        if ziel_inhalt.is_some_and(|c| c.contains(&sym)) {
            p.befunde.push(Befund::Passt);
            continue;
        }
        let anderswo: Vec<String> = dateien
            .iter()
            .filter(|(pfad, inhalt)| {
                (!hat_ziel || pfad.as_str() != zieldatei) && inhalt.contains(&sym)
            })
            .map(|(pfad, _)| pfad.clone())
            .take(3)
            .collect();
        if anderswo.is_empty() {
            p.befunde.push(Befund::Unbekannt { symbol: sym });
        } else if hat_ziel {
            p.befunde.push(Befund::AndereDatei {
                symbol: sym,
                gefunden_in: anderswo,
            });
        } else {
            p.befunde.push(Befund::Gefunden {
                symbol: sym,
                gefunden_in: anderswo,
            });
        }
    }
    p
}

/// Liest alle `.rs`-Dateien unter `root/src` ein — die Ausgangsbasis fuer
/// [`pruefe`] im echten Lauf.
pub fn quelldateien(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let src = root.join("src");
    let mut stapel = vec![src];
    while let Some(dir) = stapel.pop() {
        let Ok(eintraege) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in eintraege.flatten() {
            let pfad = e.path();
            if pfad.is_dir() {
                // Generierte Caches gehoeren nicht in die Suche.
                if pfad.file_name().is_some_and(|n| n == ".graphify") {
                    continue;
                }
                stapel.push(pfad);
            } else if pfad.extension().is_some_and(|x| x == "rs") {
                if let Ok(inhalt) = std::fs::read_to_string(&pfad) {
                    let rel = pfad
                        .strip_prefix(root)
                        .unwrap_or(&pfad)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.insert(rel, inhalt);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn welt() -> BTreeMap<String, String> {
        BTreeMap::from([
            (
                "src/benchmark/mod.rs".to_string(),
                "pub fn run_benchmark() {}\n".to_string(),
            ),
            (
                "src/tui_state.rs".to_string(),
                "pub fn bench_collapse_all(app: &mut App) {}\n".to_string(),
            ),
        ])
    }

    /// Der reale Fall vom 12.08.2026, der acht Minuten Brain-Zeit kostete.
    #[test]
    fn symbol_in_anderer_datei_wird_benannt() {
        let p = pruefe(
            "src/benchmark/mod.rs",
            "Fehlende Fehlerbehandlung bei `bench_collapse_all` fuer leere Panel-Liste",
            &welt(),
        );
        assert!(p.irrefuehrend(), "die Aufgabe zeigt nachweislich falsch");
        assert!(
            p.hinweis().contains("src/tui_state.rs"),
            "der Hinweis MUSS die richtige Datei nennen: {}",
            p.hinweis()
        );
    }

    /// Eine neu anzulegende Funktion ist KEIN Fehler — sonst lehnt die
    /// Pruefung genau die Aufgaben ab, um die es beim Bauen geht.
    #[test]
    fn neues_symbol_ist_kein_fehler() {
        let p = pruefe(
            "src/benchmark/mod.rs",
            "`validate_observation_action_id` implementieren und testen",
            &welt(),
        );
        assert!(!p.irrefuehrend(), "neu anzulegen ist kein Fehlverweis");
        assert!(p.hinweis().is_empty(), "kein Hinweis noetig");
        assert!(p
            .befunde
            .iter()
            .any(|b| matches!(b, Befund::Unbekannt { .. })));
    }

    #[test]
    fn passendes_symbol_meldet_nichts() {
        let p = pruefe(
            "src/tui_state.rs",
            "`bench_collapse_all` um einen Frueh-Ausstieg ergaenzen",
            &welt(),
        );
        assert!(!p.irrefuehrend());
        assert!(p.hinweis().is_empty());
    }

    #[test]
    fn fehlende_zieldatei_wird_gemeldet() {
        let p = pruefe("src/gibt_es_nicht.rs", "irgendwas mit `foo_bar`", &welt());
        assert!(p.irrefuehrend());
        assert!(p.hinweis().contains("existiert nicht"));
    }

    #[test]
    fn fehlende_zieldatei_liefert_symbol_fundstelle_statt_leerpfad_warnung() {
        let p = pruefe(
            "",
            "Fehlerbehandlung bei `bench_collapse_all` ergaenzen",
            &welt(),
        );
        assert_eq!(p.zieldatei_fehlt, None);
        assert!(p.hinweis().contains("src/tui_state.rs"), "{}", p.hinweis());
        assert!(p.hinweis().contains("keine Zieldatei genannt"));
        assert!(!p.hinweis().contains("`` existiert nicht"));
    }

    #[test]
    fn symbole_erkennt_backticks_typen_und_snake_case() {
        let s = symbole("Bei `bench_collapse_all` fehlt in BenchmarkUi die Behandlung, siehe crate::tui_state::fold_bench_events.");
        assert!(s.contains(&"bench_collapse_all".to_string()));
        assert!(
            s.contains(&"BenchmarkUi".to_string()),
            "Typen zaehlen: {s:?}"
        );
        assert!(
            s.contains(&"fold_bench_events".to_string()),
            "Pfad wird auf den letzten Teil gekuerzt: {s:?}"
        );
        // Fliesstext ohne Bezeichnerform bleibt draussen.
        assert!(!s.iter().any(|x| x == "fehlt" || x == "Behandlung"));
    }

    /// Der Code ist deutsch kommentiert. Grossgeschriebene Substantive duerfen
    /// im Fliesstext NICHT als Typ durchgehen — sonst findet die Pruefung sie
    /// in irgendeinem Kommentar und meldet dem Brain eine falsche Datei.
    #[test]
    fn deutsche_substantive_sind_keine_typen() {
        let s = symbole(
            "Die Fehlerbehandlung in der Zieldatei ist unvollstaendig, \
             die Behandlung leerer Listen fehlt in Panel und Ansicht.",
        );
        assert!(s.is_empty(), "kein deutsches Substantiv darf durch: {s:?}");

        // In Backticks meint der Autor Code — dort gilt die lockere Regel.
        let t = symbole("siehe `Panel` und `Behandlung`");
        assert!(t.contains(&"Panel".to_string()), "{t:?}");
    }

    #[test]
    fn kurze_woerter_und_doppelte_fallen_raus() {
        let s = symbole("`ab` `a_b` `lange_sache` `lange_sache`");
        assert_eq!(s, vec!["lange_sache".to_string()], "{s:?}");
    }

    /// Real beobachtet 2026-08-15: der Refiner belegte mit `Backend-Implementierung`,
    /// `Parse-Fehler`, `Symbole/Funktionen` usw. — deutsche Komposita mit
    /// Bindestrich/Schrägstrich, die die Pruefung als Bezeichner behandelte und
    /// daraus Phantom-/Fehlweisungs-Funde meldete.
    #[test]
    fn komposita_mit_trennzeichen_sind_keine_bezeichner() {
        let s = symbole(
            "Belege: `Backend-Implementierung`, `Parse-Fehler`, `Symbole/Funktionen`, \
             `Unit-Test-Bereich`, `Nachrichtenversand-Anker`, `Antwortverarbeitungs-Pfad`.",
        );
        assert!(s.is_empty(), "kein Kompositum darf durch: {s:?}");
    }

    /// `APIs`, `IDs`, `URLs` sind deutsche Akronym-Plurale, keine Bezeichner —
    /// im 08-15-Lauf liess genau `APIs` einen richtigen Plan kippen
    /// („keine offenen APIs ändern" fand das Wort in einem Kommentar).
    #[test]
    fn akronym_plural_ist_kein_bezeichner() {
        let s = symbole("keine bestehenden APIs oder IDs im Aufrufer anfassen");
        assert!(s.is_empty(), "kein Akronym-Plural darf durch: {s:?}");
        let t = symbole("`APIClient` ruft `ControllerError` und `bench_collapse_all`");
        assert!(t.contains(&"APIClient".to_string()), "{t:?}");
        assert!(t.contains(&"ControllerError".to_string()), "{t:?}");
        assert!(t.contains(&"bench_collapse_all".to_string()), "{t:?}");
    }
}
