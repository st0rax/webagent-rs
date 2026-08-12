//! Bedienung der Oberflaechen-Steuerelemente eines Brains: Umschalter,
//! Aufklappmenues, Segmentleisten, Modellwahl, Bereichswechsel.
//!
//! Kindmodul von `browser`, nicht Geschwister: nur so erreicht der Cluster
//! die privaten Interna des Backends (`driver`, `sel`, `eval*`, `js_scan`),
//! ohne dass ein Dutzend Helfer oeffentlich gemacht werden muesste. Ein erster
//! Versuch als Nachbarmodul haette genau das erzwungen — und die Kapselung
//! damit schlechter gemacht als vor dem Refactoring.

use super::WebBrainBackend;
use crate::brain::BrainBackend;
use std::time::Duration;

/// Selektorschluessel der Faehigkeit `temporary_chat` (siehe `capability.rs`).
pub const TEMPORARY_CHAT_KEY: &str = "temporary_chat_button";

impl WebBrainBackend {
    /// Zustand eines Umschalters als Zeichenkette: `aria-pressed`,
    /// `aria-checked`, `data-state` oder — als letzter Ausweg — die
    /// Klassenliste. Leer, wenn nichts matcht.
    ///
    /// Warum die Klassenliste mitzaehlt: viele Oberflaechen markieren einen
    /// aktiven Umschalter nur ueber eine CSS-Klasse. Ohne sie waere bei
    /// solchen Knoepfen kein Vorher/Nachher feststellbar — und ein Klick ohne
    /// feststellbare Wirkung ist von einem Klick ins Leere nicht zu
    /// unterscheiden.
    fn toggle_state(&self, key: &str) -> String {
        let expr = self.toggle_state_expr(key);
        self.eval_str(&expr)
    }

    /// Das JS hinter [`Self::toggle_state`], als eigene Funktion, damit ein Test
    /// dem Mock-Driver exakt dieselbe Ausdrucksform vorlegen kann wie der
    /// Produktivpfad. Zwei handgepflegte Kopien desselben Skripts waeren genau
    /// die Art doppelter Auslesepfad, an der Fixes hier schon vorbeigelaufen
    /// sind.
    fn toggle_state_expr(&self, key: &str) -> String {
        // Das JS selbst steht in `browser::js` — es hat einen zweiten Aufrufer
        // (`brain_probe::verify`), der denselben Ausdruck mit einem fertigen
        // Selektor statt eines Konfigurationsschluessels baut. Der Unterschied
        // ist genau diese eine Zeile.
        crate::browser::js::toggle_state_expr_for(&self.sel(key))
    }

    /// Waehlt ueber mehrere Menuestufen, z.B. `["Aufwand", "Hoch"]`.
    ///
    /// Claude legt die Denkstufe in ein Untermenue des Modellmenues
    /// ("Aufwand › Mittel"). Ein einstufiger Klick erreicht sie nicht, und der
    /// Beleg bleibt derselbe: die Beschriftung des Menuebuttons muss
    /// anschliessend den letzten Pfadschritt tragen.
    pub fn select_in_menu_path(
        &mut self,
        menu_key: &str,
        option_key: &str,
        path: &[&str],
    ) -> Result<String, String> {
        let Some(last) = path.last() else {
            return Err("leerer Pfad".into());
        };
        let last_l = last.trim().to_lowercase();
        let before = self.menu_label(menu_key);
        if before.to_lowercase().contains(&last_l) {
            return Ok(format!("{before} (bereits aktiv, kein Wechsel noetig)"));
        }
        if !self.open_menu(menu_key) {
            return Err(format!("'{menu_key}' nicht anklickbar"));
        }
        for step in path {
            let step_l = step.trim().to_lowercase();
            let list = Self::js_selectors(&self.sel(option_key));
            let needle = serde_json::to_string(&step_l).unwrap_or_else(|_| "\"\"".into());
            let expr = format!(
                "(function(){{{prelude}var S={list};var n={needle};for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').toLowerCase();if(t.indexOf(n)!==-1){{var e=els[k].closest('button,[role=menuitem],[role=option],[class*=item]')||els[k];e.click();return true;}}}}}}catch(e){{}}}}return false;}})()",
                prelude = Self::JS_SEL_PRELUDE,
                list = list,
                needle = needle
            );
            if !self.eval_bool(&expr) {
                let _ = self.press_key_escape();
                return Err(format!("Pfadschritt '{step}' nicht im Menue gefunden"));
            }
            std::thread::sleep(Duration::from_millis(1000));
        }
        let after = self.menu_label(menu_key);
        if after.to_lowercase().contains(&last_l) {
            return Ok(after);
        }
        Err(format!(
            "Pfad {path:?} geklickt, aber Beschriftung zeigt weiterhin '{after}' (vorher '{before}')"
        ))
    }

    /// Oeffnet einen Bereich der Oberflaeche (Projekte, Bibliothek, …) und
    /// belegt den Wechsel ueber die URL.
    ///
    /// Anders als bei Umschaltern gibt es hier keinen Zustand am Knopf — der
    /// Beleg ist, dass die Seite anschliessend woanders steht. Bleibt die URL
    /// gleich, ist der Klick verpufft, und das wird als Fehler gemeldet statt
    /// als Erfolg verbucht.
    pub fn open_section(&mut self, key: &str) -> Result<(String, String), String> {
        if self.sel(key).is_empty() {
            return Err(format!("kein '{key}' konfiguriert"));
        }
        let before = self.get_conversation_ref().unwrap_or_default();
        if !self.click_toggle(key) {
            return Err(format!("'{key}' nicht anklickbar"));
        }
        std::thread::sleep(Duration::from_millis(1500));
        let mut after = self.get_conversation_ref().unwrap_or_default();
        if after == before && self.click_real(key) {
            std::thread::sleep(Duration::from_millis(1500));
            after = self.get_conversation_ref().unwrap_or_default();
        }
        if after == before {
            return Err(format!(
                "'{key}' angeklickt, aber die Seite steht weiterhin auf '{after}'"
            ));
        }
        Ok((before, after))
    }

    /// Waehlt einen Eintrag einer Segmentleiste (alle Optionen dauerhaft
    /// sichtbar, kein Aufklappen) und belegt, dass er danach aktiv ist.
    ///
    /// Deepseeks `Instant | Expert | Vision` ist so gebaut: es gibt kein Menue
    /// zum Oeffnen und keine gemeinsame Beschriftung, die man gegenlesen
    /// koennte. Der Beleg ist deshalb der Zustand des GEWAEHLTEN Eintrags —
    /// er muss nach dem Klick als ausgewaehlt markiert sein, vorher nicht.
    pub fn select_segment(&mut self, option_key: &str, want: &str) -> Result<String, String> {
        let want_l = want.trim().to_lowercase();
        if want_l.is_empty() {
            return Err("kein Zielwert angegeben".into());
        }
        if self.sel(option_key).is_empty() {
            return Err(format!("kein '{option_key}' konfiguriert"));
        }
        // Signatur ALLER Stellungen statt nur der gewaehlten.
        //
        // Deepseeks Segmente tragen weder am Knopf noch am naechsten Vorfahren
        // mit Klasse ein Auswahl-Attribut — vor wie nach dem Klick identisch.
        // Die Markierung sitzt also woanders: an einem Container oder einem
        // verschobenen Indikator. Wandert die Auswahl, aendert sich aber die
        // Gesamtsignatur der Leiste, ganz gleich wo der Marker haengt. Der
        // Beleg wird damit unabhaengig davon, wie die Oberflaeche ihre Auswahl
        // ausdrueckt.
        let strip_state = |s: &Self| -> String {
            let list = Self::js_selectors(&s.sel(option_key));
            let expr = format!(
                "(function(){{{prelude}var S={list};var out=[];for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var e=els[k].closest('button,[role=button],[role=tab],[class]')||els[k];var d=[];for(var a=0;a<e.attributes.length;a++){{var at=e.attributes[a];if(at.name.indexOf('data-')===0||at.name.indexOf('aria-')===0)d.push(at.name+'='+at.value);}}d.sort();var p=e.parentElement;var pc=p?((p.className||'')+''):'';out.push(((e.innerText||e.textContent||'')+'').trim().slice(0,20)+'{{'+d.join(';')+'|'+((e.className||'')+'')+'|'+pc+'}}');}}}}catch(e){{}}}}out.sort();return out.join('~');}})()",
                prelude = Self::JS_SEL_PRELUDE,
                list = list
            );
            s.eval_str(&expr)
        };
        let state = |s: &Self| -> String {
            let list = Self::js_selectors(&s.sel(option_key));
            let needle = serde_json::to_string(&want_l).unwrap_or_else(|_| "\"\"".into());
            let expr = format!(
                "(function(){{{prelude}var S={list};var n={needle};for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').toLowerCase();if(t.indexOf(n)!==-1){{var e=els[k].closest('button,[role=button],[role=tab],[class*=button],[class*=btn],[class*=tab],[class]')||els[k];var d=[];for(var a=0;a<e.attributes.length;a++){{var at=e.attributes[a];if(at.name.indexOf('data-')===0||at.name.indexOf('aria-')===0)d.push(at.name+'='+at.value);}}d.sort();return d.join(';')+'|'+((e.className||'')+'');}}}}}}catch(e){{}}}}return '';}})()",
                prelude = Self::JS_SEL_PRELUDE,
                list = list,
                needle = needle
            );
            s.eval_str(&expr)
        };
        let before = state(self);
        if before.is_empty() {
            return Err(format!("'{want}' nicht in '{option_key}' gefunden"));
        }
        let strip_before = strip_state(self);
        // Klick auf genau diesen Eintrag (synthetisch, dann echt).
        let list = Self::js_selectors(&self.sel(option_key));
        let needle = serde_json::to_string(&want_l).unwrap_or_else(|_| "\"\"".into());
        let click_expr = format!(
            "(function(){{{prelude}var S={list};var n={needle};for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').toLowerCase();if(t.indexOf(n)!==-1){{var e=els[k].closest('button,[role=button],[role=tab],[class*=button],[class*=btn],[class*=tab],[class]')||els[k];e.click();return true;}}}}}}catch(e){{}}}}return false;}})()",
            prelude = Self::JS_SEL_PRELUDE,
            list = list,
            needle = needle
        );
        if !self.eval_bool(&click_expr) {
            return Err(format!("'{want}' nicht anklickbar"));
        }
        std::thread::sleep(Duration::from_millis(1000));
        let after = state(self);
        if after != before {
            return Ok(after);
        }
        // Der gewaehlte Eintrag selbst zeigt nichts — dann muss die Leiste als
        // Ganzes den Wechsel verraten. Tut sie es auch nicht, ist der Klick
        // nicht nachweisbar angekommen.
        let strip_after = strip_state(self);
        if strip_after != strip_before {
            return Ok(format!("{after} (Wechsel an der Leiste belegt)"));
        }
        Err(format!(
            "'{want}' angeklickt, aber weder am Eintrag noch an der Leiste ein \
             Wechsel feststellbar (weiterhin '{after}')"
        ))
    }

    /// Oeffnet ein Aufklappmenue und wartet, bis es wirklich offen ist.
    ///
    /// Erst synthetisch, dann per echtem Mausklick — qwens Denkstufen-Menue
    /// blieb auf `element.click()` zu (Screenshot danach unveraendert), weil
    /// die Oberflaeche auf pointerdown lauscht. Ob das Menue offen ist, misst
    /// `aria-expanded`; fehlt das Attribut, bleibt es beim ersten Versuch.
    fn open_menu(&self, menu_key: &str) -> bool {
        let expanded = |s: &Self| -> String {
            let list = Self::js_selectors(&s.sel(menu_key));
            let expr = Self::js_scan(
                &list,
                "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[aria-expanded]')||el;return (t.getAttribute('aria-expanded')||'');}",
                "\"\"",
            );
            s.eval_str(&expr)
        };
        if !self.click_toggle(menu_key) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(900));
        let e1 = expanded(self);
        if e1 == "false" && self.click_real(menu_key) {
            std::thread::sleep(Duration::from_millis(900));
        }
        true
    }

    /// Mittelpunkt des klickbaren Vorfahren im Viewport, oder `None`.
    fn click_point(&self, key: &str) -> Option<(f64, f64)> {
        let list = Self::js_selectors(&self.sel(key));
        let expr = Self::js_scan(
            &list,
            "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;var r=t.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}",
            "null",
        );
        let v = self.eval(&expr).ok()?;
        Some((v.get("x")?.as_f64()?, v.get("y")?.as_f64()?))
    }

    /// Echter Mausklick auf den klickbaren Vorfahren.
    ///
    /// `element.click()` loest nur ein synthetisches `click`-Ereignis aus.
    /// Oberflaechen, die auf `pointerdown`/`mousedown` lauschen, reagieren
    /// darauf nicht — real gesehen an qwens Denkstufen-Menue, das nach dem
    /// Klick unveraendert blieb. Ein Klick an den Koordinaten geht den
    /// vollstaendigen Ereignisweg und erreicht auch diese.
    fn click_real(&self, key: &str) -> bool {
        let Some((x, y)) = self.click_point(key) else {
            return false;
        };
        let mut guard = self.driver.borrow_mut();
        let Some(driver) = guard.as_mut() else {
            return false;
        };
        driver.click_at(x, y).is_ok()
    }

    /// Klickt den schaltbaren Vorfahren des Treffers (siehe `toggle_state`).
    fn click_toggle(&self, key: &str) -> bool {
        let expr = self.click_toggle_expr(key);
        self.eval_bool(&expr)
    }

    /// Schaltet einen Umschalter (z.B. deepseeks `reasoning_toggle`/DeepThink)
    /// aus, wenn er erkennbar aktiv ist. Kein Blind-Klick: belegt kein Attribut
    /// den aktiven Zustand, wird nichts angefasst. `true`, wenn der Zustand
    /// danach "aus" ist (bereits aus oder erfolgreich ausgeklickt).
    ///
    /// DeepThink eingeschaltet streamt deepseek ~100 s Reasoning, das die
    /// Stabilitäts-Erkennung blockiert — jede Turn kostet dann den vollen
    /// wait_response-Timeout (gemessen: konstant 110 s). Ausgeschaltet antwortet
    /// deepseek in Sekunden.
    pub(crate) fn disable_toggle(&self, key: &str) -> bool {
        if self.sel(key).is_empty() {
            return true;
        }
        let state = self.toggle_state(key).to_lowercase();
        let active = state.contains("aria-pressed=true")
            || state.contains("aria-checked=true")
            || state.contains("data-state=on")
            || state.contains("data-active=true");
        if !active {
            return true;
        }
        crate::bench_events::eprint_line(&format!(
            "[browser] {}: {key} ist aktiv, wird ausgeschaltet ({state})",
            self.brain_id
        ));
        self.click_toggle(key)
    }

    /// Das JS hinter [`Self::click_toggle`] — aus demselben Grund ausgelagert
    /// wie [`Self::toggle_state_expr`].
    fn click_toggle_expr(&self, key: &str) -> String {
        crate::browser::js::click_toggle_expr_for(&self.sel(key))
    }

    /// Schaltet eine Option um und belegt, dass sich dabei wirklich etwas
    /// geaendert hat. Gibt (vorher, nachher) zurueck.
    ///
    /// Ohne den Zustandsvergleich waere jeder Klick ein "Erfolg": die Seite
    /// meldet nichts, wenn ins Leere geklickt wurde. Genau daran ist der
    /// Modellwechsel bei chatgpt zunaechst vorbeigelaufen.
    pub fn toggle_option(&mut self, key: &str) -> Result<(String, String), String> {
        if self.sel(key).is_empty() {
            return Err(format!("kein '{key}' konfiguriert"));
        }
        let before = self.toggle_state(key);
        if !self.click_toggle(key) {
            return Err(format!("'{key}' nicht anklickbar"));
        }
        std::thread::sleep(Duration::from_millis(900));
        let mut after = self.toggle_state(key);
        if before == after {
            // Zweiter Anlauf mit echtem Mausklick: manche Oberflaechen
            // ignorieren das synthetische `element.click()` und lauschen nur
            // auf pointerdown/mousedown.
            if self.click_real(key) {
                std::thread::sleep(Duration::from_millis(900));
                after = self.toggle_state(key);
            }
        }
        if before == after {
            return Err(format!(
                "'{key}' angeklickt (synthetisch und per Maus), aber kein \
                 Zustandswechsel feststellbar (weiterhin '{after}')"
            ));
        }
        // Ein leerer Zustand danach heisst: das Element ist weg, nicht dass es
        // eingeschaltet waere. Real bei kimi gesehen — die Chip-Leiste rendert
        // nach dem Klick neu, der Selektor findet nichts mehr, und der blosse
        // Unterschied vorher/nachher haette das als Erfolg durchgehen lassen.
        // Verschwinden ist kein Beleg fuer Wirkung.
        if after.trim().is_empty() && !before.trim().is_empty() {
            return Err(format!(
                "'{key}' angeklickt, aber das Element ist danach nicht mehr \
                 auffindbar (vorher '{before}') — Verschwinden ist kein Beleg"
            ));
        }
        Ok((before, after))
    }

    /// Aktueller Zustand des Temporaer-Chat-Knopfs (leer = nicht auffindbar).
    pub fn temporary_chat_state(&self) -> String {
        self.toggle_state(TEMPORARY_CHAT_KEY)
    }

    /// Schaltet den temporaeren Chat um und belegt den Wechsel.
    ///
    /// Bewusst nur eine benannte Tuer vor [`Self::toggle_option`] statt eines
    /// eigenen Antriebs: der Temporaer-Chat ist ein gewoehnlicher Umschalter,
    /// und der Beleg — vorher/nachher lesen, bei Gleichstand echter Mausklick,
    /// Verschwinden zaehlt nicht als Wirkung — ist derselbe, den `web_search`
    /// und `reasoning_toggle` schon tragen. Eine zweite Fassung derselben
    /// Pruefung waere ein zweiter Auslesepfad, der beim naechsten Fix stehen
    /// bleibt.
    ///
    /// Der Selektorname steht hier fest, weil er die Faehigkeit `temporary_chat`
    /// aus `capability.rs` bedient; wer einen anderen Knopf schalten will,
    /// nimmt direkt `toggle_option`.
    pub fn toggle_temporary_chat(&mut self) -> Result<(String, String), String> {
        self.toggle_option(TEMPORARY_CHAT_KEY)
    }

    /// Öffnet die Oberfläche für interaktive Bedienung (Modellwahl u.ä.) und
    /// wartet, bis die Bedienelemente wirklich da sind. Gegenstück: `close_ui`.
    pub fn open_for_ui(&mut self, headless: bool) -> Result<(), String> {
        self.start(headless)?;
        self.dismiss_consent();
        let _ = self.ensure_ready(15.0);
        self.wait_for_labeled_controls();
        Ok(())
    }

    /// Schliesst die mit `open_for_ui` geoeffnete Oberflaeche.
    pub fn close_ui(&mut self) -> Result<(), String> {
        self.stop()
    }

    /// Beschriftung des Modell-Menüs (= aktuell gewähltes Modell), leer wenn
    /// kein `model_menu` konfiguriert ist oder nichts sichtbar.
    pub fn current_model(&self) -> String {
        let list = Self::js_selectors(&self.sel("model_menu"));
        let expr = Self::js_scan(
            &list,
            "var el=Q(S[i]);if(el){var t=((el.innerText||el.textContent||'')+'').replace(/\\s+/g,' ').trim();if(t)return t;}",
            "\"\"",
        );
        self.eval_str(&expr)
    }

    /// Öffnet das Modell-Menü und liest die wählbaren Einträge.
    ///
    /// Öffnet das Menü wirklich (ein geschlossenes Menü hat keine Einträge im
    /// DOM) und schließt es danach per Escape wieder, damit die Seite im
    /// selben Zustand zurückbleibt wie vorher.
    pub fn list_models(&mut self) -> Result<Vec<String>, String> {
        self.list_menu("model_menu", "model_option")
    }

    /// Wie `list_models`, aber fuer ein beliebiges Menue.
    pub fn list_menu(&mut self, menu_key: &str, option_key: &str) -> Result<Vec<String>, String> {
        if self.sel(menu_key).is_empty() {
            return Err(format!("kein '{menu_key}' konfiguriert"));
        }
        if !self.open_menu(menu_key) {
            return Err(format!("'{menu_key}' nicht anklickbar"));
        }
        let list = Self::js_selectors(&self.sel(option_key));
        let expr = format!(
            "(function(){{{prelude}var S={list};var out=[];for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').replace(/\\s+/g,' ').trim();if(t&&out.indexOf(t)<0)out.push(t);}}if(out.length)break;}}catch(e){{}}}}return out;}})()",
            prelude = Self::JS_SEL_PRELUDE,
            list = list
        );
        let models = self
            .eval(&expr)
            .ok()
            .and_then(|v| {
                v.as_array().map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let _ = self.press_key_escape();
        Ok(models)
    }

    /// Wählt ein Modell und **prüft nach**, dass die Auswahl wirklich griff.
    ///
    /// Die Prüfung ist der eigentliche Kern: ein Klick, der ins Leere geht,
    /// hinterlässt keine Fehlermeldung — die Seite sieht danach exakt so aus
    /// wie vorher. Ohne Nachlesen der Menü-Beschriftung würde webagent einen
    /// Fehlschlag als Erfolg verbuchen und anschließend mit dem falschen
    /// Modell weiterarbeiten, ohne dass es jemand merkt. Deshalb gilt der
    /// Wechsel nur als gelungen, wenn die Beschriftung danach den gewünschten
    /// Namen trägt.
    pub fn switch_model(&mut self, want: &str) -> Result<String, String> {
        self.select_in_menu("model_menu", "model_option", want)
    }

    /// Beschriftung eines Menue-Knopfs (= aktuell gewaehlter Eintrag).
    pub fn menu_label(&self, menu_key: &str) -> String {
        let list = Self::js_selectors(&self.sel(menu_key));
        let expr = Self::js_scan(
            &list,
            "var el=Q(S[i]);if(el){var t=((el.innerText||el.textContent||'')+'').replace(/\\s+/g,' ').trim();if(t)return t;}",
            "\"\"",
        );
        self.eval_str(&expr)
    }

    /// Waehlt einen Eintrag aus einem Aufklappmenue und prueft nach.
    ///
    /// Verallgemeinerung von `switch_model`: Modellwahl, Denkstufe und jedes
    /// weitere Menue funktionieren gleich — aufklappen, Eintrag per Text
    /// treffen, Beschriftung gegenlesen. Ein Klick ins Leere hinterlaesst
    /// keine Spur, deshalb entscheidet allein die Beschriftung danach.
    pub fn select_in_menu(
        &mut self,
        menu_key: &str,
        option_key: &str,
        want: &str,
    ) -> Result<String, String> {
        let want_l = want.trim().to_lowercase();
        if want_l.is_empty() {
            return Err("kein Zielwert angegeben".into());
        }
        let before = self.menu_label(menu_key);
        // Steht das Ziel schon, ist JEDE Nachpruefung trivial erfuellt — auch
        // wenn der Klick ins Leere ging. Real gesehen: chatgpt meldete
        // "umgestellt auf 'ChatGPT'", obwohl nichts passiert war, weil der
        // Name vorher schon dastand. Deshalb hier abbiegen und ehrlich
        // "bereits aktiv" melden, statt einen Wechsel zu behaupten.
        if before.to_lowercase().contains(&want_l) {
            return Ok(format!("{before} (bereits aktiv, kein Wechsel noetig)"));
        }
        if self.sel(menu_key).is_empty() {
            return Err(format!("kein '{menu_key}' konfiguriert"));
        }
        if !self.open_menu(menu_key) {
            return Err(format!("'{menu_key}' nicht anklickbar"));
        }

        // Perplexity (Radix-Portal) rendert den Menue-Content zeitversetzt nach
        // aria-expanded: open_menu belegt nur den offenen Trigger, die Eintraege
        // kommen erst danach ins DOM. Erst warten, bis ein Options-Selektor
        // matcht, sonst klickt der Scan ins Leere (gemessen 2026-08-12).
        let list = Self::js_selectors(&self.sel(option_key));
        let needle = serde_json::to_string(&want_l).unwrap_or_else(|_| "\"\"".into());
        let mut options_sichtbar = self.wait_for_options(&list, 26, 150);
        // Ein frisch geoeffnetes Menue eines gerade geladenen Webviews zeigt den
        // Content oft erst beim ZWEITEN Oeffnen (perplexity, gemessen 2026-08-12:
        // Erst-Oeffnen 0 Eintraege, Reopen zuverlaessig alle). Einmalig retry.
        if !options_sichtbar {
            let _ = self.press_key_escape();
            std::thread::sleep(Duration::from_millis(400));
            if self.open_menu(menu_key) {
                options_sichtbar = self.wait_for_options(&list, 26, 150);
            }
        }
        if !options_sichtbar {
            let _ = self.press_key_escape();
            return Err(format!(
                "'{want}': Eintraege von '{option_key}' trotz Warten/Reopen nicht im DOM"
            ));
        }

        // 1. Versuch: synthetisches element.click() inkl. Pointer-Dispatch —
        // fuer die meisten Brains ausreichend.
        let expr = format!(
            "(function(){{{prelude}var S={list};var n={needle};function clk(e){{var r=e.getBoundingClientRect();var cx=r.left+r.width/2,cy=r.top+r.height/2;var pd=new PointerEvent('pointerdown',{{clientX:cx,clientY:cy,bubbles:true,pointerId:1,isPrimary:true,button:0,pointerType:'mouse'}});var pu=new PointerEvent('pointerup',{{clientX:cx,clientY:cy,bubbles:true,pointerId:1,isPrimary:true,button:0,pointerType:'mouse'}});['pointerdown','mousedown','pointerup','mouseup','click'].forEach(function(t2){{e.dispatchEvent(t2==='pointerdown'?pd:t2==='pointerup'?pu:new MouseEvent(t2,{{clientX:cx,clientY:cy,bubbles:true,button:0}}));}});}}for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').toLowerCase();if(t.indexOf(n)!==-1){{clk(els[k]);return true;}}}}}}catch(e){{}}}}return false;}})()",
            prelude = Self::JS_SEL_PRELUDE,
            list = list,
            needle = needle
        );
        if !self.eval_bool(&expr) {
            let _ = self.press_key_escape();
            return Err(format!("'{want}' steht nicht im Menue '{option_key}'"));
        }
        std::thread::sleep(Duration::from_millis(1200));

        let after = self.current_model();
        if after.to_lowercase().contains(&want_l) {
            return Ok(after);
        }

        // Interaktionsnachweis: schliesst sich das Menue nach einem Klick, wurde
        // der Eintrag verarbeitet — entscheidend fuer Brains wie perplexity, die
        // den Modellnamen nirgends im DOM spiegeln (Button zeigt statisch
        // "Modell", keine Auswahl-Markierung, kein localStorage-Eintrag).
        let mut menue_zu = self.menuitem_count() == 0;

        // 2. Versuch: echter Mausklick auf die Item-Koordinaten. Perplexity
        // lauscht nur auf pointerdown; selbst der Pointer-Dispatch des
        // synthetischen Klicks waehlt dort nichts aus, der echte Klick schon
        // (am 2026-08-12 gemessen: Menue schloss sich = Eintrag verarbeitet).
        if !menue_zu {
            if let Some((x, y)) = self.option_point(&list, &needle) {
                let mut guard = self.driver.borrow_mut();
                if let Some(driver) = guard.as_mut() {
                    if driver.click_at(x, y).is_ok() {
                        std::thread::sleep(Duration::from_millis(1500));
                        let after2 = self.current_model();
                        if after2.to_lowercase().contains(&want_l) {
                            return Ok(after2);
                        }
                        menue_zu = self.menuitem_count() == 0;
                    }
                }
            }
        }
        Err(format!(
            "Wechsel nicht bestaetigt: Menue zeigt weiterhin '{}' (vorher '{}'), erwartet '{}' (Menue nach Klick {})",
            after,
            before,
            want,
            if menue_zu { "geschlossen - Eintrag verarbeitet, aber Namensbestaetigung fehlt" } else { "noch offen - Klick wohl ins Leere" }
        ))
    }

    /// Anzahl der `[role=menuitem]`-Elemente im DOM — das zuverlaessige Signal
    /// dafuer, ob ein Radix-Menue mit Content offen ist (Options-Selektoren
    /// matchen auch nach dem Schliessen weiter).
    fn menuitem_count(&self) -> i64 {
        self.eval_str("(function(){return JSON.stringify(document.querySelectorAll('[role=menuitem]').length);})()")
            .trim()
            .parse::<i64>()
            .unwrap_or(0)
    }

    /// Wartet, bis ein Options-Selektor im DOM matcht (Menue-Content rendert
    /// bei Radix-Portalen zeitversetzt). Rueckgabe: ob binnen
    /// `versuche * interval_ms` ein Treffer auftrat.
    fn wait_for_options(&self, list: &str, versuche: usize, interval_ms: u64) -> bool {
        let expr = format!(
            "(function(){{{prelude}var S={list};var n=0;for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);if(els.length){{n=els.length;break;}}}}catch(e){{}}}}return JSON.stringify(n);}})()",
            prelude = Self::JS_SEL_PRELUDE,
            list = list
        );
        for _ in 0..versuche {
            if self.eval_str(&expr).trim().parse::<i64>().unwrap_or(0) > 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(interval_ms));
        }
        false
    }

    /// Mittelpunkt des Ziel-Eintrags im Viewport, oder `None`.
    fn option_point(&self, list: &str, needle: &str) -> Option<(f64, f64)> {
        let expr = format!(
            "(function(){{{prelude}var S={list};var n={needle};for(var i=0;i<S.length;i++){{try{{var els=QA(S[i]);for(var k=0;k<els.length;k++){{var t=((els[k].innerText||els[k].textContent||'')+'').toLowerCase();if(t.indexOf(n)!==-1){{var r=els[k].getBoundingClientRect();return JSON.stringify(r.left+r.width/2)+','+JSON.stringify(r.top+r.height/2);}}}}}}catch(e){{}}}}return 'false';}})()",
            prelude = Self::JS_SEL_PRELUDE,
            list = list,
            needle = needle
        );
        let c = self.eval_str(&expr);
        let (xs, ys) = c.split_once(',')?;
        let x = xs.trim().parse::<f64>().ok()?;
        let y = ys.trim().parse::<f64>().ok()?;
        Some((x, y))
    }

    /// Escape ans Dokument — schliesst offene Menues.
    fn press_key_escape(&self) -> Result<(), String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        driver
            .press_key("Escape", "Escape", 27, "")
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    /// qwen-Backend mit angehaengtem Mock-Driver. Die Selektoren kommen aus der
    /// echten `selectors/qwen.json` — ein Test gegen erfundene Selektoren wuerde
    /// nicht merken, wenn der Schluessel dort verschwindet.
    fn qwen_with(state: MockPageState) -> WebBrainBackend {
        let backend = WebBrainBackend::from_config("qwen").expect("qwen config");
        backend.attach_page_driver(Box::new(MockPageDriver::new(state)));
        backend
    }

    #[test]
    fn qwen_has_temporary_chat_selector() {
        let backend = WebBrainBackend::from_config("qwen").expect("qwen config");
        assert!(
            !backend.sel(TEMPORARY_CHAT_KEY).is_empty(),
            "ohne '{TEMPORARY_CHAT_KEY}' in selectors/qwen.json faehrt der Antrieb ins Leere"
        );
    }

    #[test]
    fn toggle_temporary_chat_reports_state_change() {
        let probe = WebBrainBackend::from_config("qwen").expect("qwen config");
        let state_expr = probe.toggle_state_expr(TEMPORARY_CHAT_KEY);
        let click_expr = probe.click_toggle_expr(TEMPORARY_CHAT_KEY);
        let mut backend = qwen_with(
            MockPageState::new()
                .on_eval(click_expr, json!(true))
                .on_eval_seq(
                    state_expr,
                    vec![
                        json!("aria-pressed=false|chat-btn"),
                        json!("aria-pressed=true|chat-btn"),
                    ],
                ),
        );
        let (before, after) = backend.toggle_temporary_chat().expect("Wechsel belegt");
        assert_eq!(before, "aria-pressed=false|chat-btn");
        assert_eq!(after, "aria-pressed=true|chat-btn");
    }

    /// Der Kern der Regel „kein Beleg, kein Level": ein Klick, der ankommt, aber
    /// nichts aendert, muss ein Fehlschlag sein. Wuerde er als Erfolg
    /// durchgehen, stuende `temporary_chat` als fahrbar in der Bilanz, ohne es
    /// je gewesen zu sein.
    #[test]
    fn toggle_temporary_chat_without_state_change_fails() {
        let probe = WebBrainBackend::from_config("qwen").expect("qwen config");
        let state_expr = probe.toggle_state_expr(TEMPORARY_CHAT_KEY);
        let click_expr = probe.click_toggle_expr(TEMPORARY_CHAT_KEY);
        let mut backend = qwen_with(
            MockPageState::new()
                .on_eval(click_expr, json!(true))
                .on_eval(state_expr, json!("aria-pressed=false|chat-btn")),
        );
        let err = backend
            .toggle_temporary_chat()
            .expect_err("ohne Zustandswechsel darf das kein Erfolg sein");
        assert!(err.contains("kein Zustandswechsel"), "{err}");
    }

    /// Verschwindet der Knopf nach dem Klick, ist das kein Beleg fuer Wirkung —
    /// dieselbe Falle, die bei kimi einmal als Erfolg gezaehlt wurde.
    #[test]
    fn toggle_temporary_chat_vanishing_button_is_no_proof() {
        let probe = WebBrainBackend::from_config("qwen").expect("qwen config");
        let state_expr = probe.toggle_state_expr(TEMPORARY_CHAT_KEY);
        let click_expr = probe.click_toggle_expr(TEMPORARY_CHAT_KEY);
        let mut backend = qwen_with(
            MockPageState::new()
                .on_eval(click_expr, json!(true))
                .on_eval_seq(
                    state_expr,
                    vec![json!("aria-pressed=false|chat-btn"), json!("")],
                ),
        );
        let err = backend
            .toggle_temporary_chat()
            .expect_err("Verschwinden ist kein Beleg");
        assert!(err.contains("nicht mehr"), "{err}");
    }

    #[test]
    fn toggle_temporary_chat_without_selector_fails() {
        // chatgpt hat den Knopf (noch) nicht konfiguriert — dort muss der
        // Antrieb sofort und ehrlich abbrechen statt blind zu klicken.
        let mut backend = WebBrainBackend::from_config("chatgpt").expect("chatgpt config");
        assert!(
            backend.sel(TEMPORARY_CHAT_KEY).is_empty(),
            "Test setzt voraus, dass chatgpt keinen temporary_chat_button hat"
        );
        backend.attach_page_driver(Box::new(MockPageDriver::new(MockPageState::new())));
        let err = backend
            .toggle_temporary_chat()
            .expect_err("ohne Selektor kein Antrieb");
        assert!(err.contains("konfiguriert"), "{err}");
    }
}
