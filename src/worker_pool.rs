//! worker_pool — Worker-Pool-Manager (Teil 1).
//!
//! Supervidiert N aktive `bot2bot-worker` (je ein eigener Kindprozess) aus einem
//! Pool verfügbarer Brains. Fällt ein aktiver Worker aus (Exit != 0 / Crash), wird
//! das Brain als `unavailable` markiert und der nächste verfügbare Reserve-Brain
//! promoviert. Health/Status pro Brain liegt in `pool_state.json`
//! (`available` | `active` | `unavailable` + `last_error`).
//!
//! Architektur: **Prozess-Spawn** (kein in-process Thread). Jeder Worker isoliert
//! sein Browser-Profil bereits pro Prozess (Q5 in `bot2bot_worker.rs`), daher ist
//! der Kindprozess die natürliche Isolationsgrenze: ein WebView2-Crash / OOM in
//! einem Worker reißt die Geschwister nicht mit, Failover + Profil-Cleanup sind
//! sauber pro Prozess. Der Supervisor überwacht Kind-PIDs via `Child::try_wait()`.
//!
//! Lane: dieses Modul neu. `config.rs`/`login.rs`/`repl.rs`/`circuit_breaker.rs`/
//! `canary.rs` nur lesen. Die Worker-Logik wird via `run_bot2bot_worker`
//! wiederverwendet (kein Duplikat).

use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::circuit_breaker::BreakerSnapshot;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// Status eines Brains im Pool.
pub const STATUS_AVAILABLE: &str = "available";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_UNAVAILABLE: &str = "unavailable";

/// Status: Brain ist ALIVE, aber als BLOCK/HANG erkannt (Deadlock, eingefrorener
/// WebView, natives Modal, keine Fortschritte). Ein Reserve-Brain uebernimmt,
/// das Original geht in den Cooldown und wird nach Ablauf frisch wiederhergestellt.
pub const STATUS_COOLDOWN: &str = "cooldown";

/// Status: Brain wurde nach `MAX_FAILED_RESTORES` fehlgeschlagenen Restores
/// dauerhaft ausgemustert (Retired). Eigener Status statt `unavailable` +
/// Reason-String, damit die Auto-Recovery (`select_auto_recovery`) das Brain
/// NICHT nach Ablauf der Retry-Frist wiederbelebt — Retirement ist final und
/// wird nur durch manuelles Reflag (pool_control `reflag`/`reflag_all`)
/// aufgehoben. Serialisiert als gewoehnlicher Status-String in
/// `pool_state.json` -> rueckwaertskompatibel (alte States kennen den Wert
/// schlicht nicht; unbekannte Status werden ueberall wie "nicht available"
/// behandelt).
pub const STATUS_RETIRED: &str = "retired";

/// Cooldown-Dauer (Sekunden) fuer ein als BLOCK erkanntes Brain, bevor es durch
/// einen frischen Worker wiederhergestellt wird. Ueberschreibbar via
/// `config::block_cooldown_secs()` (Env WEBAGENT_BLOCK_COOLDOWN_S); dieser const
/// ist die kanonische Default-Untergrenze (600 s = 10 min).
pub const BLOCK_COOLDOWN_SECS: u64 = 600;

/// Wie lange ein `unavailable` Brain wartet, bevor es automatisch wieder
/// als `available` reflaggt wird (Default 120s). Ueberschreibbar via Env
/// `config::retry_unavailable_secs()` (Env WEBAGENT_RETRY_UNAVAILABLE_S).
pub const RETRY_UNAVAILABLE_AFTER_SECS: u64 = 120;

/// Maximale Anzahl aufeinanderfolgend fehlgeschlagener Wiederherstellungen, bevor
/// ein BLOCK-Brain als dauerhaft `retired` markiert wird (kein Retry).
const MAX_FAILED_RESTORES: u32 = 3;

/// Health/Status-Eintrag pro Brain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolEntry {
    pub brain: String,
    #[serde(default = "default_available")]
    pub status: String,
    #[serde(default)]
    pub last_error: String,
    #[serde(default)]
    pub updated_at: String,
    /// RFC3339-Zeitpunkt, bis zu dem dieses Brain im Cooldown bleibt (BLOCK-Failover).
    #[serde(default)]
    pub cooldown_until: Option<String>,
    /// Brain, das dieses (Cooldown-)Brain im Failover ersetzt (Reserve).
    #[serde(default)]
    pub replaced_by: Option<String>,
    /// Letzter Heartbeat-Zeitpunkt in Millisekunden seit Unix-Epoch.
    #[serde(default)]
    pub last_heartbeat_ms: Option<u64>,
}

impl PoolEntry {
    fn available(brain: &str) -> Self {
        PoolEntry {
            brain: brain.to_string(),
            status: STATUS_AVAILABLE.to_string(),
            last_error: String::new(),
            updated_at: crate::now_rfc3339(),
            cooldown_until: None,
            replaced_by: None,
            last_heartbeat_ms: None,
        }
    }
}

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

fn default_available() -> String {
    STATUS_AVAILABLE.to_string()
}

/// Gesamter Pool-Zustand (`bot2bot_root()/workers/pool_state.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolState {
    #[serde(default)]
    pub entries: HashMap<String, PoolEntry>,
}

impl PoolState {
    /// Lädt `pool_state.json`; fehlt die Datei, wird ein leerer Default geliefert.
    pub fn load(path: &Path) -> PoolState {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(st) = serde_json::from_str::<PoolState>(&s) {
                return st;
            }
        }
        PoolState::default()
    }

    /// Lädt und stellt sicher, dass alle Kandidaten einen Eintrag haben
    /// (Default `available`).
    pub fn load_or_init(path: &Path, candidates: &[String]) -> PoolState {
        let mut st = Self::load(path);
        for b in candidates {
            st.entries
                .entry(b.clone())
                .or_insert_with(|| PoolEntry::available(b));
        }
        st
    }

    pub fn set(&mut self, brain: &str, status: &str, last_error: &str) {
        let e = self
            .entries
            .entry(brain.to_string())
            .or_insert_with(|| PoolEntry::available(brain));
        e.status = status.to_string();
        e.last_error = last_error.to_string();
        e.updated_at = crate::now_rfc3339();
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        atomic_write(path, s.as_bytes())
    }
}

/// Steuerbefehle für den laufenden Supervisor (Datei-IPC, siehe
/// `PoolControl::take`). Die TUI (Teil 2) schreibt `pool_control.json`;
/// der Supervisor liest es pro Tick und wendet target_active / reflag / stop an.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolControl {
    /// Gewünschte Anzahl aktiver Worker (überschreibt `active`).
    /// `None` = nicht ändern, `Some(0)` = alle Worker stoppen.
    #[serde(default)]
    pub target_active: Option<usize>,
    /// Alle Kandidaten auf `available` zurücksetzen (nach Fix).
    #[serde(default)]
    pub reflag_all: bool,
    /// Einzelne Brains auf `available` zurücksetzen.
    #[serde(default)]
    pub reflag: Vec<String>,
    /// Supervisor sauber beenden (Kinder killen).
    #[serde(default)]
    pub stop: bool,
}

impl PoolControl {
    /// Übernimmt `pool_control.json` per atomarem Rename und lädt den Inhalt.
    ///
    /// Ein paralleler Schreiber kann sofort die nächste Steuerdatei publizieren,
    /// ohne dass der Supervisor sie nachträglich versehentlich löscht. Ungültiges
    /// JSON bleibt als `*.invalid-*` neben der Control-Datei zur Diagnose liegen.
    pub fn take(path: &Path) -> std::io::Result<Option<PoolControl>> {
        let claimed = sibling_temp_path(path, "processing");
        match fs::rename(path, &claimed) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        }

        let parsed = fs::read_to_string(&claimed).and_then(|s| {
            serde_json::from_str::<PoolControl>(&s)
                .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))
        });

        match parsed {
            Ok(control) => {
                if let Err(e) = fs::remove_file(&claimed) {
                    crate::bench_events::eprint_line(&format!(
                        "[worker_pool] konsumierte Control-Datei konnte nicht entfernt werden \
                         ({}): {e}",
                        claimed.display()
                    ));
                }
                Ok(Some(control))
            }
            Err(e) => {
                let invalid = sibling_temp_path(path, "invalid");
                let retained = match fs::rename(&claimed, &invalid) {
                    Ok(()) => invalid,
                    Err(_) => claimed,
                };
                Err(std::io::Error::new(
                    e.kind(),
                    format!(
                        "{}; ungültige Control-Datei behalten: {}",
                        e,
                        retained.display()
                    ),
                ))
            }
        }
    }
}

/// Schreibt eine Datei vollständig in eine gleichgeordnete temporäre Datei und
/// veröffentlicht sie anschließend per atomarem Replace.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    let tmp = sibling_temp_path(path, "tmp");
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&tmp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn sibling_temp_path(path: &Path, purpose: &str) -> PathBuf {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
    let seq = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pool");
    path.with_file_name(format!(".{name}.{purpose}-{}-{seq}", std::process::id()))
}

#[cfg(not(windows))]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn atomic_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "Kernel32")]
    extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    let from_wide: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to_wide: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Supervisor-Konfiguration.
pub struct WorkerPool {
    candidates: Vec<String>,
    active: usize,
    poll_secs: u64,
    headless: bool,
    state_path: PathBuf,
    control_path: PathBuf,
    children: HashMap<String, Child>,
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
            failover: HashMap::new(),
        }
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
            let reserve = Self::select_to_promote(candidates, state, &excluded);
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

    /// Startet einen Kindprozess pro Worker (re-exec des eigenen Binaries mit
    /// dem `bot2bot-worker`-Subcommand).
    /// Beendet alle laufenden Kindprozesse.
    ///
    /// Wird sowohl beim geordneten Herunterfahren als auch aus `Drop` gerufen,
    /// damit kein Weg am Aufräumen vorbeiführt.
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

        let blocked = detect_blocked(&running_ages, &snaps, stale);

        let running_pids: HashMap<String, u32> = self
            .children
            .iter()
            .map(|(b, c)| (b.clone(), c.id()))
            .collect();

        // 1) BLOCK-Erkennung -> Failover-Eintraege + Reserve-Promotion
        //    (Kill der geblockten Kinder erfolgt direkt danach).
        let block_actions = Self::compute_block_failover(
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
        let restore_actions = Self::compute_restore(
            now,
            &mut self.failover,
            &mut state,
            MAX_FAILED_RESTORES,
            |brain| match Self::spawn_worker(brain, self.poll_secs, self.headless) {
                Ok(child) => {
                    self.children.insert(brain.to_string(), child);
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
            match Self::select_to_promote(&self.candidates, &state, &running) {
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
fn format_rfc3339(t: SystemTime) -> Option<String> {
    let secs = t.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_secs() as i64;
    OffsetDateTime::from_unix_timestamp(secs)
        .ok()
        .and_then(|o| o.format(&Rfc3339).ok())
}

/// Parst einen RFC3339-Zeitstempel zurueck zu `SystemTime` (fuer Cooldown-Vergleich).
fn parse_rfc3339(s: &str) -> Option<SystemTime> {
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
    fn pool_state_roundtrip() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let mut st = PoolState::default();
        st.set("deepseek", STATUS_ACTIVE, "");
        st.set("chatgpt", STATUS_UNAVAILABLE, "exit code 1");
        st.save(&path).unwrap();

        let loaded = PoolState::load(&path);
        assert_eq!(loaded.entries["deepseek"].status, STATUS_ACTIVE);
        assert_eq!(loaded.entries["chatgpt"].status, STATUS_UNAVAILABLE);
        assert_eq!(loaded.entries["chatgpt"].last_error, "exit code 1");
    }

    #[test]
    fn pool_state_save_atomically_replaces_existing_file() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let mut st = PoolState::default();
        st.set("first", STATUS_ACTIVE, "");
        st.save(&path).unwrap();
        st.entries.clear();
        st.set("second", STATUS_AVAILABLE, "");
        st.save(&path).unwrap();

        let loaded = PoolState::load(&path);
        assert!(!loaded.entries.contains_key("first"));
        assert_eq!(loaded.entries["second"].status, STATUS_AVAILABLE);
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temporary files leaked: {leftovers:?}"
        );
    }

    #[test]
    fn pool_control_take_distinguishes_zero_from_no_change() {
        let dir = tmp_dir();
        let path = dir.join("pool_control.json");
        let control = PoolControl {
            target_active: Some(0),
            ..Default::default()
        };
        let json = serde_json::to_vec(&control).unwrap();
        atomic_write(&path, &json).unwrap();

        let loaded = PoolControl::take(&path).unwrap().unwrap();
        assert_eq!(loaded.target_active, Some(0));
        assert!(!path.exists());
        assert!(PoolControl::take(&path).unwrap().is_none());
    }

    #[test]
    fn invalid_pool_control_is_retained_for_diagnosis() {
        let dir = tmp_dir();
        let path = dir.join("pool_control.json");
        atomic_write(&path, b"{not json").unwrap();

        let err = PoolControl::take(&path).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
        let retained: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".invalid-"))
            .collect();
        assert_eq!(retained.len(), 1);
        assert_eq!(fs::read_to_string(retained[0].path()).unwrap(), "{not json");
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
            WorkerPool::select_to_promote(&candidates, &state, &HashSet::new()).is_some(),
            "nach Freigabe muss wieder ein Brain promotierbar sein"
        );
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
    fn load_or_init_seeds_candidates_as_available() {
        let dir = tmp_dir();
        let path = dir.join("pool_state.json");
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let st = PoolState::load_or_init(&path, &candidates);
        assert_eq!(st.entries.len(), 3);
        assert_eq!(st.entries["a"].status, STATUS_AVAILABLE);
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
            WorkerPool::select_to_promote(&candidates, &state, &running),
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
        assert_eq!(
            WorkerPool::select_to_promote(&candidates, &state, &running),
            None
        );
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
            WorkerPool::select_to_promote(&candidates, &state, &running),
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

    /// Test-Helfer: baut einen `BreakerSnapshot`.
    fn snap(brain: &str, open: bool) -> BreakerSnapshot {
        BreakerSnapshot {
            brain_id: brain.to_string(),
            open,
            consecutive_failures: if open { 3 } else { 0 },
            open_until: if open { Some(1) } else { None },
            remaining_secs: if open { Some(1) } else { None },
            last_reason: if open { Some("blocked".into()) } else { None },
        }
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

        let acts = WorkerPool::compute_block_failover(
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
            match WorkerPool::select_to_promote(&candidates, &state, &run) {
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

        let acts = WorkerPool::compute_restore(
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

        let acts = WorkerPool::compute_block_failover(
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

        let acts = WorkerPool::compute_restore(
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

        let acts = WorkerPool::compute_restore(
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

    #[test]
    fn block_cooldown_default_is_600() {
        assert_eq!(BLOCK_COOLDOWN_SECS, 600);
    }
}
