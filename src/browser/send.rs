//! Senden einer Nachricht in den Composer plus Absende-Beweis
//! (`verify_submitted`) und Block-Banner-Erkennung waehrend des Sendens.
//!
//! Extrahiert aus `mod.rs` am 2026-08-09 (browser-Split). Sibling-Zugriff:
//! `send_generic` und `detect_block_banner` sind `pub(crate)`, weil `backend`
//! (Driver-Dispatch) und `verify` sie aufrufen. Die Banner-Pruefungen kommen
//! per `use super::blocking`.

use std::time::{Duration, Instant};

use crate::brain::BrainBackend;
use crate::browser_inference::{BrowserAttachment, BrowserAttachmentKind};
use serde_json::{json, Value};

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
    /// Haengt optionale Dateien an den Composer und sendet danach die
    /// Nachricht ueber den provider-spezifischen Pfad. Die Dateiuebergabe
    /// erfolgt bewusst ueber ein vorhandenes `input[type=file]`: dadurch wird
    /// kein nativer Dateidialog geoeffnet, der den WebView blockieren wuerde.
    pub(crate) fn send_with_attachments(
        &mut self,
        text: &str,
        attachments: &[BrowserAttachment],
    ) -> Result<i32, String> {
        if !attachments.is_empty() {
            self.prepare_attachment_mode(attachments)?;
            self.attach_files(attachments)?;
        }
        self.send(text)
    }

    /// Bringt Oberflaechen, die Uploads nur in einem eigenen Modus annehmen,
    /// vor der Dateiuebergabe in diesen Zustand. DeepSeek zeigt den
    /// Datei-Chooser zwar auch in anderen Segmenten, aktiviert den Sendeknopf
    /// mit einem Bild aber nur in `Vision`.
    fn prepare_attachment_mode(&mut self, attachments: &[BrowserAttachment]) -> Result<(), String> {
        let wants_image = attachments
            .iter()
            .any(|attachment| attachment.kind == BrowserAttachmentKind::Image);
        if self.brain_id != "deepseek" || !wants_image {
            return Ok(());
        }

        if self.select_segment("mode_option", "Vision").is_ok() {
            return Ok(());
        }

        // Wenn Vision bereits aktiv war, gibt es durch einen erneuten Klick
        // keine Zustandsaenderung und `select_segment` kann den Erfolg nicht
        // belegen. Ein einmaliger Wechsel ueber Instant erzeugt in diesem Fall
        // einen messbaren Zustand; anschließend muss Vision belegbar sein.
        let _ = self.select_segment("mode_option", "Instant");
        self.select_segment("mode_option", "Vision")
            .map(|_| ())
            .map_err(|error| format!("DeepSeek-Vision-Modus nicht aktivierbar: {error}"))
    }

    fn attach_files(&self, attachments: &[BrowserAttachment]) -> Result<(), String> {
        if attachments.is_empty() {
            return Ok(());
        }
        // Der Composer kann providerseitig persistierte Altanhaenge enthalten.
        // Diese gehoeren nicht zu diesem API-Request und duerfen weder mitsamt
        // Fehlerkarten noch als scheinbar gueltige Bilder erneut gesendet werden.
        self.remove_all_attachment_previews();
        // Viele UIs rendern das Datei-Input erst nach einem Klick auf die
        // Büroklammer. Dieser Klick darf nicht über den nativen Dateidialog
        // laufen: ein solcher Dialog würde den gemeinsamen WebView-Eventloop
        // blockieren. Wir lösen deshalb nur den DOM-Handler (untrusted
        // `click()`) aus und warten anschließend auf das dynamisch gerenderte
        // Input. Wenn die Oberfläche überhaupt kein Input rendert, versuchen
        // wir noch den browserüblichen Paste/Drop-Pfad.
        if self.file_input_count() == 0 {
            let _ = self.open_attachment_surface();
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if self.file_input_count() > 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
        }
        let files: Vec<Value> = attachments
            .iter()
            .map(|attachment| {
                json!({
                    "name": attachment.file_name,
                    "mime": attachment.mime_type,
                    "data": base64_encode(&attachment.data),
                })
            })
            .collect();
        let serialized = serde_json::to_string(&files)
            .map_err(|error| format!("Dateianhaenge nicht serialisierbar: {error}"))?;
        if self.file_input_count() == 0 {
            if self.inject_attachments_via_paste_or_drop(&serialized) {
                return Ok(());
            }
            return Err(
                "Browseroberflaeche stellt keinen nutzbaren Datei-Upload bereit \
                 (no_file_input_and_paste_not_confirmed)"
                    .into(),
            );
        }
        // Prefer the native WebView2/CDP file channel whenever the current
        // PageDriver provides it. This is the trusted path used by DevTools
        // and avoids the synthetic-FileList rejection seen in several SPAs.
        let native_files: Vec<(String, Vec<u8>)> = attachments
            .iter()
            .map(|attachment| (attachment.file_name.clone(), attachment.data.clone()))
            .collect();
        let signal_before_native = self.attachment_signal_count();
        let native_result = self.set_file_input_files_native(&native_files);
        if let Err(error) = &native_result {
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!("[upload] native file-input path unavailable: {error}");
            }
        }
        if native_result.is_ok() {
            let _ = self.dispatch_file_input_events();
            let deadline = Instant::now() + Duration::from_secs(4);
            let soft_ready = Instant::now() + Duration::from_millis(1500);
            let mut native_files_observed = false;
            while Instant::now() < deadline {
                let signal_now = self.attachment_signal_count();
                let file_count_now = self.file_input_files_count();
                if file_count_now >= attachments.len() {
                    native_files_observed = true;
                }
                if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                    eprintln!(
                        "[upload] native proof: files={file_count_now}/{} signal={signal_now} (before={signal_before_native})",
                        attachments.len()
                    );
                }
                // A trusted CDP drag/drop can produce a visible provider
                // preview even when WebView2 keeps `input.files` empty.  The
                // new preview is the relevant proof in that case; do not
                // reject an otherwise successful browser-native upload solely
                // because the hidden input is recreated by the SPA.
                if signal_now > signal_before_native {
                    return Ok(());
                }
                // Vue/React uploader copy the File into their own state and
                // immediately replace or clear the hidden input. Seeing the
                // complete FileList followed by zero is therefore stronger
                // evidence than polling only the final DOM state (observed on
                // Kimi's current uploader).
                if native_files_observed && file_count_now == 0 {
                    return Ok(());
                }
                if native_files_observed && Instant::now() >= soft_ready {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            if self.file_input_files_count() >= attachments.len() {
                return Ok(());
            }
            if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_some() {
                eprintln!(
                    "[upload] native path returned but input.files has {} of {} files",
                    self.file_input_files_count(),
                    attachments.len()
                );
            }
        }
        let expression = format!(
            r#"(function(files){{
                try {{
                    var inputs=Array.from(document.querySelectorAll('input[type=file]'));
                    if(!inputs.length) return {{ok:false,error:'no_file_input'}};
                    var input=inputs.find(function(candidate){{return candidate.multiple;}})||inputs[0];
                    if(typeof DataTransfer==='undefined') return {{ok:false,error:'no_data_transfer'}};
                    var transfer=new DataTransfer();
                    for(var i=0;i<files.length;i++){{
                        var binary=atob(files[i].data), bytes=new Uint8Array(binary.length);
                        for(var j=0;j<binary.length;j++) bytes[j]=binary.charCodeAt(j);
                        transfer.items.add(new File([bytes],files[i].name,{{type:files[i].mime}}));
                    }}
                    input.files=transfer.files;
                    input.dispatchEvent(new Event('input',{{bubbles:true}}));
                    input.dispatchEvent(new Event('change',{{bubbles:true}}));
                    return {{ok:true,count:transfer.files.length}};
                }} catch(error) {{ return {{ok:false,error:String(error)}}; }}
            }})({serialized})"#
        );
        let signal_before = self.attachment_signal_count();
        let result = self.eval(&expression)?;
        if result.get("ok").and_then(Value::as_bool) != Some(true) {
            let reason = result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            return Err(format!(
                "Browseroberflaeche stellt keinen nutzbaren Datei-Upload bereit ({reason})"
            ));
        }
        let count = result.get("count").and_then(Value::as_u64).unwrap_or(0);
        if count != attachments.len() as u64 {
            // Some WebView/SPA combinations expose DataTransfer but silently
            // reject assigning its FileList to the hidden input. Give the
            // page's native paste/drop handler one chance before failing the
            // request; only a new visible preview is accepted as proof.
            if self.attachment_signal_count() > signal_before {
                return Ok(());
            }
            if self.inject_attachments_via_paste_or_drop(&serialized) {
                return Ok(());
            }
            return Err(format!(
                "Browseroberflaeche hat nur {count} von {} Dateien uebernommen",
                attachments.len()
            ));
        }
        Ok(())
    }

    fn remove_all_attachment_previews(&self) {
        // Die Kachelleiste ist ein Carousel: mehrere Delete-Container koennen
        // gleichzeitig sichtbar sein, waehrend ein einfacher Selektor-Klick
        // immer nur den ersten Treffer erreicht. Koordinaten deshalb in einem
        // Roundtrip sammeln und jeden sichtbaren Treffer trusted anklicken.
        for _ in 0..4 {
            let coords = self
                .eval(r#"(function(){var a=[];document.querySelectorAll('[class*="image-thumbnail" i],[class*="attachment" i],[class*="file-preview" i],[data-attachment]').forEach(function(c){var b=c.querySelector('[class*="delete" i],[class*="remove" i],[aria-label*="remove" i],[aria-label*="löschen" i]');if(!b)b=c;var r=b.getBoundingClientRect();if(r.width>0&&r.height>0)a.push({x:r.left+r.width/2,y:r.top+r.height/2});});return a;})()"#)
                .unwrap_or(Value::Null);
            let mut clicked = false;
            if let Some(items) = coords.as_array() {
                let mut guard = self.driver.borrow_mut();
                if let Some(driver) = guard.as_mut() {
                    for item in items {
                        if let (Some(x), Some(y)) = (
                            item.get("x").and_then(Value::as_f64),
                            item.get("y").and_then(Value::as_f64),
                        ) {
                            clicked |= driver.click_at_trusted(x, y).is_ok();
                        }
                    }
                }
            }
            if !clicked {
                break;
            }
            std::thread::sleep(Duration::from_millis(180));
        }
        for _ in 0..32 {
            if !self.click_visible_real("attachment_delete")
                && !self.click_first("attachment_delete")
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.eval(
            r#"(function(){var n=0;document.querySelectorAll('[class*="image-thumbnail" i],[class*="attachment" i],[class*="file-preview" i],[data-attachment]').forEach(function(card){var b=card.querySelector('[class*="delete" i],[class*="remove" i],[aria-label*="remove" i],[aria-label*="löschen" i]');try{if(b){b.click();n++;}}catch(e){}});return n;})()"#,
        );
    }

    fn set_file_input_files_native(&self, files: &[(String, Vec<u8>)]) -> Result<(), String> {
        let mut guard = self.driver.borrow_mut();
        let driver = guard
            .as_mut()
            .ok_or_else(|| "Backend nicht gestartet".to_string())?;
        driver
            .set_file_input_files(files)
            .map_err(|error| error.to_string())
    }

    fn dispatch_file_input_events(&self) -> bool {
        self.eval(
            r#"(function(){
                var input=document.querySelector('input[type=file]');
                if(!input)return false;
                input.dispatchEvent(new Event('input',{bubbles:true}));
                input.dispatchEvent(new Event('change',{bubbles:true}));
                return true;
            })()"#,
        )
        .ok()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    }

    fn file_input_files_count(&self) -> usize {
        self.eval(
            "(function(){var n=0;document.querySelectorAll('input[type=file]').forEach(function(i){n+=i.files?i.files.length:0;});return n;})()",
        )
        .ok()
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize
    }

    /// Liefert die Anzahl der Datei-Inputs im aktuellen DOM. Das ist absichtlich
    /// kein Selektor-Proof: ein Input kann unsichtbar sein, aber trotzdem der
    /// korrekte React-Upload-Kanal der Seite.
    fn file_input_count(&self) -> usize {
        self.eval("document.querySelectorAll('input[type=file]').length")
            .ok()
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize
    }

    /// Öffnet die Upload-Oberfläche über konfigurierte, provider-spezifische
    /// Attach-Selektoren. Der Aufruf bleibt bewusst untrusted, damit kein
    /// nativer Dateidialog geöffnet wird; anschließend übernimmt der
    /// `DataTransfer`- bzw. Paste-Pfad die eigentliche Dateiübergabe.
    fn open_attachment_surface(&self) -> bool {
        let selectors = self.sel("attach_button");
        if selectors.is_empty() {
            return false;
        }
        let serialized = match serde_json::to_string(&selectors) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let composer_selectors = self.sel_js(
            "composer",
            &[
                "div[contenteditable='true']",
                "textarea",
                "[role='textbox']",
            ],
        );
        let expression = format!(
            r#"(function(selectors,composerSelectors){{
                function visible(el){{
                    if(!el)return false;
                    var r=el.getBoundingClientRect(),s=window.getComputedStyle(el);
                    return r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden';
                }}
                // A global selector can hit an identically named icon in the
                // sidebar/header. Prefer controls in the same visual band as
                // the composer; this is essential for icon-only providers.
                var composerRects=[];
                for(var c=0;c<composerSelectors.length;c++){{
                    try{{var cs=document.querySelectorAll(composerSelectors[c]);
                        for(var ci=0;ci<cs.length;ci++)if(visible(cs[ci]))composerRects.push(cs[ci].getBoundingClientRect());
                    }}catch(e){{}}
                }}
                function nearComposer(el){{
                    if(!composerRects.length)return true;
                    var r=el.getBoundingClientRect();
                    for(var i=0;i<composerRects.length;i++){{
                        var c=composerRects[i];
                        if(r.y>=c.y-120&&r.y<=c.bottom+120&&r.x>=c.x-180&&r.x<=c.right+180)return true;
                    }}
                    return false;
                }}
                for(var i=0;i<selectors.length;i++){{
                    try{{
                        var found=document.querySelectorAll(selectors[i]);
                        for(var j=0;j<found.length;j++){{
                            var el=found[j];
                            if(!visible(el)||!nearComposer(el))continue;
                            var target=el.closest('button,[role=button],label')||el;
                            if(!visible(target)||!nearComposer(target))continue;
                            target.click();
                            return {{ok:true,selector:selectors[i]}};
                        }}
                    }}catch(e){{}}
                }}
                return {{ok:false,error:'no_attach_control'}};
            }})({serialized},{composer_selectors})"#
        );
        self.eval(&expression)
            .ok()
            .and_then(|value| value.get("ok").and_then(Value::as_bool))
            .unwrap_or(false)
    }

    /// Fallback für Provider, die Uploads über `paste`/`drop` am Composer
    /// annehmen, aber kein dauerhaftes `input[type=file]` im DOM halten. Das
    /// Ergebnis wird nicht blind als Erfolg gewertet: erst ein sichtbares
    /// Attachment-Signal nach dem Dispatch darf den Turn fortsetzen.
    fn inject_attachments_via_paste_or_drop(&self, serialized: &str) -> bool {
        let composer = self.sel_js(
            "composer",
            &[
                "div[contenteditable='true']",
                "textarea",
                "[role='textbox']",
            ],
        );
        let before = self.attachment_signal_count();
        let expression = format!(
            r#"(function(files,composerSelectors){{
                try{{
                    if(typeof DataTransfer==='undefined')return {{ok:false,error:'no_data_transfer'}};
                    var transfer=new DataTransfer();
                    for(var i=0;i<files.length;i++){{
                        var raw=atob(files[i].data),bytes=new Uint8Array(raw.length);
                        for(var j=0;j<raw.length;j++)bytes[j]=raw.charCodeAt(j);
                        transfer.items.add(new File([bytes],files[i].name,{{type:files[i].mime}}));
                    }}
                    var target=null;
                    for(var s=0;s<composerSelectors.length&&!target;s++){{
                        try{{
                            var els=document.querySelectorAll(composerSelectors[s]);
                            for(var k=0;k<els.length;k++){{
                                var r=els[k].getBoundingClientRect();
                                if(r.width>0&&r.height>0){{target=els[k];break;}}
                            }}
                        }}catch(e){{}}
                    }}
                    if(!target)return {{ok:false,error:'no_composer'}};
                    try{{target.focus();}}catch(e){{}}
                    var pasted=false;
                    try{{pasted=target.dispatchEvent(new ClipboardEvent('paste',{{bubbles:true,cancelable:true,clipboardData:transfer}}))||pasted;}}catch(e){{
                        try{{var pe=new Event('paste',{{bubbles:true,cancelable:true}});Object.defineProperty(pe,'clipboardData',{{value:transfer}});pasted=target.dispatchEvent(pe)||pasted;}}catch(e2){{}}
                    }}
                    try{{pasted=target.dispatchEvent(new DragEvent('drop',{{bubbles:true,cancelable:true,dataTransfer:transfer}}))||pasted;}}catch(e){{
                        try{{var de=new Event('drop',{{bubbles:true,cancelable:true}});Object.defineProperty(de,'dataTransfer',{{value:transfer}});pasted=target.dispatchEvent(de)||pasted;}}catch(e2){{}}
                    }}
                    return {{ok:true,dispatched:pasted,count:transfer.files.length}};
                }}catch(error){{return {{ok:false,error:String(error)}};}}
            }})({serialized},{composer})"#
        );
        let dispatched = self
            .eval(&expression)
            .ok()
            .and_then(|value| value.get("ok").and_then(Value::as_bool))
            .unwrap_or(false);
        if !dispatched {
            return false;
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            // Einige UIs bündeln mehrere Dateien in einem einzigen Preview-
            // Container. Ein neues sichtbares Signal belegt deshalb den
            // Paste/Drop-Kanal; die exakte Dateianzahl kann nur der native
            // `input.files`-Pfad sicher bestätigen.
            if self.attachment_signal_count() > before {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }

    /// Konservatives, providerneutrales Signal für ein angehängtes Vorschau-
    /// Element. Bestehende History wird über den Vorher-Nachher-Vergleich
    /// herausgerechnet; reine Datei-Inputs zählen nicht als Vorschau.
    fn attachment_signal_count(&self) -> usize {
        let expression = r#"(function(){
            var sels=[
                '[data-attachment]','[data-testid*="attachment" i]',
                '[data-testid*="file" i]','[class*="attachment" i]',
                '[class*="file-preview" i]','[class*="image-thumbnail" i]','img[src^="blob:"]',
                '[aria-label*="attachment" i]','[aria-label*="angehängt" i]'
            ],n=0;
            for(var i=0;i<sels.length;i++)try{
                var els=document.querySelectorAll(sels[i]);
                for(var j=0;j<els.length;j++){
                    var e=els[j],r=e.getBoundingClientRect(),s=window.getComputedStyle(e);
                    if(r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden')n++;
                }
            }catch(e){}
            return n;
        })()"#;
        self.eval(expression)
            .ok()
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize
    }

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
        let filled = if self.brain_id == "kimi" {
            self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.dismiss_consent();
                s.fill_composer_rich_multiline(js, t) && s.composer_matches_text(js, t)
            })
        } else {
            self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.dismiss_consent();
                s.fill_composer(js, t);
                s.composer_contains(js, t)
            })
        };
        if !filled {
            self.capture_submit_failure_trace();
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
        self.capture_submit_failure_trace();
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

    /// Schreibt nur im expliziten Verifikationsmodus einen Screenshot und eine
    /// kompakte DOM-Diagnose. Das hält fehlgeschlagene Upload-Smokes
    /// untersuchbar, ohne im normalen Betrieb Chat-Inhalte mitzuschneiden.
    fn capture_submit_failure_trace(&self) {
        if std::env::var_os("WEBAGENT_VERIFY_TRACE").is_none() {
            return;
        }
        let send_selectors = Self::js_selectors(&self.sel("send_button"));
        let composer_selectors = Self::js_selectors(&self.sel("composer"));
        let expression = format!(
            r#"(function(){{
                function scan(selectors){{
                    var out=[];
                    for(var i=0;i<selectors.length;i++)try{{
                        var els=QA(selectors[i]);
                        for(var j=0;j<els.length;j++){{
                            var e=els[j],r=e.getBoundingClientRect(),s=getComputedStyle(e);
                            var value=('value' in e)?(e.value||''):(e.innerText||e.textContent||'');
                            out.push({{selector:selectors[i],tag:e.tagName,cls:((e.className||'')+'').slice(0,180),
                                aria:e.getAttribute('aria-label'),disabled:e.disabled===true||e.getAttribute('aria-disabled')==='true',
                                visible:r.width>0&&r.height>0&&s.display!=='none'&&s.visibility!=='hidden',x:r.x,y:r.y,w:r.width,h:r.height,
                                textLength:value.length,text:value.slice(0,500)}});
                        }}
                    }}catch(error){{}}
                    return out;
                }}
                {prelude}
                return {{send:scan({send}),composer:scan({composer})}};
            }})()"#,
            prelude = Self::JS_SEL_PRELUDE,
            send = send_selectors,
            composer = composer_selectors,
        );
        if let Ok(details) = self.eval(&expression) {
            eprintln!("[send] submit failure DOM: {details}");
        }
        let path =
            std::env::temp_dir().join(format!("webagent-{}-submit-failure.png", self.brain_id));
        if let Some(driver) = self.driver.borrow_mut().as_mut() {
            match driver.capture_png() {
                Ok(png) => match std::fs::write(&path, png) {
                    Ok(()) => eprintln!("[send] submit failure screenshot: {}", path.display()),
                    Err(error) => eprintln!("[send] submit failure screenshot write: {error}"),
                },
                Err(error) => eprintln!("[send] submit failure screenshot capture: {error}"),
            }
        }
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
             var cls=((b.className||'')+'').toLowerCase();\
             return (b.disabled===true)||b.getAttribute('aria-disabled')==='true'\
             ||st.pointerEvents==='none'||cls.indexOf('disabled')!==-1;}",
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

/// Kleine, dependency-freie Base64-Kodierung fuer den Transfer in den
/// JavaScript-Seitenkontext. Die API-Schicht validiert die Eingabegroesse vorab.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(first >> 2) as usize] as char);
        out.push(ALPHABET[((first & 0x03) << 4 | second >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((second & 0x0f) << 2 | third >> 6) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(third & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
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
