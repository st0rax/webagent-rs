use std::env;
use std::path::PathBuf;

use super::brains::{brains, selectors_dir, user_selectors_dir};
use super::paths::profiles_dir;

/// In die Binary eingebettete Selektoren — Fallback, damit eine heruntergeladene
/// `webagent.exe` OHNE mitgelieferten `selectors/`-Ordner sofort funktioniert.
/// Die Platte (`selectors_dir()`) hat weiterhin Vorrang, damit Dev-Edits und
/// selbst hinzugefuegte Brains greifen.
pub(crate) const EMBEDDED_SELECTORS: &[(&str, &str)] = &[
    ("chatgpt", include_str!("../../selectors/chatgpt.json")),
    ("deepseek", include_str!("../../selectors/deepseek.json")),
    ("kimi", include_str!("../../selectors/kimi.json")),
    ("gemini", include_str!("../../selectors/gemini.json")),
    ("qwen", include_str!("../../selectors/qwen.json")),
    ("claude", include_str!("../../selectors/claude.json")),
    ("mistral", include_str!("../../selectors/mistral.json")),
    ("zai", include_str!("../../selectors/zai.json")),
];

/// Eingebettete Selektor-JSON eines Brains (falls vorhanden).
pub fn embedded_selector(brain_id: &str) -> Option<&'static str> {
    EMBEDDED_SELECTORS
        .iter()
        .find(|(id, _)| *id == brain_id)
        .map(|(_, json)| *json)
}

/// Die ausgelieferten Selektoren, so wie sie im Binary stecken — (id, JSON).
/// Fuer Tests, die eine Aussage ueber die MITGELIEFERTEN Daten treffen wollen
/// und dafuer nichts von der Platte lesen duerfen.
pub fn shipped_selector_table() -> &'static [(&'static str, &'static str)] {
    EMBEDDED_SELECTORS
}

/// Liest eine Selektor-Datei, falls vorhanden. Fehlende Datei = `None` (kein
/// Fehler); kaputtes JSON bleibt ein Fehler — sonst faellt eine verungluecke
/// Reparatur still auf die Basis zurueck und niemand merkt es.
fn read_selector_file(path: &std::path::Path) -> std::io::Result<Option<serde_json::Value>> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    serde_json::from_str(&content).map(Some).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}: {e}", path.display()),
        )
    })
}

/// Die mitgelieferten Selektoren eines Brains: Datei aus `selectors_dir()`
/// (damit Dev-Edits am Quellbaum greifen), sonst die eingebettete Kopie (damit
/// eine heruntergeladene exe ohne `selectors/`-Ordner sofort funktioniert).
pub fn shipped_selectors(brain_id: &str) -> std::io::Result<Option<serde_json::Value>> {
    if let Some(v) = read_selector_file(&selectors_dir().join(format!("{brain_id}.json")))? {
        return Ok(Some(v));
    }
    match embedded_selector(brain_id) {
        Some(json) => serde_json::from_str(json).map(Some).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("eingebettete Selektoren {brain_id}: {e}"),
            )
        }),
        None => Ok(None),
    }
}

/// Die lokale Nutzer-Datei eines Brains (`<stable_root>/selectors/<id>.json`),
/// falls vorhanden. Das ist das Overlay, nicht die ganze Wahrheit — wer den
/// tatsaechlich gueltigen Stand braucht, nimmt [`load_selectors`].
pub fn user_selectors(brain_id: &str) -> std::io::Result<Option<serde_json::Value>> {
    read_selector_file(&user_selectors_dir().join(format!("{brain_id}.json")))
}

/// Overlay ueber Basis legen: je Oberschluessel gewinnt die Nutzer-Datei
/// vollstaendig. Bewusst NICHT listenweise vereinigen — wer einen gebrochenen
/// Selektor ersetzt, will ihn los sein, nicht ergaenzt haben.
pub(crate) fn merge_selectors(base: &mut serde_json::Value, overlay: serde_json::Value) {
    match (base.as_object_mut(), overlay) {
        (Some(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                b.insert(k, v);
            }
        }
        // Eine Nutzer-Datei, die kein Objekt ist, gilt trotzdem: sie ist die
        // bewusste Aussage des Menschen, die Basis nur der Lieferstand.
        (_, other) => *base = other,
    }
}

/// Laedt die gueltigen Selektoren eines Brains: mitgelieferte Datei als Basis,
/// lokale Nutzer-Datei als Overlay darueber.
///
/// Frueher ersetzte die Nutzer-Datei die mitgelieferte KOMPLETT. Ein
/// `probe --write` schrieb dann einen Messschnappschuss auf die Platte, und ab
/// da war jede spaetere Pflege im Repo fuer diese Maschine unsichtbar — auch
/// fuer Schluessel, die die Messung nie angefasst hat. Overlay statt Ersatz
/// haelt beides: reparierbar ohne Neubau, ohne den Rest einzufrieren.
pub fn load_selectors(brain_id: &str) -> std::io::Result<serde_json::Value> {
    let shipped = shipped_selectors(brain_id)?;
    let user = user_selectors(brain_id)?;
    match (shipped, user) {
        (Some(mut base), Some(overlay)) => {
            merge_selectors(&mut base, overlay);
            Ok(base)
        }
        (Some(base), None) => Ok(base),
        // Selbst hinzugefuegte Brains haben keine Basis — dort ist die
        // Nutzer-Datei alles, was es gibt.
        (None, Some(overlay)) => Ok(overlay),
        (None, None) => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("keine Selektoren fuer '{brain_id}'"),
        )),
    }
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
pub(crate) fn fnv1a(s: &str) -> u32 {
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
