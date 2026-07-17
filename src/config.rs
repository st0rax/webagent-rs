//! Konfiguration: Pfade, Brain-Definitionen, Umgebungsvariablen.
//!
//! Portiert aus ../src/webagent/config.py

use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

/// Root-Verzeichnis der WebAgent-Installation (Elternverzeichnis von src/).
/// Compile-Zeit-Pfad (CARGO_MANIFEST_DIR) — nur für mitgelieferte Assets
/// (selectors/) und als Legacy-Quelle der Migration. Nutzerdaten (Profile/Data)
/// hängen NICHT daran, siehe `webagent_root_stable`.
pub fn root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Stabiler, install-/release-unabhängiger Basisort für nutzergeschriebene
/// Daten (Profile + data). Überlebt ein In-Place-Update, weil er NICHT am
/// Build-/Deploy-Pfad (CARGO_MANIFEST_DIR) hängt.
///
/// Auflösung (Priorität): WEBAGENT_ROOT (env) → install_webagent_root.txt
/// (Marker neben der Executable) → %LOCALAPPDATA%\webagent (Windows) bzw.
/// ~/webagent (Fallback) → CARGO_MANIFEST_DIR/webagent (allerletzter
/// Dev-/Debug-Fallback).
pub fn webagent_root_stable() -> PathBuf {
    if let Ok(d) = env::var("WEBAGENT_ROOT") {
        let s = d.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let marker = dir.join("install_webagent_root.txt");
            if let Ok(c) = std::fs::read_to_string(&marker) {
                let s = c.trim();
                if !s.is_empty() {
                    return PathBuf::from(s);
                }
            }
        }
    }
    if let Ok(v) = env::var("LOCALAPPDATA") {
        let s = v.trim();
        if !s.is_empty() {
            return PathBuf::from(s).join("webagent");
        }
    }
    if let Ok(v) = env::var("USERPROFILE") {
        let s = v.trim();
        if !s.is_empty() {
            return PathBuf::from(s).join("webagent");
        }
    }
    if let Some(home) = dirs_home() {
        return home.join("webagent");
    }
    root_dir().join("webagent")
}

/// ~/webagent ohne externes Crate (rein-Rust-Regel): nur via HOME-env.
fn dirs_home() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
}

/// src/-Verzeichnis
pub fn src_dir() -> PathBuf {
    root_dir().join("src")
}

/// data/-Verzeichnis für Runs, Memory, etc. (stabiler Ort).
pub fn data_dir() -> PathBuf {
    webagent_root_stable().join("data")
}

/// data/runs/ — Run-Metadaten und Transcripts
pub fn runs_dir() -> PathBuf {
    data_dir().join("runs")
}

/// data/memory/ — MemoryStore-Datenbank
pub fn memory_dir() -> PathBuf {
    data_dir().join("memory")
}

/// profiles/ — Browser-Profile (shared + brain-spezifisch). Stabiler Ort, damit
/// ein Login ein In-Place-Update überlebt (siehe webagent_root_stable).
pub fn profiles_dir() -> PathBuf {
    webagent_root_stable().join("profiles")
}

/// profiles/shared/ — Gemeinsames Browser-Profil (wenn shared_browser aktiviert)
pub fn shared_profile_dir() -> PathBuf {
    profiles_dir().join("shared")
}

/// Maximale Observation-Länge (Zeichen) — Python `MAX_OBSERVATION_CHARS`.
pub const MAX_OBSERVATION_CHARS: usize = 12_000;
/// Loop-Guard Warn-/Abort-Schwellen — Python `LOOP_GUARD_*`.
pub const LOOP_GUARD_WARN_COUNT: usize = 3;
pub const LOOP_GUARD_ABORT_COUNT: usize = 8;

/// Whitelist login-relevanter Artefakte für die "sparse-copy" eines
/// Referenzprofils (statt Vollkopie). Chromium hält Auth in Cookies,
/// Local Storage, Preferences, Login Data, Web Data.
pub const SPARSE_COPY_WHITELIST: &[&str] = &[
    "Cookies",
    "Login Data",
    "Web Data",
    "Preferences",
    "Secure Preferences",
    "Local Storage",
    "Session Storage",
];

/// Aktiviert die sparsame Profil-Kopie (nur SPARSE_COPY_WHITELIST) statt der
/// vollen Kopie. Default aus, um das Bestandsverhalten nicht zu ändern.
pub fn use_sparse_profile_copy() -> bool {
    let v = env::var("WEBAGENT_SPARSE_COPY").unwrap_or_default();
    matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Cooldown-Dauer (Sekunden) fuer ein als BLOCK erkanntes Brain, bevor es durch
/// einen frischen Worker wiederhergestellt wird. Ueberschreibbar via
/// WEBAGENT_BLOCK_COOLDOWN_S (Default 600 = 10 min). Spiegelt
/// `worker_pool::BLOCK_COOLDOWN_SECS` als kanonische Untergrenze.
pub fn block_cooldown_secs() -> u64 {
    env::var("WEBAGENT_BLOCK_COOLDOWN_S")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(600)
}

/// Retry-Verzoegerung (Sekunden) fuer `unavailable` Brains, bevor sie automatisch
/// wieder als `available` reflaggt werden. Ueberschreibbar via
/// WEBAGENT_RETRY_UNAVAILABLE_S (Default 120). Spiegelt
/// `worker_pool::RETRY_UNAVAILABLE_AFTER_SECS` als kanonischen Default.
pub fn retry_unavailable_secs() -> u64 {
    env::var("WEBAGENT_RETRY_UNAVAILABLE_S")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(crate::worker_pool::RETRY_UNAVAILABLE_AFTER_SECS)
}

/// Maximales Alter eines Worker-Heartbeats (Sekunden), bevor der Supervisor den
/// Worker als haengend (BLOCK) wertet. Ueberschreibbar via
/// WEBAGENT_STALE_HEARTBEAT_S (Default 300 = 5 min).
pub fn stale_heartbeat_secs() -> u64 {
    env::var("WEBAGENT_STALE_HEARTBEAT_S")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(300)
}

/// bot2bot/ — Legacy Agent-Messaging-Root for bridge/watchdog (Desktop-Sibling oder Override).
/// Note: internal messaging uses comms.rs (data/comms/) — bot2bot_root kept for compat/bridge only.
pub fn bot2bot_root() -> PathBuf {
    if let Ok(override_path) = env::var("WEBAGENT_BOT2BOT_ROOT") {
        let s = override_path.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    let link = data_dir().join("install_bot2bot_root.txt");
    if let Ok(content) = std::fs::read_to_string(&link) {
        let s = content.trim();
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    root_dir()
        .parent()
        .map(|p| p.join("bot2bot"))
        .unwrap_or_else(|| root_dir().join("bot2bot"))
}

/// consensus_workspace() — Eindeutiger Workspace-Pfad für Genius-Council
pub fn consensus_workspace() -> PathBuf {
    let stamp = crate::now_run_stamp();
    bot2bot_root().join(format!("consensus_{}", stamp))
}

/// Erstellt alle notwendigen Datenverzeichnisse, falls sie nicht existieren.
pub fn ensure_data_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(runs_dir())?;
    std::fs::create_dir_all(memory_dir())?;
    std::fs::create_dir_all(profiles_dir())?;
    std::fs::create_dir_all(bot2bot_root())?;
    Ok(())
}

/// Gibt `true` zurück, wenn shared_browser aktiviert ist (Umgebungsvariable).
/// Python-Name: `WEBAGENT_USE_SHARED_BROWSER`; Legacy-Alias: `WEBAGENT_SHARED_BROWSER`.
pub fn use_shared_browser() -> bool {
    let v = env::var("WEBAGENT_USE_SHARED_BROWSER")
        .or_else(|_| env::var("WEBAGENT_SHARED_BROWSER"))
        .unwrap_or_default();
    matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Tabs zwischen Relay-Hops offen halten. Default: an wenn shared browser an.
pub fn persist_browser_tabs() -> bool {
    let v = env::var("WEBAGENT_PERSIST_TABS").unwrap_or_default();
    match v.trim().to_lowercase().as_str() {
        "0" | "false" | "no" | "off" => false,
        "1" | "true" | "yes" | "on" => true,
        _ => use_shared_browser(),
    }
}

/// Fester Debug-Port für den Shared-Browser-Pool (ein Chromium, viele Tabs).
pub fn shared_debug_port() -> u16 {
    env::var("WEBAGENT_SHARED_DEBUG_PORT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(9222)
}

/// selectors/-Verzeichnis (ROOT/selectors/<brain>.json), wie SELECTORS_DIR in config.py.
pub fn selectors_dir() -> PathBuf {
    root_dir().join("selectors")
}

/// Statische Brain-Tabelle: (id, url) — exakt wie das BRAINS-Dict in config.py.
pub const BRAIN_TABLE: &[(&str, &str)] = &[
    ("chatgpt", "https://chatgpt.com/"),
    ("deepseek", "https://chat.deepseek.com/"),
    ("kimi", "https://www.kimi.com/"),
    ("gemini", "https://gemini.google.com/app"),
    ("qwen", "https://chat.qwen.ai/"),
    ("claude", "https://claude.ai/new"),
    ("mistral", "https://chat.mistral.ai/chat"),
    ("zai", "https://chat.z.ai/"),
];

/// Brain-Definitionen: ID -> {url, selectors, profile_dir}.
///
/// Portiert aus BRAINS-Dict in config.py. Selektoren liegen unter
/// ROOT/selectors/<brain>.json; jedes Brain erhaelt ein eigenes Profil unter
/// profiles/<brain> (Referenzprofil-Ansatz), das doctor prueft.
pub fn brains() -> HashMap<String, HashMap<String, String>> {
    let sel = selectors_dir();
    let profiles = profiles_dir();
    // Optionaler Override: alle Brains dasselbe Profil nutzen lassen (z.B. das
    // eingeloggte Shared-Profil des Python-webagent) via WEBAGENT_PROFILE_DIR.
    let profile_override = env::var("WEBAGENT_PROFILE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut brains = HashMap::new();
    for (id, url) in BRAIN_TABLE {
        let mut b = HashMap::new();
        b.insert("url".to_string(), url.to_string());
        b.insert(
            "selectors".to_string(),
            sel.join(format!("{id}.json")).to_string_lossy().to_string(),
        );
        let profile_dir = profile_override
            .clone()
            .unwrap_or_else(|| profiles.join(id).to_string_lossy().to_string());
        b.insert("profile_dir".to_string(), profile_dir);
        brains.insert(id.to_string(), b);
    }
    brains
}

/// profiles/reference/<brain_id> — kanonisches, vom Menschen gepflegtes
/// Referenzprofil (Cookies/Storage eingeloggt). Wird NICHT von der Automation
/// beschrieben; nur gelesen und als Vorlage für Laufzeit-Kopien genutzt.
/// Existiert das Verzeichnis nicht, greift der Fallback auf `profiles/<brain_id>`
/// bzw. das Shared-Profil zurück.
pub fn reference_profile_dir(brain_id: &str) -> PathBuf {
    reference_profile_dir_in(&profiles_dir(), brain_id)
}

/// Wie `reference_profile_dir`, aber mit expliziter Profil-Basis (für Tests).
pub fn reference_profile_dir_in(base: &Path, brain_id: &str) -> PathBuf {
    base.join("reference").join(brain_id)
}

/// profiles/swarm/<run_id>_<brain_id> — isolierte Laufzeit-Teilkopie eines
/// Referenzprofils für einen einzelnen Swarm-Teilnehmer. Vermeidet den
/// Chromium-`SingletonLock`-Konflikt, wenn mehrere Brains parallel im selben
/// Profil starten würden.
pub fn swarm_profile_dir(run_id: &str, brain_id: &str) -> PathBuf {
    swarm_profile_dir_in(&profiles_dir(), run_id, brain_id)
}

/// Wie `swarm_profile_dir`, aber mit expliziter Profil-Basis (für Tests).
pub fn swarm_profile_dir_in(base: &Path, run_id: &str, brain_id: &str) -> PathBuf {
    base.join("swarm")
        .join(format!("{}_{}", run_id, brain_id))
}

/// Kopiert ein Verzeichnis rekursiv (inkl. Unterverzeichnisse). Bricht nicht bei
/// einzelnen nicht-kopierbaren Dateien (z.B. Lock-Files), sondern überspringt
/// sie — für Profil-Kopien ausreichend und robuster als `fs::copy` im Loop.
pub fn copy_dir_all(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let mut copied: u32 = 0;
    let mut non_dir: u32 = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path().to_path_buf(), &target)?;
        } else {
            non_dir += 1;
            // Lock-Files u.ä. beim Kopieren ignorieren — sie werden neu erzeugt.
            let name = entry.file_name().to_string_lossy().to_lowercase();
            // Chromium/WebView2 Locks & PIDs — neu erzeugt, nicht kopieren.
            if name.contains("lock")
                || name == "singletoncookie"
                || name == "singletonsocket"
                || name.ends_with(".lock")
                || name == "lockfile"
            {
                continue;
            }
            if std::fs::copy(entry.path(), &target).is_ok() {
                copied += 1;
            }
        }
    }
    // Debug-Hinweis: Quelle hatte Dateien, aber keine wurde kopiert
    // (z.B. alles Lock-Files, oder Lese-Fehler) — sonst silently leer.
    if non_dir > 0 && copied == 0 {
        eprintln!(
            "[copy_dir_all] WARN: 0 von {non_dir} Dateien aus {:?} kopiert (alle uebersprungen oder Lese-Fehler)",
            src
        );
    }
    Ok(())
}

/// Kopiert nur die in SPARSE_COPY_WHITELIST gelisteten Dateien/Ordner aus `src`
/// nach `dst` (rekursiv). Entspricht der "sparse-copy" eines Referenzprofils:
/// nur login-relevante Artefakte statt der vollen Profilkopie. Lock-Files werden
/// wie in copy_dir_all übersprungen.
pub fn copy_dir_sparse(src: &PathBuf, dst: &PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let mut copied: u32 = 0;
    let mut non_dir: u32 = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let target = dst.join(entry.file_name());
        // Whitelist-Verzeichnisse vollstaendig kopieren (inkl. aller Dateien/Unterordner).
        if ty.is_dir()
            && SPARSE_COPY_WHITELIST
                .iter()
                .any(|w| w.eq_ignore_ascii_case(&name))
        {
            copy_dir_all(&entry.path().to_path_buf(), &target)?;
        }
        if ty.is_dir() {
            continue;
        }
        non_dir += 1;
        // Lock-Files u.ä. beim Kopieren ignorieren — sie werden neu erzeugt.
        let lower = name.to_lowercase();
        if lower.contains("lock")
            || lower == "singletoncookie"
            || lower == "singletonsocket"
            || lower.ends_with(".lock")
            || lower == "lockfile"
        {
            continue;
        }
        // Nur Whitelist-Dateien kopieren.
        if SPARSE_COPY_WHITELIST.iter().any(|w| w.eq_ignore_ascii_case(&name))
            && std::fs::copy(entry.path(), &target).is_ok()
        {
            copied += 1;
        }
    }
    if non_dir > 0 && copied == 0 {
        eprintln!(
            "[copy_dir_sparse] WARN: 0 von {non_dir} Dateien aus {:?} kopiert (Whitelist traf nicht zu oder Lese-Fehler)",
            src
        );
    }
    Ok(())
}

/// Bereitet das Profil für einen Swarm-Teilnehmer vor:
/// 1. Falls `profiles/reference/<brain_id>` existiert → Teilkopie nach
///    `profiles/swarm/<run_id>_<brain_id>`.
/// 2. Sonst Fallback auf das bestehende `profiles/<brain_id>` (falls vorhanden).
/// 3. Sonst leeres Verzeichnis (Neuanlage durch Browser).
///
/// Rückgabe: Pfad zum isolierten Profil für diesen Lauf.
pub fn prepare_swarm_profile(run_id: &str, brain_id: &str) -> PathBuf {
    prepare_swarm_profile_in(&profiles_dir(), run_id, brain_id, use_sparse_profile_copy())
}

/// Wie `prepare_swarm_profile`, aber mit expliziter Profil-Basis `base`
/// (statt `profiles_dir()`) und explizitem `sparse`-Flag (statt der globalen
/// WEBAGENT_SPARSE_COPY-Env). Ermöglicht isolierte, nebenläufige Tests ohne
/// Manipulation einer prozess-globalen Env-Variable.
pub fn prepare_swarm_profile_in(
    base: &Path,
    run_id: &str,
    brain_id: &str,
    sparse: bool,
) -> PathBuf {
    let reference = reference_profile_dir_in(base, brain_id);
    let default = base.join(brain_id);
    let dst = swarm_profile_dir_in(base, run_id, brain_id);

    // Alte Kopie dieses Runs entfernen, falls vorhanden (idempotent).
    if dst.exists() {
        let _ = std::fs::remove_dir_all(&dst);
    }

    if reference.is_dir() {
        let _ = copy_profile(&reference, &dst, sparse);
        return dst;
    }
    if default.is_dir() {
        let _ = copy_profile(&default, &dst, sparse);
        return dst;
    }
    // Weder Referenz noch Default: leeres Verzeichnis anlegen.
    let _ = std::fs::create_dir_all(&dst);
    dst
}

/// Kopiert ein Profil je nach Modus: sparse (nur Whitelist-Artefakte) oder voll.
fn copy_profile(src: &PathBuf, dst: &PathBuf, sparse: bool) -> std::io::Result<()> {
    if sparse {
        copy_dir_sparse(src, dst)
    } else {
        copy_dir_all(src, dst)
    }
}

/// Entfernt alle abgeschlossenen Swarm-Laufzeit-Profile (aufräumen nach einem Run).
pub fn cleanup_swarm_profiles(run_id: &str) -> std::io::Result<()> {
    cleanup_swarm_profiles_in(&profiles_dir(), run_id)
}

/// Wie `cleanup_swarm_profiles`, aber mit expliziter Profil-Basis (für Tests).
pub fn cleanup_swarm_profiles_in(base: &Path, run_id: &str) -> std::io::Result<()> {
    let swarm_root = base.join("swarm");
    if !swarm_root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&swarm_root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(&format!("{}_", run_id)) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

/// Einmalige Migration der Legacy-Profile/Data (CARGO_MANIFEST_DIR) an den
/// stabilen Ort. Idempotent + abort-sicher: pro Kindverzeichnis wird
/// copy_dir_all aufgerufen und danach remove (NICHT rename — rename scheitert
/// über Laufwerksgrenzen). Eine unterbrochene Migration ist beim nächsten Start
/// reparierbar (die Quelle wird bei Erfolg entfernt, bei Fehlschlag belassen
/// und erneut versucht).
pub fn ensure_stable_layout() {
    migrate_legacy_dir(&root_dir().join("profiles"), &profiles_dir());
    migrate_legacy_dir(&root_dir().join("data"), &data_dir());
}

fn migrate_legacy_dir(legacy: &Path, target: &Path) {
    if !legacy.is_dir() {
        return;
    }
    if let Err(e) = std::fs::create_dir_all(target) {
        eprintln!("[migrate] Ziel {:?} nicht anlegbar: {e}", target);
        return;
    }
    let entries = match std::fs::read_dir(legacy) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let src = entry.path();
        if !src.is_dir() {
            continue;
        }
        let name = match src.file_name() {
            Some(n) => n.to_string_lossy().into_owned(),
            None => continue,
        };
        let dst = target.join(&name);
        if copy_dir_all(&src, &dst).is_ok() {
            let _ = std::fs::remove_dir_all(&src);
        } else {
            eprintln!(
                "[migrate] Kopie von {:?} fehlgeschlagen, naechster Start erneut",
                src
            );
        }
    }
}

/// Laedt die Selektor-JSON eines Brains (wie config.load_selectors in Python).
pub fn load_selectors(brain_id: &str) -> std::io::Result<serde_json::Value> {
    let path = selectors_dir().join(format!("{brain_id}.json"));
    let content = std::fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Gibt die Liste aller verfügbaren Brain-IDs zurück (sortiert).
pub fn available_brain_ids() -> Vec<String> {
    let mut ids: Vec<String> = brains().keys().cloned().collect();
    ids.sort();
    ids
}

/// Deterministischer Chrome-Remote-Debugging-Port je Brain (kollisionsarm).
/// Basisport via `WEBAGENT_DEBUG_PORT_BASE` überschreibbar (Standard 9222).
pub fn debug_port(brain_id: &str) -> u16 {
    let base: u16 = env::var("WEBAGENT_DEBUG_PORT_BASE")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(9222);
    base.wrapping_add((fnv1a(brain_id) % 400) as u16)
}

/// FNV-1a-Hash (gemeinfrei) für die stabile Port-Zuteilung.
fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

/// profiles/encapsulated/<brain>_<runstamp> — gekapselte, isolierte Laufzeit-
/// Instanz (Linked-Clone/Delta des kanonischen Shared-Profils) fuer den Fallback,
/// wenn der geteilte Browser fuer ein Brain nicht startbar ist.
pub fn encapsulated_profile_dir(brain_id: &str, runstamp: &str) -> PathBuf {
    profiles_dir()
        .join("encapsulated")
        .join(format!("{brain_id}_{runstamp}"))
}

/// Klassifikation eines Profil-Eintrags fuer den Linked-Clone/Delta-Modus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloneClass {
    /// (A) Read-only: ueber Hardlink teilbar (same-volume) bzw. kopiert (cross-drive).
    Link,
    /// (B)-minimal: pro Instanz delta-kopiert (Login-relevant).
    Copy,
    /// Weglassen: Lockfiles oder login-irrelevante Mutable-Daten (neu erzeugt).
    Skip,
}

/// Read-only-Verzeichnisse/Dateien, die (A) verlinkt werden duerfen.
const READONLY_DIRS: &[&str] = &[
    "extensions",
    "pnacl",
    "subresource filter",
    "widevinecdm",
    "meipreload",
];

/// (B)-minimal: login-relevante Artefakte, die pro Instanz kopiert werden.
const MINIMAL_LOGIN_ARTIFACTS: &[&str] = &[
    "cookies",
    "login data",
    "web data",
    "local state",
    "preferences",
    "indexeddb",
    "local storage",
];

/// Klassifiziert einen Profil-Eintragsnamen (gross-/kleinschreibungsneutral).
fn classify(name: &str) -> CloneClass {
    let lower = name.to_lowercase();
    // Lockfiles IMMER weglassen — nie linken oder kopieren (neu erzeugt).
    if is_lockfile(&lower) {
        return CloneClass::Skip;
    }
    // (A) Read-only / linkbar
    if READONLY_DIRS.iter().any(|d| d.eq_ignore_ascii_case(name)) {
        return CloneClass::Link;
    }
    if lower.ends_with(".pak")
        || matches!(
            lower.as_str(),
            "icudtl.dat" | "snapshot_blob.bin" | "v8_context_snapshot.bin"
        )
    {
        return CloneClass::Link;
    }
    // (B)-minimal: Login-relevant -> kopieren
    if MINIMAL_LOGIN_ARTIFACTS
        .iter()
        .any(|a| a.eq_ignore_ascii_case(name))
    {
        return CloneClass::Copy;
    }
    // Alles uebrige (Rest (B): Journals, Cache, Network, Service Worker,
    // Session Storage, Secure Preferences, DataStore, History, ...) weglassen.
    CloneClass::Skip
}

/// True fuer Chromium/WebView2 Lockfiles (nie linken/kopieren).
fn is_lockfile(name: &str) -> bool {
    name.contains("lock")
        || name == "singletoncookie"
        || name == "singletonsocket"
        || name.ends_with(".lock")
        || name == "lockfile"
}

/// Geplante Linked-Clone/Delta-Kopie eines kanonischen Profils.
pub struct ProfileClonePlan {
    /// Kanonische Basis (read-only Quelle, z.B. profiles/shared).
    pub base: PathBuf,
    /// Ziel-Verzeichnis der gekapselten Instanz.
    pub dst: PathBuf,
    /// (A) Read-only-Eintraege: ueber Hardlink (same-volume) teilbar.
    pub links: Vec<PathBuf>,
    /// (B)-minimal: login-relevant, delta-kopiert.
    pub copies: Vec<PathBuf>,
    /// True, wenn base und dst auf demselben Volume (Hardlink erlaubt).
    pub same_volume: bool,
}

/// Eine klassifizierte Profil-Eintrags (Datei oder Verzeichnis).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloneEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Ergebnis von `ProfileClonePlanner::dry_run` — reine Klassifikation,
/// ohne das Dateisystem zu veraendern.
#[derive(Debug, Clone, Default)]
pub struct DryRunReport {
    /// (A) Read-only-Eintraege (verlinkbar).
    pub links: Vec<CloneEntry>,
    /// (B)-minimal login-relevante Eintraege (kopiert).
    pub copies: Vec<CloneEntry>,
    /// Weggelassene Eintraege (Lockfiles, Rest (B), Unbekanntes).
    pub skipped: Vec<CloneEntry>,
}

/// Plant und materialisiert Linked-Clone/Delta-Kopien kanonischer Profile.
pub struct ProfileClonePlanner;

impl ProfileClonePlanner {
    /// Plant den Klon einer kanonischen Basis nach `dst`. Klassifiziert alle
    /// Eintraege der Basis (ohne das Dateisystem zu veraendern) und berechnet
    /// die Volume-Gleichheit (`same_volume`) fuer die Link-Entscheidung.
    pub fn plan_canonical(base: &Path, dst: &Path, _runstamp: &str) -> ProfileClonePlan {
        let mut links = Vec::new();
        let mut copies = Vec::new();
        if let Ok(rd) = std::fs::read_dir(base) {
            for entry in rd.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                match classify(&name) {
                    CloneClass::Link => links.push(path),
                    CloneClass::Copy => copies.push(path),
                    CloneClass::Skip => {}
                }
            }
        }
        let same_volume = same_volume(base, dst);
        ProfileClonePlan {
            base: base.to_path_buf(),
            dst: dst.to_path_buf(),
            links,
            copies,
            same_volume,
        }
    }

    /// Fuehrt den Klon aus: (A) hard-link (mit Copy-Fallback) bzw. full-copy bei
    /// cross-drive; (B)-minimal kopiert; Lockfiles/Rest werden weggelassen.
    pub fn materialize(plan: &ProfileClonePlan) -> std::io::Result<()> {
        std::fs::create_dir_all(&plan.dst)?;
        // (A) Read-only: verlinken bzw. kopieren (rekursiv fuer Verzeichnisse).
        for src in &plan.links {
            let rel = src.strip_prefix(&plan.base).unwrap_or(src).to_path_buf();
            let target = plan.dst.join(&rel);
            if src.is_dir() {
                link_or_copy_dir(src, &target, plan.same_volume)?;
            } else {
                link_or_copy_file(src, &target, plan.same_volume)?;
            }
        }
        // (B)-minimal: delta-kopieren (Lockfiles innerhalb per copy_dir_all uebersprungen).
        for src in &plan.copies {
            let rel = src.strip_prefix(&plan.base).unwrap_or(src).to_path_buf();
            let target = plan.dst.join(&rel);
            if src.is_dir() {
                copy_dir_all(&src.to_path_buf(), &target)?;
            } else {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(src, &target)?;
            }
        }
        Ok(())
    }

    /// Meldet die Klassifikation einer Basis OHNE das Dateisystem zu veraendern
    /// (fuer Tests und Diagnose).
    pub fn dry_run(base: &Path) -> DryRunReport {
        let mut report = DryRunReport::default();
        if let Ok(rd) = std::fs::read_dir(base) {
            for entry in rd.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let e = CloneEntry { name, is_dir };
                match classify(&e.name) {
                    CloneClass::Link => report.links.push(e),
                    CloneClass::Copy => report.copies.push(e),
                    CloneClass::Skip => report.skipped.push(e),
                }
            }
        }
        report
    }
}

/// Verlinkt eine Datei (same-volume) bzw. kopiert sie (cross-drive oder
/// Hardlink-Fehler). Hardlink ist der v1-Link-Mechanismus: kein Admin, same-volume.
fn link_or_copy_file(src: &Path, dst: &Path, same_volume: bool) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if same_volume {
        // TODO: replace hard_link with ReFS/Dev-Drive CoW or junction once FS facts confirmed
        if std::fs::hard_link(src, dst).is_err() {
            std::fs::copy(src, dst)?;
        }
    } else {
        std::fs::copy(src, dst)?;
    }
    Ok(())
}

/// Verlinkt/kopiert ein Verzeichnis rekursiv (jede Datei einzeln ueber
/// `link_or_copy_file`, Unterverzeichnisse rekursiv).
fn link_or_copy_dir(src: &Path, dst: &Path, same_volume: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            link_or_copy_dir(&entry.path(), &target, same_volume)?;
        } else {
            link_or_copy_file(&entry.path(), &target, same_volume)?;
        }
    }
    Ok(())
}

/// True, wenn `a` und `b` auf demselben Volume liegen (Hardlink erlaubt).
/// Heuristik: kanonische Pfade vergleichen, Volume-Wurzel (Windows:
/// Laufwerksbuchstabe, z.B. "C:") heranziehen. Bei Unsicherheit → cross-drive (Copy).
fn same_volume(a: &Path, b: &Path) -> bool {
    volume_root_of(a) == volume_root_of(b)
}

/// Liefert die Volume-Wurzel eines Pfads (Windows: Laufwerkskomponente).
fn volume_root_of(path: &Path) -> PathBuf {
    // Pfad existiert evtl. noch nicht (z.B. Ziel vor `materialize`): dann den
    // naechsten existierenden Vorgänger kanonisieren, sonst unterscheiden sich
    // kanonischer Prefix (\\?\C:...) und roher Pfad (C:...) und same_volume
    // wuerde fälschlich false ergeben.
    let canon = if path.exists() {
        std::fs::canonicalize(path)
    } else {
        let mut p = path.to_path_buf();
        while !p.as_os_str().is_empty() && !p.exists() {
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => break,
            }
        }
        std::fs::canonicalize(&p)
    };
    let canon = canon.unwrap_or_else(|_| path.to_path_buf());
    volume_root(&canon)
}

#[cfg(windows)]
fn volume_root(path: &Path) -> PathBuf {
    if let Some(comp) = path.components().next() {
        return PathBuf::from(comp.as_os_str());
    }
    PathBuf::new()
}

#[cfg(not(windows))]
fn volume_root(_path: &Path) -> PathBuf {
    PathBuf::from("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_dir_exists() {
        let root = root_dir();
        assert!(root.exists(), "Root-Verzeichnis sollte existieren");
        assert!(root.is_dir(), "Root sollte ein Verzeichnis sein");
    }

    #[test]
    fn test_brains_config() {
        let brains = brains();
        assert!(
            !brains.is_empty(),
            "Mindestens ein Brain sollte konfiguriert sein"
        );

        // ChatGPT sollte vorhanden sein
        assert!(brains.contains_key("chatgpt"));
        let chatgpt = &brains["chatgpt"];
        assert!(chatgpt.contains_key("url"));
        assert!(chatgpt.contains_key("selectors"));
        assert!(chatgpt.contains_key("profile_dir"));
    }

    #[test]
    fn test_available_brain_ids() {
        let ids = available_brain_ids();
        assert!(!ids.is_empty());
        assert!(ids.contains(&"chatgpt".to_string()));

        // Sollte sortiert sein
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn test_debug_port_deterministic_and_in_range() {
        let p1 = debug_port("chatgpt");
        assert_eq!(p1, debug_port("chatgpt"), "deterministisch");
        assert!((9222..9622).contains(&p1), "in Range: {p1}");
        // Die 8 konfigurierten Brains sollten großteils verschiedene Ports haben.
        let ports: std::collections::HashSet<u16> =
            BRAIN_TABLE.iter().map(|(id, _)| debug_port(id)).collect();
        assert!(ports.len() >= 6, "zu viele Port-Kollisionen: {ports:?}");
    }

    #[test]
    fn test_parity_constants() {
        assert_eq!(MAX_OBSERVATION_CHARS, 12_000);
        assert_eq!(LOOP_GUARD_WARN_COUNT, 3);
        assert_eq!(LOOP_GUARD_ABORT_COUNT, 8);
    }

    #[test]
    fn test_persist_browser_tabs_defaults() {
        let shared_key = "WEBAGENT_USE_SHARED_BROWSER";
        let tabs_key = "WEBAGENT_PERSIST_TABS";
        let prev_shared = env::var(shared_key).ok();
        let prev_tabs = env::var(tabs_key).ok();
        env::set_var(shared_key, "1");
        env::remove_var(tabs_key);
        assert!(persist_browser_tabs());
        env::set_var(tabs_key, "0");
        assert!(!persist_browser_tabs());
        match prev_tabs {
            Some(v) => env::set_var(tabs_key, v),
            None => env::remove_var(tabs_key),
        }
        match prev_shared {
            Some(v) => env::set_var(shared_key, v),
            None => env::remove_var(shared_key),
        }
    }

    #[test]
    fn test_use_shared_browser_env_names() {
        let key = "WEBAGENT_USE_SHARED_BROWSER";
        let prev = env::var(key).ok();
        env::set_var(key, "1");
        assert!(use_shared_browser());
        env::set_var(key, "0");
        assert!(!use_shared_browser());
        match prev {
            Some(v) => env::set_var(key, v),
            None => env::remove_var(key),
        }
    }

    #[test]
    fn test_ensure_data_dirs() {
        // Sollte nicht fehlschlagen (erstellt Verzeichnisse oder sie existieren bereits)
        assert!(ensure_data_dirs().is_ok());
        assert!(data_dir().exists());
        assert!(runs_dir().exists());
    }

    #[test]
    fn test_prepare_swarm_profile_fallback_and_cleanup() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_prep_{}", stamp));
        let _ = fs::create_dir_all(&base);
        let run_id = format!("testswarm_{}", stamp);
        let brain = "chatgpt";
        let default = base.join(brain);
        let marker_src = default.join("_grok_swarm_marker.txt");
        let _ = fs::create_dir_all(&default);
        fs::write(&marker_src, b"swarm-src").expect("write marker");
        let _ = fs::write(default.join("SingletonLock"), b"pid");
        let _ = fs::write(default.join("lockfile"), b"x");

        let dst = prepare_swarm_profile_in(&base, &run_id, brain, false);
        assert!(dst.is_dir(), "swarm profile dir");
        assert!(
            dst.join("_grok_swarm_marker.txt").is_file(),
            "marker copied from profiles/<brain>"
        );
        assert!(
            !dst.join("SingletonLock").exists(),
            "lock files must be skipped"
        );
        assert!(!dst.join("lockfile").exists());

        cleanup_swarm_profiles_in(&base, &run_id).expect("cleanup");
        assert!(!dst.exists(), "cleaned after run");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_swarm_and_reference_paths() {
        let r = reference_profile_dir("claude");
        assert!(r.ends_with(std::path::Path::new("reference").join("claude")));
        let s = swarm_profile_dir("run1", "claude");
        let lossy = s.to_string_lossy();
        assert!(lossy.contains("swarm"));
        assert!(lossy.contains("run1_claude"));
    }

    #[test]
    fn test_copy_dir_sparse_keeps_only_whitelist() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let src = root_dir().join(format!("data/test_sparse_src_{}", stamp));
        let dst = root_dir().join(format!("data/test_sparse_dst_{}", stamp));
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&src).unwrap();

        // Whitelist-Dateien/Ordner
        fs::write(src.join("Cookies"), b"cookies").unwrap();
        fs::write(src.join("Login Data"), b"login").unwrap();
        fs::write(src.join("Preferences"), b"prefs").unwrap();
        fs::create_dir_all(src.join("Local Storage")).unwrap();
        fs::write(src.join("Local Storage").join("x"), b"ls").unwrap();
        // Nicht-Whitelist
        fs::write(src.join("History"), b"history").unwrap();
        fs::write(src.join("Bookmarks"), b"bm").unwrap();
        // Lock-File
        fs::write(src.join("SingletonLock"), b"pid").unwrap();

        copy_dir_sparse(&src.to_path_buf(), &dst.to_path_buf()).unwrap();

        assert!(dst.join("Cookies").is_file(), "Cookies (whitelist) kopiert");
        assert!(dst.join("Login Data").is_file(), "Login Data (whitelist) kopiert");
        assert!(
            dst.join("Preferences").is_file(),
            "Preferences (whitelist) kopiert"
        );
        assert!(
            dst.join("Local Storage").join("x").is_file(),
            "Local Storage (whitelist) kopiert"
        );

        assert!(
            !dst.join("History").exists(),
            "History (nicht whitelist) nicht kopiert"
        );
        assert!(
            !dst.join("Bookmarks").exists(),
            "Bookmarks (nicht whitelist) nicht kopiert"
        );
        assert!(!dst.join("SingletonLock").exists(), "Lock-File nicht kopiert");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_prepare_swarm_profile_respects_sparse_env() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_sparse_{}", stamp));
        let _ = fs::create_dir_all(&base);
        let run_id = format!("testsparse_{}", stamp);
        let brain = "chatgpt";
        let reference = reference_profile_dir_in(&base, brain);
        let _ = fs::create_dir_all(&reference);
        fs::write(reference.join("Cookies"), b"c").unwrap();
        fs::write(reference.join("History"), b"h").unwrap();
        fs::write(reference.join("SingletonLock"), b"pid").unwrap();

        // explizit sparse (kein globales Env -> nebenlaeufig sicher)
        let dst = prepare_swarm_profile_in(&base, &run_id, brain, true);
        assert!(dst.join("Cookies").is_file(), "sparse: Cookies kopiert");
        assert!(
            !dst.join("History").exists(),
            "sparse: History nicht kopiert"
        );
        assert!(
            !dst.join("SingletonLock").exists(),
            "sparse: Lock nicht kopiert"
        );

        cleanup_swarm_profiles_in(&base, &run_id).unwrap();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_clone_planner_dry_run_classification() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_clone_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // (A) Read-only -> link
        fs::write(base.join("resources.pak"), b"pak").unwrap();
        fs::write(base.join("chrome_100_percent.pak"), b"pak").unwrap();
        fs::write(base.join("icudtl.dat"), b"dat").unwrap();
        fs::write(base.join("snapshot_blob.bin"), b"bin").unwrap();
        fs::write(base.join("v8_context_snapshot.bin"), b"bin").unwrap();
        fs::create_dir_all(base.join("Extensions")).unwrap();
        fs::write(base.join("Extensions").join("ext.pak"), b"e").unwrap();
        fs::create_dir_all(base.join("pnacl")).unwrap();
        fs::create_dir_all(base.join("Subresource Filter")).unwrap();
        fs::create_dir_all(base.join("WidevineCdm")).unwrap();
        fs::create_dir_all(base.join("MEIPreload")).unwrap();

        // (B)-minimal -> copy
        fs::write(base.join("Cookies"), b"c").unwrap();
        fs::write(base.join("Login Data"), b"l").unwrap();
        fs::write(base.join("Web Data"), b"w").unwrap();
        fs::write(base.join("Local State"), b"s").unwrap();
        fs::write(base.join("Preferences"), b"p").unwrap();
        fs::create_dir_all(base.join("IndexedDB")).unwrap();
        fs::create_dir_all(base.join("Local Storage")).unwrap();

        // Rest (B) + Lockfiles -> skipped
        fs::write(base.join("Cookies-journal"), b"cj").unwrap();
        fs::write(base.join("Login Data-journal"), b"lj").unwrap();
        fs::write(base.join("Web Data-journal"), b"wj").unwrap();
        fs::write(base.join("Secure Preferences"), b"sp").unwrap();
        fs::create_dir_all(base.join("Service Worker")).unwrap();
        fs::create_dir_all(base.join("Cache")).unwrap();
        fs::create_dir_all(base.join("Code Cache")).unwrap();
        fs::create_dir_all(base.join("Session Storage")).unwrap();
        fs::create_dir_all(base.join("Network")).unwrap();
        fs::write(base.join("History"), b"h").unwrap();
        fs::write(base.join("SingletonLock"), b"pid").unwrap();
        fs::write(base.join("lockfile"), b"x").unwrap();

        let report = ProfileClonePlanner::dry_run(&base);
        let link_names: std::collections::HashSet<String> =
            report.links.iter().map(|e| e.name.clone()).collect();
        let copy_names: std::collections::HashSet<String> =
            report.copies.iter().map(|e| e.name.clone()).collect();
        let skip_names: std::collections::HashSet<String> =
            report.skipped.iter().map(|e| e.name.clone()).collect();

        // (A) -> links
        for a in [
            "resources.pak",
            "chrome_100_percent.pak",
            "icudtl.dat",
            "snapshot_blob.bin",
            "v8_context_snapshot.bin",
            "Extensions",
            "pnacl",
            "Subresource Filter",
            "WidevineCdm",
            "MEIPreload",
        ] {
            assert!(link_names.contains(a), "(A) '{a}' sollte link sein");
        }
        // (B)-minimal -> copies
        for b in [
            "Cookies",
            "Login Data",
            "Web Data",
            "Local State",
            "Preferences",
            "IndexedDB",
            "Local Storage",
        ] {
            assert!(copy_names.contains(b), "(B)-minimal '{b}' sollte copy sein");
        }
        // Rest (B) + Unbekanntes -> skipped
        for s in [
            "Cookies-journal",
            "Login Data-journal",
            "Web Data-journal",
            "Secure Preferences",
            "Service Worker",
            "Cache",
            "Code Cache",
            "Session Storage",
            "Network",
            "History",
        ] {
            assert!(skip_names.contains(s), "Rest( B) '{s}' sollte skipped sein");
        }
        // Lockfiles aus beiden (links UND copies) ausgelassen
        assert!(
            !link_names.contains("SingletonLock"),
            "Lockfile darf nicht gelinkt werden"
        );
        assert!(
            !copy_names.contains("SingletonLock"),
            "Lockfile darf nicht kopiert werden"
        );
        assert!(skip_names.contains("SingletonLock"), "Lockfile skipped");
        assert!(skip_names.contains("lockfile"), "lockfile skipped");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_clone_planner_materialize_links_and_omits_locks() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_mat_{}", stamp));
        let dst = std::env::temp_dir().join(format!("webagent_mat_dst_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&base).unwrap();

        // (A) Datei + (A) Verzeichnis
        fs::write(base.join("resources.pak"), b"PAK-A").unwrap();
        fs::create_dir_all(base.join("Extensions")).unwrap();
        fs::write(base.join("Extensions").join("ext.pak"), b"PAK-B").unwrap();
        // (B)-minimal Datei + Verzeichnis
        fs::write(base.join("Cookies"), b"CK").unwrap();
        fs::create_dir_all(base.join("Local Storage")).unwrap();
        fs::write(base.join("Local Storage").join("ls.txt"), b"LS").unwrap();
        // Lockfile + Rest
        fs::write(base.join("SingletonLock"), b"pid").unwrap();
        fs::write(base.join("Cookies-journal"), b"cj").unwrap();
        fs::write(base.join("History"), b"h").unwrap();

        let plan = ProfileClonePlanner::plan_canonical(&base, &dst, "run1");
        ProfileClonePlanner::materialize(&plan).expect("materialize");

        // (A) verlinkt/kopiert, (B)-minimal kopiert
        assert!(dst.join("resources.pak").is_file(), "(A) Datei vorhanden");
        assert!(
            dst.join("Extensions").join("ext.pak").is_file(),
            "(A) Verzeichnis rekursiv verarbeitet"
        );
        assert!(dst.join("Cookies").is_file(), "(B)-minimal Datei kopiert");
        assert!(
            dst.join("Local Storage").join("ls.txt").is_file(),
            "(B)-minimal Verzeichnis kopiert"
        );
        // Lockfiles + Rest weggelassen
        assert!(
            !dst.join("SingletonLock").exists(),
            "Lockfile nicht im Klon"
        );
        assert!(
            !dst.join("Cookies-journal").exists(),
            "Journal nicht im Klon"
        );
        assert!(!dst.join("History").exists(), "Rest( B) nicht im Klon");

        // (A) wird auf same-volume ueber Hardlink geteilt: Mutation der Basis
        // spiegelt sich im Klon (gleiche Inode).
        assert!(plan.same_volume, "same-volume erkannt");
        fs::write(base.join("resources.pak"), b"PAK-A-MUT").unwrap();
        let linked = fs::read_to_string(dst.join("resources.pak")).unwrap();
        assert_eq!(linked, "PAK-A-MUT", "(A) ist Hardlink (geteilt)");
        fs::write(base.join("Extensions").join("ext.pak"), b"PAK-B-MUT").unwrap();
        let linked2 = fs::read_to_string(dst.join("Extensions").join("ext.pak")).unwrap();
        assert_eq!(linked2, "PAK-B-MUT", "(A) Verzeichnis-Datei ist Hardlink");

        // (B)-minimal ist eine echte Kopie: Mutation der Basis aendert den Klon NICHT.
        fs::write(base.join("Cookies"), b"CK-MUT").unwrap();
        let copied = fs::read_to_string(dst.join("Cookies")).unwrap();
        assert_eq!(copied, "CK", "(B)-minimal ist Kopie (nicht geteilt)");

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_clone_planner_cross_drive_copies() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_xd_{}", stamp));
        let dst = std::env::temp_dir().join(format!("webagent_xd_dst_{}", stamp));
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
        fs::create_dir_all(&base).unwrap();

        fs::write(base.join("resources.pak"), b"PAK-A").unwrap();
        fs::write(base.join("Cookies"), b"CK").unwrap();
        fs::write(base.join("SingletonLock"), b"pid").unwrap();

        // Klassifikation uebernehmen, aber Volume-Gleichheit erzwingen=false
        // (simuliert cross-drive: alles wird kopiert, nichts gelinkt).
        let mut plan = ProfileClonePlanner::plan_canonical(&base, &dst, "run1");
        plan.same_volume = false;
        ProfileClonePlanner::materialize(&plan).expect("materialize");

        assert!(dst.join("resources.pak").is_file(), "(A) kopiert cross-drive");
        assert!(dst.join("Cookies").is_file(), "(B)-minimal kopiert");
        assert!(!dst.join("SingletonLock").exists(), "Lock weggelassen");

        // Copy, kein Hardlink: Mutation der Basis aendert den Klon nicht.
        fs::write(base.join("resources.pak"), b"PAK-A-MUT").unwrap();
        let content = fs::read_to_string(dst.join("resources.pak")).unwrap();
        assert_eq!(
            content, "PAK-A",
            "cross-drive: (A) ist Kopie, keine geteilte Inode"
        );

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_encapsulated_profile_dir_path() {
        let p = encapsulated_profile_dir("chatgpt", "run42");
        assert!(p.to_string_lossy().contains("encapsulated"));
        assert!(p.to_string_lossy().contains("chatgpt_run42"));
    }
}
