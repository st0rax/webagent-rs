//! Interaktive REPL: mehrere Aufgaben nacheinander gegen dasselbe Brain, wobei
//! die Konversation über `resume` fortgesetzt wird.
//!
//! Der Controller gibt die Antwort des Brains selbst aus (message-Action, siehe
//! controller.rs). Hinweis: In dieser v1 startet der Browser pro Turn neu
//! (Controller-Lebenszyklus start→stop). Eine über Turns offene Session wäre
//! eine spätere Optimierung.

use std::io::{self, BufRead, Write};

use crate::browser::WebBrainBackend;
use crate::controller::AgentController;
use crate::executor::PlatformShellExecutor;

/// Startet die REPL. Liest Aufgaben von stdin, bis `/exit` oder EOF.
pub fn run_repl(brain_id: &str, headless: bool) -> i32 {
    let backend = match WebBrainBackend::from_config(brain_id) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[repl] {e}");
            return 2;
        }
    };
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::new(backend, executor, 100);
    let mut resume: Option<String> = None;

    println!("[repl] Brain={brain_id}. Aufgabe eingeben. Befehle: /new (neue Konversation), /exit.");
    let stdin = io::stdin();
    loop {
        print!("\n> ");
        let _ = io::stdout().flush();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF (Ctrl+Z bzw. Ctrl+D)
            Ok(_) => {}
            Err(_) => break,
        }
        let task = line.trim();
        if task.is_empty() {
            continue;
        }
        match task {
            "/exit" | "/quit" => break,
            "/new" => {
                resume = None;
                println!("[repl] Neue Konversation.");
                continue;
            }
            _ => {}
        }

        match controller.run(task, brain_id, resume.as_deref(), headless) {
            Ok(meta) => {
                resume = Some(meta.run_id.clone());
                eprintln!("[repl] (status={} run_id={})", meta.status, meta.run_id);
            }
            Err(e) => eprintln!("[repl] Fehler: {e}"),
        }
    }
    println!("[repl] beendet.");
    0
}
