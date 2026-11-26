//! pool_failover — die reine Entscheidungslogik des Worker-Pools.
//!
//! Aus `worker_pool.rs` herausgeloest (Refactoring 06:20): die browser-freie,
//! testbare FSM des Pools — BLOCK-Erkennung (stale Heartbeat / offener Breaker),
//! Reserve-Promotion, Cooldown/Restore inkl. dauerhaftem Retire. Nichts hier
//! spawnet Prozesse; die Aktionen (`BlockActions`/`RestoreActions`) fuehrt der
//! Supervisor in `worker_pool.rs` aus. Extern erreichbar weiterhin unter
//! `crate::worker_pool::…` (Re-Export in `worker_pool.rs`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::circuit_breaker::BreakerSnapshot;
use crate::pool_state::{
    PoolState, STATUS_ACTIVE, STATUS_AVAILABLE, STATUS_COOLDOWN, STATUS_RETIRED, STATUS_UNAVAILABLE,
};

/// Phase eines BLOCK-Failovers (rein informativ; der Eintrag wird nach dem
/// erfolgreichen Restore entfernt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailoverPhase {
    /// Brain erkannt als BLOCK, Reserve promoviert, Original im Cooldown.
    Blocked,
}

/// Verfolgt einen BLOCK-Failover pro Brain: wann erkannt, bis wann Cooldown,
/// welches Reserve-Brain uebernimmt, wie oft die Wiederherstellung schon
/// fehlschlug. Reine Laufzeit-Datenstruktur (nicht serialisiert) — nur im
/// `WorkerPool.failover` verwaltet.
#[derive(Debug, Clone)]
pub struct FailoverRecord {
    pub phase: FailoverPhase,
    /// RFC3339-Zeitpunkt der Block-Erkennung.
    pub detected_at: String,
    /// RFC3339-Zeitpunkt, bis zu dem das Original im Cooldown bleibt.
    pub cooldown_until: Option<String>,
    /// Reserve-Brain, das das Original waehrend des Cooldowns ersetzt.
    pub standby: Option<String>,
    /// PID des urspruenglichen (gekillten) Worker-Kindprozesses, falls bekannt.
    pub original_pid: Option<u32>,
    /// Run-ID des geblockten Workers, falls bekannt.
    pub run_id: Option<String>,
    /// Grund der Block-Erkennung (Signal A: breaker open; Signal B: stale heartbeat).
    pub reason: String,
    /// Zaehler fehlgeschlagener Wiederherstellungen (Erreichen von
    /// `MAX_FAILED_RESTORES` -> Brain wird dauerhaft `retired`).
    pub failover_count: u32,
}

/// Entscheidungen des BLOCK-Erkennungsschritts, die `tick()` real ausfuehrt
/// (Kind killen / Reserve spawnen). Getrennt von der Entscheidung, damit die
/// Logik rein (ohne Prozess-Spawn) testbar bleibt.
#[derive(Debug, Default)]
pub struct BlockActions {
    /// Blockierte Brains: deren Kindprozess killen.
    pub kill: Vec<String>,
    /// Reserve-Brains, die als frische Worker gestartet werden (Block-Ersatz).
    pub spawn: Vec<String>,
}

/// Entscheidungen des Cooldown/Restore-Schritts, die `tick()` real ausfuehrt.
#[derive(Debug, Default)]
pub struct RestoreActions {
    /// Original-Brains, die nach Cooldown-Ablauf frisch re-promoted werden.
    pub spawn: Vec<String>,
    /// Standby-Brains, die nach erfolgreichem Restore eingezogen werden (kill + available).
    pub retire: Vec<String>,
    /// Brains, die nach K fehlgeschlagenen Restores dauerhaft als `retired` enden.
    pub retired: Vec<String>,
}

/// Wählt das nächste zu promovierende Brain: erstes Kandidat mit Status
/// `available` (oder fehlend = available), das nicht bereits läuft.
/// Reine Funktion — browser-frei testbar.
pub fn select_to_promote(
    candidates: &[String],
    state: &PoolState,
    running: &HashSet<String>,
) -> Option<String> {
    for b in candidates {
        if running.contains(b) {
            continue;
        }
        let status = state
            .entries
            .get(b)
            .map(|e| e.status.as_str())
            .unwrap_or(STATUS_AVAILABLE);
        if status == STATUS_AVAILABLE {
            return Some(b.clone());
        }
    }
    None
}

/// BLOCK-Schritt des Failover-FSM (rein, ohne Prozess-Spawn): fuer jedes
/// neu erkannte, noch nicht im Cooldown befindliche Brain wird ein
/// Failover-Eintrag angelegt, das Original auf `cooldown` gesetzt und ein
/// ANDERES verfuegbares Brain (Reserve) via `select_to_promote` promoviert.
/// Gibt die auszufuehrenden Kill/Spawn-Aktionen zurueck.
pub fn compute_block_failover(
    now: SystemTime,
    blocked: &[String],
    candidates: &[String],
    state: &mut PoolState,
    running: &HashSet<String>,
    running_pids: &HashMap<String, u32>,
    failover: &mut HashMap<String, FailoverRecord>,
) -> BlockActions {
    let mut actions = BlockActions::default();
    let cooldown = Duration::from_secs(crate::config::block_cooldown_secs());
    let cooldown_until = format_rfc3339(now + cooldown).unwrap_or_default();

    // Bereits laufende + gerade geblockte Brains von der Reserve-Auswahl
    // ausschliessen, damit keine Doppelbelegung entsteht.
    let mut excluded: HashSet<String> = running.clone();
    excluded.extend(blocked.iter().cloned());

    for brain in blocked {
        // (f) Bereits im Cooldown -> nicht erneut flaggen (kein Double-Failover).
        if failover.contains_key(brain) {
            continue;
        }
        let reserve = select_to_promote(candidates, state, &excluded);
        if let Some(r) = &reserve {
            excluded.insert(r.clone());
        }

        // Original -> Cooldown, Reserve -> aktiv.
        state.set(brain, STATUS_COOLDOWN, "blocked: reserve promoted");
        if let Some(e) = state.entries.get_mut(brain) {
            e.cooldown_until = Some(cooldown_until.clone());
            e.replaced_by = reserve.clone();
        }
        actions.kill.push(brain.clone());
        if let Some(r) = &reserve {
            state.set(r, STATUS_ACTIVE, "failover standby");
            actions.spawn.push(r.clone());
        }

        let pid = running_pids.get(brain).copied();
        let rec = FailoverRecord {
            phase: FailoverPhase::Blocked,
            detected_at: crate::now_rfc3339(),
            cooldown_until: Some(cooldown_until.clone()),
            standby: reserve,
            original_pid: pid,
            run_id: None,
            reason: "blocked: stale heartbeat or breaker open".to_string(),
            failover_count: 0,
        };
        crate::bench_events::eprint_line(&format!(
            "[worker_pool] BLOCK {} (pid {:?}, run_id {:?}) -> Cooldown bis {}; Reserve {} promoted. reason={}; phase={:?}",
            brain, rec.original_pid, rec.run_id, cooldown_until, rec.standby.as_deref().unwrap_or("-"), rec.reason, rec.phase
        ));
        failover.insert(brain.clone(), rec);
    }

    actions
}

/// Cooldown/Restore-Schritt des Failover-FSM (rein bis auf den injizierten
/// `spawn_ok`-Closure, der den frischen Worker real startet). Fuer jeden
/// Failover-Eintrag, dessen Cooldown abgelaufen ist:
/// - Spawn des Originals gelingt -> Original re-promoted (`active`), Standby
///   eingezogen (`available`), Eintrag entfernt (Restored -> Healthy).
/// - Spawn fehlgeschlagen -> `failover_count` hochzaehlen; bei Erreichen von
///   `max_retries` Original als `retired` markieren (dauerhaft, kein Retry,
///   keine Auto-Recovery); sonst Cooldown verlaengern und erneut versuchen.
pub fn compute_restore(
    now: SystemTime,
    failover: &mut HashMap<String, FailoverRecord>,
    state: &mut PoolState,
    max_retries: u32,
    mut spawn_ok: impl FnMut(&str) -> bool,
) -> RestoreActions {
    let mut actions = RestoreActions::default();
    let cooldown = Duration::from_secs(crate::config::block_cooldown_secs());

    let expired: Vec<String> = failover
        .iter()
        .filter(|(_, rec)| {
            rec.cooldown_until
                .as_ref()
                .and_then(|s| parse_rfc3339(s))
                .map(|u| now >= u)
                .unwrap_or(false)
        })
        .map(|(b, _)| b.clone())
        .collect();

    for brain in expired {
        let mut rec = match failover.remove(&brain) {
            Some(r) => r,
            None => continue,
        };
        let ok = spawn_ok(&brain);
        if ok {
            // Standby einziehen.
            if let Some(s) = rec.standby.clone() {
                state.set(&s, STATUS_AVAILABLE, "standby retired after restore");
                actions.retire.push(s);
            }
            state.set(&brain, STATUS_ACTIVE, "restored after cooldown");
            actions.spawn.push(brain.clone());
            crate::bench_events::eprint_line(&format!(
                "[worker_pool] RESTORE {} nach Cooldown (Standby {:?} eingezogen)",
                brain, rec.standby
            ));
        } else {
            rec.failover_count += 1;
            if rec.failover_count >= max_retries {
                // Dauerhaft ausmustern: bewusst STATUS_RETIRED statt
                // STATUS_UNAVAILABLE, damit die Auto-Recovery in `tick()`
                // dieses Brain nicht nach der Retry-Frist wiederbelebt.
                state.set(
                    &brain,
                    STATUS_RETIRED,
                    &format!("retired after {max_retries} failed restores"),
                );
                actions.retired.push(brain.clone());
                crate::bench_events::eprint_line(&format!(
                    "[worker_pool] RETIRE {} nach {} fehlgeschlagenen Restores (pid {:?})",
                    brain, rec.failover_count, rec.original_pid
                ));
            } else {
                // Cooldown verlaengern, erneut versuchen.
                let next = format_rfc3339(now + cooldown).unwrap_or_default();
                rec.cooldown_until = Some(next.clone());
                if let Some(e) = state.entries.get_mut(&brain) {
                    e.cooldown_until = Some(next);
                }
                failover.insert(brain.clone(), rec);
            }
        }
    }

    actions
}

/// Reine BLOCK-Erkennung: ein laufender Worker gilt als BLOCK, wenn sein
/// Heartbeat-Alter den `stale`-Schwellwert ueberschreitet ODER der Circuit-
/// Breaker fuer sein Brain offen (`open == true`) ist. `running` sind
/// `(brain, heartbeat_alter)`-Paare. Idle, aber pollende Worker schreiben
/// regelmaessig -> frisch (Alter 0) -> nicht blockiert (idle-sicher).
pub fn detect_blocked(
    running: &[(String, Duration)],
    snaps: &[BreakerSnapshot],
    stale: Duration,
) -> Vec<String> {
    let open: HashSet<&str> = snaps
        .iter()
        .filter(|s| s.open)
        .map(|s| s.brain_id.as_str())
        .collect();
    running
        .iter()
        .filter(|(b, age)| *age > stale || open.contains(b.as_str()))
        .map(|(b, _)| b.clone())
        .collect()
}

/// Liefert `(brain, alter-seit-letztem-Heartbeat)` fuer die gegebenen Brains,
/// basierend auf dem Aenderungsdatum der `heartbeat_<brain>.json`-Dateien in
/// `workers_dir`. Extrahiert aus der bestehenden Hang-Erkennung, damit die
/// spaetere Browser-Pool-Arbeit das Alter teilen kann. `now` ist injizierbar
/// (fuer Tests).
/// Prueft, ob ein Worker basierend auf seinem letzten Heartbeat als veraltet (stale) gilt.
/// Gibt `true` zurueck, wenn `now_ms - last_heartbeat_ms >= timeout_ms`, sonst `false`.
/// Ungueltige Zeitabstaende (z.B. Heartbeat in der Zukunft) werden als aktiv behandelt.
pub fn is_worker_stale(last_heartbeat_ms: u64, now_ms: u64, timeout_ms: u64) -> bool {
    if now_ms < last_heartbeat_ms {
        return false;
    }
    now_ms - last_heartbeat_ms >= timeout_ms
}

pub fn heartbeat_ages(
    workers_dir: &Path,
    brains: &[String],
    now: SystemTime,
) -> Vec<(String, Duration)> {
    let mut out = Vec::new();
    for brain in brains {
        let p = workers_dir.join(format!("heartbeat_{brain}.json"));
        if let Ok(meta) = fs::metadata(&p) {
            if let Ok(m) = meta.modified() {
                if let Ok(age) = now.duration_since(m) {
                    out.push((brain.clone(), age));
                }
            }
        }
    }
    out
}

/// Formatiert `SystemTime` als RFC3339-Zeitstempel (fuer `cooldown_until`).
pub fn format_rfc3339(t: SystemTime) -> Option<String> {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64;
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|o| o.format(&Rfc3339).ok())
}

/// Parst einen RFC3339-Zeitstempel zurueck zu `SystemTime` (fuer Cooldown-Vergleich).
pub fn parse_rfc3339(s: &str) -> Option<SystemTime> {
    let secs = OffsetDateTime::parse(s, &Rfc3339).ok()?.unix_timestamp();
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs.max(0) as u64))
}

/// Auto-Recovery-Auswahl (rein, browser-frei testbar): liefert die Brains, die
/// von `unavailable` wieder auf `available` reflaggt werden sollen, weil seit
/// `updated_at` mehr als `retry_after` vergangen ist. Filtert strikt auf
/// `STATUS_UNAVAILABLE`: dauerhaft ausgemusterte Brains (`STATUS_RETIRED`,
/// nach `MAX_FAILED_RESTORES` fehlgeschlagenen Restores) werden NIE
/// wiederbelebt — Retirement ist final; nur manuelles Reflag via
/// `pool_control.json` (`reflag`/`reflag_all`) hebt es auf.
pub fn select_auto_recovery(
    state: &PoolState,
    now: OffsetDateTime,
    retry_after: Duration,
) -> Vec<String> {
    state
        .entries
        .iter()
        .filter(|(_, e)| e.status == STATUS_UNAVAILABLE)
        .filter_map(|(b, e)| {
            let updated = OffsetDateTime::parse(e.updated_at.as_str(), &Rfc3339).ok()?;
            if now - updated > retry_after {
                Some(b.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Liefert die Brains, deren Cooldown abgelaufen ist und die zurück auf
/// `available` gehören.
///
/// Ohne diesen Schritt ist der Cooldown eine Einbahnstraße: `select_auto_recovery`
/// filtert strikt auf `unavailable`, `reset_orphaned_active` nur auf `active` —
/// ein Brain im Cooldown wird von keinem Pfad je wieder freigegeben. Real am
/// 2026-07-27 beobachtet: eine Failover-Kaskade am 25./26.07. hatte ALLE acht
/// Brains nacheinander in den Cooldown geschickt ("blocked: reserve promoted"),
/// die Sperren waren seit über zwei Tagen abgelaufen, und der Pool stand
/// trotzdem still — es gab kein `available` mehr, aus dem er hätte starten
/// können. Ein Deadlock, den nur manuelles Reflag aufgelöst hätte.
///
/// `retired` bleibt bewusst unangetastet: Ausmusterung ist final.
pub fn select_expired_cooldowns(state: &PoolState, now: OffsetDateTime) -> Vec<String> {
    state
        .entries
        .iter()
        .filter(|(_, e)| e.status == STATUS_COOLDOWN)
        .filter_map(|(b, e)| {
            match e.cooldown_until.as_deref() {
                // Cooldown ohne Ablaufzeitpunkt kann nie ablaufen — das ist ein
                // kaputter Eintrag, keine gültige Sperre. Freigeben.
                None => Some(b.clone()),
                Some(until) => {
                    let t = OffsetDateTime::parse(until, &Rfc3339).ok()?;
                    (now >= t).then(|| b.clone())
                }
            }
        })
        .collect()
}

/// Setzt verwaiste `active`-Einträge (kein laufender Kindprozess in `running`)
/// auf `available` zurück. Wird pro Tick angewandt, damit der Pool nach einem
/// Supervisor-Restart nicht leer bleibt (alte `pool_state` listet Brains als
/// `active`, obwohl keine Worker mehr laufen).
pub fn reset_orphaned_active(state: &mut PoolState, running: &HashSet<String>) {
    for e in state.entries.values_mut() {
        if e.status == STATUS_ACTIVE && !running.contains(&e.brain) {
            e.status = STATUS_AVAILABLE.to_string();
            e.last_error = "orphaned active -> available".to_string();
            e.updated_at = crate::now_rfc3339();
        }
    }
}

/// Liefert nur die Brains, die ein (Login-)Profil besitzen — sonst hat der
/// gespawnte Worker nichts zum Arbeiten.
pub fn candidates_with_profile(brains: &[String]) -> Vec<String> {
    brains.iter().filter(|b| has_profile(b)).cloned().collect()
}

fn has_profile(brain: &str) -> bool {
    use crate::config::{profiles_dir, reference_profile_dir};
    has_profile_in(&profiles_dir(), brain) || has_profile_in(&reference_profile_dir(brain), brain)
}

/// Prüft, ob `base/<brain>` ein Verzeichnis ist (testbar mit Temp-Basis).
pub fn has_profile_in(base: &Path, brain: &str) -> bool {
    base.join(brain).is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime};

    use crate::pool_state::{
        PoolState, STATUS_ACTIVE, STATUS_AVAILABLE, STATUS_COOLDOWN, STATUS_RETIRED,
        STATUS_UNAVAILABLE,
    };
    use crate::worker_pool::MAX_FAILED_RESTORES;

    fn tmp_dir() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "test_wpool_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    /// Test-Helfer: baut einen `BreakerSnapshot`.
    fn snap(brain: &str, open: bool) -> BreakerSnapshot {
        BreakerSnapshot {
            brain_id: brain.to_string(),
            open,
            consecutive_failures: if open { 3 } else { 0 },
            open_until: if open { Some(1) } else { None },
            remaining_secs: if open { Some(1) } else { None },
            last_reason: if open { Some("blocked".into()) } else { None },
            message_blocks: 0,
            last_message_block_at: None,
            message_window_secs: None,
        }
    }

    #[test]
    fn expired_cooldown_is_released_but_retired_stays_retired() {
        use time::macros::datetime;
        let now = datetime!(2026-07-27 12:00:00 UTC);
        let mut state = PoolState::default();

        state.set("abgelaufen", STATUS_COOLDOWN, "blocked");
        state.entries.get_mut("abgelaufen").unwrap().cooldown_until =
            Some("2026-07-26T01:07:41Z".to_string());

        state.set("laeuft_noch", STATUS_COOLDOWN, "blocked");
        state.entries.get_mut("laeuft_noch").unwrap().cooldown_until =
            Some("2026-07-27T12:30:00Z".to_string());

        // Cooldown ohne Ablaufzeitpunkt kann nie ablaufen -> kaputt, freigeben.
        state.set("ohne_frist", STATUS_COOLDOWN, "blocked");

        state.set("ausgemustert", STATUS_RETIRED, "3 restores failed");
        state.set("laeuft", STATUS_ACTIVE, "");

        let mut got = select_expired_cooldowns(&state, now);
        got.sort();
        assert_eq!(
            got,
            vec!["abgelaufen".to_string(), "ohne_frist".to_string()]
        );
    }

    #[test]
    fn full_cooldown_cascade_does_not_deadlock_the_pool() {
        // Genau der Zustand vom 2026-07-27: ALLE Brains im Cooldown, alle
        // Sperren abgelaufen. Vorher gab es kein `available` mehr und der Pool
        // stand still, bis jemand von Hand reflaggte.
        use time::macros::datetime;
        let now = datetime!(2026-07-27 12:00:00 UTC);
        let brains = ["chatgpt", "claude", "deepseek", "gemini"];
        let mut state = PoolState::default();
        for b in brains {
            state.set(b, STATUS_COOLDOWN, "blocked: reserve promoted");
            state.entries.get_mut(b).unwrap().cooldown_until =
                Some("2026-07-25T22:35:09Z".to_string());
        }
        let released = select_expired_cooldowns(&state, now);
        assert_eq!(released.len(), brains.len(), "alle vier muessen zurueck");

        for b in &released {
            state.set(b, STATUS_AVAILABLE, "cooldown expired");
        }
        let candidates: Vec<String> = brains.iter().map(|s| s.to_string()).collect();
        assert!(
            select_to_promote(&candidates, &state, &HashSet::new()).is_some(),
            "nach Freigabe muss wieder ein Brain promotierbar sein"
        );
    }

    #[test]
    fn select_to_promote_picks_available_not_running() {
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let mut state = PoolState::default();
        state.set("a", STATUS_ACTIVE, "");
        state.set("b", STATUS_AVAILABLE, "");
        state.set("c", STATUS_UNAVAILABLE, "boom");
        let running: HashSet<String> = ["a".to_string()].into_iter().collect();

        assert_eq!(
            select_to_promote(&candidates, &state, &running),
            Some("b".to_string())
        );
    }

    #[test]
    fn select_to_promote_skips_when_all_active_or_unavailable() {
        let candidates = vec!["a".into(), "b".into()];
        let mut state = PoolState::default();
        state.set("a", STATUS_ACTIVE, "");
        state.set("b", STATUS_UNAVAILABLE, "boom");
        let running: HashSet<String> = ["a".to_string()].into_iter().collect();
        assert_eq!(select_to_promote(&candidates, &state, &running), None);
    }

    #[test]
    fn failover_promotes_next_reserve() {
        // a ist unavailable (ausgefallen) -> nächster available (b) wird gewählt.
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let mut state = PoolState::default();
        state.set("a", STATUS_UNAVAILABLE, "exit code 1");
        state.set("b", STATUS_AVAILABLE, "");
        state.set("c", STATUS_AVAILABLE, "");
        let running = HashSet::new();
        assert_eq!(
            select_to_promote(&candidates, &state, &running),
            Some("b".to_string())
        );
    }

    #[test]
    fn reset_orphaned_active_clears_stale_active() {
        // Nach einem `taskkill` des Pools listet die alte `pool_state` alle
        // Brains als `active`, obwohl kein Worker läuft -> sie müssen auf
        // `available` zurückgesetzt werden, sonst würden sie nie re-promoviert.
        let mut state = PoolState::default();
        state.set("a", STATUS_ACTIVE, "");
        state.set("b", STATUS_ACTIVE, "");
        state.set("c", STATUS_UNAVAILABLE, "boom");
        let running = HashSet::new();

        reset_orphaned_active(&mut state, &running);

        assert_eq!(state.entries["a"].status, STATUS_AVAILABLE);
        assert_eq!(state.entries["b"].status, STATUS_AVAILABLE);
        // Nicht-aktive bleiben unverändert.
        assert_eq!(state.entries["c"].status, STATUS_UNAVAILABLE);
    }

    #[test]
    fn reset_orphaned_active_keeps_running_active() {
        // Ein `active` Brain mit laufendem Kindprozess bleibt `active`.
        let mut state = PoolState::default();
        state.set("a", STATUS_ACTIVE, "");
        let running: HashSet<String> = ["a".to_string()].into_iter().collect();

        reset_orphaned_active(&mut state, &running);

        assert_eq!(state.entries["a"].status, STATUS_ACTIVE);
    }

    #[test]
    fn candidates_with_profile_filters_missing() {
        let base = tmp_dir();
        fs::create_dir_all(base.join("has_profile")).unwrap();
        // "has_profile" existiert, "no_profile" nicht.
        assert!(has_profile_in(&base, "has_profile"));
        assert!(!has_profile_in(&base, "no_profile"));
        let filtered =
            candidates_with_profile_using(&base, &["has_profile".into(), "no_profile".into()]);
        assert_eq!(filtered, vec!["has_profile".to_string()]);
    }

    /// Hilfsfunktion: filtert gegen eine explizite Basis (statt config::profiles_dir).
    fn candidates_with_profile_using(base: &Path, brains: &[String]) -> Vec<String> {
        brains
            .iter()
            .filter(|b| has_profile_in(base, b))
            .cloned()
            .collect()
    }

    #[test]
    fn detect_blocked_by_stale_or_breaker_open() {
        // (a) age > stale ODER breaker.open -> blocked.
        let running = vec![
            ("a".to_string(), Duration::from_secs(400)), // stale
            ("b".to_string(), Duration::from_secs(10)),  // frisch
        ];
        let closed = vec![snap("a", false), snap("b", false)];
        assert_eq!(
            detect_blocked(&running, &closed, Duration::from_secs(300)),
            vec!["a".to_string()]
        );

        // Breaker offen fuer b zieht b rein, obwohl frisch.
        let open_b = vec![snap("a", false), snap("b", true)];
        let running2 = vec![("b".to_string(), Duration::from_secs(10))];
        assert_eq!(
            detect_blocked(&running2, &open_b, Duration::from_secs(300)),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn detect_blocked_idle_not_blocked() {
        // (e) Idle (Heartbeat-Alter 0) ist nicht blockiert.
        let running = vec![("a".to_string(), Duration::from_secs(0))];
        let closed = vec![snap("a", false)];
        assert!(detect_blocked(&running, &closed, Duration::from_secs(300)).is_empty());
    }

    #[test]
    fn detect_blocked_normal_triggers_nothing() {
        // (d) Normbetrieb: frisch + breaker zu -> keine Meldung.
        let running = vec![
            ("a".to_string(), Duration::from_secs(1)),
            ("b".to_string(), Duration::from_secs(2)),
        ];
        let closed = vec![snap("a", false), snap("b", false)];
        assert!(detect_blocked(&running, &closed, Duration::from_secs(300)).is_empty());
    }

    #[test]
    fn heartbeat_ages_reads_file_mtime() {
        let dir = tmp_dir();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("heartbeat_a.json"), b"{}").unwrap();
        fs::write(dir.join("heartbeat_b.json"), b"{}").unwrap();
        // c hat keine Datei -> wird uebersprungen.
        let ages = heartbeat_ages(
            &dir,
            &["a".into(), "b".into(), "c".into()],
            SystemTime::now(),
        );
        let mut map: HashMap<String, Duration> = ages.into_iter().collect();
        assert_eq!(map.len(), 2);
        assert!(map.remove("a").unwrap() < Duration::from_secs(5));
        assert!(map.remove("b").unwrap() < Duration::from_secs(5));
    }

    #[test]
    fn is_worker_stale_returns_true_when_timeout_exceeded() {
        // last_heartbeat_ms = 1000, now_ms = 5000, timeout_ms = 3000 -> true
        assert!(is_worker_stale(1000, 5000, 3000));
    }

    #[test]
    fn is_worker_stale_returns_false_when_timeout_not_exceeded() {
        // last_heartbeat_ms = 3000, now_ms = 5000, timeout_ms = 3000 -> false
        assert!(!is_worker_stale(3000, 5000, 3000));
    }

    #[test]
    fn is_worker_stale_returns_false_when_heartbeat_exactly_now() {
        // last_heartbeat_ms = 5000, now_ms = 5000, timeout_ms = 1000 -> false
        assert!(!is_worker_stale(5000, 5000, 1000));
    }

    #[test]
    fn is_worker_stale_returns_false_when_heartbeat_in_future() {
        // last_heartbeat_ms = 9000, now_ms = 8000, timeout_ms = 1000 -> false
        assert!(!is_worker_stale(9000, 8000, 1000));
    }

    #[test]
    fn failover_block_promotes_different_reserve_and_keeps_slot_count() {
        // (b) Reserve ist ein ANDERES verfuegbares Brain; Slot-Zahl bleibt bei `active`.
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let mut state = PoolState::default();
        state.set("a", STATUS_ACTIVE, "");
        state.set("b", STATUS_AVAILABLE, "");
        state.set("c", STATUS_AVAILABLE, "");
        let running: HashSet<String> = ["a".to_string()].into_iter().collect();
        let pids: HashMap<String, u32> = [("a".to_string(), 1234)].into_iter().collect();
        let mut failover: HashMap<String, FailoverRecord> = HashMap::new();

        let acts = compute_block_failover(
            SystemTime::now(),
            &["a".to_string()],
            &candidates,
            &mut state,
            &running,
            &pids,
            &mut failover,
        );

        assert_eq!(acts.spawn, vec!["b".to_string()]); // Reserve != a
        assert_eq!(acts.kill, vec!["a".to_string()]);
        assert_eq!(state.entries["a"].status, STATUS_COOLDOWN);
        assert_eq!(state.entries["a"].replaced_by.as_deref(), Some("b"));
        assert_eq!(state.entries["b"].status, STATUS_ACTIVE);
        assert_eq!(failover["a"].standby.as_deref(), Some("b"));
        assert_eq!(failover["a"].original_pid, Some(1234));

        // Simuliere tick(): a killen, b spawnen, dann Promote-Loop bis active=2.
        // Cooldown-Brain 'a' wird vom Promote-Loop ausgeschlossen (nicht available).
        let mut children: HashSet<String> = running.clone();
        for k in &acts.kill {
            children.remove(k);
        }
        for s in &acts.spawn {
            children.insert(s.clone());
        }
        let active = 2usize;
        while children.len() < active {
            let run: HashSet<String> = children.clone();
            match select_to_promote(&candidates, &state, &run) {
                Some(b) => {
                    children.insert(b.clone());
                    state.set(&b, STATUS_ACTIVE, "");
                }
                None => break,
            }
        }
        assert_eq!(children.len(), active, "Slot-Zahl bleibt bei active");
        assert!(children.contains("b"));
        assert!(children.contains("c"));
        assert!(!children.contains("a"));
    }

    #[test]
    fn failover_cooldown_then_restore_repromotes_original_and_retires_standby() {
        // (c) Cooldown abgelaufen -> Original re-promoted, Standby eingezogen.
        let mut state = PoolState::default();
        state.set("a", STATUS_COOLDOWN, "blocked");
        state.set("b", STATUS_ACTIVE, "standby");
        state.set("c", STATUS_AVAILABLE, "");
        let past = SystemTime::now() - Duration::from_secs(10);
        let cooldown_until = format_rfc3339(past).unwrap();
        let mut failover: HashMap<String, FailoverRecord> = HashMap::new();
        failover.insert(
            "a".to_string(),
            FailoverRecord {
                phase: FailoverPhase::Blocked,
                detected_at: crate::now_rfc3339(),
                cooldown_until: Some(cooldown_until),
                standby: Some("b".to_string()),
                original_pid: Some(1234),
                run_id: None,
                reason: "blocked".into(),
                failover_count: 0,
            },
        );

        let acts = compute_restore(
            SystemTime::now(),
            &mut failover,
            &mut state,
            3,
            |_| true, // Restore gelingt
        );

        assert_eq!(acts.spawn, vec!["a".to_string()]); // Original re-promoted
        assert_eq!(acts.retire, vec!["b".to_string()]); // Standby eingezogen
        assert!(acts.retired.is_empty());
        assert_eq!(state.entries["a"].status, STATUS_ACTIVE); // wieder aktiv
        assert_eq!(state.entries["b"].status, STATUS_AVAILABLE); // Standby freigegeben
        assert!(!failover.contains_key("a")); // Failover abgeschlossen
    }

    #[test]
    fn failover_no_double_flag_for_cooldown_brain() {
        // (f) Ein bereits im Cooldown befindliches Brain wird nicht erneut geflaggt.
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let mut state = PoolState::default();
        state.set("a", STATUS_COOLDOWN, "blocked");
        state.set("b", STATUS_ACTIVE, "standby");
        state.set("c", STATUS_AVAILABLE, "");
        let future = SystemTime::now() + Duration::from_secs(1000);
        let cooldown_until = format_rfc3339(future).unwrap();
        let mut failover: HashMap<String, FailoverRecord> = HashMap::new();
        failover.insert(
            "a".to_string(),
            FailoverRecord {
                phase: FailoverPhase::Blocked,
                detected_at: crate::now_rfc3339(),
                cooldown_until: Some(cooldown_until),
                standby: Some("b".to_string()),
                original_pid: Some(1),
                run_id: None,
                reason: "blocked".into(),
                failover_count: 0,
            },
        );
        let running: HashSet<String> = ["a".to_string()].into_iter().collect();
        let pids: HashMap<String, u32> = [("a".to_string(), 1)].into_iter().collect();

        let acts = compute_block_failover(
            SystemTime::now(),
            &["a".to_string()],
            &candidates,
            &mut state,
            &running,
            &pids,
            &mut failover,
        );
        assert!(acts.kill.is_empty());
        assert!(acts.spawn.is_empty());
        assert_eq!(failover["a"].failover_count, 0);
        assert_eq!(state.entries["a"].status, STATUS_COOLDOWN);
    }

    #[test]
    fn failover_retries_on_failed_restore_before_k() {
        // Restore scheitert, aber zaehler < K -> Cooldown verlaengert, erneut versuchen.
        let mut state = PoolState::default();
        state.set("a", STATUS_COOLDOWN, "blocked");
        state.set("b", STATUS_ACTIVE, "standby");
        let past = SystemTime::now() - Duration::from_secs(10);
        let cooldown_until = format_rfc3339(past).unwrap();
        let mut failover: HashMap<String, FailoverRecord> = HashMap::new();
        failover.insert(
            "a".to_string(),
            FailoverRecord {
                phase: FailoverPhase::Blocked,
                detected_at: crate::now_rfc3339(),
                cooldown_until: Some(cooldown_until),
                standby: Some("b".to_string()),
                original_pid: Some(1),
                run_id: None,
                reason: "blocked".into(),
                failover_count: 0,
            },
        );

        let acts = compute_restore(
            SystemTime::now(),
            &mut failover,
            &mut state,
            3,
            |_| false, // Restore scheitert
        );

        assert!(acts.spawn.is_empty());
        assert!(acts.retire.is_empty());
        assert!(acts.retired.is_empty());
        // Noch im Failover, Zaehler hochgezaehlt, Cooldown in die Zukunft verlaengert.
        assert!(failover.contains_key("a"));
        assert_eq!(failover["a"].failover_count, 1);
        let next = failover["a"]
            .cooldown_until
            .as_ref()
            .and_then(|s| parse_rfc3339(s))
            .unwrap();
        assert!(next > SystemTime::now());
    }

    #[test]
    fn failover_retires_after_k_failed_restores() {
        // Zaehler erreicht K -> Original unavailable (Retired), kein Retry mehr.
        let mut state = PoolState::default();
        state.set("a", STATUS_COOLDOWN, "blocked");
        state.set("b", STATUS_ACTIVE, "standby");
        let past = SystemTime::now() - Duration::from_secs(10);
        let cooldown_until = format_rfc3339(past).unwrap();
        let mut failover: HashMap<String, FailoverRecord> = HashMap::new();
        failover.insert(
            "a".to_string(),
            FailoverRecord {
                phase: FailoverPhase::Blocked,
                detected_at: crate::now_rfc3339(),
                cooldown_until: Some(cooldown_until),
                standby: Some("b".to_string()),
                original_pid: Some(1),
                run_id: None,
                reason: "blocked".into(),
                failover_count: MAX_FAILED_RESTORES - 1, // einer vor dem Limit
            },
        );

        let acts = compute_restore(
            SystemTime::now(),
            &mut failover,
            &mut state,
            MAX_FAILED_RESTORES,
            |_| false, // Restore scheitert erneut
        );

        assert_eq!(acts.retired, vec!["a".to_string()]);
        // Dauerhaft retired — NICHT unavailable, sonst wuerde die
        // Auto-Recovery das Brain nach der Retry-Frist wiederbeleben.
        assert_eq!(state.entries["a"].status, STATUS_RETIRED);
        assert!(!failover.contains_key("a"));
    }

    #[test]
    fn auto_recovery_recovers_transient_unavailable_after_window() {
        // (b) Ein transient `unavailable` Brain wird nach Ablauf der
        // Retry-Frist wieder zur Recovery ausgewaehlt — vorher nicht.
        let mut state = PoolState::default();
        state.set("a", STATUS_UNAVAILABLE, "exit code 1");
        let retry_after = Duration::from_secs(120);
        let now = OffsetDateTime::now_utc();

        // Frist noch nicht abgelaufen -> nichts.
        assert!(select_auto_recovery(&state, now, retry_after).is_empty());

        // Frist abgelaufen -> a wird wiederbelebt.
        let later = now + Duration::from_secs(121);
        assert_eq!(
            select_auto_recovery(&state, later, retry_after),
            vec!["a".to_string()]
        );
    }

    #[test]
    fn auto_recovery_never_resurrects_retired_brain() {
        // (a) Regression: ein nach MAX_FAILED_RESTORES dauerhaft retired Brain
        // darf auch lange nach Ablauf der Retry-Frist NICHT wiederbelebt
        // werden — "permanent" muss permanent bleiben. Nur das transient
        // unavailable Brain wird recovered.
        let mut state = PoolState::default();
        state.set("a", STATUS_RETIRED, "retired after 3 failed restores");
        state.set("b", STATUS_UNAVAILABLE, "exit code 1");
        let later = OffsetDateTime::now_utc() + Duration::from_secs(1_000_000);

        let recovered = select_auto_recovery(&state, later, Duration::from_secs(120));

        assert_eq!(recovered, vec!["b".to_string()]);
        // Der State des retired Brains bleibt unangetastet.
        assert_eq!(state.entries["a"].status, STATUS_RETIRED);
    }
}
