import json
from pathlib import Path
p = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-301\events_after.json")
j = json.loads(p.read_text(encoding="utf-8-sig"))
ev = j["events"] if isinstance(j, dict) and "events" in j else j
texts = []
lines = []
for e in ev:
    seq = e.get("seq")
    event = e.get("event")
    if isinstance(event, str):
        lines.append(f"{seq} {event}")
    elif isinstance(event, dict) and "TextDelta" in event:
        t = event["TextDelta"].get("text", "")
        texts.append(t)
        lines.append(f"{seq} TextDelta {t!r}")
    elif isinstance(event, dict):
        lines.append(f"{seq} {list(event.keys())[0]}")
dup = sum(1 for a, b in zip(texts, texts[1:]) if a == b)
summary = f"events={len(ev)} deltas={len(texts)} identical_consecutive={dup} unique={len(set(texts))}"
out = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-301\analysis.txt")
out.write_text(summary + "\n" + "\n".join(lines), encoding="utf-8")
print(summary)
