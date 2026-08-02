//! Gemeinsame JS-Bausteine fuer Selektor-Aufloesung, Zustandsauslese und
//! Klick auf den schaltbaren Vorfahren.
//!
//! # Warum hier und nicht im Backend
//!
//! Dieses JS ist ueber Monate gegen echte Oberflaechen gehaertet: es laeuft
//! zum klickbaren Vorfahren hoch (der innerste Treffer bei `text=`-Selektoren
//! ist der Textknoten, dessen Klassen sich beim Umschalten nie aendern) und es
//! liest ALLE `data-*`/`aria-*`-Attribute statt einer festen Liste (zai haelt
//! seinen Zustand in `data-autoThink`).
//!
//! Es hat inzwischen zwei Aufrufer: den Umschaltpfad des Backends
//! ([`crate::browser::ui`], Selektor aus der Konfiguration) und die
//! Verifikation der Oberflaechen-Analyse ([`crate::brain_probe`], Selektor
//! fertig aus einem Vorschlag). Eine zweite handgepflegte Kopie waere genau
//! der doppelte Auslesepfad, an dem in diesem Projekt schon Fixes vorbei-
//! gelaufen sind — deshalb steht die Logik einmal hier, und beide Seiten
//! unterscheiden sich nur in der Herkunft der Selektorliste.

/// JS-Prelude: `Q(sel)` / `QA(sel)` loesen einen Selektor auf und verstehen
/// dabei auch die Playwright-Textformen (`text=foo`, `text=/re/i`,
/// `button:has-text('x')`), die `querySelector` nicht kann. `TX(el)` liest Text
/// mit zurueckgewonnener Mathe-Quelle.
pub const JS_SEL_PRELUDE: &str = r#"
var __p=function(s){var m=/^text=\/(.*)\/([a-z]*)$/.exec(s);if(m)return{base:'*',re:new RegExp(m[1],m[2])};
m=/^text=(.*)$/.exec(s);if(m)return{base:'*',txt:m[1]};
m=/^(.*?):has-text\((['"])([\s\S]*?)\2\)$/.exec(s);if(m)return{base:m[1]||'*',txt:m[3]};return null;};
var QA=function(s){var p=__p(s);if(!p)return document.querySelectorAll(s);
var base=document.querySelectorAll(p.base),c=[];
for(var k=0;k<base.length;k++){var e=base[k],t=(e.innerText||e.textContent||'');
if(p.re?p.re.test(t):t.indexOf(p.txt)!==-1)c.push(e);}
return c.filter(function(e){return !c.some(function(o){return o!==e&&e.contains(o);});});};
var Q=function(s){var r=QA(s);return r.length?r[0]:null;};
var TX=function(el){if(!el)return '';
if(!el.querySelector||!el.querySelector('.katex'))return (el.innerText||el.textContent||'');
var c=el.cloneNode(true);
var ks=c.querySelectorAll('.katex');
for(var i=0;i<ks.length;i++){var a=ks[i].querySelector('annotation[encoding="application/x-tex"]');
var src=a?(a.textContent||''):'';
ks[i].parentNode.replaceChild(document.createTextNode(src),ks[i]);}
return (c.innerText||c.textContent||'');};
"#;

/// JS-Array-Literal aus einer Selektorliste (sicher escaped).
pub fn js_selectors(list: &[String]) -> String {
    let items: Vec<String> = list
        .iter()
        .map(|s| serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into()))
        .collect();
    format!("[{}]", items.join(","))
}

/// Baut ein IIFE, das die Selektorliste `list_js` durchlaeuft und `body` auf
/// jeden Selektor `S[i]` anwendet; liefert `default`, wenn nichts matcht.
///
/// Jeder Selektor laeuft in einem eigenen try/catch: ein kaputter Selektor darf
/// die restliche Liste nicht abbrechen.
pub fn js_scan(list_js: &str, body: &str, default: &str) -> String {
    format!(
        "(function(){{{prelude}var S={list_js};for(var i=0;i<S.length;i++){{try{{{body}}}catch(e){{}}}}return {default};}})()",
        prelude = JS_SEL_PRELUDE
    )
}

/// Rumpf der Zustandsauslese — siehe Modul-Doku, warum `closest(...)` und warum
/// ALLE `data-*`/`aria-*`-Attribute.
const TOGGLE_STATE_BODY: &str = "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;var d=[];for(var a=0;a<t.attributes.length;a++){var at=t.attributes[a];if(at.name.indexOf('data-')===0||at.name.indexOf('aria-')===0)d.push(at.name+'='+at.value);}d.sort();return d.join(';')+'|'+((t.className||'')+'');}";

/// Rumpf des Klicks auf den schaltbaren Vorfahren.
const CLICK_TOGGLE_BODY: &str = "var el=Q(S[i]);if(el){var t=el.closest('button,[role=button],[role=switch],[role=checkbox],[class*=button],[class*=btn]')||el;t.click();return true;}";

/// Ausdruck, der den Zustand des ersten passenden Elements als Zeichenkette
/// liefert (leer, wenn keiner der Selektoren matcht).
pub fn toggle_state_expr_for(selectors: &[String]) -> String {
    js_scan(&js_selectors(selectors), TOGGLE_STATE_BODY, "\"\"")
}

/// Ausdruck, der den schaltbaren Vorfahren des ersten Treffers klickt; `false`,
/// wenn nichts gefunden wurde.
pub fn click_toggle_expr_for(selectors: &[String]) -> String {
    js_scan(&js_selectors(selectors), CLICK_TOGGLE_BODY, "false")
}
