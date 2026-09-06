import json, os, time, urllib.request, urllib.error
from pathlib import Path

BASE = "http://127.0.0.1:8788"
KEY = os.environ.get("WEBAGENT_API_KEY", "t501-local")
OUT = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
DATE = "2026-09-06"
BRAINS = [b.strip() for b in os.environ.get("STREAM_BRAINS", "kimi,mistral,zai").split(",") if b.strip()]

def req(path, payload, timeout=240):
    data = json.dumps(payload).encode("utf-8")
    r = urllib.request.Request(
        BASE + path, data=data,
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + KEY},
        method="POST",
    )
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8", "replace")
            code = resp.status
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", "replace")
        code = e.code
    except Exception as e:
        return {"ok": False, "http": None, "latency_ms": int((time.perf_counter() - t0) * 1000), "error": str(e), "raw": ""}
    return {"ok": 200 <= code < 300, "http": code, "latency_ms": int((time.perf_counter() - t0) * 1000), "raw": raw, "error": None}

def has_delta(raw):
    texts = []
    err_notes = []
    for line in (raw or "").splitlines():
        if not line.startswith("data:"):
            continue
        chunk = line[5:].strip()
        if chunk == "[DONE]":
            continue
        try:
            obj = json.loads(chunk)
        except Exception:
            continue
        for ch in obj.get("choices") or []:
            delta = (ch.get("delta") or {}).get("content")
            if delta:
                texts.append(delta)
            if ch.get("finish_reason") == "error":
                msg = ((obj.get("error") or {}).get("message") or "error")
                err_notes.append(str(msg)[:300])
    return texts, err_notes

results = {}
for brain in BRAINS:
    print("== [%s] streaming %s (headless/offscreen) ==" % (DATE, brain), flush=True)
    model = "webagent/%s" % brain
    chat = req("/v1/chat/completions", {
        "model": model,
        "stream": True,
        "messages": [{"role": "user", "content": "Reply with exactly the token STREAM_OK and nothing else."}],
    })
    deltas, err_notes = has_delta(chat.get("raw") or "")
    joined = "".join(deltas).strip()
    # clean STREAM_OK only — no reasoning-echo false positives
    clean = joined == "STREAM_OK" or joined.startswith("STREAM_OK")
    rec = {
        "model": model,
        "ok": bool(chat.get("ok") and deltas and "STREAM_OK" in joined and clean),
        "http": chat.get("http"),
        "latency_ms": chat.get("latency_ms"),
        "delta_count": len(deltas),
        "joined": joined[:400],
        "error": chat.get("error"),
        "sse_errors": err_notes[:3],
        "note": None,
        "wake_binary": "467e65f",
        "api_port": 8788,
        "headless": True,
        "offscreen": True,
        "false_positive_guard": "require joined==STREAM_OK or startswith(STREAM_OK)",
    }
    if not rec["ok"]:
        observed = joined[:160] or (err_notes[0] if err_notes else None) or chat.get("error") or ("http=%s" % chat.get("http"))
        rec["note"] = "failed streaming proof - no clean STREAM_OK (provider quirk / transport / login). observed=%r" % (observed,)
        rec["raw_head"] = (chat.get("raw") or "")[:700]
    path = OUT / ("streaming_%s_%s.json" % (brain, DATE))
    path.write_text(json.dumps(rec, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    slim = {k: rec[k] for k in rec if k != "raw_head"}
    print(json.dumps(slim, ensure_ascii=False), flush=True)
    results[brain] = rec["ok"]
    time.sleep(3)

print("SUMMARY", json.dumps(results), flush=True)
