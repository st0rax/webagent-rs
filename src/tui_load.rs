//! tui_load — dateibasierte Beschaffung des Worker-Dashboards.
//!
//! Aus `tui_state.rs` herausgeloest (Refactoring 02:45): der Teil, der
//! AgentViews aus bot2bot_root + Pool-Zustand + Heartbeats + Verlauf laedt.
//! `tui_state.rs` bleibt das Zustandsmodell (App, Views, pure Helfer) — der
//! Loader ist der Dateisystem-Adapter der TUI und hat dort nichts zu suchen.

use std::path::Path;
use std::time::SystemTime;

use crate::config::bot2bot_root;
use crate::tui_state::AgentView;
use crate::worker_pool::PoolState;

/// Wie viele Verlaufseintraege der Ausklapp-Blick hoechstens laedt.
const DETAIL_HISTORY: usize = 12;

/// Lädt State aus Dateien (throttled, nicht jeden Frame).
pub fn load_state(_force: bool) -> Vec<AgentView> {
    let root = bot2bot_root();
    let pool_path = root.join("workers").join("pool_state.json");
    let now = SystemTime::now();

    let pool: PoolState = fs_read_json(&pool_path).unwrap_or_default();

    // Heartbeat-Directory
    let heartbeat_dir = root.join("workers");

    let mut agents: Vec<AgentView> = pool
        .entries
        .iter()
        .map(|(brain, entry)| {
            let hb_path = heartbeat_dir.join(format!("heartbeat_{}.json", brain));
            let heartbeat_age = hb_path
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|d| d.as_secs())
                .unwrap_or(u64::MAX);

            // Inbox-Zählen
            let inbox = root.join("agents").join(brain).join("inbox");
            let read_dir = inbox.join("_read");
            let pending = count_msgs(&inbox);
            let done = count_msgs(&read_dir);

            // Log-Zeile (letzte aus history.jsonl)
            let log = latest_log_line(&root, brain);

            AgentView {
                brain: brain.clone(),
                status: entry.status.clone(),
                pid: None, // pid kommt aus heartbeat_dir/process_map wenn nötig
                heartbeat_age_sec: heartbeat_age,
                tasks_pending: pending,
                tasks_done: done,
                last_log_line: log.clone(),
                last_response: None, // TODO: aus history.jsonl extrahieren
                detail: recent_log_lines(&root, brain, DETAIL_HISTORY),
            }
        })
        .collect();
    // Feste alphabetische Reihenfolge — sonst springt der Fokus bei jedem
    // Reload, weil die HashMap-Iteration nicht-deterministisch ist.
    overlay_bench_activity(&mut agents);
    agents.sort_by(|a, b| a.brain.cmp(&b.brain));
    agents
}

/// Sekunden seit Mitternacht aus einem `HH:MM:SS`-Stempel.
fn seconds_of_day(hhmmss: &str) -> Option<u64> {
    let mut it = hhmmss.split(':');
    let h: u64 = it.next()?.parse().ok()?;
    let m: u64 = it.next()?.parse().ok()?;
    let s: u64 = it.next()?.parse().ok()?;
    Some(h * 3600 + m * 60 + s)
}

/// Alter eines `HH:MM:SS`-Stempels in Sekunden gegen die Ortszeit.
/// Die Ereignisse tragen kein Datum — daher der Tagesumbruch.
fn age_of_stamp(hhmmss: &str) -> Option<u64> {
    let then = seconds_of_day(hhmmss)?;
    let now = seconds_of_day(&crate::timestamp())?;
    Some(if now >= then {
        now - then
    } else {
        now + 86_400 - then
    })
}

/// Blendet die Aktivitaet des laufenden Benchmarks ueber die Pool-Daten.
///
/// Der Benchmark fasst den bot2bot-Worker-Pool NIE an (`benchmark.rs` kennt
/// `worker_pool` nicht einmal). Ohne diesen Abgleich zeigt das Dashboard
/// waehrend eines Laufs nur Altbestand — Zeilen in `cooldown`, Heartbeats
/// stundenalt — obwohl die Benchmark-Ansicht daneben mitloggt (Beschwerde
/// 2026-07-26). Uebernommen wird nur ehrlich Ableitbares; `pid` bleibt
/// unbekannt, weil der Ereignisstrom keine Prozesse kennt.
fn overlay_bench_activity(agents: &mut Vec<AgentView>) {
    use std::collections::HashMap;
    let events = crate::bench_events::snapshot();
    if events.is_empty() {
        return;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut last: HashMap<String, (String, u64)> = HashMap::new();
    let mut passes: HashMap<String, usize> = HashMap::new();
    let mut detail: HashMap<String, Vec<String>> = HashMap::new();

    for ev in &events {
        let Some(brain) = ev.brain.as_deref() else {
            continue;
        };
        if !seen.iter().any(|b| b == brain) {
            seen.push(brain.to_string());
        }
        if let Some(age) = age_of_stamp(&ev.ts) {
            last.insert(brain.to_string(), (ev.text.clone(), age));
        }
        if ev.level == crate::bench_events::Level::Pass {
            *passes.entry(brain.to_string()).or_default() += 1;
        }
        let d = detail.entry(brain.to_string()).or_default();
        d.push(format!("{} {}", ev.ts, ev.text));
        if d.len() > DETAIL_HISTORY {
            d.remove(0);
        }
    }

    let apply = |a: &mut AgentView| {
        if let Some((text, age)) = last.get(&a.brain) {
            a.heartbeat_age_sec = *age;
            a.last_log_line = Some(text.clone());
            // Frisch gemeldet = arbeitet gerade. Der Pool-Status stammt aus
            // einem anderen Subsystem und waere hier irrefuehrend.
            a.status = if *age < 60 {
                "active".to_string()
            } else if *age < 600 {
                "available".to_string()
            } else {
                "cooldown".to_string()
            };
        }
        if let Some(n) = passes.get(&a.brain) {
            a.tasks_done = *n;
        }
        if let Some(d) = detail.get(&a.brain) {
            a.detail = d.clone();
        }
    };

    for a in agents.iter_mut() {
        apply(a);
    }
    // Brains ohne Pool-Eintrag ergaenzen — sonst fehlt genau das Brain, das
    // gerade arbeitet.
    for brain in seen {
        if agents.iter().any(|a| a.brain == brain) {
            continue;
        }
        let mut a = AgentView {
            brain,
            status: "available".to_string(),
            pid: None,
            heartbeat_age_sec: u64::MAX,
            tasks_pending: 0,
            tasks_done: 0,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        };
        apply(&mut a);
        agents.push(a);
    }
}

fn fs_read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn count_msgs(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .ok()
        .map(|e| {
            e.filter_map(|f| f.ok())
                .filter(|f| f.path().extension().is_some_and(|e| e == "txt"))
                .count()
        })
        .unwrap_or(0)
}

/// Die letzten `n` Verlaufseintraege eines Agenten als Klartextzeilen —
/// Rohmaterial fuer den Ausklapp-Blick (ungekuerzt; der Umbruch passiert erst
/// beim Rendern, wo die Breite bekannt ist).
fn recent_log_lines(root: &Path, brain: &str, n: usize) -> Vec<String> {
    let history = root.join("agents").join(brain).join("history.jsonl");
    let Ok(text) = std::fs::read_to_string(&history) else {
        return Vec::new();
    };
    let all: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    all.iter()
        .rev()
        .take(n)
        .rev()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| {
                    let kind = v.get("kind").and_then(|x| x.as_str()).unwrap_or("");
                    v.get("body")
                        .or_else(|| v.get("content"))
                        .and_then(|x| x.as_str())
                        .map(|b| {
                            if kind.is_empty() {
                                b.to_string()
                            } else {
                                format!("[{kind}] {b}")
                            }
                        })
                })
                .unwrap_or_else(|| (*l).to_string())
        })
        .collect()
}

fn latest_log_line(root: &Path, brain: &str) -> Option<String> {
    let history = root.join("agents").join(brain).join("history.jsonl");

    std::fs::read_to_string(&history)
        .ok()?
        .lines()
        .last()
        .map(|l| {
            // Versuche JSON zu parsen für "body" oder "content"
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| {
                    v.get("body")
                        .or_else(|| v.get("content"))
                        .and_then(|x| x.as_str().map(String::from))
                })
                .map(|s| s.chars().take(80).collect())
                .unwrap_or_else(|| l.chars().take(80).collect())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_aktivitaet_erreicht_das_worker_dashboard() {
        // Seriell gegen andere Tests am globalen Event-Bus (clear/emit sind
        // lockfrei): sonst raeumt ein paralleler Test unsere Ereignisse weg.
        let _guard = crate::bench_events::test_bus_mutex().lock();
        crate::bench_events::clear();
        let mut agents = vec![AgentView {
            brain: "deepseek".to_string(),
            status: "cooldown".to_string(),
            pid: None,
            heartbeat_age_sec: u64::MAX,
            tasks_pending: 0,
            tasks_done: 0,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        }];
        crate::bench_events::emit(
            crate::bench_events::Level::Pass,
            Some("deepseek"),
            "deepseek: Tests gruen",
        );
        crate::bench_events::emit(
            crate::bench_events::Level::Progress,
            Some("zai"),
            "zai: Iteration 1",
        );

        overlay_bench_activity(&mut agents);

        let ds = agents.iter().find(|a| a.brain == "deepseek").unwrap();
        assert_eq!(ds.status, "active");
        assert!(ds.heartbeat_age_sec < 60, "Heartbeat blieb alt");
        assert_eq!(ds.last_log_line.as_deref(), Some("deepseek: Tests gruen"));
        assert_eq!(ds.tasks_done, 1);
        assert!(!ds.detail.is_empty());
        assert!(
            agents.iter().any(|a| a.brain == "zai"),
            "arbeitendes Brain ohne Pool-Eintrag fehlt"
        );

        // Gegenprobe: ohne Ereignisse bleibt der Pool-Zustand unangetastet.
        crate::bench_events::clear();
        let mut u = vec![AgentView {
            brain: "kimi".to_string(),
            status: "cooldown".to_string(),
            pid: None,
            heartbeat_age_sec: 999,
            tasks_pending: 3,
            tasks_done: 7,
            last_log_line: None,
            last_response: None,
            detail: Vec::new(),
        }];
        overlay_bench_activity(&mut u);
        assert_eq!(u[0].status, "cooldown");
        assert_eq!(u[0].tasks_done, 7);
        assert_eq!(u.len(), 1);
        crate::bench_events::clear();
    }

    #[test]
    fn stempel_alter_ueberlebt_den_tageswechsel() {
        assert_eq!(seconds_of_day("01:02:03"), Some(3723));
        assert!(seconds_of_day("kaputt").is_none());
        assert!(age_of_stamp(&crate::timestamp()).unwrap() < 5);
    }
}
