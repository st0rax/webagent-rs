//! worker_pool — Supervisor des Worker-Pools (Kern).
//!
//! Supervidiert N aktive `bot2bot-worker` (je ein eigener Kindprozess) aus einem
//! Pool verfügbarer Brains. Fällt ein aktiver Worker aus (Exit != 0 / Crash), wird
//! das Brain als `unavailable` markiert und der nächste verfügbare Reserve-Brain
//! promoviert.
//!
//! Architektur: **Prozess-Spawn** (kein in-process Thread). Jeder Worker isoliert
//! sein Browser-Profil bereits pro Prozess (Q5 in `bot2bot_worker.rs`), daher ist
//! der Kindprozess die natürliche Isolationsgrenze: ein WebView2-Crash / OOM in
//! einem Worker reißt die Geschwister nicht mit, Failover + Profil-Cleanup sind
//! sauber pro Prozess. Der Supervisor überwacht Kind-PIDs via `Child::try_wait()`.
//!
//! Refactoring 06:20: Persistenz/Datei-IPC liegt in `pool_state.rs`, die reine
//! Failover-Entscheidungslogik in `pool_failover.rs`; dieser Kern bleibt der
//! Prozess-Supervisor. Die alten Pfade `crate::worker_pool::…` bleiben per
//! Re-Export gueltig (brains.rs/main.rs/tui.rs/tui_ansi.rs/tui_load.rs).

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime};

use time::OffsetDateTime;

pub use crate::pool_failover::{
    candidates_with_profile, compute_block_failover, compute_restore, detect_blocked,
    has_profile_in, heartbeat_ages, is_worker_stale, reset_orphaned_active, select_auto_recovery,
    select_expired_cooldowns, select_to_promote, BlockActions, FailoverPhase, FailoverRecord,
    RestoreActions,
};
pub(crate) use crate::pool_state::atomic_write;
pub use crate::pool_state::{
    PoolControl, PoolEntry, PoolState, BLOCK_COOLDOWN_SECS, RETRY_UNAVAILABLE_AFTER_SECS,
    STATUS_ACTIVE, STATUS_AVAILABLE, STATUS_COOLDOWN, STATUS_RETIRED, STATUS_UNAVAILABLE,
};

/// Maximale Anzahl aufeinanderfolgend fehlgeschlagener Wiederherstellungen, bevor
/// ein BLOCK-Brain als dauerhaft `retired` markiert wird (kein Retry).
pub(crate) const MAX_FAILED_RESTORES: u32 = 3;

/// Supervisor-Konfiguration.
pub struct WorkerPool {
    candidates: Vec<String>,
    active: usize,
    poll_secs: u64,
    headless: bool,
    state_path: PathBuf,
    control_path: PathBuf,
    children: HashMap<String, Child>,
    /// Ende der begrenzten Anlaufzeit je frisch gestartetem Worker.
    startup_grace_until: HashMap<String, SystemTime>,
    /// Laufende BLOCK-Failover pro Brain (Cooldown + Restore-Buchhaltung).
    failover: HashMap<String, FailoverRecord>,
}

impl WorkerPool {
    pub fn new(
        candidates: Vec<String>,
        active: usize,
        poll_secs: u64,
        headless: bool,
        state_path: PathBuf,
        control_path: PathBuf,
    ) -> Self {
        Self {
            candidates,
            active,
            poll_secs,
            headless,
            state_path,
            control_path,
            children: HashMap::new(),
            startup_grace_until: HashMap::new(),
            failover: HashMap::new(),
        }
    }

    /// Startet einen Kindprozess pro Worker (re-exec des eigenen Binaries mit
    /// dem `bot2bot-worker`-Subcommand).
    /// Beendet alle laufenden Kindprozesse.
    ///
    /// Wird sowohl beim geordneten Herunterfahren als auch aus `Drop` gerufen,
    /// damit kein Weg am Aufräumen vorbeiführt.
    /// Ein Browserprozess braucht nach dem Spawn mehrere Poll-Intervalle bis
    /// zum ersten Heartbeat. Die Grace verhindert nur einen falschen
    /// `stale`-Failover waehrend dieses Fensters; ein offener Circuit-Breaker
    /// bleibt weiterhin ein sofortiges Block-Signal.
    fn startup_grace(poll_secs: u64) -> Duration {
        Duration::from_secs(poll_secs.saturating_mul(3).clamp(30, 120))
    }

    fn heartbeat_age_during_startup(
        startup_grace_until: Option<&SystemTime>,
        now: SystemTime,
        heartbeat_age: Duration,
    ) -> Duration {
        if startup_grace_until.is_some_and(|until| *until > now) {
            Duration::ZERO
        } else {
            heartbeat_age
        }
    }

    fn mark_worker_spawned(&mut self, brain: &str) {
        self.startup_grace_until.insert(
            brain.to_string(),
            SystemTime::now() + Self::startup_grace(self.poll_secs),
        );
    }

    pub fn kill_all_children(&mut self) -> usize {
        let mut n = 0;
        for (brain, mut child) in self.children.drain() {
            if child.kill().is_ok() {
                n += 1;
                crate::bench_events::eprint_line(&format!(
                    "[worker_pool] Worker '{brain}' beendet"
                ));
            }
            let _ = child.wait();
        }
        n
    }

    fn spawn_worker(brain: &str, poll_secs: u64, headless: bool) -> std::io::Result<Child> {
        let exe = std::env::current_exe()?;
        let log_dir = crate::config::data_dir().join("logs");
        fs::create_dir_all(&log_dir)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("worker_{brain}.log")))?;
        let stderr = log.try_clone()?;
        let mut cmd = Command::new(exe);
        cmd.arg("bot2bot-worker")
            .arg("--brain")
            .arg(brain)
            .arg("--poll-secs")
            .arg(poll_secs.to_string())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(stderr));
        if headless {
            cmd.arg("--headless");
        }
        let child = cmd.spawn()?;
        assign_to_kill_on_exit_job(&child);
        Ok(child)
    }

    /// Erntet tote Kindprozesse: beendete Worker werden aus `children` entfernt
    /// und im State markiert (`available` bei sauberem Exit, `unavailable` bei
    /// Fehler/Crash).
    fn reap(children: &mut HashMap<String, Child>, state: &mut PoolState) {
        let brains: Vec<String> = children.keys().cloned().collect();
        for b in brains {
            let result = match children.get_mut(&b) {
                Some(c) => c.try_wait(),
                None => continue,
            };
            match result {
                // Läuft noch -> unverändert.
                Ok(None) => {}
                // Beendet: Status je nach Exit-Code setzen.
                Ok(Some(code)) => {
                    let _ = children.remove(&b);
                    if code.success() {
                        state.set(&b, STATUS_AVAILABLE, "exited cleanly");
                    } else {
                        state.set(&b, STATUS_UNAVAILABLE, &format!("exit code {code}"));
                    }
                }
                // Warte-Fehler -> als Crash werten.
                Err(e) => {
                    let _ = children.remove(&b);
                    state.set(&b, STATUS_UNAVAILABLE, &format!("wait error: {e}"));
                }
            }
        }
    }

    /// Ein einzelner Supervisor-Tick: Steuerung anwenden, tote Worker ernten,
    /// bis `active` Worker promovieren, State speichern. Reine Fortschreibung —
    /// keine Schleife (wird von `run()` oder der TUI getaktet).
    pub fn tick(&mut self, control: &PoolControl) {
        // Steuerbefehle anwenden.
        if let Some(target_active) = control.target_active {
            self.active = target_active;
        }

        let mut state = PoolState::load_or_init(&self.state_path, &self.candidates);

        if control.reflag_all {
            for b in &self.candidates {
                state.set(b, STATUS_AVAILABLE, "");
            }
        } else if !control.reflag.is_empty() {
            for b in &control.reflag {
                state.set(b, STATUS_AVAILABLE, "");
            }
        }

        Self::reap(&mut self.children, &mut state);

        // Orphan-Reset (robust gegen Supervisor-Restart): ein als `active`
        // markiertes Brain ohne laufenden Kindprozess ist verwaist (z.B. nach
        // einem `taskkill` des gesamten Pools) -> wieder `available` setzen,
        // damit die Promote-Schleife es neu startet. Ohne das bleibt der Pool
        // nach einem Restart leer, weil `select_to_promote` nur `available`
        // Brains promoviert.
        let running: HashSet<String> = self.children.keys().cloned().collect();
        reset_orphaned_active(&mut state, &running);

        // Auto-Recovery: transient `unavailable` Brains nach Ablauf der
        // Retry-Frist wieder available setzen, damit der Promote-Loop sie neu
        // startet. Dauerhaft ausgemusterte Brains (`STATUS_RETIRED`) sind
        // bewusst ausgeschlossen — siehe `select_auto_recovery`.
        {
            let retry_after = Duration::from_secs(crate::config::retry_unavailable_secs());
            let candidate_recovery =
                select_auto_recovery(&state, OffsetDateTime::now_utc(), retry_after);
            for b in &candidate_recovery {
                crate::bench_events::eprint_line(&format!(
                    "[worker_pool] Auto-Recovery: {} wieder available (unavailable > {}s)",
                    b,
                    retry_after.as_secs()
                ));
                state.set(b, STATUS_AVAILABLE, "auto-recovery after retry timeout");
            }
        }

        // Abgelaufene Cooldowns freigeben. Ohne das ist der Cooldown eine
        // Einbahnstrasse und eine Failover-Kaskade legt den Pool dauerhaft
        // stillt — siehe `select_expired_cooldowns`.
        for b in select_expired_cooldowns(&state, OffsetDateTime::now_utc()) {
            crate::bench_events::eprint_line(&format!(
                "[worker_pool] Cooldown abgelaufen: {b} wieder available"
            ));
            state.set(&b, STATUS_AVAILABLE, "cooldown expired");
            if let Some(e) = state.entries.get_mut(&b) {
                e.cooldown_until = None;
                e.replaced_by = None;
            }
        }

        // Scale-down: zu viele laufende Worker sauber beenden (fuer TUI '-').
        while self.children.len() > self.active {
            if let Some((b, mut c)) = self.children.drain().next() {
                let _ = c.kill();
                state.set(&b, STATUS_AVAILABLE, "scaled down");
            }
        }

        // --- BLOCK-Failover (Reserve-Promotion + Cooldown/Restore) ---
        // Ergaenzt den Crash-Failover (reap via Child::try_wait): erkennt einen
        // ALIVE, aber BLOCKED/HUNG Worker (kein Fortschritt, Deadlock, natives
        // Modal, eingefrorener WebView) und ersetzt ihn durch ein Reserve-Brain,
        // bis das Original nach Cooldown frisch wiederhergestellt wird.
        let now = SystemTime::now();
        let stale = Duration::from_secs(crate::config::stale_heartbeat_secs());

        // Signal A (zukunftssicher): Circuit-Breaker-Snapshots — ein offener
        // Breaker fuer ein aktives Brain signalisiert Block. (Workers fuettern
        // den Breaker derzeit noch nicht; daher in der Praxis meist `open ==
        // false`.) Die Logik ist dennoch verdrahtet, damit sie greift, sobald
        // Worker den Breaker befuettern.
        let snaps = crate::circuit_breaker::snapshots();

        // Signal B (verdrahtet): Heartbeat-Alter der laufenden Worker ueber das
        // Aenderungsdatum der `heartbeat_<brain>.json`. Idle, aber pollende
        // Worker schreiben regelmaessig -> frisch -> nicht blockiert (idle-sicher).
        let hb_dir = self.control_path.parent().map(|p| p.to_path_buf());
        let running_set: HashSet<String> = self.children.keys().cloned().collect();
        let running_ages: Vec<(String, Duration)> = match &hb_dir {
            Some(d) => heartbeat_ages(d, &self.candidates, now)
                .into_iter()
                .filter(|(b, _)| running_set.contains(b))
                .collect(),
            None => Vec::new(),
        };

        let running_ages: Vec<(String, Duration)> = running_ages
            .into_iter()
            .map(|(brain, age)| {
                let effective_age = Self::heartbeat_age_during_startup(
                    self.startup_grace_until.get(&brain),
                    SystemTime::now(),
                    age,
                );
                (brain, effective_age)
            })
            .collect();
        let blocked = detect_blocked(&running_ages, &snaps, stale);

        let running_pids: HashMap<String, u32> = self
            .children
            .iter()
            .map(|(b, c)| (b.clone(), c.id()))
            .collect();

        // 1) BLOCK-Erkennung -> Failover-Eintraege + Reserve-Promotion
        //    (Kill der geblockten Kinder erfolgt direkt danach).
        let block_actions = compute_block_failover(
            now,
            &blocked,
            &self.candidates,
            &mut state,
            &running_set,
            &running_pids,
            &mut self.failover,
        );

        // Geblockte Kinder sofort beenden.
        for b in &block_actions.kill {
            if let Some(mut c) = self.children.remove(b) {
                let _ = c.kill();
            }
        }

        // 2) Cooldown/Restore: abgelaufene Failover wiederherstellen. Der Closure
        //    spawned den frischen Worker real und liefert dessen Erfolg zurueck
        //    (bestimmt das Retry/Retire-Verhalten).
        let startup_grace = Self::startup_grace(self.poll_secs);
        let startup_grace_until = &mut self.startup_grace_until;
        let restore_actions = compute_restore(
            now,
            &mut self.failover,
            &mut state,
            MAX_FAILED_RESTORES,
            |brain| match Self::spawn_worker(brain, self.poll_secs, self.headless) {
                Ok(child) => {
                    self.children.insert(brain.to_string(), child);
                    startup_grace_until
                        .insert(brain.to_string(), SystemTime::now() + startup_grace);
                    true
                }
                Err(_) => false,
            },
        );

        // Reserve-Worker (Block-Ersatz) starten.
        for b in &block_actions.spawn {
            match Self::spawn_worker(b, self.poll_secs, self.headless) {
                Ok(child) => {
                    self.children.insert(b.clone(), child);
                    self.mark_worker_spawned(b);
                }
                Err(e) => {
                    state.set(b, STATUS_UNAVAILABLE, &format!("spawn failed: {e}"));
                }
            }
        }

        // Standby-Brains nach erfolgreichem Restore einziehen.
        for b in &restore_actions.retire {
            if let Some(mut c) = self.children.remove(b) {
                let _ = c.kill();
            }
        }

        // N aktive Worker sicherstellen (Failover via Promotion).
        while self.children.len() < self.active {
            let running: HashSet<String> = self.children.keys().cloned().collect();
            match select_to_promote(&self.candidates, &state, &running) {
                Some(b) => match Self::spawn_worker(&b, self.poll_secs, self.headless) {
                    Ok(child) => {
                        self.children.insert(b.clone(), child);
                        state.set(&b, STATUS_ACTIVE, "");
                    }
                    Err(e) => {
                        // Spawn fehlgeschlagen: nicht endlos retryen,
                        // als unavailable markieren.
                        state.set(&b, STATUS_UNAVAILABLE, &format!("spawn failed: {e}"));
                    }
                },
                None => break,
            }
        }

        if let Err(e) = state.save(&self.state_path) {
            crate::bench_events::eprint_line(&format!(
                "[worker_pool] Pool-State konnte nicht gespeichert werden: {e}"
            ));
        }
    }

    /// Beendet alle laufenden Kindprozesse sofort (Failover-Loop verlässt danach).
    pub fn kill_all(&mut self) {
        for (_, mut c) in self.children.drain() {
            let _ = c.kill();
        }
    }

    /// Supervisor-Loop: hält `active` Worker am Laufen, Failover bei Ausfall,
    /// reagiert auf `pool_control.json` (target_active / reflag / stop).
    ///
    /// BEKANNTE GRENZE (v2, nicht in v1): `try_wait()` erkennt nur beendete/
    /// abgestürzte Worker. Ein *hängender* Worker (Browser eingefroren, kein
    /// Exit) wird nicht erkannt -> kein Failover. v2 könnte ein Heartbeat
    /// ergänzen (Worker schreibt periodisch `last_seen` in `pool_state`;
    /// Supervisor prüft das Alter und markiert stale Worker als `unavailable`).
    pub fn run(&mut self) {
        loop {
            let control = match PoolControl::take(&self.control_path) {
                Ok(Some(control)) => control,
                Ok(None) => PoolControl::default(),
                Err(e) => {
                    crate::bench_events::eprint_line(&format!(
                        "[worker_pool] Control-Datei nicht anwendbar: {e}"
                    ));
                    PoolControl::default()
                }
            };
            self.tick(&control);
            if control.stop {
                self.kill_all();
                break;
            }
            thread::sleep(Duration::from_secs(self.poll_secs));
        }
    }
}

/// CLI-Einstiegspunkt. Wird von `main.rs` aufgerufen; der clap-Subcommand +
/// dispatch-Arm in `main.rs` ist "braucht wiring" (Claude).
pub fn run_worker_pool(active: usize, brains: &str, poll_secs: u64, headless: bool) -> i32 {
    let all = crate::config::available_brain_ids();
    let selected: Vec<String> = if brains.trim().is_empty() {
        all
    } else {
        brains
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    let candidates = candidates_with_profile(&selected);
    let root = crate::config::bot2bot_root();
    let state_path = root.join("workers").join("pool_state.json");
    let control_path = root.join("workers").join("pool_control.json");
    let mut pool = WorkerPool::new(
        candidates,
        active,
        poll_secs,
        headless,
        state_path,
        control_path,
    );
    pool.run();
    0
}

/// Windows-Job, der alle zugewiesenen Prozesse mitnimmt, sobald das letzte
/// Handle darauf schliesst — also spaetestens wenn webagent endet.
///
/// Warum ueberhaupt: `Drop` deckt nur das geordnete Ende ab. Wird das
/// Terminalfenster geschlossen, der Prozess per taskkill beendet oder stuerzt
/// er ab, laeuft kein Rust-Code mehr — die gespawnten Worker (jeder mit
/// eigenem Browser) blieben zurueck und hielten Profile und Speicher. Genau
/// solche Waisen mussten hier schon einmal von Hand eingesammelt werden.
///
/// Ein Job-Objekt mit `KILL_ON_JOB_CLOSE` loest das im Betriebssystem: das
/// Aufraeumen haengt nicht mehr daran, dass der Agent noch zum Aufraeumen
/// kommt.
#[cfg(all(windows, feature = "webview"))]
fn kill_on_exit_job() -> Option<windows::Win32::Foundation::HANDLE> {
    use std::sync::OnceLock;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    struct JobHandle(HANDLE);
    // HANDLE ist ein rohes Betriebssystem-Handle; der Job lebt bis Prozessende.
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    static JOB: OnceLock<Option<JobHandle>> = OnceLock::new();
    JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(None, None).ok()?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let ok = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .is_ok();
        if ok {
            Some(JobHandle(job))
        } else {
            None
        }
    })
    .as_ref()
    .map(|h| h.0)
}

/// Haengt einen frisch gestarteten Kindprozess in den Kill-on-Exit-Job.
#[cfg(all(windows, feature = "webview"))]
fn assign_to_kill_on_exit_job(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::AssignProcessToJobObject;

    let Some(job) = kill_on_exit_job() else {
        return;
    };
    unsafe {
        let _ = AssignProcessToJobObject(job, HANDLE(child.as_raw_handle()));
    }
}

#[cfg(not(all(windows, feature = "webview")))]
fn assign_to_kill_on_exit_job(_child: &Child) {}
/// Beim Verlassen alle Worker mitnehmen.
///
/// Der Job (`kill_on_exit_job`) faengt den harten Fall ab — Fenster zu,
/// taskkill, Absturz. `Drop` deckt den geordneten ab und beendet sauber statt
/// abrupt, damit die Worker ihren Zustand noch schreiben koennen.
impl Drop for WorkerPool {
    fn drop(&mut self) {
        let n = self.kill_all_children();
        if n > 0 {
            crate::bench_events::eprint_line(&format!(
                "[worker_pool] {n} Worker beim Beenden gestoppt"
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

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

    #[test]
    fn worker_pool_applies_explicit_scale_to_zero() {
        let dir = tmp_dir();
        let mut pool = WorkerPool::new(
            Vec::new(),
            3,
            1,
            true,
            dir.join("pool_state.json"),
            dir.join("pool_control.json"),
        );

        pool.tick(&PoolControl {
            target_active: Some(0),
            ..Default::default()
        });

        assert_eq!(pool.active, 0);
    }

    #[test]
    fn kill_all_children_is_idempotent_and_empties_the_pool() {
        // Ohne laufende Kinder darf das Aufraeumen weder zaehlen noch panicken —
        // `Drop` ruft es auf JEDEM Pfad, auch bei nie gestartetem Pool.
        let dir = tmp_dir();
        let mut pool = WorkerPool::new(
            vec!["a".into()],
            0,
            1,
            true,
            dir.join("pool_state.json"),
            dir.join("pool_control.json"),
        );
        assert_eq!(pool.kill_all_children(), 0);
        assert_eq!(pool.kill_all_children(), 0, "zweiter Aufruf bleibt harmlos");
    }

    #[test]
    fn startup_grace_masks_only_the_missing_first_heartbeat() {
        let now = SystemTime::now();
        let stale = Duration::from_secs(120);

        assert_eq!(
            WorkerPool::heartbeat_age_during_startup(
                Some(&(now + Duration::from_secs(1))),
                now,
                stale,
            ),
            Duration::ZERO,
        );
        assert_eq!(
            WorkerPool::heartbeat_age_during_startup(Some(&now), now, stale),
            stale,
        );
    }

    #[test]
    fn startup_grace_is_bounded_for_fast_and_slow_polls() {
        assert_eq!(WorkerPool::startup_grace(5), Duration::from_secs(30));
        assert_eq!(WorkerPool::startup_grace(90), Duration::from_secs(120));
    }
}
