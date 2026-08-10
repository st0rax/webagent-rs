use std::env;
use std::path::PathBuf;

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
///
/// Unter `cargo test` wird bewusst ein eigener Ort benutzt. Sonst schreiben
/// Testläufe in dieselben Dateien wie der Betrieb: im Score-Log standen real
/// Einträge mit `brain_id: "a"` und `"b"` neben den echten Brains und
/// verfälschten das Leaderboard. Ein Test, der Produktivdaten verändert, ist
/// kein Test mehr, sondern ein Nebeneffekt.
pub fn data_dir() -> PathBuf {
    if is_test_run() {
        return env::temp_dir().join("webagent_test_data");
    }
    webagent_root_stable().join("data")
}

/// Läuft der Prozess unter `cargo test`?
///
/// Cargo setzt beim Testlauf keine eindeutige Variable, aber die Testbinary
/// liegt immer unter `target/…/deps/`. Das ist das verlässlichste Signal ohne
/// zusätzliche Verdrahtung; wer es überstimmen muss, setzt
/// `WEBAGENT_FORCE_REAL_DATA=1`.
pub(crate) fn is_test_run() -> bool {
    if env::var("WEBAGENT_FORCE_REAL_DATA")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return false;
    }
    std::env::current_exe()
        .map(|p| {
            p.parent()
                .and_then(|d| d.file_name())
                .map(|n| n == "deps")
                .unwrap_or(false)
        })
        .unwrap_or(false)
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
