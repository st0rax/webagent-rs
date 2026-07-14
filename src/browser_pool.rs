//! Shared browser pool — ein Chromium, ein Tab pro Brain (Python `browser_pool.py`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use crate::brain::BrainBackend;
use crate::browser::WebBrainBackend;
use crate::cdp::{CdpClient, ChromeProcess};
use crate::config::{persist_browser_tabs, shared_debug_port, shared_profile_dir};

struct PooledTab {
    target_id: String,
    ws_url: String,
    refs: u32,
}

/// Prozessweiter Singleton: ein persistentes Profil, lazy Tab je `brain_id`.
pub struct BrowserPool {
    process: Option<ChromeProcess>,
    tabs: HashMap<String, PooledTab>,
}

impl BrowserPool {
    fn new() -> Self {
        Self {
            process: None,
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

    /// Startet oder reaktiviert das Brain-Tab und hängt den Client ans Backend.
    pub fn start_brain(&mut self, backend: &WebBrainBackend, headless: bool) -> Result<(), String> {
        let brain_id = backend.brain_id().to_lowercase();
        self.ensure_process(headless)?;

        let proc = self.process.as_ref().ok_or("Shared-Browser nicht gestartet")?;
        let tab = if let Some(tab) = self.tabs.get_mut(&brain_id) {
            tab.refs = tab.refs.saturating_add(1);
            tab
        } else {
            let url = backend.brain_url();
            let (target_id, ws_url) = proc
                .new_page_target(url)
                .map_err(|e| e.to_string())?;
            self.tabs.insert(
                brain_id.clone(),
                PooledTab {
                    target_id,
                    ws_url: ws_url.clone(),
                    refs: 1,
                },
            );
            self.tabs.get_mut(&brain_id).unwrap()
        };

        let mut client = CdpClient::connect(&tab.ws_url).map_err(|e| e.to_string())?;
        client
            .navigate(backend.brain_url(), Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        backend.attach_pooled_client(client);
        Ok(())
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
        let target_id = tab.target_id.clone();
        self.tabs.remove(&bid);
        if let Some(proc) = self.process.as_ref() {
            let _ = proc.close_target(&target_id);
        }
        if self.tabs.is_empty() {
            self.teardown_process();
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

    fn ensure_process(&mut self, headless: bool) -> Result<(), String> {
        if self.process.is_some() {
            return Ok(());
        }
        let profile = shared_profile_dir();
        let port = shared_debug_port();
        let proc = ChromeProcess::launch(&profile, headless, port).map_err(|e| e.to_string())?;
        self.process = Some(proc);
        Ok(())
    }

    fn teardown_process(&mut self) {
        self.process.take();
    }

    #[cfg(test)]
    fn shutdown_force(&mut self) {
        for (bid, tab) in self.tabs.drain() {
            if let Some(proc) = self.process.as_ref() {
                let _ = proc.close_target(&tab.target_id);
            }
            let _ = bid;
        }
        self.teardown_process();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_refcount_without_chrome() {
        BrowserPool::reset_for_tests();
        let mut pool = BrowserPool::new();
        pool.tabs.insert(
            "chatgpt".to_string(),
            PooledTab {
                target_id: "t1".into(),
                ws_url: "ws://127.0.0.1:9222/devtools/page/1".into(),
                refs: 1,
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
                target_id: "t2".into(),
                ws_url: "ws://127.0.0.1:9222/devtools/page/2".into(),
                refs: 1,
            },
        );
        pool.detach_brain("claude").unwrap();
        assert!(pool.has_tab("claude"));
        assert_eq!(pool.tab_ref_count("claude"), 0);
    }
}