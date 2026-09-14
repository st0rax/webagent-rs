from pathlib import Path
p = Path("src/browser/composer.rs")
t = p.read_text(encoding="utf-8")
old = "// 1) Mittelpunkt-Koordinaten des Composers holen (nicht gefunden -> false)."
new = (
    "// 1) Klickpunkt = Viewport-Schnitt des Composer-Rects (nicht gefunden -> false).\n"
    "        //    ChatGPT-ProseMirror meldet bei grossen Prompts h=13k/y=-10k; geometrischer\n"
    "        //    Mittelpunkt liegt dann ausserhalb der WebView (Live: Composer-Feld-Timeout)."
)
if old not in t:
    raise SystemExit("Mittelpunkt comment missing")
p.write_text(t.replace(old, new, 1), encoding="utf-8", newline="\n")
print("ok")
