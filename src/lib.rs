//! WebAgent — lokaler, browserbasierter Agent (Rust-Port des Python-Originals).
//!
//! Plattformunabhängiger Kern: Windows, Linux, Android (Browser via Remote-CDP).
//! Bewusst ohne schwere/plattformgebundene Dependencies — Zeitstempel und
//! Prozess-Liveness liefert dieses Crate selbst, damit der Kern überall baut.

pub mod brain;
pub mod browser;
pub mod cdp;
pub mod config;
pub mod controller;
pub mod doctor;
pub mod executor;
pub mod loop_guard;
pub mod memory;
pub mod observer;
pub mod prompts;
pub mod protocol;
pub mod run_store;
pub mod timeouts;
pub mod transcript;
pub mod watchdog;

use std::time::{SystemTime, UNIX_EPOCH};

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
pub fn civil_utc(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let hour = (rem / 3600) as u32;
    let minute = ((rem % 3600) / 60) as u32;
    let second = (rem % 60) as u32;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as i64; // [0, 146096]
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

/// Prüft, ob ein Prozess mit gegebener PID lebt — plattformübergreifend ohne
/// externe Crates (Shell-Ausfall wird als "lebt" gewertet, konservativ wie das
/// Python-Original bei Unsicherheit lieber nicht fälschlich als tot markiert).
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
