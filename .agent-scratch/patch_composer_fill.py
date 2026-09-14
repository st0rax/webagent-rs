from pathlib import Path

CLAMP = (
    "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();"
    "if(r.width>0&&r.height>0){"
    "var top=Math.max(r.top,0),bot=Math.min(r.bottom,window.innerHeight||r.bottom),"
    "left=Math.max(r.left,0),right=Math.min(r.right,window.innerWidth||r.right);"
    "if(bot-top<1||right-left<1){"
    "top=Math.min(Math.max((r.top+r.bottom)/2,2),(window.innerHeight||600)-2);"
    "left=Math.min(Math.max((r.left+r.right)/2,2),(window.innerWidth||800)-2);"
    "return {x:left,y:top};}"
    "return {x:(left+right)/2,y:(top+bot)/2};}}"
)

olds = [
    "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0)return {x:r.left+r.width/2,y:r.top+r.height/2};}",
    "var el=Q(S[i]);if(el){var r=el.getBoundingClientRect();if(r.width>0&&r.height>0){return {x:r.left+r.width/2,y:r.top+r.height/2};}}",
]

root = Path(r"C:\Users\storax\projects\GitHub\webagent-rs")
path = root / "src" / "browser" / "composer.rs"
text = path.read_text(encoding="utf-8")
count = 0
for old in olds:
    n = text.count(old)
    if n:
        text = text.replace(old, CLAMP)
        count += n
        print(f"replaced coord variant x{n}")
if count != 3:
    raise SystemExit(f"expected 3 coord_body replacements, got {count}")
path.write_text(text, encoding="utf-8", newline="\n")
print("composer.rs OK")

send = root / "src" / "browser" / "send.rs"
s = send.read_text(encoding="utf-8")

old_wait = """    fn wait_fill_composer<F>(&self, composer_js: &str, text: &str, fill: F) -> bool
    where
        F: Fn(&Self, &str, &str) -> bool,
    {
        // Ein fehlender Composer ist ein lokaler UI-/Controller-Fehler. Zwölf
        // Sekunden pro Repair-Runde machten daraus die beobachteten Minuten-
        // langen Leerlaufphasen. Der normale Provider-Response-Timeout greift
        // erst nach erfolgreichem Senden; hier reichen 4 Sekunden.
        let deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if fill(self, composer_js, text) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        false
    }"""

new_wait = """    fn wait_fill_composer<F>(&self, composer_js: &str, text: &str, fill: F) -> bool
    where
        F: Fn(&Self, &str, &str) -> bool,
    {
        // Ein fehlender Composer ist ein lokaler UI-/Controller-Fehler. Zwölf
        // Sekunden pro Repair-Runde machten daraus die beobachteten Minuten-
        // langen Leerlaufphasen. Basis bleibt 4s; grosse Pi-/Tool-Prompts
        // (20k+ Zeichen) brauchen laenger, weil CDP Input.insertText in
        // ProseMirror sonst mitten im Fuellen den 4s-Deadline reisst und der
        // Retry dann in die Mitte eines 13k-px-hohen Editors klickt.
        let boost = (text.len() as u64 / 4000).min(26);
        let deadline = Instant::now() + Duration::from_secs(4 + boost);
        while Instant::now() < deadline {
            self.wake_renderer();
            if fill(self, composer_js, text) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(400));
        }
        false
    }"""

if old_wait not in s:
    raise SystemExit("wait_fill_composer block not found")
s = s.replace(old_wait, new_wait, 1)

old_generic = """        let filled = if self.brain_id == \"kimi\" {
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
            return Err(\"Composer-Feld nicht gefunden (Timeout)\".into());
        }"""

new_generic = """        let filled = if self.brain_id == \"kimi\" {
            self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.dismiss_consent();
                s.fill_composer_rich_multiline(js, t) && s.composer_matches_text(js, t)
            })
        } else {
            self.wait_fill_composer(&composer_js, text, |s, js, t| {
                s.dismiss_consent();
                s.fill_composer(js, t);
                s.composer_contains(js, t)
            }) || self.wait_fill_composer(&composer_js, text, |s, js, t| {
                // Fallback: DOM-set after viewport-clamped focus — hilft wenn
                // der erste trusted-Insert bei riesigem Prompt (Brain-Session/
                // Pi-Tools) den Editor aufblaeht und der Center-Klick daneben lag.
                s.dismiss_consent();
                s.fill_composer_dom_set(js, t) && s.composer_contains(js, t)
            })
        };
        if !filled {
            self.capture_submit_failure_trace();
            return Err(\"Composer-Feld nicht gefunden (Timeout)\".into());
        }"""

if old_generic not in s:
    raise SystemExit("send_generic fill block not found")
s = s.replace(old_generic, new_generic, 1)

send.write_text(s, encoding="utf-8", newline="\n")
print("send.rs OK")
