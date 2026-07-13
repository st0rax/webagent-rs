//! Gezielte Gemini-Submit/Antwort-Diagnose.
//!
//!   set WEBAGENT_PROFILE_DIR=C:\Users\storax\Desktop\webagent\data\profiles\shared
//!   cargo run --example gemini_dom
//!
//! Sendet einen Prompt und dumpt danach den GENAUEN Zustand: ist der Sende-Button
//! disabled, wurde eine `user-query` gesendet, und wo (innerText vs. textContent)
//! steckt der Antworttext. Zeigt, ob das Problem Submit oder Extraktion ist.

use std::time::Duration;

use webagent::brain::BrainBackend;
use webagent::browser::WebBrainBackend;

const PROBE: &str = r#"(function(){
  function info(sel){
    var els=document.querySelectorAll(sel);
    var out=[];
    for(var i=0;i<els.length && i<4;i++){
      var e=els[i];
      var it=(e.innerText||'').trim();
      var tc=(e.textContent||'').trim();
      out.push({cls:(e.className||'').toString().slice(0,60),it:it.length,tc:tc.length,sample:tc.slice(0,60)});
    }
    return {n:els.length, els:out};
  }
  function btns(sel){
    var els=document.querySelectorAll(sel);
    var out=[];
    for(var i=0;i<els.length && i<4;i++){
      var e=els[i];
      out.push({al:e.getAttribute('aria-label')||'',disabled:!!e.disabled,ariaDisabled:e.getAttribute('aria-disabled')||''});
    }
    return {n:els.length, els:out};
  }
  return {
    composer_p: info("rich-textarea p"),
    user_query: info("user-query"),
    query_text: info(".query-text, [class*='query-text']"),
    message_content: info("message-content[class*='model-response-text']"),
    message_content_any: info("message-content"),
    markdown: info(".markdown, [class*='markdown']"),
    model_response: info("model-response, [class*='model-response']"),
    response_container: info("[class*='response-container']"),
    send_btn: btns("button[aria-label*='senden' i], button[aria-label*='Send' i]"),
    stop_btn: btns("button[aria-label*='Stop' i], button[aria-label*='stopp' i]")
  };
})()"#;

fn main() {
    let mut b = WebBrainBackend::from_config("gemini").expect("from_config");
    b.start(false).expect("start");
    let st = b.ensure_ready(60.0).unwrap_or(webagent::brain::SessionState::Error);
    eprintln!("session_state={st:?}");

    let sent = b
        .send("Antworte NUR mit dem Wort: HALLOWELT")
        .is_ok();
    eprintln!("send returned ok={sent}");

    for i in 0..6 {
        std::thread::sleep(Duration::from_millis(2000));
        match b.eval_js(PROBE) {
            Ok(v) => println!(
                "\n=== t=+{}s ===\n{}",
                (i + 1) * 2,
                serde_json::to_string_pretty(&v).unwrap_or_default()
            ),
            Err(e) => println!("\n=== t=+{}s === FEHLER: {e}", (i + 1) * 2),
        }
    }
    b.stop().ok();
    println!("\n=== fertig ===");
}
