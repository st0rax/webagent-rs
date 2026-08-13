//! browser — konkretes BrainBackend, das ein Embedded WebView (wry/tao) steuert.
//!
//! Spiegelt `../src/webagent/brains/playwright_base.py`, ersetzt Playwright aber
//! durch [`crate::page_driver::PageDriver`]. DOM-Operationen laufen über JS-Eval;
//! Tastendrücke/Maus über WebView-Injection.

pub mod backend;
pub mod composer;
pub mod js;
pub mod operations;
pub mod selectors;
pub mod ui;
pub mod verify;

use std::cell::RefCell;
use std::path::PathBuf;

use serde_json::Value;

use crate::brain::{BrainBackend, SessionState};
use crate::page_driver::PageDriver;
#[cfg(feature = "webview")]
use crate::webview_runtime::WebViewRuntime;

mod blocking;
mod send;
mod surface;

#[cfg(test)]
pub(crate) use blocking::{
    banner_is_prompt_echo, BLOCK_BANNER_MAX_CHARS, PROSE_STABILITY_SECONDS, STABILITY_SECONDS,
    TRUNCATED_STABILITY_SECONDS,
};
pub(crate) use blocking::{block_phrase_in_text, classify_completion, Completion};
pub use send::is_send_disabled_error;
#[cfg(test)]
pub(crate) use send::submit_verify_rounds;

/// Enthält der Text überhaupt etwas, das eine Protokoll-Antwort sein könnte?
///
/// Bewusst grob: ein `{` oder ein `WEBAGENT/1`-Umschlag genügt. Es geht nicht
/// darum, ob das Protokoll gültig ist (das prüft `protocol::parse`), sondern ob
/// das Brain überhaupt angefangen hat zu antworten statt nur zu denken.
pub(crate) fn has_protocol_payload(text: &str) -> bool {
    let t = text.trim();
    t.contains('{') || t.contains("WEBAGENT/1")
}

/// Web-Chat-Backend für einen Provider (chatgpt, claude, …).
/// Ergebnis einer Live-Diagnose (echter Browser gegen die Provider-Seite).
#[derive(Debug, Clone)]
pub struct LiveDiagnosis {
    pub brain_id: String,
    pub url: String,
    pub cloudflare: bool,
    pub logged_in: bool,
    /// Ist ein Anmelden-Knopf sichtbar? Getrennt ausgewiesen, weil genau diese
    /// Kombination den Fehler entlarvt: `logged_in=true` bei sichtbarem
    /// Anmelden-Knopf heisst, der Indikator matcht etwas, das auch anonym da
    /// ist (gemini: der Composer der Startseite).
    pub login_button_visible: bool,
    pub composer_found: bool,
    pub assistant_count: i32,
    pub session_state: SessionState,
}

pub struct WebBrainBackend {
    pub(crate) brain_id: String,
    url: String,
    pub(crate) selectors: selectors::Selectors,
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    profile_dir: PathBuf,
    /// Optionales isoliertes Laufzeit-Profil (z.B. Swarm-Teilkopie). Überschreibt
    /// `profile_dir` in `start()`, sofern gesetzt. Wird nicht in `from_config`
    /// befüllt — explizit via `with_profile_override` gesetzt.
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    profile_override: Option<PathBuf>,
    #[cfg(feature = "webview")]
    runtime: RefCell<Option<WebViewRuntime>>,
    pub(crate) driver: RefCell<Option<Box<dyn PageDriver>>>,
    /// Text der letzten Assistenten-Nachricht VOR dem Senden — damit wait_response
    /// den Antwortbeginn auch dann erkennt, wenn der Nachrichtenzähler nicht
    /// inkrementiert (Container-Selektor / bestehende Konversation).
    baseline_text: RefCell<String>,
    /// Zuletzt gesendeter Text. Die Blockade-Erkennung liest die ganze Seite,
    /// auf der auch die eigene Frage steht — ohne diesen Vergleich meldet jede
    /// Aufgabe, die „Nachrichtenlimit"/„Login"/„Cloudflare" erwaehnt, alle Brains
    /// als blockiert (real passiert 2026-07-21).
    last_sent: RefCell<String>,
}

impl WebBrainBackend {
    /// Start-URL des Brains (für Shared-Pool-Tabs).
    pub fn brain_url(&self) -> &str {
        &self.url
    }

    /// Hängt einen Pool-Page-Driver an (kein eigener WebView-Runtime).
    pub fn attach_page_driver(&self, driver: Box<dyn PageDriver>) {
        *self.driver.borrow_mut() = Some(driver);
        #[cfg(feature = "webview")]
        {
            *self.runtime.borrow_mut() = None;
        }
    }

    /// Erstellt ein Backend aus der zentralen Brain-Konfiguration.
    pub fn from_config(brain_id: &str) -> Result<Self, String> {
        let brains = crate::config::brains();
        let spec = brains
            .get(brain_id)
            .ok_or_else(|| format!("Unbekanntes Brain: {brain_id}"))?;
        let url = spec.get("url").cloned().unwrap_or_default();
        let profile_dir = PathBuf::from(spec.get("profile_dir").cloned().unwrap_or_default());
        let selectors = crate::config::load_selectors(brain_id)
            .map(selectors::Selectors::from_value)
            .map_err(|e| format!("Selektoren nicht ladbar: {e}"))?;
        Ok(Self {
            brain_id: brain_id.to_string(),
            url,
            selectors,
            profile_dir,
            profile_override: None,
            #[cfg(feature = "webview")]
            runtime: RefCell::new(None),
            driver: RefCell::new(None),
            baseline_text: RefCell::new(String::new()),
            last_sent: RefCell::new(String::new()),
        })
    }

    /// Erstellt ein Backend fuer einen noch nicht registrierten Brain (z.B.
    /// aus `probe`): nur URL, noch keine Selektoren. `probe_surface` fuellt sie
    /// an der lebenden Oberflaeche, danach registriert der Aufrufer das Brain.
    pub fn from_url(brain_id: &str, url: &str) -> Result<Self, String> {
        let id = crate::config::sanitize_brain_id(brain_id);
        if id.is_empty() {
            return Err("Brain-ID leer".into());
        }
        if url.trim().is_empty() {
            return Err("URL leer".into());
        }
        let profile_dir = crate::config::profiles_dir().join(&id);
        Ok(Self {
            brain_id: id,
            url: url.trim().to_string(),
            selectors: selectors::Selectors::empty(),
            profile_dir,
            profile_override: None,
            #[cfg(feature = "webview")]
            runtime: RefCell::new(None),
            driver: RefCell::new(None),
            baseline_text: RefCell::new(String::new()),
            last_sent: RefCell::new(String::new()),
        })
    }

    /// Setzt ein isoliertes Laufzeit-Profil (z.B. eine Swarm-Teilkopie aus
    /// `config::prepare_swarm_profile`). Überschreibt `profile_dir` in `start()`.
    pub fn with_profile_override(mut self, profile: PathBuf) -> Self {
        self.profile_override = Some(profile);
        self
    }

    /// Kanonisches Profil-Verzeichnis dieses Backends (`profiles/<brain>` bzw. Override-Env).
    pub fn profile_dir(&self) -> &PathBuf {
        &self.profile_dir
    }

    /// Effektives Profil (Override falls gesetzt, sonst kanonisch).
    pub fn effective_profile_dir(&self) -> &PathBuf {
        self.profile_override.as_ref().unwrap_or(&self.profile_dir)
    }

    /// Schneller Login-Check: Browser kurz starten, Zustand prüfen, stoppen.
    /// Für `login-all` (bereits eingeloggte Brains überspringen).
    /// Headless, damit der Skip-Check bei 8 Brains keine Fenster aufpoppen lässt.
    pub fn is_logged_in_quick(&self) -> Result<bool, String> {
        let mut probe = WebBrainBackend::from_config(&self.brain_id)?;
        if let Some(p) = &self.profile_override {
            probe = probe.with_profile_override(p.clone());
        }
        probe.start(true)?;
        let logged = probe.is_logged_in();
        let _ = probe.stop();
        Ok(logged)
    }

    /// Selektor-Liste zu einem Schlüssel (leere Liste, wenn nicht vorhanden).
    ///
    /// Delegiert an [`selectors::Selectors`] — Schritt 1: Selektoren als eigener
    /// Wert, damit die Operationen sie als Parameter mitgegeben bekommen.
    fn sel(&self, key: &str) -> Vec<String> {
        self.selectors.list(key)
    }

    /// JS-Array-Literal der Selektoren zu einem Schlüssel; `fallback` greift,
    /// wenn keine konfiguriert sind.
    fn sel_js(&self, key: &str, fallback: &[&str]) -> String {
        self.selectors.js(key, fallback)
    }

    /// JS-Array-Literal aus einer Selektorliste (sicher escaped).
    ///
    /// Delegiert an [`js::js_selectors`] — die Bausteine haben inzwischen einen
    /// zweiten Aufrufer ausserhalb des Backends (`brain_probe::verify`).
    fn js_selectors(list: &[String]) -> String {
        js::js_selectors(list)
    }

    /// JS-Prelude fuer [`Self::js_scan`]: `Q(sel)` / `QA(sel)` loesen einen Selektor
    /// auf und verstehen dabei **auch** die Playwright-Textformen, die querySelector
    /// nicht kann:
    ///
    /// - `text=foo`            — beliebiges Element, dessen Text `foo` enthaelt
    /// - `text=/re/i`          — dito per Regex
    /// - `button:has-text('x')`— Elemente von `button`, deren Text `x` enthaelt
    ///
    /// Warum: 96 von 283 Eintraegen in `selectors/*.json` sind in dieser Syntax
    /// geschrieben. `querySelector` wirft darauf, das try/catch schluckt es — sie
    /// waren also stumm wirkungslos. Acht Keys bestehen *ausschliesslich* daraus
    /// (u.a. `consent_reject_button` bei gemini/qwen), deren Feature konnte nie
    /// feuern. Das handgeschriebene `dismiss_qwen_blocks` ist die Narbe davon.
    ///
    /// Bei Textmatches nur die **innersten** Treffer zurueckgeben — sonst matcht
    /// jeder Vorfahr bis `<body>` mit.
    const JS_SEL_PRELUDE: &'static str = js::JS_SEL_PRELUDE;

    /// Baut ein IIFE, das die Selektorliste `list_js` durchläuft und `body` auf
    /// jeden Selektor `S[i]` anwendet; liefert `default`, wenn nichts matcht.
    ///
    /// Im `body` `Q(S[i])` / `QA(S[i])` statt `document.querySelector*` nutzen —
    /// nur die verstehen die Textformen (siehe [`Self::JS_SEL_PRELUDE`]).
    ///
    /// Jeder Selektor läuft weiterhin in einem eigenen try/catch: ein kaputter
    /// Selektor darf die restliche Liste nicht abbrechen.
    fn js_scan(list_js: &str, body: &str, default: &str) -> String {
        js::js_scan(list_js, body, default)
    }

    /// Führt ein JS im Seitenkontext aus (mit ausgeliehenem Client).
    fn eval(&self, expr: &str) -> Result<Value, String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        operations::eval(driver.as_mut(), expr)
    }

    fn eval_bool(&self, expr: &str) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::eval_bool(driver.as_mut(), expr),
            None => false,
        }
    }

    fn eval_str(&self, expr: &str) -> String {
        self.eval(expr)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default()
    }

    /// Anzahl der Assistenten-Nachrichten (robust über die Selektorliste).
    fn assistant_count(&self) -> i32 {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::assistant_count(driver.as_mut(), &self.selectors),
            None => 0,
        }
    }

    /// Diagnose-Kompendium fuer fehlgeschlagene Turns: URL, Nachrichtenanzahl,
    /// Texte aller Assistenten-Nachrichten, Stop-Knopf, Dialoge. Nur zur Fehlersuche.
    pub(crate) fn diagnostic_state(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "url={:?} assistant_count={}",
            self.get_conversation_ref(),
            self.assistant_count()
        ));
        let count = self.assistant_count();
        for i in 0..count.max(3) {
            let t = self.assistant_text(i);
            if !t.is_empty() {
                out.push_str(&format!("  [{i}] {:?}", crate::char_prefix(&t, 60)));
            }
        }
        out.push_str(&format!(
            "  stop_visible={}",
            self.any_visible("stop_button")
        ));
        let assistant_js = self.sel_js("assistant_message", &["div.prose"]);
        let stop_sel = self.sel("stop_button");
        let stop_js = Self::js_selectors(&stop_sel);
        let (pcount, ptext, pstop) = self.probe_generation(&assistant_js, &stop_js, -1);
        out.push_str(&format!(
            "  probe(count={pcount},text={:?},stop={pstop})",
            crate::char_prefix(&ptext, 60)
        ));
        let js = "var d=document.querySelector('[role=dialog],[aria-modal=true]');d?d.innerText.slice(0,120):'kein Dialog'";
        let dlg = self
            .eval(js)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.to_string()));
        out.push_str(&format!("  dialog={}", dlg.unwrap_or_default()));
        out
    }

    /// Text der n-ten Assistenten-Nachricht, mit zurueckgewonnener Mathe-Quelle.
    ///
    /// Liest ueber `TX` statt `innerText`: deepseeks Oberflaeche schickt jeden
    /// PowerShell-Befehl mit `$`-Variablen durch KaTeX. Am 30.07.2026 wurde aus
    /// `$lines=Get-Content src/controller.rs$` im `innerText` ein Zeichen pro
    /// Zeile in mathematischer Kursivschrift, `|` wurde zu `∣`, `-` zu `−`, und
    /// der Inhalt stand doppelt da (Formel + Annotation). Der Befehl des Brains
    /// war korrekt — unser Auslesen hat ihn zerstoert, und der Lauf verlor die
    /// Runde an einen `protocol_invalid`.
    ///
    /// Zurueckrechnen aus den Glyphen waere Raten. KaTeX legt die Originalquelle
    /// aber selbst im DOM ab (`annotation[encoding="application/x-tex"]`) — die
    /// wird gelesen. `TX` steht bewusst im gemeinsamen Prelude: es gibt zwei
    /// Auslesepfade (hier und `probe_generation`), und ein Fix in nur einem
    /// waere wirkungslos geblieben.
    fn assistant_text(&self, index: i32) -> String {
        let list = self.sel_js("assistant_message", &["div.prose"]);
        let body = format!(
            "var els=QA(S[i]);if(els.length>{idx}){{return TX(els[{idx}]).trim();}}",
            idx = index
        );
        // Claude rendert seine Denk-Zusammenfassung in denselben Container und
        // das DOM liefert sie doppelt — sie gehoert nicht in die Antwort.
        crate::observer::strip_repeated_lead_line(
            &self.eval_str(&Self::js_scan(&list, &body, "\"\"")),
        )
    }

    /// Ist mindestens ein Selektor aus der Liste im DOM sichtbar?
    ///
    /// Delegiert an [`operations::any_visible`] — Schritt 2: die Operation als
    /// freie Funktion, testbar mit dem `MockPageDriver`.
    fn any_visible(&self, key: &str) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::any_visible(driver.as_mut(), &self.selectors, key),
            None => false,
        }
    }

    /// Klickt das erste sichtbare Element aus der Selektorliste.
    fn click_first(&self, key: &str) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::click_first(driver.as_mut(), &self.selectors, key),
            None => false,
        }
    }

    /// Klickt das erste sichtbare Element aus der Selektorliste per ECHTEM CDP-
    /// Mausklick (trusted `Input.dispatchMouseEvent` auf die Element-Mitte). Nötig,
    /// wo synthetisches `el.click()` von Anti-Automation ignoriert wird — z.B.
    /// Geminis „Nachricht senden"-Button (Text steht im Composer, Button ist
    /// enabled, aber der untrusted Klick löst keinen Submit aus). Spiegelt den
    /// Composer-Klick in `fill_composer`, der bei allen Providern funktioniert.
    fn click_visible_real(&self, key: &str) -> bool {
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}";
        let coords = self
            .eval(&Self::js_scan(
                &Self::js_selectors(&sels),
                coord_body,
                "null",
            ))
            .unwrap_or(Value::Null);
        let (x, y) = match (
            coords.get("x").and_then(|v| v.as_f64()),
            coords.get("y").and_then(|v| v.as_f64()),
        ) {
            (Some(x), Some(y)) => (x, y),
            _ => return false,
        };
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => driver.click_at_trusted(x, y).is_ok(),
            None => false,
        }
    }

    /// Ein einziger CDP-Roundtrip, der Nachrichtenanzahl, den Text der Nachricht
    /// `target` und die Sichtbarkeit des Stop-Buttons gemeinsam ermittelt — statt
    /// dreier separater `Runtime.evaluate`-Aufrufe pro Poll-Iteration.
    fn probe_generation(
        &self,
        assistant_js: &str,
        stop_js: &str,
        target: i32,
    ) -> (i32, String, bool) {
        // MUSS das Prelude benutzen (`QA`/`Q`, nicht `document.querySelector*`):
        // 96 von 283 Eintraegen in `selectors/*.json` sind in Playwright-
        // Textsyntax geschrieben (`button:has-text('Stop')`). Rohes
        // `querySelector` wirft darauf, der try/catch schluckt es — die Eintraege
        // waren hier stumm wirkungslos.
        //
        // Genau das war der Grund, warum am 29./30.07.2026 fuer kimi, deepseek
        // und claude NIE ein Stop-Button erkannt wurde: ihre `aria-label`-Muster
        // matchen diese Oberflaechen nicht, und die `:has-text`-Eintraege, die es
        // taeten, flogen in den catch. Ohne Stop-Signal haengt die Fertig-
        // Erkennung am Stabilitaetsfenster — daran wurde mitten im Stream
        // Reasoning-Prosa als Antwort geerntet.
        let expr = Self::probe_generation_js(assistant_js, stop_js, target);
        match self.eval(&expr) {
            Ok(v) => (
                v.get("count").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                // Gleiche Bereinigung wie in `assistant_text` — dies ist der
                // zweite Auslesepfad (Warteschleife), und sein Text ist der,
                // der am Ende zurueckgegeben wird.
                crate::observer::strip_repeated_lead_line(
                    v.get("text").and_then(|x| x.as_str()).unwrap_or(""),
                ),
                v.get("stop").and_then(|x| x.as_bool()).unwrap_or(false),
            ),
            Err(_) => (0, String::new(), false),
        }
    }

    /// Baut das JS für [`Self::probe_generation`] — getrennt, damit prüfbar ist,
    /// dass es das Prelude mitbringt (siehe Test
    /// `jeder_selektor_auswertende_pfad_bringt_das_prelude_mit`).
    fn probe_generation_js(assistant_js: &str, stop_js: &str, target: i32) -> String {
        format!(
            r#"(function(){{{prelude}
var A={assistant_js};var count=0,els=null;
for(var i=0;i<A.length;i++){{try{{var e=QA(A[i]);if(e.length>0){{count=e.length;els=e;break;}}}}catch(x){{}}}}
var ti={target};if(ti<0)ti=count-1;
var text="";if(els&&ti>=0&&els.length>ti){{text=TX(els[ti]).trim();}}
var stop=false;var S={stop_js};
for(var j=0;j<S.length;j++){{try{{var el=Q(S[j]);if(el){{var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){{stop=true;break;}}}}}}catch(x){{}}}}
return {{count:count,text:text,stop:stop}};}})()"#,
            prelude = Self::JS_SEL_PRELUDE,
            assistant_js = assistant_js,
            stop_js = stop_js,
            target = target
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absende_wartezeit_waechst_mit_der_nachrichtenlaenge() {
        // Kurze Nachricht: 32 Runden = 8 Sekunden. Mindestfenster, weil Brains
        // wie perplexity/deepseek das Senden erst nach ~20 s registrieren und
        // ein zu kurzes Fenster als "kein Absende-Beweis" droppt (100er-Lauf
        // 11.08.: 7 von 14 Drops waren Send-Fehlnegative).
        assert_eq!(submit_verify_rounds(0), 32);
        assert_eq!(submit_verify_rounds(500), 32);
        // 200.000 Zeichen: 32 + 20 Runden = 13 Sekunden. Genau dieser Fall
        // scheiterte am 05.08.2026 unter Last, obwohl er eine Stunde vorher
        // durchging - der Beweis kam nur nach den festen 3 Sekunden.
        assert_eq!(submit_verify_rounds(200_000), 52);
        // Eine Million Zeichen: 32 + 100 Runden = 33 Sekunden, noch unter dem Deckel.
        assert_eq!(submit_verify_rounds(1_000_000), 120);
        // Erst darueber greift die Deckelung — sonst wartet eine irrtuemlich
        // riesige Eingabe minutenlang.
        assert_eq!(submit_verify_rounds(2_000_000), 120);
        assert_eq!(submit_verify_rounds(usize::MAX), 120);
    }

    /// Rust-Port der drei Regexe aus `JS_SEL_PRELUDE` (`text=/re/i`, `text=foo`,
    /// `sel:has-text('x')`). Kein JS-Interpreter im Testprozess verfuegbar, also
    /// prueft dieser Port dieselbe Erkennungslogik ohne echten Browser -- Ziel ist
    /// nicht "identisches Verhalten in jedem Detail", sondern "erkennt der Prelude-
    /// Parser jede Textform, die tatsaechlich in `selectors/*.json` vorkommt".
    /// Braucht `fancy-regex` statt `regex`: die `:has-text`-Form spiegelt JS'
    /// Rueckreferenz `\2` (gleiches Anfuehrungszeichen schliesst), das die
    /// linear-time `regex`-Crate nicht unterstuetzt.
    fn parses_as_text_selector(s: &str) -> bool {
        use fancy_regex::Regex as FancyRegex;
        lazy_static::lazy_static! {
            static ref RE_REGEX: FancyRegex = FancyRegex::new(r"^text=/(.*)/([a-z]*)$").unwrap();
            static ref RE_PLAIN: FancyRegex = FancyRegex::new(r"^text=(.*)$").unwrap();
            static ref RE_HAS_TEXT: FancyRegex =
                FancyRegex::new(r#"^(.*?):has-text\((['"])([\s\S]*?)\2\)$"#).unwrap();
        }
        RE_REGEX.is_match(s).unwrap_or(false)
            || RE_PLAIN.is_match(s).unwrap_or(false)
            || RE_HAS_TEXT.is_match(s).unwrap_or(false)
    }

    /// Inventar-Test (A6): jeder Selektor in `selectors/*.json`, der wie eine
    /// Playwright-Textform aussieht (enthaelt "text=" oder ":has-text"), muss vom
    /// Prelude-Parser tatsaechlich erkannt werden -- sonst faellt er still auf
    /// rohes `querySelector` zurueck, wo er nie matcht (die Ursache, warum acht
    /// Keys wie `consent_reject_button` bei gemini/qwen frueher nie feuerten).
    #[test]
    fn all_text_form_selectors_are_recognized_by_prelude_parser() {
        let dir = crate::config::selectors_dir();
        let mut checked = 0usize;
        let mut unrecognized = Vec::new();
        for entry in std::fs::read_dir(&dir).expect("selectors dir") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = std::fs::read_to_string(&path).expect("read selector file");
            let json: Value = serde_json::from_str(&content).expect("valid json");
            let Value::Object(map) = json else { continue };
            for (_key, value) in map {
                let Value::Array(items) = value else { continue };
                for item in items {
                    let Some(s) = item.as_str() else { continue };
                    if !(s.contains("text=") || s.contains(":has-text")) {
                        continue;
                    }
                    checked += 1;
                    if !parses_as_text_selector(s) {
                        unrecognized.push(format!("{}: {s}", path.display()));
                    }
                }
            }
        }
        assert!(
            checked > 0,
            "erwartete Text-Selektoren in selectors/*.json zu finden"
        );
        assert!(
            unrecognized.is_empty(),
            "Selektoren, die der Prelude-Parser nicht erkennt (fallen still auf querySelector zurueck): {unrecognized:#?}"
        );
    }

    #[test]
    fn prelude_parser_recognizes_each_syntax_form() {
        assert!(parses_as_text_selector("text=Anmelden"));
        assert!(parses_as_text_selector("text=/Which response is better/i"));
        assert!(parses_as_text_selector("button:has-text('Send')"));
        assert!(parses_as_text_selector(
            "div[role='dialog'][data-state='open'] button:has-text('Close')"
        ));
        // Plain CSS ist bewusst NICHT als Textform erkannt -- geht stattdessen den
        // normalen querySelector-Pfad.
        assert!(!parses_as_text_selector("div.prose"));
        assert!(!parses_as_text_selector("button[aria-label*='Send' i]"));
    }

    #[test]
    fn block_phrase_in_text_detects_qwen_limit() {
        let text = "Oops! There was an issue connecting to Qwen3.7-Plus.\n\
                     You have reached the daily usage limit. Please wait 3 hours before trying again.";
        assert_eq!(block_phrase_in_text(text), Some("usage limit"));
    }

    #[test]
    fn block_phrase_in_text_detects_mistral_limit() {
        assert_eq!(
            block_phrase_in_text("Sie haben Ihr Nachrichtenlimit erreicht."),
            Some("nachrichtenlimit")
        );
    }

    #[test]
    fn block_phrase_in_text_detects_chatgpt_german_usage_limit() {
        assert_eq!(
            block_phrase_in_text(
                "Dateien, Bilder und Datenanalyse sind nicht verfügbar, bis dein Nutzungslimit um 23:40 zurückgesetzt wird."
            ),
            Some("nutzungslimit")
        );
    }

    #[test]
    fn block_phrase_in_text_detects_capacity_message() {
        assert_eq!(
            block_phrase_in_text("Sorry, too many users are chatting with Kimi right now."),
            Some("too many users")
        );
        assert_eq!(
            block_phrase_in_text("Gerade zu viele Anfragen, bitte spaeter erneut versuchen."),
            Some("zu viele anfragen")
        );
    }

    #[test]
    fn block_phrase_in_text_ignores_phrase_in_long_answer() {
        // Swarm-Test-Fund 2026-07-20: eine lange, legitime Antwort, die
        // "rate limit"/"usage limit" als Verbesserungsvorschlag ERWÄHNT, darf
        // NICHT als Block gewertet werden.
        let long = format!(
            "Hier sind Verbesserungsvorschläge für webagent-rs: {} \
             Ausserdem solltest du ein rate limit und quota exceeded handling \
             einbauen, um usage limit-Fehler sauber abzufangen.",
            "Die Architektur ist solide und modular aufgebaut. ".repeat(12)
        );
        assert!(long.chars().count() > BLOCK_BANNER_MAX_CHARS);
        assert_eq!(block_phrase_in_text(&long), None);
    }

    #[test]
    fn block_phrase_in_text_none_for_normal_answer() {
        assert_eq!(
            block_phrase_in_text("Die Hauptstadt von Frankreich ist Paris."),
            None
        );
    }

    #[test]
    fn block_phrase_in_text_ignores_cloudflare_in_code_fragment() {
        let fragment = "blockierte Antworten (Tageslimit/Login/Cloudflare) \
                        zählen als Fehler statt als Beitrag";
        assert_eq!(block_phrase_in_text(fragment), None);
    }

    #[test]
    fn from_config_loads_selectors_and_url() {
        let backend = WebBrainBackend::from_config("chatgpt").expect("chatgpt config");
        assert_eq!(backend.brain_id(), "chatgpt");
        assert_eq!(backend.url, "https://chatgpt.com/");
        assert!(!backend.sel("composer").is_empty());
        assert!(
            backend
                .sel("stop_button")
                .iter()
                .all(|selector| !selector.contains("*=")),
            "ChatGPT-Stop-Signale muessen exakt sein; breite aria-Teilstrings treffen fremde Controls"
        );
    }

    #[test]
    fn every_brain_can_veto_a_false_login() {
        // `is_logged_in` verwirft einen positiven Indikator, sobald ein
        // Anmelden-Knopf sichtbar ist. Diese Sperre wirkt nur, wenn das Brain
        // ueberhaupt `login_button`-Selektoren hat — fehlen sie, faellt die
        // Erkennung still auf den Indikator zurueck. Genau daran haette es
        // gelegen: geminis Indikator war der Composer, den auch die
        // ausgeloggte Startseite zeigt.
        for (id, _url) in crate::config::BRAIN_TABLE {
            let backend = WebBrainBackend::from_config(id).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(
                !backend.sel("login_button").is_empty(),
                "{id}: ohne login_button-Selektoren kann ein falsch positiver \
                 Login nicht widerlegt werden"
            );
        }
    }

    #[test]
    fn all_configured_brains_have_selectors() {
        for (id, _url) in crate::config::BRAIN_TABLE {
            let backend = WebBrainBackend::from_config(id).unwrap_or_else(|e| panic!("{id}: {e}"));
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

    const VALID_JSON: &str = r#"{"protocol":"webagent/1","actions":[{"id":"a","type":"finish"}]}"#;

    #[test]
    fn complete_when_stop_button_disappears() {
        // Stop-Button war sichtbar, ist jetzt weg, Text steht.
        // Nutzlastfreier Text braucht zusätzlich das Prosa-Fenster.
        let r = classify_completion(
            "Antworttext ohne JSON",
            true,
            true,
            false,
            PROSE_STABILITY_SECONDS + 0.1,
            true,
        );
        assert_eq!(r, Completion::Complete);
    }

    #[test]
    fn keep_waiting_while_stop_button_visible() {
        // Solange der Stop-Button sichtbar ist, weiter warten (kein Timeout im Stream).
        let r = classify_completion("Teiltext, streamt noch", true, true, true, 5.0, true);
        assert_eq!(r, Completion::Continue);
    }

    #[test]
    fn valid_protocol_json_completes_immediately() {
        // Vollständiges JSON gilt sofort als fertig, selbst wenn der Stop-Button
        // scheinbar noch sichtbar ist (Polling-Timing).
        let r = classify_completion(VALID_JSON, true, true, true, 0.0, true);
        assert_eq!(r, Completion::Complete);
    }

    #[test]
    fn truncated_json_keeps_waiting() {
        let partial = r#"{"protocol":"webagent/1","actions":[{"id":"a","type":"shell","command":"unterminated"#;
        let r = classify_completion(partial, true, true, false, 5.0, true);
        assert_eq!(r, Completion::Continue);
    }

    #[test]
    fn stable_truncated_protocol_reaches_repair_instead_of_provider_timeout() {
        let partial =
            "WEBAGENT/1 SHELL\nid: inspect\n---SCRIPT---\nGet-Content src/lib.rs\n---END SCRIPT";
        let r = classify_completion(
            partial,
            true,
            true,
            false,
            TRUNCATED_STABILITY_SECONDS + 0.1,
            false,
        );
        assert_eq!(r, Completion::Complete);
        assert!(!crate::protocol::parse(partial).valid);
    }

    #[test]
    fn transient_label_keeps_waiting() {
        let r = classify_completion("Thinking…", true, false, true, 0.0, true);
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
            true,
        );
        assert_eq!(r, Completion::RateLimited);
    }

    // Regression: derselbe Limit-Text darf für Nicht-Claude-Brains NICHT als
    // Rate-Limit gelten. qwens Ausgabe/UI enthielt "…usage limit…" und wurde sonst
    // fälschlich als claude_rate_limited terminal abgebrochen.
    #[test]
    fn rate_limit_ignored_when_not_claude() {
        let r = classify_completion(
            "You have reached your usage limit for Claude.",
            true,
            true,
            false,
            0.0,
            false, // rate_limit_aware=false (nicht claude)
        );
        assert_ne!(r, Completion::RateLimited);
    }

    #[test]
    fn no_stop_button_falls_back_to_stability() {
        // UI ohne Stop-Button: erst nach Stabilitätsfenster fertig.
        let unstable = classify_completion("{\"a\":1}", false, false, false, 0.5, true);
        assert_eq!(unstable, Completion::Continue);
        let stable = classify_completion(
            "{\"a\":1}",
            false,
            false,
            false,
            STABILITY_SECONDS + 0.1,
            true,
        );
        assert_eq!(stable, Completion::Complete);
    }

    #[test]
    fn closed_invalid_message_reaches_repair_after_stability() {
        let text = "WEBAGENT/1 MESSAGE\nid: final\n---CONTENT---\nfertig\n---END MESSAGE---";
        assert_eq!(
            classify_completion(text, true, false, false, 0.5, false),
            Completion::Continue
        );
        assert_eq!(
            classify_completion(text, true, false, false, STABILITY_SECONDS * 1.5, false),
            Completion::Complete
        );
    }

    #[test]
    fn beide_auslesepfade_gewinnen_die_mathe_quelle_zurueck() {
        // Regression 30.07.2026: deepseeks Oberflaeche schickt PowerShell mit
        // `$`-Variablen durch KaTeX. `innerText` liefert dann ein Zeichen pro
        // Zeile in mathematischer Kursivschrift statt des Befehls. Es gibt zwei
        // Auslesepfade — ein Fix in nur einem waere wirkungslos.
        let b = WebBrainBackend::from_config("deepseek").expect("deepseek-Konfig");
        let assistant = b.sel_js("assistant_message", &[]);
        let stop = b.sel_js("stop_button", &[]);

        let probe = WebBrainBackend::probe_generation_js(&assistant, &stop, 0);
        assert!(
            probe.contains("TX(els[") && !probe.contains("els[ti].innerText"),
            "probe_generation liest noch roh: {probe}"
        );

        let body = "var els=QA(S[i]);if(els.length>0){return TX(els[0]).trim();}";
        let scan = WebBrainBackend::js_scan(&assistant, body, "\"\"");
        assert!(
            scan.contains("var TX=function"),
            "Prelude ohne TX — die Mathe-Quelle bliebe unerreichbar"
        );
        assert!(
            scan.contains("annotation[encoding=\\\"application/x-tex\\\"]")
                || scan.contains("annotation[encoding=\"application/x-tex\"]"),
            "TX liest die KaTeX-Originalquelle nicht"
        );
    }

    #[test]
    fn jeder_selektor_auswertende_pfad_bringt_das_prelude_mit() {
        // Regression 30.07.2026: `probe_generation` und `account_label` bauten
        // ihr JS selbst und werteten konfigurierte Selektoren mit rohem
        // `document.querySelector` aus. 96 von 283 Eintraegen in
        // `selectors/*.json` stehen aber in Playwright-Textsyntax
        // (`button:has-text('Stop')`); darauf wirft querySelector, der
        // try/catch schluckt es, der Eintrag ist stumm wirkungslos.
        //
        // Folge: fuer kimi, deepseek, claude und zai wurde NIE ein Stop-Button
        // erkannt — das autoritative Fertigsignal fehlte komplett.
        let b = WebBrainBackend::from_config("kimi").expect("kimi-Konfig");
        let stop = b.sel_js("stop_button", &[]);
        assert!(
            stop.contains(":has-text"),
            "Testvoraussetzung: kimi hat has-text-Selektoren — {stop}"
        );

        let assistant = b.sel_js("assistant_message", &[]);
        for (name, js) in [
            (
                "probe_generation",
                WebBrainBackend::probe_generation_js(&assistant, &stop, 0),
            ),
            ("account_label", b.account_label_js()),
        ] {
            assert!(
                js.contains("var QA=function"),
                "{name}: Prelude fehlt — has-text-Selektoren wirken dort nicht"
            );
            assert!(
                !js.contains("document.querySelector(S["),
                "{name}: wertet Selektoren roh aus statt via Q()"
            );
            assert!(
                !js.contains("document.querySelectorAll(A["),
                "{name}: wertet Selektoren roh aus statt via QA()"
            );
        }
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
        let r = classify_completion(
            "{\"kurze\":\"Antwort\"}",
            true,
            false,
            false,
            STABILITY_SECONDS * 1.5 + 0.1,
            true,
        );
        assert_eq!(r, Completion::Complete);
    }

    // Regression 2026-07-29: kimis Reasoning-Block stand still, der Stop-Button
    // wurde nie erfasst — und die halbfertige Denk-Prosa wurde als Antwort
    // geerntet. Wörtlicher Text aus Run 20260729_212023_dabfadbc.
    #[test]
    fn mid_stream_reasoning_prose_is_not_a_finished_answer() {
        let prosa = "Der Benutzer hat keine neue Aufgabe gestellt, sondern den \
                     WebAgent-Runner gestartet. Ich muss zuerst den aktuellen Zustand \
                     des Projekts erfassen, um zu verstehen, welche der offenen Tasks \
                     machbar sind. Die Langzeiterinnerungen zeigen drei offene";
        // Stop-Button nie gesehen, kurzes Fenster erreicht: früher Complete.
        let fruch = classify_completion(prosa, true, false, false, 3.0, true);
        assert_eq!(fruch, Completion::Continue, "Prosa zu früh als fertig");
        // Auch die Stop-Button-Transition darf nutzlastfreien Text nicht
        // vorzeitig abschließen.
        let transition = classify_completion(prosa, true, true, false, 3.0, true);
        assert_eq!(transition, Completion::Continue);
        // Nach dem Prosa-Fenster ist es ein echter Regelbruch — dann Repair.
        let spaet = classify_completion(
            prosa,
            true,
            true,
            false,
            PROSE_STABILITY_SECONDS + 0.1,
            true,
        );
        assert_eq!(spaet, Completion::Complete);
    }

    #[test]
    fn own_prompt_is_not_mistaken_for_a_block_banner() {
        // Realfall 2026-07-21: eine Swarm-Frage zitierte eine Fehlerstatistik mit
        // dem Wort "Nachrichtenlimit". detect_block_banner liest die ganze Seite
        // — inklusive der eigenen Frage — und meldete 4 von 8 Brains als
        // blockiert. Die Blockade-Meldung enthielt woertlich den Fragetext.
        let prompt = "Messergebnis der letzten Runde: mistral brain_incomplete 0 Zyklen                       Anbieter-Nachrichtenlimit erreicht chatgpt running 0 Zyklen Run nie                       richtig angelaufen. Nenne 10 Verbesserungen.";
        let banner =
            "0 Zyklen Anbieter-Nachrichtenlimit erreicht chatgpt running 0 Zyklen Run nie richtig";
        assert!(banner_is_prompt_echo(banner, prompt));
    }

    #[test]
    fn a_real_provider_banner_still_counts_as_blocked() {
        // Gegenprobe: mistrals echtes Banner steht NICHT im Prompt und muss
        // weiterhin als Blockade durchgehen.
        let prompt = "Implementiere pub fn validate_brain_response in src/protocol.rs mit Tests.";
        let banner = "Mehr erfahren Nachrichtenlimit erreicht Sie haben Ihr Nachrichtenlimit erreicht. Ihr Limit wird in 9 Minuten zurueckgesetzt. Upgrade";
        assert!(!banner_is_prompt_echo(banner, prompt));
    }

    #[test]
    fn echo_check_ignores_whitespace_and_case_differences() {
        let prompt = "Zeile eins

  Anbieter-Nachrichtenlimit   ERREICHT beim Brain
";
        let banner = "anbieter-nachrichtenlimit erreicht beim brain";
        assert!(banner_is_prompt_echo(banner, prompt));
    }

    #[test]
    fn echo_check_is_inert_without_a_prompt() {
        // Ohne gesendeten Text (z.B. Vorab-Pruefung) darf nichts unterdrueckt
        // werden — sonst verschluckt der Filter echte Banner.
        assert!(!banner_is_prompt_echo("Nachrichtenlimit erreicht", ""));
    }

    #[test]
    fn short_incidental_overlap_does_not_suppress_a_banner() {
        // Ein einzelnes gemeinsames Wort darf nicht reichen, sonst wird jedes
        // Banner wegfiltert, sobald der Prompt zufaellig "Login" enthaelt.
        let prompt = "Bitte pruefe den Login-Flow in src/login.rs und ergaenze Tests.";
        let banner = "Please sign in to continue. Login required to use this service.";
        assert!(!banner_is_prompt_echo(banner, prompt));
    }

    #[test]
    fn technical_block_category_list_is_not_a_provider_banner() {
        let fragment = "ockierte Antworten (Tageslimit/Login/Cloudflare) \
                        zählen als Fehler statt als Beitrag";
        assert!(blocking::is_technical_block_phrase_list(fragment));
    }

    #[test]
    fn real_limit_banner_is_not_a_technical_category_list() {
        assert!(!blocking::is_technical_block_phrase_list(
            "Nachrichtenlimit erreicht. Ihr Limit wird in 9 Minuten zurückgesetzt."
        ));
    }
}
