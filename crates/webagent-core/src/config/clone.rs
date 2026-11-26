use std::path::{Path, PathBuf};

use super::profiles::copy_dir_all;

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
