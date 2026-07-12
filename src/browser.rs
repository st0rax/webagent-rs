//! browser — konkretes BrainBackend, das ein echtes Chromium über CDP steuert.
//!
//! Spiegelt `../src/webagent/brains/playwright_base.py`, ersetzt Playwright aber
//! durch den CDP-Client (`crate::cdp`). DOM-Operationen laufen über
//! `Runtime.evaluate`; Tastendrücke über `Input.dispatchKeyEvent`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::brain::{BrainBackend, BrainResponse, SessionState};
use crate::cdp::{CdpClient, ChromeProcess};
use crate::observer::{is_claude_limit_response_text, is_transient_response_text};
use crate::protocol::is_possibly_truncated;

const STABILITY_SECONDS: f64 = 1.5;

/// Web-Chat-Backend für einen Provider (chatgpt, claude, …).
pub struct WebBrainBackend {
    brain_id: String,
    url: String,
    selectors: Value,
    profile_dir: PathBuf,
    port: u16,
    process: RefCell<Option<ChromeProcess>>,
    client: RefCell<Option<CdpClient>>,
    baseline_count: Cell<i32>,
}

impl WebBrainBackend {
    /// Erstellt ein Backend aus der zentralen Brain-Konfiguration.
    pub fn from_config(brain_id: &str) -> Result<Self, String> {
        let brains = crate::config::brains();
        let spec = brains
            .get(brain_id)
            .ok_or_else(|| format!("Unbekanntes Brain: {brain_id}"))?;
        let url = spec.get("url").cloned().unwrap_or_default();
        let profile_dir = PathBuf::from(spec.get("profile_dir").cloned().unwrap_or_default());
        let selectors = crate::config::load_selectors(brain_id)
            .map_err(|e| format!("Selektoren nicht ladbar: {e}"))?;
        // Debug-Port deterministisch je Brain, um Kollisionen zu vermeiden.
        let port = 9222 + (stable_hash(brain_id) % 400) as u16;
        Ok(Self {
            brain_id: brain_id.to_string(),
            url,
            selectors,
            profile_dir,
            port,
            process: RefCell::new(None),
            client: RefCell::new(None),
            baseline_count: Cell::new(0),
        })
    }

    /// Selektor-Liste zu einem Schlüssel (leere Liste, wenn nicht vorhanden).
    fn sel(&self, key: &str) -> Vec<String> {
        self.selectors
            .get(key)
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str().map(|x| x.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// JS-Array-Literal aus einer Selektorliste (sicher escaped).
    fn js_selectors(list: &[String]) -> String {
        let items: Vec<String> = list
            .iter()
            .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()))
            .collect();
        format!("[{}]", items.join(","))
    }

    /// Führt ein JS im Seitenkontext aus (mit ausgeliehenem Client).
    fn eval(&self, expr: &str) -> Result<Value, String> {
        let mut guard = self.client.borrow_mut();
        let client = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        client.evaluate(expr).map_err(|e| e.to_string())
    }

    fn eval_bool(&self, expr: &str) -> bool {
        self.eval(expr).ok().and_then(|v| v.as_bool()).unwrap_or(false)
    }

    fn eval_i64(&self, expr: &str) -> i64 {
        self.eval(expr).ok().and_then(|v| v.as_i64()).unwrap_or(0)
    }

    fn eval_str(&self, expr: &str) -> String {
        self.eval(expr)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// Anzahl der Assistenten-Nachrichten (robust über die Selektorliste).
    fn assistant_count(&self) -> i32 {
        let mut sels = self.sel("assistant_message");
        if sels.is_empty() {
            sels = vec!["div.prose".to_string()];
        }
        let expr = format!(
            r#"(function(){{var S={};for(var i=0;i<S.length;i++){{var n=document.querySelectorAll(S[i]).length;if(n>0)return n;}}return 0;}})()"#,
            Self::js_selectors(&sels)
        );
        self.eval_i64(&expr) as i32
    }

    /// innerText der n-ten Assistenten-Nachricht.
    fn assistant_text(&self, index: i32) -> String {
        let mut sels = self.sel("assistant_message");
        if sels.is_empty() {
            sels = vec!["div.prose".to_string()];
        }
        let expr = format!(
            r#"(function(){{var S={};for(var i=0;i<S.length;i++){{var els=document.querySelectorAll(S[i]);if(els.length>{idx}){{return (els[{idx}].innerText||"").trim();}}}}return "";}})()"#,
            Self::js_selectors(&sels),
            idx = index
        );
        self.eval_str(&expr)
    }

    /// Ist mindestens ein Selektor aus der Liste im DOM sichtbar?
    fn any_visible(&self, key: &str) -> bool {
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let expr = format!(
            r#"(function(){{var S={};for(var i=0;i<S.length;i++){{var el=document.querySelector(S[i]);if(el){{var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}}}}return false;}})()"#,
            Self::js_selectors(&sels)
        );
        self.eval_bool(&expr)
    }

    /// Klickt das erste sichtbare Element aus der Selektorliste.
    fn click_first(&self, key: &str) -> bool {
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let expr = format!(
            r#"(function(){{var S={};for(var i=0;i<S.length;i++){{var el=document.querySelector(S[i]);if(el){{el.click();return true;}}}}return false;}})()"#,
            Self::js_selectors(&sels)
        );
        self.eval_bool(&expr)
    }

    fn is_cloudflare_blocked(&self) -> bool {
        let expr = r#"(function(){var u=location.href||"";if(u.indexOf("__cf_chl")>=0)return true;var t=(document.title||"").toLowerCase();return t.indexOf("just a moment")>=0||t.indexOf("nur einen moment")>=0;})()"#;
        self.eval_bool(expr)
    }

    fn dismiss_consent(&self) -> bool {
        self.click_first("consent_reject_button")
    }

    /// Enter im aktuell fokussierten Element auslösen (echtes Tastatur-Event via CDP).
    fn press_enter(&self) -> Result<(), String> {
        let mut guard = self.client.borrow_mut();
        let client = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        for phase in ["keyDown", "keyUp"] {
            client
                .call(
                    "Input.dispatchKeyEvent",
                    json!({
                        "type": phase,
                        "key": "Enter",
                        "code": "Enter",
                        "windowsVirtualKeyCode": 13,
                        "nativeVirtualKeyCode": 13
                    }),
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn stable_hash(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in s.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

impl BrainBackend for WebBrainBackend {
    fn brain_id(&self) -> &str {
        &self.brain_id
    }

    fn start(&mut self, headless: bool) -> Result<(), String> {
        let proc = ChromeProcess::launch(&self.profile_dir, headless, self.port)
            .map_err(|e| e.to_string())?;
        let ws_url = proc.page_ws_url().map_err(|e| e.to_string())?;
        let mut client = CdpClient::connect(&ws_url).map_err(|e| e.to_string())?;
        client
            .navigate(&self.url, Duration::from_secs(30))
            .map_err(|e| e.to_string())?;
        *self.process.borrow_mut() = Some(proc);
        *self.client.borrow_mut() = Some(client);
        self.baseline_count.set(0);
        Ok(())
    }

    fn stop(&mut self) -> Result<(), String> {
        *self.client.borrow_mut() = None;
        if let Some(mut proc) = self.process.borrow_mut().take() {
            proc.kill();
        }
        Ok(())
    }

    fn ensure_ready(&mut self, timeout: f64) -> Result<SessionState, String> {
        let start = Instant::now();
        let mut cf_count = 0;
        while start.elapsed().as_secs_f64() < timeout {
            self.dismiss_consent();
            let state = self.session_state();
            match state {
                SessionState::Cloudflare => {
                    cf_count += 1;
                    std::thread::sleep(Duration::from_secs_f64(
                        3.0 + (cf_count as f64 * 0.5).min(5.0),
                    ));
                    continue;
                }
                SessionState::Ready => return Ok(SessionState::Ready),
                _ => std::thread::sleep(Duration::from_millis(1500)),
            }
        }
        Ok(self.session_state())
    }

    fn session_state(&self) -> SessionState {
        if self.client.borrow().is_none() {
            return SessionState::Error;
        }
        if self.is_cloudflare_blocked() {
            return SessionState::Cloudflare;
        }
        if !self.is_logged_in() {
            return SessionState::LoginRequired;
        }
        SessionState::Ready
    }

    fn new_chat(&mut self) -> Result<(), String> {
        // Bevorzugt einen New-Chat-Button, sonst Navigation zur Start-URL.
        if self.click_first("new_chat_button") {
            std::thread::sleep(Duration::from_millis(800));
        } else {
            let url = self.url.clone();
            let mut guard = self.client.borrow_mut();
            let client = guard.as_mut().ok_or("Backend nicht gestartet")?;
            client
                .navigate(&url, Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
        }
        self.baseline_count.set(0);
        Ok(())
    }

    fn send(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.assistant_count();
        self.baseline_count.set(baseline);

        let sels = self.sel("composer");
        if sels.is_empty() {
            return Err("Keine Composer-Selektoren konfiguriert".into());
        }
        let text_json = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let expr = format!(
            r#"(function(){{var S={sels};var T={text};for(var i=0;i<S.length;i++){{var el=document.querySelector(S[i]);if(el){{el.focus();if(el.tagName==='TEXTAREA'||el.tagName==='INPUT'){{el.value=T;}}else{{el.textContent=T;}}el.dispatchEvent(new InputEvent('input',{{bubbles:true}}));return true;}}}}return false;}})()"#,
            sels = Self::js_selectors(&sels),
            text = text_json
        );
        if !self.eval_bool(&expr) {
            return Err("Composer-Feld nicht gefunden".into());
        }
        std::thread::sleep(Duration::from_millis(150));
        // Zuerst Enter versuchen; wenn ein Senden-Button existiert, zusätzlich klicken.
        self.press_enter().ok();
        if !self.sel("send_button").is_empty() {
            self.click_first("send_button");
        }
        Ok(baseline)
    }

    fn wait_response(&mut self, baseline_count: i32, timeout: f64) -> Result<BrainResponse, String> {
        let start = Instant::now();
        // Phase 1: auf neue Nachricht warten.
        let mut count = self.assistant_count();
        while count <= baseline_count && start.elapsed().as_secs_f64() < timeout {
            std::thread::sleep(Duration::from_millis(300));
            count = self.assistant_count();
        }
        if count <= baseline_count {
            return Ok(BrainResponse {
                text: String::new(),
                message_index: -1,
                generation_complete: false,
                backend_status: "timeout_no_message".into(),
                ..Default::default()
            });
        }

        // Phase 2: Text-Stabilität der Zielnachricht.
        let target = count - 1;
        let mut last_text = String::new();
        let mut stable_since = Instant::now();
        while start.elapsed().as_secs_f64() < timeout {
            let current = self.assistant_text(target);
            if is_claude_limit_response_text(&current) {
                return Ok(BrainResponse {
                    text: current,
                    message_index: target,
                    generation_complete: false,
                    backend_status: "rate_limit".into(),
                    ..Default::default()
                });
            }
            if current != last_text {
                last_text = current.clone();
                stable_since = Instant::now();
            } else if !current.is_empty()
                && !is_transient_response_text(&current)
                && !is_possibly_truncated(&current)
                && stable_since.elapsed().as_secs_f64() >= STABILITY_SECONDS
            {
                return Ok(BrainResponse {
                    text: current,
                    message_index: target,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    ..Default::default()
                });
            }
            // Schnellpfad: Protokoll-JSON bereits vollständig erkennbar.
            if current.replace(' ', "").contains("\"protocol\":\"webagent/1\"")
                && !is_possibly_truncated(&current)
            {
                return Ok(BrainResponse {
                    text: current,
                    message_index: target,
                    generation_complete: true,
                    backend_status: "ok".into(),
                    ..Default::default()
                });
            }
            std::thread::sleep(Duration::from_millis(300));
        }

        // Timeout: letzten bekannten Text unvollständig zurückgeben.
        Ok(BrainResponse {
            text: last_text,
            message_index: target,
            generation_complete: false,
            backend_status: "timeout_or_incomplete".into(),
            ..Default::default()
        })
    }

    fn is_logged_in(&self) -> bool {
        if self.client.borrow().is_none() {
            return false;
        }
        // Positive Signale: Composer / New-Chat / Login-Indikator sichtbar.
        if self.any_visible("login_indicator")
            || self.any_visible("composer")
            || self.any_visible("new_chat_button")
        {
            return true;
        }
        // Sonst: expliziter Login-Button => nicht eingeloggt; ohne jedes Signal
        // konservativ als nicht eingeloggt behandeln.
        false
    }

    fn click_login(&mut self) -> Result<(), String> {
        if self.click_first("login_button") {
            Ok(())
        } else {
            Err("Anmelden-Button nicht gefunden".into())
        }
    }

    fn wait_for_login(&mut self, poll_interval: f64) -> Result<(), String> {
        while !self.is_logged_in() {
            std::thread::sleep(Duration::from_secs_f64(poll_interval.max(0.5)));
        }
        Ok(())
    }

    fn get_conversation_ref(&self) -> Option<String> {
        let mut guard = self.client.borrow_mut();
        let client = guard.as_mut()?;
        let url = client.current_url().ok()?;
        let url = url.trim();
        if url.is_empty() || url == "about:blank" {
            None
        } else {
            Some(url.to_string())
        }
    }

    fn restore_conversation(&mut self, reference: &str) -> Result<bool, String> {
        if reference.is_empty() {
            return Ok(false);
        }
        let mut guard = self.client.borrow_mut();
        let client = match guard.as_mut() {
            Some(c) => c,
            None => return Ok(false),
        };
        match client.navigate(reference, Duration::from_secs(30)) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_config_loads_selectors_and_url() {
        let backend = WebBrainBackend::from_config("chatgpt").expect("chatgpt config");
        assert_eq!(backend.brain_id(), "chatgpt");
        assert_eq!(backend.url, "https://chatgpt.com/");
        assert!(!backend.sel("composer").is_empty());
    }

    #[test]
    fn all_configured_brains_have_selectors() {
        for (id, _url) in crate::config::BRAIN_TABLE {
            let backend =
                WebBrainBackend::from_config(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(
                !backend.sel("composer").is_empty(),
                "{id}: composer-Selektoren fehlen"
            );
            assert!(
                !backend.sel("assistant_message").is_empty(),
                "{id}: assistant_message-Selektoren fehlen"
            );
        }
    }

    #[test]
    fn js_selectors_escapes_quotes() {
        let js = WebBrainBackend::js_selectors(&["a.b".to_string(), "c\"d".to_string()]);
        assert!(js.starts_with('[') && js.ends_with(']'));
        assert!(js.contains("\"a.b\""));
    }

    #[test]
    fn unknown_brain_errors() {
        assert!(WebBrainBackend::from_config("does_not_exist").is_err());
    }

    #[test]
    fn session_state_error_without_client() {
        let backend = WebBrainBackend::from_config("claude").unwrap();
        assert_eq!(backend.session_state(), SessionState::Error);
    }
}
