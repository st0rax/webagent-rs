//! Ereignisstrom des Benchmark-/Loop-Laufs.
//!
//! Bisher ging das Geschehen ausschliesslich als `println!` nach stdout — die
//! ratatui-TUI in [`crate::tui`] konnte davon nichts sehen, weshalb
//! `webagent tui` und die Benchmark-Ausgabe zwei getrennte Welten waren.
//! Dieser Bus ist die gemeinsame Quelle: der Benchmark meldet hierher, die
//! Konsole druckt weiter wie gehabt, und die TUI liest denselben Strom.
//!
//! Bewusst ein prozessglobaler Ringpuffer und kein Kanal: die TUI pollt im
//! Rendertakt und darf Frames verpassen, ohne dass der Benchmark blockiert
//! oder Meldungen verloren gehen.

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Mutex, OnceLock,
};

/// Wie viele Meldungen vorgehalten werden.
///
/// Ein langer Lauf produziert Zehntausende Zeilen; der Puffer deckelt das,
/// damit ein Nachtlauf nicht den Speicher frisst. Aeltestes faellt raus.
pub const CAPACITY: usize = 2000;

/// Wie viele Zeichen ein ausklappbarer Detailblock hoechstens in den Bus
/// uebernimmt. Ein Controller-Step oder eine Brain-Antwort darf mehrere
/// Kilobyte gross sein; damit ein Nachtlauf nicht den Puffer sprengt, wird das
/// Detail gekappt (der Knoten-Text bleibt immer vollstaendig).
const DETAIL_CAP_CHARS: usize = 6000;

/// Laufende ID je Ereignis. Die TUI braucht eine stabile Adresse, um Knoten
/// auf-/zuzuklappen — die Position im Ringpuffer wandert ja mit jedem Eintrag.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_event_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Die Ratatui-Ansicht besitzt den Terminal-Renderer. Direkte stdout/stderr-
/// Zeilen würden ihren Alternate Screen zerreißen, daher werden sie während
/// dieser Ansicht zentral unterdrückt. Die strukturierten Ereignisse bleiben
/// davon unberührt und sind weiter im Dashboard sichtbar.
static CONSOLE_OUTPUT_ENABLED: AtomicBool = AtomicBool::new(true);

pub fn set_console_output(enabled: bool) {
    CONSOLE_OUTPUT_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn console_output_enabled() -> bool {
    CONSOLE_OUTPUT_ENABLED.load(Ordering::Relaxed)
}

/// Echoe die unstrukturierten `print_line`-Zeilen zusaetzlich in den
/// Ereignisbus. Der TUI-Benchmark schaltet das bei `--verbose` an: die
/// Schritt-fuer-Schritt-Zeilen des Controllers (`[shell:id] …`, `[edit:id] …`,
/// Antworttexte) landen dann sichtbar im "Ereignisstrom", statt nur an stdout
/// zu gehen, das die ratatui-Ansicht unterdrueckt.
static ECHO_TO_BUS: AtomicBool = AtomicBool::new(false);

pub fn set_echo_bus(enabled: bool) {
    ECHO_TO_BUS.store(enabled, Ordering::Relaxed);
}

/// Steht der Spiegelmodus? `true`, sobald der Benchmark mit `--verbose` laeuft.
///
/// Er steuert nicht nur, ob `print_line` in den Bus spiegelt, sondern auch, ob
/// die Logquellen (brain_score-events.jsonl, Run-Events, Controller-Ausgaben)
/// ihre Details hierher spiegeln. Im normalen Betrieb bleibt der Bus unberuehrt.
pub fn echo_bus_enabled() -> bool {
    ECHO_TO_BUS.load(Ordering::Relaxed)
}

/// Wohin eine Meldung auf der Konsole geht.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Console {
    Aus,
    Stdout,
    Stderr,
}

/// Baut das Ereignis **einmal** und verteilt es an die angemeldeten Senken.
///
/// Vorher rief jede Ausgabefunktion ihre Senken einzeln auf — und gab ihnen
/// verschiedene Nutzlast: `print_detailed` reichte `detail` an den Bus und
/// druckte auf der Konsole nur die Kopfzeile. Solche Bauformen driften
/// auseinander, weil nichts sie zusammenhaelt; am 12.08.2026 kostete das eine
/// Fehlersuche, die aus dem Log nicht zu fuehren war.
///
/// Hier gibt es nur noch **eine** Nutzlast: das [`BenchEvent`]. Beide Senken
/// rendern daraus, keine kann etwas anderes bekommen als die andere.
/// Die *Weiterleitung* bleibt unveraendert — wer vorher nur auf die Konsole
/// ging, geht weiterhin nur dorthin.
fn dispatch(
    level: Level,
    brain: Option<&str>,
    text: &str,
    detail: Option<&str>,
    to_bus: bool,
    console: Console,
) {
    let ev = BenchEvent {
        id: next_event_id(),
        ts: crate::timestamp(),
        level,
        brain: brain.map(|b| b.to_string()),
        text: text.to_string(),
        detail: detail.map(cap_detail),
    };

    if console != Console::Aus && console_output_enabled() {
        for line in console_lines(&ev) {
            match console {
                Console::Stderr => eprintln!("{line}"),
                _ => println!("{line}"),
            }
        }
    }

    if to_bus {
        note_activity();
        let mut q = lock();
        if q.len() >= CAPACITY {
            q.pop_front();
        }
        q.push_back(ev);
    }
}

/// Wie ein Ereignis auf der Konsole aussieht.
///
/// Bewusst eine reine Funktion ueber dem Ereignis: nur so laesst sich in einem
/// Test festhalten, dass Konsole und Bus **dieselbe Quelle** haben. Genau diese
/// Zusicherung fehlte, als `detail` still verlorenging.
pub fn console_lines(ev: &BenchEvent) -> Vec<String> {
    let mut out = vec![ev.text.clone()];
    if let Some(d) = &ev.detail {
        out.extend(detail_excerpt(d));
    }
    out
}

pub fn print_line(text: &str) {
    dispatch(
        Level::Info,
        None,
        text,
        None,
        echo_bus_enabled(),
        Console::Stdout,
    );
}

/// Wie [`print_line`], aber mit ausklappbarem Detailblock: der Knoten-Text ist
/// die Zusammenfassung (`[shell:step-1] cargo test --lib`), `detail` der volle
/// Inhalt (Terminal-Ausgabe, Edit-Ergebnis, Antworttext). In der TUI-Baumansicht
/// wird das Detail per Enter/Rechts auf-, mit Links wieder zugeklappt.
pub fn print_detailed(text: &str, detail: Option<&str>) {
    dispatch(
        Level::Info,
        None,
        text,
        detail,
        echo_bus_enabled(),
        Console::Stdout,
    );
}

/// Wie viele Zeilen einer Beobachtung ins Log wandern.
const DETAIL_LOG_LINES: usize = 3;
/// Wie breit eine davon hoechstens sein darf.
const DETAIL_LOG_WIDTH: usize = 200;

/// Kurzfassung einer Beobachtung fuer Konsole und Log.
///
/// Bewusst knapp: eine vollstaendige Beobachtung kann eine ganze Datei
/// enthalten und wuerde das Log unlesbar machen. Exit-Code und die ersten
/// Zeilen von stdout/stderr reichen, um „Anker getroffen?" zu beantworten —
/// genau die Frage, die vorher offen blieb.
pub fn detail_excerpt(detail: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut genommen = 0usize;
    let mut uebrig = 0usize;
    for line in detail.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()) {
        if genommen < DETAIL_LOG_LINES {
            out.push(format!("      {}", crate::char_prefix(line, DETAIL_LOG_WIDTH)));
            genommen += 1;
        } else {
            uebrig += 1;
        }
    }
    if uebrig > 0 {
        out.push(format!("      … {uebrig} weitere Zeile(n)"));
    }
    out
}

/// Meldet einen fachlichen Fortschritt sowohl strukturiert an die TUI als auch
/// an die normale Konsole. Für längere Phasen gedacht, nicht für Ticker.
pub fn info_line(text: &str) {
    dispatch(Level::Info, None, text, None, true, Console::Stdout);
}

pub fn eprint_line(text: &str) {
    dispatch(Level::Info, None, text, None, false, Console::Stderr);
}

/// Schweregrad einer Meldung — die TUI faerbt danach ein.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Progress,
    Pass,
    Fail,
    Warn,
}

/// Eine Zeile Benchmark-/Loop-Geschehen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchEvent {
    /// Stabile Adresse fuer die TUI (Auf-/Zuklappen im Baum).
    pub id: u64,
    /// Ortszeit `HH:MM:SS` zum Zeitpunkt der Meldung.
    pub ts: String,
    pub level: Level,
    /// Betroffenes Brain, falls die Meldung einem zuzuordnen ist.
    pub brain: Option<String>,
    /// Zusammenfassung / Knoten-Text. Bleibt im Baum immer sichtbar.
    pub text: String,
    /// Ausklappbarer Detailblock, `None` = der Text ist bereits vollstaendig.
    /// Mehrzeilige Inhalte (Terminal-Ausgabe, Antworttext, Payload) werden von
    /// der Baumansicht eingerueckt unter dem Knoten gerendert.
    pub detail: Option<String>,
}

fn bus() -> &'static Mutex<VecDeque<BenchEvent>> {
    static BUS: OnceLock<Mutex<VecDeque<BenchEvent>>> = OnceLock::new();
    BUS.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Sperrt den Bus und uebersteht dabei einen vergifteten Mutex.
///
/// Ein Panic in irgendeinem Thread darf den Lauf nicht mitreissen — der
/// Ereignisstrom ist Anzeige, keine kritische Zustandsverwaltung.
fn lock() -> std::sync::MutexGuard<'static, VecDeque<BenchEvent>> {
    bus().lock().unwrap_or_else(|e| e.into_inner())
}

/// Haengt eine Meldung an (ohne Detailblock). Der Zeitstempel wird hier gesetzt.
pub fn emit(level: Level, brain: Option<&str>, text: &str) {
    emit_detailed(level, brain, text, None);
}

/// Haengt eine Meldung mit ausklappbarem Detailblock an — nur an den Bus,
/// nicht an die Konsole. Die Verteilung selbst macht [`dispatch`].
pub fn emit_detailed(level: Level, brain: Option<&str>, text: &str, detail: Option<&str>) {
    dispatch(level, brain, text, detail, true, Console::Aus);
}

/// Sekunden seit Prozessstart — monoton, also immun gegen Zeitumstellung.
fn uptime_seconds() -> u64 {
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_secs()
}

/// Zeitpunkt des letzten Ereignisses, als Sekunden seit Prozessstart.
/// `u64::MAX` = noch gar keines.
static LAST_EVENT_AT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);

fn note_activity() {
    LAST_EVENT_AT.store(uptime_seconds(), Ordering::Relaxed);
}

/// Sekunden seit dem letzten Ereignis; `None`, solange keines kam.
///
/// Grundlage des Totmannschalters. Am 02.08.2026 stand der Dauerlauf drei
/// Stunden still, ohne dass es jemandem auffiel: die TUI zeigte unveraendert
/// den letzten Stand, und ein toter Lauf sah damit exakt aus wie ein
/// laufender. Wer nur Ereignisse anzeigt, zeigt nicht das Ausbleiben von
/// Ereignissen — genau das ist hier aber die Nachricht.
pub fn seconds_since_last_event() -> Option<u64> {
    let last = LAST_EVENT_AT.load(Ordering::Relaxed);
    if last == u64::MAX {
        return None;
    }
    Some(uptime_seconds().saturating_sub(last))
}

/// Meldet Panics aus beliebigen Threads in den Ereignisstrom.
///
/// Ein Panic in einem gespawnten Thread beendet nur diesen Thread, nicht den
/// Prozess. Ohne Hook verschwindet er spurlos: am 02.08.2026 starb so die
/// Benchmark-Schleife um 12:53, waehrend die TUI munter weiterlief und drei
/// Stunden lang niemand etwas merkte.
///
/// Der Hook nimmt bewusst `try_lock`: passiert der Panic ausgerechnet,
/// waehrend der Bus gesperrt ist, wuerde ein blockierendes Sperren im selben
/// Thread verklemmen. Lieber eine Meldung verlieren als den Prozess einfrieren
/// — die Kopie auf stderr bleibt in jedem Fall.
pub fn install_panic_hook() {
    static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unbenannt").to_string();
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unbekannte Stelle".to_string());
        let text = format!("PANIC in Thread '{name}' bei {location}");
        // Immer auf stderr — auch wenn der Bus gerade gesperrt ist.
        eprintln!("{text}\n{info}");
        if let Ok(mut q) = bus().try_lock() {
            LAST_EVENT_AT.store(uptime_seconds(), Ordering::Relaxed);
            if q.len() >= CAPACITY {
                q.pop_front();
            }
            q.push_back(BenchEvent {
                id: next_event_id(),
                ts: crate::timestamp(),
                level: Level::Fail,
                brain: None,
                text,
                detail: Some(cap_detail(&info.to_string())),
            });
        }
        previous(info);
    }));
}

/// Kappt einen Detailblock auf [`DETAIL_CAP_CHARS`] Zeichen, zeichensicher.
fn cap_detail(raw: &str) -> String {
    if raw.len() <= DETAIL_CAP_CHARS {
        return raw.to_string();
    }
    let mut out: String = raw.chars().take(DETAIL_CAP_CHARS).collect();
    out.push_str("\n… (gekürzt)");
    out
}

/// Vollstaendige Kopie des Puffers.
pub fn snapshot() -> Vec<BenchEvent> {
    lock().iter().cloned().collect()
}

/// Nur die Meldungen ab Index `n`, damit die TUI nicht jedes Frame alles
/// kopiert. Liegt `n` hinter dem Ende (Puffer wurde geleert), kommt nichts.
pub fn snapshot_since(n: usize) -> Vec<BenchEvent> {
    let q = lock();
    if n >= q.len() {
        return Vec::new();
    }
    q.iter().skip(n).cloned().collect()
}

pub fn len() -> usize {
    lock().len()
}

pub fn is_empty() -> bool {
    len() == 0
}

/// Leert den Puffer. Fuer Tests und fuer den Start eines neuen Laufs.
pub fn clear() {
    lock().clear();
}

/// Test-Serialisierung fuer alle Tests, die den Bus manipulieren.
///
/// Der Bus ist prozessglobal und die Tests laufen parallel: ein Test, der
/// `clear()`/`len()` prüft oder `set_echo_bus` schaltet, darf nicht neben
/// einem anderen laufen, der den Puffer fuellt. Deshalb greifen alle
/// Bus-Tests auf denselben Mutex zu.
#[cfg(test)]
pub fn test_bus_mutex() -> &'static Mutex<()> {
    static TEST_LOCK: Mutex<()> = Mutex::new(());
    &TEST_LOCK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Der Bus ist prozessglobal, Tests laufen parallel: alle Faelle teilen
    /// sich EINEN Test, damit sie sich nicht gegenseitig den Puffer umbauen.
    /// Ein flackernder Test waere schlimmer als kein Test.
    /// Der eigentliche Beweis: ein Panic in einem GESPAWNTEN Thread muss im
    /// Ereignisstrom landen.
    ///
    /// Genau das fehlte am 02.08.2026. Die Benchmark-Schleife starb um 12:53
    /// in ihrem Thread, der Prozess lief weiter, die TUI zeigte unveraendert
    /// den letzten Stand — drei Stunden lang merkte es niemand. Ein Test, der
    /// nur prueft, dass sich der Hook installieren laesst, haette das nicht
    /// verhindert; deshalb stirbt hier wirklich ein Thread.
    #[test]
    fn panic_in_fremdem_thread_landet_im_ereignisstrom() {
        let _test_guard = test_bus_mutex().lock();
        install_panic_hook();
        clear();

        let died = std::thread::Builder::new()
            .name("kanarienvogel".into())
            .spawn(|| panic!("absichtlich gestorben"))
            .expect("Thread startet")
            .join();
        assert!(died.is_err(), "der Thread muss wirklich gepanickt sein");

        let snap = snapshot();
        let found = snap
            .iter()
            .find(|e| e.text.contains("PANIC") && e.text.contains("kanarienvogel"));
        let found = found.unwrap_or_else(|| {
            panic!("kein Panic-Ereignis im Bus: {:?}", snap.iter().map(|e| &e.text).collect::<Vec<_>>())
        });
        assert_eq!(found.level, Level::Fail, "ein Panic ist kein Hinweis");
        assert!(
            found.detail.as_deref().unwrap_or("").contains("absichtlich gestorben"),
            "die Panic-Meldung selbst muss im Detail stehen"
        );
        // Und der Totmannschalter muss den Panic als Lebenszeichen zaehlen.
        assert!(seconds_since_last_event().is_some());
    }

    #[test]
    fn ringpuffer_haelt_reihenfolge_deckel_und_seit_index_ein() {
        let _test_guard = test_bus_mutex().lock();
        clear();

        // --- Reihenfolge bleibt erhalten -------------------------------
        emit(Level::Info, None, "erste");
        emit(Level::Pass, Some("deepseek"), "zweite");
        let snap = snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].text, "erste");
        assert_eq!(snap[1].text, "zweite");
        assert_eq!(snap[1].brain.as_deref(), Some("deepseek"));
        assert_eq!(snap[1].level, Level::Pass);
        assert!(!snap[0].ts.is_empty(), "Zeitstempel fehlt");

        // --- snapshot_since liefert nur Neues --------------------------
        let vorher = len();
        emit(Level::Warn, None, "dritte");
        let neu = snapshot_since(vorher);
        assert_eq!(neu.len(), 1);
        assert_eq!(neu[0].text, "dritte");
        // Index hinter dem Ende ist kein Fehler, sondern schlicht leer.
        assert!(snapshot_since(len() + 10).is_empty());

        // --- Deckel greift, Aeltestes faellt raus ----------------------
        clear();
        for i in 0..(CAPACITY + 50) {
            emit(Level::Progress, None, &format!("e{i}"));
        }
        assert_eq!(len(), CAPACITY, "Ringpuffer haelt den Deckel nicht ein");
        let snap = snapshot();
        assert_eq!(
            snap.first().unwrap().text,
            "e50",
            "Aeltestes nicht verworfen"
        );
        assert_eq!(snap.last().unwrap().text, format!("e{}", CAPACITY + 49));

        clear();
        assert!(is_empty());
    }

    #[test]
    fn echo_bus_spiegelt_print_line_nur_wenn_angeschaltet() {
        let _test_guard = test_bus_mutex().lock();
        // Regression 2026-08-01: der TUI-Benchmark soll die Schritt-Zeilen
        // (shell/edit/message) im Ereignisstrom zeigen — print_line muss dazu
        // bei `--verbose` in den Bus spiegeln, ohne Echo aber unverändert
        // nur an die Konsole gehen.
        let vorher_console = console_output_enabled();
        let vorher_echo = ECHO_TO_BUS.load(Ordering::Relaxed);
        set_console_output(false);
        clear();

        set_echo_bus(true);
        print_line("[shell:step-1] cargo test --lib");
        assert_eq!(len(), 1, "Echo an: Schritt-Zeile muss im Puffer landen");
        assert_eq!(snapshot()[0].text, "[shell:step-1] cargo test --lib");
        assert_eq!(snapshot()[0].detail, None, "print_line hat kein Detail");

        print_detailed(
            "[shell:step-2] cargo test",
            Some("[Terminal-Ausgabe action_id=step-2]\nalles gut\n[exit_code: 0]"),
        );
        assert_eq!(len(), 2, "print_detailed spiegelt in den Puffer");
        assert_eq!(snapshot()[1].text, "[shell:step-2] cargo test");
        assert_eq!(
            snapshot()[1].detail.as_deref(),
            Some("[Terminal-Ausgabe action_id=step-2]\nalles gut\n[exit_code: 0]"),
            "Detailblock muss den Baum fuettern"
        );
        assert_ne!(
            snapshot()[1].id, snapshot()[0].id,
            "Jeder Knoten braucht eine eigene, stabile ID"
        );

        set_echo_bus(false);
        print_line("[shell:step-3] ls");
        assert_eq!(len(), 2, "Echo aus: nichts im Puffer");

        // Aufraeumen, damit kein Test den anderen infiziert.
        set_echo_bus(vorher_echo);
        set_console_output(vorher_console);
        clear();
    }

    /// Regression zum 12.08.2026: `print_detailed` gab die Beobachtung nur an
    /// den TUI-Bus und verwarf sie auf der Konsole. Im Log stand damit der
    /// Versuch ohne sein Ergebnis.
    #[test]
    fn beobachtung_wird_gekuerzt_aber_nicht_verschluckt() {
        let d = "exit_code: 1
edit fehlgeschlagen: old_string nicht gefunden
Anker: pub fn progress";
        let out = detail_excerpt(d);
        assert_eq!(out.len(), 3, "drei Zeilen, keine verschluckt: {out:?}");
        assert!(out[0].contains("exit_code: 1"));
        assert!(out[1].contains("old_string nicht gefunden"), "die Ursache MUSS sichtbar sein");
        assert!(out.iter().all(|l| l.starts_with("      ")), "eingerueckt unter der Kopfzeile");
    }

    #[test]
    fn lange_beobachtung_wird_gedeckelt_und_der_rest_gezaehlt() {
        let d = (1..=10).map(|i| format!("zeile {i}")).collect::<Vec<_>>().join("
");
        let out = detail_excerpt(&d);
        assert_eq!(out.len(), 4, "3 Zeilen + Hinweis");
        assert!(out[3].contains("7 weitere"), "der Rest wird gezaehlt statt still zu verschwinden: {:?}", out[3]);
    }

    #[test]
    fn breite_zeile_wird_utf8_sicher_gekuerzt() {
        let d = "ä".repeat(500);
        let out = detail_excerpt(&d);
        assert_eq!(out.len(), 1);
        assert!(out[0].chars().count() <= 6 + 200 + 1);
    }

    #[test]
    fn leere_beobachtung_erzeugt_keine_zeile() {
        assert!(detail_excerpt("").is_empty());
        assert!(detail_excerpt("

   
").is_empty());
    }

    /// Die Zusicherung, die vor dem Umbau fehlte: Bus und Konsole bekommen
    /// **dieselbe** Nutzlast, weil beide aus demselben Ereignis rendern.
    ///
    /// Vorher gab `print_detailed` `detail` an den Bus und der Konsole nur die
    /// Kopfzeile — nichts im Code hielt die beiden zusammen.
    #[test]
    fn bus_und_konsole_rendern_aus_derselben_quelle() {
        let _g = test_bus_mutex().lock().unwrap_or_else(|e| e.into_inner());
        let vorher_echo = echo_bus_enabled();
        clear();
        set_echo_bus(true);

        print_detailed("[edit:step-4] src/foo.rs", Some("exit_code: 1
old_string nicht gefunden"));

        let evs = snapshot();
        let ev = evs.last().expect("Ereignis im Bus");
        let detail = ev.detail.as_deref().expect("Bus hat das Detail");
        assert!(detail.contains("old_string nicht gefunden"));

        let konsole = console_lines(ev);
        assert_eq!(konsole[0], ev.text, "Kopfzeile ist der Ereignistext");
        assert!(
            konsole.iter().any(|l| l.contains("old_string nicht gefunden")),
            "die Konsole sieht dasselbe wie der Bus: {konsole:?}"
        );

        set_echo_bus(vorher_echo);
        clear();
    }

    /// Der Umbau darf die Weiterleitung nicht veraendern — nur ihre Bauform.
    #[test]
    fn weiterleitung_bleibt_wie_vorher() {
        let _g = test_bus_mutex().lock().unwrap_or_else(|e| e.into_inner());
        let vorher_echo = echo_bus_enabled();

        // emit geht in den Bus, nie auf die Konsole.
        clear();
        emit(Level::Warn, Some("deepseek"), "nur bus");
        assert_eq!(snapshot().len(), 1);

        // eprint_line geht auf die Konsole, nie in den Bus.
        clear();
        eprint_line("nur konsole");
        assert!(snapshot().is_empty(), "eprint_line darf den Bus nicht fuellen");

        // print_line respektiert weiterhin den Spiegelschalter.
        clear();
        set_echo_bus(false);
        print_line("ohne spiegel");
        assert!(snapshot().is_empty());
        set_echo_bus(true);
        print_line("mit spiegel");
        assert_eq!(snapshot().len(), 1);

        set_echo_bus(vorher_echo);
        clear();
    }
}
