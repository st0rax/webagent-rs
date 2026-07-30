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
fn is_test_run() -> bool {
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

/// Voreingestellte maximale Observation-Länge in Zeichen.
///
/// Bleibt bei 12.000. Ich hatte den Wert am 30.07.2026 auf 40.000 angehoben,
/// weil Brains sich durch Dateien *lesen* statt sie zu bearbeiten — und das
/// wieder zurückgenommen: der Wert war geraten, nicht gemessen, und er behebt
/// die Ursache nicht.
///
/// `src/protocol.rs` hat 62.276 Zeichen, aber nur 34 Signaturen. Ein Brain, das
/// eine Funktion ergänzen soll, braucht die Fundstelle und genug Umgebung für
/// einen eindeutigen Anker — ein paar Dutzend Zeilen, nicht 1567. Ein höheres
/// Limit macht dieselbe Verschwendung nur teurer: dieselbe Datei, jetzt in
/// einem Rutsch, und das in jedem Turn des Verlaufs erneut.
///
/// Der Hebel ist stattdessen die Gliederung im Aufgabentext (siehe
/// `benchmark::file_outline`): Signaturen mit Zeilennummern, rund 2.000 statt
/// 62.000 Zeichen, danach liest das Brain gezielt.
pub const DEFAULT_MAX_OBSERVATION_CHARS: usize = 12_000;

/// Observation-Kappung für ein bestimmtes Brain — aus der Messung abgeleitet.
///
/// Am 30.07.2026 mit `webagent measure-limits` gemessen: chatgpt, deepseek,
/// kimi und zai nahmen jeweils **100.000 Zeichen** an, alle beim ersten
/// Versuch. Das ist die oberste Sprosse der Probenleiter, also eine untere
/// Schranke — nach oben ist offen. Der bisherige Wert von 12.000 war damit rund
/// achtmal zu vorsichtig; er stammte aus der Python-Portierung und war nie
/// nachgeprüft worden.
///
/// Genutzt wird die **Hälfte** des gemessenen Werts: die Messung gilt für eine
/// ganze Nachricht, unsere besteht aber aus Aufgabentext, Verlauf UND
/// Observation. Die Hälfte lässt Luft für den Rest, damit kein Turn an einer
/// Ablehnung verlorengeht.
///
/// Ohne Messwert bleibt der konservative Standard — bewusst klein, bis gemessen
/// wurde, statt zu raten.
pub fn max_observation_chars_for(brain_id: &str) -> usize {
    if let Ok(v) = env::var("WEBAGENT_MAX_OBSERVATION_CHARS") {
        if let Some(n) = v.trim().parse::<usize>().ok().filter(|n| *n >= 1_000) {
            return n;
        }
    }
    match crate::brain_limits::accepted_chars(brain_id) {
        Some(gemessen) => (gemessen / 2).max(DEFAULT_MAX_OBSERVATION_CHARS),
        None => DEFAULT_MAX_OBSERVATION_CHARS,
    }
}

/// Maximale Observation-Länge ohne Brain-Bezug (Fallback).
pub fn max_observation_chars() -> usize {
    env::var("WEBAGENT_MAX_OBSERVATION_CHARS")
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1_000)
        .unwrap_or(DEFAULT_MAX_OBSERVATION_CHARS)
}
/// Loop-Guard Warn-/Abort-Schwellen — Python `LOOP_GUARD_*`.
pub const LOOP_GUARD_WARN_COUNT: usize = 3;
pub const LOOP_GUARD_ABORT_COUNT: usize = 8;

/// Gesamte Wall-Clock-Obergrenze (Sekunden) für einen einzelnen Run. Fängt
/// hängende Läufe ab, die weder max_cycles noch der Loop-Guard je erreichen,
/// weil sie in der Warte-/Sendephase eines Brains klemmen (real beobachtet:
/// kimi hing 30+ min). Default; via WEBAGENT_MAX_RUN_SECONDS überschreibbar.
pub const MAX_RUN_WALL_SECONDS: u64 = 600;

/// Aufgelöste Wall-Clock-Deadline (Sekunden) eines Runs: WEBAGENT_MAX_RUN_SECONDS
/// falls gesetzt und sinnvoll, sonst MAX_RUN_WALL_SECONDS. Leer/„0"/ungültig →
/// Default.
pub fn max_run_wall_secs() -> u64 {
    resolve_max_run_wall_secs(env::var("WEBAGENT_MAX_RUN_SECONDS").ok().as_deref())
}

/// Reine Auflösung der Wall-Clock-Deadline (ohne Env-Zugriff, für Tests).
/// `None`/leer/„0"/nicht-parsebar → `MAX_RUN_WALL_SECONDS`; eine positive Zahl
/// wird übernommen.
pub fn resolve_max_run_wall_secs(raw: Option<&str>) -> u64 {
    match raw.map(str::trim) {
        Some(s) if !s.is_empty() => match s.parse::<u64>() {
            Ok(0) | Err(_) => MAX_RUN_WALL_SECONDS,
            Ok(n) => n,
        },
        _ => MAX_RUN_WALL_SECONDS,
    }
}

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
    // "Network" enthaelt bei aktuellem Chromium/WebView2 den Cookie-Jar
    // (Default/Network/Cookies) — ohne das ist die Kopie ausgeloggt.
    "Network",
    // "Local State" haelt den DPAPI-verschluesselten Schluessel, mit dem die
    // Cookies ueberhaupt erst entschluesselt werden. Fehlt er, sind die
    // kopierten Cookies wertlos.
    "Local State",
];

/// Verzeichnisse, die beim sparsamen Kopieren uebersprungen werden: reine
/// Caches/Diagnose, gross und fuer den Login irrelevant.
const SPARSE_SKIP_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "GPUPersistentCache",
    "GrShaderCache",
    "ShaderCache",
    "Crashpad",
    "BrowserMetrics",
    "EBWebViewMetrics",
    "component_crx_cache",
    "Service Worker",
];

/// Verzeichnisse, die auch bei der VOLLEN Profil-Kopie uebersprungen werden:
/// reine, jederzeit neu erzeugbare Caches ohne jeden Anmeldebezug.
///
/// Gemessen am 30.07.2026 an `profiles\kimi`: 459 MB gesamt, davon 312 MB
/// `Code Cache`, 77 MB `Cache` und 17 MB `component_crx_cache` — **88 % reiner
/// Cache**. Der Benchmark kopiert vor jedem Start acht solche Profile, also
/// ~3,6 GB, und zwar bevor die erste Logzeile erscheint: rund acht Minuten
/// stiller Startvorlauf pro Runde. Weil ein Kill den Aufraeum-Guard umgeht,
/// lagen am 30.07.2026 real 84 Kopien mit 37 GB unter `profiles\swarm`.
///
/// `Service Worker` steht bewusst NICHT hier: er ist kein reiner Cache und war
/// in der Messung ohnehin winzig. Wo Anmeldedaten liegen (Cookies, Local
/// Storage, IndexedDB), wird unveraendert vollstaendig kopiert.
const FULL_COPY_SKIP_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "GPUPersistentCache",
    "GrShaderCache",
    "ShaderCache",
    "Crashpad",
    "BrowserMetrics",
    "EBWebViewMetrics",
    "component_crx_cache",
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
/// Nutzer-Selektoren am stabilen Ort: `<stable_root>/selectors/<brain>.json`.
///
/// `selectors_dir()` zeigt auf CARGO_MANIFEST_DIR, also den Build-Pfad — dort
/// liegen die mitgelieferten Selektoren, und dorthin darf zur Laufzeit nichts
/// geschrieben werden (ein deploytes Binary hat den Quellbaum evtl. gar nicht).
/// Dieses Verzeichnis ist das schreibbare Gegenstück und hat Vorrang: bricht
/// ein Anbieter sein HTML, ist der Brain hier reparierbar — ohne Neubau.
pub fn user_selectors_dir() -> PathBuf {
    webagent_root_stable().join("selectors")
}

/// Selbst hinzugefügte Brains: `<stable_root>/data/custom_brains.json`.
pub fn custom_brains_path() -> PathBuf {
    data_dir().join("custom_brains.json")
}

/// Brain-IDs werden zu Dateinamen und Profilverzeichnissen — nur harmlose
/// Zeichen zulassen, damit ein Eintrag in custom_brains.json nicht aus dem
/// Datenverzeichnis ausbrechen kann.
pub fn sanitize_brain_id(s: &str) -> String {
    s.trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

/// Selektor-Datei eines Brains: Nutzer-Version schlägt mitgelieferte Version.
pub fn resolve_selectors_path(brain_id: &str) -> PathBuf {
    let user = user_selectors_dir().join(format!("{brain_id}.json"));
    if user.is_file() {
        return user;
    }
    selectors_dir().join(format!("{brain_id}.json"))
}

/// Selbst hinzugefügte Brains von der Platte lesen: `[{"id":..,"url":..}, ..]`.
/// Fehlende oder kaputte Datei = keine Custom-Brains (nie ein harter Fehler,
/// sonst legt eine verunglückte Datei den ganzen Agenten lahm).
pub fn load_custom_brains() -> Vec<(String, String)> {
    match std::fs::read_to_string(custom_brains_path()) {
        Ok(raw) => parse_custom_brains(&raw),
        Err(_) => Vec::new(),
    }
}

/// Reine Parse-/Filterlogik von [`load_custom_brains`] (ohne Dateizugriff).
pub fn parse_custom_brains(raw: &str) -> Vec<(String, String)> {
    let parsed: Vec<serde_json::Value> = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let builtin: Vec<&str> = BRAIN_TABLE.iter().map(|(id, _)| *id).collect();
    let mut out: Vec<(String, String)> = Vec::new();
    for entry in parsed {
        let id = sanitize_brain_id(entry.get("id").and_then(|v| v.as_str()).unwrap_or(""));
        let url = entry
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if id.is_empty() || url.is_empty() {
            continue;
        }
        // Eingebaute Brains nicht überschreibbar machen, sonst kann ein Tippfehler
        // in der Datei ein funktionierendes Brain auf eine falsche URL umbiegen.
        if builtin.contains(&id.as_str()) || out.iter().any(|(i, _)| i == &id) {
            continue;
        }
        out.push((id, url));
    }
    out
}

/// Ein selbst hinzugefügtes Brain eintragen (idempotent). Gibt `false` zurück,
/// wenn die ID bereits vergeben ist (eingebaut oder custom).
pub fn register_custom_brain(brain_id: &str, url: &str) -> std::io::Result<bool> {
    let id = sanitize_brain_id(brain_id);
    let url = url.trim().to_string();
    if id.is_empty() || url.is_empty() {
        return Ok(false);
    }
    if BRAIN_TABLE.iter().any(|(b, _)| *b == id) {
        return Ok(false);
    }
    let mut existing = load_custom_brains();
    if existing.iter().any(|(i, _)| i == &id) {
        return Ok(false);
    }
    existing.push((id, url));
    let list: Vec<serde_json::Value> = existing
        .into_iter()
        .map(|(i, u)| serde_json::json!({"id": i, "url": u}))
        .collect();
    let path = custom_brains_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(&list)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    crate::worker_pool::atomic_write(&path, body.as_bytes())?;
    Ok(true)
}

pub fn brains() -> HashMap<String, HashMap<String, String>> {
    let profiles = profiles_dir();
    // Optionaler Override: alle Brains dasselbe Profil nutzen lassen (z.B. das
    // eingeloggte Shared-Profil des Python-webagent) via WEBAGENT_PROFILE_DIR.
    let profile_override = env::var("WEBAGENT_PROFILE_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let mut brains = HashMap::new();
    let builtin = BRAIN_TABLE
        .iter()
        .map(|(i, u)| (i.to_string(), u.to_string()));
    for (id, url) in builtin.chain(load_custom_brains()) {
        let mut b = HashMap::new();
        b.insert("url".to_string(), url);
        b.insert(
            "selectors".to_string(),
            resolve_selectors_path(&id).to_string_lossy().to_string(),
        );
        let profile_dir = profile_override
            .clone()
            .unwrap_or_else(|| profiles.join(&id).to_string_lossy().to_string());
        b.insert("profile_dir".to_string(), profile_dir);
        brains.insert(id, b);
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
    base.join("swarm").join(format!("{}_{}", run_id, brain_id))
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
    Ok(())
}

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
/// „Voll" heisst vollstaendig bis auf reine Caches — siehe
/// [`FULL_COPY_SKIP_DIRS`].
fn copy_profile(src: &PathBuf, dst: &PathBuf, sparse: bool) -> std::io::Result<()> {
    if sparse {
        copy_dir_sparse(src, dst)
    } else {
        copy_dir_without_caches(src, dst)
    }
}

/// Wie [`copy_dir_all`], laesst aber die Verzeichnisse aus
/// [`FULL_COPY_SKIP_DIRS`] weg. Bewusst eine eigene Funktion: `copy_dir_all`
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
                if m < cutoff && std::fs::remove_dir_all(&path).is_ok() {
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

/// In die Binary eingebettete Selektoren — Fallback, damit eine heruntergeladene
/// `webagent.exe` OHNE mitgelieferten `selectors/`-Ordner sofort funktioniert.
/// Die Platte (`selectors_dir()`) hat weiterhin Vorrang, damit Dev-Edits und
/// selbst hinzugefuegte Brains greifen.
const EMBEDDED_SELECTORS: &[(&str, &str)] = &[
    ("chatgpt", include_str!("../selectors/chatgpt.json")),
    ("deepseek", include_str!("../selectors/deepseek.json")),
    ("kimi", include_str!("../selectors/kimi.json")),
    ("gemini", include_str!("../selectors/gemini.json")),
    ("qwen", include_str!("../selectors/qwen.json")),
    ("claude", include_str!("../selectors/claude.json")),
    ("mistral", include_str!("../selectors/mistral.json")),
    ("zai", include_str!("../selectors/zai.json")),
];

/// Eingebettete Selektor-JSON eines Brains (falls vorhanden).
pub fn embedded_selector(brain_id: &str) -> Option<&'static str> {
    EMBEDDED_SELECTORS
        .iter()
        .find(|(id, _)| *id == brain_id)
        .map(|(_, json)| *json)
}

/// Laedt die Selektor-JSON eines Brains. Zuerst von Platte (`selectors_dir()`),
/// damit Dev-Edits und eigene Brains greifen; fehlt die Datei (z. B. bei einer
/// heruntergeladenen exe ohne `selectors/`-Ordner), Fallback auf die in die
/// Binary eingebetteten Selektoren.
pub fn load_selectors(brain_id: &str) -> std::io::Result<serde_json::Value> {
    // resolve_selectors_path: Nutzer-Kopie unter <stable_root>/selectors schlaegt
    // die mitgelieferte Datei — so ist ein gebrochener Selektor reparierbar,
    // ohne den Quellbaum zu haben oder neu zu bauen.
    let path = resolve_selectors_path(brain_id);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match embedded_selector(brain_id) {
            Some(json) => json.to_string(),
            None => return Err(e),
        },
        Err(e) => return Err(e),
    };
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

    /// `assistant_message` darf NIE den laufenden Streaming-Container treffen.
    ///
    /// Real 2026-07-26: claudes Liste enthielt `div[data-is-streaming='true']`.
    /// Der Scraper las damit die Denk-Anzeige ("Crystallizing" samt
    /// Private-Use-Glyph) als fertige Antwort. Auswertung ueber 176 Laeufe:
    /// mit Denk-Glyph im Transkript 67% protocol_error, ohne 3% — Faktor 22,
    /// und 73 der 75 Protokollfehler hatten den Glyph. Schlimmer als die
    /// verfaelschte Messung: der Harness feuerte daraufhin identische
    /// Reparatur-Prompts, bis die Gegenseite das Gespraech beendete.
    #[test]
    fn assistant_message_trifft_nie_den_streaming_container() {
        for (brain, json_text) in EMBEDDED_SELECTORS {
            let v: serde_json::Value =
                serde_json::from_str(json_text).unwrap_or_else(|e| panic!("{brain}: {e}"));
            let Some(list) = v.get("assistant_message").and_then(|x| x.as_array()) else {
                continue;
            };
            for sel in list {
                let sel = sel.as_str().unwrap_or_default();
                assert!(
                    !sel.contains("data-is-streaming"),
                    "{brain}: assistant_message enthaelt den Streaming-Container `{sel}`                      — der Scraper liest damit die Denk-Anzeige als Antwort"
                );
            }
        }
    }

    #[test]
    fn test_root_dir_exists() {
        let root = root_dir();
        assert!(root.exists(), "Root-Verzeichnis sollte existieren");
        assert!(root.is_dir(), "Root sollte ein Verzeichnis sein");
    }

    #[test]
    fn test_embedded_selectors_cover_all_brains_and_parse() {
        // Beweist Portabilitaet: jede heruntergeladene exe hat die Selektoren
        // fuer alle BRAIN_TABLE-Brains eingebettet und sie sind gueltiges JSON.
        for (id, _url) in BRAIN_TABLE {
            let embedded = embedded_selector(id)
                .unwrap_or_else(|| panic!("kein eingebetteter Selektor fuer Brain '{id}'"));
            let parsed: serde_json::Value = serde_json::from_str(embedded)
                .unwrap_or_else(|e| panic!("eingebetteter Selektor '{id}' ist kein JSON: {e}"));
            assert!(
                parsed.is_object(),
                "eingebetteter Selektor '{id}' sollte ein JSON-Objekt sein"
            );
        }
    }

    #[test]
    fn test_embedded_selector_unknown_brain_is_none() {
        assert!(embedded_selector("does-not-exist").is_none());
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
        assert_eq!(DEFAULT_MAX_OBSERVATION_CHARS, 12_000);
        // Ohne Env-Ueberschreibung gilt der Standard.
        assert!(max_observation_chars() >= 1_000);
        assert_eq!(LOOP_GUARD_WARN_COUNT, 3);
        assert_eq!(LOOP_GUARD_ABORT_COUNT, 8);
    }

    #[test]
    fn test_resolve_max_run_wall_secs_parsing() {
        // Default-Fälle: None, leer, nur Whitespace, "0", Müll → Default.
        assert_eq!(resolve_max_run_wall_secs(None), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("   ")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("0")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("abc")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("-5")), MAX_RUN_WALL_SECONDS);
        assert_eq!(resolve_max_run_wall_secs(Some("12x")), MAX_RUN_WALL_SECONDS);
        // Gültige positive Werte (auch mit umgebendem Whitespace) → übernommen.
        assert_eq!(resolve_max_run_wall_secs(Some("900")), 900);
        assert_eq!(resolve_max_run_wall_secs(Some("  900  ")), 900);
        assert_eq!(resolve_max_run_wall_secs(Some("1")), 1);
        assert_eq!(MAX_RUN_WALL_SECONDS, 600);
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
    fn test_sweep_stale_runtime_profiles_spares_fresh_and_logins() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_sweep_{}", stamp));
        let swarm_orphan = base.join("swarm").join("deadrun_chatgpt");
        let enc_orphan = base.join("encapsulated").join("chatgpt_deadstamp");
        // Kanonische Profile — hier liegen die Logins, die darf der Sweep nie anfassen.
        let login_shared = base.join("shared");
        let login_brain = base.join("chatgpt");
        for d in [&swarm_orphan, &enc_orphan, &login_shared, &login_brain] {
            fs::create_dir_all(d).expect("mkdir");
            fs::write(d.join("marker"), b"x").expect("write");
        }

        // max_age = 0 -> jedes Wegwerf-Profil gilt als alt. Beide Wurzeln werden
        // erfasst, die Login-Profile aber nicht.
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 0), 2);
        assert!(!swarm_orphan.exists(), "swarm-Waise entfernt");
        assert!(!enc_orphan.exists(), "encapsulated-Waise entfernt");
        assert!(login_shared.is_dir(), "shared-Login unangetastet");
        assert!(login_brain.is_dir(), "Brain-Login unangetastet");

        // Frisch + realistische Grenze -> ein laufender Run bleibt stehen.
        fs::create_dir_all(&swarm_orphan).expect("recreate");
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 12 * 60 * 60), 0);
        assert!(swarm_orphan.is_dir(), "laufender Run darf nicht weg");

        // Fehlende Wurzeln sind kein Fehler.
        let _ = fs::remove_dir_all(&base);
        assert_eq!(sweep_stale_runtime_profiles_in(&base, 0), 0);
    }

    #[test]
    fn volle_profilkopie_laesst_caches_weg_aber_keine_logins() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("webagent_cachecopy_{stamp}"));
        let src = base.join("src");
        let dst = base.join("dst");

        // Anmeldebezogenes ...
        let net = src.join("EBWebView").join("Default").join("Network");
        fs::create_dir_all(&net).expect("mkdir");
        fs::write(net.join("Cookies"), b"keks").expect("write");
        let ls = src.join("EBWebView").join("Default").join("Local Storage");
        fs::create_dir_all(&ls).expect("mkdir");
        fs::write(ls.join("leveldb.log"), b"token").expect("write");

        // ... und reiner Cache, der 88 % des Volumens ausmacht.
        let cc = src.join("EBWebView").join("Default").join("Code Cache");
        fs::create_dir_all(&cc).expect("mkdir");
        fs::write(cc.join("gross.bin"), vec![0u8; 4096]).expect("write");
        let crx = src.join("EBWebView").join("component_crx_cache");
        fs::create_dir_all(&crx).expect("mkdir");
        fs::write(crx.join("x.crx"), b"crx").expect("write");

        copy_dir_without_caches(&src, &dst).expect("copy");

        let d = dst.join("EBWebView").join("Default");
        assert!(
            d.join("Network").join("Cookies").is_file(),
            "Cookies fehlen"
        );
        assert!(
            d.join("Local Storage").join("leveldb.log").is_file(),
            "Local Storage fehlt"
        );
        assert!(!d.join("Code Cache").exists(), "Code Cache mitkopiert");
        assert!(
            !dst.join("EBWebView").join("component_crx_cache").exists(),
            "crx-Cache mitkopiert"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn tests_never_write_into_the_production_data_dir() {
        // Real beobachtet: im Score-Log standen Eintraege mit brain_id "a" und
        // "b" neben den echten Brains — aus Testlaeufen, die in dieselbe Datei
        // schrieben wie der Betrieb. Das verfaelscht das Leaderboard.
        let d = data_dir();
        assert!(
            d.starts_with(std::env::temp_dir()),
            "unter cargo test muss data_dir im Temp liegen, ist aber {d:?}"
        );
        assert!(
            !d.starts_with(webagent_root_stable()),
            "darf nicht auf den Produktivort zeigen"
        );
    }

    #[test]
    fn test_sanitize_brain_id_blocks_path_escape() {
        assert_eq!(sanitize_brain_id("  MyBrain  "), "mybrain");
        assert_eq!(sanitize_brain_id("chat.z.ai"), "chat-z-ai");
        // Ein Eintrag in custom_brains.json darf nicht aus dem Datenverzeichnis ausbrechen.
        assert_eq!(sanitize_brain_id("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_brain_id("a/b\\c"), "a-b-c");
        assert!(!sanitize_brain_id("../x").contains(".."));
        assert_eq!(sanitize_brain_id("---"), "");
        assert_eq!(sanitize_brain_id(""), "");
    }

    #[test]
    fn test_load_custom_brains_skips_junk_and_builtin_shadowing() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("webagent_custom_{}", stamp));
        fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("custom_brains.json");

        // Kaputtes JSON darf den Agenten nicht lahmlegen -> leere Liste, kein Panic.
        fs::write(&path, b"{ not json").expect("write");
        assert!(parse_custom_brains(&fs::read_to_string(&path).unwrap()).is_empty());

        let raw = r#"[
            {"id": "Grok", "url": "https://grok.com/"},
            {"id": "chatgpt", "url": "https://evil.example/"},
            {"id": "grok", "url": "https://dup.example/"},
            {"id": "", "url": "https://nada/"},
            {"id": "leer-url", "url": "  "}
        ]"#;
        let got = parse_custom_brains(raw);
        assert_eq!(
            got,
            vec![("grok".to_string(), "https://grok.com/".to_string())],
            "eingebautes chatgpt nicht ueberschreibbar, Dubletten und Luecken raus"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_resolve_selectors_prefers_user_copy() {
        // Ohne Nutzer-Datei muss der mitgelieferte Pfad herauskommen.
        let p = resolve_selectors_path("chatgpt");
        assert!(p.to_string_lossy().ends_with("chatgpt.json"));
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
    fn test_copy_dir_sparse_finds_nested_webview2_artifacts() {
        // Regression fuer den Fund 2026-07-21: WebView2 legt alles unter
        // EBWebView/Default/ ab (Cookies sogar unter Default/Network/Cookies).
        // Die frueher nur oberflaechliche Suche traf die Whitelist NIE — die
        // Swarm-Kopien waren leer und die Brains wirkten ausgeloggt.
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let src = root_dir().join(format!("data/test_sparse_nested_src_{}", stamp));
        let dst = root_dir().join(format!("data/test_sparse_nested_dst_{}", stamp));
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);

        let web = src.join("EBWebView");
        let default = web.join("Default");
        fs::create_dir_all(default.join("Network")).unwrap();
        fs::create_dir_all(default.join("Local Storage")).unwrap();
        fs::create_dir_all(web.join("Crashpad")).unwrap();
        fs::create_dir_all(default.join("Cache")).unwrap();
        // Auth-relevant, verschachtelt:
        fs::write(default.join("Network").join("Cookies"), b"jar").unwrap();
        fs::write(web.join("Local State"), b"key").unwrap();
        fs::write(default.join("Preferences"), b"prefs").unwrap();
        fs::write(default.join("Local Storage").join("leveldb"), b"ls").unwrap();
        // Ballast, der NICHT mitkommen soll:
        fs::write(web.join("Crashpad").join("dump"), b"x").unwrap();
        fs::write(default.join("Cache").join("blob"), b"x").unwrap();
        fs::write(default.join("History"), b"h").unwrap();

        copy_dir_sparse(&src.to_path_buf(), &dst.to_path_buf()).unwrap();

        let d_web = dst.join("EBWebView");
        let d_def = d_web.join("Default");
        assert!(
            d_def.join("Network").join("Cookies").is_file(),
            "Cookie-Jar (Default/Network/Cookies) muss mitkommen"
        );
        assert!(
            d_web.join("Local State").is_file(),
            "Local State (Entschluesselungs-Key) muss mitkommen"
        );
        assert!(d_def.join("Preferences").is_file(), "Preferences kopiert");
        assert!(
            d_def.join("Local Storage").join("leveldb").is_file(),
            "Local Storage kopiert"
        );
        assert!(!d_def.join("History").exists(), "History bleibt weg");
        assert!(
            !d_web.join("Crashpad").exists(),
            "Crashpad wird uebersprungen"
        );
        assert!(!d_def.join("Cache").exists(), "Cache wird uebersprungen");

        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dst);
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
        assert!(
            dst.join("Login Data").is_file(),
            "Login Data (whitelist) kopiert"
        );
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
        assert!(
            !dst.join("SingletonLock").exists(),
            "Lock-File nicht kopiert"
        );

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

        assert!(
            dst.join("resources.pak").is_file(),
            "(A) kopiert cross-drive"
        );
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
