//! WebAgent — lokaler, browserbasierter Agent (Rust-Port des Python-Originals).
//!
//! Plattformunabhängiger Kern: Windows, Linux, Android (Browser via Embedded WebView).
//! Prozess-Liveness liefert dieses Crate selbst (siehe [`ProcessSnapshot`]), damit der
//! Kern überall baut. Zeitstempel werden *formatiert* über [`civil_utc`] (Python-kompatibel,
//! siehe dort) und *geparst* über `time` — das ist ohnehin Dependency.

pub mod autoresearch;
pub mod bench_events;
pub mod benchmark;
pub mod bot2bot_worker;
pub mod brain;
pub mod brain_score;
pub mod brains_health;
pub mod browser;
pub mod browser_pool;
pub mod canary;
pub mod circuit_breaker;
pub mod code_score;
pub mod comms;
pub mod config;
pub mod controller;
pub mod design_vote;
pub mod doctor;
pub mod executor;
pub mod file_actions;
pub mod knockout;
pub mod login;
pub mod loop_guard;
pub mod memory;
pub mod mock_page;
pub mod observer;
pub mod oobe;
pub mod page_driver;
pub mod prompts;
pub mod protocol;
pub mod relay;
pub mod repl;
pub mod run_store;
pub mod runs_report;
pub mod self_research;
pub mod shell_policy;
pub mod timeouts;
pub mod transcript;
pub mod tui;
#[cfg(feature = "tui")]
pub mod tui_render;
#[cfg(feature = "tui")]
pub mod tui_state;
pub mod watchdog;
#[cfg(feature = "webview")]
pub mod webview_runtime;
pub mod wiki_memory;
pub mod worker_pool;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Teilbarer Schreibgriff auf die Detailzeile eines [`StageTimer`].
///
/// `StageTimer` selbst ist an den Aufrufer gebunden; der Controller laeuft aber
/// tief in `bench_run`. Der Griff ist `Clone + Send + Sync` und laesst sich
/// dorthin durchreichen, ohne den Timer zu verschieben.
#[derive(Clone)]
pub struct StageNote(std::sync::Arc<std::sync::Mutex<String>>);

impl StageNote {
    /// Setzt den Text, den die mitlaufende Zeile hinter der Laufzeit zeigt.
    pub fn set(&self, what: &str) {
        if let Ok(mut g) = self.0.lock() {
            *g = what.trim().replace('\n', " ");
        }
    }
}

/// Laufzeit-Anzeige fuer langlaufende Schritte.
///
/// Am Terminal aktualisiert sich EINE Zeile an Ort und Stelle (Wagenruecklauf)
/// im Viertelsekundentakt und zeigt Stadium, Laufzeit und — via [`StageNote`] —
/// den gerade laufenden Schritt. Jede Zeile traegt einen absoluten Zeitstempel.
/// Geht stdout in eine Pipe, bleibt es beim Zeilenumbruch alle
/// [`PIPE_TICKER_INTERVAL_MS`], weil ein Wagenruecklauf in Logdateien nur Brei
/// erzeugt.
pub struct StageTimer {
    started: Instant,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    detail: std::sync::Arc<std::sync::Mutex<String>>,
    handle: Option<std::thread::JoinHandle<()>>,
    is_tty: bool,
}

/// Breite, auf die die mitlaufende Zeile aufgefuellt wird, damit ein kuerzer
/// gewordener Text keine Reste der vorherigen Ausgabe stehen laesst.
const LIVE_LINE_WIDTH: usize = 110;

/// Abstand der Ticker-Zeilen, wenn stdout NICHT an einer Konsole haengt.
///
/// Dort ist `\r` unbrauchbar, jede Meldung ist also eine neue Zeile. 20 s
/// haben eine Logdatei zugemuellt (Beschwerde 2026-07-24: „Timer-Spam"),
/// 60 s reichen, um zu sehen, dass noch etwas laeuft.
const PIPE_TICKER_INTERVAL_MS: u64 = 60_000;

/// `true`, wenn stdout an einem Terminal haengt.
///
/// Nur dann darf die Zeile per `\r` an Ort und Stelle aktualisiert werden. Geht
/// stdout in eine Pipe (`Tee-Object`, Logdatei), erzeugt `\r` unlesbaren Brei —
/// dort bleibt es beim periodischen Zeilenumbruch.
fn stdout_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// Prüft, ob stdout ein echtes Konsolen-Handle ist (Windows).
/// Damit wird der TTY-Pfad auch in der Windows-Konsole aktiviert,
/// nicht nur bei Unix-ttys.
#[cfg(all(windows, feature = "webview"))]
fn is_console_handle() -> bool {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Console::GetConsoleMode;
    use windows::Win32::System::Console::CONSOLE_MODE;

    let handle = HANDLE(std::io::stdout().as_raw_handle());
    let mut mode = CONSOLE_MODE(0);
    unsafe { GetConsoleMode(handle, &mut mode).is_ok() }
}

#[cfg(not(all(windows, feature = "webview")))]
fn is_console_handle() -> bool {
    false
}

/// Aktueller Zeitstempel als `HH:MM:SS` in ORTSZEIT.
///
/// Bewusst Ortszeit, nicht UTC: der Stempel steht in der Konsole vor dem
/// Nutzer, der ihn mit seiner Uhr vergleicht — ein UTC-Stempel lag hier zwei
/// Stunden daneben. Persistierte Zeitstempel (Transkript, `meta.json`) bleiben
/// UTC ueber [`now_rfc3339`]; das hier ist reine Anzeige.
///
/// Laesst sich die Zeitzone nicht bestimmen (das passiert in Prozessen mit
/// mehreren Threads, `time` verweigert dort die Offset-Abfrage), faellt es auf
/// UTC zurueck — ein leicht falscher Stempel ist besser als kein Stempel.
pub(crate) fn timestamp() -> String {
    if let Ok(now) = time::OffsetDateTime::now_local() {
        return format!("{:02}:{:02}:{:02}", now.hour(), now.minute(), now.second());
    }
    let (secs, _) = unix_now();
    let (_, _, _, h, mi, s) = civil_utc(secs);
    format!("{h:02}:{mi:02}:{s:02}")
}

impl StageTimer {
    pub fn start(label: String) -> Self {
        let is_tty = stdout_is_tty() || is_console_handle();
        let started = Instant::now();
        if !is_tty && crate::bench_events::console_output_enabled() {
            let ts = timestamp();
            println!("[{ts}] [benchmark]   {label} …");
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let detail = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let s2 = stop.clone();
        let d2 = detail.clone();
        let handle = std::thread::spawn(move || {
            use std::io::Write;
            let mut waited = 0u64;
            // Der Takt bleibt fein, damit finish() nicht auf einen langen
            // Sleep warten muss — gedrosselt wird die AUSGABE, nicht der Loop.
            let tick = 250u64;
            loop {
                if s2.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(tick));
                waited += tick;
                let s = waited / 1000;
                if is_tty && crate::bench_events::console_output_enabled() {
                    // In der laufenden Zeile aktualisieren. Fremde Ausgaben
                    // (Controller, Shell-Echo) zerschiessen die Zeile kurz —
                    // der naechste Tick malt sie 250 ms spaeter wieder sauber.
                    let d = d2.lock().map(|g| g.clone()).unwrap_or_default();
                    let suffix = if d.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", char_prefix(&d, 60))
                    };
                    let line = format!(
                        "[{}] [benchmark]   {label} … {}:{:02}{suffix}",
                        timestamp(),
                        s / 60,
                        s % 60
                    );
                    let pad = LIVE_LINE_WIDTH.saturating_sub(line.chars().count());
                    print!("\r{line}{}", " ".repeat(pad));
                    let _ = std::io::stdout().flush();
                } else if crate::bench_events::console_output_enabled()
                    && waited.is_multiple_of(PIPE_TICKER_INTERVAL_MS)
                {
                    // Ohne Flush bleibt die Zeile in Rusts Blockpuffer haengen,
                    // sobald stdout in eine Pipe geht — dann sieht der Nutzer
                    // den Ticker nie.
                    let d = d2.lock().map(|g| g.clone()).unwrap_or_default();
                    if d.is_empty() {
                        println!(
                            "[{}] [benchmark]   … {label} laeuft seit {}:{:02}",
                            timestamp(),
                            s / 60,
                            s % 60
                        );
                    } else {
                        println!(
                            "[{}] [benchmark]   … {label} laeuft seit {}:{:02} · {}",
                            timestamp(),
                            s / 60,
                            s % 60,
                            char_prefix(&d, 80)
                        );
                    }
                    let _ = std::io::stdout().flush();
                }
            }
        });
        Self {
            started,
            stop,
            detail,
            handle: Some(handle),
            is_tty,
        }
    }

    /// Teilbarer Griff auf die Detailzeile, zum Durchreichen in tiefere Ebenen.
    pub fn note_handle(&self) -> StageNote {
        StageNote(self.detail.clone())
    }

    /// Setzt den Zusatztext der mitlaufenden Zeile — was das Stadium GERADE tut
    /// (aktuelles Shell-Kommando, Aktionstyp). Damit zeigt eine einzige Zeile
    /// Stadium, Laufzeit und aktuellen Schritt, statt nur zu ticken.
    pub fn note(&self, what: &str) {
        if let Ok(mut g) = self.detail.lock() {
            *g = what.trim().replace('\n', " ");
        }
    }

    pub fn finish(mut self, result: &str) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let s = self.started.elapsed().as_secs();
        let ts = timestamp();
        let line = format!(
            "[{ts}] [benchmark]   -> {result} ({}:{:02})",
            s / 60,
            s % 60
        );
        if !crate::bench_events::console_output_enabled() {
            return;
        }
        if self.is_tty {
            // Restzeichen der Live-Zeile ueberschreiben, dann fest umbrechen.
            let pad = LIVE_LINE_WIDTH.saturating_sub(line.chars().count());
            println!("\r{line}{}", " ".repeat(pad));
        } else {
            println!("{line}");
        }
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
}

/// Zeichen-sichere Kürzung (Python-Slicing `s[:n]` arbeitet auf Zeichen, nicht Bytes).
pub fn char_prefix(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

/// Zeichen-sichere Endstück-Auswahl (`s[-n:]`).
pub fn char_suffix(s: &str, n: usize) -> &str {
    let total = s.chars().count();
    if total <= n {
        return s;
    }
    let skip = total - n;
    match s.char_indices().nth(skip) {
        Some((idx, _)) => &s[idx..],
        None => s,
    }
}

/// Sekunden seit Unix-Epoch (UTC).
fn unix_now() -> (i64, u32) {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, d.subsec_micros()),
        // Uhr vor 1970: nicht realistisch, aber niemals paniken.
        Err(e) => (-(e.duration().as_secs() as i64), 0),
    }
}

/// (Jahr, Monat, Tag, Stunde, Minute, Sekunde) aus Unix-Sekunden (UTC).
/// Algorithmus nach Howard Hinnant (civil_from_days), gemeinfrei.
///
/// Bleibt handgerollt, obwohl `time` Dependency ist: die beiden Nutzer unten
/// erzeugen **Python-kompatible** Stempel (`.%06d+00:00` bzw. `%Y%m%d_%H%M%S`),
/// die so in `meta.json` und in Run-IDs landen. `time`s Rfc3339 formatiert
/// Sub-Sekunden variabel — ein Wechsel waere ein Formatbruch, kein Aufraeumen.
/// Die *Parser*-Richtung ist dagegen vereinheitlicht (siehe doctor/run_store).
pub fn civil_utc(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, hour, minute, second)
}

/// UTC-Zeitstempel im ISO-8601-Format wie Pythons
/// `datetime.now(timezone.utc).isoformat()` — inkl. Mikrosekunden und `+00:00`.
pub fn now_rfc3339() -> String {
    let (secs, micros) = unix_now();
    let (y, mo, d, h, mi, s) = civil_utc(secs);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}+00:00",
        y, mo, d, h, mi, s, micros
    )
}

/// Run-Stempel wie Pythons `strftime("%Y%m%d_%H%M%S")` (UTC).
pub fn now_run_stamp() -> String {
    let (secs, _) = unix_now();
    let (y, mo, d, h, mi, s) = civil_utc(secs);
    format!("{:04}{:02}{:02}_{:02}{:02}{:02}", y, mo, d, h, mi, s)
}

/// Momentaufnahme der laufenden PIDs — **ein** Prozess-Aufruf statt einer pro PID.
///
/// `pid_alive` spawnt pro Aufruf `tasklist` (gemessen ~154 ms; der `/FI`-Filter
/// spart nichts, tasklist enumeriert intern ohnehin alles). Alle Aufrufer prüfen
/// aber N Kandidaten in einer Schleife — `reconcile_stale_runs` sogar im Startup
/// vor jedem Kommando. Damit kostete N Runs N × 154 ms blockierend.
///
/// `None` heißt "konnte nicht ermitteln"; [`Self::is_alive`] antwortet dann
/// konservativ `true`, wie `pid_alive` es bei Shell-Ausfall schon tat — lieber
/// nichts fälschlich als tot markieren.
pub struct ProcessSnapshot(Option<std::collections::HashSet<i64>>);

impl ProcessSnapshot {
    /// Liest die Prozessliste einmal.
    pub fn capture() -> Self {
        Self(running_pids())
    }

    /// Aus einer bekannten PID-Menge (für Tests).
    pub fn from_pids(pids: impl IntoIterator<Item = i64>) -> Self {
        Self(Some(pids.into_iter().collect()))
    }

    /// Snapshot, der nichts weiß — jede PID gilt konservativ als lebend.
    pub fn unknown() -> Self {
        Self(None)
    }

    pub fn is_alive(&self, pid: i64) -> bool {
        if pid <= 0 {
            return false;
        }
        match &self.0 {
            Some(set) => set.contains(&pid),
            None => true,
        }
    }
}

/// Alle laufenden PIDs, oder `None` wenn die Abfrage fehlschlägt.
fn running_pids() -> Option<std::collections::HashSet<i64>> {
    #[cfg(windows)]
    {
        // Eine Abfrage ohne /FI: die Liste kommt ohnehin komplett, filtern spart nichts.
        let out = std::process::Command::new("tasklist")
            .args(["/NH", "/FO", "CSV"])
            .output()
            .ok()?;
        Some(parse_tasklist_csv(&String::from_utf8_lossy(&out.stdout)))
    }
    #[cfg(not(windows))]
    {
        // `ps -e -o pid=` listet alle PIDs, eine pro Zeile.
        let out = std::process::Command::new("ps")
            .args(["-e", "-o", "pid="])
            .output()
            .ok()?;
        Some(
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<i64>().ok())
                .collect(),
        )
    }
}

/// PIDs aus `tasklist /NH /FO CSV`: `"name","pid","session",...` — zweites Feld.
#[cfg(any(windows, test))]
fn parse_tasklist_csv(text: &str) -> std::collections::HashSet<i64> {
    text.lines()
        .filter_map(|line| {
            line.split("\",\"")
                .nth(1)
                .and_then(|f| f.trim_matches('"').trim().parse::<i64>().ok())
        })
        .collect()
}

/// Prüft, ob ein Prozess mit gegebener PID lebt — plattformübergreifend ohne
/// externe Crates (Shell-Ausfall wird als "lebt" gewertet, konservativ wie das
/// Python-Original bei Unsicherheit lieber nicht fälschlich als tot markiert).
///
/// Für mehrere PIDs [`ProcessSnapshot`] nutzen — dieser Aufruf kostet einen
/// eigenen Prozess-Spawn.
pub fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(windows)]
    {
        // tasklist liefert die Zeile nur, wenn der Prozess existiert.
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH", "/FO", "CSV"])
            .output();
        match out {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                text.contains(&format!("\"{}\"", pid))
            }
            Err(_) => true,
        }
    }
    #[cfg(not(windows))]
    {
        // `kill -0 <pid>` gibt Exit 0, wenn der Prozess existiert.
        match std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
        {
            Ok(st) => st.success(),
            Err(_) => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_prefix_suffix_are_char_safe() {
        assert_eq!(char_prefix("äöü-abc", 3), "äöü");
        assert_eq!(char_suffix("äöü-abc", 3), "abc");
        assert_eq!(char_prefix("kurz", 100), "kurz");
        assert_eq!(char_suffix("kurz", 100), "kurz");
    }

    #[test]
    fn civil_utc_known_epoch() {
        // 2026-07-12T10:00:00Z == 1_783_850_400 Unix-Sekunden.
        let (y, mo, d, h, mi, s) = civil_utc(1_783_850_400);
        assert_eq!((y, mo, d, h, mi, s), (2026, 7, 12, 10, 0, 0));
    }

    #[test]
    fn civil_utc_unix_zero() {
        assert_eq!(civil_utc(0), (1970, 1, 1, 0, 0, 0));
    }

    #[test]
    fn rfc3339_shape() {
        let ts = now_rfc3339();
        assert!(ts.ends_with("+00:00"), "ts={ts}");
        assert_eq!(ts.len(), "2026-07-12T10:00:00.000000+00:00".len());
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::{parse_tasklist_csv, ProcessSnapshot};

    const SAMPLE: &str = "\"System Idle Process\",\"0\",\"Services\",\"0\",\"8 K\"\r\n\"webagent.exe\",\"29576\",\"Console\",\"1\",\"12.345 K\"\r\n\"pwsh.exe\",\"1234\",\"Console\",\"1\",\"98.765 K\"";

    #[test]
    fn parses_pids_from_tasklist_csv() {
        let pids = parse_tasklist_csv(SAMPLE);
        assert!(pids.contains(&29576), "webagent.exe PID fehlt: {pids:?}");
        assert!(pids.contains(&1234), "pwsh.exe PID fehlt: {pids:?}");
        assert_eq!(pids.len(), 3);
    }

    #[test]
    fn parse_ignores_garbage_lines() {
        assert!(parse_tasklist_csv("INFO: no tasks are running").is_empty());
        assert!(parse_tasklist_csv("").is_empty());
    }

    #[test]
    fn snapshot_answers_from_the_set() {
        let s = ProcessSnapshot::from_pids([100, 200]);
        assert!(s.is_alive(100));
        assert!(!s.is_alive(300));
    }

    // Konservativ wie pid_alive: laesst sich die Liste nicht ermitteln, gilt jede
    // PID als lebend — lieber keinen laufenden Run faelschlich als verwaist killen.
    #[test]
    fn unknown_snapshot_is_conservative() {
        let s = ProcessSnapshot::unknown();
        assert!(s.is_alive(12345));
    }

    #[test]
    fn nonpositive_pid_is_never_alive() {
        assert!(!ProcessSnapshot::unknown().is_alive(0));
        assert!(!ProcessSnapshot::unknown().is_alive(-1));
        assert!(!ProcessSnapshot::from_pids([0]).is_alive(0));
    }
}
