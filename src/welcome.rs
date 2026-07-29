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
    /// Ohne Anmeldung benutzbar? Live mit frischem Profil gemessen.
    pub anonymous_ok: bool,
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
            (true, Some(false)) if self.anonymous_ok => "bereit (anonym)",
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
        format!("  {:<10} {:<16} {:<8}{}", self.brain_id, status, lvl, note)
    }

    /// Einsatzbereit heißt: benutzbar — angemeldet ODER anonym nutzbar.
    ///
    /// Mehrere Oberflächen lassen sich ohne Konto bedienen. Ein fehlender
    /// Login ist bei denen kein Grund, das Brain aus dem Pool zu nehmen; wer
    /// nur auf `logged_in` schaut, verschenkt sie.
    pub fn ready(&self) -> bool {
        self.reachable && (self.logged_in == Some(true) || self.anonymous_ok)
    }
}

/// Prüft, ob ein Brain **ohne Anmeldung** benutzbar ist.
///
/// Gemessen, nicht vermutet: dafür wird ein frisches, leeres Profil benutzt.
/// Mit dem normalen Profil wäre die Frage nicht beantwortbar — dort liegt die
/// Sitzung, und jede Antwort hieße „ja, angemeldet geht es".
///
/// Kriterium bewusst streng: Eingabefeld sichtbar UND kein Anmelden-Knopf.
/// Ein Composer allein genügt nicht — geminis ausgeloggte Startseite hat einen
/// und ist trotzdem nicht benutzbar.
pub fn probe_anonymous(brain_id: &str) -> bool {
    let tmp =
        std::env::temp_dir().join(format!("webagent_anon_{}_{}", brain_id, std::process::id()));
    if std::fs::create_dir_all(&tmp).is_err() {
        return false;
    }
    let result = crate::browser::WebBrainBackend::from_config(brain_id)
        .map(|b| b.with_profile_override(tmp.clone()))
        .and_then(|mut b| b.live_diagnose(true))
        .map(|d| d.composer_found && !d.login_button_visible && !d.cloudflare)
        .unwrap_or(false);
    // Wegwerfprofil sofort entfernen — sonst sammelt sich derselbe Muell an,
    // den `sweep_stale_runtime_profiles` schon einmal aufraeumen musste.
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

/// Prüft ein Brain live (Browser auf, Zustand lesen, zu).
/// Wie `probe`, schreibt bei gesetztem `shots_dir` zusaetzlich das Bild.
///
/// Ein Browserstart statt zwei: Startuebersicht und Bilderwand lesen denselben
/// Zustand derselben Seite.
fn probe_with_shot(
    brain_id: &str,
    headless: bool,
    shots_dir: Option<&std::path::Path>,
) -> BrainStatus {
    let lvl = level_of(brain_id);
    let mut st = BrainStatus {
        brain_id: brain_id.to_string(),
        reachable: false,
        logged_in: None,
        login_visible: false,
        anonymous_ok: false,
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
    match backend.live_diagnose_with_shot(headless, shots_dir.is_some()) {
        Ok((d, shot)) => {
            if let (Some(dir), Some(png)) = (shots_dir, shot) {
                let _ = std::fs::create_dir_all(dir);
                let _ = std::fs::write(dir.join(format!("{brain_id}.png")), &png);
            }
            st.reachable = true;
            st.logged_in = Some(d.logged_in);
            st.login_visible = d.login_button_visible;
            // Nur messen, wenn noetig: der Anonym-Check kostet einen zweiten
            // Browserstart. Wer angemeldet ist, braucht ihn nicht.
            if !d.logged_in {
                st.anonymous_ok = probe_anonymous(brain_id);
            }
            if d.cloudflare {
                st.note = "Cloudflare-Prüfung".into();
            } else if !d.logged_in && st.anonymous_ok {
                st.note = "ohne Anmeldung nutzbar".into();
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
    probe_all_with_shots(brains, headless, parallel, None)
}

/// Wie `probe_all`, schreibt bei gesetztem `shots_dir` zusaetzlich die Bilder
/// fuer die Kachelseite — im selben Durchgang.
pub fn probe_all_with_shots(
    brains: &[String],
    headless: bool,
    parallel: usize,
    shots_dir: Option<&std::path::Path>,
) -> Vec<BrainStatus> {
    let parallel = parallel.clamp(1, 4);
    let mut out: Vec<BrainStatus> = Vec::new();
    for chunk in brains.chunks(parallel) {
        let (tx, rx) = mpsc::channel();
        let mut handles = Vec::new();
        for b in chunk {
            let tx = tx.clone();
            let b = b.clone();
            let dir = shots_dir.map(|p| p.to_path_buf());
            handles.push(std::thread::spawn(move || {
                let _ = tx.send(probe_with_shot(&b, headless, dir.as_deref()));
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

/// Öffnet ein Anmeldefenster — aber **nur**, wenn gar kein Brain mehr
/// benutzbar ist.
///
/// Reihenfolge mit Absicht: solange irgendein Brain angemeldet oder anonym
/// nutzbar ist, arbeitet der Pool damit weiter und niemand wird unterbrochen.
/// Ein Fenster, das bei jedem fehlenden Login aufpoppt, wäre in einem
/// Achter-Pool ständig im Weg — Failover ist der Normalfall, Anmelden die
/// Ausnahme.
///
/// Das Fenster bleibt offen, bis die Anmeldung erkannt wird **oder** es
/// geschlossen wird. Kein stiller Abbruch nach wenigen Sekunden: wer sich
/// anmeldet, braucht manchmal Zwei-Faktor, ein Passwortfeld aus dem Manager
/// oder einen zweiten Anlauf.
///
/// Gibt das Brain zurück, für das ein Fenster geöffnet wurde.
pub fn login_if_nothing_usable(
    statuses: &[BrainStatus],
    max_wait: std::time::Duration,
) -> Option<String> {
    if statuses.iter().any(|s| s.ready()) {
        return None;
    }
    // Erreichbar, aber nicht angemeldet ist der aussichtsreichste Kandidat:
    // dort fehlt nur die Sitzung. Ein nicht erreichbares Brain hat ein anderes
    // Problem, das ein Anmeldefenster nicht loest.
    let ziel = statuses
        .iter()
        .find(|s| s.reachable && s.logged_in == Some(false))
        .or_else(|| statuses.first())?;

    println!(
        "\n  Kein einziges Brain ist benutzbar — öffne Anmeldung für '{}'.",
        ziel.brain_id
    );
    println!("  Fenster bleibt offen, bis die Anmeldung erkannt wird oder du es schließt.");

    let mut backend = crate::browser::WebBrainBackend::from_config(&ziel.brain_id).ok()?;
    match backend.interactive_login(max_wait) {
        Ok(true) => println!("  '{}' angemeldet.", ziel.brain_id),
        Ok(false) => println!(
            "  '{}': keine Anmeldung erkannt — Fenster geschlossen oder Zeit abgelaufen.",
            ziel.brain_id
        ),
        Err(e) => println!("  '{}': {e}", ziel.brain_id),
    }
    Some(ziel.brain_id.clone())
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

/// Schreibt die Kachelseite fuer die Bilderwand.
///
/// Liegt bewusst in der Bibliothek, nicht im CLI-Modul: sowohl `webagent wall`
/// als auch die Startuebersicht erzeugen sie. Zwei Fassungen desselben HTML
/// waeren genau die Doppelung, die in diesem Projekt schon mehrfach dazu
/// gefuehrt hat, dass ein Fix nur an einer Stelle ankam.
pub fn write_wall_html(
    dir: &std::path::Path,
    brains: &[String],
    interval: u64,
    round: u64,
) -> std::io::Result<std::path::PathBuf> {
    let mut tiles = String::new();
    for b in brains {
        // Cache-Buster: ohne ihn zeigt der Browser nach dem Neuladen weiter
        // das alte Bild, und die Wand waere still eingefroren.
        tiles.push_str(&format!(
            "<figure><img src=\"{b}.png?r={round}\" alt=\"{b}\" loading=\"lazy\"><figcaption>{b}</figcaption></figure>\n"
        ));
    }
    let refresh = if interval > 0 {
        format!("<meta http-equiv=\"refresh\" content=\"{interval}\">")
    } else {
        String::new()
    };
    let html = format!(
        "<!doctype html><html lang=\"de\"><head><meta charset=\"utf-8\">{refresh}\
<title>webagent · Bilderwand</title><style>\
body{{margin:0;background:#111;color:#ddd;font:13px system-ui,sans-serif}}\
h1{{font-size:14px;font-weight:600;margin:10px 12px;color:#888}}\
.grid{{display:grid;grid-template-columns:repeat(4,1fr);gap:8px;padding:0 12px 12px}}\
figure{{margin:0;background:#000;border:1px solid #333;border-radius:6px;overflow:hidden}}\
img{{width:100%;display:block;aspect-ratio:1280/900;object-fit:cover;object-position:top left}}\
figcaption{{padding:4px 8px;color:#9ab;font-weight:600}}\
@media(max-width:1100px){{.grid{{grid-template-columns:repeat(2,1fr)}}}}\
</style></head><body><h1>webagent · {n} Brains · Runde {round}</h1>\
<div class=\"grid\">{tiles}</div></body></html>",
        n = brains.len()
    );
    let path = dir.join("wall.html");
    std::fs::write(&path, html)?;
    Ok(path)
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
            anonymous_ok: false,
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
    fn login_window_only_opens_when_nothing_is_usable() {
        use std::time::Duration;
        // Solange EIN Brain benutzbar ist, wird niemand unterbrochen —
        // Failover ist der Normalfall, Anmelden die Ausnahme.
        let mit_einem = vec![
            st("a", true, Some(true), false),
            st("b", true, Some(false), true),
        ];
        assert_eq!(
            login_if_nothing_usable(&mit_einem, Duration::from_secs(1)),
            None
        );

        // Auch ein anonym nutzbares Brain reicht.
        let mut anon = st("c", true, Some(false), false);
        anon.anonymous_ok = true;
        assert_eq!(
            login_if_nothing_usable(&[anon], Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn anonymously_usable_counts_as_ready_without_login() {
        // Mehrere Oberflaechen lassen sich ohne Konto bedienen. Wer nur auf
        // `logged_in` schaut, nimmt sie grundlos aus dem Pool.
        let mut s = st("deepseek", true, Some(false), false);
        s.anonymous_ok = true;
        assert!(s.ready(), "ohne Anmeldung nutzbar ist einsatzbereit");
        assert!(s.line().contains("bereit (anonym)"), "{}", s.line());

        // Aber nicht, wenn die Oberflaeche gar nicht erreichbar war.
        let mut tot = st("x", false, None, false);
        tot.anonymous_ok = true;
        assert!(!tot.ready());
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
