use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;
use std::sync::OnceLock;

use super::paths::{profiles_dir, shared_profile_dir};
use super::profiles::{copy_dir_sparse, copy_dir_sparse_strict, restore_sparse_backup};
use super::selectors::encapsulated_profile_dir;

/// Laufzeit-Kopie des Master-Profils, einmal pro Prozess (`OnceLock`).
///
/// Das Master (`profiles/shared`) ist das **read-only Hauptprofil** mit allen
/// eingeloggten Brains. Der Betrieb öffnet es NIE direkt: Beim ersten Zugriff
/// wird eine sparsame Kopie der Login-Artefakte (Cookies, Local State,
/// Preferences, … — siehe [`SPARSE_COPY_WHITELIST`]) nach
/// `profiles/encapsulated/pool_<stamp>` gezogen und ausschließlich dort
/// gearbeitet. Caches erzeugt WebView2 in der Kopie frisch.
///
/// Damit überlebt ein Neu-Login dauerhaft: Selbst wenn eine Laufzeit-Kopie
/// aufgeräumt wird (siehe [`sweep_stale_runtime_profiles`]), klont der nächste
/// Start erneut aus dem unangetasteten Master.
static RUNTIME_POOL_PROFILE: OnceLock<PathBuf> = OnceLock::new();

pub fn runtime_pool_profile_dir() -> Result<PathBuf, String> {
    if let Some(p) = RUNTIME_POOL_PROFILE.get() {
        return Ok(p.clone());
    }
    let master = shared_profile_dir();
    let _lock = prepare_master_for_runtime_clone(&master)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let dst = encapsulated_profile_dir("pool", &stamp);
    copy_dir_sparse(&master, &dst).map_err(|error| {
        format!(
            "[master-profile] Laufzeit-Kopie von {:?} fehlgeschlagen: {error}",
            master
        )
    })?;
    if has_login_artifacts(&dst) {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] Laufzeit-Kopie des Hauptprofils → {:?}",
            dst
        ));
    } else {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] WARN: Laufzeit-Kopie von {:?} ohne Login-Artefakte \
             (Master offen/gesperrt? Kopie nach {:?})",
            master, dst
        ));
    }
    // Master gegen die kanonischen Profile pruefen: kennt das Master einen
    // Brain nicht, den profiles/<brain> eingeloggt traegt, wird der Pool fuer
    // genau diesen Brain "Login nötig" melden - obwohl die Session gueltig ist
    // (gemessen 08.08.2026: kimi und chatgpt). Laut warnen statt still sperren.
    let missing = master_missing_sessions_from_canonical();
    if !missing.is_empty() {
        crate::bench_events::eprint_line(&format!(
            "[master-profile] WARN: Master kennt {} nicht, obwohl die kanonischen \
             Profile eingeloggt sind. Der Pool wuerde Login nötig melden trotz \
             gueltiger Session. Abhilfe: login-all (spiegelt die Sitzungen ins Master).",
            missing.join(", ")
        ));
    }
    let _ = RUNTIME_POOL_PROFILE.set(dst.clone());
    Ok(RUNTIME_POOL_PROFILE.get().cloned().unwrap_or(dst))
}

/// Spielt die aufgefrischte Sitzung aus der Laufzeit-Kopie ins Master zurueck.
///
/// # Warum es das braucht
///
/// Das Master ist read-only und der Betrieb laeuft auf einem Klon. Der Browser
/// erneuert Sitzungen aber IN DIE KOPIE — und ohne Rueckweg blieb das Master
/// stehen. Gemessen am 05.08.2026: letzte Schreiboperation im Master
/// 03.08. 18:07:44, in der Laufzeit-Kopie laufend.
///
/// Das ist nicht bloss veraltet, es ist toedlich: die Anbieter rotieren
/// Refresh-Tokens. Sobald der Browser eine Sitzung erneuert, entwertet der
/// Anbieter den alten Token serverseitig. Die Kopie haelt den neuen, das Master
/// den bereits entwerteten. Nach ein bis zwei Runden ist das Master garantiert
/// abgemeldet — und eine frische Anmeldung haelt nur bis zur naechsten
/// Rotation. Genau so wurden 6 von 8 Brains abgemeldet.
///
/// # Die Schutzbedingung
///
/// Zurueckgeschrieben wird NUR, wenn die Kopie selbst Login-Artefakte hat. Ein
/// Lauf, der frueh scheitert oder mit leerem Profil startet, darf das Master
/// nicht ueberschreiben — sonst zerstoert genau dieser Rueckweg die
/// Anmeldung, die er schuetzen soll.
pub fn write_back_session_to_master() -> Result<(), String> {
    let Some(runtime) = RUNTIME_POOL_PROFILE.get() else {
        // Nie geklont, also nichts zurueckzuschreiben.
        return Ok(());
    };
    if !runtime.is_dir() {
        return Ok(());
    }
    write_back_dir_to_master(runtime)
}

/// Spielt eine Laufzeit-Kopie ins Master-Profil zurueck — der gemeinsame Kern
/// von [`write_back_session_to_master`] und dem CLI-Rettungsweg
/// `webagent sync-master`.
///
/// Das Master ist read-only und der Betrieb laeuft auf einem Klon. Der Browser
/// erneuert Sitzungen aber IN DIE KOPIE — und ohne Rueckweg blieb das Master
/// stehen. Gemessen am 05.08.2026: letzte Schreiboperation im Master
/// 03.08. 18:07:44, in der Laufzeit-Kopie laufend.
///
/// # Die Schutzbedingung
///
/// Zurueckgeschrieben wird NUR, wenn die Kopie selbst Login-Artefakte hat. Ein
/// Lauf, der frueh scheitert oder mit leerem Profil startet, darf das Master
/// nicht ueberschreiben — sonst zerstoert genau dieser Rueckweg die
/// Anmeldung, die er schuetzen soll.
/// Writes a verified runtime session back to the shared master profile.
///
/// The runtime source is validated before a backup name is reserved. Invalid or
/// empty clones therefore cannot leave misleading empty backup directories.
pub fn write_back_dir_to_master(dir: &Path) -> Result<(), String> {
    let master = shared_profile_dir();
    let _lock = acquire_write_back_lock(&master)?;
    recover_pending_write_back_transaction(&master)?;
    validate_write_back_source(dir)?;
    let backup = reserve_unique_backup_dir(&master)?;
    write_back_dir_to_master_locked(dir, &master, &backup)
}

/// Testable write-back entry point with explicit paths. It takes the same
/// fail-closed lock as the production path.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
pub(crate) fn prepare_shared_profile_for_clone() -> Result<WriteBackLock, String> {
    let master = shared_profile_dir();
    prepare_master_for_runtime_clone(&master)
}

pub(crate) fn prepare_master_for_runtime_clone(master: &Path) -> Result<WriteBackLock, String> {
    let lock = acquire_write_back_lock(master)?;
    recover_pending_write_back_transaction(master)?;
    Ok(lock)
}
#[allow(dead_code)]
pub(crate) fn write_back_dir_to_master_at(
    dir: &Path,
    master: &Path,
    backup: &Path,
) -> Result<(), String> {
    let _lock = acquire_write_back_lock(master)?;
    recover_pending_write_back_transaction(master)?;
    write_back_dir_to_master_locked(dir, master, backup)
}

fn validate_write_back_source(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(format!("{:?} ist kein Verzeichnis", dir));
    }
    if !has_login_artifacts(dir) {
        return Err(format!(
            "Laufzeit-Kopie {:?} hat keine Login-Artefakte - nicht zurueckschreiben",
            dir
        ));
    }
    Ok(())
}

fn write_back_dir_to_master_locked(dir: &Path, master: &Path, backup: &Path) -> Result<(), String> {
    validate_write_back_source(dir)?;
    let source_weight = login_artifact_weight(dir);
    let target_weight = login_artifact_weight(master);
    if !write_back_is_safe(source_weight, target_weight) {
        let msg = format!(
            "[master-profile] Rueckschreiben ABGELEHNT: Laufzeit-Kopie traegt {} KB \
             Sitzungsdaten, das Hauptprofil {} KB. Ueberschreiben wuerde Anmeldungen \
             vernichten. Kopie bleibt unter {:?}",
            source_weight / 1024,
            target_weight / 1024,
            dir
        );
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
        return Err(msg);
    }

    let lost = runtime_lost_sessions(&cookies_db_bytes(master), &cookies_db_bytes(dir));
    if !lost.is_empty() {
        let msg = format!(
            "[master-profile] Rueckschreiben ABGELEHNT: Laufzeit-Kopie hat die Sitzung \
             verloren, die das Hauptprofil noch traegt: {}. Ueberschreiben wuerde \
             diese Anmeldung vernichten. Kopie bleibt unter {:?}",
            lost.join(", "),
            dir
        );
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
        return Err(msg);
    }

    // Even a zero-weight master can contain sparse artifacts. Snapshot it before
    // every mutation so failure always has a restore source.
    if !master.exists() {
        std::fs::create_dir_all(master)
            .map_err(|error| format!("[master-profile] master create failed: {error}"))?;
    }
    copy_dir_sparse_strict(master, backup).map_err(|error| {
        let msg = format!(
            "[master-profile] Sicherung vor dem Rueckschreiben fehlgeschlagen; Master bleibt unveraendert: {error}"
        );
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
        msg
    })?;

    set_profile_readonly_strict(backup, false)?;
    sync_profile_tree_and_parent(backup)?;
    begin_write_back_transaction(master, backup)?;
    if let Err(unseal_error) = set_profile_readonly_strict(master, false) {
        // A strict unseal can fail after changing a prefix of files. Always
        // attempt to reseal the complete tree before returning that failure.
        let detail = match set_profile_readonly_strict(master, true) {
            Ok(()) => {
                format!("Master vor Write-back nicht vollstaendig entsiegelbar: {unseal_error}")
            }
            Err(reseal_error) => format!(
                "Master vor Write-back nicht vollstaendig entsiegelbar: {unseal_error}; Reseal ebenfalls fehlgeschlagen: {reseal_error}"
            ),
        };
        crate::bench_events::emit(crate::bench_events::Level::Warn, None, &detail);
        return Err(detail);
    }
    let copy_result = copy_dir_sparse_strict(dir, master).map_err(|error| error.to_string());
    // Copy already mutated an unsealed master. Any durability failure here must
    // roll back and reseal; `?` would leave a partial unsealed tree.
    let sync_result = sync_profile_tree_and_parent(master);
    let reseal_result = set_profile_readonly_strict(master, true);
    if let Some(error) = copy_result.err().or_else(|| sync_result.err()) {
        let detail = match reseal_result {
            Ok(()) => error,
            Err(reseal_error) => format!("{error}; reseal failed: {reseal_error}"),
        };
        return fail_and_restore_master(master, backup, detail);
    }
    if let Err(reseal_error) = reseal_result {
        return fail_and_restore_master(master, backup, format!("reseal failed: {reseal_error}"));
    }
    let post_weight = login_artifact_weight(master);
    let missing_after = runtime_lost_sessions(&cookies_db_bytes(dir), &cookies_db_bytes(master));
    if !has_login_artifacts(master)
        || !write_back_is_safe(post_weight, source_weight)
        || !missing_after.is_empty()
    {
        return fail_and_restore_master(
            master,
            backup,
            format!(
                "Post-Verify fehlgeschlagen ({} KB statt mindestens {} KB, fehlende Nachweise: {})",
                post_weight / 1024,
                (source_weight as f64 * WRITE_BACK_MIN_RATIO / 1024.0).ceil() as u64,
                if missing_after.is_empty() {
                    "keine".to_string()
                } else {
                    missing_after.join(", ")
                }
            ),
        );
    }

    crate::bench_events::emit(
        crate::bench_events::Level::Info,
        None,
        "[master-profile] aufgefrischte Sitzung ins Hauptprofil zurueckgeschrieben",
    );
    commit_write_back_transaction(master, backup)?;
    clear_write_back_transaction(master)?;
    Ok(())
}

fn restore_master_durably(master: &Path, backup: &Path) -> Result<(), String> {
    restore_sparse_backup(backup, master).map_err(|error| error.to_string())?;
    // Content must be durable before reseal or journal clear; file fsync after
    // reseal can fail on Windows because files are read-only.
    sync_profile_tree_and_parent(master)
}

fn fail_and_restore_master(master: &Path, backup: &Path, error: String) -> Result<(), String> {
    let unseal = set_profile_readonly_strict(master, false);
    let restore = match unseal {
        Ok(()) => restore_master_durably(master, backup),
        Err(unseal_error) => Err(format!("unseal before restore failed: {unseal_error}")),
    };
    let reseal = set_profile_readonly_strict(master, true);
    match (restore, reseal) {
        (Ok(()), Ok(())) => {
            let msg = format!("[master-profile] Rueckschreiben verworfen ({error}); Master aus Backup wiederhergestellt");
            crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
            clear_write_back_transaction(master)?;
            Err(msg)
        }
        (Err(restore_error), Ok(())) => {
            let msg = format!("[master-profile] Rueckschreiben fehlgeschlagen ({error}); Backup-Restore ebenfalls fehlgeschlagen: {restore_error}");
            crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
            Err(msg)
        }
        (Ok(()), Err(reseal_error)) => {
            let msg = format!("[master-profile] Rueckschreiben fehlgeschlagen ({error}); Restore gelang, Master konnte aber nicht versiegelt werden: {reseal_error}");
            crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
            Err(msg)
        }
        (Err(restore_error), Err(reseal_error)) => {
            let msg = format!("[master-profile] Rueckschreiben fehlgeschlagen ({error}); Restore: {restore_error}; Versiegelung: {reseal_error}");
            crate::bench_events::emit(crate::bench_events::Level::Warn, None, &msg);
            Err(msg)
        }
    }
}
/// OS-managed cross-process lock for the irreversible shared-profile mutation.
/// The advisory file lock is released by the operating system on process exit,
/// so no stale-directory heuristic can steal a fresh writer or wedge recovery.
pub(crate) struct WriteBackLock {
    file: File,
}

impl Drop for WriteBackLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn acquire_write_back_lock(master: &Path) -> Result<WriteBackLock, String> {
    let path = master.with_file_name("shared.session-writeback.lock");
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            format!(
                "[master-profile] Write-back-Sperre {:?} nicht oeffenbar: {error}",
                path
            )
        })?;
    file.try_lock_exclusive().map_err(|error| {
        format!(
            "[master-profile] Rueckschreiben gesperrt unter {:?}: {error}",
            path
        )
    })?;
    Ok(WriteBackLock { file })
}

fn write_back_journal_pending_path(master: &Path) -> PathBuf {
    master.with_file_name("shared.session-writeback.journal.pending")
}

fn write_back_journal_committed_path(master: &Path) -> PathBuf {
    master.with_file_name("shared.session-writeback.journal.committed")
}

fn sync_profile_tree(dir: &Path) -> Result<(), String> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            sync_profile_tree(&path)?;
        } else {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .and_then(|file| file.sync_all())
                .map_err(|error| {
                    format!(
                        "[master-profile] Datei {:?} nicht synchronisierbar: {error}",
                        path
                    )
                })?;
        }
    }
    // Directory entries (creates, replaces, deletes) are only durable after the
    // directory itself is fsynced. Recurse first, then fsync this directory.
    sync_parent_dir(dir)
}

fn sync_profile_tree_and_parent(dir: &Path) -> Result<(), String> {
    sync_profile_tree(dir)?;
    match dir.parent() {
        Some(parent) => sync_parent_dir(parent),
        None => Ok(()),
    }
}

#[cfg(test)]
mod sync_test_hooks {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    #[derive(Default)]
    struct State {
        synced: Vec<PathBuf>,
        fail: Option<(PathBuf, i8)>,
        fail_after_matches: usize,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            reset();
        }
    }

    pub fn install() -> Guard {
        reset();
        Guard
    }

    fn reset() {
        STATE.with(|state| *state.borrow_mut() = State::default());
    }

    pub fn fail_once(path: PathBuf) {
        STATE.with(|state| state.borrow_mut().fail = Some((path, 1)));
    }

    pub fn fail_always(path: PathBuf) {
        STATE.with(|state| state.borrow_mut().fail = Some((path, -1)));
    }

    pub fn fail_on_nth(path: PathBuf, call: usize) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.fail = Some((path, 1));
            state.fail_after_matches = call.saturating_sub(1);
        });
    }

    pub fn synced() -> Vec<PathBuf> {
        STATE.with(|state| state.borrow().synced.clone())
    }

    pub fn observe(path: &Path) -> Result<(), String> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.synced.push(path.to_path_buf());
            let matches = state
                .fail
                .as_ref()
                .is_some_and(|(fail_path, remaining)| fail_path == path && *remaining != 0);
            if matches && state.fail_after_matches > 0 {
                state.fail_after_matches -= 1;
                return Ok(());
            }
            let hit = matches;
            if !hit {
                return Ok(());
            }
            if let Some((_, remaining)) = state.fail.as_mut() {
                if *remaining > 0 {
                    *remaining -= 1;
                }
            }
            if state
                .fail
                .as_ref()
                .is_some_and(|(_, remaining)| *remaining == 0)
            {
                state.fail = None;
            }
            Err(format!(
                "[master-profile] Dateibaum {:?} nicht synchronisierbar: injected",
                path
            ))
        })
    }
}

fn sync_parent_dir(path: &Path) -> Result<(), String> {
    #[cfg(test)]
    sync_test_hooks::observe(path)?;
    sync_directory_durably(path)
}

#[cfg(windows)]
fn sync_directory_durably(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is NUL-terminated and lives through the Win32 calls; the
    // acquired directory handle is closed on every path below.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!(
            "[master-profile] Journal-Verzeichnis {:?} nicht oeffenbar: {}",
            path,
            std::io::Error::last_os_error()
        ));
    }
    let flushed = unsafe { FlushFileBuffers(handle) } != 0;
    let close = unsafe { CloseHandle(handle) } != 0;
    if !flushed {
        return Err(format!(
            "[master-profile] Journal-Verzeichnis {:?} nicht synchronisierbar: {}",
            path,
            std::io::Error::last_os_error()
        ));
    }
    if !close {
        return Err(format!(
            "[master-profile] Journal-Verzeichnis {:?} nicht schliessbar: {}",
            path,
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory_durably(path: &Path) -> Result<(), String> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            format!(
                "[master-profile] Journal-Verzeichnis {:?} nicht synchronisierbar: {error}",
                path
            )
        })
}

fn durable_publish_journal(path: &Path, backup: &Path) -> Result<(), String> {
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let payload = format!("{}\n", backup.to_string_lossy());
    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        use std::io::Write;
        file.write_all(payload.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "[master-profile] Journal {:?} nicht dauerhaft publizierbar: {error}",
            path
        ));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            format!(
                "[master-profile] Journal {:?} nicht synchronisierbar: {error}",
                path
            )
        })?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("[master-profile] Journal {:?} ohne Elternverzeichnis", path))?;
    sync_parent_dir(parent)?;
    Ok(())
}
fn begin_write_back_transaction(master: &Path, backup: &Path) -> Result<(), String> {
    durable_publish_journal(&write_back_journal_pending_path(master), backup)
}

fn commit_write_back_transaction(master: &Path, backup: &Path) -> Result<(), String> {
    durable_publish_journal(&write_back_journal_committed_path(master), backup)
}

fn remove_write_back_journal_durably(path: &Path, parent: &Path) -> Result<(), String> {
    let backup = match std::fs::read_to_string(path) {
        Ok(text) => Some(PathBuf::from(text.lines().next().unwrap_or_default())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "[master-profile] Write-back-Journal {:?} vor Cleanup nicht lesbar: {error}",
                path
            ))
        }
    };
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "[master-profile] Write-back-Journal {:?} nicht entfernbar: {error}",
                path
            ))
        }
    }
    if let Err(sync_error) = sync_parent_dir(parent) {
        let Some(backup) = backup else {
            return Err(sync_error);
        };
        return match durable_publish_journal(path, &backup) {
            Ok(()) => Err(format!(
                "{sync_error}; Recovery-Journal {:?} wurde wiederhergestellt",
                path
            )),
            Err(restore_error) => Err(format!(
                "{sync_error}; Recovery-Journal {:?} konnte nicht wiederhergestellt werden: {restore_error}",
                path
            )),
        };
    }
    Ok(())
}

fn clear_write_back_transaction(master: &Path) -> Result<(), String> {
    let parent = master.parent().ok_or_else(|| {
        format!(
            "[master-profile] Master {:?} ohne Elternverzeichnis",
            master
        )
    })?;
    let pending = write_back_journal_pending_path(master);
    remove_write_back_journal_durably(&pending, parent)?;

    let committed = write_back_journal_committed_path(master);
    remove_write_back_journal_durably(&committed, parent)?;
    Ok(())
}

fn recover_pending_write_back_transaction(master: &Path) -> Result<(), String> {
    let pending = write_back_journal_pending_path(master);
    let committed = write_back_journal_committed_path(master);
    // A separately synchronized commit marker is decisive: the verified new
    // master wins, even if cleanup was interrupted after the commit.
    if committed.exists() {
        return clear_write_back_transaction(master);
    }
    let backup = match std::fs::read_to_string(&pending) {
        Ok(text) => PathBuf::from(text.lines().next().unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "[master-profile] Write-back-Journal {:?} nicht lesbar: {error}",
                pending
            ))
        }
    };
    if !backup.is_dir() {
        return Err(format!(
            "[master-profile] Journal-Backup {:?} fehlt",
            backup
        ));
    }
    if let Err(unseal) = set_profile_readonly_strict(master, false) {
        // Strict unsealing can fail after making a prefix of the tree writable.
        // Fail closed by attempting to reseal the complete tree before returning.
        return match set_profile_readonly_strict(master, true) {
            Ok(()) => Err(format!(
                "[master-profile] Crash-Recovery konnte Master nicht entsiegeln: {unseal}"
            )),
            Err(reseal) => Err(format!(
                "[master-profile] Crash-Recovery konnte Master nicht entsiegeln: {unseal}; Reseal ebenfalls fehlgeschlagen: {reseal}"
            )),
        };
    }
    let restore = restore_master_durably(master, &backup);
    let reseal = set_profile_readonly_strict(master, true);
    match (restore, reseal) {
        (Ok(()), Ok(())) => clear_write_back_transaction(master),
        (Err(restore), Ok(())) => Err(format!(
            "[master-profile] Crash-Recovery fehlgeschlagen: {restore}"
        )),
        (Ok(()), Err(reseal)) => Err(format!(
            "[master-profile] Crash-Recovery versiegelt Master nicht: {reseal}"
        )),
        (Err(restore), Err(reseal)) => Err(format!(
            "[master-profile] Crash-Recovery fehlgeschlagen: {restore}; Reseal: {reseal}"
        )),
    }
}

/// Reserviert ein prozess- und zeitlich eindeutiges Backup-Verzeichnis, bevor
/// der Master-Write-back beginnt. `create_dir` ist die Kollisionssicherung: ein
/// vorhandenes Backup wird niemals Ã¼berlagert oder wiederverwendet.
pub(crate) fn reserve_unique_backup_dir(master: &Path) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    for attempt in 0..128u32 {
        let candidate =
            master.with_file_name(format!("shared.session-bak-{nanos}-{pid}-{attempt}"));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "[master-profile] Sicherungsordner {:?} nicht reservierbar: {error}",
                    candidate
                ));
            }
        }
    }
    Err("[master-profile] kein eindeutiger Sicherungsordner nach 128 Versuchen".to_string())
}
/// Gesamtgroesse aller Login-Dateien in `dir` — ein grobes Mass dafuer, wie
/// viele Sitzungen ein Profil traegt.
///
/// Kein exakter Zaehler (dafuer muesste man die SQLite-Datenbank oeffnen), aber
/// ein belastbares Warnsignal: acht angemeldete Dienste hinterlassen deutlich
/// mehr als zwei. Gemessen am 07.08.2026 fiel `Cookies` im Hauptprofil von
/// 108 KB auf 40 KB, nachdem eine aermere Laufzeit-Kopie darueber geschrieben
/// wurde.
fn login_artifact_weight(dir: &Path) -> u64 {
    const NEEDED: &[&str] = &["Cookies", "Local State", "Login Data"];
    let mut sum = 0u64;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else { continue };
            if ty.is_dir() {
                stack.push(e.path());
            } else {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if NEEDED.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    sum += e.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
        }
    }
    sum
}

/// Ab welchem Anteil des Ziels eine Quelle noch als „nicht aermer" durchgeht.
///
/// Nicht 100 %: eine SQLite-Datenbank schrumpft auch beim Aufraeumen, und ein
/// Schutz, der bei jeder Schwankung anschlaegt, wird abgeschaltet. 60 % laesst
/// normales Atmen zu und faengt den Fall vom 07.08.2026 (Abfall auf 37 %).
const WRITE_BACK_MIN_RATIO: f64 = 0.6;

/// Darf `source` das `target` ueberschreiben, ohne Sitzungen zu vernichten?
///
/// Reine Funktion auf den beiden Gewichten, damit die Entscheidung ohne
/// Dateisystem pruefbar ist.
pub(crate) fn write_back_is_safe(source: u64, target: u64) -> bool {
    if source == 0 {
        return false; // nichts drin — niemals
    }
    if target == 0 {
        return true; // Ziel leer, jede Quelle ist eine Verbesserung
    }
    source as f64 >= target as f64 * WRITE_BACK_MIN_RATIO
}

/// Liegt in `dir` (rekursiv) mindestens eine der Login-Dateien, deren Fehlen
/// eine ausgeloggte Kopie bedeutet?
pub(crate) fn has_login_artifacts(dir: &Path) -> bool {
    const NEEDED: &[&str] = &["Cookies", "Local State", "Login Data"];
    let mut found = false;
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else {
                continue;
            };
            if ty.is_dir() {
                stack.push(e.path());
            } else {
                let name = e.file_name().to_string_lossy().to_lowercase();
                if NEEDED.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    found = true;
                    break;
                }
            }
        }
        if found {
            break;
        }
    }
    found
}

/// Sitzungs-Nachweis je Brain: ein markanter Cookie-Name, der NUR in einem
/// eingeloggten Profil auftaucht. Gemessen am 08.08.2026 an den echten
/// Cookie-Datenbanken: kimi traegt `kimi-auth`, chatgpt
/// `__Secure-next-auth.session-token` (Präfix, Suffix .0/.1), mistral
/// `ory_session` (variables Suffix). Das Master trug diese Cookies am 07.08.
/// noch; nach einem Rueckschreiben aus einer Laufzeit-Kopie, die die Brains
/// als ausgeloggt erlebt hatte, fehlten kimi und chatgpt — und der Pool
/// meldete "Login nötig", obwohl `profiles/<brain>` die gueltige Sitzung trug.
///
/// Bewusst keine Vollstaendigkeit: ein Brain ohne bekannten, eindeutigen
/// Sitzungs-Cookie faellt einfach unter den Gewichts-Schutz.
const SESSION_PROOF_COOKIES: &[(&str, &[&str])] = &[
    ("kimi", &["kimi-auth"]),
    ("chatgpt", &["__Secure-next-auth.session-token"]),
    ("mistral", &["ory_session"]),
];

/// Enthaelt der Byte-Haufen `hay` den ASCII-Text `needle`?
///
/// Chromium speichert Cookie-Namen als Klartext-Spalten in der SQLite-Datei;
/// ein Rohbyte-Scan genuegt als Sitzungs-Nachweis, ohne SQLite zu parsen oder
/// verschluesselte Werte zu lesen.
pub(crate) fn bytes_contain(hay: &[u8], needle: &str) -> bool {
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > hay.len() {
        return n.is_empty();
    }
    hay.windows(n.len()).any(|w| w == n)
}

/// Pfad zur Cookies-Datenbank unter `dir` (rekursiv, genau der Name "Cookies").
pub(crate) fn cookies_db_path(dir: &Path) -> Option<PathBuf> {
    let mut stack: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(cur) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&cur) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(ty) = e.file_type() else {
                continue;
            };
            let p = e.path();
            if ty.is_dir() {
                stack.push(p);
            } else if e
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case("Cookies")
            {
                return Some(p);
            }
        }
    }
    None
}

/// Rohbytes der Cookies-Datenbank unter `dir` (leer, wenn keine da ist).
pub(crate) fn cookies_db_bytes(dir: &Path) -> Vec<u8> {
    cookies_db_path(dir)
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default()
}

/// Welche Brains hat die Laufzeit-Kopie ausgeloggt, die das Master noch kennt?
///
/// Kern der Schutzwache beim Rueckschreiben: nur der Fall "Nachweis im Master
/// da, in der Kopie weg" vernichtet eine gueltige Anmeldung. Das Gewichts-Mass
/// aus ed802aa sieht genau das nicht — am 08.08.2026 fehlten einem ~90-KB-
/// Cookie-Vorrat ein paar hundert Bytes (kimi-auth) und die Schranke schwieg.
/// Reine Funktion auf den Rohbytes, damit ohne Dateisystem pruefbar.
pub(crate) fn runtime_lost_sessions(
    master_cookies: &[u8],
    runtime_cookies: &[u8],
) -> Vec<&'static str> {
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            let master_has = needles.iter().any(|n| bytes_contain(master_cookies, n));
            let runtime_has = needles.iter().any(|n| bytes_contain(runtime_cookies, n));
            (master_has && !runtime_has).then_some(*brain)
        })
        .collect()
}

/// Welche Brains traegt das kanonische `canonical_cookies`-Profil eingeloggt,
/// das Master aber nicht?
///
/// Gemessen 08.08.2026: kimi und chatgpt fehlten im Master, obwohl
/// `profiles/kimi` und `profiles/chatgpt` gueltige Sitzungen trugen. Der Pool
/// klont ausschliesslich aus dem Master — daher "Login nötig" trotz gueltiger
/// Session. Reine Funktion auf den Rohbytes.
// Reiner Kern des Vergleichs. Die Produktivfunktion
// `master_missing_sessions_from_canonical` liest pro Brain ein eigenes
// kanonisches Profil und kann diesen Kern deshalb nicht direkt nutzen;
// hier bleibt er als pruefbare Fassung der Regel.
#[cfg(test)]
pub(crate) fn master_missing_sessions(
    canonical_cookies: &[u8],
    master_cookies: &[u8],
) -> Vec<&'static str> {
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            let canonical_has = needles.iter().any(|n| bytes_contain(canonical_cookies, n));
            let master_has = needles.iter().any(|n| bytes_contain(master_cookies, n));
            (canonical_has && !master_has).then_some(*brain)
        })
        .collect()
}

/// Kanonische Profile gegen das Master vergleichen: welche Brains kennt das
/// Master nicht, obwohl `profiles/<brain>` eingeloggt ist? Laute Warnung beim
/// Pool-Klon, damit die Luecke sichtbar wird, statt still in "Login nötig" zu
/// muenden.
fn master_missing_sessions_from_canonical() -> Vec<&'static str> {
    let master_cookies = cookies_db_bytes(&shared_profile_dir());
    SESSION_PROOF_COOKIES
        .iter()
        .filter_map(|(brain, needles)| {
            // Nur den eigenen Sitzungs-Nachweis pruefen: ein kanonisches Profil
            // kann auch fremde Cookies tragen (Sitzung aus einer gemeinsamen
            // Login-Runde), das darf das Nachbar-Brain nicht als fehlend melden.
            let canonical = cookies_db_bytes(&profiles_dir().join(brain));
            let canonical_has = needles.iter().any(|n| bytes_contain(&canonical, n));
            let master_has = needles.iter().any(|n| bytes_contain(&master_cookies, n));
            (canonical_has && !master_has).then_some(*brain)
        })
        .collect()
}

/// Versiegelt das Master-Profil: alle Dateien unter `profiles/shared` werden
/// read-only. NUR der Login-Mechanismus (unseal → schreiben → seal) macht das
/// Profil jemals wieder beschreibbar; der Betrieb nutzt ausschließlich die
/// Laufzeit-Kopie ([`runtime_pool_profile_dir`]).
pub fn seal_master_profile() {
    set_master_readonly(true);
}

/// Macht das Master-Profil wieder beschreibbar — NUR vom Login-Mechanismus
/// aufgerufen, bevor ein Browser das Master öffnet.
pub fn unseal_master_profile() {
    set_master_readonly(false);
}

fn set_profile_readonly_strict(root: &Path, readonly: bool) -> Result<(), String> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(format!("profile path {:?} is not a directory", root));
    }
    let mut files = Vec::new();
    collect_files_strict(root, &mut files).map_err(|error| error.to_string())?;
    for file in files {
        #[cfg(test)]
        readonly_test_hooks::observe(readonly)?;
        let metadata = std::fs::metadata(&file).map_err(|error| error.to_string())?;
        let mut permissions = metadata.permissions();
        permissions.set_readonly(readonly);
        std::fs::set_permissions(&file, permissions)
            .map_err(|error| format!("permissions for {:?} not settable: {error}", file))?;
    }
    Ok(())
}

#[cfg(test)]
mod readonly_test_hooks {
    use std::cell::RefCell;

    #[derive(Default)]
    struct State {
        fail_unseal_after: Option<usize>,
        fail_reseal: bool,
        reseal_attempted: bool,
    }

    thread_local! {
        static STATE: RefCell<State> = RefCell::new(State::default());
    }

    pub struct Guard;

    impl Drop for Guard {
        fn drop(&mut self) {
            reset();
        }
    }

    pub fn install() -> Guard {
        reset();
        Guard
    }

    fn reset() {
        STATE.with(|state| *state.borrow_mut() = State::default());
    }

    pub fn fail_unseal_after(successful_files: usize, fail_reseal: bool) {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.fail_unseal_after = Some(successful_files);
            state.fail_reseal = fail_reseal;
        });
    }

    pub fn reseal_attempted() -> bool {
        STATE.with(|state| state.borrow().reseal_attempted)
    }

    pub fn observe(readonly: bool) -> Result<(), String> {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            if readonly {
                state.reseal_attempted = true;
                if state.fail_reseal {
                    state.fail_reseal = false;
                    return Err("injected reseal permission failure".to_string());
                }
                return Ok(());
            }

            let Some(remaining) = state.fail_unseal_after.as_mut() else {
                return Ok(());
            };
            if *remaining > 0 {
                *remaining -= 1;
                return Ok(());
            }
            state.fail_unseal_after = None;
            Err("injected unseal permission failure".to_string())
        })
    }
}

fn collect_files_strict(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files_strict(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}
fn set_master_readonly(readonly: bool) {
    set_profile_readonly(&shared_profile_dir(), readonly);
}

fn set_profile_readonly(root: &Path, readonly: bool) {
    if !root.is_dir() {
        return;
    }
    let mut files = Vec::new();
    collect_files(root, &mut files);
    let mut ok = 0usize;
    let mut failed = 0usize;
    for file in &files {
        if let Ok(metadata) = std::fs::metadata(file) {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(readonly);
            if std::fs::set_permissions(file, permissions).is_ok() {
                ok += 1;
            } else {
                failed += 1;
            }
        }
    }
    crate::bench_events::eprint_line(&format!(
        "[master-profile] Profil {:?} {}: {ok} Dateien ({failed} Fehler)",
        root,
        if readonly {
            "versiegelt (read-only)"
        } else {
            "entsiegelt (beschreibbar)"
        }
    ));
}
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(ty) = e.file_type() else {
            continue;
        };
        if ty.is_dir() {
            collect_files(&e.path(), out);
        } else {
            out.push(e.path());
        }
    }
}

#[cfg(test)]
mod writeback_durability_tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_base(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("webagent_{tag}_{stamp}"))
    }

    #[test]
    fn writeback_sync_profile_tree_fsyncs_nested_directories() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_nested_sync");
        let root = base.join("profile");
        let nested = root.join("Default").join("Network");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Cookies"), b"kimi-auth").unwrap();

        sync_profile_tree(&root).unwrap();
        let synced = sync_test_hooks::synced();
        assert!(
            synced.iter().any(|path| path == &nested),
            "nested dir with changed entries must be fsynced: {synced:?}"
        );
        assert!(
            synced.iter().any(|path| path == &root.join("Default")),
            "intermediate nested dir must be fsynced: {synced:?}"
        );
        assert!(
            synced.iter().any(|path| path == &root),
            "tree root dir must be fsynced: {synced:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_post_unseal_sync_failure_rolls_back_master() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_post_unseal_sync");
        let master = base.join("shared");
        let runtime = base.join("runtime");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(master.join("Cookies"), b"kimi-auth master").unwrap();
        fs::write(master.join("Local State"), b"master-state").unwrap();
        fs::write(runtime.join("Cookies"), b"kimi-auth runtime").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();

        sync_test_hooks::fail_once(master.clone());
        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();
        assert!(
            error.contains("wiederhergestellt"),
            "post-unseal sync failure must roll back: {error}"
        );
        assert!(
            error.contains("injected"),
            "rollback must preserve the sync failure: {error}"
        );
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"kimi-auth master"
        );
        assert_eq!(
            fs::read(master.join("Local State")).unwrap(),
            b"master-state"
        );
        assert!(
            !write_back_journal_pending_path(&master).exists(),
            "successful rollback must clear the pending journal after durable restore"
        );
        let master_syncs = sync_test_hooks::synced()
            .into_iter()
            .filter(|path| path == &master)
            .count();
        assert!(
            master_syncs >= 2,
            "mutated tree and restored tree must both be fsynced"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_pending_recovery_fsync_failure_keeps_journal() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_pending_sync_fail");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"corrupted").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        let pending = write_back_journal_pending_path(&master);
        fs::write(&pending, format!("{}\n", backup.display())).unwrap();

        sync_test_hooks::fail_always(master.clone());
        let error = recover_pending_write_back_transaction(&master).unwrap_err();
        assert!(
            error.contains("Crash-Recovery"),
            "pending recovery must fail closed when restored tree is not durable: {error}"
        );
        assert!(
            pending.exists(),
            "journal must remain until restored tree and parent are fsynced"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_pending_recovery_unseal_failure_reseals_master() {
        let base = unique_base("writeback_pending_unseal_fail");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"current").unwrap();
        fs::write(master.join("Local State"), b"current-state").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        set_profile_readonly_strict(&master, true).unwrap();
        let pending = write_back_journal_pending_path(&master);
        fs::write(&pending, format!("{}\n", backup.display())).unwrap();

        let _hook = readonly_test_hooks::install();
        readonly_test_hooks::fail_unseal_after(1, false);
        let error = recover_pending_write_back_transaction(&master).unwrap_err();

        assert!(
            error.contains("injected unseal permission failure"),
            "recovery must preserve the unseal failure: {error}"
        );
        assert!(
            readonly_test_hooks::reseal_attempted(),
            "recovery must attempt to reseal after a partial unseal"
        );
        assert!(
            fs::read_dir(&master).unwrap().all(|entry| entry
                .unwrap()
                .metadata()
                .unwrap()
                .permissions()
                .readonly()),
            "the complete master must be read-only after successful reseal"
        );
        assert_eq!(fs::read(master.join("Cookies")).unwrap(), b"current");
        assert!(pending.exists(), "failed recovery must keep its journal");

        set_profile_readonly_strict(&master, false).unwrap();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_pending_recovery_reports_unseal_and_reseal_failures() {
        let base = unique_base("writeback_pending_unseal_reseal_fail");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"current").unwrap();
        fs::write(master.join("Local State"), b"current-state").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        set_profile_readonly_strict(&master, true).unwrap();
        let pending = write_back_journal_pending_path(&master);
        fs::write(&pending, format!("{}\n", backup.display())).unwrap();

        let _hook = readonly_test_hooks::install();
        readonly_test_hooks::fail_unseal_after(1, true);
        let error = recover_pending_write_back_transaction(&master).unwrap_err();

        assert!(
            error.contains("injected unseal permission failure"),
            "combined failure must report unseal: {error}"
        );
        assert!(
            error.contains("injected reseal permission failure"),
            "combined failure must report reseal: {error}"
        );
        assert!(pending.exists(), "failed recovery must keep its journal");

        set_profile_readonly_strict(&master, false).unwrap();
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_pending_recovery_parent_fsync_failure_keeps_journal() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_pending_parent_sync");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        fs::write(master.join("Cookies"), b"corrupted").unwrap();
        fs::write(backup.join("Cookies"), b"last-good").unwrap();
        let pending = write_back_journal_pending_path(&master);
        fs::write(&pending, format!("{}\n", backup.display())).unwrap();

        let parent = master.parent().unwrap().to_path_buf();
        sync_test_hooks::fail_always(parent);
        let error = recover_pending_write_back_transaction(&master).unwrap_err();
        assert!(
            error.contains("Crash-Recovery"),
            "pending recovery must fail closed when parent dir is not durable: {error}"
        );
        assert!(
            pending.exists(),
            "journal must remain until parent dir is fsynced"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_post_verify_failure_restores_master_from_backup() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_post_verify_restore");
        let master = base.join("shared");
        let runtime = base.join("runtime");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(runtime.join("Cache")).unwrap();
        fs::write(master.join("Cookies"), b"master-cookie-jar").unwrap();
        fs::write(master.join("Local State"), b"master-state").unwrap();
        fs::write(runtime.join("Cookies"), b"runtime-cookie-jar").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();
        // `Cache` is a sparse-copy skip dir, so this weight counts towards the
        // source but can never reach the master. That is exactly the shape of a
        // write-back that passes the pre-checks and still lands too light:
        // post-verify must catch it and restore.
        fs::write(runtime.join("Cache").join("Cookies"), vec![b'x'; 64 * 1024]).unwrap();

        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();

        assert!(
            error.contains("Post-Verify fehlgeschlagen"),
            "a master that lands under the minimum ratio must fail post-verify: {error}"
        );
        assert!(
            error.contains("wiederhergestellt"),
            "post-verify failure must restore the master from backup: {error}"
        );
        assert_eq!(
            fs::read(master.join("Cookies")).unwrap(),
            b"master-cookie-jar",
            "restore must undo the partial write-back"
        );
        assert_eq!(
            fs::read(master.join("Local State")).unwrap(),
            b"master-state",
            "restore must undo the partial write-back"
        );
        assert!(
            !write_back_journal_pending_path(&master).exists(),
            "a completed post-verify rollback must clear the pending journal"
        );
        assert!(
            !write_back_journal_committed_path(&master).exists(),
            "a rolled-back write-back must never be journalled as committed"
        );

        let _ = set_profile_readonly_strict(&master, false);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_restore_failure_after_sync_failure_reports_both() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_double_restore_fail");
        let master = base.join("shared");
        let runtime = base.join("runtime");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(master.join("Cookies"), b"master-cookie-jar").unwrap();
        fs::write(master.join("Local State"), b"master-state").unwrap();
        fs::write(runtime.join("Cookies"), b"runtime-cookie-jar").unwrap();
        fs::write(runtime.join("Local State"), b"runtime-state").unwrap();

        // The same master tree stays undurable for the write-back AND for the
        // rollback, so the restore path itself fails. The caller must learn
        // both failures instead of seeing a reassuring "restored" message.
        sync_test_hooks::fail_always(master.clone());
        let error = write_back_dir_to_master_at(&runtime, &master, &backup).unwrap_err();

        assert!(
            error.contains("injected"),
            "the original durability failure must survive the failed rollback: {error}"
        );
        assert!(
            error.contains("Backup-Restore ebenfalls fehlgeschlagen"),
            "a failed restore must be reported, not masked as a successful rollback: {error}"
        );
        assert!(
            !error.contains("wiederhergestellt"),
            "a failed restore must never claim the master was restored: {error}"
        );
        assert!(
            write_back_journal_pending_path(&master).exists(),
            "an unrecovered master must keep its pending journal for crash recovery"
        );
        assert_eq!(
            fs::read_to_string(write_back_journal_pending_path(&master)).unwrap(),
            format!("{}\n", backup.display()),
            "the journal must still point at the backup that recovery needs"
        );
        assert!(
            !write_back_journal_committed_path(&master).exists(),
            "a failed write-back must never be journalled as committed"
        );

        let _ = set_profile_readonly_strict(&master, false);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn writeback_cleanup_second_parent_fsync_failure_restores_committed_journal() {
        let _hook = sync_test_hooks::install();
        let base = unique_base("writeback_cleanup_parent_sync");
        let master = base.join("shared");
        let backup = base.join("backup");
        fs::create_dir_all(&master).unwrap();
        fs::create_dir_all(&backup).unwrap();
        let pending = write_back_journal_pending_path(&master);
        let committed = write_back_journal_committed_path(&master);
        fs::write(&pending, format!("{}\n", backup.display())).unwrap();
        fs::write(&committed, format!("{}\n", backup.display())).unwrap();

        let parent = master.parent().unwrap().to_path_buf();
        sync_test_hooks::fail_on_nth(parent, 2);
        let error = clear_write_back_transaction(&master).unwrap_err();
        assert!(
            error.contains("Recovery-Journal") && error.contains("wiederhergestellt"),
            "cleanup must report that it restored recovery evidence: {error}"
        );
        assert!(
            !pending.exists(),
            "pending deletion must be fsynced before committed cleanup starts"
        );
        assert!(
            committed.exists(),
            "a parent fsync failure after committed deletion must restore discoverable evidence"
        );
        assert_eq!(
            fs::read_to_string(&committed).unwrap(),
            format!("{}\n", backup.display())
        );
        let _ = fs::remove_dir_all(&base);
    }
}
