//! DOM-Inspektion eines Brains, um Selektor-Drift aufzudecken.
//!
//!   set WEBAGENT_PROFILE_DIR=C:\Users\storax\Desktop\webagent\data\profiles\shared
//!   cargo run --example inspect -- qwen
//!
//! Öffnet den Browser sichtbar, meldet fuer die konfigurierten Selektoren die
//! Trefferzahl, listet Kandidaten-Container und Stop-/Send-Buttons — im
//! Ruhezustand und waehrend/nach einer kurzen Generierung.

use std::time::Duration;

use serde_json::Value;
use webagent::brain::BrainBackend;
use webagent::browser::WebBrainBackend;

fn concise(v: &Value) -> String {
    let counts = v.get("counts").cloned().unwrap_or(Value::Null);
    let cands = v.get("candidates").cloned().unwrap_or(Value::Null);
    let empty: Vec<Value> = Vec::new();
    let s = |o: &Value, k: &str| o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let i64f = |o: &Value, k: &str| o.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    let b64f = |o: &Value, k: &str| o.get(k).and_then(|x| x.as_bool()).unwrap_or(false);

    // Nachrichten-Container mit Text (Kandidaten fuer assistant_message).
    let msgs: Vec<String> = v
        .get("messages")
        .and_then(|m| m.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|m| i64f(m, "tl") > 0)
        .map(|m| format!("    [{}ch] .{} | \"{}\"", i64f(m, "tl"), s(m, "cls"), s(m, "tp")))
        .collect();

    // Icon-/beschriftete Buttons (Stop-Button ist meist ein svg-Icon-Button).
    let btns: Vec<String> = v
        .get("buttons")
        .and_then(|b| b.as_array())
        .unwrap_or(&empty)
        .iter()
        .filter(|b| b64f(b, "svg") || !s(b, "al").is_empty() || !s(b, "dt").is_empty())
        .map(|b| {
            format!(
                "    btn svg={} al='{}' dt='{}' .{} \"{}\"",
                b64f(b, "svg"),
                s(b, "al"),
                s(b, "dt"),
                s(b, "cls"),
                s(b, "tp")
            )
        })
        .collect();

    // Groesste Text-Bloecke (leaf-ish) — der Antwort-Container ist hier zu finden.
    let tbs: Vec<String> = v
        .get("textblocks")
        .and_then(|t| t.as_array())
        .unwrap_or(&empty)
        .iter()
        .map(|t| format!("    [{}ch] {} .{} | \"{}\"", i64f(t, "tl"), s(t, "tag"), s(t, "cls"), s(t, "tp")))
        .collect();

    format!(
        "counts={counts}\n  candidates={cands}\n  TEXTBLOCKS (Antwort-Container?):\n{}\n  BUTTONS:\n{}",
        tbs.join("\n"),
        btns.join("\n")
    )
}

fn main() {
    let brain = std::env::args().nth(1).unwrap_or_else(|| "qwen".to_string());
    let mut b = match WebBrainBackend::from_config(&brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("from_config: {e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = b.start(false) {
        eprintln!("start: {e}");
        std::process::exit(1);
    }
    let st = b.ensure_ready(60.0).unwrap_or(webagent::brain::SessionState::Error);
    eprintln!("session_state={st:?}");

    match b.dom_report() {
        Ok(v) => {
            eprintln!(
                "url={} title={} viewport={}x{} webdriver={} ua={}",
                v.get("url").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("title").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("w").and_then(|x| x.as_i64()).unwrap_or(0),
                v.get("h").and_then(|x| x.as_i64()).unwrap_or(0),
                v.get("wd").map(|x| x.to_string()).unwrap_or_default(),
                v.get("ua").and_then(|x| x.as_str()).unwrap_or("")
            );
            println!("=== IDLE (vor senden) ===\n{}", concise(&v));
        }
        Err(e) => println!("IDLE dom_report FEHLER: {e}"),
    }

    let sent = b
        .send("Zaehle langsam von eins bis fuenfzehn, jede Zahl in einer eigenen Zeile.")
        .is_ok();
    eprintln!("send ok={sent}");

    for i in 0..9 {
        std::thread::sleep(Duration::from_millis(2000));
        match b.dom_report() {
            Ok(v) => println!("\n=== t=+{}s ===\n{}", (i + 1) * 2, concise(&v)),
            Err(e) => println!("\n=== t=+{}s === FEHLER: {e}", (i + 1) * 2),
        }
    }
    b.stop().ok();
    println!("\n=== fertig ===");
}
