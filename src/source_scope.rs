//! Session-Source-Scope (T-602): genau eine aktive Quelle je Brain je Session.
//!
//! Standardquelle ist der Browser-Chat (`default`). Eine OpenAI-kompatible
//! API-Quelle wird nur manuell gewaehlt (Web-UI-Schalter oder `/quelle`).
//! Es gibt kein automatisches Routing und keinen versteckten Fallback.
//! `providers.json` wird ausschliesslich bei ausdruecklichem `--save` geschrieben.
//! Keys bleiben Env-Namen (T-601); Transport bleibt der rustls-Client.

use crate::https_client::{ProviderSource, ProvidersFile};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Kanonischer Name der Browser-Chat-Quelle.
pub const BROWSER_SOURCE: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    Browser,
    Api,
}

impl SourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceKind::Browser => "browser",
            SourceKind::Api => "api",
        }
    }
}

/// Aktive Quelle eines Brains in einer Session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveSource {
    pub brain: String,
    pub source: String,
    pub kind: SourceKind,
    pub model: Option<String>,
    pub persisted: bool,
}

/// Eintrag fuer die Quellenliste (UI-Schalter / `/quelle list`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedSource {
    pub id: String,
    pub label: String,
    pub kind: SourceKind,
    pub active: bool,
}

/// `/quelle <brain> <quelle|list|default>` plus optionales `--save`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuelleCommand {
    pub brain: Option<String>,
    pub spec: QuelleSpec,
    pub save: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuelleSpec {
    List,
    Show,
    Set(String),
}

/// Menschliche Kurzfassung nach einem `/quelle`-Lauf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuelleReport {
    pub text: String,
    pub persisted: bool,
    pub active: Option<ActiveSource>,
}

/// Session-lokale Quellenwahl plus Katalog aus `providers.json`.
#[derive(Debug, Clone)]
pub struct SourceScope {
    overrides: BTreeMap<String, String>,
    catalog: ProvidersFile,
    path: PathBuf,
}

impl Default for SourceScope {
    fn default() -> Self {
        Self::load_default()
    }
}

impl SourceScope {
    pub fn providers_path() -> PathBuf {
        crate::config::data_dir().join("providers.json")
    }

    pub fn load_default() -> Self {
        let path = Self::providers_path();
        let catalog = ProvidersFile::load_default().unwrap_or_else(|_| ProvidersFile {
            version: 1,
            brains: BTreeMap::new(),
        });
        Self {
            overrides: BTreeMap::new(),
            catalog,
            path,
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, String> {
        let catalog = if path.exists() {
            ProvidersFile::load_path(path)?
        } else {
            ProvidersFile {
                version: 1,
                brains: BTreeMap::new(),
            }
        };
        Ok(Self {
            overrides: BTreeMap::new(),
            catalog,
            path: path.to_path_buf(),
        })
    }

    pub fn in_memory(catalog: ProvidersFile) -> Self {
        Self {
            overrides: BTreeMap::new(),
            catalog,
            path: PathBuf::from("providers.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Aktive Quelle: Session-Override, sonst Katalog, sonst Browser.
    /// Verfuegbarkeit, Health und Breaker werden absichtlich nicht gelesen.
    pub fn active(&self, brain: &str) -> ActiveSource {
        if let Some(source) = self.overrides.get(brain) {
            return self.describe(brain, source);
        }
        let persisted = self.catalog.source_of(brain);
        self.describe(brain, &persisted.source)
    }

    /// Verfuegbarkeit aendert die Wahl nicht. Kein Fallback, kein Auto-Routing.
    pub fn active_ignoring_health(
        &self,
        brain: &str,
        _health: &BTreeMap<String, bool>,
    ) -> ActiveSource {
        self.active(brain)
    }

    pub fn list(&self, brain: Option<&str>) -> Vec<ListedSource> {
        match brain {
            Some(b) => self.list_brain(b),
            None => {
                let mut brains: Vec<String> = self.catalog.brains.keys().cloned().collect();
                for k in self.overrides.keys() {
                    if !brains.iter().any(|existing| existing == k) {
                        brains.push(k.clone());
                    }
                }
                brains.sort();
                if brains.is_empty() {
                    return vec![ListedSource {
                        id: BROWSER_SOURCE.to_string(),
                        label: "Browser-Chat".to_string(),
                        kind: SourceKind::Browser,
                        active: true,
                    }];
                }
                brains
                    .into_iter()
                    .flat_map(|b| self.list_brain(&b))
                    .collect()
            }
        }
    }

    fn list_brain(&self, brain: &str) -> Vec<ListedSource> {
        let active = self.active(brain);
        let mut ids: BTreeMap<String, SourceKind> = BTreeMap::new();
        ids.insert(BROWSER_SOURCE.to_string(), SourceKind::Browser);
        let cat = self.catalog.source_of(brain);
        let canon = canonicalize_source(&cat.source).unwrap_or_else(|_| BROWSER_SOURCE.to_string());
        if !is_browser_source(&canon) {
            ids.insert(canon, SourceKind::Api);
        }
        if let Some(over) = self.overrides.get(brain) {
            if !is_browser_source(over) {
                ids.insert(over.clone(), SourceKind::Api);
            }
        }
        ids.into_iter()
            .map(|(id, kind)| ListedSource {
                label: source_label(&id, kind),
                active: id == active.source,
                id,
                kind,
            })
            .collect()
    }

    pub fn set_session(&mut self, brain: &str, source: &str) -> Result<ActiveSource, String> {
        let brain = normalize_brain(brain)?;
        let source = canonicalize_source(source)?;
        self.overrides.insert(brain.clone(), source.clone());
        Ok(self.describe(&brain, &source))
    }

    pub fn persist_brain(&mut self, brain: &str) -> Result<ActiveSource, String> {
        let brain = normalize_brain(brain)?;
        let active = self.active(&brain);
        let mut entry = self.catalog.brains.remove(&brain).unwrap_or_default();
        entry.source = active.source.clone();
        if is_browser_source(&entry.source) {
            entry.source = BROWSER_SOURCE.to_string();
        }
        self.catalog.brains.insert(brain.clone(), entry);
        if self.catalog.version == 0 {
            self.catalog.version = 1;
        }
        self.write_catalog()?;
        self.overrides.remove(&brain);
        Ok(self.describe(&brain, &self.catalog.source_of(&brain).source))
    }

    fn write_catalog(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("providers.json Ordner: {e}"))?;
        }
        let json = serde_json::to_string_pretty(&self.catalog)
            .map_err(|e| format!("providers.json serialize: {e}"))?;
        std::fs::write(&self.path, json)
            .map_err(|e| format!("providers.json schreiben: {e}"))?;
        ProvidersFile::load_path(&self.path).map(|_| ())
    }

    pub fn apply(&mut self, cmd: &QuelleCommand) -> Result<QuelleReport, String> {
        if cmd.save && !matches!(cmd.spec, QuelleSpec::Set(_)) {
            return Err("--save gilt nur beim Setzen einer Quelle.".to_string());
        }
        match (&cmd.brain, &cmd.spec) {
            (None, QuelleSpec::List) | (None, QuelleSpec::Show) => Ok(QuelleReport {
                text: self.format_list(None),
                persisted: false,
                active: None,
            }),
            (Some(brain), QuelleSpec::List) => Ok(QuelleReport {
                text: self.format_list(Some(brain)),
                persisted: false,
                active: None,
            }),
            (Some(brain), QuelleSpec::Show) => {
                let active = self.active(brain);
                Ok(QuelleReport {
                    text: format_active(&active),
                    persisted: active.persisted,
                    active: Some(active),
                })
            }
            (Some(brain), QuelleSpec::Set(source)) => {
                let mut active = self.set_session(brain, source)?;
                let mut persisted = false;
                if cmd.save {
                    active = self.persist_brain(brain)?;
                    persisted = true;
                }
                let note = if persisted {
                    "persistiert in providers.json"
                } else {
                    "nur Session, nicht persistiert"
                };
                Ok(QuelleReport {
                    text: format!("{} ({note})", format_active(&active)),
                    persisted,
                    active: Some(active),
                })
            }
            (None, QuelleSpec::Set(_)) => {
                Err("Nutzung: /quelle <brain> <quelle|list|default> [--save]".to_string())
            }
        }
    }

    fn format_list(&self, brain: Option<&str>) -> String {
        if let Some(b) = brain {
            let active = self.active(b);
            let rows: Vec<String> = self
                .list(Some(b))
                .into_iter()
                .map(|s| {
                    let mark = if s.active { "*" } else { " " };
                    format!("{mark} {} ({})", s.id, s.kind.as_str())
                })
                .collect();
            format!(
                "[quelle] {b}: aktiv {} ({})\n{}",
                active.source,
                active.kind.as_str(),
                rows.join("\n")
            )
        } else {
            let mut brains: Vec<String> = self.catalog.brains.keys().cloned().collect();
            for k in self.overrides.keys() {
                if !brains.iter().any(|existing| existing == k) {
                    brains.push(k.clone());
                }
            }
            brains.sort();
            if brains.is_empty() {
                return "[quelle] keine Session-Overrides; Standard ist Browser-Chat.".to_string();
            }
            let lines: Vec<String> = brains
                .iter()
                .map(|b| format_active(&self.active(b)))
                .collect();
            format!("[quelle]\n{}", lines.join("\n"))
        }
    }

    fn describe(&self, brain: &str, source: &str) -> ActiveSource {
        let source = canonicalize_source(source).unwrap_or_else(|_| BROWSER_SOURCE.to_string());
        let kind = if is_browser_source(&source) {
            SourceKind::Browser
        } else {
            SourceKind::Api
        };
        let catalog_src = self.catalog.source_of(brain);
        let catalog_canon =
            canonicalize_source(&catalog_src.source).unwrap_or_else(|_| BROWSER_SOURCE.to_string());
        let model = if kind == SourceKind::Api {
            catalog_src.model.filter(|_| catalog_canon == source)
        } else {
            None
        };
        ActiveSource {
            brain: brain.to_string(),
            source,
            kind,
            model,
            persisted: !self.overrides.contains_key(brain),
        }
    }
}

/// Health-/Breaker-Karte wird bewusst ignoriert: die manuelle Wahl bleibt stehen.
pub fn choose_without_fallback(chosen: &str, _health: &BTreeMap<String, bool>) -> String {
    canonicalize_source(chosen).unwrap_or_else(|_| BROWSER_SOURCE.to_string())
}

pub fn is_browser_source(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "default" | "browser" | "browser-chat"
    )
}

pub fn canonicalize_source(raw: &str) -> Result<String, String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return Err("Quelle fehlt.".to_string());
    }
    if s == "list" {
        return Err("list ist ein Unterbefehl, kein Quellenname.".to_string());
    }
    if is_browser_source(&s) {
        return Ok(BROWSER_SOURCE.to_string());
    }
    if s.contains("sk-") || s.len() > 64 {
        return Err("Quellenname ungueltig (kein Secret, max. 64 Zeichen).".to_string());
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
    {
        return Err("Quellenname darf nur Buchstaben, Zahlen, -_. : enthalten.".to_string());
    }
    Ok(s)
}

fn normalize_brain(raw: &str) -> Result<String, String> {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return Err("Brain fehlt.".to_string());
    }
    if s == "list" {
        return Err("list ist kein Brain.".to_string());
    }
    Ok(s)
}

fn source_label(id: &str, kind: SourceKind) -> String {
    match kind {
        SourceKind::Browser => "Browser-Chat".to_string(),
        SourceKind::Api => id.to_string(),
    }
}

fn format_active(active: &ActiveSource) -> String {
    let where_ = if active.persisted {
        "persistiert"
    } else {
        "Session"
    };
    format!(
        "[quelle] {}: {} ({}, {where_})",
        active.brain,
        active.source,
        active.kind.as_str()
    )
}

/// Parst die Argumente hinter `/quelle`.
pub fn parse_quelle_args(rest: &str) -> Result<QuelleCommand, String> {
    let mut save = false;
    let mut tokens: Vec<String> = Vec::new();
    for tok in rest.split_whitespace() {
        if tok == "--save" {
            save = true;
        } else if let Some(stripped) = tok.strip_prefix("--save=") {
            save = matches!(stripped.to_ascii_lowercase().as_str(), "1" | "true" | "yes");
        } else if tok.starts_with("--") {
            return Err(format!("Unbekanntes Flag: {tok}"));
        } else {
            tokens.push(tok.to_ascii_lowercase());
        }
    }
    match tokens.as_slice() {
        [] => Ok(QuelleCommand {
            brain: None,
            spec: QuelleSpec::List,
            save,
        }),
        [a] if a == "list" => Ok(QuelleCommand {
            brain: None,
            spec: QuelleSpec::List,
            save,
        }),
        [brain] => Ok(QuelleCommand {
            brain: Some(normalize_brain(brain)?),
            spec: QuelleSpec::Show,
            save,
        }),
        [brain, spec] if spec == "list" => Ok(QuelleCommand {
            brain: Some(normalize_brain(brain)?),
            spec: QuelleSpec::List,
            save,
        }),
        [brain, source] => Ok(QuelleCommand {
            brain: Some(normalize_brain(brain)?),
            spec: QuelleSpec::Set(canonicalize_source(source)?),
            save,
        }),
        _ => Err("Nutzung: /quelle <brain> <quelle|list|default> [--save]".to_string()),
    }
}

pub fn catalog_entry(source: &str) -> ProviderSource {
    ProviderSource {
        source: source.to_string(),
        ..ProviderSource::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wa-src-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    fn write_catalog(dir: &Path, json: &str) -> PathBuf {
        let path = dir.join("providers.json");
        let mut f = fs::File::create(&path).unwrap();
        f.write_all(json.as_bytes()).unwrap();
        path
    }

    #[test]
    fn default_ist_browser_chat() {
        let scope = SourceScope::in_memory(ProvidersFile::default());
        let active = scope.active("claude");
        assert_eq!(active.source, BROWSER_SOURCE);
        assert_eq!(active.kind, SourceKind::Browser);
        assert!(active.persisted);
    }

    #[test]
    fn session_scope_ist_pro_instanz_isoliert() {
        let mut catalog = ProvidersFile {
            version: 1,
            brains: BTreeMap::new(),
        };
        catalog.brains.insert("claude".into(), catalog_entry("default"));
        let mut a = SourceScope::in_memory(catalog.clone());
        let mut b = SourceScope::in_memory(catalog);
        a.set_session("claude", "openrouter").unwrap();
        b.set_session("claude", "groq").unwrap();
        assert_eq!(a.active("claude").source, "openrouter");
        assert_eq!(b.active("claude").source, "groq");
        assert_eq!(a.active("claude").kind, SourceKind::Api);
        assert!(!a.active("claude").persisted);
        assert!(!b.active("claude").persisted);
    }

    #[test]
    fn persistiert_nur_mit_save() {
        let dir = temp_dir();
        let path = write_catalog(
            &dir,
            r#"{"version":1,"brains":{"claude":{"source":"default"}}}"#,
        );
        let mut scope = SourceScope::from_path(&path).unwrap();
        let without = scope
            .apply(&parse_quelle_args("claude openrouter").unwrap())
            .unwrap();
        assert!(!without.persisted);
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("default"));
        assert!(!on_disk.contains("openrouter"));
        assert_eq!(scope.active("claude").source, "openrouter");

        let with = scope
            .apply(&parse_quelle_args("claude groq --save").unwrap())
            .unwrap();
        assert!(with.persisted);
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("groq"));
        assert!(!on_disk.contains("openrouter"));
        let reloaded = SourceScope::from_path(&path).unwrap();
        assert_eq!(reloaded.active("claude").source, "groq");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_flag_am_anfang_oder_ende() {
        let a = parse_quelle_args("--save claude default").unwrap();
        let b = parse_quelle_args("claude default --save").unwrap();
        assert!(a.save && b.save);
        assert_eq!(a.spec, QuelleSpec::Set(BROWSER_SOURCE.into()));
        assert_eq!(b.spec, QuelleSpec::Set(BROWSER_SOURCE.into()));
    }

    #[test]
    fn kein_automatisches_routing_kein_versteckter_fallback() {
        let mut catalog = ProvidersFile {
            version: 1,
            brains: BTreeMap::new(),
        };
        catalog
            .brains
            .insert("claude".into(), catalog_entry("openrouter"));
        let mut scope = SourceScope::in_memory(catalog);
        let mut health = BTreeMap::new();
        health.insert("openrouter".into(), false);
        health.insert(BROWSER_SOURCE.into(), true);
        let active = scope.active_ignoring_health("claude", &health);
        assert_eq!(active.source, "openrouter");
        assert_eq!(active.kind, SourceKind::Api);
        assert_eq!(choose_without_fallback("openrouter", &health), "openrouter");
        scope.set_session("claude", "default").unwrap();
        assert_eq!(
            scope.active_ignoring_health("claude", &health).source,
            "default"
        );
        assert_eq!(
            scope.active_ignoring_health("claude", &health).kind,
            SourceKind::Browser
        );
        // DoD: health/availability never changes the chosen source.
    }

    #[test]
    fn quelle_list_und_show() {
        let mut catalog = ProvidersFile {
            version: 1,
            brains: BTreeMap::new(),
        };
        catalog
            .brains
            .insert("chatgpt".into(), catalog_entry("openrouter"));
        let scope = SourceScope::in_memory(catalog);
        let listed = scope.list(Some("chatgpt"));
        assert!(listed.iter().any(|s| s.id == "default"));
        assert!(listed.iter().any(|s| s.id == "openrouter" && s.active));
        let show = parse_quelle_args("chatgpt").unwrap();
        assert_eq!(show.spec, QuelleSpec::Show);
        let list = parse_quelle_args("chatgpt list").unwrap();
        assert_eq!(list.spec, QuelleSpec::List);
        let all = parse_quelle_args("").unwrap();
        assert_eq!(all.spec, QuelleSpec::List);
    }

    #[test]
    fn keys_bleiben_env_namen_beim_save() {
        let dir = temp_dir();
        let path = write_catalog(
            &dir,
            r#"{"version":1,"brains":{"chatgpt":{"source":"openrouter","api_key_env":"OPENROUTER_API_KEY","base_url_env":"OPENROUTER_BASE_URL"}}}"#,
        );
        let mut scope = SourceScope::from_path(&path).unwrap();
        scope
            .apply(&parse_quelle_args("chatgpt groq --save").unwrap())
            .unwrap();
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("OPENROUTER_API_KEY"));
        assert!(!on_disk.contains("sk-"));
        assert!(on_disk.contains("groq"));
        let _ = fs::remove_dir_all(&dir);
    }
}
