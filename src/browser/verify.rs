//! Capability-Verifikation in EINER Browsersitzung (§8 des Capability-Proof-Plans).
//!
//! Strategie: chat, stop_generation und new_chat teilen sich einen Lauf — der
//! Stop-Button-Beweis entsteht mitten in der Probe, die ohnehin läuft, statt
//! für jede Fähigkeit einen eigenen Browserstart zu fahren. Toggles/Menüs
//! laufen zuerst (nebeneffektarm), die Generierungs-Sequenz danach, Navigation
//! (`open_section`) zuletzt (teuerster Eingriff).
//!
//! Die INNERE `operations::verify_surface` wird aufgerufen, nicht der Wrapper
//! `WebBrainBackend::verify_surface` — der macht start/stop pro Aufruf und
//! wäre genau die Browserstart-Verschwendung, die der Plan vermeiden will.
//! Dieses Modul ist ein Kindmodul von `browser` wie `ui.rs` und erreicht so
//! die privaten Interna (`driver`, `sel`, `eval*`, `probe_generation`) ohne
//! Sichtbarkeitsänderung.
//!
//! Die Rückgabe ist bewusst NICHT der Store: dieses Modul misst und urteilt,
//! das Aufschreiben übernimmt der Aufrufer (`capability_proof::record_measurement`).

use std::time::{Duration, Instant};

use serde_json::Value;

use super::js;
use super::operations;
use super::selectors::Selectors;
use super::{SessionState, WebBrainBackend};
use crate::brain::BrainBackend;
use crate::capability::{Capability, ProofKind};
use crate::capability_proof::{Measurement, ProofOutcome};

/// Poll-Intervall der Generierungs-Probe (ein CDP-Roundtrip pro Poll).
const POLL_INTERVAL_MS: u64 = 300;
/// Generierung gilt als beendet, wenn der Text so lange unverändert blieb und
/// kein Stop-Button kam — ohne dieses Fenster würde ein toter Stop-Selektor
/// den vollen `wait_response`-Timeout verbrennen.
const STABLE_DONE_SECS: u64 = 5;
/// Alle so viele Polls wird der Block-Banner geprüft (nicht jeder Poll — der
/// Banner-Scan ist teurer als die Zähl-Probe).
const BLOCK_POLL_EVERY: u32 = 7;

/// Ergebnis EINER gemessenen Fähigkeit — so, wie der Store es braucht.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub measurement: Measurement,
    pub outcome: ProofOutcome,
    pub hash: u32,
    pub latency_ms: u64,
}

impl VerifyResult {
    fn new(m: Measurement, outcome: ProofOutcome, hash: u32, start: Instant) -> Self {
        Self {
            measurement: m,
            outcome,
            hash,
            latency_ms: start.elapsed().as_millis() as u64,
        }
    }
}

/// Probe-Prompt (§5): klebt einen eindeutigen Stempel in die Frage.
pub fn probe_message(stamp: &str) -> String {
    format!("webagent capability probe {stamp} — zaehle langsam von 1 bis 100, je Zahl eine Zeile.")
}

/// Baut die Verify-Zielliste aus CLI-Argumenten: leer = ganzer Katalog,
/// sonst die angefragten Fähigkeiten in Katalogreihenfolge. `stop_generation`
/// zieht die Generation-Sequenz mit (`chat`, `new_chat` — §5). Nicht fahrbare
/// (`driveable: false`) oder nicht erreichbare (`attainable: false`) Einträge
/// werden mit Warnung übersprungen; unbekannte Schlüssel sind ein Fehler, der
/// die gültigen Schlüssel auflistet. Reine Funktion — kein Browser, kein Store.
pub fn resolve_verify_targets(
    caps: &[String],
) -> Result<(Vec<&'static Capability>, Vec<String>), String> {
    let all = crate::capability::CATALOG;
    if caps.is_empty() {
        return Ok((all.iter().collect(), Vec::new()));
    }
    let valid: Vec<&str> = all.iter().map(|c| c.key).collect();
    let mut keys: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for c in caps {
        let cap = crate::capability::capability(c).ok_or_else(|| {
            format!(
                "unbekannte Faehigkeit '{c}' — gueltig: {}",
                valid.join(", ")
            )
        })?;
        if !cap.driveable || !cap.attainable {
            warnings.push(format!(
                "{c}: uebersprungen (driveable: {}, attainable: {})",
                cap.driveable, cap.attainable
            ));
            continue;
        }
        if !keys.iter().any(|k| k == c) {
            keys.push(c.clone());
        }
    }
    // `stop_generation` hat keinen eigenen Lauf — die Generation-Sequenz
    // belegt `chat` und `new_chat` mit, die dabei real gemessen werden (§5).
    if keys.iter().any(|k| k == "stop_generation") {
        for dep in ["chat", "new_chat"] {
            if !keys.iter().any(|k| k == dep) {
                keys.push(dep.to_string());
            }
        }
    }
    let targets: Vec<&'static Capability> = all
        .iter()
        .filter(|c| keys.iter().any(|k| k == c.key))
        .collect();
    Ok((targets, warnings))
}

/// Reine Umwandlung: `VerifyResult`-Befunde → Store-Records, 1:1 — inklusive
/// `Unreachable`, die `proof_state` zwar ignoriert, die aber geschrieben
/// werden, damit die Drift-Historie vollständig bleibt.
pub fn verify_records(brain_id: &str, results: &[VerifyResult]) -> Vec<crate::capability_proof::ProofRecord> {
    results
        .iter()
        .map(|r| {
            crate::capability_proof::record_from_measurement(
                brain_id,
                &r.measurement,
                r.outcome,
                r.hash,
                r.latency_ms,
            )
        })
        .collect()
}

/// Verifiziert `targets` in EINER Sitzung (§8 Ablauf):
///
/// 1. `circuit_breaker::check` (read-only) — offen → alle Unreachable
///    `circuit_open`, ohne Sitzung.
/// 2. Sitzung starten + `ensure_ready` als Vorlauf — blockiert → alle
///    Unreachable mit festem Vokabular (`cloudflare`, `logged_out`,
///    `start_failed`), dann fertig (kein zweiter Lauf).
/// 3. `verify_session` (Toggles/Menüs → Generation → Navigation).
///
/// Ist bereits ein Driver angehängt (Mock-Tests), werden `start`/`stop`
/// übersprungen — `start` würde den angehängten Mock sonst ersetzen.
///
/// `preflight_secs` ist die Wartezeit des `ensure_ready`-Vorlaufs; Tests
/// kürzen sie, der echte Aufruf nutzt 15 s.
pub fn verify_capabilities(
    backend: &mut WebBrainBackend,
    headless: bool,
    targets: &[&Capability],
    probe: &str,
    preflight_secs: f64,
) -> Vec<VerifyResult> {
    if let Some(_secs) = crate::circuit_breaker::check(&backend.brain_id) {
        return all_unreachable(backend, targets, "circuit_open");
    }
    let driver_attached = backend.driver.borrow().is_some();
    if !driver_attached {
        if let Err(_e) = backend.start(headless) {
            return all_unreachable(backend, targets, "start_failed");
        }
    }
    backend.dismiss_consent();
    let state = backend.ensure_ready(preflight_secs).unwrap_or(SessionState::Error);
    let results = match state {
        SessionState::Ready => verify_session(backend, targets, probe),
        SessionState::Cloudflare => all_unreachable(backend, targets, "cloudflare"),
        SessionState::LoginRequired | SessionState::Unbestimmt => {
            all_unreachable(backend, targets, "logged_out")
        }
        SessionState::Error => all_unreachable(backend, targets, "start_failed"),
    };
    if !driver_attached {
        let _ = backend.stop();
    }
    results
}

/// Reihenfolge im Brain: Toggles/Menüs → Generation → Navigation. Nicht
/// fahrbare oder nicht erreichbare Fähigkeiten bleiben Quest (kein Record).
fn verify_session(
    backend: &mut WebBrainBackend,
    targets: &[&Capability],
    probe: &str,
) -> Vec<VerifyResult> {
    let mut results = Vec::new();
    let mut gen_targets: Vec<&Capability> = Vec::new();
    let mut nav_targets: Vec<&Capability> = Vec::new();
    for cap in targets {
        match cap.proof {
            ProofKind::RoundTripToggle => results.extend(verify_roundtrip(backend, cap)),
            ProofKind::RoundTripMenu => {
                if cap.key == "reasoning_effort" {
                    results.extend(verify_reasoning_effort(backend, cap));
                } else {
                    results.extend(verify_roundtrip(backend, cap));
                }
            }
            ProofKind::Generation | ProofKind::Induced | ProofKind::Navigation => {
                if cap.key == "projects" {
                    nav_targets.push(*cap);
                } else {
                    gen_targets.push(*cap);
                }
            }
            ProofKind::RoundTripSegment | ProofKind::None => {
                // Nicht fahrbar — bleibt Quest (§6 des Plans).
            }
        }
    }
    if !gen_targets.is_empty() {
        results.extend(generation_sequence(backend, &gen_targets, probe));
    }
    for cap in nav_targets {
        results.extend(verify_navigation(backend, cap));
    }
    results
}

/// Alle `needs`-Schlüssel müssen nicht-leere Selektorlisten haben, sonst bleibt
/// die Fähigkeit eine Quest (ehrlich NeedsSelectors statt erfundener Beleg —
/// §13 des Plans).
fn has_sel(sel: &Selectors, needs: &[&str]) -> bool {
    needs.iter().all(|k| !sel.list(k).is_empty())
}

fn hash_for(backend: &WebBrainBackend, cap: &Capability) -> u32 {
    crate::capability_proof::selector_hash_for(cap, backend.selectors.as_value())
}

fn measure(
    cap: &str,
    before: String,
    after: String,
    proven: bool,
    note: String,
    winner: Option<String>,
) -> Measurement {
    Measurement {
        capability_key: cap.to_string(),
        before,
        after,
        proven,
        restored: None,
        note,
        winning_selector: winner,
    }
}

/// Unreachable-Verzeichnis für den Vorlauf: festes Vokabular als `note`, damit
/// `evidence` auszählbar bleibt (`blocked`, `rate_limit`, `cloudflare`,
/// `logged_out`, `start_failed`, `circuit_open`).
fn all_unreachable(
    backend: &WebBrainBackend,
    targets: &[&Capability],
    reason: &str,
) -> Vec<VerifyResult> {
    let started = Instant::now();
    targets
        .iter()
        .map(|cap| {
            let hash = hash_for(backend, cap);
            VerifyResult::new(
                measure(cap.key, String::new(), String::new(), false, reason.to_string(), None),
                ProofOutcome::Unreachable,
                hash,
                started,
            )
        })
        .collect()
}

fn cap_by_key<'a>(targets: &'a [&Capability], key: &str) -> Option<&'a Capability> {
    targets.iter().find(|c| c.key == key).copied()
}

/// Nur die `needs`-Schlüssel als Objekt — in Katalog-Reihenfolge. Ohne fremde
/// Schlüssel, sonst wäre der Flat-Index der Fallback-Kette verzerrt.
fn needs_subset(all: &Value, needs: &[&str]) -> Value {
    let mut map = serde_json::Map::new();
    for key in needs {
        if let Some(v) = all.get(*key) {
            map.insert((*key).to_string(), v.clone());
        }
    }
    Value::Object(map)
}

/// Baut den `js_scan_indexed`-Ausdruck über die `needs`-Teilmenge — für
/// `resolve_fallback` und für Tests, die den Mock auf exakte Zeichenketten
/// registrieren.
pub(crate) fn fallback_expr_for(sel: &Selectors, needs: &[&str]) -> String {
    let subset = needs_subset(sel.as_value(), needs);
    js::js_scan_indexed(&subset, js::FALLBACK_VISIBLE_BODY, "{i:-1,v:null}")
}

/// Erster SICHTBARER Selektor der `needs`-Kette — der aufgelöste Gewinner des
/// Fallbacks (§5, `js_scan_indexed`). `(selektor, flat_index)` oder `None`.
fn resolve_fallback(backend: &WebBrainBackend, needs: &[&str]) -> Option<(String, i32)> {
    let expr = fallback_expr_for(&backend.selectors, needs);
    let mut guard = backend.driver.borrow_mut();
    let driver = guard.as_mut()?;
    let v = driver.evaluate(&expr).ok()?;
    let i = v.get("i").and_then(|x| x.as_i64()).unwrap_or(-1);
    if i < 0 {
        return None;
    }
    let sel = v.get("v").and_then(|x| x.as_str()).map(str::to_string);
    sel.map(|s| (s, i as i32))
}

/// RoundTrip-Arme (Toggle und Menü): inneres `operations::verify_surface` mit
/// `proposal_from(cap, winner)`. Nur `reasoning_effort` nimmt den Pfad-Arm.
fn verify_roundtrip(backend: &mut WebBrainBackend, cap: &Capability) -> Vec<VerifyResult> {
    let start = Instant::now();
    if !has_sel(&backend.selectors, cap.needs) {
        return Vec::new(); // bleibt Quest
    }
    let hash = hash_for(backend, cap);
    let winner = match resolve_fallback(backend, cap.needs) {
        Some((w, _)) => w,
        None => {
            return vec![VerifyResult::new(
                measure(
                    cap.key,
                    String::new(),
                    String::new(),
                    false,
                    "kein sichtbarer Selektor".to_string(),
                    None,
                ),
                ProofOutcome::Unreachable,
                hash,
                start,
            )];
        }
    };
    let proposal = crate::brain_probe::proposal_from(cap, &winner);
    let verdict = {
        let mut guard = backend.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::verify_surface(driver.as_mut(), &proposal),
            None => Err("Backend nicht gestartet".to_string()),
        }
    };
    match verdict {
        Ok(v) => {
            let m: Measurement = (&v).into();
            let outcome = if v.proven {
                ProofOutcome::Passed
            } else {
                ProofOutcome::Failed
            };
            vec![VerifyResult::new(m, outcome, hash, start)]
        }
        Err(e) => vec![VerifyResult::new(
            measure(
                cap.key,
                String::new(),
                String::new(),
                false,
                e,
                Some(winner),
            ),
            ProofOutcome::Unreachable,
            hash,
            start,
        )],
    }
}

/// `reasoning_effort` braucht einen Untermenü-Pfad (`select_in_menu_path`);
/// ohne konfigurierten `reasoning_effort_path` bleibt die Fähigkeit ehrlich
/// eine Quest (§13 — Pfade müssen noch nachgetragen werden).
fn verify_reasoning_effort(backend: &mut WebBrainBackend, cap: &Capability) -> Vec<VerifyResult> {
    let start = Instant::now();
    if !has_sel(&backend.selectors, cap.needs) {
        return Vec::new(); // bleibt Quest
    }
    let sel = backend.sel("reasoning_effort_path");
    let path: Vec<&str> = sel.iter().map(|s| s.as_str()).collect();
    if path.is_empty() {
        return Vec::new(); // kein Pfad konfiguriert -> Quest
    }
    let hash = hash_for(backend, cap);
    let menu_key = "reasoning_effort_menu";
    let before = backend.menu_label(menu_key);
    let last_step = path[path.len() - 1];
    match backend.select_in_menu_path(menu_key, "model_option", &path) {
        Ok(after) => {
            let proven = after.to_lowercase().contains(&last_step.trim().to_lowercase());
            let note = if after.contains("bereits aktiv") {
                after.clone()
            } else {
                format!("{before} -> {after}")
            };
            let m = measure(
                cap.key,
                before,
                after,
                proven,
                note,
                Some(path[0].to_string()),
            );
            let outcome = if proven {
                ProofOutcome::Passed
            } else {
                ProofOutcome::Failed
            };
            vec![VerifyResult::new(m, outcome, hash, start)]
        }
        Err(e) => vec![VerifyResult::new(
            measure(cap.key, before, String::new(), false, e, None),
            ProofOutcome::Failed,
            hash,
            start,
        )],
    }
}

/// Navigation via `open_section`: URL-Paar vorher/nachher als Beleg, dann
/// selbst zurücknavigieren — schlägt der Rückweg fehl, ist der Lauf verwirkt.
fn verify_navigation(backend: &mut WebBrainBackend, cap: &Capability) -> Vec<VerifyResult> {
    if !has_sel(&backend.selectors, cap.needs) {
        return Vec::new();
    }
    let start = Instant::now();
    let hash = hash_for(backend, cap);
    let key = &cap.needs[0];
    match backend.open_section(key) {
        Ok((before, after)) => {
            let winner = resolve_fallback(backend, cap.needs).map(|(w, _)| w);
            let m = measure(
                cap.key,
                before,
                after,
                true,
                "URL-Wechsel".to_string(),
                winner,
            );
            let mut results = vec![VerifyResult::new(m, ProofOutcome::Passed, hash, start)];
            if navigate_back(backend).is_err() {
                results.push(VerifyResult::new(
                    measure(
                        cap.key,
                        String::new(),
                        String::new(),
                        false,
                        "Rueckkehr zur Startseite fehlgeschlagen — Lauf abgebrochen".to_string(),
                        None,
                    ),
                    ProofOutcome::Unreachable,
                    hash,
                    start,
                ));
            }
            results
        }
        Err(e) => vec![VerifyResult::new(
            measure(cap.key, String::new(), String::new(), false, e, None),
            ProofOutcome::Failed,
            hash,
            start,
        )],
    }
}

fn navigate_back(backend: &WebBrainBackend) -> Result<(), String> {
    let url = backend.brain_url().to_string();
    let mut guard = backend.driver.borrow_mut();
    match guard.as_mut() {
        Some(driver) => driver
            .navigate(&url, Duration::from_secs(30))
            .map_err(|e| e.to_string()),
        None => Err("Backend nicht gestartet".to_string()),
    }
}

/// Urteil für `new_chat` aus zwei unabhängigen Signalen — reine Rechnung,
/// damit sie ohne Browser und ohne Mock-Aufrufzählerei prüfbar ist.
///
/// Zwei Kriterien, weil eines nicht reicht: der URL-Wechsel trägt bei den
/// meisten Brains, aber gemini bleibt beim neuen Chat auf `/app`. Ein geleerter
/// Verlauf belegt denselben Vorgang, ohne die URL zu brauchen.
///
/// `count_before > 0` ist Bedingung: nach der Probe steht dort eine Antwort.
/// Ohne diese Schranke würde ein ohnehin leerer Verlauf (0 → 0) als „geleert"
/// durchgehen und jeden wirkungslosen Klick belegen — genau die
/// Trivialerfüllung, die der Round-Trip an anderer Stelle ausschließt.
///
/// Die beiden Zweige bleiben im Text getrennt, damit im `evidence` steht,
/// WELCHES Kriterium getragen hat. Ein blankes ODER würde verdecken, dass ein
/// Brain nur noch über den Ersatzweg belegt wird — und damit die beginnende
/// Selektor-Drift unsichtbar machen.
fn new_chat_outcome(url_changed: bool, count_before: i32, count_after: i32) -> (ProofOutcome, String) {
    let history_cleared = count_before > 0 && count_after == 0;
    match (url_changed, history_cleared) {
        (true, true) => (
            ProofOutcome::Passed,
            format!("URL-Wechsel + Verlauf geleert ({count_before} -> 0)"),
        ),
        (true, false) => (ProofOutcome::Passed, "URL-Wechsel".to_string()),
        (false, true) => (
            ProofOutcome::Passed,
            format!("Verlauf geleert ({count_before} -> 0), URL unveraendert"),
        ),
        (false, false) => (
            ProofOutcome::Failed,
            format!(
                "weder URL-Wechsel noch geleerter Verlauf \
                 ({count_before} -> {count_after}) — kein Beleg"
            ),
        ),
    }
}

/// Die Generierungs-Sequenz: Probe senden → eigener Poll mit Dreier-ODER →
/// Stop-Beweis → zuletzt `new_chat`. Schreibt Belege für `chat`,
/// `stop_generation` und `new_chat` aus EINEM Lauf.
///
/// Ablauf im Detail:
///
/// 0. Hygiene, **unbewertet**: ist der Verlauf nicht leer (`assistant_count > 0`),
///    einmal `new_chat` klicken. Die Wurzel-URLs sind normalerweise schon der
///    neue Chat; das greift nur, wenn eine Oberfläche das letzte Gespräch
///    wiederherstellt.
/// 1. `send(probe)` liefert die Baseline; ein `Err` ist ein chat-Failed
///    (Absenden ist die Fähigkeit, nicht ihr Umfeld).
/// 2. EIGENER Poll (nicht `wait_response` — dessen Phase 2 wartet bis zur
///    Vollständigkeit, dann wäre nichts mehr zu stoppen):
///    `probe_generation` in einem CDP-Roundtrip, Dreier-ODER wie `wait_response`
///    (`count > baseline || (has_stop && stop) || text != baseline_text`).
/// 3. Stop sichtbar → `click_first("stop_button")` → weg UND Text wächst nicht
///    weiter → `stop_generation` Passed. Nie sichtbar → Failed (toter
///    Selektor). War sichtbar, aber der Klick blieb wirkungslos → Unreachable.
/// 4. **Zuletzt** `new_chat`: jetzt existiert eine Konversation, die man
///    verlassen kann. Urteil über [`new_chat_outcome`] aus URL-Wechsel ODER
///    geleertem Verlauf.
///
/// Die Reihenfolge ist nicht beliebig: `new_chat` stand ursprünglich an
/// Position 1 und scheiterte am 2026-08-09 bei 8 von 8 Brains, weil die Sitzung
/// auf `brain_url` startet — das IST bereits der neue Chat, es gab nichts zu
/// verlassen.
fn generation_sequence(
    backend: &mut WebBrainBackend,
    targets: &[&Capability],
    probe: &str,
) -> Vec<VerifyResult> {
    let mut results = Vec::new();
    let do_new_chat = targets.iter().any(|c| c.key == "new_chat");
    let do_chat = targets.iter().any(|c| c.key == "chat");
    let do_stop = targets.iter().any(|c| c.key == "stop_generation");

    // --- Hygiene: frischer Thread, aber UNBEWERTET ---
    //
    // Die Wurzel-URLs sind bereits der neue Chat (`claude.ai/new`,
    // `chatgpt.com/`, …), die Sitzung startet also normalerweise leer. Stellt
    // eine Oberflaeche doch das letzte Gespraech wieder her, laege die Probe
    // mitten im echten Verlauf des Nutzers — dann einmal `new_chat` klicken.
    //
    // Dieser Klick ist ausdruecklich KEIN Beleg: an dieser Stelle gaebe es
    // nichts zu verlassen, und genau daran ist der Beleg am 2026-08-09 bei 8
    // von 8 Brains gescheitert. Belegt wird `new_chat` am Ende der Sequenz,
    // wenn eine Konversation mit eigener URL existiert (§5).
    if backend.assistant_count() > 0 {
        let _ = backend.new_chat();
    }

    // --- chat + stop_generation ---
    let cap_chat = cap_by_key(targets, "chat");
    let cap_stop = cap_by_key(targets, "stop_generation");
    let chat_driveable = cap_chat
        .map(|c| has_sel(&backend.selectors, c.needs))
        .unwrap_or(false);
    let stop_driveable = cap_stop.map(|_| !backend.sel("stop_button").is_empty()).unwrap_or(false);
    if do_chat && !chat_driveable {
        return results; // chat bleibt Quest — ohne chat kein Stop-Lauf
    }
    // `new_chat` haengt seit der Umstellung an der Probe: ohne Konversation
    // gibt es nichts zu verlassen. Also laeuft die Sequenz auch dann, wenn nur
    // `--cap new_chat` angefordert wurde — dieselbe Abhaengigkeitsregel wie bei
    // `Induced`/`stop_generation` (§8). Belege werden trotzdem nur fuer das
    // Angeforderte geschrieben.
    //
    // Fehlt der Selektor, bleibt `new_chat` eine Quest: dann darf hier auch
    // keine Probe laufen, sonst kostet ein nicht fahrbarer Beleg ein Kontingent.
    let new_chat_driveable = do_new_chat
        && cap_by_key(targets, "new_chat")
            .map(|c| has_sel(&backend.selectors, c.needs))
            .unwrap_or(false);
    if !do_chat && !do_stop && !new_chat_driveable {
        return results; // nichts mehr zu tun
    }

    let start = Instant::now();
    let baseline = match backend.send(probe) {
        Ok(b) => b,
        Err(e) => {
            // Nur berichten, was angefordert wurde: laeuft die Sequenz allein
            // wegen `--cap new_chat`, gehoert hier kein chat-Record hin.
            if do_chat {
                let hash = cap_chat.map(|c| hash_for(backend, c)).unwrap_or(0);
                let winner =
                    cap_chat.and_then(|c| resolve_fallback(backend, c.needs).map(|(w, _)| w));
                let m = measure(
                    "chat",
                    String::new(),
                    String::new(),
                    false,
                    format!("send fehlgeschlagen: {e}"),
                    winner,
                );
                results.push(VerifyResult::new(m, ProofOutcome::Failed, hash, start));
            }
            return results;
        }
    };

    // Selektor-Literale einmal bauen, dann ein CDP-Roundtrip pro Poll.
    let assistant_js = backend.sel_js("assistant_message", &["div.prose"]);
    let stop_js = WebBrainBackend::js_selectors(&backend.sel("stop_button"));
    let baseline_text = backend.baseline_text.borrow().clone();
    let deadline = crate::timeouts::resolve_timeout("wait_response", &backend.brain_id, probe, None);
    let deadline = Instant::now() + Duration::from_secs_f64(deadline);

    let mut chat_proven = false;
    let mut chat_trigger = String::new();
    let mut stop_seen = false;
    let mut stop_clicked = false;
    let mut stop_gone = false;
    let mut frozen = false;
    let mut last_text = String::new();
    let mut stable_since = Instant::now();
    let mut block_polls = 0u32;
    let mut blocked = false;

    loop {
        if Instant::now() >= deadline {
            break;
        }
        let (count, text, stop) = backend.probe_generation(&assistant_js, &stop_js, -1);
        let stop_visible = stop_driveable && stop;

        if !chat_proven {
            let text_changed = !text.trim().is_empty() && text != baseline_text;
            if count > baseline || stop_visible || text_changed {
                chat_proven = true;
                chat_trigger = if count > baseline {
                    "count>baseline".to_string()
                } else if stop_visible {
                    "stop sichtbar".to_string()
                } else {
                    "text geaendert".to_string()
                };
            }
        }
        if stop_visible {
            stop_seen = true;
        }
        if chat_proven && stop_visible && !stop_clicked {
            stop_clicked = backend.click_first("stop_button");
        }
        if stop_clicked {
            if !stop_visible {
                stop_gone = true;
            }
            if !last_text.is_empty() && text == last_text {
                frozen = true;
            }
        }
        let text_stable = chat_proven && !text.is_empty() && text == last_text;
        if text_stable && stable_since.elapsed() >= Duration::from_secs(STABLE_DONE_SECS) {
            break; // Generierung fertig (Text steht still) — Stop-Fenster vorbei.
        }
        if stop_clicked && stop_gone && frozen {
            break; // Stop nachweislich gewirkt.
        }
        if !text_stable {
            stable_since = Instant::now();
        }
        last_text = text;

        block_polls += 1;
        if block_polls.is_multiple_of(BLOCK_POLL_EVERY) && backend.detect_block_banner().is_some() {
            blocked = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }

    if blocked {
        // Rate-Limit/Blockade während der Probe: alles Unreachable `blocked`
        // (§10), nicht Failed — die Fähigkeit war gar nicht im Spiel.
        let reason = "blocked".to_string();
        if do_chat {
            let m = measure("chat", String::new(), String::new(), false, reason.clone(), None);
            let hash = cap_chat.map(|c| hash_for(backend, c)).unwrap_or(0);
            results.push(VerifyResult::new(m, ProofOutcome::Unreachable, hash, start));
        }
        if do_stop {
            let m = measure("stop_generation", String::new(), String::new(), false, reason, None);
            let hash = cap_stop.map(|c| hash_for(backend, c)).unwrap_or(0);
            results.push(VerifyResult::new(m, ProofOutcome::Unreachable, hash, start));
        }
        return results;
    }

    // --- chat belegt? ---
    if do_chat {
        let cap = cap_chat.expect("chat im Katalog");
        let winner = resolve_fallback(backend, cap.needs).map(|(w, _)| w);
        let hash = hash_for(backend, cap);
        if chat_proven {
            let m = measure(
                cap.key,
                format!("baseline {baseline}"),
                String::new(),
                true,
                format!("chat belegt ({chat_trigger})"),
                winner,
            );
            results.push(VerifyResult::new(m, ProofOutcome::Passed, hash, start));
        } else {
            let m = measure(
                cap.key,
                String::new(),
                String::new(),
                false,
                "kein Antwort-Signal im Poll (Timeout)".to_string(),
                winner,
            );
            results.push(VerifyResult::new(m, ProofOutcome::Failed, hash, start));
        }
    }

    // --- stop_generation belegt? ---
    if do_stop && stop_driveable {
        let cap = cap_stop.expect("stop_generation im Katalog");
        let winner = resolve_fallback(backend, cap.needs).map(|(w, _)| w);
        let hash = hash_for(backend, cap);
        let (outcome, note) = if stop_seen && stop_clicked && stop_gone && frozen {
            (
                ProofOutcome::Passed,
                "Stop geklickt, verschwunden, Text eingefroren".to_string(),
            )
        } else if !stop_seen {
            (ProofOutcome::Failed, "Stop-Button nie sichtbar".to_string())
        } else if !stop_clicked {
            (
                ProofOutcome::Unreachable,
                "Stop war sichtbar, Klick kam nicht an".to_string(),
            )
        } else {
            (
                ProofOutcome::Unreachable,
                "Stop-Klick ohne belegbare Wirkung".to_string(),
            )
        };
        let m = measure(
            cap.key,
            String::new(),
            String::new(),
            outcome == ProofOutcome::Passed,
            note,
            winner,
        );
        results.push(VerifyResult::new(m, outcome, hash, Instant::now()));
    }

    // --- new_chat belegen: ZULETZT, wenn es etwas zu verlassen gibt ---
    //
    // Vorher stand das am Anfang der Sequenz. Ergebnis der ersten Messung
    // (2026-08-09): 8 von 8 `Failed`, immer "neuer Chat ohne URL-Wechsel", und
    // zwar ohne einen einzigen Selektorfehler. `get_conversation_ref` ist
    // `driver.current_url()`, und die Sitzung startet auf `brain_url` — das IST
    // der neue Chat. Es gab nichts zu verlassen.
    //
    // Nach der Probe existiert eine Konversation mit eigener URL; erst jetzt ist
    // der Wechsel echt. Nebeneffekt: das Konto bleibt auf einem leeren Chat
    // stehen statt im Probe-Gespraech.
    if do_new_chat {
        let cap = cap_by_key(targets, "new_chat").expect("new_chat im Katalog");
        if has_sel(&backend.selectors, cap.needs) {
            // MESSBEFUND 2026-08-10 (offen, nicht behoben): bei gemini haengt
            // das Ergebnis dieses Schritts davon ab, ob `stop_generation` im
            // selben Lauf geprueft wurde.
            //   ohne  --cap stop_generation:  Passed, "URL-Wechsel + Verlauf
            //                                 geleert (1 -> 0)"  (2x)
            //   mit   --cap stop_generation:  Failed, "1 -> 1"    (3x)
            // Ein Setzenlassen von 1,5 s hat NICHTS geaendert, die Vermutung
            // "Generierung laeuft noch" ist damit widerlegt — der Poll war
            // ohnehin bis zum Deadline (137 s) gelaufen. Wahrscheinlicher:
            // der Stop-Klick hinterlaesst bei gemini einen DOM-Zustand, in dem
            // der Neu-Chat-Anker nicht mehr trifft.
            //
            // Bewusst KEIN spekulativer Workaround an dieser Stelle: eine
            // Aenderung ohne Wirkungsnachweis waere genau das, was dieses
            // Modul sonst verhindert. Zum Klaeren braucht es einen
            // DOM-Abzug NACH dem Stop-Klick.
            let nc_start = Instant::now();
            let hash = hash_for(backend, cap);
            let before = backend.get_conversation_ref();
            // Zweites Kriterium: der Verlauf muss leer werden. Noetig fuer
            // Oberflaechen, die beim neuen Chat die URL NICHT wechseln —
            // gemini bleibt auf `/app` und meldete deshalb am 2026-08-09 einen
            // echten Versuch (845 ms) als Fehlschlag. Der Zaehler vor dem Klick
            // ist > 0, weil die Probe gerade gelaufen ist.
            let count_before = backend.assistant_count();
            let winner = resolve_fallback(backend, cap.needs).map(|(w, _)| w);
            match backend.new_chat() {
                Ok(()) => {
                    let after = backend.get_conversation_ref();
                    let count_after = backend.assistant_count();
                    let url_changed = after != before && after.is_some();
                    let (outcome, note) = new_chat_outcome(url_changed, count_before, count_after);
                    let m = measure(
                        cap.key,
                        before.unwrap_or_default(),
                        after.unwrap_or_default(),
                        outcome == ProofOutcome::Passed,
                        note,
                        winner,
                    );
                    results.push(VerifyResult::new(m, outcome, hash, nc_start));
                }
                Err(e) => {
                    // Kein frueher Return mehr: `new_chat` ist der letzte
                    // Schritt, es haengt nichts mehr daran.
                    let m = measure(
                        cap.key,
                        before.unwrap_or_default(),
                        String::new(),
                        false,
                        e,
                        winner,
                    );
                    results.push(VerifyResult::new(m, ProofOutcome::Unreachable, hash, nc_start));
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::selectors::Selectors;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    const PROBE: &str =
        "webagent capability probe test-stamp — zaehle langsam von 1 bis 100, je Zahl eine Zeile.";

    fn cap(key: &str) -> &'static Capability {
        crate::capability::capability(key).expect("im Katalog")
    }

    fn backend_for(brain_id: &str, state: MockPageState) -> WebBrainBackend {
        let backend = WebBrainBackend::from_config(brain_id).expect("Brain-Konfiguration");
        backend.attach_page_driver(Box::new(MockPageDriver::new(state)));
        backend
    }

    fn qwen_with(state: MockPageState) -> WebBrainBackend {
        backend_for("qwen", state)
    }

    fn backend_with_selectors(
        brain_id: &str,
        selectors: Value,
        state: MockPageState,
    ) -> WebBrainBackend {
        let mut backend = backend_for(brain_id, state);
        backend.selectors = Selectors::from_value(selectors);
        backend
    }

    /// Baut den Zustand, der `session_state` zu `Ready` macht: Eval-Alive und
    /// ein sichtbarer `login_indicator`.
    fn ready_state(sel: &Selectors) -> MockPageState {
        let login = js::js_scan(
            &js::js_selectors(&sel.list("login_indicator")),
            "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}",
            "false",
        );
        MockPageState::new().on_eval("1", json!(1)).on_eval(login, json!(true))
    }

    fn composer_coords_expr(sel: &Selectors) -> String {
        js::js_scan(
            &sel.js("composer", &[]),
            "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}",
            "null",
        )
    }

    fn composer_set_expr(sel: &Selectors, text: &str) -> String {
        let t = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        js::js_scan(
            &sel.js("composer", &[]),
            &format!(
                "var el=Q(S[i]);if(el){{var cur=('value' in el)?(el.value||''):(el.textContent||'');if(cur.trim().length===0){{if('value' in el){{el.value={t};}}else{{el.textContent={t};}}el.dispatchEvent(new InputEvent('input',{{bubbles:true}}));}}return true;}}"
            ),
            "false",
        )
    }

    fn click_first_expr(sel: &Selectors, key: &str) -> String {
        js::js_scan(
            &js::js_selectors(&sel.list(key)),
            "var el=Q(S[i]);if(el){el.click();return true;}",
            "false",
        )
    }

    fn assistant_count_expr(sel: &Selectors) -> String {
        js::js_scan(
            &sel.js("assistant_message", &["div.prose"]),
            "var n=QA(S[i]).length;if(n>0)return n;",
            "0",
        )
    }

    fn gen(count: i32, text: &str, stop: bool) -> Value {
        json!({"count": count, "text": text, "stop": stop})
    }

    fn probe_expr(sel: &Selectors) -> String {
        let assistant_js = sel.js("assistant_message", &["div.prose"]);
        let stop_js = WebBrainBackend::js_selectors(&sel.list("stop_button"));
        WebBrainBackend::probe_generation_js(&assistant_js, &stop_js, -1)
    }

    fn fallback_expr(sel: &Selectors, needs: &[&str]) -> String {
        super::fallback_expr_for(sel, needs)
    }

    fn labeled_expr() -> String {
        js::js_scan(
            &js::js_selectors(&[
                "body [aria-label]".to_string(),
                "body [role=button]".to_string(),
                "body button".to_string(),
                "body [role=menuitem]".to_string(),
            ]),
            "var n=QA(S[i]).length;if(n>0)return n;",
            "0",
        )
    }

    /// Eine Sitzung belegt drei Fähigkeiten: new_chat (URL-Wechsel),
    /// chat (Dreier-ODER) und stop_generation (Stop sichtbar → Klick → weg).
    #[test]
    fn eine_sitzung_belegt_new_chat_chat_und_stop() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let mut state = ready_state(&sel);
        state = state
            .on_eval(composer_coords_expr(&sel), json!({"x": 10.0, "y": 12.0}))
            .on_eval(composer_set_expr(&sel, PROBE), json!(true))
            .on_eval(click_first_expr(&sel, "send_button"), json!(true))
            .on_eval(click_first_expr(&sel, "stop_button"), json!(true))
            // Drei Werte: der erste geht an die Hygiene-Pruefung (leerer Thread
            // → kein Klick), der zweite ist die Baseline vor dem Senden, der
            // dritte belegt das Absenden.
            .on_eval_seq(assistant_count_expr(&sel), vec![json!(0), json!(0), json!(1)])
            .on_eval(
                fallback_expr(&sel, cap("chat").needs),
                json!({"i": 0, "v": "button[aria-label*='Send' i]"}),
            )
            .on_eval(
                fallback_expr(&sel, cap("new_chat").needs),
                json!({"i": 0, "v": "button[aria-label*='New chat' i]"}),
            )
            .on_eval(
                fallback_expr(&sel, cap("stop_generation").needs),
                json!({"i": 0, "v": "button[aria-label*='stoppen' i]"}),
            )
            .on_eval_seq(
                probe_expr(&sel),
                vec![
                    gen(0, "", false),
                    gen(1, "1\n2", false),
                    gen(1, "1\n2", true),
                    gen(1, "1\n2", false),
                ],
            );
        let mut backend = qwen_with(state);
        let results = verify_capabilities(
            &mut backend,
            false,
            &[cap("new_chat"), cap("chat"), cap("stop_generation")],
            PROBE,
            5.0,
        );

        assert_eq!(results.len(), 3, "drei Belege aus einem Lauf");
        let by = |k: &str| {
            results
                .iter()
                .find(|r| r.measurement.capability_key == k)
                .unwrap_or_else(|| panic!("kein Beleg für {k}: {results:?}"))
        };
        let nc = by("new_chat");
        assert!(nc.measurement.proven, "new_chat: {nc:?}");
        assert_eq!(nc.outcome, ProofOutcome::Passed);
        assert!(nc.measurement.after.contains("://"), "URL-Wechsel belegt");
        let ch = by("chat");
        assert!(ch.measurement.proven, "chat: {ch:?}");
        assert_eq!(ch.outcome, ProofOutcome::Passed);
        assert_eq!(ch.measurement.winning_selector.as_deref(), Some("button[aria-label*='Send' i]"));
        let st = by("stop_generation");
        assert!(st.measurement.proven, "stop_generation: {st:?}");
        assert_eq!(st.outcome, ProofOutcome::Passed);
        assert!(st.measurement.note.contains("Stop"));
    }

    /// `new_chat` hat zwei unabhängige Kriterien. Der URL-Zweig trägt bei den
    /// meisten Brains, der Verlaufs-Zweig ist für die, die beim neuen Chat auf
    /// derselben URL bleiben (gemini: `/app`).
    #[test]
    fn new_chat_belegt_auch_ohne_url_wechsel() {
        // Nur URL: klassischer Fall.
        let (o, n) = new_chat_outcome(true, 1, 1);
        assert_eq!(o, ProofOutcome::Passed);
        assert_eq!(n, "URL-Wechsel");

        // Nur Verlauf geleert: gemini-Fall — muss ebenfalls belegen, und das
        // evidence muss sagen, welches Kriterium getragen hat.
        let (o, n) = new_chat_outcome(false, 2, 0);
        assert_eq!(o, ProofOutcome::Passed);
        assert!(n.contains("Verlauf geleert"), "{n}");
        assert!(n.contains("URL unveraendert"), "{n}");

        // Beides: darf nicht als „nur URL" verbucht werden.
        let (o, n) = new_chat_outcome(true, 3, 0);
        assert_eq!(o, ProofOutcome::Passed);
        assert!(n.contains("URL-Wechsel") && n.contains("Verlauf geleert"), "{n}");

        // Nichts von beidem: Failed.
        let (o, n) = new_chat_outcome(false, 2, 2);
        assert_eq!(o, ProofOutcome::Failed);
        assert!(n.contains("kein Beleg"), "{n}");
    }

    /// Ein ohnehin leerer Verlauf darf NICHT als „geleert" durchgehen — sonst
    /// belegt jeder wirkungslose Klick die Fähigkeit. Das ist dieselbe
    /// Trivialerfüllung, die der Round-Trip an anderer Stelle ausschließt.
    #[test]
    fn leerer_verlauf_ist_kein_geleerter_verlauf() {
        let (o, n) = new_chat_outcome(false, 0, 0);
        assert_eq!(o, ProofOutcome::Failed, "0 -> 0 ist kein Beleg: {n}");
    }

    /// Stop ist im Angebot, wird aber nie sichtbar: ehrliches Failed (toter
    /// Selektor), nicht Unreachable — die Fähigkeit war ansprechbar.
    #[test]
    fn stop_nie_sichtbar_ist_failed() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let mut state = ready_state(&sel);
        state = state
            .on_eval(composer_coords_expr(&sel), json!({"x": 10.0, "y": 12.0}))
            .on_eval(composer_set_expr(&sel, PROBE), json!(true))
            .on_eval(click_first_expr(&sel, "send_button"), json!(true))
            // Drei Werte: der erste geht an die Hygiene-Pruefung (leerer Thread
            // → kein Klick), der zweite ist die Baseline vor dem Senden, der
            // dritte belegt das Absenden.
            .on_eval_seq(assistant_count_expr(&sel), vec![json!(0), json!(0), json!(1)])
            .on_eval(fallback_expr(&sel, cap("chat").needs), json!({"i": 0, "v": "b"}))
            .on_eval(
                fallback_expr(&sel, cap("stop_generation").needs),
                json!({"i": 0, "v": "button[aria-label*='stoppen' i]"}),
            )
            .on_eval_seq(
                probe_expr(&sel),
                vec![gen(0, "", false), gen(1, "1\n2", false)],
            );
        let mut backend = qwen_with(state);
        let results = verify_capabilities(&mut backend, false, &[cap("chat"), cap("stop_generation")], PROBE, 5.0);

        assert_eq!(results.len(), 2);
        let st = results
            .iter()
            .find(|r| r.measurement.capability_key == "stop_generation")
            .unwrap();
        assert_eq!(st.outcome, ProofOutcome::Failed);
        assert!(!st.measurement.proven);
        assert!(st.measurement.note.contains("nie sichtbar"));
        let ch = results.iter().find(|r| r.measurement.capability_key == "chat").unwrap();
        assert!(ch.measurement.proven, "chat läuft trotzdem: {ch:?}");
    }

    /// Stop ist sichtbar, aber der Klick kommt nicht an (unregistrierter
    /// Mock-Klick): Unreachable, kein Failed — die Fähigkeit war nie im Spiel.
    #[test]
    fn stop_klick_kommt_nicht_an_ist_unreachable() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let mut state = ready_state(&sel);
        state = state
            .on_eval(composer_coords_expr(&sel), json!({"x": 10.0, "y": 12.0}))
            .on_eval(composer_set_expr(&sel, PROBE), json!(true))
            .on_eval(click_first_expr(&sel, "send_button"), json!(true))
            // Drei Werte: der erste geht an die Hygiene-Pruefung (leerer Thread
            // → kein Klick), der zweite ist die Baseline vor dem Senden, der
            // dritte belegt das Absenden.
            .on_eval_seq(assistant_count_expr(&sel), vec![json!(0), json!(0), json!(1)])
            .on_eval(fallback_expr(&sel, cap("chat").needs), json!({"i": 0, "v": "b"}))
            .on_eval(
                fallback_expr(&sel, cap("stop_generation").needs),
                json!({"i": 0, "v": "button[aria-label*='stoppen' i]"}),
            )
            // Stop bleibt sichtbar, aber `click_first` ist nicht registriert → false.
            .on_eval_seq(
                probe_expr(&sel),
                vec![gen(0, "", false), gen(1, "1\n2", true)],
            );
        let mut backend = qwen_with(state);
        let results = verify_capabilities(&mut backend, false, &[cap("chat"), cap("stop_generation")], PROBE, 5.0);

        let st = results
            .iter()
            .find(|r| r.measurement.capability_key == "stop_generation")
            .unwrap();
        assert_eq!(st.outcome, ProofOutcome::Unreachable);
        assert!(st.measurement.note.contains("Klick kam nicht an"));
        let ch = results.iter().find(|r| r.measurement.capability_key == "chat").unwrap();
        assert!(ch.measurement.proven);
    }

    /// Stop ist sichtbar und geklickt, bleibt aber sichtbar (Klick wirkungslos):
    /// Unreachable — die Wirkung ist nicht belegbar.
    #[test]
    fn stop_klick_ohne_wirkung_ist_unreachable() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let mut state = ready_state(&sel);
        state = state
            .on_eval(composer_coords_expr(&sel), json!({"x": 10.0, "y": 12.0}))
            .on_eval(composer_set_expr(&sel, PROBE), json!(true))
            .on_eval(click_first_expr(&sel, "send_button"), json!(true))
            .on_eval(click_first_expr(&sel, "stop_button"), json!(true))
            // Drei Werte: der erste geht an die Hygiene-Pruefung (leerer Thread
            // → kein Klick), der zweite ist die Baseline vor dem Senden, der
            // dritte belegt das Absenden.
            .on_eval_seq(assistant_count_expr(&sel), vec![json!(0), json!(0), json!(1)])
            .on_eval(fallback_expr(&sel, cap("chat").needs), json!({"i": 0, "v": "b"}))
            .on_eval(
                fallback_expr(&sel, cap("stop_generation").needs),
                json!({"i": 0, "v": "button[aria-label*='stoppen' i]"}),
            )
            // Stop bleibt ewig sichtbar → nie `gone`, nie eingefroren.
            .on_eval_seq(probe_expr(&sel), vec![gen(0, "", false), gen(1, "1\n2", true)]);
        let mut backend = qwen_with(state);
        let results = verify_capabilities(&mut backend, false, &[cap("chat"), cap("stop_generation")], PROBE, 5.0);

        let st = results
            .iter()
            .find(|r| r.measurement.capability_key == "stop_generation")
            .unwrap();
        assert_eq!(st.outcome, ProofOutcome::Unreachable);
        assert!(st.measurement.note.contains("ohne belegbare Wirkung"));
    }

    /// Das Absenden scheitert: chat ist ein ehrliches Failed (der Beweis — kein
    /// Absende-Signal — fehlt), nicht ein Unreachable.
    #[test]
    fn sendefehler_ist_chat_failed() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let mut state = ready_state(&sel);
        state = state
            .on_eval(composer_coords_expr(&sel), json!({"x": 10.0, "y": 12.0}))
            .on_eval(composer_set_expr(&sel, PROBE), json!(true))
            .on_eval(click_first_expr(&sel, "send_button"), json!(true))
            // Zaehler waechst nie → verify_submitted scheitert 4×, dann Fehler.
            .on_eval(assistant_count_expr(&sel), json!(0))
            .on_eval(fallback_expr(&sel, cap("chat").needs), json!({"i": 0, "v": "b"}));
        let mut backend = qwen_with(state);
        let results = verify_capabilities(&mut backend, false, &[cap("chat")], PROBE, 5.0);

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.measurement.capability_key, "chat");
        assert_eq!(r.outcome, ProofOutcome::Failed);
        assert!(!r.measurement.proven);
        assert!(r.measurement.note.contains("Absenden fehlgeschlagen"), "{:?}", r.measurement.note);
    }

    /// Fehlt ein `needs`-Selektor (new_chat_button), bleibt die Fähigkeit eine
    /// Quest — kein erfundener Beleg, kein Eintrag (§13).
    #[test]
    fn new_chat_ohne_selektor_bleibt_quest() {
        let sel = json!({
            "login_indicator": ["textarea"],
        });
        let mut backend = backend_with_selectors("qwen", sel, ready_state(&Selectors::from_value(json!({"login_indicator": ["textarea"]}))));
        let results = verify_capabilities(&mut backend, false, &[cap("new_chat")], PROBE, 0.5);
        assert!(results.is_empty(), "Quest ist kein Eintrag: {results:?}");
    }

    /// `reasoning_effort` ohne konfigurierten `reasoning_effort_path` bleibt
    /// ehrlich eine Quest — Pfade sind Nacharbeit (§13).
    #[test]
    fn reasoning_effort_ohne_pfad_bleibt_quest() {
        let sel = json!({
            "login_indicator": ["textarea"],
            "reasoning_effort_menu": ["[aria-label='Denkstufe']"],
        });
        let state = ready_state(&Selectors::from_value(sel.clone()));
        let mut backend = backend_with_selectors("qwen", sel, state);
        let results = verify_capabilities(&mut backend, false, &[cap("reasoning_effort")], PROBE, 0.5);
        assert!(results.is_empty(), "Quest ist kein Eintrag: {results:?}");
    }

    /// RoundTrip (Toggle): `operations::verify_surface` mit aufgelöstem Gewinner
    /// belegt Zustandswechsel + Rückweg, und der Gewinner landet im Record.
    #[test]
    fn roundtrip_toggle_belegt_wechsel_mit_gewinner() {
        let winner = "button[aria-label='DeepThink' i]";
        let sel = json!({
            "login_indicator": ["textarea"],
            "reasoning_toggle": [winner],
        });
        let state_expr = js::toggle_state_expr_for(&[winner.to_string()]);
        let click_expr = js::click_toggle_expr_for(&[winner.to_string()]);
        let fallback = fallback_expr(&Selectors::from_value(sel.clone()), cap("reasoning_toggle").needs);
        let mut state = ready_state(&Selectors::from_value(sel.clone()));
        state = state
            .on_eval(labeled_expr(), json!(5))
            .on_eval(fallback, json!({"i": 0, "v": winner}))
            .on_eval_seq(state_expr, vec![json!("off"), json!("on"), json!("off")])
            .on_eval(click_expr, json!(true));
        let mut backend = backend_with_selectors("qwen", sel, state);
        let results = verify_capabilities(&mut backend, false, &[cap("reasoning_toggle")], PROBE, 0.5);

        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert!(r.measurement.proven, "{:?}", r.measurement);
        assert_eq!(r.outcome, ProofOutcome::Passed);
        assert_eq!(r.measurement.restored, Some(true));
        assert_eq!(r.measurement.before, "off");
        assert_eq!(r.measurement.after, "on");
        assert_eq!(r.measurement.winning_selector.as_deref(), Some(winner));
    }

    /// Ausgeloggte Sitzung: alle Fähigkeiten Unreachable `logged_out` mit dem
    /// festen Vokabular, kein erfundener Beleg und kein zweiter Lauf.
    #[test]
    fn ausgeloggt_ist_unreachable_logged_out() {
        let sel = json!({
            "login_button": ["button:text('Log in')"],
        });
        let login_btn = js::js_scan(
            &js::js_selectors(&["button:text('Log in')".to_string()]),
            "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}",
            "false",
        );
        let state = MockPageState::new().on_eval("1", json!(1)).on_eval(login_btn, json!(true));
        let mut backend = backend_with_selectors("qwen", sel, state);
        let results = verify_capabilities(
            &mut backend,
            false,
            &[cap("chat"), cap("new_chat")],
            PROBE,
            0.5,
        );
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.outcome, ProofOutcome::Unreachable);
            assert_eq!(r.measurement.note, "logged_out");
            assert!(!r.measurement.proven);
        }
    }

    #[test]
    fn ziele_leer_ist_ganzer_katalog() {
        let (targets, warnings) = resolve_verify_targets(&[]).expect("leer ist ok");
        assert!(warnings.is_empty());
        assert_eq!(targets.len(), crate::capability::CATALOG.len());
        assert_eq!(targets.first().unwrap().key, "chat");
    }

    #[test]
    fn stop_generation_zieht_chat_und_new_chat_mit() {
        let (targets, warnings) =
            resolve_verify_targets(&["stop_generation".to_string()]).expect("ok");
        assert!(warnings.is_empty());
        let keys: Vec<&str> = targets.iter().map(|c| c.key).collect();
        assert_eq!(keys, vec!["chat", "new_chat", "stop_generation"]);
    }

    #[test]
    fn dedup_und_katalogreihenfolge() {
        let (targets, warnings) = resolve_verify_targets(&[
            "new_chat".to_string(),
            "chat".to_string(),
            "chat".to_string(),
            "stop_generation".to_string(),
        ])
        .expect("ok");
        assert!(warnings.is_empty());
        let keys: Vec<&str> = targets.iter().map(|c| c.key).collect();
        assert_eq!(keys, vec!["chat", "new_chat", "stop_generation"]);
    }

    #[test]
    fn unbekannter_key_ist_fehlermit_gueltigen() {
        let err = resolve_verify_targets(&["kaefer".to_string()]).unwrap_err();
        assert!(err.starts_with("unbekannte Faehigkeit 'kaefer'"));
        assert!(err.contains("chat"));
        assert!(err.contains("projects"));
    }

    #[test]
    fn nicht_fahrbare_werden_uebersprungen() {
        let (targets, warnings) =
            resolve_verify_targets(&["deep_research".to_string()]).expect("ok");
        assert!(targets.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].starts_with("deep_research"));
    }

    #[test]
    fn records_sind_1zu1_inklusive_unreachable() {
        use std::time::Instant;
        let m = Measurement {
            capability_key: "chat".to_string(),
            before: "a".to_string(),
            after: "b".to_string(),
            proven: true,
            restored: Some(true),
            note: "ok".to_string(),
            winning_selector: None,
        };
        let result = VerifyResult::new(m, ProofOutcome::Passed, 42, Instant::now());
        let records = verify_records("qwen", &[result]);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].brain_id, "qwen");
        assert_eq!(records[0].capability, "chat");
        assert_eq!(records[0].outcome, ProofOutcome::Passed);
        assert_eq!(records[0].selector_hash, 42);
    }
}
