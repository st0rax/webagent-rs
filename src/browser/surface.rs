//! DOM-Inspektion, Login und Live-Diagnose des Web-Backends.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Die Methoden bauen
//! auf den `operations`-Free-Fns auf. `dismiss_consent`/`dismiss_qwen_blocks`
//! sind `pub(crate)` (vom `send`-Pfad gerufen), `account_label_js` ebenso
//! (Regressionstest in `mod.rs`).

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::brain::{BrainBackend, SessionState};

use super::{operations, LiveDiagnosis, WebBrainBackend};

impl WebBrainBackend {
    pub fn dom_report(&self) -> Result<Value, String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        operations::dom_report(driver.as_mut(), &self.selectors)
    }

    /// Diagnose-Hilfe: beliebiges JS am aktiven Target auswerten. Nur für
    /// `examples/`/Tools zur Selektor-Analyse gedacht — nicht im Agentenpfad nutzen.
    pub fn eval_js(&self, expr: &str) -> Result<Value, String> {
        self.eval(expr)
    }

    pub(crate) fn is_cloudflare_blocked(&self) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::is_cloudflare_blocked(driver.as_mut()),
            None => false,
        }
    }

    pub(crate) fn dismiss_consent(&self) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => {
                operations::dismiss_consent(driver.as_mut(), &self.selectors, &self.brain_id)
            }
            None => false,
        }
    }

    /// qwen: „App herunterladen / not supported"-Banner schließen.
    pub(crate) fn dismiss_qwen_blocks(&self) -> bool {
        let mut guard = self.driver.borrow_mut();
        match guard.as_mut() {
            Some(driver) => operations::dismiss_qwen_blocks(driver.as_mut()),
            None => false,
        }
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
    pub(crate) fn visible_button_inventory(&self) -> String {
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
    pub(crate) fn account_label_js(&self) -> String {
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
        let shot = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::live_screenshot_with(driver.as_mut(), &self.selectors, open_key)
        };
        let _ = self.stop();
        shot
    }

    /// Wartet, bis die Oberflaeche beschriftete Bedienelemente zeigt.
    /// Ein Lade-Skelett bringt sofort Dutzende leerer Platzhalter mit; ein Scan
    /// darauf sah real 107 Elemente ohne einen einzigen Namen.
    pub(crate) fn wait_for_labeled_controls(&self) {
        let mut guard = self.driver.borrow_mut();
        if let Some(driver) = guard.as_mut() {
            operations::wait_for_labeled_controls(driver.as_mut());
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
        let report = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::live_survey_with(driver.as_mut(), &self.selectors, open_key)
        };
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
        let result = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::probe_surface_with_raw(driver.as_mut(), &self.selectors, open_key)
        };
        let _ = self.stop();
        result
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
        let verdict = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::verify_surface(driver.as_mut(), proposal)
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
        let result = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::probe_surface_with_fill(driver.as_mut(), &self.selectors, open_key)
        };
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
        let result = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::live_diagnose_with_shot(
                driver.as_mut(),
                &self.selectors,
                &self.brain_id,
                session_state,
                with_shot,
            )
        };
        let _ = self.stop();
        result
    }
}
