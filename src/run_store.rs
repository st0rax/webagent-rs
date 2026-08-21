//! Run-Persistenz.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const CROSS_BRAIN_HANDOFF_KIND: &str = "cross_brain_session_handoff";
pub const CROSS_BRAIN_HANDOFF_VERSION: u32 = 1;
pub const CROSS_BRAIN_HANDOFF_MAX_CONTEXT_CHARS: usize = 4_000;
const CROSS_BRAIN_HANDOFF_MAX_ID_CHARS: usize = 160;
const CROSS_BRAIN_HANDOFF_MAX_BRAIN_CHARS: usize = 80;
const CROSS_BRAIN_HANDOFF_MAX_ATTEMPT: u32 = 64;

/// Bounded, textual metadata passed between two different brains.
///
/// Deliberately has no `conversation_ref`: a cross-brain target gets a new run
/// and a new provider conversation. Unknown serialized fields are rejected so
/// a caller cannot smuggle a foreign session reference through this contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CrossBrainHandoffEnvelope {
    kind: String,
    version: u32,
    source_run_id: String,
    source_brain_id: String,
    target_brain_id: String,
    attempt: u32,
    context: String,
}

impl CrossBrainHandoffEnvelope {
    pub fn new(
        source_run_id: &str,
        source_brain_id: &str,
        target_brain_id: &str,
        attempt: u32,
        context: &str,
    ) -> Result<Self, String> {
        let envelope = Self {
            kind: CROSS_BRAIN_HANDOFF_KIND.to_string(),
            version: CROSS_BRAIN_HANDOFF_VERSION,
            source_run_id: source_run_id.trim().to_string(),
            source_brain_id: source_brain_id.trim().to_string(),
            target_brain_id: target_brain_id.trim().to_string(),
            attempt,
            context: context.trim().to_string(),
        };
        envelope.validate()?;
        Ok(envelope)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.kind != CROSS_BRAIN_HANDOFF_KIND {
            return Err(format!("Unbekannte Handoff-Art {:?}", self.kind));
        }
        if self.version != CROSS_BRAIN_HANDOFF_VERSION {
            return Err(format!(
                "Nicht unterstuetzte Handoff-Version {}",
                self.version
            ));
        }
        validate_bounded_field(
            "source_run_id",
            &self.source_run_id,
            CROSS_BRAIN_HANDOFF_MAX_ID_CHARS,
        )?;
        validate_bounded_field(
            "source_brain_id",
            &self.source_brain_id,
            CROSS_BRAIN_HANDOFF_MAX_BRAIN_CHARS,
        )?;
        validate_bounded_field(
            "target_brain_id",
            &self.target_brain_id,
            CROSS_BRAIN_HANDOFF_MAX_BRAIN_CHARS,
        )?;
        if self.source_brain_id == self.target_brain_id {
            return Err("Cross-Brain-Handoff erfordert verschiedene Brains".to_string());
        }
        if !(1..=CROSS_BRAIN_HANDOFF_MAX_ATTEMPT).contains(&self.attempt) {
            return Err(format!(
                "Handoff-attempt muss zwischen 1 und {CROSS_BRAIN_HANDOFF_MAX_ATTEMPT} liegen"
            ));
        }
        validate_bounded_field(
            "context",
            &self.context,
            CROSS_BRAIN_HANDOFF_MAX_CONTEXT_CHARS,
        )
    }

    pub fn validate_for(&self, source: &RunMeta, target_brain_id: &str) -> Result<(), String> {
        self.validate()?;
        if source.run_id != self.source_run_id {
            return Err(format!(
                "Handoff-Quelle erwartet Run {:?}, erhalten {:?}",
                self.source_run_id, source.run_id
            ));
        }
        if source.brain_id != self.source_brain_id {
            return Err(format!(
                "Handoff-Quelle erwartet Brain {:?}, Run gehoert {:?}",
                self.source_brain_id, source.brain_id
            ));
        }
        if target_brain_id != self.target_brain_id {
            return Err(format!(
                "Handoff-Ziel erwartet Brain {:?}, erhalten {:?}",
                self.target_brain_id, target_brain_id
            ));
        }
        Ok(())
    }

    pub fn source_run_id(&self) -> &str {
        &self.source_run_id
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn source_brain_id(&self) -> &str {
        &self.source_brain_id
    }

    pub fn target_brain_id(&self) -> &str {
        &self.target_brain_id
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn context(&self) -> &str {
        &self.context
    }
}

fn validate_bounded_field(name: &str, value: &str, max_chars: usize) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("Handoff-Feld {name} darf nicht leer sein"));
    }
    let chars = value.chars().count();
    if chars > max_chars {
        return Err(format!(
            "Handoff-Feld {name} ist mit {chars} Zeichen groesser als das Limit {max_chars}"
        ));
    }
    Ok(())
}

/// Terminal-Status, die nicht mehr geändert werden können.
const TERMINAL_STATUSES: &[&str] = &[
    "done",
    "failed",
    "interrupted",
    "never_started",
    "protocol_error",
];

/// Ausgangsstatus fuer einen verwaisten, nicht mehr laufenden Run.
///
/// Ein Run mit `cycles == 0` wurde angelegt, hat aber nie eine Schleife
/// ausgefuehrt — er ist nie wirklich gestartet und daher kein Abbruch, sondern
/// ein Nichtereignis (`never_started`). Erst ein Run mit tatsaechlicher Arbeit
/// (`cycles >= 1`), der mitten in der Ausfuehrung abbrach, ist `interrupted`.
pub fn stale_status(cycles: u32) -> &'static str {
    if cycles == 0 {
        "never_started"
    } else {
        "interrupted"
    }
}

/// Bestimmt den Status eines verwaisten Runs anhand seiner tatsaechlich
/// persistierten Arbeit.
///
/// `cycles` ist kein verlaesslicher Aktivitaetszaehler: der Controller schreibt
/// ihn erst beim geordneten Abschluss zurueck. Nach einem harten Prozessabbruch
/// kann ein Run deshalb `cycles == 0` haben, obwohl bereits Actions ausgefuehrt
/// und Observations gespeichert wurden. Solche Laeufe sind `interrupted`, nicht
/// `never_started`.
pub fn stale_status_for(meta: &RunMeta) -> &'static str {
    let observation_bytes = meta
        .extra
        .get("observation_bytes")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(0);
    let act_steps = meta
        .extra
        .get("act_steps")
        .and_then(|value| value.as_u64())
        .unwrap_or(0);

    if meta.cycles > 0
        || !meta.completed_actions.is_empty()
        || observation_bytes > 0
        || act_steps > 0
    {
        "interrupted"
    } else {
        "never_started"
    }
}

/// Nicht-laufende Status (außer Terminal).
const NON_RUNNING_STATUSES: &[&str] = &[
    "brain_incomplete",
    "max_cycles",
    "login_required",
    "cloudflare",
    "error",
    "protocol_error",
];

/// Erlaubte Status-Übergänge.
fn allowed_status_transitions() -> HashMap<&'static str, HashSet<&'static str>> {
    let mut map = HashMap::new();

    let mut running_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        running_next.insert(*s);
    }
    for s in NON_RUNNING_STATUSES {
        running_next.insert(*s);
    }
    map.insert("running", running_next);

    let mut brain_incomplete_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        brain_incomplete_next.insert(*s);
    }
    brain_incomplete_next.insert("max_cycles");
    map.insert("brain_incomplete", brain_incomplete_next);

    let mut max_cycles_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        max_cycles_next.insert(*s);
    }
    max_cycles_next.insert("brain_incomplete");
    map.insert("max_cycles", max_cycles_next);

    let mut login_required_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        login_required_next.insert(*s);
    }
    map.insert("login_required", login_required_next);

    let mut cloudflare_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        cloudflare_next.insert(*s);
    }
    map.insert("cloudflare", cloudflare_next);

    let mut error_next = HashSet::new();
    for s in TERMINAL_STATUSES {
        error_next.insert(*s);
    }
    map.insert("error", error_next);

    map
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunMeta {
    pub run_id: String,
    pub brain_id: String,
    pub task: String,
    pub created_at: String,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub cycles: u32,
    #[serde(default)]
    pub conversation_ref: Option<String>,
    #[serde(default)]
    pub completed_actions: HashMap<String, String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
}

fn default_status() -> String {
    "running".to_string()
}

impl RunMeta {
    /// Verzeichnis für diesen Run.
    pub fn dir(&self, runs_dir: &Path) -> PathBuf {
        runs_dir.join(&self.run_id)
    }

    pub fn cross_brain_handoff(&self) -> Result<Option<CrossBrainHandoffEnvelope>, String> {
        let Some(value) = self.extra.get("cross_brain_handoff") else {
            return Ok(None);
        };
        let envelope: CrossBrainHandoffEnvelope = serde_json::from_value(value.clone())
            .map_err(|e| format!("Ungueltige Cross-Brain-Provenienz in RunMeta: {e}"))?;
        envelope.validate()?;
        Ok(Some(envelope))
    }
}

pub struct RunStore {
    runs_dir: PathBuf,
    logs_dir: PathBuf,
}

impl RunStore {
    pub fn new(runs_dir: PathBuf, logs_dir: PathBuf) -> Self {
        fs::create_dir_all(&runs_dir).ok();
        fs::create_dir_all(&logs_dir).ok();
        Self { runs_dir, logs_dir }
    }

    /// Erstellt einen neuen Run.
    pub fn create(&self, brain_id: &str, task: &str) -> Result<RunMeta, String> {
        self.create_internal(brain_id, task, None)
    }

    /// Erstellt den frischen Ziel-Run fuer einen validierten Cross-Brain-Handoff.
    /// Der Quell-Run wird nur zur Provenienzpruefung geladen; seine Browser-
    /// Konversation und sein sonstiger Zustand werden nicht uebernommen.
    pub fn create_cross_brain_handoff(
        &self,
        brain_id: &str,
        task: &str,
        handoff: &CrossBrainHandoffEnvelope,
    ) -> Result<RunMeta, String> {
        let source = self.load(handoff.source_run_id())?;
        handoff.validate_for(&source, brain_id)?;
        self.create_internal(brain_id, task, Some(handoff))
    }

    fn create_internal(
        &self,
        brain_id: &str,
        task: &str,
        handoff: Option<&CrossBrainHandoffEnvelope>,
    ) -> Result<RunMeta, String> {
        // Einfache Zufalls-ID ohne uuid-Crate: Timestamp + Prozess-ID + Zufallszahl
        let random_suffix = {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let pid = std::process::id();
            format!("{:08x}", (nanos ^ pid).wrapping_mul(0x9e3779b9))
        };
        let run_id = format!("{}_{}", crate::now_run_stamp(), random_suffix);
        let mut meta = RunMeta {
            run_id: run_id.clone(),
            brain_id: brain_id.to_string(),
            task: task.to_string(),
            created_at: crate::now_rfc3339(),
            status: "running".to_string(),
            cycles: 0,
            conversation_ref: None,
            completed_actions: HashMap::new(),
            extra: HashMap::new(),
        };
        if let Some(handoff) = handoff {
            meta.extra.insert(
                "cross_brain_handoff".to_string(),
                serde_json::to_value(handoff)
                    .map_err(|e| format!("Fehler beim Serialisieren des Handoffs: {e}"))?,
            );
        }

        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;

        let log_dir = self.logs_dir.join(&run_id);
        fs::create_dir_all(&log_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", log_dir.display(), e))?;

        self.save_internal(&meta)?;
        self.append_event(&meta, "created", serde_json::json!({"status": meta.status}))?;
        if let Some(handoff) = handoff {
            self.append_event(
                &meta,
                "cross_brain_handoff_received",
                serde_json::to_value(handoff)
                    .map_err(|e| format!("Fehler beim Serialisieren des Handoffs: {e}"))?,
            )?;
        }

        Ok(meta)
    }

    /// Lädt einen Run.
    pub fn load(&self, run_id: &str) -> Result<RunMeta, String> {
        let path = self.runs_dir.join(run_id).join("meta.json");
        let content = fs::read_to_string(&path)
            .map_err(|e| format!("Fehler beim Lesen von {}: {}", path.display(), e))?;

        let meta: RunMeta = serde_json::from_str(&content)
            .map_err(|e| format!("Fehler beim Parsen von {}: {}", path.display(), e))?;

        Ok(meta)
    }

    /// Speichert einen Run mit Validierung.
    pub fn save(&self, meta: &RunMeta) -> Result<(), String> {
        let previous = self.load_existing_meta(&meta.run_id);

        if let Some(prev) = &previous {
            self.validate_status_transition(&prev.status, &meta.status)?;
        }

        self.save_internal(meta)?;
        self.append_save_events(previous.as_ref(), meta)?;

        Ok(())
    }

    /// Reaktiviert einen explizit fortgesetzten Run, ohne die allgemeinen
    /// Status-Übergänge für normale Saves aufzuweichen.
    ///
    /// Ein Provider-/Protokollfehler beendet einen einzelnen Controller-Aufruf,
    /// nicht zwingend die langlebige Agent-Session. Nur bekannte reparierbare
    /// Endzustände dürfen über diesen dedizierten Pfad wieder `running` werden.
    pub fn activate_continuation(&self, meta: &mut RunMeta) -> Result<(), String> {
        const ACTIVATABLE: &[&str] = &[
            "brain_incomplete",
            "done",
            "max_cycles",
            "protocol_error",
            "wall_timeout",
            "interrupted",
        ];

        let persisted = self
            .load_existing_meta(&meta.run_id)
            .ok_or_else(|| format!("Run {:?} existiert nicht", meta.run_id))?;
        if persisted.status != meta.status {
            return Err(format!(
                "Run-Status wurde parallel geändert: gespeichert={:?}, geladen={:?}",
                persisted.status, meta.status
            ));
        }
        if meta.status != "running" && !ACTIVATABLE.contains(&meta.status.as_str()) {
            return Err(format!(
                "Run mit Status {:?} kann nicht fortgesetzt werden",
                meta.status
            ));
        }

        let previous = meta.clone();
        meta.status = "running".to_string();
        meta.extra.remove("protocol_error_streak");
        meta.extra.remove("protocol_error");
        self.save_internal(meta)?;
        self.append_save_events(Some(&previous), meta)
    }

    /// Interne Speicherfunktion ohne Validierung.
    fn save_internal(&self, meta: &RunMeta) -> Result<(), String> {
        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;

        let path = run_dir.join("meta.json");
        let tmp_path = run_dir.join("meta.json.tmp");

        let json = serde_json::to_string_pretty(meta)
            .map_err(|e| format!("Fehler beim Serialisieren: {}", e))?;

        fs::write(&tmp_path, &json)
            .map_err(|e| format!("Fehler beim Schreiben von {}: {}", tmp_path.display(), e))?;

        // Der Rename bleibt der bevorzugte atomare Weg. Unter Windows kann er
        // jedoch bei einem bereits vorhandenen Ziel (oder kurzem Scanner-/Handle-
        // Nachlauf) scheitern. Dann schreibt der Fallback denselben vollständig
        // serialisierten Zustand direkt und entfernt die Temp-Datei best effort.
        if let Err(rename_error) = fs::rename(&tmp_path, &path) {
            fs::write(&path, &json).map_err(|write_error| {
                format!(
                    "Fehler beim Umbenennen von {} nach {}: {}; direkter Windows-Fallback schlug fehl: {}",
                    tmp_path.display(),
                    path.display(),
                    rename_error,
                    write_error
                )
            })?;
            let _ = fs::remove_file(&tmp_path);
        }

        Ok(())
    }

    /// Lädt existierende Meta-Daten (ohne Fehler bei Nicht-Existenz).
    fn load_existing_meta(&self, run_id: &str) -> Option<RunMeta> {
        let path = self.runs_dir.join(run_id).join("meta.json");
        if !path.exists() {
            return None;
        }

        let content = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Validiert Status-Übergänge.
    fn validate_status_transition(&self, previous: &str, next: &str) -> Result<(), String> {
        if previous == next {
            return Ok(());
        }

        let transitions = allowed_status_transitions();
        let allowed = transitions.get(previous);

        match allowed {
            Some(set) if set.contains(next) => Ok(()),
            Some(_) => Err(format!(
                "Ungültiger Run-Status-Übergang: {:?} -> {:?}",
                previous, next
            )),
            None => Err(format!(
                "Ungültiger Run-Status-Übergang: {:?} -> {:?}",
                previous, next
            )),
        }
    }

    /// Schreibt Events beim Speichern.
    fn append_save_events(&self, previous: Option<&RunMeta>, meta: &RunMeta) -> Result<(), String> {
        if previous.is_none() {
            return self.append_event(meta, "created", serde_json::json!({"status": &meta.status}));
        }

        let prev = previous.unwrap();
        if prev.status != meta.status {
            self.append_event(
                meta,
                "status_changed",
                serde_json::json!({
                    "from": &prev.status,
                    "to": &meta.status,
                }),
            )
        } else {
            self.append_event(
                meta,
                "meta_saved",
                serde_json::json!({"status": &meta.status}),
            )
        }
    }

    /// Schreibt ein Event in events.jsonl.
    fn append_event(
        &self,
        meta: &RunMeta,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let run_dir = meta.dir(&self.runs_dir);
        fs::create_dir_all(&run_dir)
            .map_err(|e| format!("Fehler beim Erstellen von {}: {}", run_dir.display(), e))?;

        let path = run_dir.join("events.jsonl");
        let event = serde_json::json!({
            "timestamp": crate::now_rfc3339(),
            "run_id": &meta.run_id,
            "type": event_type,
            "payload": payload,
        });

        let line = serde_json::to_string(&event)
            .map_err(|e| format!("Fehler beim Serialisieren des Events: {}", e))?;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Fehler beim Öffnen von {}: {}", path.display(), e))?;

        writeln!(file, "{}", line)
            .map_err(|e| format!("Fehler beim Schreiben in {}: {}", path.display(), e))?;

        // Storax-Vorgabe (2026-08-01): die Run-Events (meta_saved,
        // status_changed) spiegeln in den TUI-Baum, damit der Lebenszyklus
        // eines Phase-B-Runs dort mitläuft.
        if crate::bench_events::echo_bus_enabled() {
            let level = if event_type == "status_changed" {
                crate::bench_events::Level::Progress
            } else {
                crate::bench_events::Level::Info
            };
            crate::bench_events::emit_detailed(
                level,
                None,
                &format!("[run:{}] {}", meta.run_id, event_type),
                Some(&payload.to_string()),
            );
        }

        Ok(())
    }

    /// Listet alle Runs auf (sortiert, neueste zuerst).
    pub fn list_runs(&self) -> Vec<String> {
        let mut runs = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.runs_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        runs.push(name.to_string());
                    }
                }
            }
        }

        runs.sort_by(|a, b| b.cmp(a)); // Neueste zuerst
        runs
    }

    /// Markiert verwaiste `running`-Runs als `interrupted`.
    /// Zeitstempel des letzten vollstaendigen Abgleichs.
    fn reconcile_marker(&self) -> PathBuf {
        self.runs_dir.join(".last_reconcile")
    }

    /// Zeitpunkt des letzten Abgleichs, abzueglich einer Sicherheitsspanne.
    ///
    /// `None` = noch nie abgeglichen, dann wird alles angesehen.
    fn last_reconcile(&self) -> Option<SystemTime> {
        let modified = fs::metadata(self.reconcile_marker())
            .ok()
            .and_then(|m| m.modified().ok())?;
        // Eine Minute Spanne, damit ein Lauf, der waehrend des vorigen
        // Abgleichs geschrieben wurde, nicht durchrutscht.
        Some(modified - Duration::from_secs(60))
    }

    /// Laeufe, die beim letzten Abgleich noch liefen — unabhaengig von ihrer
    /// Schreibzeit erneut anzusehen.
    fn watched_running(&self) -> HashSet<String> {
        fs::read_to_string(self.reconcile_marker())
            .map(|t| {
                t.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Muss dieser Lauf angesehen werden?
    ///
    /// Die Schreibzeit allein genuegt NICHT: ein alter Lauf, dessen Prozess
    /// beim letzten Abgleich noch lebte, wird spaeter nie wieder angefasst,
    /// wenn sein Prozess ohne Schreibzugriff stirbt — er bliebe fuer immer
    /// auf `running` stehen (Fall von codex, 2026-07-27). Deshalb werden die
    /// zuletzt laufenden IDs zusaetzlich vorgemerkt und immer geprueft.
    fn should_inspect(
        run_id: &str,
        meta_modified: Option<SystemTime>,
        since: Option<SystemTime>,
        watched: &HashSet<String>,
    ) -> bool {
        if watched.contains(run_id) {
            return true;
        }
        let (Some(since), Some(modified)) = (since, meta_modified) else {
            return true;
        };
        modified >= since
    }

    /// Prueft alle nicht abgeschlossenen Laeufe und repariert verwaiste.
    ///
    /// Angesehen werden nur Laeufe, die SEIT dem letzten Abgleich geschrieben
    /// wurden. Ohne diese Eingrenzung parst jeder Programmstart jede einzelne
    /// `meta.json`: bei 1642 Laeufen sind das 8,3 Sekunden vor der ersten
    /// Bildschirmausgabe — und zwar typischerweise umsonst, weil gar kein Lauf
    /// auf `running` steht (gemessen 2026-07-27: 0 von 1642). Ein reines
    /// Altersfenster waere unsauber, weil ein lange liegengebliebener Lauf nie
    /// wieder geprueft wuerde; der Marker schliesst diese Luecke, denn nach
    /// einer Reparatur wird `meta.json` neu geschrieben und faellt damit
    /// wieder ins Fenster.
    pub fn reconcile_stale_runs(&self, legacy_age_seconds: f64) -> Vec<String> {
        let mut repaired = Vec::new();
        let since = self.last_reconcile();
        let watched = self.watched_running();
        let mut still_running: Vec<String> = Vec::new();
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        // Lazy: diese Funktion laeuft im Startup vor jedem Kommando. Liegt kein
        // "running"-Run vor (Normalfall), darf sie keinen Prozess-Spawn kosten.
        let mut procs: Option<crate::ProcessSnapshot> = None;

        for run_id in self.list_runs() {
            // Billiger Dateisystem-Blick statt teurem JSON-Parse.
            let meta_modified = fs::metadata(self.runs_dir.join(&run_id).join("meta.json"))
                .and_then(|m| m.modified())
                .ok();
            if !Self::should_inspect(&run_id, meta_modified, since, &watched) {
                continue;
            }
            let meta = match self.load(&run_id) {
                Ok(m) => m,
                Err(_) => continue,
            };

            if meta.status != "running" {
                continue;
            }

            let owner_pid = meta.extra.get("owner_pid").and_then(|v| v.as_i64());

            let stale = if let Some(pid) = owner_pid {
                !procs
                    .get_or_insert_with(crate::ProcessSnapshot::capture)
                    .is_alive(pid)
            } else {
                // Legacy: kein owner_pid → Alter prüfen
                match parse_rfc3339_to_unix(&meta.created_at) {
                    Some(created_secs) => {
                        let age = (now_secs - created_secs) as f64;
                        age >= legacy_age_seconds
                    }
                    None => true, // Ungültiger Zeitstempel → als stale behandeln
                }
            };

            if !stale {
                // Laeuft noch: fuer den naechsten Abgleich vormerken, sonst
                // ginge er verloren, falls sein Prozess ohne Schreibzugriff
                // endet.
                still_running.push(run_id.clone());
                continue;
            }

            let mut updated = meta.clone();
            updated.status = stale_status_for(&meta).to_string();
            updated.extra.insert(
                "reconciled_at".to_string(),
                serde_json::Value::String(crate::now_rfc3339()),
            );
            updated.extra.insert(
                "error".to_string(),
                serde_json::Value::String("Prozess endete ohne finalen Run-Status.".to_string()),
            );

            if self.save(&updated).is_ok() {
                repaired.push(run_id);
            }
        }

        // Marker NACH dem Durchlauf setzen: faellt der Abgleich vorher aus,
        // wird beim naechsten Start derselbe Bereich erneut geprueft statt
        // uebersprungen. Inhalt sind die noch laufenden IDs; sie werden beim
        // naechsten Mal unabhaengig von ihrer Schreibzeit geprueft.
        let _ = fs::create_dir_all(&self.runs_dir);
        let _ = fs::write(
            self.reconcile_marker(),
            still_running.join(
                "
",
            ),
        );
        repaired
    }
}

/// Parst RFC3339-Zeitstempel zu Unix-Sekunden (UTC).
///
/// Hier stand eine handgerollte Regex plus eine Kopie von Howard Hinnants
/// `days_from_civil` (~40 Zeilen) — obwohl `time` mit aktiviertem `parsing`-Feature
/// schon direkte Dependency ist und `watchdog.rs` genau dieses Feld bereits so
/// liest. Nebeneffekt der Vereinheitlichung: die alte Regex akzeptierte nur `Z`
/// oder `+00:00` und lieferte fuer jeden anderen Offset `None` (= "stale"); `time`
/// rechnet ihn korrekt um.
fn parse_rfc3339_to_unix(s: &str) -> Option<i64> {
    OffsetDateTime::parse(s, &Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // --- Inkrementeller Abgleich ----------------------------------------
    // Der Marker darf Tempo bringen, ohne Laeufe zu verlieren. Der kritische
    // Fall (gefunden von codex, 2026-07-27): ein alter Lauf lebt beim ersten
    // Scan noch, sein Prozess stirbt danach OHNE meta.json anzufassen. Ohne
    // Vormerkung wuerde er wegen zu alter Schreibzeit nie wieder angesehen
    // und bliebe fuer immer auf "running".

    fn vorgemerkt(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn alter_lauf_wird_uebersprungen_wenn_nicht_vorgemerkt() {
        let jetzt = SystemTime::now();
        let alt = jetzt - Duration::from_secs(86_400);
        assert!(
            !RunStore::should_inspect("alt-1", Some(alt), Some(jetzt), &vorgemerkt(&[])),
            "alter, unbeteiligter Lauf haette uebersprungen werden muessen"
        );
    }

    #[test]
    fn vorgemerkter_lauf_wird_trotz_alter_schreibzeit_geprueft() {
        let jetzt = SystemTime::now();
        let alt = jetzt - Duration::from_secs(86_400);
        assert!(
            RunStore::should_inspect("laeuft-1", Some(alt), Some(jetzt), &vorgemerkt(&["laeuft-1"])),
            "vorgemerkter Lauf ging verloren — genau der Fall, der ihn fuer              immer auf running stehen liesse"
        );
    }

    #[test]
    fn frisch_geschriebener_lauf_wird_geprueft() {
        let jetzt = SystemTime::now();
        let seit = jetzt - Duration::from_secs(600);
        assert!(RunStore::should_inspect(
            "neu-1",
            Some(jetzt),
            Some(seit),
            &vorgemerkt(&[])
        ));
    }

    #[test]
    fn ohne_marker_wird_alles_geprueft() {
        assert!(RunStore::should_inspect("x", None, None, &vorgemerkt(&[])));
        let jetzt = SystemTime::now();
        assert!(
            RunStore::should_inspect("x", None, Some(jetzt), &vorgemerkt(&[])),
            "ohne lesbare Schreibzeit muss im Zweifel geprueft werden"
        );
    }

    /// Ende-zu-Ende: ein laufender Lauf mit lebender PID bleibt stehen UND
    /// landet im Marker; danach wird er trotz unveraenderter meta.json erneut
    /// angesehen und repariert, sobald die PID tot ist.
    #[test]
    fn laufender_lauf_wird_vorgemerkt_und_spaeter_repariert() {
        let tmp = unique_tmp();
        let dir = tmp.join("runs");
        let store = RunStore::new(dir.clone(), tmp.join("logs"));
        let mut meta = store.create("mock", "laeuft-noch").unwrap();
        let run_id = meta.run_id.clone();
        meta.status = "running".to_string();
        meta.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(std::process::id().into()),
        );
        store.save_internal(&meta).unwrap();

        let repariert = store.reconcile_stale_runs(600.0);
        assert!(
            repariert.is_empty(),
            "lebender Lauf wurde faelschlich repariert"
        );
        let marker = std::fs::read_to_string(dir.join(".last_reconcile")).unwrap_or_default();
        assert!(
            marker.contains(&run_id),
            "lebender Lauf wurde nicht vorgemerkt, Marker={marker:?}"
        );

        // PID auf einen mit Sicherheit toten Wert setzen, meta.json bleibt
        // ansonsten unveraendert.
        let mut tot = store.load(&run_id).unwrap();
        tot.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(0x7FFF_FFFEu32.into()),
        );
        store.save_internal(&tot).unwrap();

        let repariert = store.reconcile_stale_runs(600.0);
        assert_eq!(
            repariert,
            vec![run_id.clone()],
            "toter Lauf blieb auf running"
        );
        assert_eq!(store.load(&run_id).unwrap().status, "never_started");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Eindeutiges Temp-Verzeichnis pro Testaufruf. `now_run_stamp()` ist nur
    /// sekundengenau; da Rust Tests parallel ausführt, würden mehrere Tests
    /// sonst dasselbe Verzeichnis teilen und sich gegenseitig sehen.
    fn unique_tmp() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let id = N.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "test_run_store_{}_{}_{}",
            std::process::id(),
            crate::now_run_stamp(),
            id
        ))
    }

    #[test]
    fn cross_brain_envelope_rejects_same_brain_malformed_and_oversized_input() {
        assert!(
            CrossBrainHandoffEnvelope::new("run-1", "alpha", "alpha", 1, "context")
                .unwrap_err()
                .contains("verschiedene Brains")
        );
        assert!(
            CrossBrainHandoffEnvelope::new("run-1", "alpha", "beta", 0, "context")
                .unwrap_err()
                .contains("attempt")
        );
        let oversized = "x".repeat(CROSS_BRAIN_HANDOFF_MAX_CONTEXT_CHARS + 1);
        assert!(
            CrossBrainHandoffEnvelope::new("run-1", "alpha", "beta", 1, &oversized)
                .unwrap_err()
                .contains("context")
        );

        let with_foreign_ref = serde_json::json!({
            "kind": CROSS_BRAIN_HANDOFF_KIND,
            "version": CROSS_BRAIN_HANDOFF_VERSION,
            "source_run_id": "run-1",
            "source_brain_id": "alpha",
            "target_brain_id": "beta",
            "attempt": 1,
            "context": "text only",
            "conversation_ref": "https://foreign.example/session/1"
        });
        assert!(
            serde_json::from_value::<CrossBrainHandoffEnvelope>(with_foreign_ref).is_err(),
            "a foreign conversation_ref must not fit the envelope contract"
        );

        let valid = CrossBrainHandoffEnvelope::new("run-1", "alpha", "beta", 1, "context").unwrap();
        let mut malformed_kind = valid.clone();
        malformed_kind.kind = "same_brain_continuation".to_string();
        assert!(malformed_kind.validate().is_err());
        let mut malformed_version = valid;
        malformed_version.version = CROSS_BRAIN_HANDOFF_VERSION + 1;
        assert!(malformed_version.validate().is_err());
    }

    #[test]
    fn cross_brain_target_validates_source_and_persists_provenance_without_session_ref() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let store = RunStore::new(runs_dir.clone(), tmp.join("logs"));
        let mut source = store.create("alpha", "source task").unwrap();
        source.conversation_ref = Some("https://chatgpt.com/c/foreign-source".to_string());
        store.save(&source).unwrap();

        let wrong_source =
            CrossBrainHandoffEnvelope::new(&source.run_id, "gamma", "beta", 1, "compiler output")
                .unwrap();
        assert!(store
            .create_cross_brain_handoff("beta", "target task", &wrong_source)
            .unwrap_err()
            .contains("Run gehoert"));

        let handoff =
            CrossBrainHandoffEnvelope::new(&source.run_id, "alpha", "beta", 1, "compiler output")
                .unwrap();
        assert!(store
            .create_cross_brain_handoff("gamma", "target task", &handoff)
            .unwrap_err()
            .contains("Handoff-Ziel"));

        let target = store
            .create_cross_brain_handoff("beta", "target task", &handoff)
            .unwrap();
        assert_ne!(target.run_id, source.run_id);
        assert_eq!(target.brain_id, "beta");
        assert_eq!(target.conversation_ref, None);
        assert_eq!(target.cross_brain_handoff().unwrap(), Some(handoff));

        let events =
            fs::read_to_string(runs_dir.join(&target.run_id).join("events.jsonl")).unwrap();
        assert!(events.contains("cross_brain_handoff_received"));
        assert!(events.contains(&source.run_id));
        assert!(events.contains(CROSS_BRAIN_HANDOFF_KIND));

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_reconcile_legacy_stale_running_run() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "stale").unwrap();

        // Setze created_at auf 1 Stunde in der Vergangenheit (3600 Sekunden)
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let past_secs = now_secs - 3600;

        // Konvertiere zu RFC3339 (nutzt civil_utc aus lib.rs)
        let (y, mo, d, h, mi, s) = crate::civil_utc(past_secs);
        meta.created_at = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000000+00:00",
            y, mo, d, h, mi, s
        );
        store.save_internal(&meta).unwrap();

        let repaired = store.reconcile_stale_runs(10.0);
        assert_eq!(repaired, vec![meta.run_id.clone()]);

        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "never_started");
        assert!(loaded.extra.contains_key("reconciled_at"));

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    /// Nur ein Run, der tatsaechlich gearbeitet hat (cycles >= 1), wird bei der
    /// Reconcile-Reparatur `interrupted`; ein Lauf ohne Schleife ist ein
    /// Nichtereignis (`never_started`).
    #[test]
    fn test_reconcile_unterscheidet_never_started_von_interrupted() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "stale").unwrap();
        meta.cycles = 3;
        store.save_internal(&meta).unwrap();

        let repaired = store.reconcile_stale_runs(0.0);
        assert_eq!(repaired, vec![meta.run_id.clone()]);
        assert_eq!(store.load(&meta.run_id).unwrap().status, "interrupted");

        let meta2 = store.create("mock", "nie-gestartet").unwrap();
        store.save_internal(&meta2).unwrap();
        let repaired = store.reconcile_stale_runs(0.0);
        assert!(repaired.contains(&meta2.run_id));
        assert_eq!(store.load(&meta2.run_id).unwrap().status, "never_started");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn reconcile_erkennt_arbeit_auch_wenn_cycles_nach_hartem_abbruch_null_ist() {
        let tmp = unique_tmp();
        let store = RunStore::new(tmp.join("runs"), tmp.join("logs"));
        let mut meta = store.create("mock", "wurde bereits bearbeitet").unwrap();
        meta.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(0x7FFF_FFFEu32.into()),
        );
        meta.extra.insert(
            "observation_bytes".to_string(),
            serde_json::Value::String("84341".to_string()),
        );
        meta.completed_actions
            .insert("find-registry".to_string(), "[exit_code: 0]".to_string());
        assert_eq!(meta.cycles, 0, "simuliert den ungeordneten Abbruch");
        store.save_internal(&meta).unwrap();

        assert_eq!(store.reconcile_stale_runs(600.0), vec![meta.run_id.clone()]);
        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "interrupted");
        assert!(loaded.extra.contains_key("reconciled_at"));
        assert_eq!(
            loaded.extra.get("error").and_then(|value| value.as_str()),
            Some("Prozess endete ohne finalen Run-Status.")
        );

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn stale_status_for_akzeptiert_numerische_und_legacy_string_metriken() {
        let tmp = unique_tmp();
        let store = RunStore::new(tmp.join("runs"), tmp.join("logs"));
        let mut meta = store.create("mock", "metriken").unwrap();
        assert_eq!(stale_status_for(&meta), "never_started");

        meta.extra.insert(
            "observation_bytes".to_string(),
            serde_json::Value::Number(1u64.into()),
        );
        assert_eq!(stale_status_for(&meta), "interrupted");

        meta.extra.clear();
        meta.extra.insert(
            "observation_bytes".to_string(),
            serde_json::Value::String("1".to_string()),
        );
        assert_eq!(stale_status_for(&meta), "interrupted");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_reconcile_keeps_live_owned_run() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "live").unwrap();

        // Setze owner_pid auf aktuellen Prozess
        meta.extra.insert(
            "owner_pid".to_string(),
            serde_json::Value::Number(std::process::id().into()),
        );
        store.save(&meta).unwrap();

        let repaired = store.reconcile_stale_runs(0.0);
        assert!(repaired.is_empty());

        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "running");

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn login_required_persists_over_existing_running_meta() {
        let tmp = unique_tmp();
        let store = RunStore::new(tmp.join("runs"), tmp.join("logs"));
        let mut meta = store.create("gemini", "login status").unwrap();

        meta.status = "login_required".to_string();
        store.save(&meta).unwrap();

        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.status, "login_required");
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_status_transition_validation() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let mut meta = store.create("mock", "test").unwrap();

        // Erlaubter Übergang: running -> done
        meta.status = "done".to_string();
        assert!(store.save(&meta).is_ok());

        // Unerlaubter Übergang: done -> running
        meta.status = "running".to_string();
        assert!(store.save(&meta).is_err());

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn continuation_activation_is_explicit_and_clears_transient_protocol_state() {
        let tmp = unique_tmp();
        let store = RunStore::new(tmp.join("runs"), tmp.join("logs"));
        let mut meta = store.create("mock", "repair").unwrap();
        meta.status = "protocol_error".to_string();
        meta.cycles = 3;
        meta.completed_actions
            .insert("read-before-error".to_string(), "ok".to_string());
        meta.extra.insert(
            "protocol_error_streak".to_string(),
            serde_json::Value::String("3".to_string()),
        );
        meta.extra.insert(
            "protocol_error".to_string(),
            serde_json::Value::String("legacy parser failure".to_string()),
        );
        store.save(&meta).unwrap();

        let mut generic = meta.clone();
        generic.status = "running".to_string();
        assert!(store.save(&generic).is_err());

        store.activate_continuation(&mut meta).unwrap();
        assert_eq!(meta.status, "running");
        assert_eq!(meta.cycles, 3);
        assert!(meta.completed_actions.contains_key("read-before-error"));
        assert!(!meta.extra.contains_key("protocol_error_streak"));
        assert!(!meta.extra.contains_key("protocol_error"));
        assert_eq!(store.load(&meta.run_id).unwrap().status, "running");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn continuation_activation_reopens_brain_done_for_external_review_repair() {
        let tmp = unique_tmp();
        let store = RunStore::new(tmp.join("runs"), tmp.join("logs"));
        let mut meta = store.create("mock", "already complete").unwrap();
        meta.status = "done".to_string();
        store.save(&meta).unwrap();

        let mut generic = meta.clone();
        generic.status = "running".to_string();
        assert!(store.save(&generic).is_err());

        store.activate_continuation(&mut meta).unwrap();
        assert_eq!(meta.status, "running");
        assert_eq!(store.load(&meta.run_id).unwrap().status, "running");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_create_and_load() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());
        let meta = store.create("test_brain", "test task").unwrap();

        assert_eq!(meta.brain_id, "test_brain");
        assert_eq!(meta.task, "test task");
        assert_eq!(meta.status, "running");
        assert_eq!(meta.cycles, 0);

        let loaded = store.load(&meta.run_id).unwrap();
        assert_eq!(loaded.run_id, meta.run_id);
        assert_eq!(loaded.brain_id, "test_brain");

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn test_list_runs() {
        let tmp = unique_tmp();
        let runs_dir = tmp.join("runs");
        let logs_dir = tmp.join("logs");

        let store = RunStore::new(runs_dir.clone(), logs_dir.clone());

        let meta1 = store.create("brain1", "task1").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let meta2 = store.create("brain2", "task2").unwrap();

        let runs = store.list_runs();
        assert_eq!(runs.len(), 2);
        // Beide Runs entstehen i.d.R. in derselben Sekunde; der Run-Stempel ist
        // sekundengenau, daher ist die exakte Reihenfolge nicht deterministisch.
        // Robust: beide IDs müssen vorhanden sein.
        assert!(runs.contains(&meta1.run_id));
        assert!(runs.contains(&meta2.run_id));

        // Cleanup
        fs::remove_dir_all(&tmp).ok();
    }
}
