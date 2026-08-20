//! Ballspiel / Zaehl-Test: die Brains zaehlen abwechselnd der Reihe nach bis
//! `target` — der Ball (die Zahl) wandert im Kreis. Kein Featurespiel, sondern
//! ein Performance- und Zuverlaessigkeitsmass des Harness: jede Runde oeffnet
//! eine Sitzung, alle bleiben offen, die Zahl wechselt den Brain pro Schritt.
//! Traces werden als JSON gespeichert und am Ende ausgewertet.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use crate::brain::{BrainBackend, SessionState};
use crate::browser::WebBrainBackend;
use crate::timeouts::resolve_timeout;

/// Per-Brain-Mindestfenster fuer `wait_response`. chatgpt braucht in einer
/// frischen Sitzung bis zu mehrere Minuten fuer die erste Antwort (Platzhalter
/// sofort, Text erst spaet) — ohne Floor wuerde jeder Turn als Drop enden und
/// das Spiel nach drei Fehlwuerfen abbrechen. Der Floor verwandelt Drops in
/// "langsam aber korrekt" und bleibt ein ehrliches Mass: die Zeit fliesst in
/// die Gesamtdauer und den Brain-Average ein.
fn wait_floor(brain: &str) -> Option<f64> {
    match brain.to_lowercase().as_str() {
        "chatgpt" => Some(240.0),
        _ => None,
    }
}

#[derive(Debug, serde::Serialize)]
struct Turn {
    step: u32,
    brain: String,
    expected: u32,
    ok: bool,
    latency_ms: u64,
    note: String,
}

#[derive(Debug, serde::Serialize)]
struct BrainStat {
    brain: String,
    turns: u32,
    ok: u32,
    drops: u32,
    total_ms: u64,
    avg_ms: u64,
}

fn antwort_enthaelt(text: &str, n: u32) -> bool {
    // Die Zahl als eigenstaendiges Token, nicht als Teil von z.B. 157 oder 57.0.
    let pat = format!(r"\b{n}\b");
    regex_lite_match(&pat, text)
}

/// Kleiner Wortgrenzen-Matcher ohne Regex-Crate-Abhaengigkeit (Zahlen nur).
fn regex_lite_match(pattern: &str, text: &str) -> bool {
    // pattern ist immer "\b<zahl>\b"; zerlegen reicht.
    let inner = &pattern[2..pattern.len() - 2];
    let mut rest = text;
    loop {
        let Some(pos) = rest.find(inner) else {
            return false;
        };
        let before_ok = pos == 0 || !rest.as_bytes()[pos - 1].is_ascii_digit();
        let after = pos + inner.len();
        let after_ok = after == rest.len() || !rest.as_bytes()[after].is_ascii_digit();
        if before_ok && after_ok {
            return true;
        }
        rest = &rest[after..];
    }
}

/// Faehrt das Zaehl-Spiel; gibt den Exit-Code zurueck.
pub fn counting_game(brain_ids: &[String], headless: bool, target: u32) -> i32 {
    if brain_ids.is_empty() {
        crate::bench_events::eprint_line("[count] keine Brains angegeben");
        return 2;
    }
    let started = Instant::now();
    crate::bench_events::eprint_line(&format!(
        "[count] {} Brain(s), Ziel: {target} (Runden = {})",
        brain_ids.len(),
        target
    ));

    // Alle Sitzungen oeffnen und bereit melden lassen.
    let mut backends: Vec<(String, Option<WebBrainBackend>, bool)> = Vec::new();
    for id in brain_ids {
        crate::bench_events::eprint_line(&format!("[count] {id}: Sitzung oeffnen…"));
        let mut backend = match WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                crate::bench_events::eprint_line(&format!(
                    "[count] {id}: nicht konfigurierbar: {e}"
                ));
                backends.push((id.clone(), None, false));
                continue;
            }
        };
        match backend.start(headless) {
            Ok(()) => {}
            Err(e) => {
                crate::bench_events::eprint_line(&format!(
                    "[count] {id}: Start fehlgeschlagen: {e}"
                ));
                backends.push((id.clone(), None, false));
                continue;
            }
        }
        let ready_to = resolve_timeout("ensure_ready", id, "", None);
        let state = backend
            .ensure_ready(ready_to)
            .unwrap_or(SessionState::Error);
        let ready = state == SessionState::Ready;
        if ready {
            if let Err(e) = backend.new_chat() {
                crate::bench_events::eprint_line(&format!(
                    "[count] {id}: new_chat fehlgeschlagen: {e}"
                ));
            }
            // deepseek steht standardmaessig auf DeepThink (R1): das streamt ~100 s
            // Reasoning pro Turn und kostet den vollen wait_timeout. Fuer das
            // Zaehlspiel aus — schaltet sich aus, wenn erkennbar aktiv.
            if !backend.disable_toggle("reasoning_toggle") {
                crate::bench_events::eprint_line(&format!(
                    "[count] {id}: reasoning_toggle liess sich nicht abschalten"
                ));
            }
        }
        crate::bench_events::eprint_line(&format!(
            "[count] {id}: {}",
            if ready {
                "bereit".to_string()
            } else {
                format!("nicht bereit ({state:?})")
            }
        ));
        if ready && wait_floor(id).is_some() {
            crate::bench_events::eprint_line(&format!(
                "[count] {id}: wait-Fenster >= {}s (gemessen sehr langsam)",
                wait_floor(id).unwrap_or(0.0) as u32
            ));
        }
        backends.push((id.clone(), Some(backend), ready));
    }

    let mut turns: Vec<Turn> = Vec::with_capacity(target as usize);
    let mut active: Vec<usize> = (0..backends.len()).collect();
    let mut streak: HashMap<String, u32> = HashMap::new();

    for step in 1..=target {
        if active.is_empty() {
            crate::bench_events::eprint_line("[count] alle Brains deaktiviert — Abbruch.");
            break;
        }
        let idx = active[(step - 1) as usize % active.len()];
        let (brain, backend, ready) = &mut backends[idx];
        let expected = step;
        let prev = step - 1;
        let mut note = String::new();
        let mut ok = false;
        let turn_started = Instant::now();

        if !*ready {
            note = "sitzung nicht bereit".to_string();
        } else if let Some(b) = backend.as_mut() {
            let prompt = format!(
                "Ballspiel: Wir zaehlen der Reihe nach im Kreis. Bisher wurde bis {prev} gerufen. \
                 Du bist dran: rufe jetzt die Zahl {expected}. Antworte NUR mit der Zahl {expected} \
                 und sonst mit nichts."
            );
            match b.send(&prompt) {
                Ok(baseline) => {
                    let wait_to =
                        resolve_timeout("wait_response", brain, &prompt, wait_floor(brain));
                    match b.wait_response(baseline, wait_to) {
                        Ok(resp) if resp.text.trim().is_empty() => {
                            note = format!(
                                "leere Antwort (status={})  | {}",
                                resp.backend_status,
                                b.diagnostic_state()
                            );
                        }
                        Ok(resp) => {
                            let text = resp.text.trim().to_string();
                            if !resp.generation_complete {
                                // Antworttext da, aber der Lauf endete im Timeout
                                // (deepseek/DeepThink streamt ueber die Frist hinaus).
                                note = format!(
                                    "unvollstaendig (status={}): {}",
                                    resp.backend_status,
                                    kurz(&text, 40)
                                );
                            } else if antwort_enthaelt(&text, expected) {
                                ok = true;
                                note = "korrekt gerufen".to_string();
                            } else {
                                note = format!("falsche Zahl: {}", kurz(&text, 60));
                            }
                        }
                        Err(e) => {
                            note = format!("Timeout/Fehler: {}", kurz(&e, 60));
                        }
                    }
                }
                Err(e) => {
                    note = format!(
                        "Senden fehlgeschlagen: {}  | {}",
                        kurz(&e, 60),
                        b.diagnostic_state()
                    );
                }
            }
        }

        let latency_ms = turn_started.elapsed().as_millis() as u64;
        turns.push(Turn {
            step,
            brain: brain.clone(),
            expected,
            ok,
            latency_ms,
            note: note.clone(),
        });
        let mark = if ok { "OK " } else { "DROP" };
        println!(
            "[count] {:>3}/{} {mark} {:<10} erwartet={expected} {}ms {}",
            step, target, brain, latency_ms, note
        );
        // Der Ball bleibt im Spiel: ein verpatzter Wurf stoppt das Zaehlen nicht.
        // Erst 3 Fehlwuerfe in Folge fuer DIESEN Brain nehmen ihn aus der Rotation —
        // der Rest laeuft weiter (ein langsamer Brain darf die Messung nicht killen).
        let s = streak.entry(brain.clone()).or_insert(0);
        if ok {
            *s = 0;
        } else {
            *s += 1;
        }
        if !ok && *s >= 3 {
            crate::bench_events::eprint_line(&format!(
                "[count] {brain}: 3 Fehlwuerfe in Folge — aus der Rotation genommen, der Rest laeuft weiter."
            ));
            active.retain(|&i| i != idx);
        }
    }

    for (_, backend, _) in &mut backends {
        if let Some(b) = backend {
            let _ = b.stop();
        }
    }

    // Auswertung.
    let total_ms = started.elapsed().as_millis() as u64;
    let mut stats: Vec<BrainStat> = Vec::new();
    for (id, _, _) in &backends {
        let mine: Vec<&Turn> = turns.iter().filter(|t| &t.brain == id).collect();
        let ok = mine.iter().filter(|t| t.ok).count() as u32;
        let sum: u64 = mine.iter().map(|t| t.latency_ms).sum();
        let n = mine.len() as u32;
        stats.push(BrainStat {
            brain: id.clone(),
            turns: n,
            ok,
            drops: n - ok,
            total_ms: sum,
            avg_ms: if n > 0 { sum / n as u64 } else { 0 },
        });
    }

    println!(
        "\n[count] Auswertung — {} Schritte, Gesamtzeit {}s",
        turns.len(),
        total_ms / 1000
    );
    println!(
        "[count] {:<10} {:>5} {:>5} {:>5} {:>8} {:>7}",
        "Brain", "Turns", "OK", "Drop", "Total", "Avg"
    );
    for s in &stats {
        println!(
            "[count] {:<10} {:>5} {:>5} {:>5} {:>8} {:>7}",
            s.brain, s.turns, s.ok, s.drops, s.total_ms, s.avg_ms
        );
    }

    // Traces speichern.
    let out = save_trace(&turns, &stats, total_ms);
    match out {
        Ok(path) => println!("[count] Traces: {}", path.display()),
        Err(e) => {
            crate::bench_events::eprint_line(&format!("[count] Trace nicht speicherbar: {e}"))
        }
    }

    // Exit-Code: 1 nur, wenn der Lauf vorzeitig abgebrochen ist (kein Brain aktiv,
    // bevor das Ziel erreicht war). Ein vollstaendiger Lauf ist auch mit Drops ein
    // Messergebnis — Auswertung entscheidet.
    let aborted_early = turns.len() < target as usize;
    if aborted_early {
        1
    } else {
        0
    }
}

fn kurz(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() > max {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    } else {
        s.to_string()
    }
}

fn save_trace(turns: &[Turn], stats: &[BrainStat], total_ms: u64) -> std::io::Result<PathBuf> {
    #[derive(serde::Serialize)]
    struct Trace<'a> {
        target: usize,
        total_ms: u64,
        per_brain: &'a [BrainStat],
        turns: &'a [Turn],
    }
    let dir = std::env::temp_dir().join("opencode");
    std::fs::create_dir_all(&dir)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = dir.join(format!("count_trace_{stamp}.json"));
    let body = serde_json::to_string_pretty(&Trace {
        target: turns.len(),
        total_ms,
        per_brain: stats,
        turns,
    })
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    std::fs::write(&path, body)?;
    Ok(path)
}
