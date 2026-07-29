//! welcome — Init-Bericht beim Start von `webagent`.
//!
//! Zeigt vor der ersten Eingabe, womit man es zu tun hat: wie viele Brains
//! registriert sind, welche davon **live geprüft** erreichbar und angemeldet
//! sind, und was jedes davon kann.
//!
//! Warum live statt aus der Konfiguration: die Konfiguration hat sich als
//! unzuverlässig erwiesen. `brains-health` meldete monatelang „ok" für ein
//! Brain, dessen Login-Indikator auf das Eingabefeld zeigte — sichtbar auch
//! ohne Anmeldung. Ein Bericht, der nur Dateien liest, wiederholt genau diesen
//! Fehler. Deshalb öffnet der Check jede Oberfläche wirklich.
//!
//! Der Preis ist Zeit: acht Browserstarts dauern. Deshalb laufen sie parallel,
//! und der Bericht sagt ausdrücklich, wann er gemessen hat.

use std::sync::mpsc;

use crate::capability::level_of;

/// Zustand eines Brains zum Startzeitpunkt.
#[derive(Debug, Clone)]
pub struct BrainStatus {
    pub brain_id: String,
    /// Konnte die Oberfläche geöffnet und geprüft werden?
    pub reachable: bool,
    /// Angemeldet? `None`, wenn die Prüfung selbst fehlschlug.
    pub logged_in: Option<bool>,
    /// Ist ein Anmelden-Knopf sichtbar? Zusammen mit `logged_in` entlarvt das
    /// falsch positive Erkennungen.
    pub login_visible: bool,
    /// Fahrbare Fähigkeiten / angebotene Optionen (siehe `capability`).
    pub level: usize,
    pub max_level: Option<usize>,
    /// Kurzgrund bei Fehlschlag.
    pub note: String,
}

impl BrainStatus {
    /// Einzeiler für die Übersicht.
    pub fn line(&self) -> String {
        let status = match (self.reachable, self.logged_in) {
            (false, _) => "nicht erreichbar",
            (true, Some(true)) => "bereit",
            (true, Some(false)) => "nicht angemeldet",
            (true, None) => "unklar",
        };
        let lvl = match self.max_level {
            Some(m) => format!("[{}/{}]", self.level, m),
            None => format!("[{}/?]", self.level),
        };
        let note = if self.note.is_empty() {
            String::new()
        } else {
            format!("  ({})", self.note)
        };
        format!(
            "  {:<10} {:<16} {:<8}{}",
            self.brain_id, status, lvl, note
        )
    }

    /// Einsatzbereit heißt: erreichbar UND angemeldet.
    pub fn ready(&self) -> bool {
        self.reachable && self.logged_in == Some(true)
    }
}

/// Prüft ein Brain live (Browser auf, Zustand lesen, zu).
fn probe(brain_id: &str, headless: bool) -> BrainStatus {
    let lvl = level_of(brain_id);
    let mut st = BrainStatus {
        brain_id: brain_id.to_string(),
        reachable: false,
        logged_in: None,
        login_visible: false,
        level: lvl.level(),
        max_level: lvl.max_level(),
        note: String::new(),
    };
    let mut backend = match crate::browser::WebBrainBackend::from_config(brain_id) {
        Ok(b) => b,
        Err(e) => {
            st.note = format!("Konfiguration: {e}");
            return st;
        }
    };
    match backend.live_diagnose(headless) {
        Ok(d) => {
            st.reachable = true;
            st.logged_in = Some(d.logged_in);
            st.login_visible = d.login_button_visible;
            if d.cloudflare {
                st.note = "Cloudflare-Prüfung".into();
            } else if !d.logged_in {
                st.note = "webagent login --brain ".to_string() + brain_id;
            }
        }
        Err(e) => {
            // Kurz halten: die volle Fehlerkette gehört ins Log, nicht in die
            // Startübersicht.
            st.note = e.chars().take(60).collect();
        }
    }
    st
}

/// Prüft alle Brains parallel und liefert die Ergebnisse in Katalogreihenfolge.
///
/// `parallel` deckelt die gleichzeitig offenen Browser — acht auf einmal sind
/// speicherhungrig und provozieren genau die Zeitüberschreitungen, die der
/// Bericht eigentlich melden soll.
pub fn probe_all(brains: &[String], headless: bool, parallel: usize) -> Vec<BrainStatus> {
    let parallel = parallel.clamp(1, 4);
    let mut out: Vec<BrainStatus> = Vec::new();
    for chunk in brains.chunks(parallel) {
        let (tx, rx) = mpsc::channel();
        let mut handles = Vec::new();
        for b in chunk {
            let tx = tx.clone();
            let b = b.clone();
            handles.push(std::thread::spawn(move || {
                let _ = tx.send(probe(&b, headless));
            }));
        }
        drop(tx);
        let mut got: Vec<BrainStatus> = rx.iter().collect();
        for h in handles {
            let _ = h.join();
        }
        // Reihenfolge innerhalb des Blocks wiederherstellen: die Threads
        // antworten in beliebiger Folge, die Anzeige soll aber stabil sein.
        got.sort_by_key(|s| chunk.iter().position(|b| b == &s.brain_id).unwrap_or(0));
        out.extend(got);
    }
    out
}

/// Formatiert den Init-Bericht.
pub fn render(statuses: &[BrainStatus], measured_at: &str) -> String {
    let ready = statuses.iter().filter(|s| s.ready()).count();
    let mut s = String::new();
    s.push('\n');
    s.push_str("  webagent — Startübersicht\n");
    s.push_str(&format!(
        "  {ready}/{} Brains einsatzbereit · live geprüft {measured_at}\n\n",
        statuses.len()
    ));
    for st in statuses {
        s.push_str(&st.line());
        s.push('\n');
    }

    // Falsch positive Anmeldung sichtbar machen: `logged_in` und ein sichtbarer
    // Anmelden-Knopf schliessen sich aus. Genau diese Kombination hat einen
    // monatelangen Fehlbefund bei gemini entlarvt.
    let widersprueche: Vec<&BrainStatus> = statuses
        .iter()
        .filter(|s| s.logged_in == Some(true) && s.login_visible)
        .collect();
    if !widersprueche.is_empty() {
        s.push_str("\n  Widersprüchlich (angemeldet, aber Anmelden-Knopf sichtbar):\n");
        for w in widersprueche {
            s.push_str(&format!("    {}\n", w.brain_id));
        }
    }

    let offen: Vec<&BrainStatus> = statuses.iter().filter(|s| !s.ready()).collect();
    if !offen.is_empty() {
        s.push_str(&format!(
            "\n  {} Brain(s) nicht einsatzbereit — der Pool nutzt sie nicht.\n",
            offen.len()
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: &str, reachable: bool, logged: Option<bool>, login_visible: bool) -> BrainStatus {
        BrainStatus {
            brain_id: id.into(),
            reachable,
            logged_in: logged,
            login_visible,
            level: 3,
            max_level: Some(6),
            note: String::new(),
        }
    }

    #[test]
    fn ready_requires_reachable_and_logged_in() {
        assert!(st("a", true, Some(true), false).ready());
        assert!(!st("b", true, Some(false), true).ready());
        assert!(!st("c", false, Some(true), false).ready());
        // Unklarer Zustand gilt NICHT als bereit: im Zweifel lieber ein Brain
        // zu wenig als eines, das im Betrieb ausfaellt.
        assert!(!st("d", true, None, false).ready());
    }

    #[test]
    fn render_counts_only_ready_brains() {
        let list = vec![
            st("a", true, Some(true), false),
            st("b", true, Some(false), true),
            st("c", false, None, false),
        ];
        let out = render(&list, "gerade eben");
        assert!(out.contains("1/3 Brains einsatzbereit"), "{out}");
        assert!(out.contains("2 Brain(s) nicht einsatzbereit"), "{out}");
    }

    #[test]
    fn render_flags_the_contradiction_that_hid_a_logged_out_brain() {
        // `logged_in: true` BEI sichtbarem Anmelden-Knopf ist die Signatur des
        // Fehlbefunds, der bei gemini monatelang unentdeckt blieb.
        let list = vec![st("gemini", true, Some(true), true)];
        let out = render(&list, "gerade eben");
        assert!(out.contains("Widersprüchlich"), "{out}");
        assert!(out.contains("gemini"), "{out}");
    }

    #[test]
    fn line_shows_unknown_maximum_as_question_mark() {
        let mut s = st("x", true, Some(true), false);
        s.max_level = None;
        assert!(s.line().contains("[3/?]"), "{}", s.line());
    }
}
