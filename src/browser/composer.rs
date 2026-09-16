//! Eingabefeld bedienen: Text setzen, tippen, absenden, gegenpruefen.
//!
//! Kindmodul von `browser` — sieht dessen private Interna ohne
//! Sichtbarkeitsaenderungen.

use super::WebBrainBackend;
use serde_json::Value;
use std::time::Duration;

/// T-937: wie der Composer fokussiert wurde. Keyboard = Tab-Tastaturweg,
/// ElFocus = In-Page `focus()` (fuer tabindex=-1-Elemente), Click =
/// Koordinaten-Klick als letzter Rueckfall, None = keine Strategie griff.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FocusMethod {
    Keyboard,
    ElFocus,
    Click,
    None,
}

/// T-937: Ergebnis der Fokusverifikation — Methode, ob der Fokus nachweislich
/// angekommen ist, verbrauchte Tab-Drucke und ob der tabindex=-1-Rueckfall
/// gegriffen hat (von "Fokus kam nie an" unterscheidbar).
#[derive(Debug, Clone, Copy)]
pub(super) struct FocusOutcome {
    pub method: FocusMethod,
    pub arrived: bool,
    pub tries: u32,
    pub tabindex_fallback: bool,
}

/// So viele Tab-Drucke maximal, bevor `el.focus()`/Klick als Rueckfall greift.
const FOCUS_TAB_TRIES: u32 = 5;

impl WebBrainBackend {
    /// T-936: Koordinatenergebnis (Index, Masse, Clamp) in die laufende
    /// Turn-Beobachtung eintragen — die Metadaten, die heute berechnet und
    /// weggeworfen werden.
    fn note_composer_metrics(&self, coords: &Value) {
        let idx = coords.get("i").and_then(Value::as_u64).map(|i| i as u32);
        let w = coords.get("w").and_then(Value::as_f64);
        let h = coords.get("h").and_then(Value::as_f64);
        let clamp = coords
            .get("clamp")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        crate::brain_score::update_pending_turn(|obs| {
            obs.phase = crate::brain_score::SendPhase::Focus;
            obs.selector_index = idx;
            obs.element_w = w;
            obs.element_h = h;
            obs.clamp_triggered = clamp;
        });
    }

    /// Ausdruck, der prueft, ob der Composer gegen den Selektor den Fokus
    /// wirklich haelt (`document.activeElement === el`).
    fn composer_focus_probe_expr(&self, composer_js: &str) -> String {
        let probe =
            "var el=Q(S[i]);if(el){return (document.activeElement===el||el.matches(':focus'));}";
        Self::js_scan(composer_js, probe, "false")
    }

    /// Ein einzelner echter Tastendruck ueber den PageDriver (innerhalb der
    /// Seite, siehe press_key_script): moegliche Seiten-/Editor-Reaktionen
    /// (Menues, Fokuswechsel) werden damit ausgeloest, keine Koordinaten.
    fn press_simple_key(&self, key: &str, code: &str, vk: i64) -> Result<(), String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        driver
            .press_key(key, code, vk, "")
            .map_err(|e| e.to_string())
    }

    /// T-937: Composer per Tastatur fokussieren und nach jedem Druck gegen
    /// `document.activeElement` verifizieren (Tastatur-Loop, selbstkorrigierend).
    /// Liefert die Zahl der noetigen Tab-Drucke, sobald der Fokus angekommen ist.
    fn focus_composer_keyboard(&self, composer_js: &str) -> Option<u32> {
        let probe = self.composer_focus_probe_expr(composer_js);
        for try_n in 1..=FOCUS_TAB_TRIES {
            if self.press_simple_key("Tab", "Tab", 9).is_err() {
                return None;
            }
            if self.eval_bool(&probe) {
                return Some(try_n);
            }
        }
        None
    }

    /// T-937 Rueckfallweg: In-Page `focus()` + Verifikation (Muster
    /// webview_runtime.rs:1198). Greift auch bei tabindex=-1 — dort ist
    /// `focus()` erlaubt, Tab nicht; der Erfolg heisst dann `tabindex_fallback`.
    fn focus_composer_el_focus(&self, composer_js: &str) -> bool {
        let body =
            "var el=Q(S[i]);if(el){el.focus();return (document.activeElement===el||el.matches(':focus'));}";
        self.eval_bool(&Self::js_scan(composer_js, body, "false"))
    }

    /// Bestehender Koordinaten-Klick (letzter Rueckfall, NOACTIVATE-tauglich).
    fn click_composer_coords(&self, x: f64, y: f64) {
        self.wake_renderer();
        let mut guard = self.driver.borrow_mut();
        if let Some(driver) = guard.as_mut() {
            let _ = driver.click_at(x, y);
        }
    }

    /// T-937: Composer fokussieren und VERIFIZIEREN — kein blinder Schuss.
    /// Reihenfolge: Tastatur (Tab), In-Page focus() (tabindex=-1), Klick.
    /// Der Klick wird beibehalten, bis der Tastaturweg gemessen besser ist
    /// (Non-Goal), aber auch nach dem Klick wird die Fokus-Lage aufgezeichnet.
    fn focus_composer_verified(&self, composer_js: &str, coords: &Value) -> FocusOutcome {
        if let Some(tries) = self.focus_composer_keyboard(composer_js) {
            return FocusOutcome {
                method: FocusMethod::Keyboard,
                arrived: true,
                tries,
                tabindex_fallback: false,
            };
        }
        if self.focus_composer_el_focus(composer_js) {
            return FocusOutcome {
                method: FocusMethod::ElFocus,
                arrived: true,
                tries: FOCUS_TAB_TRIES,
                tabindex_fallback: true,
            };
        }
        if let (Some(x), Some(y)) = (
            coords.get("x").and_then(Value::as_f64),
            coords.get("y").and_then(Value::as_f64),
        ) {
            self.click_composer_coords(x, y);
            std::thread::sleep(Duration::from_millis(80));
            let arrived = self.eval_bool(&self.composer_focus_probe_expr(composer_js));
            return FocusOutcome {
                method: FocusMethod::Click,
                arrived,
                tries: FOCUS_TAB_TRIES,
                tabindex_fallback: false,
            };
        }
        FocusOutcome {
            method: FocusMethod::None,
            arrived: false,
            tries: FOCUS_TAB_TRIES,
            tabindex_fallback: false,
        }
    }

    /// Fokus-Beobachtung (T-936/T-937) in die laufende Turn-Beobachtung eintragen.
    fn note_focus_outcome(&self, out: &FocusOutcome) {
        let method = match out.method {
            FocusMethod::Keyboard => "keyboard",
            FocusMethod::ElFocus => "el_focus",
            FocusMethod::Click => "click",
            FocusMethod::None => "none",
        };
        crate::brain_score::update_pending_turn(|obs| {
            obs.phase = crate::brain_score::SendPhase::Focus;
            obs.focus_arrived = Some(out.arrived);
            obs.focus_method = Some(method.to_string());
            obs.focus_tries = Some(out.tries);
            obs.tabindex_fallback = out.tabindex_fallback;
        });
    }

    /// Fuellt einen contenteditable Rich-Text-Editor absatzweise. Lexical
    /// verwirft bei `Input.insertText` alles hinter dem ersten Zeilenumbruch;
    /// `execCommand('insertParagraph')` geht dagegen durch seinen Editor-State.
    pub(super) fn fill_composer_rich_multiline(&self, composer_js: &str, text: &str) -> bool {
        // Viewport-clamped click target (see fill_composer): tall ProseMirror rects.
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){var top=Math.max(r.top,0),bot=Math.min(r.bottom,window.innerHeight||r.bottom),left=Math.max(r.left,0),right=Math.min(r.right,window.innerWidth||r.right);var clamp=(bot-top<1||right-left<1);if(clamp){top=Math.min(Math.max((r.top+r.bottom)/2,2),(window.innerHeight||600)-2);left=Math.min(Math.max((r.left+r.right)/2,2),(window.innerWidth||800)-2);return {x:left,y:top,w:r.width,h:r.height,clamp:clamp,i:i};}return {x:(left+right)/2,y:(top+bot)/2,w:r.width,h:r.height,clamp:clamp,i:i};}}";
        let coords = self
            .eval(&Self::js_scan(composer_js, coord_body, "null"))
            .unwrap_or(Value::Null);
        if coords.get("x").and_then(Value::as_f64).is_some()
            && coords.get("y").and_then(Value::as_f64).is_some()
        {
            self.note_composer_metrics(&coords);
        }
        // T-937: erst Tastatur (Tab), dann In-Page focus(), zuletzt Klick —
        // und danach verifiziert. Meist (kimi/Lexical) ist der Composer per
        // Tab erreichbar; der Koordinaten-Klick bleibt als letzter Rueckfall.
        let out = self.focus_composer_verified(composer_js, &coords);
        self.note_focus_outcome(&out);
        match out.method {
            FocusMethod::Click => {
                // Eingeschlagener Klickweg: in-process CDP bevorzugt, sonst portabler Body.
                let mut guard = self.driver.borrow_mut();
                if let Some(driver) = guard.as_mut() {
                    if driver.replace_multiline_text(text).is_ok() {
                        return true;
                    }
                }
            }
            FocusMethod::None => return false,
            _ => {}
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
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){var top=Math.max(r.top,0),bot=Math.min(r.bottom,window.innerHeight||r.bottom),left=Math.max(r.left,0),right=Math.min(r.right,window.innerWidth||r.right);var clamp=(bot-top<1||right-left<1);if(clamp){top=Math.min(Math.max((r.top+r.bottom)/2,2),(window.innerHeight||600)-2);left=Math.min(Math.max((r.left+r.right)/2,2),(window.innerWidth||800)-2);return {x:left,y:top,w:r.width,h:r.height,clamp:clamp,i:i};}return {x:(left+right)/2,y:(top+bot)/2,w:r.width,h:r.height,clamp:clamp,i:i};}}";
        let coords = self
            .eval(&Self::js_scan(composer_js, coord_body, "null"))
            .unwrap_or(Value::Null);
        if coords.get("x").and_then(Value::as_f64).is_some()
            && coords.get("y").and_then(Value::as_f64).is_some()
        {
            self.note_composer_metrics(&coords);
        }
        // T-937: verifizierte Fokussierung statt blindem Koordinaten-Klick.
        let out = self.focus_composer_verified(composer_js, &coords);
        self.note_focus_outcome(&out);
        if out.method == FocusMethod::None {
            return false;
        }
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
        // 1) Klickpunkt = Viewport-Schnitt des Composer-Rects (nicht gefunden -> false).
        //    ChatGPT-ProseMirror meldet bei grossen Prompts h=13k/y=-10k; geometrischer
        //    Mittelpunkt liegt dann ausserhalb der WebView (Live: Composer-Feld-Timeout).
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){var top=Math.max(r.top,0),bot=Math.min(r.bottom,window.innerHeight||r.bottom),left=Math.max(r.left,0),right=Math.min(r.right,window.innerWidth||r.right);var clamp=(bot-top<1||right-left<1);if(clamp){top=Math.min(Math.max((r.top+r.bottom)/2,2),(window.innerHeight||600)-2);left=Math.min(Math.max((r.left+r.right)/2,2),(window.innerWidth||800)-2);return {x:left,y:top,w:r.width,h:r.height,clamp:clamp,i:i};}return {x:(left+right)/2,y:(top+bot)/2,w:r.width,h:r.height,clamp:clamp,i:i};}}";
        let coords = self
            .eval(&Self::js_scan(composer_js, coord_body, "null"))
            .unwrap_or(Value::Null);
        if coords.get("x").and_then(Value::as_f64).is_some()
            && coords.get("y").and_then(Value::as_f64).is_some()
        {
            self.note_composer_metrics(&coords);
        }
        // 2) Composer fokussieren UND verifizieren — kein blinder Klick mehr.
        //    T-937: erst Tastatur (Tab + activeElement-Prüfung nach jedem
        //    Druck), dann In-Page focus() (greift auch bei tabindex=-1), erst
        //    danach der bisherige Koordinaten-Klick als letzter Rueckfall.
        //    Ohne Koordinaten UND ohne erreichten Fokus: kein Composer erreichbar.
        let out = self.focus_composer_verified(composer_js, &coords);
        self.note_focus_outcome(&out);
        if out.method == FocusMethod::None {
            return false;
        }
        let clear_body = "var el=Q(S[i]);if(el){el.focus();try{if('value' in el){el.value='';}else{el.textContent='';}el.dispatchEvent(new InputEvent('input',{bubbles:true}));}catch(e){}return true;}";
        let _ = self.eval_bool(&Self::js_scan(composer_js, clear_body, "false"));
        // 3) Echt tippen via PageDriver::insert_text — in Bloecken, damit der
        //    WebView-Loop zwischen den Bloecken ansprechbar bleibt. Ein einziger
        //    25k-Zeichen-execCommand blockiert ihn synchron ueber Sekunden
        //    (ProseMirror-Reflow eines 13k-px-Editors): jeder andere Befehl
        //    (Klick, Navigation, Verify) liefe dann in den 8s-Page-Befehl-Timeout
        //    ("eingefroren"), obwohl die Seite lebt und der Cursor blinkt.
        //    Kleine Texte unveraendert in einem Aufruf (kein Verhaltenswechsel).
        {
            let mut guard = self.driver.borrow_mut();
            if let Some(driver) = guard.as_mut() {
                const INSERT_CHUNK_CHARS: usize = 2000;
                let chars: Vec<char> = text.chars().collect();
                if chars.len() <= INSERT_CHUNK_CHARS {
                    let _ = driver.insert_text(text);
                } else {
                    for piece in chars.chunks(INSERT_CHUNK_CHARS) {
                        let part: String = piece.iter().collect();
                        if driver.insert_text(&part).is_err() {
                            break;
                        }
                    }
                }
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
        let needle = Self::composer_needle(text);
        let n = serde_json::to_string(&needle).unwrap_or_else(|_| "\"\"".into());
        let body = format!(
            "var el=Q(S[i]);if(el){{var v=('value' in el)?(el.value||''):(el.innerText||el.textContent||'');v=v.replace(/\\s+/g,' ');if(v.indexOf({n})!==-1)return true;}}"
        );
        self.eval_bool(&Self::js_scan(composer_js, &body, "false"))
    }

    /// Nadel fuer [`Self::composer_contains`]: die ersten sechs Woerter mit
    /// normalisiertem Leerraum. Ein rohes `take(8)` scheiterte, sobald der Prompt
    /// mit Umbruch oder Einrueckung beginnt: Der Editor rendert den Leerraum
    /// anders, `indexOf` traf nie, und der Aufrufer meldete faelschlich
    /// „Composer-Feld nicht gefunden". Beitrag aus einer deepseek-Pi-Session
    /// vom 2026-09-14 (T-962).
    pub(super) fn composer_needle(text: &str) -> String {
        text.split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    /// activeElement-Sonde gegen den Composer-Selektor (zum Registrieren).
    fn focus_probe_expr(composer_js: &str) -> String {
        WebBrainBackend::js_scan(
            composer_js,
            "var el=Q(S[i]);if(el){return (document.activeElement===el||el.matches(':focus'));}",
            "false",
        )
    }

    /// In-Page-focus()-Sonde (tabindex=-1-faehiger Rueckfallweg).
    fn el_focus_expr(composer_js: &str) -> String {
        WebBrainBackend::js_scan(
            composer_js,
            "var el=Q(S[i]);if(el){el.focus();return (document.activeElement===el||el.matches(':focus'));}",
            "false",
        )
    }

    fn backend_for(provider: &str, state: MockPageState) -> WebBrainBackend {
        let backend = WebBrainBackend::from_config(provider).expect("Brain-Konfiguration");
        backend.attach_page_driver(Box::new(MockPageDriver::new(state)));
        backend
    }

    /// T-937: Der Composer wird per Tastatur fokussiert — der Fokus kommt nach
    /// dem dritten Tab-Druck nachweislich an (`activeElement`-Pruefung), und
    /// das gelingt **ohne jede Koordinate** (Musterfall des blinden Klicks,
    /// der hier ersetzt wird).
    #[test]
    fn tastaturfokus_erreicht_composer_ohne_koordinaten() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let composer_js = sel.js("composer", &[]);
        // Keine coords registriert → coords eval liefert null. Die Sonde meldet
        // Treffer erst nach dem dritten Tab-Druck (2x false, danach true).
        let probe = focus_probe_expr(&composer_js);
        let state =
            MockPageState::new().on_eval_seq(probe, vec![json!(false), json!(false), json!(true)]);
        let state2 = state.clone();
        let backend = backend_for("qwen", state);

        backend.fill_composer(&composer_js, "Hallo Welt");

        // Fokusstrategie war keyboard, verifiziert, ohne Koordinaten gegriffen:
        // genau drei echte Tab-Drucke vor dem activeElement-Treffer.
        assert_eq!(state2.press_key_payloads().len(), 3);
        let obs = crate::brain_score::pending_turn_snapshot().expect("Turn-Beobachtung");
        assert_eq!(obs.focus_method.as_deref(), Some("keyboard"));
        assert_eq!(obs.focus_arrived, Some(true));
        assert_eq!(obs.focus_tries, Some(3));
        assert!(!obs.tabindex_fallback);
    }

    /// T-937 Rueckfallweg: Composer mit tabindex=-1 ist per Tab nie erreichbar
    /// (5 Fehlversuche), `el.focus()` greift aber und wird verifiziert — das
    /// bleibt von "Fokus kam nie an" unterscheidbar.
    #[test]
    fn tabindex_rueckfall_erreicht_composer_und_bleibt_unterscheidbar() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let composer_js = sel.js("composer", &[]);
        let probe = focus_probe_expr(&composer_js);
        let el_focus = el_focus_expr(&composer_js);
        let state = MockPageState::new()
            .on_eval_seq(
                probe,
                vec![
                    json!(false),
                    json!(false),
                    json!(false),
                    json!(false),
                    json!(false),
                ],
            )
            .on_eval(el_focus, json!(true));
        let state2 = state.clone();
        let backend = backend_for("qwen", state);

        backend.fill_composer(&composer_js, "Hallo Welt");

        // Alle 5 Tab-Versuche scheiterten (tabindex=-1), el_focus hat geholfen.
        assert_eq!(state2.press_key_payloads().len(), 5);
        let obs = crate::brain_score::pending_turn_snapshot().expect("Turn-Beobachtung");
        assert_eq!(obs.focus_method.as_deref(), Some("el_focus"));
        assert_eq!(obs.focus_arrived, Some(true));
        assert_eq!(obs.focus_tries, Some(5));
        assert!(obs.tabindex_fallback);
    }

    /// T-937: Weder Tastatur noch el.focus greifen, und es gibt keine
    /// Koordinaten → fill_composer scheitert klar, die Beobachtung heisst
    /// "none" statt einer Klick-Luege.
    #[test]
    fn fokus_kam_nie_an_bleibt_als_none_unterscheidbar() {
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let composer_js = sel.js("composer", &[]);
        let probe = focus_probe_expr(&composer_js);
        let el_focus = el_focus_expr(&composer_js);
        let state = MockPageState::new()
            .on_eval(probe, json!(false))
            .on_eval(el_focus, json!(false));
        let state2 = state.clone();
        let backend = backend_for("qwen", state);

        let filled = backend.fill_composer(&composer_js, "Hallo Welt");

        assert!(!filled);
        assert_eq!(state2.press_key_payloads().len(), 5);
        let obs = crate::brain_score::pending_turn_snapshot().expect("Turn-Beobachtung");
        assert_eq!(obs.focus_method.as_deref(), Some("none"));
        assert_eq!(obs.focus_arrived, Some(false));
        assert!(!obs.tabindex_fallback);
    }

    /// T-937: Der letzte Rueckfall ist weiterhin der Koordinaten-Klick
    /// (Non-Goal: nicht entfernt, bis der Tastaturweg gemessen besser ist);
    /// er wird als `click`-Methode ausgewiesen. Der ProseMirror-Fall (h=13k,
    /// clamp aktiv) liefert Koordinaten, obwohl geometrisch nichts klickbar ist.
    #[test]
    fn klick_bleibt_als_letzter_rueckfall_ausgewiesen() {
        // Coord-Body identisch zu composer.rs (Test-Harness-Duplikat wie in verify).
        let coord_body = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){var top=Math.max(r.top,0),bot=Math.min(r.bottom,window.innerHeight||r.bottom),left=Math.max(r.left,0),right=Math.min(r.right,window.innerWidth||r.right);var clamp=(bot-top<1||right-left<1);if(clamp){top=Math.min(Math.max((r.top+r.bottom)/2,2),(window.innerHeight||600)-2);left=Math.min(Math.max((r.left+r.right)/2,2),(window.innerWidth||800)-2);return {x:left,y:top,w:r.width,h:r.height,clamp:clamp,i:i};}return {x:(left+right)/2,y:(top+bot)/2,w:r.width,h:r.height,clamp:clamp,i:i};}}";
        let sel = backend_for("qwen", MockPageState::new()).selectors.clone();
        let composer_js = sel.js("composer", &[]);
        let probe = focus_probe_expr(&composer_js);
        let el_focus = el_focus_expr(&composer_js);
        let coords_expr = WebBrainBackend::js_scan(&composer_js, coord_body, "null");
        let state = MockPageState::new()
            .on_eval(coords_expr, json!({"x": 10.0, "y": 12.0, "clamp": true}))
            .on_eval(probe, json!(false))
            .on_eval(el_focus, json!(false));
        let backend = backend_for("qwen", state);

        backend.fill_composer(&composer_js, "Hallo Welt");

        let obs = crate::brain_score::pending_turn_snapshot().expect("Turn-Beobachtung");
        assert_eq!(obs.focus_method.as_deref(), Some("click"));
        assert_eq!(obs.focus_arrived, Some(false));
        assert!(!obs.tabindex_fallback);
    }
}
