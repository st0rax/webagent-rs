//! browser — konkretes BrainBackend, das ein echtes Chromium über CDP steuert.
//!
//! Spiegelt `../src/webagent/brains/playwright_base.py`, ersetzt Playwright aber
//! durch den CDP-Client (`crate::cdp`). DOM-Operationen laufen über
//! `Runtime.evaluate`; Tastendrücke über `Input.dispatchKeyEvent`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::brain::{BrainBackend, BrainResponse, SessionState};
use crate::cdp::{CdpClient, ChromeProcess};
use crate::observer::{is_claude_limit_response_text, is_transient_response_text};
use crate::protocol::is_possibly_truncated;

const STABILITY_SECONDS: f64 = 1.5;

/// Ergebnis der Vollständigkeitsprüfung einer laufenden Antwort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Completion {
    /// Antwort ist vollständig — zurückgeben.
    Complete,
    /// Noch nicht fertig — weiter beobachten.
    Continue,
    /// Rate-Limit/Usage-Banner statt echter Antwort.
    RateLimited,
}

/// Reine, testbare Entscheidung, ob eine Antwort vollständig ist.
///
/// Autoritatives Fertigsignal ist das Verschwinden des Stop-/Generating-Buttons
/// (bzw. ein bereits vollständiges Protokoll-Dokument). Reine Textstabilität ist
/// nur der Fallback für UIs ohne erkennbaren Stop-Button. Damit werden die zwei
/// Hauptprobleme adressiert: (a) Timeout mitten im Stream (wir warten, solange der
/// Stop-Button sichtbar ist) und (b) fälschlich „unvollständig" (ein vollständiges
/// JSON gilt sofort als fertig, unabhängig vom Stabilitätsfenster).
fn classify_completion(
    text: &str,
    has_stop_selectors: bool,
    stop_seen_ever: bool,
    stop_visible: bool,
    stable_secs: f64,
) -> Completion {
    if is_claude_limit_response_text(text) {
        return Completion::RateLimited;
    }

    let text_ready = !text.trim().is_empty()
        && !is_transient_response_text(text)
        && !is_possibly_truncated(text);
    if !text_ready {
        return Completion::Continue;
    }

    // Ein vollständig geparstes Protokoll-Dokument ist immer fertig — auch wenn
    // der Stop-Button (durch Polling-Timing) noch kurz sichtbar wirkt.
    if crate::protocol::parse(text).valid {
        return Completion::Complete;
    }

    if has_stop_selectors {
        if stop_seen_ever && !stop_visible {
            // Generierung war aktiv und ist nun beendet.
            Completion::Complete
        } else if !stop_seen_ever && stable_secs >= STABILITY_SECONDS * 1.5 {
            // Stop-Button wurde nie erfasst (sehr schnelle Antwort) — nach etwas
            // längerer Stabilität dennoch als fertig werten, statt zu blockieren.
            Completion::Complete
        } else {
            Completion::Continue
        }
    } else if stable_secs >= STABILITY_SECONDS {
        Completion::Complete
    } else {
        Completion::Continue
    }
}

/// Web-Chat-Backend für einen Provider (chatgpt, claude, …).
/// Ergebnis einer Live-Diagnose (echter Browser gegen die Provider-Seite).
#[derive(Debug, Clone)]
pub struct LiveDiagnosis {
    pub brain_id: String,
    pub url: String,
    pub cloudflare: bool,
    pub logged_in: bool,
    pub composer_found: bool,
    pub assistant_count: i32,
    pub session_state: SessionState,
}

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
        // Debug-Port deterministisch je Brain (zentral in config, env-überschreibbar).
        let port = crate::config::debug_port(brain_id);
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

    /// JS-Array-Literal der Selektoren zu einem Schlüssel; `fallback` greift,
    /// wenn keine konfiguriert sind.
    fn sel_js(&self, key: &str, fallback: &[&str]) -> String {
        let mut sels = self.sel(key);
        if sels.is_empty() {
            sels = fallback.iter().map(|s| s.to_string()).collect();
        }
        Self::js_selectors(&sels)
    }

    /// Baut ein IIFE, das die Selektorliste `list_js` durchläuft und `body` auf
    /// jeden Selektor `S[i]` anwendet; liefert `default`, wenn nichts matcht.
    ///
    /// Jeder Selektor läuft in einem eigenen try/catch: ungültige (z.B.
    /// Playwright-`:has-text()`) Selektoren werfen bei `querySelector*` und
    /// dürfen NICHT die restliche Liste abbrechen — genau das hat frueher die
    /// komplette Erkennung lahmgelegt.
    fn js_scan(list_js: &str, body: &str, default: &str) -> String {
        format!("(function(){{var S={list_js};for(var i=0;i<S.length;i++){{try{{{body}}}catch(e){{}}}}return {default};}})()")
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
        let list = self.sel_js("assistant_message", &["div.prose"]);
        let expr = Self::js_scan(
            &list,
            "var n=document.querySelectorAll(S[i]).length;if(n>0)return n;",
            "0",
        );
        self.eval_i64(&expr) as i32
    }

    /// innerText der n-ten Assistenten-Nachricht.
    fn assistant_text(&self, index: i32) -> String {
        let list = self.sel_js("assistant_message", &["div.prose"]);
        let body = format!(
            "var els=document.querySelectorAll(S[i]);if(els.length>{idx}){{return (els[{idx}].innerText||\"\").trim();}}",
            idx = index
        );
        self.eval_str(&Self::js_scan(&list, &body, "\"\""))
    }

    /// Ist mindestens ein Selektor aus der Liste im DOM sichtbar?
    fn any_visible(&self, key: &str) -> bool {
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let expr = Self::js_scan(
            &Self::js_selectors(&sels),
            "var el=document.querySelector(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}",
            "false",
        );
        self.eval_bool(&expr)
    }

    /// Klickt das erste sichtbare Element aus der Selektorliste.
    fn click_first(&self, key: &str) -> bool {
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let expr = Self::js_scan(
            &Self::js_selectors(&sels),
            "var el=document.querySelector(S[i]);if(el){el.click();return true;}",
            "false",
        );
        self.eval_bool(&expr)
    }

    /// Ein einziger CDP-Roundtrip, der Nachrichtenanzahl, den Text der Nachricht
    /// `target` und die Sichtbarkeit des Stop-Buttons gemeinsam ermittelt — statt
    /// dreier separater `Runtime.evaluate`-Aufrufe pro Poll-Iteration.
    fn probe_generation(&self, assistant_js: &str, stop_js: &str, target: i32) -> (i32, String, bool) {
        let expr = format!(
            r#"(function(){{
var A={assistant_js};var count=0,els=null;
for(var i=0;i<A.length;i++){{try{{var e=document.querySelectorAll(A[i]);if(e.length>0){{count=e.length;els=e;break;}}}}catch(x){{}}}}
var text="";if(els&&{target}>=0&&els.length>{target}){{text=(els[{target}].innerText||"").trim();}}
var stop=false;var S={stop_js};
for(var j=0;j<S.length;j++){{try{{var el=document.querySelector(S[j]);if(el){{var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){{stop=true;break;}}}}}}catch(x){{}}}}
return {{count:count,text:text,stop:stop}};}})()"#,
            assistant_js = assistant_js,
            stop_js = stop_js,
            target = target
        );
        match self.eval(&expr) {
            Ok(v) => (
                v.get("count").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                v.get("stop").and_then(|x| x.as_bool()).unwrap_or(false),
            ),
            Err(_) => (0, String::new(), false),
        }
    }

    /// Diagnose des echten DOM: wie viele Elemente matchen die konfigurierten
    /// Selektoren, welche Buttons/Kandidaten-Container gibt es? Deckt Selektor-Drift
    /// auf (der Hauptgrund, warum die Antworterkennung eine fertige Nachricht
    /// "nicht sieht").
    pub fn dom_report(&self) -> Result<Value, String> {
        let keys = [
            "composer",
            "assistant_message",
            "stop_button",
            "send_button",
            "new_chat_button",
            "login_indicator",
        ];
        let mut counts_js = String::from("var counts={};");
        for k in keys {
            let list = Self::js_selectors(&self.sel(k));
            counts_js.push_str(&format!(
                "counts[{k:?}]=(function(){{var S={list};var n=0;for(var i=0;i<S.length;i++){{try{{n+=document.querySelectorAll(S[i]).length;}}catch(e){{}}}}return n;}})();"
            ));
        }
        let expr = format!(
            r#"(function(){{
{counts_js}
function inf(el){{var t=(el.innerText||'').trim();return {{tag:el.tagName,cls:(el.className||'').toString().slice(0,90),al:el.getAttribute('aria-label')||'',ti:el.getAttribute('title')||'',dt:el.getAttribute('data-testid')||'',svg:!!el.querySelector('svg'),tl:t.length,tp:t.slice(0,50)}};}}
var btns=[];document.querySelectorAll('button').forEach(function(b){{btns.push(inf(b));}});
var msgs=[];document.querySelectorAll('[class*=message]').forEach(function(m){{msgs.push(inf(m));}});
var cand=[];['[data-message-author-role]','[data-testid]','.markdown','[class*=markdown]','[class*=message]','[class*=assistant]','[class*=chat]','div.prose','[class*=answer]','[class*=response]','[class*=bubble]'].forEach(function(s){{try{{var n=document.querySelectorAll(s).length;if(n>0)cand.push({{sel:s,n:n}});}}catch(e){{}}}});
var tb=[];document.querySelectorAll('div,p,article,section,li').forEach(function(e){{var t=(e.innerText||'').trim();if(t.length<40)return;var cm=0;for(var k=0;k<e.children.length;k++){{var ct=(e.children[k].innerText||'').length;if(ct>cm)cm=ct;}}if(cm<t.length*0.75){{tb.push(inf(e));}}}});tb.sort(function(a,b){{return b.tl-a.tl;}});
return {{url:location.href,title:document.title,w:window.innerWidth,h:window.innerHeight,wd:navigator.webdriver,ua:(navigator.userAgent||'').slice(0,90),counts:counts,buttons:btns.slice(0,60),messages:msgs.slice(0,20),candidates:cand,textblocks:tb.slice(0,8)}};
}})()"#
        );
        self.eval(&expr)
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
        client.press_key("Enter", "Enter", 13, "\r").map_err(|e| e.to_string())
    }

    /// Setzt den Text in den Composer (fokussiert, `value`/`textContent`, feuert
    /// `input`). Gibt true, wenn ein Composer gefunden wurde.
    fn fill_composer(&self, composer_js: &str, text: &str) -> bool {
        // 1) Mittelpunkt-Koordinaten des Composers holen (nicht gefunden -> false).
        let coord_body = "var el=document.querySelector(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}";
        let coords = self
            .eval(&Self::js_scan(composer_js, coord_body, "null"))
            .unwrap_or(Value::Null);
        let (x, y) = match (
            coords.get("x").and_then(|v| v.as_f64()),
            coords.get("y").and_then(|v| v.as_f64()),
        ) {
            (Some(x), Some(y)) => (x, y),
            _ => return false,
        };
        // 2) Echter Mausklick auf den Composer (Fokus), dann leeren.
        {
            let mut guard = self.client.borrow_mut();
            if let Some(client) = guard.as_mut() {
                let _ = client.click_at(x, y);
            }
        }
        std::thread::sleep(Duration::from_millis(80));
        let clear_body = "var el=document.querySelector(S[i]);if(el){el.focus();try{if('value' in el){el.value='';}else{el.textContent='';}el.dispatchEvent(new InputEvent('input',{bubbles:true}));}catch(e){}return true;}";
        let _ = self.eval_bool(&Self::js_scan(composer_js, clear_body, "false"));
        // 3) Echt tippen via CDP Input.insertText.
        {
            let mut guard = self.client.borrow_mut();
            if let Some(client) = guard.as_mut() {
                let _ = client.insert_text(text);
            }
        }
        // 4) Fallback: nur falls der Composer weiterhin leer ist, .value setzen.
        let t = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let set_body = format!(
            "var el=document.querySelector(S[i]);if(el){{var cur=('value' in el)?(el.value||''):(el.textContent||'');if(cur.trim().length===0){{if('value' in el){{el.value={t};}}else{{el.textContent={t};}}el.dispatchEvent(new InputEvent('input',{{bubbles:true}}));}}return true;}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &set_body, "false"))
    }

    /// True, wenn der Composer leer ist (Indiz, dass die Nachricht abgeschickt und
    /// das Feld geleert wurde). False auch, wenn kein Composer gefunden wird.
    fn composer_is_empty(&self, composer_js: &str) -> bool {
        let body = "var el=document.querySelector(S[i]);if(el){var v=(el.value!==undefined&&el.value!==null?el.value:el.textContent)||'';return v.trim().length===0;}";
        self.eval_bool(&Self::js_scan(composer_js, body, "false"))
    }

    /// Öffnet ein **sichtbares** Chromium auf der Brain-URL und wartet, bis der
    /// Nutzer eingeloggt ist. Es werden **keine Zugangsdaten eingegeben** — die
    /// Anmeldung macht der Nutzer selbst im Fenster; diese Methode pollt nur den
    /// Login-Zustand und schließt danach sauber (damit Chrome die Session ins
    /// persistente Profil schreibt). Gibt `true`, wenn Login erkannt wurde.
    pub fn interactive_login(&mut self, timeout: Duration) -> Result<bool, String> {
        self.start(false)?; // headed — Login erfordert Nutzerinteraktion
        let start = Instant::now();
        if self.is_logged_in() {
            std::thread::sleep(Duration::from_secs(1));
            let _ = self.stop();
            return Ok(true);
        }
        eprintln!(
            "[login] Browser geöffnet — bitte im Fenster bei '{}' anmelden. Warte auf Login…",
            self.brain_id
        );
        loop {
            self.dismiss_consent();
            if self.is_logged_in() {
                // Kurz warten, damit Chrome Cookies/Session ins Profil flusht.
                std::thread::sleep(Duration::from_secs(2));
                let _ = self.stop();
                return Ok(true);
            }
            if start.elapsed() >= timeout {
                let _ = self.stop();
                return Ok(false);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Live-Diagnose: startet den Browser, prüft am echten DOM Login-Zustand,
    /// Composer-/Assistant-Selektoren und Cloudflare, und schließt wieder. Deckt
    /// Selektor-Drift auf, die `doctor` (read-only) nicht sehen kann.
    pub fn live_diagnose(&mut self, headless: bool) -> Result<LiveDiagnosis, String> {
        self.start(headless)?;
        self.dismiss_consent();
        let session_state = self.ensure_ready(15.0).unwrap_or(SessionState::Error);
        let diag = LiveDiagnosis {
            brain_id: self.brain_id.clone(),
            url: self.get_conversation_ref().unwrap_or_default(),
            cloudflare: self.is_cloudflare_blocked(),
            logged_in: self.is_logged_in(),
            composer_found: self.any_visible("composer"),
            assistant_count: self.assistant_count(),
            session_state,
        };
        let _ = self.stop();
        Ok(diag)
    }
}

impl BrainBackend for WebBrainBackend {
    fn brain_id(&self) -> &str {
        &self.brain_id
    }

    fn start(&mut self, headless: bool) -> Result<(), String> {
        // Remote-CDP-Endpunkt: kein lokaler Chrome-Launch (für Android/Termux).
        if let Ok(endpoint) = std::env::var("WEBAGENT_CDP_ENDPOINT") {
            if !endpoint.trim().is_empty() {
                let mut client =
                    CdpClient::connect_endpoint(&endpoint).map_err(|e| e.to_string())?;
                client
                    .navigate(&self.url, Duration::from_secs(30))
                    .map_err(|e| e.to_string())?;
                *self.process.borrow_mut() = None;
                *self.client.borrow_mut() = Some(client);
                self.baseline_count.set(0);
                return Ok(());
            }
        }
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
        // Verbindungs-Liveness: schlägt ein triviales Eval fehl, ist die
        // Seite/CDP-Verbindung tot — das ist ein Fehler, kein fehlender Login.
        if self.eval("1").is_err() {
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

        if self.sel("composer").is_empty() {
            return Err("Keine Composer-Selektoren konfiguriert".into());
        }
        let composer_js = self.sel_js("composer", &[]);
        let has_send_button = !self.sel("send_button").is_empty();

        // Auf den Composer WARTEN und befüllen: manche Seiten (z.B. ChatGPT)
        // melden ensure_ready=Ready, bevor das Eingabefeld hydratisiert/gerendert
        // ist. Bis ~12s pollen, statt sofort zu scheitern.
        let fill_deadline = Instant::now() + Duration::from_secs(12);
        let mut filled = false;
        while Instant::now() < fill_deadline {
            if self.fill_composer(&composer_js, text) {
                filled = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        if !filled {
            return Err("Composer-Feld nicht gefunden (Timeout)".into());
        }
        std::thread::sleep(Duration::from_millis(150));

        // Bis zu drei Absende-Versuche mit Verifikation. Manche Brains senden per
        // Enter, andere brauchen den Button — und Enter fügt bei manchen nur eine
        // Zeile ein. Als abgeschickt gilt: neue Nachricht, sichtbarer Stop-Button
        // oder geleerter Composer.
        for attempt in 0..3 {
            if attempt == 0 || !has_send_button {
                self.press_enter().ok();
            } else {
                self.click_first("send_button");
            }
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(250));
                if self.assistant_count() > baseline
                    || self.any_visible("stop_button")
                    || self.composer_is_empty(&composer_js)
                {
                    return Ok(baseline);
                }
            }
            // Nicht abgeschickt — Text neu setzen und den nächsten Weg versuchen.
            self.fill_composer(&composer_js, text);
        }

        // Best effort: der Controller/`wait_response` erkennt ein ausbleibendes
        // Ergebnis über sein Timeout.
        Ok(baseline)
    }

    fn wait_response(&mut self, baseline_count: i32, timeout: f64) -> Result<BrainResponse, String> {
        let start = Instant::now();
        // Selektor-Literale einmal bauen (ändern sich zur Laufzeit nie), dann pro
        // Poll-Iteration nur einen einzigen CDP-Roundtrip fahren.
        let assistant_js = self.sel_js("assistant_message", &["div.prose"]);
        let stop_sel = self.sel("stop_button");
        let has_stop = !stop_sel.is_empty();
        let stop_js = Self::js_selectors(&stop_sel);

        let mk = |text: String, idx: i32, done: bool, status: &str| BrainResponse {
            text,
            message_index: idx,
            generation_complete: done,
            backend_status: status.to_string(),
            ..Default::default()
        };

        // Phase 1: auf neue Nachricht ODER einen sichtbaren Stop-/Generating-Button
        // warten (die Generierung kann starten, bevor der Nachrichten-Container im
        // DOM committet ist).
        loop {
            let (count, _text, stop) = self.probe_generation(&assistant_js, &stop_js, -1);
            if count > baseline_count || (has_stop && stop) {
                break;
            }
            if start.elapsed().as_secs_f64() >= timeout {
                return Ok(mk(String::new(), -1, false, "timeout_no_message"));
            }
            std::thread::sleep(Duration::from_millis(300));
        }

        // Phase 2: Generierung überwachen. Autoritatives Fertigsignal ist das
        // Verschwinden des Stop-Buttons (bzw. ein vollständiges Protokoll-Dokument);
        // reine Textstabilität ist nur der Fallback für UIs ohne Stop-Button.
        let mut last_text = String::new();
        let mut stable_since = Instant::now();
        let mut stop_seen_ever = false;
        let mut target = (self.probe_generation(&assistant_js, &stop_js, -1).0 - 1)
            .max(baseline_count)
            .max(0);

        loop {
            let (count, current, stop_raw) = self.probe_generation(&assistant_js, &stop_js, target);
            if count - 1 > target {
                target = count - 1;
            }
            let stop_visible = has_stop && stop_raw;
            stop_seen_ever |= stop_visible;

            if current != last_text {
                last_text = current.clone();
                stable_since = Instant::now();
            }
            let stable_secs = stable_since.elapsed().as_secs_f64();

            match classify_completion(&current, has_stop, stop_seen_ever, stop_visible, stable_secs) {
                Completion::RateLimited => return Ok(mk(current, target, false, "rate_limit")),
                Completion::Complete => {
                    // Kurzes Settle: der letzte Chunk committet oft erst, nachdem
                    // der Stop-Button verschwunden ist. Danach final nachlesen.
                    std::thread::sleep(Duration::from_millis(500));
                    let finalized = self.assistant_text(target);
                    let text = if finalized.len() >= current.len() {
                        finalized
                    } else {
                        current
                    };
                    return Ok(mk(text, target, true, "ok"));
                }
                Completion::Continue => {}
            }

            if start.elapsed().as_secs_f64() >= timeout {
                let status = if stop_visible {
                    "timeout_still_generating"
                } else if last_text.trim().is_empty() {
                    "timeout_no_text"
                } else {
                    "timeout_unstable"
                };
                return Ok(mk(last_text, target, false, status));
            }
            std::thread::sleep(Duration::from_millis(300));
        }
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
        // Ohne positives Signal konservativ als nicht eingeloggt behandeln.
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

    const VALID_JSON: &str =
        r#"{"protocol":"webagent/1","actions":[{"id":"a","type":"finish"}]}"#;

    #[test]
    fn complete_when_stop_button_disappears() {
        // Stop-Button war sichtbar, ist jetzt weg, Text steht.
        let r = classify_completion("Antworttext ohne JSON", true, true, false, 0.2);
        assert_eq!(r, Completion::Complete);
    }

    #[test]
    fn keep_waiting_while_stop_button_visible() {
        // Solange der Stop-Button sichtbar ist, weiter warten (kein Timeout im Stream).
        let r = classify_completion("Teiltext, streamt noch", true, true, true, 5.0);
        assert_eq!(r, Completion::Continue);
    }

    #[test]
    fn valid_protocol_json_completes_immediately() {
        // Vollständiges JSON gilt sofort als fertig, selbst wenn der Stop-Button
        // scheinbar noch sichtbar ist (Polling-Timing).
        let r = classify_completion(VALID_JSON, true, true, true, 0.0);
        assert_eq!(r, Completion::Complete);
    }

    #[test]
    fn truncated_json_keeps_waiting() {
        let partial = r#"{"protocol":"webagent/1","actions":[{"id":"a","type":"shell","command":"unterminated"#;
        let r = classify_completion(partial, true, true, false, 5.0);
        assert_eq!(r, Completion::Continue);
    }

    #[test]
    fn transient_label_keeps_waiting() {
        let r = classify_completion("Thinking…", true, false, true, 0.0);
        assert_eq!(r, Completion::Continue);
    }

    #[test]
    fn rate_limit_detected() {
        let r = classify_completion(
            "You have reached your usage limit for Claude.",
            true,
            true,
            false,
            0.0,
        );
        assert_eq!(r, Completion::RateLimited);
    }

    #[test]
    fn no_stop_button_falls_back_to_stability() {
        // UI ohne Stop-Button: erst nach Stabilitätsfenster fertig.
        let unstable = classify_completion("fertiger Text", false, false, false, 0.5);
        assert_eq!(unstable, Completion::Continue);
        let stable = classify_completion("fertiger Text", false, false, false, STABILITY_SECONDS + 0.1);
        assert_eq!(stable, Completion::Complete);
    }

    #[test]
    fn js_scan_wraps_body_and_default() {
        let js = WebBrainBackend::js_scan("[\"a.b\"]", "return 1;", "0");
        assert!(js.contains("var S=[\"a.b\"]"), "js={js}");
        assert!(js.contains("return 1;"));
        assert!(js.trim_end().ends_with("return 0;})()"), "js={js}");
    }

    #[test]
    fn sel_js_uses_fallback_when_key_missing() {
        let b = WebBrainBackend::from_config("chatgpt").unwrap();
        let composer = b.sel_js("composer", &["div.prose"]);
        assert!(composer.starts_with('[') && composer.len() > 2);
        let fb = b.sel_js("does_not_exist_key", &["div.fallback"]);
        assert!(fb.contains("div.fallback"), "fb={fb}");
    }

    #[test]
    fn missed_stop_button_completes_after_longer_stability() {
        // Stop-Button nie erfasst (sehr schnelle Antwort): nach 1.5×-Fenster fertig,
        // statt dauerhaft zu blockieren.
        let r = classify_completion("kurze Antwort", true, false, false, STABILITY_SECONDS * 1.5 + 0.1);
        assert_eq!(r, Completion::Complete);
    }
}
