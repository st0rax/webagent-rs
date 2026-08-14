//! Senden einer Nachricht in den Composer plus Absende-Beweis
//! (`verify_submitted`) und Block-Banner-Erkennung waehrend des Sendens.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Sibling-Zugriff:
//! `send_generic` und `detect_block_banner` sind `pub(crate)`, weil `backend`
//! (Driver-Dispatch) und `verify` sie aufrufen. Die Banner-Pruefungen kommen
//! per `use super::blocking`.

use std::time::{Duration, Instant};

use crate::brain::BrainBackend;

use super::blocking::{banner_is_prompt_echo, block_banner_expr, is_technical_block_phrase_list};
use super::WebBrainBackend;

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
    const BASE: u32 = 32;
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

fn submission_is_proven(
    chatgpt_user_baseline_known: bool,
    user_echo: bool,
    composer_consumed: bool,
    stop_visible: bool,
    assistant_grew: bool,
    url_changed: bool,
) -> bool {
    if chatgpt_user_baseline_known {
        stop_visible || (composer_consumed && (user_echo || url_changed))
    } else {
        stop_visible || assistant_grew || (url_changed && composer_consumed)
    }
}

impl WebBrainBackend {
    pub(crate) fn send_generic(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        let user_baseline = self.user_message_count();
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
            // Absendung ist im Gange, wenn der Composer die Eingabe schon
            // konsumiert hat (leer): dann NUR den Beweis abwarten, nicht neu
            // fuellen/senden — sonst ensteht ein Doppel-Send bei Brains, deren
            // Send-Registrierung (perplexity/deepseek ~20s) laenger dauert als
            // das Beweisfenster des ersten Versuchs.
            let consumed = !self.composer_contains(&composer_js, text);
            if !consumed {
                if attempt == 0 || !has_send_button {
                    self.press_enter().ok();
                } else if !self.click_visible_real("send_button") {
                    self.click_first("send_button");
                }
            }
            if self.verify_submitted(baseline, user_baseline, url_before.as_deref()) {
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
        self.eval(&js).ok().and_then(|v| v.as_u64()).unwrap_or(0) as usize
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

    pub(crate) fn send_gemini(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        self.handle_interruptions();
        let composer_js = self.sel_js("composer", &[]);
        // ProseMirror (geminis Editor) registriert ein reines DOM-Set (textContent
        // + InputEvent) NICHT — der Absendeknopf bleibt dann deaktiviert und der
        // ehrliche Fehler "kein Absende-Beweis" war die Folge. Darum zuerst echt
        // tippen (`fill_composer`: Klick + trusted `Input.insertText`), DOM-Set nur
        // als Fallback.
        if !self.wait_fill_composer(&composer_js, text, |s, js, t| s.fill_composer(js, t)) {
            let _ = self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.fill_composer_dom_set(js, t) && s.type_text_char_by_char(t).is_ok()
            });
        }
        std::thread::sleep(Duration::from_millis(200));
        // Gibt die Oberflaeche den Text nicht an ProseMirror weiter, bleibt der
        // Absendeknopf grau — dann Zeichen fuer Zeichen nachtippen statt blind
        // zu klicken (React/ProseMirror registriert echte Tastatur-Events).
        if self.send_button_disabled() == Some(true) {
            let _ = self.fill_composer_dom_set(&composer_js, text);
            let _ = self.type_text_char_by_char(text);
        }
        let url_before = self.get_conversation_ref();
        for attempt in 0..3 {
            // Abwechselnd echten Klick und Enter: Geminis "Nachricht senden"-Button
            // ignoriert gelegentlich den trusted Klick (Anti-Automation), Enter
            // sendet zuverlaessig, wenn der Text im Composer steht. Nach dem
            // ersten Senden einer Konversation wechselt die UI teils den Knopf.
            if attempt % 2 == 0 {
                if self.click_visible_real("send_button") || self.click_first("send_button") {
                    std::thread::sleep(Duration::from_millis(400));
                }
            } else {
                let _ = self.press_enter();
            }
            if self.verify_submitted(baseline, None, url_before.as_deref()) {
                return Ok(baseline);
            }
            let _ = self.fill_composer_dom_set(&composer_js, text);
        }
        // Kein Ok(baseline) bei ausbleibendem Absende-Beweis (Vergiftungsquelle) —
        // ehrlicher Fehler wie in send_generic.
        Err(self.submit_failed_error(3))
    }

    pub(crate) fn send_qwen(&mut self, text: &str) -> Result<i32, String> {
        let baseline = self.prepare_send_baseline();
        self.dismiss_consent();
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
            if self.verify_submitted(baseline, None, url_before.as_deref()) {
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
    pub(crate) fn detect_block_banner(&self) -> Option<String> {
        let sent = self.last_sent.borrow().clone();
        self.detect_block_banner_excluding(&sent)
    }

    /// Wie `detect_block_banner`, ignoriert aber Treffer, die nur das Echo der
    /// gerade gesendeten Nachricht sind (siehe `banner_is_prompt_echo`).
    fn detect_block_banner_excluding(&self, sent: &str) -> Option<String> {
        let v = self.eval(&block_banner_expr()).ok()?;
        let banner = v
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        if banner_is_prompt_echo(&banner, sent) {
            // Unsere eigene Frage, kein Banner des Anbieters.
            return None;
        }
        if is_technical_block_phrase_list(&banner) {
            return None;
        }
        Some(banner)
    }

    fn verify_submitted(
        &self,
        baseline: i32,
        user_baseline: Option<i32>,
        url_before: Option<&str>,
    ) -> bool {
        let rounds = submit_verify_rounds(self.last_sent.borrow().chars().count());
        let composer_js = self.sel_js("composer", &[]);
        let sent = self.last_sent.borrow().clone();
        for _ in 0..rounds {
            std::thread::sleep(Duration::from_millis(250));
            let url_changed = match (url_before, self.get_conversation_ref()) {
                (Some(before), Some(now)) => now != before,
                _ => false,
            };
            // URL-Wechsel allein ist KEIN Beweis: chatgpt erzeugt /c/<uuid> schon
            // beim Fokussieren des Composers, bevor die Nachricht wirklich raus ist
            // — die UI "startet eine neue Session statt zu senden". Der Beweis steht
            // erst, wenn die Oberflaeche die Eingabe konsumiert hat (Composer leer)
            // ODER eine neue Antwort/Stop erschienen ist.
            let composer_consumed = !self.composer_contains(&composer_js, &sent);
            let user_echo = user_baseline
                .zip(self.user_message_count())
                .is_some_and(|(before, now)| now > before);
            let stop_visible = self.any_visible("stop_button");
            let assistant_grew = self.assistant_count() > baseline;
            // ChatGPTs Assistant-Zaehler schwankt bei DOM-Rehydration und
            // kann nach F5 wachsen, obwohl die neue User-Nachricht nie im
            // Chat angekommen ist. Fuer bestehende Chats gilt deshalb das
            // User-Echo als Beleg; ein sichtbarer Stop-Knopf bleibt ein
            // autoritatives Generierungssignal. URL-Wechsel gilt nur beim
            // Erzeugen eines neuen Chats und weiterhin nur konsumiert.
            let proven = submission_is_proven(
                user_baseline.is_some(),
                user_echo,
                composer_consumed,
                stop_visible,
                assistant_grew,
                url_changed,
            );
            if proven {
                return true;
            }
        }
        false
    }

    /// Optionaler User-Turn-Zaehler fuer den Absende-Beweis.
    /// Nur Provider mit einem `user_message`-Selektor in ihrer Profil-Datei
    /// liefern hier ein Signal; alle anderen behalten ihre vorhandenen Signale.
    fn user_message_count(&self) -> Option<i32> {
        if self.sel("user_message").is_empty() {
            return None;
        }
        let user_js = Self::js_selectors(&self.sel("user_message"));
        self.eval(&format!("document.querySelectorAll({user_js}).length"))
            .ok()
            .and_then(|value| value.as_i64())
            .and_then(|count| i32::try_from(count).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::submission_is_proven;

    #[test]
    fn chatgpt_dom_rehydration_is_not_a_send_proof_without_user_echo() {
        assert!(!submission_is_proven(true, false, true, false, true, false));
        assert!(submission_is_proven(true, true, true, false, false, false));
        assert!(submission_is_proven(true, false, false, true, false, false));
    }
}
