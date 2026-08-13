//! Betrieb und Diagnose: Canary, Relay, Swarm, OOBE, Runs, Login, Doctor,
//! Watchdog, Wartungspruefung.

use std::collections::HashMap;
use webagent::run_store::RunStore;

/// Startup-Helfer: stale Runs reparieren (Python `main()` vor jedem Command
/// ausser maintenance-check).
pub fn startup_reconcile_runs() -> Vec<String> {
    let runs_dir = webagent::config::runs_dir();
    let store = RunStore::new(runs_dir.clone(), runs_dir.join("logs"));
    store.reconcile_stale_runs(600.0)
}

/// Manueller Rettungsweg `webagent sync-master`: spielt die neueste
/// Laufzeit-Kopie (`profiles/encapsulated/pool_*`) ins Master-Profil zurueck.
///
/// `write_back_session_to_master` haengt an `RUNTIME_POOL_PROFILE`, einem
/// prozess-lokalen OnceLock — in einem frischen CLI-Prozess ist der leer und
/// die Funktion tut nichts. Dieser Befehl findet die Kopie stattdessen auf der
/// Platte, waehlt die juengste mit Login-Artefakten und kopiert sparsam ins
/// Master (gleiche Schutzbedingung wie der Automatismus).
pub fn cmd_sync_master() -> i32 {
    use webagent::config::write_back_dir_to_master;
    let base = webagent::config::profiles_dir().join("encapsulated");
    let mut pools: Vec<std::path::PathBuf> = match std::fs::read_dir(&base) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("pool_"))
                        .unwrap_or(false)
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    if pools.is_empty() {
        println!("[sync-master] keine Laufzeit-Kopien unter {}", base.display());
        return 2;
    }
    // Juengste zuerst: die mtime der Kopie ist der letzte Browser-Schreibzugriff.
    pools.sort_by_key(|p| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    pools.reverse();
    for dir in pools {
        match write_back_dir_to_master(&dir) {
            Ok(()) => {
                println!(
                    "[sync-master] {} -> Master kopiert",
                    dir.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| dir.display().to_string())
                );
                return 0;
            }
            Err(e) => eprintln!("[sync-master] {}: {e}", dir.display()),
        }
    }
    println!("[sync-master] keine Laufzeit-Kopie mit Login-Artefakten gefunden");
    1
}

pub fn cmd_canary() -> i32 {
    let results = webagent::canary::run_canary();
    if results.is_empty() {
        println!("[canary] keine Brains registriert");
        return 2;
    }
    println!("[canary] {} Brains:", results.len());
    let mut fail = 0u32;
    for r in &results {
        let status = if r.ok { "ok" } else { "FAIL" };
        if !r.ok {
            fail += 1;
        }
        println!(
            "  {:<10} {status:<4}  latency_ms={}  reason={}",
            r.brain_id, r.latency_ms, r.reason
        );
    }
    if fail > 0 {
        println!("[canary] {fail}/{} failed", results.len());
        1
    } else {
        println!("[canary] all ok");
        0
    }
}

/// Zaehl-Spiel: Brains zaehlen abwechselnd bis `count`, Traces als JSON.
pub fn cmd_count(brains: Option<Vec<String>>, count: u32, headless: bool) -> i32 {
    let ids: Vec<String> = match brains {
        Some(b) => b,
        None => webagent::config::available_brain_ids(),
    };
    webagent::counting::counting_game(&ids, headless, count)
}

/// Ein Brain-Ergebnis fuer `--json` (relay + swarm).
#[derive(Debug, Clone, serde::Serialize)]
pub struct BrainIoResult {
    brain: String,
    ok: bool,
    answer: String,
    latency_ms: u64,
    reason: String,
}

pub fn brain_io_json(r: &BrainIoResult) -> String {
    serde_json::to_string(r).unwrap_or_else(|_| {
        format!(
            r#"{{"brain":"{}","ok":false,"answer":"","latency_ms":0,"reason":"json_serialize_failed"}}"#,
            r.brain
        )
    })
}

/// Misst je Brain, wie lange eine Eingabe sein darf, und schreibt das Ergebnis
/// nach `<data>/brain_limits.json`.
///
/// Erst verdoppeln bis zur ersten Ablehnung, dann zwischen letztem
/// angenommenen und erstem abgelehnten Wert schachteln. Die frueher genutzte
/// feste Leiter endete bei 100.000 und meldete deren Annahme als „Grenze" —
/// tatsaechlich war das nur die Obergrenze des Tests. Brains mit bereits
/// gemessenem Wert werden uebersprungen (ausser mit `--force`).
pub fn cmd_measure_limits(
    brains: &str,
    headless: bool,
    force: bool,
    start: usize,
    ceiling: usize,
    tolerance: usize,
) -> i32 {
    let liste: Vec<String> = if brains.trim().is_empty() {
        webagent::config::available_brain_ids()
    } else {
        brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let offen = if force {
        liste.clone()
    } else {
        webagent::brain_limits::unmeasured(&liste)
    };
    if offen.is_empty() {
        println!("[limits] alle {} Brains bereits gemessen", liste.len());
        for b in &liste {
            if let Some(n) = webagent::brain_limits::accepted_chars(b) {
                println!("  {b:<10} {n} Zeichen");
            }
        }
        return 0;
    }
    println!("[limits] zu messen: {}", offen.join(", "));

    let cfg = webagent::brain_limits::SearchConfig {
        start,
        ceiling,
        tolerance,
        ..Default::default()
    };

    let mut fehler = 0;
    for brain in &offen {
        let mut notiz = String::new();
        let ergebnis = webagent::brain_limits::search_limit(&cfg, |groesse| {
            let fuellung = "x".repeat(groesse.saturating_sub(200));
            let probe = format!(
                "Antworte AUSSCHLIESSLICH mit dem Wort OK. Ignoriere den Fuelltext.\n\n{fuellung}"
            );
            print!("  {brain:<10} {groesse:>8} Zeichen … ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            match webagent::relay::relay_single_turn(brain, &probe, headless, Some(180.0), None) {
                Ok(antwort) => {
                    if webagent::brain_limits::looks_like_length_rejection(&antwort) {
                        println!("abgelehnt");
                        notiz = webagent::char_prefix(antwort.trim(), 120).to_string();
                        webagent::brain_limits::ProbeOutcome::Rejected
                    } else {
                        println!("angenommen");
                        webagent::brain_limits::ProbeOutcome::Accepted
                    }
                }
                Err(e) => {
                    let text = e.to_string();
                    println!("Fehler: {}", webagent::char_prefix(&text, 80));
                    // Eine Laengenablehnung muss kein Text sein: gemessen am
                    // 05.08.2026 stand bei claude und mistral ab 400.000
                    // Zeichen die Eingabe VOLLSTAENDIG im Composer, es gab
                    // keinen Dialog und keine Meldung — die Oberflaeche
                    // deaktivierte nur den Absendeknopf. Wer bloss auf Texte
                    // schaut, verbucht das als Harness-Fehler und sucht ewig
                    // weiter nach einer Grenze, die er gerade beruehrt hat.
                    if webagent::browser::is_send_disabled_error(&text) {
                        notiz = "Absendeknopf deaktiviert (Ablehnung ohne Meldung)".to_string();
                        webagent::brain_limits::ProbeOutcome::Rejected
                    } else if webagent::brain_limits::looks_like_length_rejection(&text) {
                        notiz = webagent::char_prefix(&text, 120).to_string();
                        webagent::brain_limits::ProbeOutcome::Rejected
                    } else {
                        // Kein Laengenproblem (Login, Kontingent, Netz).
                        webagent::brain_limits::ProbeOutcome::Aborted(text)
                    }
                }
            }
        });

        if let Some(grund) = &ergebnis.aborted {
            eprintln!(
                "  {brain}: abgebrochen, kein Laengenproblem: {}",
                webagent::char_prefix(grund, 100)
            );
        }

        match ergebnis.accepted {
            Some(n) => {
                if ergebnis.rejected.is_none() {
                    notiz = format!(
                        "nie abgelehnt bis {n} — untere Schranke, keine Grenze (Decke {ceiling})"
                    );
                    println!("  {brain:<10} bis {n} angenommen, NIE abgelehnt (untere Schranke)");
                } else {
                    println!(
                        "  {brain:<10} Grenze zwischen {n} und {}",
                        ergebnis.rejected.unwrap_or(0)
                    );
                }
                let eintrag = webagent::brain_limits::BrainLimit {
                    accepted_chars: n,
                    rejected_chars: ergebnis.rejected,
                    measured_at: webagent::now_rfc3339(),
                    note: notiz,
                };
                if let Err(e) = webagent::brain_limits::record(brain, eintrag) {
                    eprintln!("  {brain}: nicht speicherbar: {e}");
                    fehler += 1;
                }
            }
            None => {
                eprintln!("  {brain}: kein Wert ermittelt (nicht erreichbar oder alles abgelehnt)");
                fehler += 1;
            }
        }
    }
    if fehler > 0 {
        1
    } else {
        0
    }
}

pub fn cmd_relay(
    brain: &str,
    message: &str,
    message_file: Option<&str>,
    headless: bool,
    timeout: f64,
    json: bool,
    model: Option<&str>,
) -> i32 {
    let text = match message_file {
        Some(p) => match std::fs::read_to_string(p) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[relay] {p} nicht lesbar: {e}");
                return 2;
            }
        },
        None => message.to_string(),
    };
    if text.trim().is_empty() {
        eprintln!("[relay] leere Nachricht — --message oder --message-file angeben");
        return 2;
    }
    let message = text.as_str();
    let to = if timeout > 0.0 { Some(timeout) } else { None };
    let started = std::time::Instant::now();
    match webagent::relay::relay_single_turn(brain, message, headless, to, model) {
        Ok(reply) => {
            let r = BrainIoResult {
                brain: brain.to_string(),
                ok: true,
                answer: reply.clone(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            };
            if json {
                println!("{}", brain_io_json(&r));
            } else {
                println!("{reply}");
            }
            0
        }
        Err(e) => {
            let r = BrainIoResult {
                brain: brain.to_string(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            };
            if json {
                println!("{}", brain_io_json(&r));
            } else {
                eprintln!("[relay] error: {e}");
            }
            1
        }
    }
}

pub fn cmd_swarm(message: &str, headless: bool, timeout: f64, brains: &str, json: bool) -> i32 {
    let to = if timeout > 0.0 { Some(timeout) } else { None };
    let targets: Vec<String> = {
        let listed: Vec<String> = brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if listed.is_empty() {
            webagent::config::available_brain_ids()
        } else {
            listed
        }
    };
    if targets.is_empty() {
        if json {
            println!(r#"{{"brains":[],"synthesis":null,"error":"no_brains"}}"#);
        } else {
            eprintln!("[swarm] keine Brains");
        }
        return 2;
    }

    if !json {
        println!("[swarm] Phase 1 — {} Brains…", targets.len());
    }

    let mut results: Vec<BrainIoResult> = Vec::new();
    for brain in &targets {
        let started = std::time::Instant::now();
        let r = match webagent::relay::relay_single_turn(brain, message, headless, to, None) {
            Ok(answer) => BrainIoResult {
                brain: brain.clone(),
                ok: true,
                answer,
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            },
            Err(e) => BrainIoResult {
                brain: brain.clone(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            },
        };
        if !json {
            let status = if r.ok { "ok" } else { "FAIL" };
            let preview: String = r.answer.chars().take(160).collect();
            println!(
                "  {:<10} {status:<4}  {}ms  {}{}",
                r.brain,
                r.latency_ms,
                if r.ok { preview } else { r.reason.clone() },
                if r.ok && r.answer.chars().count() > 160 {
                    "…"
                } else {
                    ""
                }
            );
        }
        results.push(r);
    }

    let ok_brains: Vec<&BrainIoResult> = results.iter().filter(|r| r.ok).collect();
    let synthesis = if ok_brains.is_empty() {
        None
    } else if ok_brains.len() == 1 {
        Some(ok_brains[0].clone())
    } else {
        // Reliability-Orch (wie REPL-Default), Fallback: erstes ok
        let board = webagent::brain_score::leaderboard();
        let score_of = |id: &str| -> f64 {
            board
                .iter()
                .find(|s| s.brain_id == id)
                .map(|s| s.reliability)
                .unwrap_or(0.0)
        };
        let orch = ok_brains
            .iter()
            .max_by(|a, b| {
                score_of(&a.brain)
                    .partial_cmp(&score_of(&b.brain))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|r| r.brain.as_str())
            .unwrap_or(ok_brains[0].brain.as_str());

        let joined: String = ok_brains
            .iter()
            .map(|r| format!("### {}\n{}", r.brain, r.answer))
            .collect::<Vec<_>>()
            .join("\n\n");
        let synth_prompt = format!(
            "Aufgabe: «{message}».\n\nDie beteiligten Modelle haben so geantwortet:\n\n{joined}\n\n\
             Führe diese Antworten zu einer einzigen, besten finalen Antwort zusammen. \
             Nenne Widersprüche, wenn es welche gibt. Du bist der Orchestrator ({orch})."
        );
        if !json {
            println!("[swarm] Phase 2/3 — Synthese via {orch}…");
        }
        let started = std::time::Instant::now();
        match webagent::relay::relay_single_turn(orch, &synth_prompt, headless, to, None) {
            Ok(answer) => Some(BrainIoResult {
                brain: orch.to_string(),
                ok: true,
                answer,
                latency_ms: started.elapsed().as_millis() as u64,
                reason: "ok".into(),
            }),
            Err(e) => Some(BrainIoResult {
                brain: orch.to_string(),
                ok: false,
                answer: String::new(),
                latency_ms: started.elapsed().as_millis() as u64,
                reason: e.to_string(),
            }),
        }
    };

    if json {
        let payload = serde_json::json!({
            "brains": results,
            "synthesis": synthesis,
        });
        match serde_json::to_string(&payload) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("[swarm] json error: {e}");
                return 1;
            }
        }
    } else if let Some(s) = &synthesis {
        if s.ok {
            println!("\n[swarm ⇒ final via {}]\n{}\n", s.brain, s.answer);
        } else {
            println!("[swarm] Synthese fehlgeschlagen: {}", s.reason);
        }
    } else {
        println!("[swarm] Keine Antworten — Abbruch.");
    }

    let any_ok = results.iter().any(|r| r.ok);
    if any_ok {
        0
    } else {
        1
    }
}

pub fn cmd_oobe(brains: &str, skip_login: bool, yes: bool) -> i32 {
    match webagent::oobe::run_oobe_wizard(!yes, skip_login, brains, yes) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("[oobe] {e}");
            2
        }
    }
}

pub fn cmd_run(
    brain: &str,
    task: &str,
    resume: Option<&str>,
    headless: bool,
    max_cycles: u32,
    no_memory: bool,
) -> i32 {
    use webagent::browser::WebBrainBackend;
    use webagent::controller::{AgentController, RunOptions};
    use webagent::executor::PlatformShellExecutor;

    let backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[run] {e}");
            return 2;
        }
    };
    let executor = PlatformShellExecutor::new();
    let mut controller = AgentController::new(backend, executor, max_cycles as usize);

    // Fortschritt auf stdout — auf stderr rendern PowerShell-Wrapper das als
    // roten NativeCommandError-Block, obwohl nichts kaputt ist (Dogfood-Fund 2026-07-20).
    println!(
        "[run] brain={} headless={} max_cycles={} — starte Embedded WebView…",
        brain, headless, max_cycles
    );
    let opts = RunOptions {
        suppress_memory_context: no_memory,
        ..RunOptions::default()
    };
    let result = match resume {
        Some(run_id) => controller.continue_run(
            run_id,
            task,
            brain,
            headless,
            opts,
        ),
        None => controller.run_with_options(task, brain, None, headless, opts),
    };
    match result {
        Ok(meta) => {
            println!(
                "[run] status={} run_id={} cycles={}",
                meta.status, meta.run_id, meta.cycles
            );
            if meta.status == "done" {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("[run] Fehler: {e}");
            1
        }
    }
}

pub fn cmd_login_all(timeout_secs: u64, force: bool, parallel: usize) -> i32 {
    use std::time::Duration;

    let parallel = if parallel > 3 {
        eprintln!("[login-all] --parallel {parallel} gedeckelt auf 3");
        3
    } else {
        parallel
    };
    if parallel == 0 {
        eprintln!("[login-all] sequenziell, {timeout_secs}s pro Brain (profiles/<brain>)…");
    } else {
        eprintln!("[login-all] parallel={parallel} (experimentell), {timeout_secs}s pro Brain…");
    }
    let results = webagent::login::login_all(Duration::from_secs(timeout_secs), parallel, force);
    let mut fail = 0usize;
    for r in &results {
        let tag = if r.skipped {
            "skip"
        } else if r.ok {
            "ok"
        } else {
            fail += 1;
            "FAIL"
        };
        println!("[login-all] [{tag}] {}: {}", r.brain_id, r.message);
    }
    println!(
        "[login-all] fertig: {}/{} ok, {} übersprungen, {} fail",
        results.iter().filter(|r| r.ok).count(),
        results.len(),
        results.iter().filter(|r| r.skipped).count(),
        fail
    );
    if fail > 0 {
        1
    } else {
        0
    }
}

pub fn cmd_login(brain: &str, timeout_secs: u64, force: bool, auto: bool) -> i32 {
    use std::time::Duration;
    use webagent::browser::WebBrainBackend;

    // Im Shared-Betrieb loggt auch der Einzel-Login direkt ins read-only
    // Master-Hauptprofil (profiles/shared) statt nach profiles/<brain>.
    let shared_mode = webagent::config::use_shared_browser();
    if shared_mode {
        webagent::config::unseal_master_profile();
    }
    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[login] {e}");
            if shared_mode {
                webagent::config::seal_master_profile();
            }
            return 2;
        }
    };
    if shared_mode {
        backend = backend.with_profile_override(webagent::config::shared_profile_dir());
    }
    let timeout = Duration::from_secs(timeout_secs);
    let code = if force {
        eprintln!(
            "[login] {brain}: --force — Fenster bleibt {timeout_secs}s offen, unabhaengig vom Login-Check. \
             Fenster schliessen, sobald du fertig bist."
        );
        match backend.hold_window_open(timeout) {
            Ok(()) => {
                println!("[login] {brain}: Fenster geschlossen, Profil geschrieben.");
                webagent::login::clear_breaker(brain);
                0
            }
            Err(e) => {
                eprintln!("[login] {brain}: Fehler: {e}");
                1
            }
        }
    } else {
        if auto {
            eprintln!(
                "[login] {brain}: --auto — klicke Login-Kette selbst durch (Anmelden → ggf. Google-SSO)."
            );
        }
        match if auto {
            backend.try_auto_login(timeout)
        } else {
            backend.interactive_login(timeout)
        } {
            Ok(true) => {
                println!("[login] {brain}: Login erkannt und Session gespeichert.");
                webagent::login::clear_breaker(brain);
                0
            }
            Ok(false) => {
                eprintln!(
                    "[login] {brain}: kein Login innerhalb von {timeout_secs}s erkannt. Erneut versuchen mit --timeout \
                     oder --force (Erkennung ist bei manchen Brains zu optimistisch)."
                );
                1
            }
            Err(e) => {
                eprintln!("[login] {brain}: Fehler: {e}");
                1
            }
        }
    };
    if shared_mode {
        drop(backend);
        webagent::config::seal_master_profile();
    }
    code
}

pub fn cmd_diagnose(brain: &str, headless: bool) -> i32 {
    use webagent::browser::WebBrainBackend;

    let mut backend = match WebBrainBackend::from_config(brain) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[diagnose] {e}");
            return 2;
        }
    };
    eprintln!("[diagnose] {brain}: starte Browser (headless={headless})…");
    match backend.live_diagnose(headless) {
        Ok(d) => {
            let ok = |b: bool| if b { "ok" } else { "FEHLT" };
            println!("[diagnose] {}", d.brain_id);
            println!("    session_state:  {:?}", d.session_state);
            println!("    logged_in:      {}", d.logged_in);
            println!("    login_button:   {}", d.login_button_visible);
            println!("    composer:       {}", ok(d.composer_found));
            println!("    assistant_msgs: {}", d.assistant_count);
            println!("    cloudflare:     {}", d.cloudflare);
            println!("    url:            {}", d.url);
            // Healthy = eingeloggt, Composer da, keine Cloudflare-Blockade.
            if d.logged_in && d.composer_found && !d.cloudflare {
                0
            } else {
                1
            }
        }
        Err(e) => {
            eprintln!("[diagnose] {brain}: Fehler: {e}");
            1
        }
    }
}

pub fn cmd_doctor(brain_ids: Option<Vec<String>>, json: bool) -> i32 {
    // Konfiguration aus config.rs laden
    let brains_config = webagent::config::brains();

    // Runs-Verzeichnis aus config.rs
    let runs_dir = webagent::config::runs_dir().to_string_lossy().to_string();

    let report = webagent::doctor::run_doctor(
        brain_ids,
        Some(&brains_config),
        &runs_dir,
        None, // list_runs_fn
        None, // load_fn
    );

    if json {
        // JSON-Ausgabe
        match serde_json::to_string_pretty(&serde_json::json!({
            "ok": report.ok(),
            "timestamp": report.timestamp,
            "healthy": report.healthy_brain_ids(),
            "unhealthy": report.unhealthy_brain_ids(),
            "brains": report.brains.iter().map(|(id, check)| {
                (id, serde_json::json!({
                    "healthy": check.healthy(),
                    "selectors_ok": check.selectors_ok,
                    "selectors_path": check.selectors_path,
                    "selectors_mtime": check.selectors_mtime,
                    "profile_exists": check.profile_exists,
                    "profile_dir": check.profile_dir,
                    "profile_lock_files": check.profile_lock_files,
                    "last_done_run": check.last_done_run,
                    "last_done_run_age_hours": check.last_done_run_age_hours,
                    "login_state": check.login_state,
                    "recovery_hint": check.recovery_hint,
                }))
            }).collect::<HashMap<_, _>>(),
        })) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("[doctor] JSON-Serialisierung fehlgeschlagen: {}", e);
                return 1;
            }
        }
    } else {
        // Menschenlesbare Ausgabe
        println!("[doctor] {}", report.timestamp);
        println!(
            "[doctor] healthy: {}",
            if report.healthy_brain_ids().is_empty() {
                "keine".to_string()
            } else {
                report.healthy_brain_ids().join(", ")
            }
        );
        if !report.unhealthy_brain_ids().is_empty() {
            println!(
                "[doctor] unhealthy: {}",
                report.unhealthy_brain_ids().join(", ")
            );
        }
        println!();

        let mut brain_ids: Vec<_> = report.brains.keys().collect();
        brain_ids.sort();

        for brain_id in brain_ids {
            let check = &report.brains[brain_id];
            let status_icon = if check.healthy() { "ok" } else { "PROBLEM" };
            println!("  [{}] {}", status_icon, brain_id);
            println!(
                "    selectors:  {} ({})",
                if check.selectors_ok { "ok" } else { "FEHLT" },
                check.selectors_path
            );
            println!(
                "    selectors:  mtime={}",
                if check.selectors_mtime.is_empty() {
                    "n/a"
                } else {
                    &check.selectors_mtime
                }
            );
            println!(
                "    profile:    {} ({})",
                if check.profile_exists { "ok" } else { "FEHLT" },
                check.profile_dir
            );
            if !check.profile_lock_files.is_empty() {
                println!("    locks:      {}", check.profile_lock_files.join(", "));
            }
            if !check.last_done_run.is_empty() {
                let age = check.last_done_run_age_hours;
                let age_str = if age >= 0.0 {
                    format!("{:.0}h", age)
                } else {
                    "unbekannt".to_string()
                };
                println!("    last_run:   {} ({})", check.last_done_run, age_str);
            } else {
                println!("    last_run:   keiner");
            }
            println!("    login_state: {}", check.login_state);
            if !check.recovery_hint.is_empty() {
                println!("    recovery:   {}", check.recovery_hint);
            }
            println!();
        }
    }

    if report.ok() {
        0
    } else {
        2
    }
}

pub fn cmd_watchdog(
    bot2bot_root: Option<String>,
    profile_dir: Option<String>,
    runs_dir: Option<String>,
    repair: bool,
    json: bool,
) -> i32 {
    use webagent::config;
    use webagent::run_store::RunStore;
    use webagent::watchdog;

    let bot2bot_root =
        bot2bot_root.unwrap_or_else(|| config::bot2bot_root().to_string_lossy().to_string());
    let profile_dir = profile_dir.unwrap_or_else(|| {
        config::profiles_dir()
            .join("shared")
            .to_string_lossy()
            .to_string()
    });
    let runs_dir = runs_dir.unwrap_or_else(|| config::runs_dir().to_string_lossy().to_string());

    let store = RunStore::new(config::runs_dir(), config::runs_dir().join("logs"));

    let report = watchdog::run_watchdog(
        &bot2bot_root,
        &profile_dir,
        &runs_dir,
        Some(&store),
        repair,
        None,
    );

    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!("[watchdog] JSON-Serialisierung fehlgeschlagen: {}", e);
                return 1;
            }
        }
    } else {
        println!("[watchdog] {}", report.timestamp);
        println!("[watchdog] orphaned_runs: {}", report.orphaned_runs.len());
        println!(
            "[watchdog] stale_bridge_locks: {}",
            report.stale_bridge_locks.len()
        );
        println!(
            "[watchdog] stale_profile_locks: {}",
            report.stale_profile_locks.len()
        );
        if repair {
            println!("[watchdog] repaired_runs: {}", report.repaired_runs.len());
            println!(
                "[watchdog] repaired_bridge_locks: {}",
                report.repaired_bridge_locks.len()
            );
            println!(
                "[watchdog] repaired_profile_locks: {}",
                report.repaired_profile_locks.len()
            );
        }
        if !report.errors.is_empty() {
            println!("[watchdog] errors: {}", report.errors.join(", "));
        }
        println!();
    }

    if (report.ok() && report.errors.is_empty()) || (repair && report.total_repaired() > 0) {
        0
    } else {
        2
    }
}

/// Führt einen Unterprozess aus und bricht nach `timeout_secs` ab.
/// Gibt `Some(true/false)` bei regulärem Exit zurück, `None` bei Timeout/Fehler.
pub fn run_command_with_timeout(cmd: &str, args: &[&str], timeout_secs: f64) -> Option<bool> {
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    let mut child = match Command::new(cmd)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return None,
    };

    let start = Instant::now();
    let limit = Duration::from_secs_f64(timeout_secs.max(1.0));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) => {
                if start.elapsed() >= limit {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => return None,
        }
    }
}

/// Bündelt die Read-only-Gates: doctor + watchdog (dry-run) + optional Test-Suite.
pub fn maintenance_healthy(do_pytest: bool, pytest_timeout: f64) -> bool {
    // 1) Doctor: alle konfigurierten Brains gesund?
    let brains_config = webagent::config::brains();
    let runs_dir = webagent::config::runs_dir().to_string_lossy().to_string();
    let doctor_report =
        webagent::doctor::run_doctor(None, Some(&brains_config), &runs_dir, None, None);
    if !doctor_report.ok() {
        return false;
    }

    // 2) Watchdog dry-run: keine Funde/Fehler?
    let store = RunStore::new(
        webagent::config::runs_dir(),
        webagent::config::runs_dir().join("logs"),
    );
    let wd = webagent::watchdog::run_watchdog(
        webagent::config::bot2bot_root().to_string_lossy().as_ref(),
        webagent::config::profiles_dir()
            .join("shared")
            .to_string_lossy()
            .as_ref(),
        &runs_dir,
        Some(&store),
        false,
        None,
    );
    if !wd.ok() || !wd.errors.is_empty() {
        return false;
    }

    // 3) Optional: Test-Suite
    if do_pytest {
        match run_command_with_timeout("cargo", &["test", "--quiet"], pytest_timeout) {
            Some(true) => {}
            _ => return false,
        }
    }

    true
}

pub fn cmd_maintenance_check(json: bool, pytest: bool, pytest_timeout: f64) -> i32 {
    let healthy = maintenance_healthy(pytest, pytest_timeout);

    if json {
        match serde_json::to_string_pretty(&serde_json::json!({
            "healthy": healthy,
            "pytest": pytest,
        })) {
            Ok(output) => println!("{}", output),
            Err(e) => {
                eprintln!(
                    "[maintenance-check] JSON-Serialisierung fehlgeschlagen: {}",
                    e
                );
                return 1;
            }
        }
    } else {
        println!("[maintenance-check] healthy={}", healthy);
        if pytest {
            println!("[maintenance-check] pytest={}", pytest);
        }
    }

    if healthy {
        0
    } else {
        2
    }
}

/// Verifikationslauf `webagent verify` (Phase 6 des Capability-Proof-Plans).
///
/// Faehrt fuer jede Brain die ausgewaehlten Faehigkeiten in EINER Sitzung,
/// misst sie real im Browser und schreibt je Befund einen Record in
/// `proofs.jsonl`. Die eigentliche Messlogik lebt in
/// `webagent::browser::verify`; hier passiert nur Argumentaufloesung, Loop,
/// Ausgabe und Exit-Code. Belege werden IMMER geschrieben (auch Unreachable),
/// aber es gibt keine Writes an `brain_score` oder `circuit_breaker` — der
/// Befund ist ein Klartext-Zeugnis, kein Level.
///
/// Exit-Code 0, wenn der Lauf durchlief (auch wenn einzelne Faehigkeiten
/// Unreachable sind); != 0 nur bei Konfigurations-/Store-Fehlern.
pub fn cmd_verify(brain_ids: Option<Vec<String>>, caps: Vec<String>, headless: bool) -> i32 {
    use std::io::Write;
    use webagent::browser::verify::{probe_message, resolve_verify_targets, verify_capabilities, verify_records};
    use webagent::capability_proof::{record_proof, ProofOutcome};

    const PREFLIGHT_SECS: f64 = 15.0;
    let mut out = std::io::stdout();

    let (targets, warnings) = match resolve_verify_targets(&caps) {
        Ok(x) => x,
        Err(msg) => {
            eprintln!("[verify] {msg}");
            return 1;
        }
    };
    for w in &warnings {
        eprintln!("[verify] Warnung: {w}");
    }
    if targets.is_empty() {
        eprintln!("[verify] keine fahrbaren Faehigkeiten ausgewaehlt");
        return 1;
    }
    let ids: Vec<String> = match brain_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => webagent::config::available_brain_ids(),
    };
    if ids.is_empty() {
        eprintln!("[verify] keine Brains registriert");
        return 1;
    }
    let keys: Vec<&str> = targets.iter().map(|c| c.key).collect();
    println!(
        "[verify] {} Brain(s), {} Faehigkeit(en): {}",
        ids.len(),
        keys.len(),
        keys.join(", ")
    );
    if caps.iter().any(|c| c == "stop_generation")
        && !(caps.iter().any(|c| c == "chat") && caps.iter().any(|c| c == "new_chat"))
    {
        println!("[verify] stop_generation laeuft in der Generation-Sequenz: chat und new_chat werden mitbelegt");
    }
    out.flush().ok();
    let probe = probe_message(&webagent::now_rfc3339());
    let mut exit = 0;
    let mut measured = 0usize;
    let mut unreachable = 0usize;
    for id in &ids {
        println!("[verify] {id}: Start");
        out.flush().ok();
        let mut backend = match webagent::browser::WebBrainBackend::from_config(id) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[verify] {id}: Backend-Konfiguration fehlt: {e}");
                exit = 1;
                continue;
            }
        };
        let results =
            verify_capabilities(&mut backend, headless, &targets, &probe, PREFLIGHT_SECS);
        for rec in verify_records(id, &results) {
            record_proof(rec);
        }
        for r in &results {
            match r.outcome {
                ProofOutcome::Unreachable => unreachable += 1,
                _ => measured += 1,
            }
            let status = match r.outcome {
                ProofOutcome::Passed => "Passed",
                ProofOutcome::Failed => "Failed",
                ProofOutcome::Unreachable => "Unreachable",
            };
            println!(
                "[verify] {id}: {} = {status} ({}ms) — {}",
                r.measurement.capability_key, r.latency_ms, r.measurement.note
            );
            out.flush().ok();
        }
        out.flush().ok();
    }

    // Ein Lauf, der ausschliesslich `Unreachable` liefert, hat NICHTS gemessen.
    // `Unreachable` entzieht bewusst keinen Beleg (§6) — deshalb sieht so ein
    // Lauf im Store unauffaellig aus, und am 2026-08-09 waren 128 von 195
    // Eintraegen `start_failed`. Ohne diese Meldung sieht "fertig" aus wie
    // "geprueft", also genau die Selbsttaeuschung, gegen die das Feature gebaut
    // ist. Deshalb laut und mit Exitcode != 0.
    if measured == 0 && unreachable > 0 {
        eprintln!();
        eprintln!(
            "[verify] KEIN EINZIGER BELEG: alle {unreachable} Pruefungen endeten \
             'Unreachable' — dieser Lauf hat nichts gemessen."
        );
        eprintln!(
            "[verify] Bestehende Belege sind unveraendert (Unreachable entzieht nie). \
             Ursache pruefen: Login, Cloudflare, Browserstart, offener Circuit-Breaker."
        );
        exit = 1;
    }

    exit
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_reconcile_runs_does_not_panic() {
        let repaired = startup_reconcile_runs();
        let _ = repaired;
    }

    #[test]
    fn test_maintenance_healthy_does_not_panic() {
        // Übt den Gate-Pfad (doctor + watchdog) ohne pytest aus.
        // Assertiert primär, dass die Funktion ohne Panic ein bool liefert.
        let result = maintenance_healthy(false, 60.0);
        assert!(result || !result);
    }
}
