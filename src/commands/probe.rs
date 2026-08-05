//! `webagent probe` — Oberflaechen-Analyse einer BELIEBIGEN Chat-Seite.
//!
//! Anders als `survey` (das ein eingetragenes Brain samt Selektordatei
//! voraussetzt) bekommt dieser Befehl nur eine URL und klopft ab, was da ist.
//!
//! # Warum ein Wegwerf-Profil
//!
//! Die analysierte Seite ist per Definition fremd. Sie in den kanonischen
//! Profilen unter `%LOCALAPPDATA%\webagent\profiles` zu oeffnen, waere die
//! einzige Stelle im Projekt, an der eine unbekannte Seite Cookies neben den
//! angemeldeten Sitzungen ablegen koennte. Deshalb ein leeres Verzeichnis unter
//! `std::env::temp_dir()` — bewusst *ohne* die Profilkopie, die `browser_pool`
//! fuer den Swarm macht: hier soll gerade nichts Angemeldetes mitkommen.

#[cfg(feature = "webview")]
use std::time::Duration;

/// Anzahl der Versuche, das DOM einzusammeln.
///
/// Chat-Oberflaechen sind Single-Page-Apps: `document.readyState == complete`
/// heisst dort nur, dass das leere Geruest steht. Wer einmal misst und aufgibt,
/// meldet „nichts gefunden" fuer eine Seite, die zwei Sekunden spaeter voll
/// bedienbar ist.
#[cfg(feature = "webview")]
const ATTEMPTS: usize = 8;

#[cfg(not(feature = "webview"))]
pub fn cmd_probe(_url: &str, _throwaway_profile: bool, _headless: bool, _json: bool) -> i32 {
    eprintln!("[probe] {}", crate::page_driver::webview_unavailable());
    2
}

#[cfg(feature = "webview")]
pub fn cmd_probe(url: &str, throwaway_profile: bool, headless: bool, json: bool) -> i32 {
    use webagent::page_driver::PageDriver;
    use webagent::webview_runtime::WebViewRuntime;

    let profile = if throwaway_profile {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("webagent-probe-{}-{stamp}", std::process::id()))
    } else {
        // Ausdruecklich verlangt (`--throwaway-profile false`): fuer ein
        // bereits bekanntes, angemeldetes Brain am eigenen kanonischen Profil.
        webagent::config::profiles_dir().join("probe")
    };
    if let Err(e) = std::fs::create_dir_all(&profile) {
        eprintln!("[probe] Profilverzeichnis nicht anlegbar: {e}");
        return 2;
    }
    if !json {
        eprintln!("[probe] {url}");
        eprintln!(
            "[probe] Profil: {} ({})",
            profile.display(),
            if throwaway_profile {
                "Wegwerf, leer"
            } else {
                "kanonisch"
            }
        );
    }

    let runtime = match WebViewRuntime::launch(&profile, headless) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[probe] WebView nicht startbar: {e}");
            return 2;
        }
    };
    let mut driver = match runtime.open_page(&profile, url, headless, "probe") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[probe] Seite nicht zu oeffnen: {e}");
            return 2;
        }
    };
    if let Err(e) = driver.navigate(url, Duration::from_secs(45)) {
        eprintln!("[probe] Navigation fehlgeschlagen: {e}");
        return 2;
    }

    // Einsammeln, bis sich die Zahl der Bedienelemente stabilisiert — nicht
    // beim ersten Treffer aufhoeren: eine halb aufgebaute Seite liefert schon
    // Elemente, aber die Hauptleiste fehlt noch.
    let mut candidates = Vec::new();
    let mut last = 0usize;
    for attempt in 1..=ATTEMPTS {
        std::thread::sleep(Duration::from_millis(1500));
        match webagent::brain_probe::collect(&mut driver) {
            Ok(found) => {
                let n = found.iter().filter(|c| c.visible).count();
                if !json {
                    eprintln!("[probe] Versuch {attempt}: {n} sichtbare Bedienelemente");
                }
                candidates = found;
                if n > 0 && n == last {
                    break;
                }
                last = n;
            }
            Err(e) => {
                if !json {
                    eprintln!("[probe] Versuch {attempt}: {e}");
                }
            }
        }
    }

    let current_url = driver.current_url().unwrap_or_else(|_| url.to_string());
    let login = webagent::brain_probe::looks_like_login(&candidates, &current_url);
    let proposals = webagent::brain_probe::classify(&candidates);
    let visible = candidates.iter().filter(|c| c.visible).count();

    // Bewusst KEIN automatisches `verify()`: das klickt, und auf einer fremden
    // Seite weiss niemand, was ein Klick ausloest (Abschicken, Kauf, Zustimmung).
    // Nachweisen darf man erst, wenn ein Mensch die Vorschlaege gesehen hat.

    if json {
        let payload = serde_json::json!({
            "url": current_url,
            "visible_controls": visible,
            "login_required": login,
            "proposals": proposals.iter().map(|p| serde_json::json!({
                "capability_key": p.capability_key,
                "selector_key": p.selector_key,
                "selector": p.selector,
                "confidence": p.confidence,
                "evidence": p.evidence,
            })).collect::<Vec<_>>(),
        });
        match serde_json::to_string_pretty(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("[probe] JSON-Fehler: {e}");
                return 1;
            }
        }
    } else {
        println!();
        println!("  Seite: {current_url}");
        println!("  {visible} sichtbare Bedienelemente eingesammelt");
        if login {
            println!();
            println!("  ANMELDUNG VERLANGT — die Seite zeigt eine Anmeldemaske.");
            println!("  Es wird nichts eingegeben und nichts automatisiert.");
            println!("  Die Vorschlaege unten stammen damit von der Anmeldeseite,");
            println!("  nicht von der Chat-Oberflaeche, und taugen nichts.");
        }
        println!();
        if proposals.is_empty() {
            println!("  KEINE VORSCHLAEGE.");
        } else {
            println!(
                "  {:<18} {:<24} {:>5}  {}",
                "FAEHIGKEIT", "SELEKTORSCHLUESSEL", "GUETE", "SELEKTOR"
            );
            for p in &proposals {
                println!(
                    "  {:<18} {:<24} {:>4}%  {}",
                    p.capability_key, p.selector_key, p.confidence, p.selector
                );
                println!("  {:<18} {:<24} {:>5}  Beleg: {}", "", "", "", p.evidence);
            }
        }
        println!();
    }

    drop(driver);
    drop(runtime);
    if throwaway_profile {
        // Best effort: ein gehaltener WebView-Handle darf den Befehl nicht mit
        // einem Aufraeumfehler scheitern lassen.
        let _ = std::fs::remove_dir_all(&profile);
    }

    if login {
        1
    } else if proposals.is_empty() {
        // Nichts gefunden ist ein Messfehler, kein Ergebnis — sonst sieht ein
        // Skript einen Erfolg, wo die Analyse versagt hat.
        1
    } else {
        0
    }
}
