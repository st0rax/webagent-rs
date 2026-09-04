import json, os, time, urllib.request, urllib.error
from pathlib import Path

BASE = "http://127.0.0.1:8787"
KEY = os.environ.get("WEBAGENT_API_KEY", "t501-local")
OUT = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
OUT.mkdir(parents=True, exist_ok=True)
BRAINS = ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "perplexity", "qwen", "zai"]

def req(path, payload, timeout=200):
    data = json.dumps(payload).encode("utf-8")
    r = urllib.request.Request(
        BASE + path,
        data=data,
        headers={
            "Content-Type": "application/json",
            "Authorization": "Bearer " + KEY,
        },
        method="POST",
    )
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            body = resp.read()
            code = resp.status
            raw = body.decode("utf-8", "replace")
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", "replace")
        code = e.code
    except Exception as e:
        return {"ok": False, "error": str(e), "latency_ms": int((time.perf_counter() - t0) * 1000)}
    ms = int((time.perf_counter() - t0) * 1000)
    return {"ok": 200 <= code < 300, "http": code, "latency_ms": ms, "raw": raw[:8000]}

def has_delta(raw):
    texts = []
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
        t = obj.get("type") or ""
        if "delta" in t:
            d = obj.get("delta")
            if isinstance(d, str) and d:
                texts.append(d)
            elif isinstance(obj.get("text"), str) and obj["text"]:
                texts.append(obj["text"])
    return texts

results = []
for brain in BRAINS:
    model = f"webagent/{brain}"
    print(f"== {brain} stream chat ==", flush=True)
    chat = req(
        "/v1/chat/completions",
        {
            "model": model,
            "stream": True,
            "messages": [{"role": "user", "content": "Reply with exactly the token STREAM_OK and nothing else."}],
        },
    )
    deltas = has_delta(chat.get("raw") or "")
    chat_rec = {
        "model": model,
        "ok": bool(chat.get("ok") and deltas and "STREAM_OK" in "".join(deltas)),
        "latency_ms": chat.get("latency_ms"),
        "http": chat.get("http"),
        "delta_count": len(deltas),
        "joined": "".join(deltas)[:200],
        "error": chat.get("error"),
    }
    (OUT / f"streaming_{brain}_2026-09-04.json").write_text(json.dumps(chat_rec, indent=2) + "\n", encoding="utf-8")
    print(chat_rec, flush=True)
    print(f"== {brain} responses ==", flush=True)
    resp = req(
        "/v1/responses",
        {
            "model": model,
            "stream": True,
            "input": "Reply with exactly the token RESP_OK and nothing else.",
        },
    )
    rdeltas = has_delta(resp.get("raw") or "")
    raw = resp.get("raw") or ""
    resp_rec = {
        "model": model,
        "ok": bool(resp.get("ok") and "RESP_OK" in ("".join(rdeltas) + raw)),
        "latency_ms": resp.get("latency_ms"),
        "http": resp.get("http"),
        "delta_count": len(rdeltas),
        "joined": "".join(rdeltas)[:200],
        "error": resp.get("error"),
        "raw_head": raw[:400],
    }
    (OUT / f"api_responses_{brain}_2026-09-04.json").write_text(json.dumps(resp_rec, indent=2) + "\n", encoding="utf-8")
    slim = {k: resp_rec[k] for k in resp_rec if k != "raw_head"}
    print(slim, flush=True)
    results.append({"brain": brain, "streaming": chat_rec["ok"], "api_responses": resp_rec["ok"]})

(OUT / "t501_stream_responses_summary.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
print("SUMMARY", json.dumps(results), flush=True)
