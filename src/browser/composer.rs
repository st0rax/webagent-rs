//! Eingabefeld bedienen: Text setzen, tippen, absenden, gegenpruefen.
//!
//! Kindmodul von `browser` — sieht dessen private Interna ohne
//! Sichtbarkeitsaenderungen.

use super::WebBrainBackend;
use serde_json::Value;
use std::time::Duration;

impl WebBrainBackend {
    /// Fuellt einen contenteditable Rich-Text-Editor absatzweise. Lexical
    /// verwirft bei `Input.insertText` alles hinter dem ersten Zeilenumbruch;
    /// `execCommand('insertParagraph')` geht dagegen durch seinen Editor-State.
    pub(super) fn fill_composer_rich_multiline(&self, composer_js: &str, text: &str) -> bool {
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}";
        let coords = self
            .eval(&Self::js_scan(composer_js, coord_body, "null"))
            .unwrap_or(Value::Null);
        if let (Some(x), Some(y)) = (
            coords.get("x").and_then(Value::as_f64),
            coords.get("y").and_then(Value::as_f64),
        ) {
            self.wake_renderer();
            let mut guard = self.driver.borrow_mut();
            if let Some(driver) = guard.as_mut() {
                let _ = driver.click_at(x, y);
                std::thread::sleep(Duration::from_millis(80));
                if driver.replace_multiline_text(text).is_ok() {
                    return true;
                }
            }
        }

        // Portabler Fallback fuer Treiber ohne in-process CDP.
        let serialized = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let body = format!(
            "var el=Q(S[i]);if(el){{var r=el.getBoundingClientRect();if(r.width<=0||r.height<=0)return false;\
             el.focus();try{{if('value' in el){{el.value='';el.dispatchEvent(new Event('input',{{bubbles:true}}));}}else{{\
             var selection=window.getSelection(),range=document.createRange();range.selectNodeContents(el);\
             selection.removeAllRanges();selection.addRange(range);document.execCommand('delete',false,null);}}\
             var parts={serialized}.replace(/\\r\\n/g,'\\n').split('\\n');\
             for(var p=0;p<parts.length;p++){{if(parts[p])document.execCommand('insertText',false,parts[p]);\
             if(p+1<parts.length)document.execCommand('insertParagraph',false,null);}}\
             el.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText'}}));return true;}}catch(error){{return false;}}}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &body, "false"))
    }

    /// Vergleicht den gesamten sichtbaren Editorinhalt, wobei nur die von
    /// Rich-Text-Editoren unterschiedlich gerenderte Leerraumstruktur
    /// normalisiert wird. Ein passender Anfang reicht fuer Maschinenprompts
    /// nicht: Kimi hatte dadurch still nur Absatz eins uebernommen.
    pub(super) fn composer_matches_text(&self, composer_js: &str, text: &str) -> bool {
        let expected = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let expected = serde_json::to_string(&expected).unwrap_or_else(|_| "\"\"".into());
        let body = format!(
            "var el=Q(S[i]);if(el){{var v=('value' in el)?(el.value||''):(el.innerText||el.textContent||'');\
             return v.replace(/\\s+/g,' ').trim()==={expected};}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &body, "false"))
    }

    /// Playwright-`fill()`-Äquivalent: DOM setzen + input/change-Events (Angular/React).
    pub(super) fn fill_composer_dom_set(&self, composer_js: &str, text: &str) -> bool {
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}";
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
        self.wake_renderer();
        {
            let mut guard = self.driver.borrow_mut();
            if let Some(driver) = guard.as_mut() {
                let _ = driver.click_at(x, y);
            }
        }
        std::thread::sleep(Duration::from_millis(80));
        let t = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        let set_body = format!(
            "var el=Q(S[i]);if(!el)return false;el.focus();\
            if(el.isContentEditable){{el.textContent={t};el.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:{t}}}));}}\
            else if('value' in el){{el.value={t};el.dispatchEvent(new Event('input',{{bubbles:true}}));el.dispatchEvent(new Event('change',{{bubbles:true}}));}}\
            else return false;return true;"
        );
        self.eval_bool(&Self::js_scan(composer_js, &set_body, "false"))
    }

    pub(super) fn type_text_char_by_char(&self, text: &str) -> Result<(), String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        for ch in text.chars() {
            let s = ch.to_string();
            driver.press_key(&s, &s, 0, &s).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Provider-spezifische Unterbrechungen wegklicken, die den Antwortfluss
    /// blockieren — z.B. Geminis „Welche Antwort bevorzugst du?"-Vergleich
    /// (`response_preference_choice`) oder Hinweis-Dialoge (`notice_close_button`).
    /// Alle Aufrufe sind harmlos, wenn die Selektoren nicht konfiguriert sind.
    pub(super) fn handle_interruptions(&self) {
        self.click_first("response_preference_choice");
        self.click_first("notice_close_button");
    }

    /// Enter im aktuell fokussierten Element auslösen (echtes Tastatur-Event via CDP).
    pub(super) fn press_enter(&self) -> Result<(), String> {
        // Headed NOACTIVATE tiles freeze until a trusted pointer nudge — wake first.
        self.wake_renderer_or_err()?;
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        driver
            .press_key("Enter", "Enter", 13, "\r")
            .map_err(|e| e.to_string())
    }

    /// Setzt den Text in den Composer (fokussiert, `value`/`textContent`, feuert
    /// `input`). Gibt true, wenn ein Composer gefunden wurde.
    pub(super) fn fill_composer(&self, composer_js: &str, text: &str) -> bool {
        // 1) Mittelpunkt-Koordinaten des Composers holen (nicht gefunden -> false).
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}";
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
        // 2) Wake then real mouse click on the composer (focus), then clear.
        //    NOACTIVATE/headed tiles otherwise ignore the click until mouseover.
        self.wake_renderer();
        {
            let mut guard = self.driver.borrow_mut();
            if let Some(driver) = guard.as_mut() {
                let _ = driver.click_at(x, y);
            }
        }
        std::thread::sleep(Duration::from_millis(80));
        let clear_body = "var el=Q(S[i]);if(el){el.focus();try{if('value' in el){el.value='';}else{el.textContent='';}el.dispatchEvent(new InputEvent('input',{bubbles:true}));}catch(e){}return true;}";
        let _ = self.eval_bool(&Self::js_scan(composer_js, clear_body, "false"));
        // 3) Echt tippen via PageDriver::insert_text.
        {
            let mut guard = self.driver.borrow_mut();
            if let Some(driver) = guard.as_mut() {
                let _ = driver.insert_text(text);
            }
        }
        let t = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        // 4) Falls der Composer weiterhin leer ist: execCommand('insertText'). Das
        //    feuert beforeinput/input mit inputType 'insertText' — der Weg, den
        //    Rich-Text-Editoren (Lexical bei kimi, ProseMirror bei mistral) als echte
        //    Eingabe registrieren. Ein direktes textContent=… (Schritt 5) rendert zwar
        //    sichtbar, aber Lexical verwirft es beim naechsten Reconcile, sodass Enter
        //    nichts abschickt — genau das machte kimi frueher unzuverlaessig.
        let exec_body = format!(
            "var el=Q(S[i]);if(el){{var cur=('value' in el)?(el.value||''):(el.textContent||'');if(cur.trim().length===0){{el.focus();try{{document.execCommand('insertText',false,{t});}}catch(e){{}}}}return true;}}"
        );
        let _ = self.eval_bool(&Self::js_scan(composer_js, &exec_body, "false"));
        // 5) Letzter Ausweg: nur falls immer noch leer, roh .value/.textContent setzen.
        let set_body = format!(
            "var el=Q(S[i]);if(el){{var cur=('value' in el)?(el.value||''):(el.textContent||'');if(cur.trim().length===0){{if('value' in el){{el.value={t};}}else{{el.textContent={t};}}el.dispatchEvent(new InputEvent('input',{{bubbles:true}}));}}return true;}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &set_body, "false"))
    }

    /// True, wenn der Composer sichtbar den Anfang von `text` enthaelt — also das
    /// Fuellen **als der Editor es sieht** gegriffen hat. `fill_composer` allein meldet
    /// nur, dass ein Feld existiert; bei kimis Lexical-Editor kann es leer bleiben. Nur
    /// senden, wenn der Text wirklich drinsteht.
    pub(super) fn composer_contains(&self, composer_js: &str, text: &str) -> bool {
        let needle = text.chars().take(8).collect::<String>();
        let n = serde_json::to_string(&needle).unwrap_or_else(|_| "\"\"".into());
        let body = format!(
            "var el=Q(S[i]);if(el){{var v=('value' in el)?(el.value||''):(el.innerText||el.textContent||'');if(v.indexOf({n})!==-1)return true;}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &body, "false"))
    }
}
