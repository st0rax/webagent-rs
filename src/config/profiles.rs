use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::brains::{
    reference_profile_dir_in, swarm_profile_dir_in, swarm_profile_scope_key,
    use_sparse_profile_copy, FULL_COPY_SKIP_DIRS, SPARSE_COPY_WHITELIST, SPARSE_SKIP_DIRS,
};
use super::paths::*;

/// Unix seconds; heartbeat or lease within this window is considered live.
pub(crate) const HEARTBEAT_FRESH_SECS: i64 = 120;

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
        crate::bench_events::eprint_line(&format!(
            "[copy_dir_all] WARN: 0 von {non_dir} Dateien aus {:?} kopiert (alle uebersprungen oder Lese-Fehler)",
            src
        ));
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
    // REKURSIV: WebView2 legt alles unter `EBWebView/Default/…` ab (Cookies
    // sogar unter `Default/Network/Cookies`). Eine Suche nur auf der obersten
    // Ebene traf die Whitelist deshalb NIE — die Kopie blieb leer und das Brain
    // wirkte ausgeloggt (Fund 2026-07-21: 6 von 8 Swarm-Profilen waren leer).
    copy_sparse_rec(src, dst, &mut copied, &mut non_dir, 0)?;
    if non_dir > 0 && copied == 0 {
        crate::bench_events::eprint_line(&format!(
            "[copy_dir_sparse] WARN: 0 von {non_dir} Dateien aus {:?} kopiert (Whitelist traf nicht zu oder Lese-Fehler)",
            src
        ));
    }
    // Das Master ist versiegelt (read-only). `fs::copy` uebernimmt das Attribut
    // in die Laufzeit-Kopie — ein read-only Klon kann WebView2 aber NIE
    // beschreiben: Cookies und Local-State-Flush scheiterten still (gemessen
    // 07.08.2026: Cookie-DB stand seit 17:17:59, nur Browser-neu erzeugte
    // LevelDB-Dateien waren beschreibbar). Die Kopie muss beschreibbar sein,
    // das Siegel bleibt am Master.
    clear_readonly_recursive(dst);
    Ok(())
}

/// Strikte Variante der Sparse-Kopie fuer irreversible Write-back-Pfade.
///
/// Laufzeit- und Swarm-Klone duerfen bewusst best effort bleiben: Lock-Dateien
/// und noch offene WebView-Handles werden dort neu erzeugt. Beim Ueberschreiben
/// des Master-Profils waere ein stiller Teilfehler dagegen Datenverlust. Diese
/// Variante propagiert deshalb jeden nicht absichtlich uebersprungenen I/O-Fehler.
pub(super) fn copy_dir_sparse_strict(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    copy_sparse_rec_strict(src, dst, 0)?;
    clear_readonly_recursive_strict(dst)?;
    Ok(())
}

/// Stellt einen zuvor strikt gesicherten Sparse-Profilzustand vollstÃ¤ndig wieder her.
///
/// Der normale strikte Kopierer arbeitet absichtlich als Overlay. Beim Rollback
/// wÃ¤re das unzureichend: Artefakte, die der fehlgeschlagene Runtime-Write-back
/// zusÃ¤tzlich angelegt hat, dÃ¼rften nicht neben dem Backup bestehen bleiben.
pub(super) fn restore_sparse_backup(src: &Path, dst: &Path) -> std::io::Result<()> {
    clear_sparse_targets(dst, 0)?;
    copy_dir_sparse_strict(src, dst)
}

fn clear_sparse_targets(dir: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 6 || !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ty = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let hit = SPARSE_COPY_WHITELIST
            .iter()
            .any(|whitelist| whitelist.eq_ignore_ascii_case(&name));
        if ty.is_dir() {
            if SPARSE_SKIP_DIRS
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            if hit {
                std::fs::remove_dir_all(path)?;
            } else {
                clear_sparse_targets(&path, depth + 1)?;
            }
        } else if hit {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}
fn copy_dir_all_strict(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all_strict(&entry.path(), &target)?;
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains("lock")
            || name == "singletoncookie"
            || name == "singletonsocket"
            || name.ends_with(".lock")
            || name == "lockfile"
        {
            continue;
        }
        std::fs::copy(entry.path(), target)?;
    }
    Ok(())
}

fn copy_sparse_rec_strict(src: &Path, dst: &Path, depth: usize) -> std::io::Result<()> {
    if depth > 6 {
        return Ok(());
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let target = dst.join(entry.file_name());
        let hit = SPARSE_COPY_WHITELIST
            .iter()
            .any(|whitelist| whitelist.eq_ignore_ascii_case(&name));
        if ty.is_dir() {
            if SPARSE_SKIP_DIRS
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            if hit {
                copy_dir_all_strict(&entry.path(), &target)?;
            } else {
                copy_sparse_rec_strict(&entry.path(), &target, depth + 1)?;
            }
            continue;
        }
        let lower = name.to_lowercase();
        if lower.contains("lock")
            || lower == "singletoncookie"
            || lower == "singletonsocket"
            || lower.ends_with(".lock")
            || lower == "lockfile"
        {
            continue;
        }
        if hit {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
/// Entfernt das Read-only-Attribut rekursiv (Klon muss beschreibbar sein).
/// Strict counterpart to the best-effort runtime clone permission reset.
#[cfg(windows)]
fn clear_readonly_recursive_strict(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            clear_readonly_recursive_strict(&path)?;
        }
        let metadata = entry.metadata()?;
        if metadata.permissions().readonly() {
            let mut permissions = metadata.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(path, permissions)?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn clear_readonly_recursive_strict(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}
#[cfg(windows)]
fn clear_readonly_recursive(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ty) = entry.file_type() else { continue };
        if ty.is_dir() {
            clear_readonly_recursive(&path);
        }
        let Ok(md) = entry.metadata() else { continue };
        if !md.permissions().readonly() {
            continue;
        }
        let mut perm = md.permissions();
        // Readonly-Flag entfernen (Windows: sonst scheitert das Löschen). clippy
        // warnt, weil set_readonly(false) auf Unix world-writable machen kann —
        // hier ist der Aufruf bewusst Teil des Windows-Aufräumens.
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = std::fs::set_permissions(&path, perm);
    }
}

#[cfg(not(windows))]
fn clear_readonly_recursive(_dir: &Path) {}

/// Rekursiver Helfer für [`copy_dir_sparse`]: kopiert Whitelist-Treffer
/// (Datei ODER Verzeichnis) an ihrer relativen Position und steigt in alle
/// übrigen Verzeichnisse ab, um verschachtelte Artefakte zu finden.
/// Cache-Verzeichnisse werden übersprungen, damit die Kopie sparsam bleibt.
fn copy_sparse_rec(
    src: &Path,
    dst: &Path,
    copied: &mut u32,
    non_dir: &mut u32,
    depth: usize,
) -> std::io::Result<()> {
    // Profile sind flach genug; die Grenze verhindert Endlosläufe bei Symlinks.
    if depth > 6 {
        return Ok(());
    }
    let entries = match std::fs::read_dir(src) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    for entry in entries.flatten() {
        let Ok(ty) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        let target = dst.join(entry.file_name());
        let hit = SPARSE_COPY_WHITELIST
            .iter()
            .any(|w| w.eq_ignore_ascii_case(&name));

        if ty.is_dir() {
            if SPARSE_SKIP_DIRS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            if hit {
                // Whitelist-Verzeichnis vollständig übernehmen (z.B. Network/).
                if copy_dir_all(&entry.path().to_path_buf(), &target).is_ok() {
                    *copied += 1;
                }
            } else {
                copy_sparse_rec(&entry.path(), &target, copied, non_dir, depth + 1)?;
            }
            continue;
        }

        *non_dir += 1;
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
        if hit {
            // Zielverzeichnis erst anlegen, wenn wirklich etwas hineinkommt.
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if std::fs::copy(entry.path(), &target).is_ok() {
                *copied += 1;
            }
        }
    }
    Ok(())
}

const SWARM_OWNER_FILE: &str = ".webagent-swarm-owner.json";
/// Version 2: added pid, process_started_at, generation, heartbeat fields for
/// cross-process lease visibility (T-804).
const SWARM_OWNER_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SwarmProfileOwner {
    version: u32,
    run_id: String,
    brain_id: String,
    scope_key: String,
    pid: u32,
    #[serde(default)]
    process_started_at: i64,
    #[serde(default)]
    generation: u64,
    #[serde(default)]
    heartbeat: Option<i64>,
}

impl SwarmProfileOwner {
    fn new(run_id: &str, brain_id: &str) -> Self {
        Self {
            version: SWARM_OWNER_VERSION,
            run_id: run_id.to_string(),
            brain_id: brain_id.to_string(),
            scope_key: swarm_profile_scope_key(run_id, brain_id),
            pid: std::process::id(),
            process_started_at: now_secs(),
            generation: 0,
            heartbeat: Some(now_secs()),
        }
    }
}

/// Exclusive ownership of one isolated swarm profile. The on-disk owner marker
/// binds cleanup to the exact run and brain; `release` is safe to call more
/// than once, and `Drop` provides best-effort cleanup on early returns.
#[derive(Debug)]
pub struct SwarmProfileLease {
    owner: SwarmProfileOwner,
    profile_dir: PathBuf,
    released: bool,
}

impl SwarmProfileLease {
    pub fn profile_dir(&self) -> &Path {
        &self.profile_dir
    }

    pub fn run_id(&self) -> &str {
        &self.owner.run_id
    }

    pub fn brain_id(&self) -> &str {
        &self.owner.brain_id
    }

    pub fn scope_key(&self) -> &str {
        &self.owner.scope_key
    }

    pub fn generation(&self) -> u64 {
        self.owner.generation
    }

    pub fn pid(&self) -> u32 {
        self.owner.pid
    }

    /// Refresh the on-disk heartbeat to the current time. Call this while the
    /// browser is alive so other processes can detect this lease as active.
    pub fn heartbeat_now(&mut self) -> std::io::Result<()> {
        self.owner.heartbeat = Some(now_secs());
        write_swarm_owner(&self.profile_dir, &self.owner)
    }

    pub fn release(&mut self) -> std::io::Result<()> {
        if self.released {
            return Ok(());
        }
        // If another process wrote a different owner, refuse without touching
        // the marker — we must not silently overwrite a foreign lease.
        let on_disk = read_swarm_owner(&self.profile_dir)
            .map_err(|error| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "refusing release: cannot read owner at {}: {error}",
                        self.profile_dir.display()
                    ),
                )
            })?;
        if on_disk != self.owner {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing release of profile owned by run={} brain={} (we own run={} brain={})",
                    on_disk.run_id, on_disk.brain_id, self.owner.run_id, self.owner.brain_id
                ),
            ));
        }
        // Clear the heartbeat before removing so other processes see the
        // lease as released even if the directory delete is delayed.
        self.owner.heartbeat = None;
        let _ = write_swarm_owner(&self.profile_dir, &self.owner);
        release_swarm_profile(&self.profile_dir, &self.owner)?;
        self.released = true;
        Ok(())
    }
}

impl Drop for SwarmProfileLease {
    fn drop(&mut self) {
        if let Err(error) = self.release() {
            crate::bench_events::eprint_line(&format!(
                "[profile] lease cleanup failed for run={} brain={}: {error}",
                self.owner.run_id, self.owner.brain_id
            ));
        }
    }
}

/// True if a swarm profile for `brain_id` has a fresh heartbeat (owned by a
/// live process). Used to prevent probes from running while a profile is held.
pub fn is_profile_leased(brain_id: &str) -> bool {
    is_profile_leased_in(&profiles_dir(), brain_id)
}

/// Wie [`is_profile_leased`], aber mit expliziter Profil-Basis (für Tests).
pub fn is_profile_leased_in(base: &Path, brain_id: &str) -> bool {
    let swarm_root = base.join("swarm");
    if !swarm_root.is_dir() {
        return false;
    }
    let cutoff = now_secs() - HEARTBEAT_FRESH_SECS;
    for entry in std::fs::read_dir(&swarm_root).into_iter().flatten() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let owner = match read_swarm_owner(&entry.path()) {
            Ok(o) => o,
            Err(_) => continue,
        };
        if owner.brain_id != brain_id || owner.version != SWARM_OWNER_VERSION {
            continue;
        }
        if let Some(hb) = owner.heartbeat {
            if hb >= cutoff {
                return true;
            }
        }
    }
    false
}

/// Block until the swarm profile for `brain_id` is free (no fresh heartbeat),
/// up to `timeout`. Returns Ok(()) when free, Err if still leased after timeout.
pub fn wait_for_profile_free(brain_id: &str, timeout: std::time::Duration) -> std::io::Result<()> {
    wait_for_profile_free_in(&profiles_dir(), brain_id, timeout)
}

pub(crate) fn wait_for_profile_free_in(
    base: &Path,
    brain_id: &str,
    timeout: std::time::Duration,
) -> std::io::Result<()> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if !is_profile_leased_in(base, brain_id) {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "profile for brain '{}' still leased after {:?}",
                    brain_id, timeout
                ),
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

/// Nimmt einen Scope nach einem Prozessabsturz wieder auf (T-804:
/// „Wiederaufnahme"). Voraussetzungen, alle der Reihe nach geprueft:
///
/// 1. `is_profile_leased_in` muss den Scope als **frei** melden — ein
///    abgestuerzter Prozess laesst sein Heartbeat-Feld alt werden, sodass
///    dieser Check nach `HEARTBEAT_FRESH_SECS` kippt. Bei frischem Heartbeat
///    wird NICHT wiedergeklaut (fail-closed).
/// 2. Der alte Owner-Marker muss lesbar sein und **exakt** zum gesuchten
///    Scope passen. Ein unlesbarer, fremder oder format-unbekannter Marker
///    blockiert die Wiederaufnahme — nie blind recyceln.
/// 3. Erst dann wird das alte Verzeichnis entfernt und ein frischer Lease
///    auf demselben Scope angelegt.
pub fn reclaim_swarm_profile_in(
    base: &Path,
    run_id: &str,
    brain_id: &str,
    sparse: bool,
    timeout: std::time::Duration,
) -> std::io::Result<SwarmProfileLease> {
    wait_for_profile_free_in(base, brain_id, timeout)?;
    let dst = swarm_profile_dir_in(base, run_id, brain_id);
    if dst.is_dir() {
        let actual = match read_swarm_owner(&dst) {
            Ok(o) => o,
            Err(error) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "refusing reclaim without readable owner marker at {}: {error}",
                        dst.display()
                    ),
                ));
            }
        };
        if actual.version != SWARM_OWNER_VERSION
            || actual.scope_key != swarm_profile_scope_key(&actual.run_id, &actual.brain_id)
            || actual.run_id != run_id
            || actual.brain_id != brain_id
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "refusing reclaim of profile owned by run={} brain={}",
                    actual.run_id, actual.brain_id
                ),
            ));
        }
        // Heartbeat ist hier bereits stale (Schritt 1) — altes Verzeichnis
        // entfernen und frisch aufsetzen.
        std::fs::remove_dir_all(&dst)?;
    }
    prepare_swarm_profile_in(base, run_id, brain_id, sparse)
}

/// Bereitet das Profil für einen Swarm-Teilnehmer vor:
/// 1. Falls `profiles/reference/<brain_id>` existiert → Teilkopie nach
///    `profiles/swarm/<run>_<brain>_<scope-key>`.
/// 2. Sonst Fallback auf das bestehende `profiles/<brain_id>` (falls vorhanden).
/// 3. Sonst leeres Verzeichnis (Neuanlage durch Browser).
///
/// Preparation is transactional and fail-closed: no lease is returned unless
/// cloning and durable ownership metadata both succeeded.
pub fn prepare_swarm_profile(run_id: &str, brain_id: &str) -> std::io::Result<SwarmProfileLease> {
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
) -> std::io::Result<SwarmProfileLease> {
    prepare_swarm_profile_in_with(base, run_id, brain_id, sparse, copy_profile_strict)
}

fn prepare_swarm_profile_in_with<F>(
    base: &Path,
    run_id: &str,
    brain_id: &str,
    sparse: bool,
    clone_profile: F,
) -> std::io::Result<SwarmProfileLease>
where
    F: FnOnce(&Path, &Path, bool) -> std::io::Result<()>,
{
    if run_id.trim().is_empty() || brain_id.trim().is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "swarm profile requires non-empty run_id and brain_id",
        ));
    }
    let reference = reference_profile_dir_in(base, brain_id);
    let default = base.join(brain_id);
    let dst = swarm_profile_dir_in(base, run_id, brain_id);

    // Atomically reserve the scope. Never reclaim an existing path here: it
    // may be an active lease, and deleting it would invalidate another caller.
    let parent = dst.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "swarm profile path has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    std::fs::create_dir(&dst).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("swarm profile scope is already leased: {}", dst.display()),
            )
        } else {
            error
        }
    })?;

    let prepared = if reference.is_dir() {
        clone_profile(&reference, &dst, sparse)
    } else if default.is_dir() {
        clone_profile(&default, &dst, sparse)
    } else {
        // Weder Referenz noch Default: leeres Verzeichnis für den Browser.
        Ok(())
    };
    if let Err(error) = prepared {
        remove_partial_profile(&dst);
        return Err(error);
    }

    let owner = SwarmProfileOwner::new(run_id, brain_id);
    if let Err(error) = write_swarm_owner(&dst, &owner) {
        remove_partial_profile(&dst);
        return Err(error);
    }
    Ok(SwarmProfileLease {
        owner,
        profile_dir: dst,
        released: false,
    })
}

fn copy_profile_strict(src: &Path, dst: &Path, sparse: bool) -> std::io::Result<()> {
    if sparse {
        copy_dir_sparse_strict(src, dst)
    } else {
        copy_dir_without_caches_strict(src, dst)
    }
}

fn copy_dir_without_caches_strict(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            if FULL_COPY_SKIP_DIRS
                .iter()
                .any(|skip| skip.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            copy_dir_without_caches_strict(&entry.path(), &target)?;
        } else {
            let lower = name.to_lowercase();
            if lower.contains("lock")
                || lower == "singletoncookie"
                || lower == "singletonsocket"
                || lower == "lockfile"
            {
                continue;
            }
            std::fs::copy(entry.path(), target)?;
        }
    }
    clear_readonly_recursive_strict(dst)
}

fn write_swarm_owner(dir: &Path, owner: &SwarmProfileOwner) -> std::io::Result<()> {
    let marker = dir.join(SWARM_OWNER_FILE);
    let pending = dir.join(format!("{SWARM_OWNER_FILE}.pending"));
    let bytes = serde_json::to_vec(owner)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    std::fs::write(&pending, bytes)?;
    std::fs::rename(pending, marker)
}

fn read_swarm_owner(dir: &Path) -> std::io::Result<SwarmProfileOwner> {
    let bytes = std::fs::read(dir.join(SWARM_OWNER_FILE))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn remove_partial_profile(path: &Path) {
    if path.exists() {
        let _ = remove_runtime_profile(path);
    }
}

/// Wie [`copy_dir_all`], laesst aber die Verzeichnisse aus
/// ``FULL_COPY_SKIP_DIRS`` weg. Bewusst eine eigene Funktion: `copy_dir_all`
/// bleibt der wortwoertliche Kopierer fuer alle anderen Aufrufer.
pub fn copy_dir_without_caches(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        let Ok(ty) = entry.file_type() else { continue };
        let name = entry.file_name().to_string_lossy().to_string();
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            if FULL_COPY_SKIP_DIRS
                .iter()
                .any(|s| s.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            copy_dir_without_caches(&entry.path(), &target)?;
        } else {
            // Lock-/PID-Dateien werden neu erzeugt (wie in copy_dir_all).
            let low = name.to_lowercase();
            if low.contains("lock")
                || low == "singletoncookie"
                || low == "singletonsocket"
                || low == "lockfile"
            {
                continue;
            }
            let _ = std::fs::copy(entry.path(), &target);
        }
    }
    Ok(())
}

/// Windows gibt WebView2-Dateien gelegentlich erst kurz nach `backend.stop()`
/// frei. Bereinigt daher eine reine Laufzeitkopie mit kleinen, begrenzten
/// Wiederholungen statt einen einzelnen Lock-Fehler dauerhaft zu verschlucken.
const RUNTIME_PROFILE_DELETE_ATTEMPTS: u32 = 20;

fn remove_runtime_profile(path: &Path) -> std::io::Result<()> {
    for attempt in 0..RUNTIME_PROFILE_DELETE_ATTEMPTS {
        // `fs::copy` kann das Read-only-Attribut aus dem Master übernehmen;
        // unter Windows muss es vor `remove_dir_all` entfernt werden.
        clear_readonly_recursive(path);
        match std::fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) if attempt + 1 == RUNTIME_PROFILE_DELETE_ATTEMPTS => {
                return Err(error);
            }
            Err(_) => {
                // Der Browser-Prozessbaum wurde bereits angehalten. Die kurze
                // Wartezeit deckt den Nachlauf von WebView2-Dateihandles ab.
                std::thread::sleep(std::time::Duration::from_millis(
                    50 * u64::from(attempt + 1),
                ));
            }
        }
    }
    unreachable!("mindestens ein Löschversuch wird ausgeführt")
}

/// Entfernt alle abgeschlossenen Swarm-Laufzeit-Profile (aufräumen nach einem Run).
pub fn cleanup_swarm_profiles(run_id: &str) -> std::io::Result<()> {
    cleanup_swarm_profiles_in(&profiles_dir(), run_id)
}

/// Compatibility cleanup for a whole run. Directory names are never trusted:
/// only profiles whose owner marker contains the exact run are removed.
pub fn cleanup_swarm_profiles_in(base: &Path, run_id: &str) -> std::io::Result<()> {
    let swarm_root = base.join("swarm");
    if !swarm_root.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&swarm_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let owner = match read_swarm_owner(&path) {
            Ok(owner) => owner,
            // Missing or malformed ownership is not authority to delete.
            Err(_) => continue,
        };
        if owner.version == SWARM_OWNER_VERSION
            && owner.run_id == run_id
            && owner.scope_key == swarm_profile_scope_key(&owner.run_id, &owner.brain_id)
            && swarm_profile_dir_in(base, &owner.run_id, &owner.brain_id) == path
        {
            release_swarm_profile(&path, &owner)?;
        }
    }
    Ok(())
}

fn release_swarm_profile(path: &Path, expected: &SwarmProfileOwner) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let actual = read_swarm_owner(path).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing cleanup without readable ownership marker at {}: {error}",
                path.display()
            ),
        )
    })?;
    if &actual != expected
        || actual.version != SWARM_OWNER_VERSION
        || actual.scope_key != swarm_profile_scope_key(&actual.run_id, &actual.brain_id)
        || path
            .parent()
            .and_then(Path::parent)
            .map(|base| swarm_profile_dir_in(base, &actual.run_id, &actual.brain_id) != path)
            .unwrap_or(true)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "refusing cleanup of profile owned by run={} brain={}",
                actual.run_id, actual.brain_id
            ),
        ));
    }
    remove_runtime_profile(path)
}

/// Ein Wegwerf-Profil, das älter als das hier ist, kann keinem laufenden Run
/// mehr gehören — ein Swarm-Turn dauert Minuten, nicht Stunden.
const STALE_RUNTIME_PROFILE_SECS: u64 = 12 * 60 * 60;

/// Profil-Unterverzeichnisse, die ausschliesslich Wegwerf-Laufzeitkopien
/// enthalten. Beide werden im Normalfall von einem `Drop`-Guard aufgeräumt
/// (`cleanup_swarm_profiles` bzw. `CloneGuard`/`stop_brain`) — und beide
/// lecken bei Absturz, Kill oder Stromausfall, weil der Guard dann nie läuft.
/// Kanonische Profile (`shared`, `<brain>`, `reference`) stehen bewusst NICHT
/// hier: dort liegen die Logins.
const DISPOSABLE_PROFILE_ROOTS: [&str; 2] = ["swarm", "encapsulated"];

/// Entfernt Wegwerf-Profile verwaister Runs. Netz unter den `Drop`-Guards, die
/// bei Absturz oder Kill nicht laufen — real hatten sich so 161 Profile /
/// 33,6 GB angesammelt. Gibt die Anzahl der entfernten Profile zurück.
pub fn sweep_stale_runtime_profiles() -> usize {
    sweep_stale_runtime_profiles_in(&profiles_dir(), STALE_RUNTIME_PROFILE_SECS)
}

/// Wie `sweep_stale_runtime_profiles`, aber mit expliziter Profil-Basis und
/// Altersgrenze (für Tests).
pub fn sweep_stale_runtime_profiles_in(base: &Path, max_age_secs: u64) -> usize {
    let cutoff = match std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(max_age_secs))
    {
        Some(c) => c,
        None => return 0,
    };
    let mut removed = 0;
    for root in DISPOSABLE_PROFILE_ROOTS {
        let entries = match std::fs::read_dir(base.join(root)) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let modified = entry.metadata().ok().and_then(|m| m.modified().ok());
            // Ohne lesbare mtime lieber stehen lassen als fremde Daten löschen.
            if let Some(m) = modified {
                if m < cutoff && remove_runtime_profile(&path).is_ok() {
                    removed += 1;
                }
            }
        }
    }
    removed
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
        crate::bench_events::eprint_line(&format!(
            "[migrate] Ziel {:?} nicht anlegbar: {e}",
            target
        ));
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
            crate::bench_events::eprint_line(&format!(
                "[migrate] Kopie von {:?} fehlgeschlagen, naechster Start erneut",
                src
            ));
        }
    }
}

#[cfg(test)]
mod lease_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    fn temp_base(label: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "webagent_swarm_lease_{label}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn failed_clone_returns_no_lease_and_removes_partial_profile() {
        let base = temp_base("clone_failure");
        let source = reference_profile_dir_in(&base, "chatgpt");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Cookies"), b"login").unwrap();
        let expected = swarm_profile_dir_in(&base, "run-a", "chatgpt");

        let result = prepare_swarm_profile_in_with(
            &base,
            "run-a",
            "chatgpt",
            false,
            |_src, dst, _sparse| {
                std::fs::create_dir_all(dst)?;
                std::fs::write(dst.join("partial"), b"x")?;
                Err(std::io::Error::other("injected clone failure"))
            },
        );

        assert!(result.is_err());
        assert!(
            !expected.exists(),
            "partial clone must not remain launchable"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn run_and_brain_scopes_cannot_collide() {
        let base = temp_base("scope_collision");
        let scopes = [("a_b", "c"), ("a", "b_c"), ("a_b", "d"), ("z", "c")];
        let mut leases = Vec::new();
        for (run, brain) in scopes {
            leases.push(prepare_swarm_profile_in(&base, run, brain, false).unwrap());
        }
        let paths: std::collections::HashSet<_> = leases
            .iter()
            .map(|lease| lease.profile_dir().to_path_buf())
            .collect();
        assert_eq!(paths.len(), scopes.len());
        let keys: std::collections::HashSet<_> = leases
            .iter()
            .map(|lease| lease.scope_key().to_string())
            .collect();
        assert_eq!(keys.len(), scopes.len());
        assert!(
            prepare_swarm_profile_in(&base, "a_b", "c", false).is_err(),
            "an active run+brain scope is exclusive"
        );
        drop(leases);
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn release_is_idempotent_and_refuses_foreign_owner() {
        let base = temp_base("safe_release");
        let mut lease = prepare_swarm_profile_in(&base, "run-a", "chatgpt", false).unwrap();
        let path = lease.profile_dir().to_path_buf();
        lease.release().unwrap();
        lease.release().unwrap();
        assert!(!path.exists());

        let mut guarded = prepare_swarm_profile_in(&base, "run-b", "claude", false).unwrap();
        let guarded_path = guarded.profile_dir().to_path_buf();
        write_swarm_owner(
            &guarded_path,
            &SwarmProfileOwner::new("different-run", "different-brain"),
        )
        .unwrap();
        assert!(guarded.release().is_err());
        assert!(
            guarded_path.exists(),
            "foreign-owned profile must be spared"
        );

        // Run-wide compatibility cleanup is marker-scoped too.
        cleanup_swarm_profiles_in(&base, "run-b").unwrap();
        assert!(guarded_path.exists());
        cleanup_swarm_profiles_in(&base, "different-run").unwrap();
        assert!(
            guarded_path.exists(),
            "a foreign marker cannot re-scope a profile for cleanup"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    /// Zwei konkurrierende Prozesse (hier: Threads) auf demselben Scope:
    /// genau einer gewinnt die atomare Reservation, der andere bekommt
    /// `AlreadyExists` — und das Profil des Gewinners bleibt unversehrt.
    #[test]
    fn scope_konkurrenz_ist_fail_closed() {
        let base = temp_base("race");
        let source = reference_profile_dir_in(&base, "chatgpt");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Cookies"), b"login").unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let (base_a, base_b) = (base.clone(), base.clone());
        let (ba, bb) = (barrier.clone(), barrier.clone());
        let a = std::thread::spawn(move || {
            ba.wait();
            prepare_swarm_profile_in(&base_a, "run-a", "chatgpt", false)
        });
        let b = std::thread::spawn(move || {
            bb.wait();
            prepare_swarm_profile_in(&base_b, "run-a", "chatgpt", false)
        });
        let results = [a.join().unwrap(), b.join().unwrap()];
        let winners: Vec<_> = results.iter().filter(|r| r.is_ok()).collect();
        let losers: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
        assert_eq!(winners.len(), 1, "nur ein Prozess darf den Scope halten");
        assert_eq!(losers.len(), 1);
        assert_eq!(
            losers[0].as_ref().unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists,
            "Verlierer muss als 'belegt' scheitern, nie blind ueberschreiben"
        );
        let winner_dir = winners[0].as_ref().unwrap().profile_dir().to_path_buf();
        assert!(winner_dir.join("Cookies").exists(), "Profil unversehrt");
        drop(results);
        let _ = std::fs::remove_dir_all(base);
    }

    /// Windows-Addendum zu [`scope_konkurrenz_ist_fail_closed`]: Haelt ein
    /// Prozess eine Profildatei mit `share=0` (WebView2/SingletonLock-
    /// Semantik), scheitert der zweite öffnende Prozess mit
    /// ERROR_SHARING_VIOLATION (`os error 32`). Diese Fehlerklasse darf
    /// weder ein Fremdprofil zerstören noch als Providerlimit zählen — der
    /// Lease-Schutz antwortet bereits vor dem Öffnen.
    #[cfg(windows)]
    #[test]
    fn os_error_32_ist_profilkonkurrenz_nicht_providerlimit() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{
            CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, GetFileAttributesW, OPEN_EXISTING,
        };

        let base = temp_base("os_error_32");
        let lease = prepare_swarm_profile_in(&base, "run-a", "chatgpt", false).unwrap();
        assert!(is_profile_leased_in(&base, "chatgpt"));

        let marker = lease.profile_dir().join(".webagent-swarm-owner.json");
        let wide: Vec<u16> = marker.as_os_str().encode_wide().chain(Some(0)).collect();
        // Erster Prozess haelt den Marker mit share=0 — genau wie ein
        // WebView2-Prozess sein SingletonLock haelt.
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                0, // dwShareMode: keine Freigaben
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert!(handle != INVALID_HANDLE_VALUE, "erster Oeffner gewinnt");

        // Zweiter Prozess: gleiche Datei oeffnen -> ERROR_SHARING_VIOLATION.
        let second = unsafe {
            CreateFileW(
                wide.as_ptr(),
                GENERIC_READ,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        assert_eq!(
            second, INVALID_HANDLE_VALUE,
            "zweiter Oeffner muss mit Sharing-Violation scheitern"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(32),
            "os error 32 (ERROR_SHARING_VIOLATION) muss es sein"
        );
        unsafe { let _ = CloseHandle(handle); }

        // Das Profil ist deshalb noch da und weiter korrekt vergeben — kein
        // Aufraeumer darf es auf Basis der Fehlerklasse geloescht haben.
        assert!(marker.exists(), "Marker bleibt bestehen");
        assert!(is_profile_leased_in(&base, "chatgpt"));
        assert_eq!(
            unsafe { GetFileAttributesW(wide.as_ptr()) }
                != windows_sys::Win32::Storage::FileSystem::INVALID_FILE_ATTRIBUTES,
            true
        );
        drop(lease);
        let _ = std::fs::remove_dir_all(base);
    }

    /// Abgestuerzter Arbeiter + Wiederaufnahme (T-804): Solange der Heartbeat
    /// frisch ist, bleibt der Scope fail-closed „belegt" (nichts anfassen,
    /// keine Probes). Erst wenn der Heartbeat nach `HEARTBEAT_FRESH_SECS`
    /// abkaltet, darf der naechste Prozess den Scope per Reclaim wieder
    /// aufnehmen. Ein frueher Reclaim schlaegt fehl statt blind zu loeschen.
    #[test]
    fn stale_heartbeat_ermöglicht_wiederaufnahme() {
        let base = temp_base("crash_resume");
        let source = reference_profile_dir_in(&base, "chatgpt");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Cookies"), b"login").unwrap();
        let scope = swarm_profile_dir_in(&base, "run-a", "chatgpt");

        {
            let mut crashed = prepare_swarm_profile_in(&base, "run-a", "chatgpt", false).unwrap();
            assert!(crashed.heartbeat_now().is_ok());
            assert!(is_profile_leased_in(&base, "chatgpt"));

            // Solange der Heartbeat frisch ist, muss ein Reclaim scheitern —
            // der Besitzer koennte noch leben.
            let early = reclaim_swarm_profile_in(
                &base,
                "run-a",
                "chatgpt",
                false,
                Duration::from_millis(30),
            );
            assert!(early.is_err(), "frischer Lease darf nicht wiedergeklaut werden");
            assert!(scope.exists(), "fremdes Profil bleibt unangetastet");
            assert!(is_profile_leased_in(&base, "chatgpt"));

            // Absturz simulieren: Heartbeat abkaltend alt schreiben (wie die
            // Zeit vergeht, ohne dass der Prozess noch heartbeatet).
            let aged = SwarmProfileOwner {
                version: SWARM_OWNER_VERSION,
                run_id: "run-a".to_string(),
                brain_id: "chatgpt".to_string(),
                scope_key: swarm_profile_scope_key("run-a", "chatgpt"),
                pid: crashed.pid(),
                process_started_at: now_secs() - 3600,
                generation: crashed.generation(),
                heartbeat: Some(now_secs() - HEARTBEAT_FRESH_SECS - 1),
            };
            write_swarm_owner(crashed.profile_dir(), &aged).unwrap();
            // Nun ist der Scope frei — ein frischer Worker darf das alte
            // Verzeichnis ersetzen.
            let mut resumed =
                reclaim_swarm_profile_in(&base, "run-a", "chatgpt", false, Duration::from_secs(1))
                    .unwrap();
            assert!(scope.exists());
            assert!(scope.join("Cookies").exists(), "frischer Clone");
            assert!(resumed.release().is_ok());
        }
        assert!(!scope.exists());
        let _ = std::fs::remove_dir_all(base);
    }

    /// Ein Navigations-Timeout waehnt sich nicht vor dem Lease: der Besitzer
    /// kann weiterarbeiten, bis die ganze Run fuehrt den Scope frei.
    #[test]
    fn navigationstimeout_laesst_lease_intakt() {
        let base = temp_base("nav_timeout");
        let source = reference_profile_dir_in(&base, "chatgpt");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("Cookies"), b"login").unwrap();
        let mut lease = prepare_swarm_profile_in(&base, "run-a", "chatgpt", false).unwrap();
        assert!(is_profile_leased_in(&base, "chatgpt"));

        // Simulierter Timeout der Navigations-Routine: Statusmeldungen,
        // erneuter Heartbeat — das Profil bleibt unangetastet.
        assert!(lease.heartbeat_now().is_ok());
        assert!(is_profile_leased_in(&base, "chatgpt"));
        let path = lease.profile_dir().to_path_buf();
        assert!(path.join("Cookies").exists());
        // Und der Verlierer-Prozess wartet kontrolliert, statt zu loeschen.
        assert!(wait_for_profile_free_in(&base, "chatgpt", Duration::from_millis(20)).is_err());
        let _ = lease.release();
        assert!(!is_profile_leased_in(&base, "chatgpt"));
        let _ = std::fs::remove_dir_all(base);
    }
}
