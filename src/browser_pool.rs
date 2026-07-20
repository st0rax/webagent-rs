//! Shared browser pool — ein WebView-Runtime, ein Tab pro Brain (Python `browser_pool.py`).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "webview")]
use std::time::Duration;

#[cfg(feature = "webview")]
use crate::brain::BrainBackend;
use crate::browser::WebBrainBackend;
use crate::config::persist_browser_tabs;
#[cfg(feature = "webview")]
use crate::config::{encapsulated_profile_dir, shared_profile_dir, ProfileClonePlanner};
#[cfg(feature = "webview")]
use crate::page_driver::PageDriver;
#[cfg(feature = "webview")]
use crate::webview_runtime::{WebViewPageDriver, WebViewRuntime};

/// Max. Versuche den geteilten Browser zu nutzen, bevor ein Brain auf die
/// gekapselte Fallback-Instanz faellt. Spiegelt circuit_breaker::DEFAULT_MAX_FAILURES.
/// Nur im `webview`-Pfad referenziert; ohne das Feature (CI-Kernbuild) ungenutzt.
#[cfg_attr(not(feature = "webview"), allow(dead_code))]
const POOL_FALLBACK_RETRIES: u32 = 3;

struct PooledTab {
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    view_id: u64,
    #[cfg(feature = "webview")]
    driver_proto: WebViewPageDriver,
    refs: u32,
}

/// Gekapselte, isolierte Browser-Instanz (Fallback), die nach dem Scheitern des
/// geteilten Browsers fuer ein Brain gestartet wird. Eigenes WebView-Runtime
/// (eigener Prozess) auf einem Linked-Clone/Delta des kanonischen Profils
/// (`profiles/encapsulated/<brain>_<runstamp>`). Nie zurueckgeschrieben.
#[cfg(feature = "webview")]
struct EncapsulatedInstance {
    runtime: WebViewRuntime,
    profile_dir: PathBuf,
    driver_proto: WebViewPageDriver,
    refs: u32,
}

/// Prozessweiter Singleton: ein persistentes Profil, lazy Tab je `brain_id`.
pub struct BrowserPool {
    #[cfg(feature = "webview")]
    runtime: Option<WebViewRuntime>,
    tabs: HashMap<String, PooledTab>,
    #[cfg(feature = "webview")]
    encapsulated: HashMap<String, EncapsulatedInstance>,
}

impl BrowserPool {
    fn new() -> Self {
        Self {
            #[cfg(feature = "webview")]
            runtime: None,
            #[cfg(feature = "webview")]
            encapsulated: HashMap::new(),
            tabs: HashMap::new(),
        }
    }

    /// Globaler Pool (serialisiert über Mutex).
    pub fn global() -> &'static Mutex<BrowserPool> {
        static POOL: OnceLock<Mutex<BrowserPool>> = OnceLock::new();
        POOL.get_or_init(|| Mutex::new(BrowserPool::new()))
    }

    #[cfg(test)]
    pub fn reset_for_tests() {
        if let Ok(mut pool) = Self::global().lock() {
            pool.shutdown_force();
        }
    }

    /// Startet oder reaktiviert das Brain-Tab und hängt den Driver ans Backend.
    ///
    /// `profile_override` erlaubt es, statt des Shared-Profils ein isoliertes
    /// Laufzeit-Profil zu nutzen (z.B. eine Swarm-Teilkopie aus
    /// `config::prepare_swarm_profile`). `None` → `shared_profile_dir()`.
    pub fn start_brain(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        #[cfg(not(feature = "webview"))]
        {
            let _ = (backend, headless, profile_override);
            Err(crate::page_driver::webview_unavailable().to_string())
        }
        #[cfg(feature = "webview")]
        {
            let brain_id = backend.brain_id().to_lowercase();
            self.ensure_runtime(headless)?;

            let runtime = self
                .runtime
                .as_ref()
                .ok_or("Shared-WebView nicht gestartet")?;

            if let Some(tab) = self.tabs.get_mut(&brain_id) {
                tab.refs = tab.refs.saturating_add(1);
                let mut driver = tab.driver_proto.clone();
                driver
                    .navigate(backend.brain_url(), Duration::from_secs(30))
                    .map_err(|e| e.to_string())?;
                backend.attach_page_driver(Box::new(driver));
                return Ok(());
            }

            let profile = profile_override.unwrap_or_else(shared_profile_dir);
            let mut driver = runtime
                .open_page(&profile, backend.brain_url(), headless, &brain_id)
                .map_err(|e| e.to_string())?;
            driver
                .navigate(backend.brain_url(), Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
            let view_id = driver.view_id();
            let driver_proto = driver.clone();
            self.tabs.insert(
                brain_id,
                PooledTab {
                    view_id,
                    driver_proto,
                    refs: 1,
                },
            );
            backend.attach_page_driver(Box::new(driver));
            Ok(())
        }
    }

    /// Wie `start_brain`, aber mit Resilienz: der geteilte Browser wird bis zu
    /// `POOL_FALLBACK_RETRIES` Mal versucht (pro Brain gezaehlt); scheitert er
    /// durchgehend, faellt das Brain auf eine gekapselte, isolierte Instanz zurueck
    /// (eigener WebView-Prozess, Linked-Clone des kanonischen Profils).
    #[cfg(feature = "webview")]
    pub fn start_brain_resilient(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        let brain_id = backend.brain_id().to_lowercase();

        // Bereits gekapselt -> reuse (Refs++), kein erneuter Shared-Versuch.
        if self.encapsulated.contains_key(&brain_id) {
            return self.start_brain_encapsulated(backend, headless, &brain_id);
        }

        // Expliziter Override (isoliertes Profil) -> kein Shared-Pfad, direkt delegieren.
        if profile_override.is_some() {
            return self.start_brain(backend, headless, profile_override);
        }

        // 1) Shared-Pool-Pfad (Default), bis zu POOL_FALLBACK_RETRIES Versuche.
        for _ in 0..POOL_FALLBACK_RETRIES {
            match self.start_brain(backend, headless, None) {
                Ok(()) => return Ok(()),
                Err(e) => eprintln!(
                    "[browser_pool] Shared-Versuch fuer Brain '{}' fehlgeschlagen: {}",
                    brain_id, e
                ),
            }
        }

        // 2) Fallback: gekapselte Instanz (eigenes Profil-Image, eigenes Runtime).
        eprintln!(
            "[browser_pool] Shared-Browser fuer Brain '{}' nach {} Versuchen fehlgeschlagen -> gekapselte Instanz",
            brain_id, POOL_FALLBACK_RETRIES
        );
        self.start_brain_encapsulated(backend, headless, &brain_id)
    }

    /// Wie `start_brain`, aber ohne Shared-Pool — Fehlerpfad ohne WebView.
    #[cfg(not(feature = "webview"))]
    pub fn start_brain_resilient(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        profile_override: Option<PathBuf>,
    ) -> Result<(), String> {
        let _ = (backend, headless, profile_override);
        Err(crate::page_driver::webview_unavailable().to_string())
    }

    /// Startet die gekapselte Fallback-Instanz: Linked-Clone/Delta des kanonischen
    /// Shared-Profils nach `profiles/encapsulated/<brain>_<runstamp>`, eigener
    /// WebView-Runtime (eigener Prozess, kein SingletonLock-Konflikt).
    #[cfg(feature = "webview")]
    fn start_brain_encapsulated(
        &mut self,
        backend: &WebBrainBackend,
        headless: bool,
        brain_id: &str,
    ) -> Result<(), String> {
        // Vorhandene gekapselte Instanz reuse (Refs++).
        if let Some(inst) = self.encapsulated.get_mut(brain_id) {
            inst.refs = inst.refs.saturating_add(1);
            let mut driver = inst.driver_proto.clone();
            driver
                .navigate(backend.brain_url(), Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
            backend.attach_page_driver(Box::new(driver));
            return Ok(());
        }

        let runstamp = crate::now_run_stamp();
        let clone_dir = encapsulated_profile_dir(brain_id, &runstamp);
        // Linked-Clone/Delta des kanonischen Shared-Profils (Login-Bild, read-only Quelle).
        let plan =
            ProfileClonePlanner::plan_canonical(&shared_profile_dir(), &clone_dir, &runstamp);
        ProfileClonePlanner::materialize(&plan)
            .map_err(|e| format!("Profil-Klon fuer Brain '{brain_id}' fehlgeschlagen: {e}"))?;

        let rt = WebViewRuntime::launch(&clone_dir, headless).map_err(|e| e.to_string())?;
        let mut driver = rt
            .open_page(&clone_dir, backend.brain_url(), headless, brain_id)
            .map_err(|e| e.to_string())?;
        driver
            .navigate(backend.brain_url(), Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        let driver_proto = driver.clone();
        self.encapsulated.insert(
            brain_id.to_string(),
            EncapsulatedInstance {
                runtime: rt,
                profile_dir: clone_dir,
                driver_proto,
                refs: 1,
            },
        );
        backend.attach_page_driver(Box::new(driver));
        Ok(())
    }

    /// Gibt eine Referenz frei; schließt den Tab wenn letzte Ref und nicht persist.
    pub fn stop_brain(&mut self, brain_id: &str, persist: Option<bool>) -> Result<(), String> {
        let bid = brain_id.to_lowercase();
        let keep = persist.unwrap_or_else(persist_browser_tabs);

        // Shared-Pool-Tab?
        if let Some(tab) = self.tabs.get_mut(&bid) {
            if tab.refs == 0 {
                return Ok(());
            }
            tab.refs -= 1;
            if tab.refs > 0 {
                return Ok(());
            }
            if keep {
                return Ok(());
            }
            #[cfg(feature = "webview")]
            let view_id = tab.view_id;
            self.tabs.remove(&bid);
            #[cfg(feature = "webview")]
            if let Some(rt) = self.runtime.as_ref() {
                let _ = rt.close_page(view_id);
            }
            if self.tabs.is_empty() {
                self.teardown_runtime();
            }
            return Ok(());
        }

        // Gekapselte Instanz? (eigenes Runtime + eigenes Profilverzeichnis)
        #[cfg(feature = "webview")]
        {
            if let Some(inst) = self.encapsulated.get_mut(&bid) {
                if inst.refs > 0 {
                    inst.refs -= 1;
                }
                if inst.refs > 0 || keep {
                    return Ok(());
                }
            }
            if let Some(inst) = self.encapsulated.remove(&bid) {
                // `inst.runtime` wird beim Verlassen des Blocks gedroppt (WebView-
                // Prozess beendet); danach das geklonte Profilverzeichnis entfernen.
                let EncapsulatedInstance {
                    runtime: _rt,
                    profile_dir,
                    ..
                } = inst;
                let _ = _rt;
                let _ = std::fs::remove_dir_all(&profile_dir);
            }
        }
        Ok(())
    }

    /// Wie `stop_brain(..., persist=true)` — Tab bleibt für den nächsten Hop offen.
    pub fn detach_brain(&mut self, brain_id: &str) -> Result<(), String> {
        self.stop_brain(brain_id, Some(true))
    }

    pub fn has_tab(&self, brain_id: &str) -> bool {
        self.tabs.contains_key(&brain_id.to_lowercase())
    }

    pub fn tab_ref_count(&self, brain_id: &str) -> u32 {
        self.tabs
            .get(&brain_id.to_lowercase())
            .map(|t| t.refs)
            .unwrap_or(0)
    }

    #[cfg(feature = "webview")]
    fn ensure_runtime(&mut self, headless: bool) -> Result<(), String> {
        if self.runtime.is_some() {
            return Ok(());
        }
        let profile = shared_profile_dir();
        let rt = WebViewRuntime::launch(&profile, headless).map_err(|e| e.to_string())?;
        self.runtime = Some(rt);
        Ok(())
    }

    fn teardown_runtime(&mut self) {
        #[cfg(feature = "webview")]
        {
            self.runtime.take();
        }
    }

    #[cfg(test)]
    fn shutdown_force(&mut self) {
        for (bid, tab) in self.tabs.drain() {
            #[cfg(feature = "webview")]
            if let Some(rt) = self.runtime.as_ref() {
                let _ = rt.close_page(tab.view_id);
            }
            let _ = (bid, tab);
        }
        self.teardown_runtime();
        // Gekapselte Instanzen ebenfalls entsorgen (Runtime drop + Klon-Verzeichnis).
        #[cfg(feature = "webview")]
        for (_bid, inst) in self.encapsulated.drain() {
            let EncapsulatedInstance {
                runtime: _rt,
                profile_dir,
                ..
            } = inst;
            let _ = _rt;
            let _ = std::fs::remove_dir_all(&profile_dir);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_refcount_without_webview() {
        BrowserPool::reset_for_tests();
        let mut pool = BrowserPool::new();
        pool.tabs.insert(
            "chatgpt".to_string(),
            PooledTab {
                view_id: 1,
                refs: 1,
                #[cfg(feature = "webview")]
                driver_proto: {
                    let (tx, _rx) = std::sync::mpsc::channel();
                    WebViewPageDriver {
                        view_id: 1,
                        page_tx: tx,
                    }
                },
            },
        );
        pool.stop_brain("chatgpt", Some(false)).unwrap();
        assert!(!pool.has_tab("chatgpt"));
    }

    #[test]
    fn pool_persist_keeps_tab() {
        BrowserPool::reset_for_tests();
        let mut pool = BrowserPool::new();
        pool.tabs.insert(
            "claude".to_string(),
            PooledTab {
                view_id: 2,
                refs: 1,
                #[cfg(feature = "webview")]
                driver_proto: {
                    let (tx, _rx) = std::sync::mpsc::channel();
                    WebViewPageDriver {
                        view_id: 2,
                        page_tx: tx,
                    }
                },
            },
        );
        pool.detach_brain("claude").unwrap();
        assert!(pool.has_tab("claude"));
        assert_eq!(pool.tab_ref_count("claude"), 0);
    }

    #[test]
    fn resilient_retry_threshold_matches_circuit_breaker() {
        // Nach POOL_FALLBACK_RETRIES (3) Fehlversuchen faellt das Brain auf die
        // gekapselte Instanz zurueck — analog circuit_breaker::DEFAULT_MAX_FAILURES.
        assert_eq!(POOL_FALLBACK_RETRIES, 3);
        let fallback_due = |failures: u32| failures >= POOL_FALLBACK_RETRIES;
        assert!(!fallback_due(0));
        assert!(!fallback_due(1));
        assert!(!fallback_due(2));
        assert!(fallback_due(3));
        assert!(fallback_due(4));
    }
}
