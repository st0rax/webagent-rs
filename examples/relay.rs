//! Live-Relay über den Rust-Port zur Verifikation/Härtung der Antworterkennung.
//!
//! Nutzung (Browser wird sichtbar gestartet; braucht eingeloggtes Profil):
//!   set WEBAGENT_PROFILE_DIR=C:\Users\storax\Desktop\webagent\data\profiles\shared
//!   cargo run --example relay -- qwen "Erklaere in fuenf Absaetzen, wie CDP funktioniert."
//!
//! Gibt aus: session_state, ob die Antwort als VOLLSTAENDIG erkannt wurde,
//! backend_status, Laenge, und ob sie als webagent/1-Protokoll parsen wuerde.

use std::time::Duration;

use webagent::brain::BrainBackend;
use webagent::browser::WebBrainBackend;

fn main() {
    let mut args = std::env::args().skip(1);
    let brain = args.next().unwrap_or_else(|| "qwen".to_string());
    let message = args
        .next()
        .unwrap_or_else(|| "Sag in einem Satz hallo.".to_string());

    let mut backend = match WebBrainBackend::from_config(&brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[relay] from_config: {e}");
            std::process::exit(2);
        }
    };

    if let Err(e) = backend.start(false) {
        eprintln!("[relay] start: {e}");
        std::process::exit(1);
    }

    let state = backend
        .ensure_ready(60.0)
        .unwrap_or(webagent::brain::SessionState::Error);
    eprintln!("[relay] session_state={state:?}");

    // Frischen Chat erzwingen, damit baseline=0 (keine bestehende Konversation).
    let _ = backend.new_chat();
    std::thread::sleep(Duration::from_millis(800));

    let baseline = match backend.send(&message) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[relay] send: {e}");
            let _ = backend.stop();
            std::process::exit(1);
        }
    };

    let resp = match backend.wait_response(baseline, 200.0) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[relay] wait_response: {e}");
            let _ = backend.stop();
            std::process::exit(1);
        }
    };
    let _ = backend.stop();

    let truncated = webagent::protocol::is_possibly_truncated(&resp.text);
    let parses = webagent::protocol::parse(&resp.text).valid;
    println!("========================================================");
    println!("complete        = {}", resp.generation_complete);
    println!("backend_status  = {}", resp.backend_status);
    println!("len_chars       = {}", resp.text.chars().count());
    println!("looks_truncated = {truncated}");
    println!("parses_protocol = {parses}");
    println!("---------------- ANTWORTTEXT ---------------------------");
    println!("{}", resp.text);
    println!("========================================================");
}
