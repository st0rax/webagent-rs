use std::env;
use std::path::PathBuf;

/// Root-Verzeichnis der WebAgent-Installation (Elternverzeichnis von src/).
/// Compile-Zeit-Pfad — nur als Legacy-Quelle der Migration und für
/// Entwicklungs-Prompts. Nutzerdaten (Profile/Data) hängen NICHT daran,
/// siehe `webagent_root_stable`.
///
/// T-1011: `config` moved into the `webagent-core` member. Two levels above
/// that crate's manifest is the workspace root, which keeps the pre-extraction
/// path semantics for callers in the root crate. Bundled selector assets are
/// crate-local and have their own path in `config::selectors_dir()`.
pub fn root_dir() -> PathBuf {
    let candidate = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Prompt building canonicalizes the workspace root; make the helper agree
    // so `build_task_prompt()` and its callers see the same string.
    candidate.canonicalize().unwrap_or(candidate)
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
/// Cargo setzt beim Testlauf keine eindeutige Variable. Früher wurde nur
/// geprüft, ob die Binary unter `target/…/deps/` liegt. Auf dieser Werkbank
/// legt Cargo die Testbinaries jedoch unter
/// `target/debug/build/<crate>/<hash>/out/<target>-<16 Hex>.exe` ab — die
/// Annahme war also falsch, und Tests schrieben in
/// `%LOCALAPPDATA%\webagent\data` (echtes Leaderboard und Circuit-Breaker).
/// Der erste Fix erkannte nur `webagent-<16 Hex>.exe`; damit blieben die
/// Testbinaries der Untercrates (`webagent_api-…`, `webagent_core-…`,
/// `arch_layers-…`) unerkannt. Entscheidend ist der 16-Hex-Suffix, nicht der
/// Crate-Name davor. `cfg!(test)` deckt zusätzlich die Unit-Tests der eigenen
/// Crates ab. Wer es überstimmen muss, setzt `WEBAGENT_FORCE_REAL_DATA=1`.
pub(crate) fn is_test_run() -> bool {
    if env::var("WEBAGENT_FORCE_REAL_DATA")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return false;
    }
    if cfg!(test) {
        return true;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let name = exe.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    if is_cargo_test_binary_name(name) {
        return true;
    }
    exe.parent()
        .and_then(|d| d.file_name())
        .map(|n| n == "deps")
        .unwrap_or(false)
}

/// Erkennt Cargo-Testbinaries am unverwechselbaren `<name>-<16 Hex>`-Suffix.
/// Der Präfix ist bewusst egal: je nach Crate heißt die Datei
/// `webagent-…`, `webagent_api-…`, `webagent_core-…` oder `arch_layers-…`.
/// Die produktive `webagent.exe` hat keinen Hex-Suffix und bleibt erkannt
/// als Nicht-Testlauf.
fn is_cargo_test_binary_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".exe").unwrap_or(&lower);
    let Some((_, hash)) = stem.rsplit_once('-') else {
        return false;
    };
    hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit())
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

/// profiles/shared/ - Gemeinsames Browser-Profil (wenn shared_browser aktiviert)
pub fn shared_profile_dir() -> PathBuf {
    profiles_dir().join("shared")
}

#[cfg(test)]
mod tests {
    use super::is_cargo_test_binary_name;

    #[test]
    fn testbinary_suffix_ist_unabhaengig_vom_crate_namen() {
        for name in [
            "webagent-0123456789abcdef.exe",
            "webagent_api-5dac752d0b479209.exe",
            "webagent_core-5dac752d0b479209.exe",
            "arch_layers-5dac752d0b479209.exe",
            "webagent_api-0123456789abcdef",
            "zz_audit_shell-0123456789ABCDEF.exe",
        ] {
            assert!(
                is_cargo_test_binary_name(name),
                "Testbinary nicht erkannt: {name}"
            );
        }
    }

    #[test]
    fn produktivbinary_ist_kein_testbinary() {
        for name in [
            "webagent.exe",
            "webagent",
            "webagent-1234.exe",
            "webagent-0123456789abcdefgh.exe",
            "webagent-0123456789abcdef.bak",
        ] {
            assert!(
                !is_cargo_test_binary_name(name),
                "Falsch als Testbinary erkannt: {name}"
            );
        }
    }
}
