//! Freie Funktionen über [`PageDriver`] + [`Selectors`] — der Arbeitsteil der
//! Diagnose, ohne Backend-`&self`.
//!
//! Schritt 2 des Entkoppelns: `is_logged_in`, `any_visible`, `assistant_count`,
//! `is_cloudflare_blocked`, `get_conversation_ref` waren Methoden am
//! `WebBrainBackend`. Als freie Funktionen sind sie mit dem `MockPageDriver`
//! testbar — die Testlücke am Fundament schließt sich, statt nur eingestanden
//! zu werden.
//!
//! Jede Funktion bekommt das, was sie braucht, explizit: den Driver und die
//! Selektoren. Der Browser-Lebenszyklus (`start`/`stop`) bleibt beim Backend.

use std::time::{Duration, Instant};

use serde_json::Value;

use crate::browser::js;
use crate::browser::selectors::Selectors;
use crate::page_driver::PageDriver;

/// Rumpf der Sichtbarkeitsprüfung: das erste Element, das eine reale Fläche
/// hat, gewinnt.
const VISIBLE_BODY: &str = "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return true;}";

/// Ist mindestens ein Selektor aus der Liste im DOM sichtbar?
pub fn any_visible(driver: &mut dyn PageDriver, sel: &Selectors, key: &str) -> bool {
    let sels = sel.list(key);
    if sels.is_empty() {
        return false;
    }
    let expr = js::js_scan(&js::js_selectors(&sels), VISIBLE_BODY, "false");
    driver
        .evaluate(&expr)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Ist der Brain angemeldet?
///
/// Ein sichtbarer Anmelden-Knopf schlaegt JEDEN positiven Indikator (siehe
/// Geschichte am 2026-07-28: geminis `login_indicator` war der Composer, den
/// auch die ausgeloggte Startseite zeigt). Ohne konfigurierten Indikator
/// Composer/New-Chat als grobe Naeherung.
pub fn is_logged_in(driver: &mut dyn PageDriver, sel: &Selectors) -> bool {
    // Ein sichtbarer Anmelden-Knopf schlaegt JEDEN positiven Indikator.
    // (Geschichte: geminis login_indicator war der Composer, den auch die
    // ausgeloggte Startseite zeigt - deshalb darf der Composer NIE als
    // Login-Beweis zaehlen, und ein sichtbarer login_button widerlegt alles.)
    if any_visible(driver, sel, "login_button") {
        return false;
    }
    let indicator = sel.list("login_indicator");
    if !indicator.is_empty() {
        return any_visible(driver, sel, "login_indicator");
    }
    // Ohne konfigurierten Indikator: Composer/New-Chat als grobe Naeherung.
    any_visible(driver, sel, "composer") || any_visible(driver, sel, "new_chat_button")
}

/// Anzahl der Assistenten-Nachrichten (robust über die Selektorliste).
pub fn assistant_count(driver: &mut dyn PageDriver, sel: &Selectors) -> i32 {
    let list = sel.js("assistant_message", &["div.prose"]);
    let expr = js::js_scan(&list, "var n=QA(S[i]).length;if(n>0)return n;", "0");
    driver
        .evaluate(&expr)
        .ok()
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32
}

/// Cloudflare-Challenge-Erkennung: `__cf_chl` in der URL, oder ein
/// „Just a moment"-Titel (DE+EN). Geteilt zwischen Implementierung und Test —
/// der Mock-Driver matcht auf die EXAKTE Zeichenkette.
const CF_CHALLENGE_EXPR: &str = r#"(function(){var u=location.href||"";if(u.indexOf("__cf_chl")>=0)return true;var t=(document.title||"").toLowerCase();return t.indexOf("just a moment")>=0||t.indexOf("nur einen moment")>=0;})()"#;

/// Ist die Seite von Cloudflare blockiert (`__cf_chl`-URL oder Challenge-Titel)?
pub fn is_cloudflare_blocked(driver: &mut dyn PageDriver) -> bool {
    driver
        .evaluate(CF_CHALLENGE_EXPR)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Aktuelle Konversations-URL als Referenz; `None` für leere/`about:blank`.
pub fn get_conversation_ref(driver: &mut dyn PageDriver) -> Option<String> {
    let url = driver.current_url().ok()?;
    let url = url.trim();
    if url.is_empty() || url == "about:blank" {
        None
    } else {
        Some(url.to_string())
    }
}

/// Führt ein JS im Seitenkontext aus.
pub(crate) fn eval(driver: &mut dyn PageDriver, expr: &str) -> Result<Value, String> {
    driver.evaluate(expr).map_err(|e| e.to_string())
}

/// Wie [`eval`], aber `false` statt Fehler.
pub(crate) fn eval_bool(driver: &mut dyn PageDriver, expr: &str) -> bool {
    eval(driver, expr)
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Wie [`eval`], aber `0` statt Fehler.
pub(crate) fn eval_i64(driver: &mut dyn PageDriver, expr: &str) -> i64 {
    eval(driver, expr).ok().and_then(|v| v.as_i64()).unwrap_or(0)
}

/// Klickt das erste sichtbare Element aus der Selektorliste.
pub(crate) fn click_first(driver: &mut dyn PageDriver, sel: &Selectors, key: &str) -> bool {
    let sels = sel.list(key);
    if sels.is_empty() {
        return false;
    }
    let expr = js::js_scan(
        &js::js_selectors(&sels),
        "var el=Q(S[i]);if(el){el.click();return true;}",
        "false",
    );
    eval_bool(driver, &expr)
}

/// Schliesst Consent- und Werbe-Dialoge; gemini/qwen bekommen ihre Zusatz-Schritte.
pub(crate) fn dismiss_consent(driver: &mut dyn PageDriver, sel: &Selectors, brain_id: &str) -> bool {
    let mut dismissed = click_first(driver, sel, "consent_reject_button");
    // Konfigurierte Dialog-Schliesser (bisher tote Config — nie aufgerufen).
    dismissed |= click_first(driver, sel, "dialog_dismiss_button");
    // Generischer Werbe-/Ankuendigungs-Modal-Schliesser. mistral warf z.B. ein
    // "Mistral Vibe CLI"-Announcement ueber den Composer, das jede Eingabe
    // blockierte — der Grund fuer konsistente mistral-Timeouts. Nur Buttons
    // INNERHALB offener Dialoge/Overlays, damit nichts Legitimes getroffen wird.
    dismissed |= dismiss_modal_buttons(driver);
    if brain_id == "gemini" {
        dismissed |= click_first(driver, sel, "notice_close_button");
    }
    if brain_id == "qwen" {
        dismissed |= dismiss_qwen_blocks(driver);
    }
    dismissed
}

/// Schliesst Werbe-/Ankuendigungs-Modals: klickt einen „Spaeter/Later/Skip/Got
/// it"-artigen Button, aber NUR innerhalb eines offenen Dialogs/Overlays
/// (`[role=dialog]`, `[data-state=open]`, `*modal*`/`*overlay*`), damit auf der
/// normalen Seite nichts faelschlich geklickt wird.
pub(crate) fn dismiss_modal_buttons(driver: &mut dyn PageDriver) -> bool {
    eval_bool(
        driver,
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
pub(crate) fn dismiss_qwen_blocks(driver: &mut dyn PageDriver) -> bool {
    eval_bool(
        driver,
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

/// Zaehlung der beschrifteten Bedienelemente — als Konstante, weil der
/// Mock-Driver auf die EXAKTE Zeichenkette matcht und die Tests sie
/// registrieren muessen.
const LABELED_CONTROLS_EXPR: &str = "(function(){var n=0;document.querySelectorAll('button,[role=button],[aria-label],[data-testid]').forEach(function(e){var t=((e.innerText||e.textContent||'')+'').trim();if(e.getAttribute('aria-label')||e.getAttribute('title')||t)n++;});return n;})()";

/// Wartet, bis die Oberflaeche beschriftete Bedienelemente zeigt.
/// Ein Lade-Skelett bringt sofort Dutzende leerer Platzhalter mit; ein Scan
/// darauf sah real 107 Elemente ohne einen einzigen Namen.
pub(crate) fn wait_for_labeled_controls(driver: &mut dyn PageDriver) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let labeled = eval_i64(driver, LABELED_CONTROLS_EXPR);
        if labeled >= 5 {
            std::thread::sleep(Duration::from_millis(1500));
            return;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// Ein einzelner Scan gegen die offene Seite (Browser muss laufen).
pub(crate) fn scan_once(
    driver: &mut dyn PageDriver,
) -> Result<(Vec<crate::brain_probe::Candidate>, Vec<crate::brain_probe::Proposal>), String> {
    let raw = driver
        .evaluate(crate::brain_probe::PROBE_SCRIPT)
        .map_err(|e| e.to_string())?;
    let candidates: Vec<crate::brain_probe::Candidate> =
        serde_json::from_value(raw).unwrap_or_default();
    let proposals = crate::brain_probe::classify(&candidates);
    Ok((candidates, proposals))
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
pub(crate) fn dom_report(driver: &mut dyn PageDriver, sel: &Selectors) -> Result<Value, String> {
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
        let list = js::js_selectors(&sel.list(k));
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
        prelude = js::JS_SEL_PRELUDE
    );
    eval(driver, &expr)
}

/// Faehrt einen Vorschlag aus [`crate::brain_probe::classify`] live an der
/// offenen Seite: klicken, messbarer Zustandswechsel als Beleg, Rueckweg
/// wiederherstellen. Kein `sel`-Parameter — die Original-Methode hat die
/// Selektoren nie benutzt; der Selektoren-Satz ist der einzelne Vorschlag.
pub(crate) fn verify_surface(
    driver: &mut dyn PageDriver,
    proposal: &crate::brain_probe::Proposal,
) -> Result<crate::brain_probe::Verdict, String> {
    wait_for_labeled_controls(driver);
    crate::brain_probe::verify(driver, proposal).map_err(|e| e.to_string())
}

#[cfg(test)]
mod verify_surface_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn proposal() -> crate::brain_probe::Proposal {
        crate::brain_probe::Proposal {
            capability_key: "chat",
            selector_key: "send_button",
            selector: "button[data-testid='send-button']".to_string(),
            confidence: 90,
            evidence: "aria-label 'Nachricht senden'".to_string(),
        }
    }

    fn verify_exprs() -> (String, String) {
        let selectors = vec![proposal().selector];
        (
            crate::browser::js::toggle_state_expr_for(&selectors),
            crate::browser::js::click_toggle_expr_for(&selectors),
        )
    }

    #[test]
    fn belegt_zustandswechsel_und_wiederherstellung() {
        let (state_expr, click_expr) = verify_exprs();
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(LABELED_CONTROLS_EXPR, json!(5))
                .on_eval_seq(&state_expr, vec![json!("before"), json!("after"), json!("before")])
                .on_eval(&click_expr, json!(true)),
        );
        let verdict = verify_surface(&mut driver, &proposal()).unwrap();
        assert_eq!(
            verdict,
            crate::brain_probe::Verdict {
                capability_key: "chat",
                selector_key: "send_button",
                selector: "button[data-testid='send-button']".to_string(),
                before: "before".to_string(),
                after: "after".to_string(),
                proven: true,
                restored: Some(true),
                note: "Zustandswechsel belegt, Ausgangszustand wiederhergestellt".to_string(),
            }
        );
    }

    #[test]
    fn nicht_anklickbar_ist_ok_ohne_beleg() {
        let (state_expr, click_expr) = verify_exprs();
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(LABELED_CONTROLS_EXPR, json!(5))
                .on_eval(&state_expr, json!("x"))
                .on_eval(&click_expr, json!(false)),
        );
        let verdict = verify_surface(&mut driver, &proposal()).unwrap();
        assert!(!verdict.proven);
        assert_eq!(verdict.restored, None);
    }

    #[test]
    fn fehler_wenn_kein_mock_skript_registriert() {
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(LABELED_CONTROLS_EXPR, json!(5)),
        );
        assert!(verify_surface(&mut driver, &proposal()).is_err());
    }
}

#[cfg(test)]
mod any_visible_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn sel_with(key: &str, selectors: &[&str]) -> Selectors {
        Selectors::from_value(json!({ key: selectors }))
    }

    #[test]
    fn true_wenn_element_sichtbar() {
        let sel = sel_with("key", &["div.x"]);
        let expr = js::js_scan(&js::js_selectors(&["div.x".to_string()]), VISIBLE_BODY, "false");
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(true)));
        assert!(any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_wenn_nicht_sichtbar() {
        let sel = sel_with("key", &["div.x"]);
        let expr = js::js_scan(&js::js_selectors(&["div.x".to_string()]), VISIBLE_BODY, "false");
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(false)));
        assert!(!any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_bei_leerem_schluessel() {
        let sel = sel_with("key", &[]);
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!any_visible(&mut driver, &sel, "key"));
    }

    #[test]
    fn false_wenn_eval_scheitert() {
        let sel = sel_with("key", &["div.x"]);
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!any_visible(&mut driver, &sel, "key"));
    }
}

#[cfg(test)]
mod is_logged_in_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn expr_for(selectors: &[&str]) -> String {
        let sels: Vec<String> = selectors.iter().map(|s| s.to_string()).collect();
        js::js_scan(&js::js_selectors(&sels), VISIBLE_BODY, "false")
    }

    fn sel_with(key: &str, selectors: &[&str]) -> Selectors {
        Selectors::from_value(json!({ key: selectors }))
    }

    #[test]
    fn true_wenn_login_indicator_sichtbar() {
        let sel = sel_with("login_indicator", &["div.avatar"]);
        let indicator_expr = expr_for(&["div.avatar"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&indicator_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn false_wenn_login_button_sichtbar_trotz_indikator() {
        let sel = Selectors::from_value(json!({
            "login_button": ["button.login"],
            "login_indicator": ["div.avatar"],
        }));
        let button_expr = expr_for(&["button.login"]);
        let indicator_expr = expr_for(&["div.avatar"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(&button_expr, json!(true))
                .on_eval(&indicator_expr, json!(true)),
        );
        assert!(!is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_auf_composer_ohne_indikator() {
        let sel = sel_with("composer", &["textarea.prompt"]);
        let composer_expr = expr_for(&["textarea.prompt"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&composer_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_auf_new_chat_button() {
        let sel = sel_with("new_chat_button", &["button.new-chat"]);
        let new_chat_expr = expr_for(&["button.new-chat"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(&new_chat_expr, json!(true)),
        );
        assert!(is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn fallback_false_wenn_nichts_sichtbar() {
        let sel = Selectors::from_value(json!({
            "composer": ["textarea.prompt"],
            "new_chat_button": ["button.new-chat"],
        }));
        let composer_expr = expr_for(&["textarea.prompt"]);
        let new_chat_expr = expr_for(&["button.new-chat"]);
        let mut driver = MockPageDriver::new(
            MockPageState::new()
                .on_eval(&composer_expr, json!(false))
                .on_eval(&new_chat_expr, json!(false)),
        );
        assert!(!is_logged_in(&mut driver, &sel));
    }

    #[test]
    fn false_bei_unbekannten_oder_leeren_selektoren() {
        let sel = Selectors::from_value(json!({
            "composer": [],
            "new_chat_button": [],
        }));
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!is_logged_in(&mut driver, &sel));
    }
}

#[cfg(test)]
mod assistant_count_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    fn count_expr_for(sel: &Selectors) -> String {
        let list = sel.js("assistant_message", &["div.prose"]);
        js::js_scan(&list, "var n=QA(S[i]).length;if(n>0)return n;", "0")
    }

    #[test]
    fn returns_count_when_js_returns_number() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(5)));
        assert_eq!(assistant_count(&mut driver, &sel), 5);
    }

    #[test]
    fn returns_zero_when_js_returns_zero() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(0)));
        assert_eq!(assistant_count(&mut driver, &sel), 0);
    }

    #[test]
    fn returns_zero_when_eval_fails() {
        let sel = Selectors::from_value(json!({ "assistant_message": ["div.prose"] }));
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert_eq!(assistant_count(&mut driver, &sel), 0);
    }

    #[test]
    fn uses_fallback_list_when_key_absent() {
        let sel = Selectors::from_value(json!({}));
        let expr = count_expr_for(&sel);
        let mut driver = MockPageDriver::new(MockPageState::new().on_eval(&expr, json!(3)));
        assert_eq!(assistant_count(&mut driver, &sel), 3);
    }
}

#[cfg(test)]
mod is_cloudflare_blocked_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};
    use serde_json::json;

    #[test]
    fn true_wenn_js_true_liefert() {
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(CF_CHALLENGE_EXPR, json!(true)),
        );
        assert!(is_cloudflare_blocked(&mut driver));
    }

    #[test]
    fn false_wenn_js_false_liefert() {
        let mut driver = MockPageDriver::new(
            MockPageState::new().on_eval(CF_CHALLENGE_EXPR, json!(false)),
        );
        assert!(!is_cloudflare_blocked(&mut driver));
    }

    #[test]
    fn false_wenn_eval_scheitert() {
        let mut driver = MockPageDriver::new(MockPageState::new());
        assert!(!is_cloudflare_blocked(&mut driver));
    }
}

#[cfg(test)]
mod get_conversation_ref_tests {
    use super::*;
    use crate::mock_page::{MockPageDriver, MockPageState};

    #[test]
    fn normale_url_ergibt_some() {
        let mut driver =
            MockPageDriver::new(MockPageState::new().with_url("https://chatgpt.com/c/abc"));
        assert_eq!(
            get_conversation_ref(&mut driver),
            Some("https://chatgpt.com/c/abc".to_string())
        );
    }

    #[test]
    fn about_blank_ergibt_none() {
        let mut driver = MockPageDriver::new(MockPageState::new().with_url("about:blank"));
        assert_eq!(get_conversation_ref(&mut driver), None);
    }

    #[test]
    fn leere_url_ergibt_none() {
        let mut driver = MockPageDriver::new(MockPageState::new().with_url(""));
        assert_eq!(get_conversation_ref(&mut driver), None);
    }

    #[test]
    fn whitespace_um_url_wird_getrimmt() {
        let mut driver =
            MockPageDriver::new(MockPageState::new().with_url("  https://chatgpt.com/c/def\t\n"));
        assert_eq!(
            get_conversation_ref(&mut driver),
            Some("https://chatgpt.com/c/def".to_string())
        );
    }
}
