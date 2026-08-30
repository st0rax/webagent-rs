//! Die `BrainBackend`-Umsetzung des WebView-Backends: Sitzungssteuerung,
//! Senden, Antwort abwarten, Login- und Konversationszustand.
//!
//! Ein vollstaendiger Trait-Block ist die natuerlichste Schnittkante, die es in
//! `browser` gibt — er ist per Definition in sich geschlossen. Als Kindmodul
//! erreicht er die privaten Interna des Backends ohne jede
//! Sichtbarkeitsaenderung.

use super::{block_phrase_in_text, classify_completion, Completion, SessionState, WebBrainBackend};
use crate::brain::{BrainBackend, BrainResponse};
// Beide nur mit `webview`: ohne das Feature gibt es weder das Modul noch einen
// Aufrufer, und die CI baut mit `--no-default-features`.
#[cfg(feature = "webview")]
use crate::page_driver::PageDriver;
#[cfg(feature = "webview")]
use crate::webview_runtime::WebViewRuntime;
use std::time::{Duration, Instant};

impl BrainBackend for WebBrainBackend {
    fn brain_id(&self) -> &str {
        &self.brain_id
    }

    fn start(&mut self, headless: bool) -> Result<(), String> {
        #[cfg(not(feature = "webview"))]
        {
            let _ = headless;
            Err(crate::page_driver::webview_unavailable().to_string())
        }
        #[cfg(feature = "webview")]
        {
            // Shared-Pool nur ohne Override. Mit profile_override (Swarm-Kopie)
            // nie den Pool-Tab recyclen — sonst landet man wieder im Shared-
            // Profil (SingletonLock / Session-Mix). Eigener Runtime-Pfad.
            if crate::config::use_shared_browser() && self.profile_override.is_none() {
                return crate::browser_pool::BrowserPool::global()
                    .lock()
                    .map_err(|_| "BrowserPool-Sperre verloren".to_string())?
                    .start_brain_resilient(self, headless, None);
            }
            let profile = self.effective_profile_dir().clone();
            let runtime = WebViewRuntime::launch(&profile, headless).map_err(|e| e.to_string())?;
            let mut driver = runtime
                .open_page(&profile, &self.url, headless, &self.brain_id)
                .map_err(|e| e.to_string())?;
            let nav_start = Instant::now();
            let nav_timeout = Duration::from_secs(15);
            driver.navigate(&self.url, nav_timeout).map_err(|e| {
                let elapsed = nav_start.elapsed();
                format!(
                    "Navigation timeout after {:.2}s to {} (limit 15s): {}",
                    elapsed.as_secs_f64(),
                    self.url,
                    e
                )
            })?;
            *self.runtime.borrow_mut() = Some(runtime);
            *self.driver.borrow_mut() = Some(Box::new(driver));
            Ok(())
        }
    }

    fn stop(&mut self) -> Result<(), String> {
        #[cfg(not(feature = "webview"))]
        {
            *self.driver.borrow_mut() = None;
            Ok(())
        }
        #[cfg(feature = "webview")]
        {
            // Mirror start(): Shared-Pool nur ohne Override. Mit profile_override
            // (Swarm-Kopie) haben wir eine eigene Runtime — die muss hier fallen,
            // sonst bleibt der WebView-Prozess hängen und Isolation bricht.
            if crate::config::use_shared_browser() && self.profile_override.is_none() {
                *self.driver.borrow_mut() = None;
                return crate::browser_pool::BrowserPool::global()
                    .lock()
                    .map_err(|_| "BrowserPool-Sperre verloren".to_string())?
                    .stop_brain(&self.brain_id, None);
            }
            *self.driver.borrow_mut() = None;
            *self.runtime.borrow_mut() = None;
            Ok(())
        }
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
        if self.driver.borrow().is_none() {
            return SessionState::Error;
        }
        // Verbindungs-Liveness: schlägt ein triviales Eval fehl, ist die
        // Seite/WebView-Verbindung tot — das ist ein Fehler, kein fehlender Login.
        if self.eval("1").is_err() {
            return SessionState::Error;
        }
        if self.is_cloudflare_blocked() {
            return SessionState::Cloudflare;
        }
        // Zwei verschiedene Aussagen, zwei verschiedene Zustaende. Ein
        // sichtbarer Anmelden-Knopf ist ein BELEG fuer eine Anmelde-Wand; ein
        // fehlender Indikator ist nur ein fehlender Nachweis und trifft auch
        // eine Seite, die noch laedt, oder einen Selektor, der nach einem
        // Website-Umbau nicht mehr passt. Beides in einen Topf zu werfen hat am
        // 07.08.2026 alle acht Brains fuer sechs Stunden gesperrt, obwohl jedes
        // angemeldet war.
        if self.any_visible("login_button") {
            return SessionState::LoginRequired;
        }
        if !self.is_logged_in() {
            return SessionState::Unbestimmt;
        }
        SessionState::Ready
    }

    fn new_chat(&mut self) -> Result<(), String> {
        // Bevorzugt einen New-Chat-Button, sonst Navigation zur Start-URL.
        if self.click_first("new_chat_button") {
            std::thread::sleep(Duration::from_millis(800));
        } else {
            let url = self.url.clone();
            let mut guard = self.driver.borrow_mut();
            let driver = guard.as_mut().ok_or("Backend nicht gestartet")?;
            driver
                .navigate(&url, Duration::from_secs(30))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn send(&mut self, text: &str) -> Result<i32, String> {
        *self.last_sent.borrow_mut() = text.to_string();
        let strategy = self.sel("send_strategy").into_iter().next();
        match strategy.as_deref() {
            Some("gemini") => self.send_gemini(text),
            Some("qwen") => self.send_qwen(text),
            _ => self.send_generic(text),
        }
    }

    fn wait_response(
        &mut self,
        baseline_count: i32,
        timeout: f64,
    ) -> Result<BrainResponse, String> {
        self.wait_response_streaming(baseline_count, timeout, &mut |_| {})
    }

    fn wait_response_streaming(
        &mut self,
        baseline_count: i32,
        timeout: f64,
        on_update: &mut dyn FnMut(&str),
    ) -> Result<BrainResponse, String> {
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

        // Phase 1: warten auf (a) neue Nachricht, (b) sichtbaren Stop-Button ODER
        // (c) geänderten Text der letzten Nachricht — Trigger (c) fängt Brains ab,
        // deren Zähler nicht inkrementiert (Container-Selektor / bestehende Konversation).
        let baseline_text = self.baseline_text.borrow().clone();
        let mut block_polls = 0u32;
        loop {
            let (count, text, stop) = self.probe_generation(&assistant_js, &stop_js, -1);
            let text_changed = !text.trim().is_empty() && text != baseline_text;
            if count > baseline_count || (has_stop && stop) || text_changed {
                break;
            }
            // Frueh (statt erst beim Timeout) auf ein Block-Banner pruefen, damit ein
            // Rate-/Nachrichtenlimit nicht ~timeout Sekunden je Turn kostet. ~alle 2 s.
            block_polls += 1;
            if block_polls.is_multiple_of(7) {
                if let Some(banner) = self.detect_block_banner() {
                    return Ok(mk(banner, -1, false, "blocked"));
                }
            }
            if start.elapsed().as_secs_f64() >= timeout {
                if let Some(banner) = self.detect_block_banner() {
                    return Ok(mk(banner, -1, false, "blocked"));
                }
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
        let mut stop_inventory_done = false;
        // `baseline_count` ist eine ANZAHL, kein nullbasierter Index. Einige UIs
        // (real: ChatGPT nach DOM-Rehydration) ersetzen den letzten Assistant-
        // Container, ohne die Anzahl zu erhoehen. Das fruehere
        // `(count - 1).max(baseline_count)` las dann Index `count` und damit
        // dauerhaft hinter das Array: sichtbare fertige Antwort, aber
        // `timeout_no_text`. Immer den aktuell letzten existierenden Container
        // verfolgen; separat sicherstellen, dass dessen Inhalt wirklich neu ist.
        let mut target =
            latest_response_target(self.probe_generation(&assistant_js, &stop_js, -1).0);
        let mut response_started = false;

        let mut p2_polls = 0u32;
        loop {
            // Provider-Unterbrechungen (z.B. Geminis Antwort-Vergleich) wegklicken,
            // sonst bleibt der Antwort-Container leer und die Erkennung timeoutet.
            self.handle_interruptions();
            let (count, current, stop_raw) = self.probe_generation(&assistant_js, &stop_js, target);
            on_update(&current);
            target = latest_response_target(count);
            let stop_visible = has_stop && stop_raw;
            stop_seen_ever |= stop_visible;
            response_started |=
                response_has_started(count, baseline_count, &current, &baseline_text);

            if current != last_text {
                // Der Text waechst gerade, also generiert die Oberflaeche
                // gerade — und trotzdem kennt kein Selektor den Stop-Button.
                // JETZT ist der einzige Moment, in dem er im DOM steht: die
                // Warnung nach Antwortende kommt zu spaet, da ist er weg.
                //
                // Also einmal je Prozess und Brain aufschreiben, welche
                // Bedienelemente stattdessen sichtbar sind. Damit liefert der
                // Dauerlauf die Beweise fuer korrekte Selektoren selbst, statt
                // dass ich sie in einer Extra-Sitzung von Hand suchen muesste.
                if has_stop && !stop_seen_ever && !stop_inventory_done {
                    stop_inventory_done = true;
                    // Nur auf Anforderung protokollieren (WEBAGENT_STOP_INVENTORY=1):
                    // das Inventar ist ein Suchwerkzeug, kein Dauerlauf-Log.
                    if std::env::var("WEBAGENT_STOP_INVENTORY").ok().as_deref() == Some("1") {
                        let inventar = self.visible_button_inventory();
                        if !inventar.is_empty() {
                            crate::bench_events::eprint_line(&format!(
                                "[browser] {}: kein Stop-Button erkannt, sichtbare Bedienelemente waehrend der Generierung: {inventar}",
                                self.brain_id
                            ));
                        }
                    }
                }
                last_text = current.clone();
                if !current.trim().is_empty() {
                    on_update(&current);
                }
                stable_since = Instant::now();
            }
            let stable_secs = stable_since.elapsed().as_secs_f64();

            // Auch in Phase 2 frueh auf ein Block-Banner pruefen (~alle 2 s), solange
            // noch kein echter Text steht — sonst kostet ein Limit den vollen Timeout.
            // mistrals „Nachrichtenlimit erreicht" erscheint erst NACH dem Senden,
            // also bricht Phase 1 vorher ab und nur hier wird es rechtzeitig erkannt.
            p2_polls += 1;
            if last_text.trim().is_empty() && p2_polls.is_multiple_of(7) {
                if let Some(banner) = self.detect_block_banner() {
                    return Ok(mk(banner, target, false, "blocked"));
                }
            }

            // Ein Stop-Signal kann vor dem ersten Textchunk auftauchen. In
            // diesem Fenster steht im letzten Container noch die alte Antwort;
            // sie darf keinesfalls als neue, bereits valide Antwort geerntet
            // werden.
            let completion = if response_started {
                classify_completion(
                    &current,
                    has_stop,
                    stop_seen_ever,
                    stop_visible,
                    stable_secs,
                    !self.sel("rate_limit_banner").is_empty(),
                )
            } else {
                Completion::Continue
            };
            match completion {
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
                    // Manche Provider (qwen) rendern ihr Limit-/Fehler-Banner NICHT
                    // separat auf der Seite, sondern als Text IM Antwort-Container --
                    // das ist dann eine "vollstaendige, stabile" Antwort im Sinne von
                    // classify_completion, obwohl es keine echte ist. Ohne diese
                    // Pruefung landet der Limit-Text als vermeintlich echte Antwort
                    // (siehe swarm-Test: qwens "daily usage limit" wurde als Antwort
                    // gezaehlt statt als "blocked").
                    // Ein Stop-Selektor, der eine ganze Antwort lang nie
                    // gegriffen hat, ist Dekoration: das autoritative
                    // Fertigsignal fehlt und die Erkennung haengt am
                    // Stabilitaetsfenster. Genau daran wurde bei kimi mitten im
                    // Stream Reasoning-Prosa geerntet. „Selektor vorhanden" ist
                    // nicht „Selektor funktioniert" — also sagen wir es laut,
                    // statt es still zu kompensieren.
                    //
                    // Die Meldung sagt bewusst NICHT mehr „Selektoren pruefen".
                    // Am 30.07.2026 hat die Bestandsaufnahme fuer deepseek und
                    // zai gezeigt, dass ihr Stop-Knopf DASSELBE Element ist wie
                    // der Senden-Knopf (`ds-button--primary ds-button--filled
                    // ds-button--circle` bzw. `p-2 bg-black rounded-full`), nur
                    // mit anderem Icon — kein Attribut unterscheidet die beiden.
                    // Ein Selektor darauf waere aktiv schaedlich: der Stop-Knopf
                    // gaelte dauerhaft als sichtbar, und weil auf „war da, ist
                    // jetzt weg" gewartet wird, liefe JEDER Lauf ins Timeout.
                    // Fuer diese Brains ist das Stabilitaetsfenster nicht die
                    // Kruecke, sondern der einzige korrekte Mechanismus.
                    // Auch diese Bestandsaufnahme ist ein Suchwerkzeug: nur auf
                    // Anforderung (WEBAGENT_STOP_INVENTORY=1) statt bei jeder
                    // Generation, sonst spamt der Dauerlauf.
                    if has_stop
                        && !stop_seen_ever
                        && std::env::var("WEBAGENT_STOP_INVENTORY").ok().as_deref() == Some("1")
                    {
                        crate::bench_events::eprint_line(&format!(
                            "[browser] {}: kein Stop-Knopf erkannt — Fertigsignal kommt aus \
                             Textstabilitaet. Bestandsaufnahme im Log zeigt, ob ein \
                             unterscheidbarer Knopf ueberhaupt existiert",
                            self.brain_id
                        ));
                    }
                    if let Some(hit) = block_phrase_in_text(&text) {
                        crate::bench_events::eprint_line(&format!(
                            "[browser] {}: Block-Phrase '{hit}' im Antworttext erkannt",
                            self.brain_id
                        ));
                        return Ok(mk(text, target, false, "blocked"));
                    }
                    return Ok(mk(text, target, true, "ok"));
                }
                Completion::Continue => {}
            }

            if start.elapsed().as_secs_f64() >= timeout {
                // Kam keine (stabile) Antwort, aber ein Limit-/Block-Banner steht auf
                // der Seite (mistral: „Nachrichtenlimit erreicht") -> als Block melden.
                if last_text.trim().is_empty() {
                    if let Some(banner) = self.detect_block_banner() {
                        return Ok(mk(banner, target, false, "blocked"));
                    }
                }
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
        // Wenn ein Brain `login_indicator` konfiguriert, ist das die Antwort — sonst
        // nichts. Frueher wurde `composer`/`new_chat_button` dazu-ODER-t, was die
        // sorgfaeltig authorten Indikatoren aushebelte: kimi zeigt seinen Composer
        // auch anonym, also galt jeder Besucher als eingeloggt. Der Composer ist ein
        // Beweis fuer "Seite geladen", nicht fuer "angemeldet".
        // Ein sichtbarer Anmelden-Knopf schlaegt JEDEN positiven Indikator.
        // Ohne diese Sperre haengt die Erkennung daran, dass der Indikator
        // sorgfaeltig gewaehlt wurde — und genau das ging schief: geminis
        // `login_indicator` war `div[contenteditable='true']`, also der
        // Composer. Geminis ausgeloggte Startseite hat aber ebenfalls einen
        // ("Frag Gemini"), weshalb `diagnose` am 2026-07-28 `logged_in: true`
        // meldete, waehrend der Screenshot klar "Anmelden" zeigte. Ein
        // ausgeloggtes Brain, das als gesund gilt, bekommt Auftraege und
        // liefert nie. Der Anmelden-Knopf ist das verlaesslichere Signal:
        // eingeloggt zeigt ihn keine Oberflaeche.
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => super::operations::is_logged_in(driver.as_mut(), &self.selectors),
            None => false,
        }
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
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => super::operations::get_conversation_ref(driver.as_mut()),
            None => None,
        }
    }

    fn restore_conversation(&mut self, reference: &str) -> Result<bool, String> {
        if reference.is_empty() {
            return Ok(false);
        }
        let mut guard = self.driver.borrow_mut();
        let driver = match guard.as_mut() {
            Some(c) => c,
            None => return Err("Browser-Driver nicht gestartet".into()),
        };
        driver
            .navigate(reference, Duration::from_secs(30))
            .map(|_| true)
            .map_err(|e| e.to_string())
    }
}

fn latest_response_target(count: i32) -> i32 {
    (count - 1).max(0)
}

fn response_has_started(
    count: i32,
    baseline_count: i32,
    current: &str,
    baseline_text: &str,
) -> bool {
    count > baseline_count || (!current.trim().is_empty() && current != baseline_text)
}

#[cfg(test)]
mod response_tracking_tests {
    use super::{latest_response_target, response_has_started};

    #[test]
    fn reused_last_container_uses_existing_index_and_detects_changed_text() {
        assert_eq!(latest_response_target(3), 2);
        assert!(response_has_started(
            3,
            3,
            "WEBAGENT/1 MESSAGE\ntext: fertig",
            "alte Antwort"
        ));
    }

    #[test]
    fn stop_before_first_text_change_does_not_reuse_old_answer() {
        assert_eq!(latest_response_target(3), 2);
        assert!(!response_has_started(3, 3, "alte Antwort", "alte Antwort"));
    }
}
