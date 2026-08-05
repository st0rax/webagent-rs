//! diagnose_log — strukturierte Diagnose, die der TUI nicht ins Bild funkt.
//!
//! Der Brain-Schwarm hat `tracing` in drei Abstimmungsrunden gefordert
//! (`verbesserungs-top10-2026-07`, `-runde2`). Bis zum 05.08.2026 gab es null
//! Treffer im Code, dafuer 383 `println!`/`eprintln!`.
//!
//! # Warum das hier nicht einfach nach stdout schreibt
//!
//! Die ratatui-Oberflaeche besitzt das Terminal im Alternate Screen. Direkte
//! stdout/stderr-Zeilen zerreissen ihr Bild — genau deshalb existiert
//! [`crate::bench_events::set_console_output`]. Ein Subscriber, der munter nach
//! stdout schreibt, macht denselben Fehler noch einmal, nur strukturiert.
//!
//! Deshalb schreibt die Diagnose in eine DATEI unter `data/logs/`. Das hat
//! einen zweiten Vorteil, der am 05.08.2026 teuer erkauft wurde: als der
//! Benchmark sich um 12:53 selbst beendete, war die Begruendung nur im
//! Terminalfenster sichtbar und beim naechsten Start weg. Eine Datei ueberlebt
//! den Neustart.
//!
//! # Was NICHT hierher gehoert
//!
//! Benutzerausgaben. `webagent quests`, `doctor --json`, `menu` und die
//! uebrigen CLI-Befehle geben Ergebnisse aus, die ein Mensch lesen oder ein
//! Skript parsen will. Wer die in Logzeilen verwandelt, zerstoert die
//! Bedienbarkeit. Diagnose ist, was beim Suchen nach einer Ursache hilft —
//! nicht, was der Aufrufer bestellt hat.

use std::sync::OnceLock;

/// Umgebungsvariable fuer die Filterstufe, z.B. `WEBAGENT_LOG=debug`.
pub const FILTER_ENV: &str = "WEBAGENT_LOG";

/// Pfad der aktuellen Logdatei, sobald initialisiert.
static LOG_PATH: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Wohin die Diagnose geschrieben wird.
pub fn log_path() -> Option<&'static std::path::Path> {
    LOG_PATH.get().map(|p| p.as_path())
}

/// Richtet die Diagnose einmalig ein. Weitere Aufrufe tun nichts.
///
/// Schlaegt das Anlegen der Datei fehl, laeuft das Programm ohne Diagnose
/// weiter — ein nicht schreibbares Logverzeichnis darf kein Startfehler sein.
pub fn init() {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }

    let dir = crate::config::data_dir().join("logs");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("webagent-{}.log", crate::now_run_stamp()));
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = LOG_PATH.set(path);

    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_env(FILTER_ENV).unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file)
        .with_ansi(false) // Steuerzeichen in einer Logdatei sind nur Muell.
        .with_target(true)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zweiter_aufruf_ist_folgenlos() {
        // Prozessglobal: ein zweiter Subscriber wuerde sonst scheitern und im
        // schlimmsten Fall den ersten unbrauchbar machen.
        init();
        let erste = log_path().map(|p| p.to_path_buf());
        init();
        assert_eq!(log_path().map(|p| p.to_path_buf()), erste);
    }

    #[test]
    fn filtervariable_heisst_wie_dokumentiert() {
        // Der Name steht in der Modul-Doku und in der README-Zeile des
        // Commits; ein stiller Umbenenner macht jede Anleitung falsch.
        assert_eq!(FILTER_ENV, "WEBAGENT_LOG");
    }
}
