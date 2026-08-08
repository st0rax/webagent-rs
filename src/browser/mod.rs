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

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::brain::{BrainBackend, SessionState};
use crate::observer::{is_claude_limit_response_text, is_transient_response_text};
use crate::page_driver::PageDriver;
use crate::protocol::is_possibly_truncated;
#[cfg(feature = "webview")]
use crate::webview_runtime::WebViewRuntime;

const STABILITY_SECONDS: f64 = 1.5;

/// Stabilitätsfenster für Text, der noch keine Protokoll-Nutzlast enthält.
/// Deutlich länger als `STABILITY_SECONDS`, weil ein Reasoning-Block zwischen
/// Denk- und Antwortphase sekundenlang stillstehen kann. Siehe
/// `classify_completion`.
const PROSE_STABILITY_SECONDS: f64 = 8.0;

/// Phrasen, die auf eine externe Blockierung hindeuten (Tages-/Nachrichtenlimit,
/// Login, Cloudflare) — DE+EN. Geteilt zwischen `detect_block_banner` (JS-Scan der
/// ganzen Seite) und `block_phrase_in_text` (reine Rust-Pruefung des bereits
/// gelesenen Antworttexts), damit beide dieselbe Liste verwenden.
/// Wie viele 250-ms-Runden auf den Absende-Beweis gewartet wird.
///
/// Bis zum 05.08.2026 waren es fest 12 Runden = 3 Sekunden, unabhaengig von der
/// Nachrichtenlaenge. Bei grossen Eingaben reicht das nicht: der Browser muss
/// erst eine riesige Eingabe verarbeiten und rendern, bevor ein Beweis (neue
/// Antwort, Stop-Knopf, URL-Wechsel) ueberhaupt entstehen kann.
///
/// Gemessen: eine 200.000-Zeichen-Nachricht an claude ging um 17:41 durch und
/// scheiterte um 18:58 an derselben Stelle — dazwischen lag nur Last auf der
/// Maschine. Ein zeitabhaengiger Fehlschlag sieht wie eine Ablehnung aus und
/// hat die Laengenmessung stundenlang in die Irre gefuehrt: gemeldet wurde
/// „Absenden fehlgeschlagen", gemeint war „zu frueh aufgegeben".
///
/// Eine Runde je 10.000 Zeichen obendrauf, gedeckelt bei 30 Sekunden — lieber
/// einmal laenger warten als eine Grenze erfinden, die es nicht gibt.
pub fn submit_verify_rounds(prompt_chars: usize) -> u32 {
    const BASE: u32 = 12;
    const MAX: u32 = 120;
    let extra = (prompt_chars / 10_000) as u32;
    BASE.saturating_add(extra).min(MAX)
}

/// Marker im Fehlertext, an dem Aufrufer eine Ablehnung per DEAKTIVIERTEM
/// Absendeknopf erkennen — im Unterschied zu einem echten Harness-Fehler.
///
/// Ein Marker im String ist die haessliche Loesung; richtig waere eine
/// Fehlervariante. Die kommt mit der thiserror-Umstellung, bis dahin ist ein
/// EINE Stelle definierender Marker besser als der Textvergleich, den sich
/// sonst jeder Aufrufer selbst zusammenbaut.
pub const SEND_DISABLED_MARKER: &str = "ABSENDEKNOPF_DEAKTIVIERT";

/// `true`, wenn der Fehler eine Ablehnung per deaktiviertem Absendeknopf ist.
pub fn is_send_disabled_error(message: &str) -> bool {
    message.contains(SEND_DISABLED_MARKER)
}

const BLOCK_PHRASES: &[&str] = &[
    "nachrichtenlimit",
    "message limit",
    "usage limit",
    "rate limit",
    "ratelimit",
    "daily limit",
    "tageslimit",
    "limit reached",
    "limit erreicht",
    "too many requests",
    "quota exceeded",
    "you have reached",
    "verify you are human",
    "checking your browser",
    // Kapazitaets-/Auslastungsmeldungen — anderes Muster als "Limit erreicht":
    // kein persoenliches Kontingent, sondern "der Dienst ist gerade ueberlastet".
    // Ausloeser fuer diese Ergaenzung: kimi zeigt unter Last einen Dialog/Overlay,
    // der den Composer blockiert; der genaue Wortlaut war zum Zeitpunkt des Fixes
    // nicht reproduzierbar (live_diagnose traf kimi im Ready-Zustand), daher eine
    // Bandbreite plausibler Formulierungen statt einer einzelnen bestaetigten Phrase.
    "too many users",
    "too many people",
    "high traffic",
    "currently busy",
    "server is busy",
    "at capacity",
    "zu viele nutzer",
    "zu viele anfragen",
    "überlastet",
    "derzeit ausgelastet",
    "cloudflare",
];

/// Prueft einen bereits gelesenen Antworttext (nicht die ganze Seite) auf eine
/// Block-Phrase. Faengt Faelle wie qwen, wo das Limit-Banner NICHT separat auf der
/// Seite steht, sondern als Text INNERHALB des Antwort-Containers erscheint — dort
/// sah `wait_response` es vorher nicht, weil der periodische Banner-Scan nur laeuft,
/// solange noch kein Text da ist, und ein bereits "vollstaendiger" Text-Block direkt
/// als echte Antwort durchgereicht wurde.
/// Obergrenze (Zeichen), bis zu der ein Text als Block-*Banner* gelten darf.
/// Echte Limit-/Auslastungs-Banner sind kurz (ein bis zwei Sätze). Eine lange
/// inhaltliche Antwort, die "rate limit"/"usage limit" nur ERWÄHNT (z.B. als
/// Verbesserungsvorschlag), ist KEIN Block — genau dieser False-Positive trat
/// im Swarm-Test "Verbesserungsvorschläge zu webagent-rs" auf: mistrals/
/// deepseeks legitime Essays empfahlen Rate-Limiting und wurden als "blocked"
/// verworfen.
const BLOCK_BANNER_MAX_CHARS: usize = 400;

/// Normalisiert Text für den Echo-Vergleich: Kleinschreibung, Whitespace
/// kollabiert — genau wie das JS die Seite einliest.
fn normalize_for_echo(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `true`, wenn der gefundene „Banner"-Ausschnitt in Wahrheit unsere eigene,
/// gerade gesendete Frage ist.
///
/// `detect_block_banner` liest `document.body.innerText` — also die GANZE Seite
/// samt der eben abgeschickten Nachricht. Enthält die Aufgabe selbst eines der
/// Stichwörter („Nachrichtenlimit", „Login", „Cloudflare"), meldet die Erkennung
/// jedes Brain als blockiert. Real passiert am 2026-07-21: eine Swarm-Frage, die
/// eine Fehlerstatistik zitierte, ließ 4 von 8 Brains fälschlich als „blocked"
/// gelten — die Blockade-Meldung enthielt wörtlich den Fragetext.
///
/// Der Ausschnitt ist ein Fenster um den Treffer (20 Zeichen davor, 120 danach)
/// und daher meist an beiden Rändern angeschnitten. Verglichen wird deshalb der
/// längste zusammenhängende Kern, nicht der Ausschnitt als Ganzes.
fn banner_is_prompt_echo(banner: &str, prompt: &str) -> bool {
    if prompt.trim().is_empty() {
        return false;
    }
    let hay = normalize_for_echo(prompt);
    let needle = normalize_for_echo(banner);
    if needle.is_empty() {
        return false;
    }
    if hay.contains(&needle) {
        return true;
    }
    // Ränder abschneiden: an Wortgrenzen von beiden Seiten einkürzen, bis ein
    // hinreichend langer Kern übrig ist, der im Prompt vorkommt.
    let words: Vec<&str> = needle.split(' ').collect();
    const MIN_CORE_WORDS: usize = 5;
    for start in 0..words.len() {
        for end in (start + MIN_CORE_WORDS..=words.len()).rev() {
            let core = words[start..end].join(" ");
            if core.chars().count() >= 25 && hay.contains(&core) {
                return true;
            }
        }
    }
    false
}

fn block_phrase_in_text(text: &str) -> Option<&'static str> {
    // Nur kurze Texte können ein Banner sein; in Fließtext ist die Phrase Inhalt.
    if text.chars().count() > BLOCK_BANNER_MAX_CHARS {
        return None;
    }
    let low = text.to_lowercase();
    BLOCK_PHRASES.iter().copied().find(|p| low.contains(p))
}

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
    rate_limit_aware: bool,
) -> Completion {
    // Die Rate-Limit-Erkennung ist Claude-spezifisch (`claude_rate_limited`) und wird
    // NUR fuer claude angewandt. Sonst schlug sie fuer andere Brains fehl: qwens
    // Ausgabe/UI-Chrome enthielt "…limit…", wurde faelschlich als Claude-Limit
    // gewertet und der (terminale) Rate-Limit-Pfad brach den Lauf ohne Retry ab.
    if rate_limit_aware && is_claude_limit_response_text(text) {
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

    // Antwort ohne jede Protokoll-Nutzlast braucht ein deutlich längeres
    // Stabilitätsfenster.
    //
    // Grund (gemessen am 29.07.2026): kimis `stop_button`-Selektoren sind
    // geratene `aria-label*='Stop'`-Muster und greifen nie. Damit fiel die
    // Entscheidung auf das kurze Stabilitätsfenster zurück — und das war
    // erreicht, sobald der Reasoning-Block fertig war und der eigentliche
    // Antwort-Block noch nicht begonnen hatte. Ergebnis: mitten im Stream
    // geerntete Prosa (186–292 Zeichen, mitten im Wort abgebrochen), die als
    // `protocol_invalid` verworfen wurde und einen Repair-Roundtrip kostete.
    // Über alle 108 Läufe des Tages waren so 145 von ~500 Brain-Turns (29 %)
    // reine Verschwendung — der größte Produktivitätsverlust im Dauerlauf.
    //
    // Im Protokollmodus ist Text ohne Nutzlast entweder Zwischenstand oder
    // Regelbruch. Beides verträgt ein paar Sekunden Warten; ein
    // Repair-Roundtrip kostet 10–35 s.
    if !has_protocol_payload(text) && stable_secs < PROSE_STABILITY_SECONDS {
        return Completion::Continue;
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

/// Enthält der Text überhaupt etwas, das eine Protokoll-Antwort sein könnte?
///
/// Bewusst grob: ein `{` oder ein `WEBAGENT/1`-Umschlag genügt. Es geht nicht
/// darum, ob das Protokoll gültig ist (das prüft `protocol::parse`), sondern ob
/// das Brain überhaupt angefangen hat zu antworten statt nur zu denken.
fn has_protocol_payload(text: &str) -> bool {
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
    brain_id: String,
    url: String,
    selectors: selectors::Selectors,
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    profile_dir: PathBuf,
    /// Optionales isoliertes Laufzeit-Profil (z.B. Swarm-Teilkopie). Überschreibt
    /// `profile_dir` in `start()`, sofern gesetzt. Wird nicht in `from_config`
    /// befüllt — explizit via `with_profile_override` gesetzt.
    #[cfg_attr(not(feature = "webview"), allow(dead_code))]
    profile_override: Option<PathBuf>,
    #[cfg(feature = "webview")]
    runtime: RefCell<Option<WebViewRuntime>>,
    driver: RefCell<Option<Box<dyn PageDriver>>>,
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
        driver.evaluate(expr).map_err(|e| e.to_string())
    }

    fn eval_bool(&self, expr: &str) -> bool {
        self.eval(expr)
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
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
        let expr = Self::js_scan(&list, "var n=QA(S[i]).length;if(n>0)return n;", "0");
        self.eval_i64(&expr) as i32
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
        let sels = self.sel(key);
        if sels.is_empty() {
            return false;
        }
        let expr = Self::js_scan(
            &Self::js_selectors(&sels),
            "var el=Q(S[i]);if(el){el.click();return true;}",
            "false",
        );
        self.eval_bool(&expr)
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
            Some(driver) => driver.click_at(x, y).is_ok(),
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

    /// Diagnose des echten DOM: wie viele Elemente matchen die konfigurierten
    /// Selektoren, welche Buttons/Kandidaten-Container gibt es? Deckt Selektor-Drift
    /// auf (der Hauptgrund, warum die Antworterkennung eine fertige Nachricht
    /// "nicht sieht").
    ///
    /// Grundlage der Fähigkeits-Vermessung, und zwar notgedrungen: Selbstauskunft
    /// der Brains ist dafür unbrauchbar — real getestet am 2026-07-27 gab
    /// deepseek die komplette abgefragte Liste zurück, inklusive Optionen, die
    /// seine Oberfläche gar nicht hat.
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
                "counts[{k:?}]=(function(){{var S={list};var n=0;for(var i=0;i<S.length;i++){{try{{n+=QA(S[i]).length;}}catch(e){{}}}}return n;}})();"
            ));
        }
        let expr = format!(
            r#"(function(){{
{prelude}
{counts_js}
// innerText haengt am Layout und ist headless haeufig leer — textContent nicht.
// Icon-only-Knoepfe (deepseek: div.ds-button--icon, svg, kein Text, kein
// aria-label) tragen ihren Namen anderswo: im <title>/<desc> des SVG, in der
// id, in data-*-Attributen oder am Elternelement. `ex` sammelt genau das,
// sonst ist so ein Knopf nicht identifizierbar.
function nm(el){{if(!el)return '';return ((el.getAttribute('aria-label')||'')+' '+(el.getAttribute('title')||'')+' '+(el.getAttribute('id')||'')).trim();}}
function inf(el){{
  var t=((el.innerText||el.textContent||'')+'').replace(/\s+/g,' ').trim();
  var ex=[];
  try{{var st=el.querySelector('svg title,svg desc');if(st)ex.push((st.textContent||'').trim());}}catch(e){{}}
  try{{var u=el.querySelector('svg use');if(u)ex.push(u.getAttribute('href')||u.getAttribute('xlink:href')||'');}}catch(e){{}}
  try{{for(var a=0;a<el.attributes.length;a++){{var at=el.attributes[a];if(at.name.indexOf('data-')===0||at.name.indexOf('aria-')===0)ex.push(at.name+'='+at.value);}}}}catch(e){{}}
  ex.push(el.getAttribute('id')||'');
  ex.push(nm(el.parentElement));
  return {{tag:el.tagName,cls:(el.className||'').toString().slice(0,90),al:el.getAttribute('aria-label')||'',ti:el.getAttribute('title')||'',dt:el.getAttribute('data-testid')||'',ex:ex.filter(function(s){{return s&&s.length;}}).join(' ').slice(0,160),svg:!!el.querySelector('svg'),tl:t.length,tp:t.slice(0,50)}};}}
var btns=[],seen=[];
['button','[role=button]','[role=switch]','[role=checkbox]','[role=menuitem]','[role=tab]','label','input[type=file]','[aria-label]','[data-testid]','[aria-pressed]','[aria-checked]','[aria-expanded]'].forEach(function(s){{
  try{{document.querySelectorAll(s).forEach(function(b){{
    if(seen.indexOf(b)>=0)return;
    // Verlaufseintraege der Seitenleiste sind keine Bedienelemente: sie tragen
    // frei getexteten Gespraechstitel und keinerlei Steuer-Attribut.
    var lab=(b.getAttribute('aria-label')||'')+(b.getAttribute('title')||'')+(b.getAttribute('data-testid')||'');
    var txt=((b.innerText||b.textContent||'')+'').replace(/\s+/g,' ').trim();
    if(!lab&&txt.length>28)return;
    seen.push(b);btns.push(inf(b));
  }});}}catch(e){{}}
}});
var msgs=[];document.querySelectorAll('[class*=message]').forEach(function(m){{msgs.push(inf(m));}});
var cand=[];['[data-message-author-role]','[data-testid]','.markdown','[class*=markdown]','[class*=message]','[class*=assistant]','[class*=chat]','div.prose','[class*=answer]','[class*=response]','[class*=bubble]'].forEach(function(s){{try{{var n=document.querySelectorAll(s).length;if(n>0)cand.push({{sel:s,n:n}});}}catch(e){{}}}});
var tb=[];document.querySelectorAll('div,p,article,section,li').forEach(function(e){{var t=(e.innerText||'').trim();if(t.length<40)return;var cm=0;for(var k=0;k<e.children.length;k++){{var ct=(e.children[k].innerText||'').length;if(ct>cm)cm=ct;}}if(cm<t.length*0.75){{tb.push(inf(e));}}}});tb.sort(function(a,b){{return b.tl-a.tl;}});
return {{url:location.href,title:document.title,w:window.innerWidth,h:window.innerHeight,wd:navigator.webdriver,ua:(navigator.userAgent||'').slice(0,90),counts:counts,buttons:btns.slice(0,200),messages:msgs.slice(0,20),candidates:cand,textblocks:tb.slice(0,8)}};
}})()"#,
            prelude = Self::JS_SEL_PRELUDE
        );
        self.eval(&expr)
    }

    /// Diagnose-Hilfe: beliebiges JS am aktiven Target auswerten. Nur für
    /// `examples/`/Tools zur Selektor-Analyse gedacht — nicht im Agentenpfad nutzen.
    pub fn eval_js(&self, expr: &str) -> Result<Value, String> {
        self.eval(expr)
    }

    fn is_cloudflare_blocked(&self) -> bool {
        let expr = r#"(function(){var u=location.href||"";if(u.indexOf("__cf_chl")>=0)return true;var t=(document.title||"").toLowerCase();return t.indexOf("just a moment")>=0||t.indexOf("nur einen moment")>=0;})()"#;
        self.eval_bool(expr)
    }

    fn dismiss_consent(&self) -> bool {
        let mut dismissed = self.click_first("consent_reject_button");
        // Konfigurierte Dialog-Schliesser (bisher tote Config — nie aufgerufen).
        dismissed |= self.click_first("dialog_dismiss_button");
        // Generischer Werbe-/Ankuendigungs-Modal-Schliesser. mistral warf z.B. ein
        // "Mistral Vibe CLI"-Announcement ueber den Composer, das jede Eingabe
        // blockierte — der Grund fuer konsistente mistral-Timeouts. Nur Buttons
        // INNERHALB offener Dialoge/Overlays, damit nichts Legitimes getroffen wird.
        dismissed |= self.dismiss_modal_buttons();
        if self.brain_id == "gemini" {
            dismissed |= self.click_first("notice_close_button");
        }
        if self.brain_id == "qwen" {
            dismissed |= self.dismiss_qwen_blocks();
        }
        dismissed
    }

    /// Schliesst Werbe-/Ankuendigungs-Modals: klickt einen „Spaeter/Later/Skip/Got
    /// it"-artigen Button, aber NUR innerhalb eines offenen Dialogs/Overlays
    /// (`[role=dialog]`, `[data-state=open]`, `*modal*`/`*overlay*`), damit auf der
    /// normalen Seite nichts faelschlich geklickt wird.
    fn dismiss_modal_buttons(&self) -> bool {
        self.eval_bool(
            r#"(function(){
              var hit=false;
              var scopes=document.querySelectorAll('[role=dialog],[data-state="open"],[class*="modal"],[class*="Modal"],[class*="overlay"],[class*="Overlay"]');
              var words=['später','spater','later','not now','maybe later','skip','got it','no thanks','dismiss','verstanden','vielleicht später','nur notwendige','nur notwendige cookies','notwendige','necessary','essenziell','accept necessary','reject all','ablehnen','erforderlich'];
              for(var s=0;s<scopes.length;s++){
                var btns=scopes[s].querySelectorAll('button,a,[role=button]');
                for(var i=0;i<btns.length;i++){
                  var t=(btns[i].innerText||btns[i].textContent||'').trim().toLowerCase();
                  if(!t||t.length>24)continue;
                  for(var w=0;w<words.length;w++){
                    if(t.indexOf(words[w])>=0){try{btns[i].click();hit=true;}catch(e){}break;}
                  }
                }
              }
              return hit;
            })()"#,
        )
    }

    /// qwen: „App herunterladen / not supported"-Banner schließen.
    fn dismiss_qwen_blocks(&self) -> bool {
        self.eval_bool(
            r#"(function(){
              var hit=false;
              document.querySelectorAll('button,a,[role=button]').forEach(function(el){
                var t=(el.textContent||'').toLowerCase();
                if(t.indexOf('continue on web')>=0||t.indexOf('use web')>=0||
                   t.indexOf('web version')>=0||t.indexOf('im browser')>=0){
                  try{el.click();hit=true;}catch(e){}
                }
              });
              return hit;
            })()"#,
        )
    }

    ///
    /// Noetig, weil `interactive_login` sofort mit "schon eingeloggt" abbricht, sobald
    /// `is_logged_in()` true meldet, und das ist zu optimistisch: die Pruefung genuegt
    /// sich mit einem sichtbaren Composer, den kimi und mistral auch anonym zeigen.
    /// Der Nutzer kaeme dort nie zum Anmelden. Deckt ausserdem den Fall ab, dass gar
    /// kein Login fehlt, sondern nur ein Dialog zu bestaetigen ist (mistral-AGB).
    ///
    /// Gibt das Tool selbst nichts ein — der Nutzer handelt, wir halten nur das Fenster.
    pub fn hold_window_open(&mut self, timeout: Duration) -> Result<(), String> {
        self.start(false)?; // headed
        let start = Instant::now();
        while start.elapsed() < timeout {
            // Verschwindet der Tab (Nutzer hat das Fenster geschlossen), schlaegt der
            // naechste Eval fehl — das ist unser Fertig-Signal.
            if self.eval("1").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        // Kurz warten, damit die Session ins Profil geflusht wird.
        std::thread::sleep(Duration::from_secs(2));
        let _ = self.stop();
        Ok(())
    }

    /// Liest den eingeloggten Account (E-Mail oder Anzeigename) aus der Seite —
    /// fuer den REPL-Startbanner (pi.dev-Stil). Bevorzugt eine E-Mail; sonst den
    /// Text/Titel eines User-/Account-/Avatar-Elements. `None`, wenn nichts Plausibles
    /// gefunden wird (z.B. nicht eingeloggt). Erst ein optionaler per-Brain-`account`-
    /// Selektor, dann generische Heuristik.
    pub fn account_label(&self) -> Option<String> {
        let js = self.account_label_js();
        let raw = self
            .eval(&js)
            .ok()
            .and_then(|v| v.as_str().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty() && s != "null")?;
        // Dedup doppelter Namen wie „storax storax" -> „storax".
        let words: Vec<&str> = raw.split_whitespace().collect();
        if words.len() == 2 && words[0].eq_ignore_ascii_case(words[1]) {
            return Some(words[0].to_string());
        }
        Some(raw)
    }

    /// Kurze Liste der sichtbaren Bedienelemente — als Beweismittel, wenn kein
    /// Stop-Button erkannt wurde.
    ///
    /// Erfasst bewusst auch Knoepfe ohne Text: der Stop-Knopf ist in diesen
    /// Oberflaechen meist ein reiner Icon-Button. Sein Name steckt dann im
    /// `<title>`/`<desc>` des SVG, in `id`, `data-testid` oder `aria-label` —
    /// deshalb werden genau diese Merkmale ausgegeben und nicht der Text.
    fn visible_button_inventory(&self) -> String {
        // Reihenfolge ist hier alles.
        //
        // Erster Versuch nahm die ersten 14 Knoepfe in DOM-Reihenfolge — und die
        // Seitenleiste steht im DOM vorn. Ergebnis waren fuenf Listen voller
        // "Suchen", "Seitenleiste schliessen" und "Weitere Optionen fuer
        // <Chattitel>", bei kimi elfmal "Mehr". Kein einziges Element aus dem
        // Eingabebereich, wo der Stop-Knopf sitzt: die Messung war wertlos.
        //
        // Der Stop-Knopf sitzt in allen diesen Oberflaechen unten am Composer.
        // Deshalb von unten nach oben, und Navigations-/Seitenleisten-Container
        // fliegen raus. Das ist generisch statt pro Anbieter geraten.
        let js = r#"(function(){
var cand=[],seen=[];
function imSeitenband(el){
  for(var p=el;p;p=p.parentElement){
    var tag=(p.tagName||'').toLowerCase();
    if(tag==='nav'||tag==='aside')return true;
    var c=((p.className||'')+'').toLowerCase();
    if(c.indexOf('sidebar')>=0||c.indexOf('side-bar')>=0||c.indexOf('history')>=0)return true;
    if((p.getAttribute&&(p.getAttribute('id')||'').toLowerCase().indexOf('sidebar')>=0))return true;
  }
  return false;
}
['button','[role=button]'].forEach(function(s){try{
document.querySelectorAll(s).forEach(function(b){
  if(seen.indexOf(b)>=0)return;seen.push(b);
  var r=b.getBoundingClientRect();if(r.width<=0||r.height<=0)return;
  if(imSeitenband(b))return;
  var svgt='';try{var st=b.querySelector('svg title,svg desc');if(st)svgt=(st.textContent||'').trim();}catch(e){}
  var id=b.getAttribute('id')||'',al=b.getAttribute('aria-label')||'',
      dt=b.getAttribute('data-testid')||'',
      cls=(b.className||'').toString().slice(0,160),
      t=((b.innerText||b.textContent||'')+'').replace(/\s+/g,' ').trim().slice(0,24);
  var parts=[];
  if(al)parts.push('aria='+al);
  if(dt)parts.push('testid='+dt);
  if(id)parts.push('id='+id);
  if(svgt)parts.push('svg='+svgt);
  if(t)parts.push('txt='+t);
  if(cls)parts.push('cls='+cls);
  if(parts.length)cand.push({y:r.top,s:'{'+parts.join(' ')+'}'});
});}catch(e){}});
// Unterste zuerst: der Composer und sein Stop-Knopf sitzen am Fuss der Seite.
cand.sort(function(a,b){return b.y-a.y;});
return cand.slice(0,12).map(function(c){return c.s;}).join(' ');})()"#;
        self.eval_str(js)
    }

    /// Baut das JS für [`Self::account_label`] — getrennt, damit prüfbar ist,
    /// dass es das Prelude mitbringt.
    ///
    /// Hohe Praezision statt Vollstaendigkeit: lieber `None` als ein
    /// Avatar-Alt-Text. (1) konfigurierter per-Brain-`account`-Selektor, (2) eine
    /// E-Mail irgendwo, (3) ein „angemeldet als X"/„signed in as X"-Muster.
    /// Sonst nichts.
    fn account_label_js(&self) -> String {
        let account_sels = self.sel_js("account", &[]);
        format!(
            r#"(function(){{{prelude}
function clean(t){{return (t||'').replace(/\s+/g,' ').trim();}}
var EMAIL=/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{{2,}}/i;
var cfg={account_sels};
for(var i=0;i<cfg.length;i++){{try{{var el=Q(cfg[i]);if(el){{var t=clean(el.getAttribute('title')||el.getAttribute('aria-label')||el.innerText||el.textContent);if(t)return t.slice(0,48);}}}}catch(e){{}}}}
var body=clean(document.body?document.body.innerText:'');
var m=body.match(EMAIL);
if(m)return m[0];
// E-Mail auch in Attributen (gemini: im aria-label des Konto-Links, nicht im Text).
var attrEls=document.querySelectorAll('[aria-label],[title],[alt]');
for(var a=0;a<attrEls.length;a++){{var s=(attrEls[a].getAttribute('aria-label')||'')+' '+(attrEls[a].getAttribute('title')||'')+' '+(attrEls[a].getAttribute('alt')||'');var mm=s.match(EMAIL);if(mm)return mm[0];}}
var sa=body.match(/(?:signed in as|angemeldet als|logged in as|account:)\s*([^\s,;|]{{2,40}})/i);
if(sa)return sa[1];
return null;}})()"#,
            prelude = Self::JS_SEL_PRELUDE,
            account_sels = account_sels
        )
    }

    pub fn interactive_login(&mut self, timeout: Duration) -> Result<bool, String> {
        self.start(false)?; // headed — Login erfordert Nutzerinteraktion
        let start = Instant::now();
        if self.is_logged_in() {
            std::thread::sleep(Duration::from_secs(1));
            let _ = self.stop();
            return Ok(true);
        }
        crate::bench_events::eprint_line(&format!(
            "[login] Browser geöffnet — bitte im Fenster bei '{}' anmelden. Warte auf Login…",
            self.brain_id
        ));
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

    /// Öffnet die Oberfläche und nimmt sie als PNG auf.
    ///
    /// Der Gegenentwurf zur DOM-Vermessung: was keinen Namen im DOM hat, hat
    /// trotzdem ein Bild. Gedacht als Vorlage für ein sehendes Brain — damit
    /// der Schwarm Oberflächen selbst vermisst, statt dass jemand die Optionen
    /// von Hand einträgt.
    pub fn live_screenshot(&mut self, headless: bool) -> Result<Vec<u8>, String> {
        self.live_screenshot_with(headless, None)
    }

    /// Wie `live_screenshot`, oeffnet vorher optional ein Menue.
    ///
    /// Ein geschlossenes Menue hat keine Eintraege im DOM — und auf einem
    /// Screenshot des Startbildschirms sieht man sie ebenso wenig. Wer wissen
    /// will, was hinter `model_menu` steckt, muss es aufklappen und DANN
    /// aufnehmen, statt Rollen-Selektoren zu raten.
    pub fn live_screenshot_with(
        &mut self,
        headless: bool,
        open_key: Option<&str>,
    ) -> Result<Vec<u8>, String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        self.wait_for_labeled_controls();
        if let Some(key) = open_key {
            if !self.click_first(key) {
                let _ = self.stop();
                return Err(format!("'{key}' nicht anklickbar"));
            }
            std::thread::sleep(Duration::from_millis(1200));
        }
        let shot = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            driver.capture_png().map_err(|e| e.to_string())
        };
        let _ = self.stop();
        shot
    }

    /// Wartet, bis die Oberflaeche beschriftete Bedienelemente zeigt.
    /// Ein Lade-Skelett bringt sofort Dutzende leerer Platzhalter mit; ein Scan
    /// darauf sah real 107 Elemente ohne einen einzigen Namen.
    fn wait_for_labeled_controls(&self) {
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            let labeled = self.eval_i64(
                "(function(){var n=0;document.querySelectorAll('button,[role=button],[aria-label],[data-testid]').forEach(function(e){var t=((e.innerText||e.textContent||'')+'').trim();if(e.getAttribute('aria-label')||e.getAttribute('title')||t)n++;});return n;})()",
            );
            if labeled >= 5 {
                std::thread::sleep(Duration::from_millis(1500));
                return;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    pub fn live_survey(&mut self, headless: bool) -> Result<Value, String> {
        self.live_survey_with(headless, None)
    }

    /// Wie `live_survey`, oeffnet vorher optional ein Menue — sonst fehlen
    /// dessen Eintraege im DOM und man raet ihre Selektoren.
    pub fn live_survey_with(
        &mut self,
        headless: bool,
        open_key: Option<&str>,
    ) -> Result<Value, String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        // Der Titel steht, lange bevor die SPA ihren DOM aufgebaut hat: ein
        // sofortiger Scan sah 0 Buttons auf einer korrekt geladenen Seite.
        self.wait_for_labeled_controls();
        if let Some(key) = open_key {
            if !self.click_first(key) {
                let _ = self.stop();
                return Err(format!("'{key}' nicht anklickbar"));
            }
            std::thread::sleep(Duration::from_millis(1200));
        }
        let report = self.dom_report();
        let _ = self.stop();
        report
    }

    /// Oberflaechen-Analyse wie die Link-Analyse in JDownloader: die offene
    /// Seite nach Bedienelementen scannen und sie per [`crate::brain_probe`]
    /// deutet. Der Aufrufer entscheidet, ob die Funde als Selektoren
    /// uebernommen werden.
    pub fn probe_surface(&mut self, headless: bool) -> Result<Vec<crate::brain_probe::Proposal>, String> {
        let (_, proposals) = self.probe_surface_with_raw(headless, None)?;
        Ok(proposals)
    }

    /// Wie [`probe_surface`], gibt aber auch die rohen DOM-Kandidaten mit
    /// zurueck — fuer die Analyse von Fehlfunden (Warum wurde der Absende-
    /// Knopf nicht erkannt?).
    pub fn probe_surface_with_raw(
        &mut self,
        headless: bool,
        open_key: Option<&str>,
    ) -> Result<(Vec<crate::brain_probe::Candidate>, Vec<crate::brain_probe::Proposal>), String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        self.wait_for_labeled_controls();
        let result = self.scan_once();
        let opened = match (open_key, result) {
            (Some(key), Ok((cands, props))) => {
                let clicked = props.iter().find(|p| p.selector_key == key).map(|p| {
                    let expr = crate::browser::js::js_scan(
                        &crate::browser::js::js_selectors(&[p.selector.clone()]),
                        "var el=Q(S[i]);if(el){el.click();return true;}",
                        "false",
                    );
                    self.eval_bool(&expr)
                });
                match clicked {
                    Some(true) => {
                        eprintln!("[probe] {key} geklickt — scanne nach den Menue-Eintraegen…");
                        std::thread::sleep(Duration::from_millis(1500));
                        self.scan_once()
                    }
                    _ => {
                        eprintln!("[probe] {key}: nichts anzuklicken ({})", cands.len());
                        Ok((cands, props))
                    }
                }
            }
            (None, r) => r,
            (_, Err(e)) => Err(e),
        };
        let _ = self.stop();
        opened
    }

    /// Ein einzelner Scan gegen die offene Seite (Browser muss laufen).
    fn scan_once(
        &self,
    ) -> Result<(Vec<crate::brain_probe::Candidate>, Vec<crate::brain_probe::Proposal>), String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        let raw = driver
            .evaluate(crate::brain_probe::PROBE_SCRIPT)
            .map_err(|e| e.to_string())?;
        let candidates: Vec<crate::brain_probe::Candidate> =
            serde_json::from_value(raw).unwrap_or_default();
        let proposals = crate::brain_probe::classify(&candidates);
        Ok((candidates, proposals))
    }

    /// Faehrt einen Vorschlag aus [`probe_surface`] live an der offenen Seite:
    /// klicken, messbarer Zustandswechsel als Beleg, Rueckweg wiederherstellen.
    /// Passt zu `probe_surface`, weil dort derselbe (eigene) Browser laeuft.
    pub fn verify_surface(
        &mut self,
        headless: bool,
        proposal: &crate::brain_probe::Proposal,
    ) -> Result<crate::brain_probe::Verdict, String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        self.wait_for_labeled_controls();
        let verdict = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            crate::brain_probe::verify(driver.as_mut(), proposal).map_err(|e| e.to_string())
        };
        let _ = self.stop();
        verdict
    }

    /// Oberflaechen-Analyse wie [`probe_surface`], aber mit einer zweiten Runde:
    /// wird ein Composer gefunden, aber kein Absende-Knopf, fuellt ein
    /// Testwort den Editor und scannt erneut. Viele SPAs (z.B. Perplexity)
    /// rendern den Send-Button erst, wenn Text drinsteht.
    ///
    /// `open_key`: nach dem ersten Scan diesen Vorschlag anklicken und einen
    /// weiteren Scan ausloesen — Menue-Eintraege (`model_option` etc.) sind
    /// erst sichtbar, wenn das Menue offen ist.
    pub fn probe_surface_with_fill(
        &mut self,
        headless: bool,
        open_key: Option<&str>,
    ) -> Result<(Vec<crate::brain_probe::Candidate>, Vec<crate::brain_probe::Proposal>), String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        self.wait_for_labeled_controls();
        let first = self.scan_once();
        let has_composer = first
            .as_ref()
            .map_or(false, |(_, p)| p.iter().any(|p| p.selector_key == "composer"));
        let has_send = first
            .as_ref()
            .map_or(false, |(_, p)| p.iter().any(|p| p.selector_key == "send_button"));
        let result = if has_composer && !has_send {
            eprintln!(
                "[probe] Composer gefunden, Send-Button nicht — fuelle Editor und scanne erneut…"
            );
            let fill = "(function(){var el=document.querySelector('#ask-input')||document.querySelector('[contenteditable=true]')||document.querySelector('textarea');if(!el)return false;el.focus();if(el.isContentEditable){if(document.execCommand&&document.execCommand('insertText',false,'test')){return true;}el.innerText='test';var ev=new Event('input',{bubbles:true});el.dispatchEvent(ev);return true;}if(el.tagName==='TEXTAREA'||el.tagName==='INPUT'){var set=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value')||Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value');if(set&&set.set){set.set.call(el,'test');var ev2=new Event('input',{bubbles:true});el.dispatchEvent(ev2);return true;}}return false;})()";
            let _ = self.eval_js(fill);
            std::thread::sleep(Duration::from_millis(1200));
            self.scan_once()
        } else {
            first
        };
        if let Some(key) = open_key {
            let opened = match result {
                Ok((cands, props)) => {
                    let clicked = props.iter().find(|p| p.selector_key == key).map(|p| {
                        let expr = crate::browser::js::js_scan(
                            &crate::browser::js::js_selectors(&[p.selector.clone()]),
                            "var el=Q(S[i]);if(el){el.click();return true;}",
                            "false",
                        );
                        self.eval_bool(&expr)
                    });
                    match clicked {
                        Some(true) => {
                            eprintln!("[probe] {key} geklickt — scanne nach den Menue-Eintraegen…");
                            std::thread::sleep(Duration::from_millis(1500));
                            self.scan_once()
                        }
                        _ => {
                            eprintln!("[probe] {key}: nichts anzuklicken ({})", cands.len());
                            Ok((cands, props))
                        }
                    }
                }
                Err(e) => Err(e),
            };
            let _ = self.stop();
            return opened;
        }
        let _ = self.stop();
        result
    }
    /// Composer-/Assistant-Selektoren und Cloudflare, und schließt wieder. Deckt
    /// Selektor-Drift auf, die `doctor` (read-only) nicht sehen kann.
    pub fn live_diagnose(&mut self, headless: bool) -> Result<LiveDiagnosis, String> {
        self.live_diagnose_with_shot(headless, false)
            .map(|(d, _)| d)
    }

    /// Diagnose und optional ein Bildschirmfoto in **einer** Sitzung.
    ///
    /// Vorher brauchte die Startübersicht einen Browserstart und die
    /// Bilderwand einen zweiten — bei acht Brains also sechzehn. Beides liest
    /// denselben Zustand derselben Seite; es einmal zu öffnen genügt.
    pub fn live_diagnose_with_shot(
        &mut self,
        headless: bool,
        with_shot: bool,
    ) -> Result<(LiveDiagnosis, Option<Vec<u8>>), String> {
        self.start(headless)?;
        self.dismiss_consent();
        let session_state = self.ensure_ready(15.0).unwrap_or(SessionState::Error);
        let diag = LiveDiagnosis {
            brain_id: self.brain_id.clone(),
            url: self.get_conversation_ref().unwrap_or_default(),
            cloudflare: self.is_cloudflare_blocked(),
            logged_in: self.is_logged_in(),
            login_button_visible: self.any_visible("login_button"),
            composer_found: self.any_visible("composer"),
            assistant_count: self.assistant_count(),
            session_state,
        };
        let shot = if with_shot {
            // Fehlschlaege beim Bild duerfen die Diagnose nicht wegwerfen —
            // der Zustand ist die wichtigere Information.
            let mut guard = self.driver.borrow_mut();
            guard.as_mut().and_then(|d| d.capture_png().ok())
        } else {
            None
        };
        let _ = self.stop();
        Ok((diag, shot))
    }

    fn send_generic(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        if self.sel("composer").is_empty() {
            return Err("Keine Composer-Selektoren konfiguriert".into());
        }
        // Werbe-/Consent-Modals wegklicken, bevor gefuellt wird — sonst blockiert
        // z.B. mistrals "Vibe CLI"-Announcement den Composer und jeder Versuch scheitert.
        self.dismiss_consent();
        let composer_js = self.sel_js("composer", &[]);
        let has_send_button = !self.sel("send_button").is_empty();
        // Fuellen und **bestaetigen**, dass der Text wirklich im Editor steht: bei
        // kimis Lexical-Editor meldete fill_composer frueher Erfolg, obwohl das Feld
        // leer blieb — dann ging Enter ins Leere und verify_submitted meldete
        // faelschlich "abgeschickt". Vor jedem Fuell-Versuch nochmal Modals schliessen.
        if !self.wait_fill_composer(&composer_js, text, |s, js, t| {
            s.dismiss_consent();
            s.fill_composer(js, t);
            s.composer_contains(js, t)
        }) {
            return Err("Composer-Feld nicht gefunden (Timeout)".into());
        }
        std::thread::sleep(Duration::from_millis(150));
        let url_before = self.get_conversation_ref();
        // Fuenf Versuche statt drei: das Absenden in Lexical-/contenteditable-Editoren
        // (kimi) greift pro Versuch nur ~zur Haelfte; jeder weitere Versuch, der bei
        // Erfolg gar nicht erst laeuft, hebt die Zuverlaessigkeit deutlich. Bei einem
        // wirklich blockierten Composer (mistral-Dialog) scheitern trotzdem alle.
        for attempt in 0..5 {
            // Vor jedem Absende-Versuch sicherstellen, dass der Text (noch) drinsteht.
            if !self.composer_contains(&composer_js, text) {
                let _ = self.fill_composer(&composer_js, text);
            }
            if attempt == 0 || !has_send_button {
                self.press_enter().ok();
            } else if !self.click_visible_real("send_button") {
                self.click_first("send_button");
            }
            if self.verify_submitted(baseline, url_before.as_deref()) {
                return Ok(baseline);
            }
        }
        // Frueher: `Ok(baseline)`, auch wenn jeder Versuch scheiterte — der Aufrufer
        // lief dann in den vollen wait_response-Timeout (150s Stille). Jetzt ehrlicher
        // Fehler: es kam kein Absende-Beweis (URL-Wechsel / Stop-Button / neue
        // Antwort). Ursache ist meist ein blockierender Dialog/Overlay ueber dem
        // Composer -- z.B. kimis "gerade zu viele Nutzer"-Kapazitaetsmeldung. Statt
        // das nur zu vermuten: die Seite nach einem bekannten Block-Text absuchen und
        // den tatsaechlichen Text melden, falls vorhanden, damit der naechste
        // Auftritt im Log/`/score` diagnostizierbar ist statt ein Ratespiel zu bleiben.
        Err(self.submit_failed_error(5))
    }

    /// Einheitlicher Fehler, wenn kein Absende-Beweis (URL-Wechsel / Stop-Button /
    /// neue Antwort) zustande kam. WICHTIG: alle send_*-Funktionen MÜSSEN diesen
    /// Fehler zurückgeben statt `Ok(baseline)`, sonst hält `wait_response` den
    /// stehengebliebenen (oft STALE) Bildschirmtext für die Antwort — genau die
    /// Konversations-Vergiftung, die gemini/deepseek im Swarm zeigten
    /// ("gemini lebt um 11:19:06" aus einem alten Chat).
    fn submit_failed_error(&self, attempts: u32) -> String {
        if let Some(banner) = self.detect_block_banner() {
            return format!(
                "blockiert: kein Absende-Beweis nach {attempts} Versuchen -- Seite zeigt: {banner}"
            );
        }
        // Keine BEKANNTE Phrase getroffen. Frueher endete die Meldung hier — und
        // damit war das Entscheidende verschwiegen: WAS steht denn da?
        //
        // Real beobachtet am 05.08.2026 bei der Laengenmessung von claude: ab
        // 400.000 Zeichen schlug das Absenden fuenfmal fehl, Grund unbekannt.
        // Womoeglich war es genau die gesuchte Laengenablehnung, nur anders
        // formuliert als die Phrasenliste sie kennt. Ohne den Text laesst sich
        // die Liste nie erweitern, und die Messung raet weiter.
        // Erst die naheliegendste Frage beantworten: steht der Text ueberhaupt
        // im Composer? Am 05.08.2026 scheiterte das Absenden bei claude und
        // mistral reproduzierbar ab 400.000 Zeichen, und die Meldung behauptete
        // einen blockierenden Dialog. Es gab keinen — der Editor hatte den Text
        // schlicht nicht angenommen. Eine Vermutung als Ursache auszugeben, hat
        // die Suche in die falsche Richtung geschickt.
        let intended = self.last_sent.borrow().chars().count();
        let actual = self.composer_char_count();
        if intended > 0 && actual + actual / 10 < intended {
            return format!(
                "Absenden fehlgeschlagen nach {attempts} Versuchen: der Composer enthaelt \
                 nur {actual} von {intended} Zeichen — die Eingabe wurde von der \
                 Oberflaeche gekuerzt oder gar nicht uebernommen, es ist KEINE Blockade"
            );
        }
        // Eine Laengenablehnung muss kein Text sein. Am 05.08.2026 stand bei
        // mistral und claude ab 400.000 Zeichen die Eingabe VOLLSTAENDIG im
        // Composer, es gab keinen Dialog, und trotzdem ging nichts raus — bei
        // 200.000 dagegen reibungslos. Die Oberflaeche deaktiviert schlicht den
        // Absendeknopf. `looks_like_length_rejection` sucht in TEXTEN und kann
        // eine Ablehnung, die nur ein grauer Knopf ist, nie sehen.
        if self.send_button_disabled() == Some(true) {
            return format!(
                "{SEND_DISABLED_MARKER}: Absendeknopf ist deaktiviert, obwohl der Text \
                 vollstaendig im Composer steht ({attempts} Versuche) — die Oberflaeche \
                 verweigert das Absenden ohne Meldung"
            );
        }
        if let Some(overlay) = self.blocking_dialog_text() {
            return format!(
                "Absenden fehlgeschlagen: kein Absende-Beweis nach {attempts} Versuchen. \
                 Unbekannter Dialog ueber dem Composer, Text: \"{overlay}\" \
                 -- falls das eine Blockade ist, gehoert die Formulierung in BLOCK_PHRASES"
            );
        }
        format!(
            "Absenden fehlgeschlagen: kein Absende-Beweis nach {attempts} Versuchen \
             (Text steht vollstaendig im Composer, kein Dialog gefunden — moeglich \
             sind ein deaktivierter Absendeknopf oder eine stumm verworfene Eingabe)"
        )
    }

    /// Ist der Absendeknopf deaktiviert? `None` = kein Knopf gefunden.
    ///
    /// Prueft `disabled`, `aria-disabled` und `pointer-events: none` — die drei
    /// Arten, auf die Weboberflaechen einen Knopf stilllegen.
    fn send_button_disabled(&self) -> Option<bool> {
        let list = Self::js_selectors(&self.sel("send_button"));
        let js = Self::js_scan(
            &list,
            "var el=Q(S[i]);if(el){var b=el.closest('button')||el;\
             var st=window.getComputedStyle(b);\
             return (b.disabled===true)||b.getAttribute('aria-disabled')==='true'\
             ||st.pointerEvents==='none';}",
            "null",
        );
        self.eval(&js).ok().and_then(|v| v.as_bool())
    }

    /// Wie viele Zeichen stehen tatsaechlich im Composer?
    ///
    /// Der billigste Weg, „Eingabe kam gar nicht an" von „Absenden blockiert"
    /// zu unterscheiden — und genau diese Unterscheidung fehlte.
    fn composer_char_count(&self) -> usize {
        let list = Self::js_selectors(&self.sel("composer"));
        let js = Self::js_scan(
            &list,
            "var el=Q(S[i]);if(el){return (el.value!==undefined?el.value:(el.innerText||'')).length;}",
            "0",
        );
        self.eval(&js)
            .ok()
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize
    }

    /// Text eines echten Dialogs ueber dem Composer, gekuerzt.
    ///
    /// Absichtlich OHNE Phrasenliste — hier geht es um den Fall, dass die Liste
    /// den Text nicht kennt. Aber MIT Dialog-Semantik: ein erster Versuch ueber
    /// „feste Positionierung + hoher z-index" fing bei mistral die Seitenleiste
    /// („Vibe Chat Work Code Neuer Chat Agenten …") und meldete sie als
    /// blockierenden Dialog. Eine Diagnose, die das Falsche zeigt, ist
    /// schlimmer als eine, die schweigt.
    fn blocking_dialog_text(&self) -> Option<String> {
        let js = r#"(function(){
var nodes=document.querySelectorAll('[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open]');
var best='';
for(var i=0;i<nodes.length;i++){
  var e=nodes[i];var r=e.getBoundingClientRect();
  if(r.width<40||r.height<20)continue;
  var st=window.getComputedStyle(e);
  if(st.visibility==='hidden'||st.display==='none')continue;
  // Navigation ist kein Dialog, auch wenn sie modal aussieht.
  if(e.closest('nav,aside,[role=navigation]'))continue;
  var links=e.querySelectorAll('a,[role=link]').length;
  if(links>5)continue;
  var t=(e.innerText||'').replace(/\s+/g,' ').trim();
  if(t.length>best.length)best=t;
}
return best?best.slice(0,300):null;})()"#;
        let value = self.eval(js).ok()?;
        let text = value.as_str()?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        let sent = self.last_sent.borrow().clone();
        if banner_is_prompt_echo(&text, &sent) {
            return None;
        }
        Some(text)
    }

    /// Text eines sichtbaren Dialogs/Overlays ueber dem Composer, gekuerzt.
    ///
    /// Absichtlich OHNE Phrasenliste: hier geht es genau um den Fall, dass die
    /// Liste den Text NICHT kennt. Gesucht wird deshalb nach der Bauform —
    /// `role=dialog`, `aria-modal`, oder ein Element mit fester/absoluter
    /// Positionierung und hohem z-index —, nicht nach dem Inhalt.
    fn visible_overlay_text(&self) -> Option<String> {
        let js = r#"(function(){
var sel='[role=dialog],[role=alertdialog],[aria-modal=true],dialog[open]';
var best='';
var nodes=document.querySelectorAll(sel);
for(var i=0;i<nodes.length;i++){
  var e=nodes[i];var r=e.getBoundingClientRect();
  if(r.width<40||r.height<20)continue;
  var t=(e.innerText||'').replace(/\s+/g,' ').trim();
  if(t.length>best.length)best=t;
}
if(!best){
  var all=document.body?document.body.querySelectorAll('*'):[];
  for(var j=0;j<all.length;j++){
    var el=all[j];var st=window.getComputedStyle(el);
    if(st.position!=='fixed'&&st.position!=='absolute')continue;
    if(parseInt(st.zIndex||'0',10)<10)continue;
    var rr=el.getBoundingClientRect();
    if(rr.width<120||rr.height<40)continue;
    if(st.visibility==='hidden'||st.display==='none'||parseFloat(st.opacity||'1')<0.2)continue;
    var tt=(el.innerText||'').replace(/\s+/g,' ').trim();
    if(tt.length>10&&tt.length<600&&tt.length>best.length)best=tt;
  }
}
return best?best.slice(0,300):null;})()"#;
        let value = self.eval(js).ok()?;
        let text = value.as_str()?.trim().to_string();
        if text.is_empty() {
            return None;
        }
        // Das Echo der eigenen Frage ist kein fremder Dialog.
        let sent = self.last_sent.borrow().clone();
        if banner_is_prompt_echo(&text, &sent) {
            return None;
        }
        Some(text)
    }

    fn send_gemini(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        self.handle_interruptions();
        let composer_js = self.sel_js("composer", &[]);
        if !self.wait_fill_composer(&composer_js, text, |s, js, t| {
            s.fill_composer_dom_set(js, t)
        }) {
            let _ = self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.fill_composer(js, t) && s.type_text_char_by_char(t).is_ok()
            });
        }
        std::thread::sleep(Duration::from_millis(200));
        let url_before = self.get_conversation_ref();
        for _ in 0..3 {
            if self.click_visible_real("send_button") || self.click_first("send_button") {
                std::thread::sleep(Duration::from_millis(400));
            }
            if self.verify_submitted(baseline, url_before.as_deref()) {
                return Ok(baseline);
            }
            let _ = self.fill_composer_dom_set(&composer_js, text);
        }
        // Kein Ok(baseline) bei ausbleibendem Absende-Beweis (Vergiftungsquelle) —
        // ehrlicher Fehler wie in send_generic.
        Err(self.submit_failed_error(3))
    }

    fn send_qwen(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        let _ = self.dismiss_qwen_blocks();
        let composer_js = self.sel_js("composer", &[]);
        if !self.wait_fill_composer(&composer_js, text, |s, js, t| s.fill_composer(js, t))
            && !self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.fill_composer_dom_set(js, t)
            })
        {
            return Err("Composer-Feld nicht gefunden (Timeout)".into());
        }
        std::thread::sleep(Duration::from_millis(300));
        let url_before = self.get_conversation_ref();
        for attempt in 0..4 {
            if attempt % 2 == 0 {
                if !self.click_visible_real("send_button") {
                    self.click_first("send_button");
                }
            } else {
                self.press_enter().ok();
            }
            if self.verify_submitted(baseline, url_before.as_deref()) {
                return Ok(baseline);
            }
            let _ = self.fill_composer(&composer_js, text);
        }
        // Kein Ok(baseline) bei ausbleibendem Absende-Beweis (Vergiftungsquelle) —
        // ehrlicher Fehler wie in send_generic.
        Err(self.submit_failed_error(4))
    }

    fn prepare_send_baseline(&mut self) -> i32 {
        let baseline = self.assistant_count();
        let bt = if baseline > 0 {
            self.assistant_text(baseline - 1)
        } else {
            String::new()
        };
        *self.baseline_text.borrow_mut() = bt;
        baseline
    }

    fn wait_fill_composer<F>(&self, composer_js: &str, text: &str, fill: F) -> bool
    where
        F: Fn(&Self, &str, &str) -> bool,
    {
        let deadline = Instant::now() + Duration::from_secs(12);
        while Instant::now() < deadline {
            if fill(self, composer_js, text) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        false
    }

    /// Wartet darauf, dass ein Absende-**Beweis** erscheint. `url_before` ist die URL
    /// vor dem Absenden.
    ///
    /// Ein leerer Composer allein ist **kein** Beweis: das Fuellen von Lexical-/
    /// contenteditable-Editoren (kimi) schlaegt manchmal still fehl, dann ist das Feld
    /// leer, obwohl nie etwas raus ging — `verify_submitted` meldete dann faelschlich
    /// Erfolg, und der Aufrufer lief in den vollen `wait_response`-Timeout. Echte
    /// Signale: die Seite navigiert in einen Chat (URL-Wechsel), ein Stop-Button
    /// erscheint, oder der Assistant-Zaehler waechst. Composer-leer zaehlt nur noch
    /// **zusammen** mit einem dieser Signale (gegen Fehlalarm), nicht fuer sich.
    /// Sucht auf der Seite nach einem **Block-Banner** (Rate-/Nachrichten-/Tageslimit,
    /// Login, Cloudflare) und gibt dessen Text zurueck. Nur aufrufen, wenn KEINE echte
    /// Antwort erkannt wurde — dann ist so ein Banner ein starkes Block-Indiz, kein
    /// False Positive. Diese Banner stehen auf der Seite (mistral:
    /// „Nachrichtenlimit erreicht", qwen: „daily usage limit"), NICHT im Antworttext,
    /// darum sieht sie eine reine Antwort-Text-Pruefung nicht.
    fn detect_block_banner(&self) -> Option<String> {
        let sent = self.last_sent.borrow().clone();
        self.detect_block_banner_excluding(&sent)
    }

    /// Wie `detect_block_banner`, ignoriert aber Treffer, die nur das Echo der
    /// gerade gesendeten Nachricht sind (siehe `banner_is_prompt_echo`).
    fn detect_block_banner_excluding(&self, sent: &str) -> Option<String> {
        let pats_js = BLOCK_PHRASES
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(",");
        let js = format!(
            r#"(function(){{
var b=(document.body?document.body.innerText:'').replace(/\s+/g,' ');
var low=b.toLowerCase();
var pats=[{pats_js}];
for(var i=0;i<pats.length;i++){{var k=low.indexOf(pats[i]);if(k>=0){{return b.slice(Math.max(0,k-20),k+120);}}}}
return null;}})()"#
        );
        let v = self.eval(&js).ok()?;
        let banner = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        if banner_is_prompt_echo(&banner, sent) {
            // Unsere eigene Frage, kein Banner des Anbieters.
            return None;
        }
        Some(banner)
    }

    fn verify_submitted(&self, baseline: i32, url_before: Option<&str>) -> bool {
        let rounds = submit_verify_rounds(self.last_sent.borrow().chars().count());
        for _ in 0..rounds {
            std::thread::sleep(Duration::from_millis(250));
            let url_changed = match (url_before, self.get_conversation_ref()) {
                (Some(before), Some(now)) => now != before,
                _ => false,
            };
            if self.assistant_count() > baseline || self.any_visible("stop_button") || url_changed {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absende_wartezeit_waechst_mit_der_nachrichtenlaenge() {
        // Kurze Nachricht: unveraendert 3 Sekunden.
        assert_eq!(submit_verify_rounds(0), 12);
        assert_eq!(submit_verify_rounds(500), 12);
        // 200.000 Zeichen: 12 + 20 Runden = 8 Sekunden. Genau dieser Fall
        // scheiterte am 05.08.2026 unter Last, obwohl er eine Stunde vorher
        // durchging - der Beweis kam nur nach den festen 3 Sekunden.
        assert_eq!(submit_verify_rounds(200_000), 32);
        // Eine Million Zeichen: 12 + 100 Runden = 28 Sekunden, noch unter dem Deckel.
        assert_eq!(submit_verify_rounds(1_000_000), 112);
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
    fn from_config_loads_selectors_and_url() {
        let backend = WebBrainBackend::from_config("chatgpt").expect("chatgpt config");
        assert_eq!(backend.brain_id(), "chatgpt");
        assert_eq!(backend.url, "https://chatgpt.com/");
        assert!(!backend.sel("composer").is_empty());
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
}
