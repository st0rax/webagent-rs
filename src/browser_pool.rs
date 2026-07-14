//! Shared browser pool — ein WebView-Runtime, ein Tab pro Brain (Python `browser_pool.py`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
#[cfg(feature = "webview")]
use std::time::Duration;

#[cfg(feature = "webview")]
use crate::brain::BrainBackend;
use crate::browser::WebBrainBackend;
use crate::config::persist_browser_tabs;
#[cfg(feature = "webview")]
use crate::config::shared_profile_dir;
#[cfg(feature = "webview")]
use crate::webview_runtime::{WebViewPageDriver, WebViewRuntime};

struct PooledTab {
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    view_id: u64,
    #[cfg(feature = "webview")]
    driver_proto: WebViewPageDriver,
    refs: u32,
}

/// Prozessweiter Singleton: ein persistentes Profil, lazy Tab je `brain_id`.
pub struct BrowserPool {
    #[cfg(feature = "webview")]
    runtime: Option<WebViewRuntime>,
    tabs: HashMap<String, PooledTab>,
}

impl BrowserPool {
    fn new() -> Self {
        Self {
            #[cfg(feature = "webview")]
            runtime: None,
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
    pub fn start_brain(&mut self, backend: &WebBrainBackend, headless: bool) -> Result<(), String> {
        #[cfg(not(feature = "webview"))]
        {
            let _ = (backend, headless);
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

            let profile = shared_profile_dir();
            let mut driver = runtime
                .open_page(&profile, backend.brain_url(), headless)
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

    /// Gibt eine Referenz frei; schließt den Tab wenn letzte Ref und nicht persist.
    pub fn stop_brain(&mut self, brain_id: &str, persist: Option<bool>) -> Result<(), String> {
        let bid = brain_id.to_lowercase();
        let keep = persist.unwrap_or_else(persist_browser_tabs);
        let Some(tab) = self.tabs.get_mut(&bid) else {
            return Ok(());
        };
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
}
