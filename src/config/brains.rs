use std::collections::HashMap;
use std::env;
use std::path::{Path, PathBuf};

use super::paths::*;

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
    // "IndexedDB" haelt bei mehreren AI-Web-UI die Sitzung statt der Cookies:
    // Claude authentifiziert ueber IndexedDB (gemessen 07.08.2026: kein
    // einziges Login-Cookie fuer die 9 Brains auf der Platte, dafuer frische
    // IndexedDB-Schreibungen). Ohne den Eintrag ueberlebt der Login den
    // Rueckweg ins Master nicht.
    "IndexedDB",
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
pub(crate) const SPARSE_SKIP_DIRS: &[&str] = &[
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
pub(crate) const FULL_COPY_SKIP_DIRS: &[&str] = &[
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

/// selectors/-Verzeichnis (ROOT/selectors/{brain}.json), wie SELECTORS_DIR in config.py.
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
/// ROOT/selectors/{brain}.json; jedes Brain erhaelt ein eigenes Profil unter
/// profiles/{brain} (Referenzprofil-Ansatz), das doctor prueft.
/// Nutzer-Selektoren am stabilen Ort: `<stable_root>/selectors/<brain>.json`.
///
/// `selectors_dir()` zeigt auf CARGO_MANIFEST_DIR, also den Build-Pfad — dort
/// liegen die mitgelieferten Selektoren, und dorthin darf zur Laufzeit nichts
/// geschrieben werden (ein deploytes Binary hat den Quellbaum evtl. gar nicht).
/// Dieses Verzeichnis ist das schreibbare Gegenstück und hat Vorrang: bricht
/// ein Anbieter sein HTML, ist der Brain hier reparierbar — ohne Neubau.
/// Unter `cargo test` bewusst ins Temp-Verzeichnis umgelenkt — wie [`data_dir`].
/// Sonst liest (und schreibt) ein Testlauf die echten Nutzer-Selektoren dieser
/// Maschine, und ein Test ueber ausgelieferte Daten wird zur Wettervorhersage.
pub fn user_selectors_dir() -> PathBuf {
    if is_test_run() {
        return env::temp_dir().join("webagent_test_data").join("selectors");
    }
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

/// Welche Selektor-Datei eines Brains obenauf liegt: Nutzer-Version, sonst
/// mitgelieferte. Rein fuer Anzeige und Existenzpruefungen (doctor, canary,
/// brains-health) — GELADEN wird nicht diese eine Datei, sondern beide
/// uebereinander, siehe ``load_selectors``.
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

