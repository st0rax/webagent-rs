use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::brains::{
    reference_profile_dir_in, swarm_profile_dir_in, swarm_profile_scope_key,
    use_sparse_profile_copy, FULL_COPY_SKIP_DIRS, SPARSE_COPY_WHITELIST, SPARSE_SKIP_DIRS,
};
use super::paths::*;

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
const SWARM_OWNER_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct SwarmProfileOwner {
    version: u32,
    run_id: String,
    brain_id: String,
    scope_key: String,
}

impl SwarmProfileOwner {
    fn new(run_id: &str, brain_id: &str) -> Self {
        Self {
            version: SWARM_OWNER_VERSION,
            run_id: run_id.to_string(),
            brain_id: brain_id.to_string(),
            scope_key: swarm_profile_scope_key(run_id, brain_id),
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

    pub fn release(&mut self) -> std::io::Result<()> {
        if self.released {
            return Ok(());
        }
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
}
