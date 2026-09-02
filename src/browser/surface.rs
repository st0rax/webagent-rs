//! DOM-Inspektion, Login und Live-Diagnose des Web-Backends.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Die Methoden bauen
//! auf den `operations`-Free-Fns auf. `dismiss_consent` ist `pub(crate)`
//! (vom `send`-Pfad gerufen), `account_label_js` ebenso
//! (Regressionstest in `mod.rs`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::brain::{BrainBackend, SessionState};

use super::{operations, LiveDiagnosis, WebBrainBackend};

// Repeating the exact same CDP mouseMoved coordinates can be coalesced by the
// browser. Alternate between two harmless points so background/headless pages
// receive a real movement while a long-running generation is being polled.
static OFFSCREEN_POINTER_PHASE: AtomicBool = AtomicBool::new(false);

/// Ergebnis von [`WebBrainBackend::probe_stop_by_disappearance`]:
/// `(waehrend der Generierung, danach, Stop-Kandidaten)`.
///
/// Der dritte Teil ist der eigentliche Ertrag — die Elemente, die es nur
/// während der Antwort gab oder die sich an gleicher Stelle verändert haben.
pub type StopDiff = (
    Vec<crate::brain_probe::Candidate>,
    Vec<crate::brain_probe::Candidate>,
    Vec<crate::brain_probe::Candidate>,
);

fn is_terminal_image_generation_error(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "limit for image generation",
        "limit for image generations",
        "image generation is not available",
        "can't create more images",
        "cannot create more images",
        "keine weiteren bilder erstellen",
        "bildgenerierung ist nicht verfuegbar",
        "upgrade your plan",
        "upgrade dein abo",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

impl WebBrainBackend {
    pub fn mark_image_generation_baseline(&self) -> Result<(), String> {
        self.eval_js(
            r#"(()=>{const key=src=>{try{const u=new URL(src,location.href);return u.searchParams.get('id')||u.pathname}catch(e){return src}};window.__webagentImageBaseline=[...document.images].map(img=>key(img.currentSrc||img.src||'')).filter(Boolean);window.__webagentCanvasBaselineCount=document.querySelectorAll('canvas').length;return window.__webagentImageBaseline.length})()"#,
        )
        .map(|_| ())
    }

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

    /// Holt das neueste grosse, sichtbare Bild aus einer Assistentenantwort
    /// als Base64. Der Fetch laeuft im angemeldeten Browserkontext, damit auch
    /// `blob:`-URLs und cookie-geschuetzte Provider-Ressourcen funktionieren.
    pub fn latest_generated_image(&self) -> Result<Option<(String, String)>, String> {
        let expression = r#"(async()=>{
          const visible=e=>{const r=e.getBoundingClientRect();const s=getComputedStyle(e);return r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden'};
          const imageKey=src=>{try{const u=new URL(src,location.href);return u.searchParams.get('id')||u.pathname}catch(e){return src}};
          const baseline=new Set(window.__webagentImageBaseline||[]);
          const isNewImage=img=>!baseline.has(imageKey(img.currentSrc||img.src||''));
          const images=[...document.querySelectorAll('img')].filter(img=>visible(img)&&img.naturalWidth>=256&&img.naturalHeight>=256&&isNewImage(img));
          for(let i=images.length-1;i>=0;i--){
            const img=images[i];
            if(img.closest('form,[data-attachment],[class*=avatar i]')) continue;
            const src=img.currentSrc||img.src;
            if(!src) continue;
            try{
              const response=await fetch(src,{credentials:'include'});
              if(!response.ok) continue;
              const blob=await response.blob();
              if(!blob.type.startsWith('image/')) continue;
              const data=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=()=>reject(reader.error);reader.readAsDataURL(blob)});
              const comma=data.indexOf(',');
              return {mime_type:blob.type||'image/png',base64:data.slice(comma+1)};
            }catch(e){}
          }
          const canvases=[...document.querySelectorAll('canvas')].slice(window.__webagentCanvasBaselineCount||0).filter(c=>visible(c)&&c.width>=256&&c.height>=256);
          for(let i=canvases.length-1;i>=0;i--){
            try{const data=canvases[i].toDataURL('image/png');return {mime_type:'image/png',base64:data.slice(data.indexOf(',')+1)}}catch(e){}
          }
          const elements=[...document.querySelectorAll('main *')].filter(e=>visible(e)&&!e.dataset.webagentBaselineBackground).reverse();
          for(const element of elements){
            const match=getComputedStyle(element).backgroundImage.match(/^url\(["']?(.*?)["']?\)$/);
            if(!match) continue;
            const r=element.getBoundingClientRect();if(r.width<256||r.height<256) continue;
            try{const response=await fetch(match[1],{credentials:'include'});if(!response.ok)continue;const blob=await response.blob();if(!blob.type.startsWith('image/'))continue;const data=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=()=>reject(reader.error);reader.readAsDataURL(blob)});return {mime_type:blob.type||'image/png',base64:data.slice(data.indexOf(',')+1)}}catch(e){}
          }
          return null;
        })()"#;
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        let value = driver
            .evaluate_async(expression)
            .map_err(|e| e.to_string())?;
        if value.is_null() {
            let box_value = driver
                .evaluate(r#"(()=>{const visible=e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0};const key=src=>{try{const u=new URL(src,location.href);return u.searchParams.get('id')||u.pathname}catch(e){return src}};const baseline=new Set(window.__webagentImageBaseline||[]);const images=[...document.querySelectorAll('img')].filter(i=>visible(i)&&i.naturalWidth>=256&&i.naturalHeight>=256&&!baseline.has(key(i.currentSrc||i.src||''))&&!i.closest('form,[data-attachment],[class*=avatar i]'));const image=images.at(-1);if(!image)return null;const r=image.getBoundingClientRect();return{x:r.left+scrollX,y:r.top+scrollY,width:r.width,height:r.height,scale:Math.max(1,Math.min(2,image.naturalWidth/r.width))}})()"#)
                .map_err(|e| e.to_string())?;
            if box_value.is_null() {
                return Ok(None);
            }
            let number = |name: &str| {
                box_value
                    .get(name)
                    .and_then(Value::as_f64)
                    .ok_or_else(|| format!("Bild-Ausschnitt ohne {name}"))
            };
            let base64 = driver
                .capture_png_clip_base64(
                    number("x")?,
                    number("y")?,
                    number("width")?,
                    number("height")?,
                    number("scale")?,
                )
                .map_err(|e| e.to_string())?;
            return Ok(Some(("image/png".to_string(), base64)));
        }
        let mime = value
            .get("mime_type")
            .and_then(Value::as_str)
            .ok_or_else(|| "Bildartefakt ohne MIME-Typ".to_string())?;
        let base64 = value
            .get("base64")
            .and_then(Value::as_str)
            .ok_or_else(|| "Bildartefakt ohne Base64-Daten".to_string())?;
        if base64.len() < 128 {
            return Err("Bildartefakt ist unerwartet klein".to_string());
        }
        Ok(Some((mime.to_string(), base64.to_string())))
    }

    pub fn image_generation_report(&self) -> String {
        let expression = r#"(()=>{const visible=e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0};const imgs=[...document.images].filter(visible);const large=imgs.filter(i=>i.naturalWidth>=256&&i.naturalHeight>=256);const canvases=[...document.querySelectorAll('canvas')].filter(visible);const assistant=[...document.querySelectorAll('[data-message-author-role="assistant"],[data-testid*="assistant" i],article')].filter(visible);const composer=[...document.querySelectorAll('textarea,[contenteditable="true"],[role="textbox"]')].filter(visible).at(-1);const draft=('value' in (composer||{})?composer.value:composer?.innerText)||'';return {url:location.href,images:imgs.length,large_images:large.length,assistant_large_images:large.filter(i=>i.closest('[data-message-author-role="assistant"],[data-testid*="assistant" i],article')).length,canvases:canvases.length,large_canvases:canvases.filter(c=>c.width>=256&&c.height>=256).length,draft:draft.trim().slice(-300),last_assistant:(assistant.at(-1)?.innerText||'').trim().slice(-500),page_tail:(document.body.innerText||'').trim().slice(-800)}})()"#;
        self.eval_js(expression)
            .map(|value| value.to_string())
            .unwrap_or_else(|error| format!("Diagnose fehlgeschlagen: {error}"))
    }

    pub fn image_generation_provider_error(&self) -> Option<String> {
        let text = self
            .eval_js(
                r#"(()=>{const visible=e=>{const r=e.getBoundingClientRect();return r.width>0&&r.height>0};const nodes=[...document.querySelectorAll('[data-message-author-role="assistant"],[data-testid*="assistant" i],article')].filter(visible);return(nodes.at(-1)?.innerText||'').trim().slice(-1000)})()"#,
            )
            .ok()?
            .as_str()?
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }
        is_terminal_image_generation_error(&text).then_some(text)
    }

    pub fn wake_offscreen_renderer(&self) {
        if let Some(driver) = self.driver.borrow_mut().as_mut() {
            let phase = OFFSCREEN_POINTER_PHASE.fetch_xor(true, Ordering::Relaxed);
            let coordinate = if phase { 2.0 } else { 1.0 };
            let _ = driver.move_pointer(coordinate, coordinate);
        }
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
            Some(driver) => operations::dismiss_consent(driver.as_mut(), &self.selectors),
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

    /// Schliesst den Browser und laesst ihn die Sitzung fertig schreiben.
    ///
    /// WebView2 schreibt Cookies, Local Storage und IndexedDB NICHT waehrend
    /// des Betriebs, sondern asynchron BEIM Schliessen — derselbe Grund, aus
    /// dem `teardown_runtime` nach dem Drop wartet, bevor es zurueckschreibt.
    /// Vorher zu warten hilft deshalb nichts: Zum Zeitpunkt des Wartens hat
    /// der Browser noch gar nichts zu schreiben begonnen.
    ///
    /// Gemessen 2026-08-22 an kimi: Nach jeder Anmeldung blieb im Profil nur
    /// ein `anonymous_refresh_token` zurueck — die Sitzung starb mit dem
    /// Fenster, und der naechste Lauf verlangte erneut eine Anmeldung.
    fn stop_and_flush(&mut self) {
        let _ = self.stop();
        std::thread::sleep(Duration::from_secs(5));
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
                self.stop_and_flush();
                return Ok(true);
            }
            if start.elapsed() >= timeout {
                // Auch eine NICHT erkannte Anmeldung hat Sitzungsdaten erzeugt:
                // Chromium haelt sie noch im Speicher und schreibt sie erst beim
                // geordneten Schliessen. Hart zu schliessen wirft genau die
                // Anmeldung weg, auf die hier gerade Minuten gewartet wurde.
                //
                // Bei einem Brain, dessen Indikator nach einem Website-Umbau
                // nicht mehr trifft, passiert das JEDES Mal: anmelden, Timeout,
                // Sitzung verworfen, beim naechsten Lauf wieder "Login noetig"
                // (gemessen 2026-08-22 an kimi). Dieselbe Gnadenfrist wie im
                // Erfolgsfall — danach ein letzter Blick, denn wer sich kurz vor
                // Schluss angemeldet hat, wird so noch als Erfolg erkannt.
                let logged_in_late = self.is_logged_in();
                self.stop_and_flush();
                return Ok(logged_in_late);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    /// Skriptbasiertes Login: klickt die Login-Kette selbst durch statt auf
    /// manuelle Eingabe zu warten.
    ///
    /// Die Kette ist bewusst kurz und am Selektorbestand entlanggebaut:
    /// `login_button` (Anmelden) → ggf. `google_sso_button` (SSO-Klick) → warten
    /// bis `is_logged_in`. Mehr tut sie nicht — Passwort, 2FA und unerwartete
    /// Zwischenseiten überlässt sie dem Menschen, das Fenster bleibt dabei
    /// offen. Wer schon eingeloggt ist, bekommt sofort `true`.
    pub fn try_auto_login(&mut self, timeout: Duration) -> Result<bool, String> {
        self.start(false)?; // headed — der Mensch soll sehen (und ggf. nachhelfen) können
        let deadline = Instant::now() + timeout;
        if self.is_logged_in() {
            let _ = self.stop();
            return Ok(true);
        }
        // Klick 1: "Anmelden"/"Sign in". Echte Maus-Klicks, denn Geminis SSO-
        // Buttons ignorieren synthetische `el.click()`.
        self.dismiss_consent();
        crate::bench_events::eprint_line("[auto-login] klicke 'Anmelden'…");
        let first = self.click_visible_real("login_button");
        if !first {
            self.dismiss_consent();
            if !self.click_visible_real("login_button") {
                let _ = self.stop();
                return Err("Anmelden-Button nicht gefunden".into());
            }
        }
        std::thread::sleep(Duration::from_secs(2));
        // Klick 2: falls eine Google-SSO-Oberfläche erscheint, deren Button
        // mitnehmen. Nur wenn sichtbar — manche Brains loggen ohne SSO ein.
        if self.any_visible("google_sso_button") {
            crate::bench_events::eprint_line("[auto-login] klicke Google-SSO…");
            self.click_visible_real("google_sso_button");
            std::thread::sleep(Duration::from_secs(3));
        }
        // Klick 3+: falls ein Kontowähler die letzte Sitzung anbietet, wird er
        // typischerweise mit EINEM weiteren Klick übernommen. Das Fenster bleibt
        // offen; der Mensch kann eingreifen, und wir pollen bis zum Timeout.
        while Instant::now() < deadline {
            self.dismiss_consent();
            if self.is_logged_in() {
                std::thread::sleep(Duration::from_secs(2)); // Session ins Profil flushen
                let _ = self.stop();
                return Ok(true);
            }
            // Beim Google-Kontowähler sitzt die "Weiter"-Logik oft in einem
            // nicht selektorisierten Element — dafür gibt es keinen Selektorschlüssel.
            // Statt zu raten: warten, damit die Oberfläche sich stabilisiert.
            std::thread::sleep(Duration::from_secs(2));
        }
        let _ = self.stop();
        Ok(false)
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
    pub fn probe_surface(
        &mut self,
        headless: bool,
    ) -> Result<Vec<crate::brain_probe::Proposal>, String> {
        let (_, proposals) = self.probe_surface_with_raw(headless, None)?;
        Ok(proposals)
    }

    /// Wie ``probe_surface``, gibt aber auch die rohen DOM-Kandidaten mit
    /// zurueck — fuer die Analyse von Fehlfunden (Warum wurde der Absende-
    /// Knopf nicht erkannt?).
    pub fn probe_surface_with_raw(
        &mut self,
        headless: bool,
        open_key: Option<&str>,
    ) -> Result<
        (
            Vec<crate::brain_probe::Candidate>,
            Vec<crate::brain_probe::Proposal>,
        ),
        String,
    > {
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

    /// Faehrt einen Vorschlag aus ``probe_surface`` live an der offenen Seite:
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

    /// Oberflaechen-Analyse wie ``probe_surface``, aber mit einer zweiten Runde:
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
    ) -> Result<
        (
            Vec<crate::brain_probe::Candidate>,
            Vec<crate::brain_probe::Proposal>,
        ),
        String,
    > {
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
    /// Oberflächen-Analyse **während einer laufenden Generierung**.
    ///
    /// Der Stop-Knopf existiert im Ruhezustand nicht. Ein normaler
    /// [`probe_surface`](Self::probe_surface)-Scan sieht ihn deshalb nie — und
    /// genau daran sind am 2026-08-10 die Selektor-Reparaturen für deepseek und
    /// kimi gescheitert: raten statt messen. Diese Methode sendet eine Probe,
    /// wartet, bis die Antwort nachweislich läuft, und scannt **dann**.
    ///
    /// Sie schreibt eine echte Nachricht in einen echten Chat — wie die
    /// Generation-Sequenz von `verify` und aus demselben Grund: eine Fähigkeit,
    /// die nur während einer Generierung existiert, ist ohne Generierung nicht
    /// messbar.
    ///
    /// Bricht mit Fehler ab, wenn kein Antwortsignal kommt: ein Scan im
    /// Ruhezustand wäre wertlos und würde als „Stop-Knopf nicht gefunden"
    /// missverstanden.
    pub fn probe_surface_generating(
        &mut self,
        headless: bool,
        probe: &str,
    ) -> Result<
        (
            Vec<crate::brain_probe::Candidate>,
            Vec<crate::brain_probe::Proposal>,
        ),
        String,
    > {
        use std::time::{Duration, Instant};

        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);

        let baseline = match self.send(probe) {
            Ok(b) => b,
            Err(e) => {
                let _ = self.stop();
                return Err(format!("Probe nicht absendbar: {e}"));
            }
        };

        // Warten, bis die Generierung nachweislich läuft — dieselben Signale
        // wie im Beleg-Poll (`browser/verify.rs`), nur ohne Stop-Zweig: der
        // Knopf, den wir suchen, darf hier nicht Voraussetzung sein.
        let assistant_js = self.sel_js("assistant_message", &["div.prose"]);
        let baseline_text = self.baseline_text.borrow().clone();
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut running = false;
        while Instant::now() < deadline {
            let (count, text, _) = self.probe_generation(&assistant_js, "[]", -1);
            if count > baseline || (!text.trim().is_empty() && text != baseline_text) {
                running = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !running {
            let _ = self.stop();
            return Err("kein Antwort-Signal — Scan waere im Ruhezustand und damit wertlos".into());
        }

        // Kurz laufen lassen: manche Oberflächen rendern den Stop-Knopf erst
        // mit dem ersten Textchunk, nicht schon beim Absenden.
        std::thread::sleep(Duration::from_millis(1200));

        let result = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::scan_once(driver.as_mut())
        };
        let _ = self.stop();
        result
    }

    /// Wie [`Self::probe_surface_generating`], liefert aber TEXTCONTAINER
    /// statt Bedienelemente — die Kandidaten für `assistant_message`.
    ///
    /// Der reguläre Scan sieht nur Interaktives, ein Antwortbereich ist nichts
    /// davon. Deshalb war der Container für jedes Brain unsichtbar, und ein
    /// fehlender `assistant_message`-Selektor ließ sich nicht vermessen,
    /// sondern nur raten.
    pub fn probe_text_generating(
        &mut self,
        headless: bool,
        probe: &str,
    ) -> Result<Vec<crate::brain_probe::TextCandidate>, String> {
        use std::time::Duration;

        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);

        let baseline_text = self.baseline_text.borrow().clone();
        if let Err(e) = self.send(probe) {
            let _ = self.stop();
            return Err(format!("Probe nicht absendbar: {e}"));
        }

        // Auf Text warten, NICHT auf `assistant_message`: Wer den Selektor
        // sucht, kann ihn nicht zur Voraussetzung machen. Der Vergleich gegen
        // den Seitentext vor dem Senden kommt ohne ihn aus.
        // Feste Wartezeit statt Wachstumserkennung. Ein Vergleich der
        // Textmenge vorher/nachher taugt hier nicht: Die Seite traegt oft
        // schon einen langen Verlauf (gemessen: 100.237 Zeichen Eigentext vor
        // dem Senden), in dem eine neue Antwort untergeht. Und ein Warten auf
        // `assistant_message` verbietet sich, weil genau dieser Selektor
        // gesucht wird. Fuer ein Diagnosewerkzeug ist die schlichte Wartezeit
        // die ehrlichere Loesung — sie behauptet kein Signal, das es nicht
        // gibt.
        let _ = &baseline_text;
        let url_jetzt = |backend: &Self| -> String {
            let mut guard = backend.driver.borrow_mut();
            guard
                .as_mut()
                .and_then(|d| d.eval_string("location.href").ok())
                .unwrap_or_default()
        };
        // Perplexity navigiert beim Absenden in einen neuen Thread
        // (`/search/new/<id>` → `/search/<andere-id>`, gemessen 2026-08-23).
        // Wer sofort scannt, liest die Seite VOR der Navigation. Erst auf eine
        // stabile URL warten, dann die Antwort ausschreiben lassen.
        let mut letzte = url_jetzt(self);
        let mut ruhig = 0;
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(500));
            let jetzt = url_jetzt(self);
            if jetzt == letzte {
                ruhig += 1;
                if ruhig >= 6 {
                    break;
                }
            } else {
                letzte = jetzt;
                ruhig = 0;
            }
        }
        let warte: u64 = std::env::var("WEBAGENT_PROBE_WAIT_SECS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(35);
        std::thread::sleep(Duration::from_secs(warte));
        // Ausschreiben lassen, damit der Container seine endgueltige Form hat.
        std::thread::sleep(Duration::from_millis(2500));

        let result = {
            let mut guard = self.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            crate::brain_probe::collect_text(driver.as_mut()).map_err(|e| e.to_string())
        };
        let _ = self.stop();
        result
    }

    /// Zwei Abzüge — während der Generierung und danach — und die Differenz.
    ///
    /// Der Stop-Knopf trägt bei manchen Oberflächen **kein** unterscheidendes
    /// Attribut: deepseek rendert Dutzende `div[role=button]` mit identischer
    /// Klasse, ohne Label, Text, id oder title. Statisch ist er dort nicht
    /// findbar — wohl aber an seiner *zeitlichen* Signatur: er ist da, solange
    /// die Antwort läuft, und verschwindet danach.
    ///
    /// Liefert `(waehrend, danach, nur_waehrend)`. Der dritte Teil sind die
    /// Kandidaten, die es nur während der Generierung gab — unter ihnen muss
    /// der Stop-Knopf sein. Verglichen wird über Lage und Größe, weil bei
    /// solchen Oberflächen sonst nichts sie unterscheidet.
    pub fn probe_stop_by_disappearance(
        &mut self,
        headless: bool,
        probe: &str,
    ) -> Result<StopDiff, String> {
        use std::time::{Duration, Instant};

        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);

        let baseline = match self.send(probe) {
            Ok(b) => b,
            Err(e) => {
                let _ = self.stop();
                return Err(format!("Probe nicht absendbar: {e}"));
            }
        };

        let assistant_js = self.sel_js("assistant_message", &["div.prose"]);
        let baseline_text = self.baseline_text.borrow().clone();

        let scan = |me: &Self| -> Result<Vec<crate::brain_probe::Candidate>, String> {
            let mut guard = me.driver.borrow_mut();
            let driver = guard
                .as_mut()
                .ok_or_else(|| "Backend nicht gestartet".to_string())?;
            operations::scan_once(driver.as_mut()).map(|(c, _)| c)
        };

        // Phase 1: warten bis die Antwort läuft, dann Abzug.
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut running = false;
        while Instant::now() < deadline {
            let (count, text, _) = self.probe_generation(&assistant_js, "[]", -1);
            if count > baseline || (!text.trim().is_empty() && text != baseline_text) {
                running = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !running {
            let _ = self.stop();
            return Err("kein Antwort-Signal — ohne laufende Generierung kein Vergleich".into());
        }
        // Warten, bis die Oberfläche fertig gerendert ist — nicht bloß eine
        // feste Pause. Gemessen an zai: ein Abzug 1,2 s nach dem ersten Signal
        // sah 21 sichtbare Elemente, der Abzug danach 203. Ein Vergleich
        // zwischen einer halb aufgebauten und einer fertigen Seite meldet
        // Hunderte „Unterschiede" und keinen davon brauchbar.
        //
        // Kriterium: zwei aufeinanderfolgende Abzüge mit gleicher Anzahl
        // sichtbarer Elemente. Danach ist der Aufbau stabil.
        let mut during = scan(self)?;
        let stable_until = Instant::now() + Duration::from_secs(20);
        loop {
            std::thread::sleep(Duration::from_millis(700));
            let next = scan(self)?;
            let a = during.iter().filter(|c| c.visible).count();
            let b = next.iter().filter(|c| c.visible).count();
            during = next;
            if a == b || Instant::now() >= stable_until {
                break;
            }
        }

        // Phase 2: warten bis der Text stillsteht, dann erneut.
        let mut last = String::new();
        let mut stable_since = Instant::now();
        let deadline2 = Instant::now() + Duration::from_secs(180);
        while Instant::now() < deadline2 {
            let (_, text, _) = self.probe_generation(&assistant_js, "[]", -1);
            if !text.is_empty() && text == last {
                if stable_since.elapsed() >= Duration::from_secs(4) {
                    break;
                }
            } else {
                stable_since = Instant::now();
                last = text;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        std::thread::sleep(Duration::from_millis(1500));
        let after = scan(self)?;
        let _ = self.stop();

        // Die Identität über Lage/Größe und die beiden Muster (Verschwinden +
        // Verwandlung) sind in `brain_probe.rs` als reine Rechnung, damit sie
        // ohne Browser testbar sind.
        let candidates = crate::brain_probe::stop_diff_candidates(&during, &after);
        Ok((during, after, candidates))
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
        // Profil kann den letzten Chat wiederherstellen (fremde Systemprompts).
        // Diagnose und Relays sollen auf einem leeren Turn landen.
        let _ = self.new_chat();
        let session_state = self.ensure_ready(45.0).unwrap_or(SessionState::Error);
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

#[cfg(test)]
mod image_generation_tests {
    use super::is_terminal_image_generation_error;

    #[test]
    fn image_generation_quota_messages_are_terminal() {
        assert!(is_terminal_image_generation_error(
            "You've hit the Free plan limit for image generations requests."
        ));
        assert!(is_terminal_image_generation_error(
            "Du kannst vorerst keine weiteren Bilder erstellen. Upgrade dein Abo."
        ));
        assert!(!is_terminal_image_generation_error(
            "Your image is being generated."
        ));
    }
}
