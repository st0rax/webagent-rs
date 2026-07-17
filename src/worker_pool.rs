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
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// Status eines Brains im Pool.
pub const STATUS_AVAILABLE: &str = "available";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_UNAVAILABLE: &str = "unavailable";

/// Maximales Alter eines Worker-Heartbeats (Sekunden), bevor der Supervisor
/// den Worker als haengend wertet und neu startet (v2-Hang-Erkennung).
/// Grosszuegig genug, dass einzelne lange Tasks (< 5 min) nicht falsch
/// positiv gekillt werden; ein im Login/Idle haengender Worker (der nie
/// wieder ein Heartbeat schreibt) wird dennoch sicher erkannt.
const STALE_HEARTBEAT_SECS: u64 = 300;

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
}

impl PoolEntry {
    fn available(brain: &str) -> Self {
        PoolEntry {
            brain: brain.to_string(),
            status: STATUS_AVAILABLE.to_string(),
            last_error: String::new(),
            updated_at: crate::now_rfc3339(),
        }
    }
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
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let s = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e.to_string()))?;
        fs::write(path, s)
    }
}

/// Steuerbefehle für den laufenden Supervisor (Datei-IPC, siehe
/// `PoolControl::load`). Die TUI (Teil 2) schreibt `pool_control.json`;
/// der Supervisor liest es pro Tick und wendet target_active / reflag / stop an.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PoolControl {
    /// Gewünschte Anzahl aktiver Worker (überschreibt `active`). 0 = nicht ändern.
    #[serde(default)]
    pub target_active: usize,
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
    /// Lädt `pool_control.json`; fehlt die Datei, wird Default (kein Eingriff)
    /// geliefert. Ungültiges/leeres JSON ebenfalls als Default behandelt.
    pub fn load(path: &Path) -> PoolControl {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(c) = serde_json::from_str::<PoolControl>(&s) {
                return c;
            }
        }
        PoolControl::default()
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

    /// Startet einen Kindprozess pro Worker (re-exec des eigenen Binaries mit
    /// dem `bot2bot-worker`-Subcommand).
    fn spawn_worker(brain: &str, poll_secs: u64, headless: bool) -> std::io::Result<Child> {
        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.arg("bot2bot-worker")
            .arg("--brain")
            .arg(brain)
            .arg("--poll-secs")
            .arg(poll_secs.to_string());
        if headless {
            cmd.arg("--headless");
        }
        cmd.spawn()
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
        if control.target_active > 0 {
            self.active = control.target_active;
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

        // Scale-down: zu viele laufende Worker sauber beenden (fuer TUI '-').
        while self.children.len() > self.active {
            if let Some((b, mut c)) = self.children.drain().next() {
                let _ = c.kill();
                state.set(&b, STATUS_AVAILABLE, "scaled down");
            }
        }

        // Hang-Erkennung (v2): Worker ohne frisches Heartbeat -> killen und
        // auf `available` zuruecksetzen, damit die Promote-Schleife einen
        // frischen (Browser-)Worker startet. Idle, aber pollende Worker
        // schreiben regelmaessig -> werden nicht falsch positiv gekillt.
        if let Some(hb_dir) = self.control_path.parent() {
            let mut stale: Vec<String> = Vec::new();
            for brain in self.children.keys() {
                let p = hb_dir.join(format!("heartbeat_{brain}.json"));
                if let Ok(meta) = fs::metadata(&p) {
                    if let Ok(m) = meta.modified() {
                        if let Ok(age) = SystemTime::now().duration_since(m) {
                            if age > Duration::from_secs(STALE_HEARTBEAT_SECS) {
                                stale.push(brain.clone());
                            }
                        }
                    }
                }
            }
            for brain in stale {
                if let Some(mut c) = self.children.remove(&brain) {
                    let _ = c.kill();
                    state.set(&brain, STATUS_AVAILABLE, "stale heartbeat -> restart");
                }
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

        let _ = state.save(&self.state_path);
        // Steuerbefehl konsumieren (One-Shot): nach Anwendung loeschen, damit
        // z.B. `stop` nicht ueber einen Relaunch hinweg persistiert und `reflag`
        // nicht jeden Tick wiederholt. `target_active == 0` (kein Eingriff)
        // haelt den Wert ueber Ticks via `self.active`.
        let _ = fs::remove_file(&self.control_path);
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
            let control = PoolControl::load(&self.control_path);
            self.tick(&control);
            if control.stop {
                self.kill_all();
                break;
            }
            thread::sleep(Duration::from_secs(self.poll_secs));
        }
    }
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
    let mut pool = WorkerPool::new(candidates, active, poll_secs, headless, state_path, control_path);
    pool.run();
    0
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
        let filtered = candidates_with_profile_using(&base, &["has_profile".into(), "no_profile".into()]);
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
}
