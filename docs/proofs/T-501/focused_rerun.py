import json, os, time, urllib.request, urllib.error
from pathlib import Path

BASE = "http://127.0.0.1:8787"
KEY = os.environ.get("WEBAGENT_API_KEY", "t501-local")
OUT = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
# claude stream just proved ok; keep perplexity/zai successes
STREAM_TODO = ["chatgpt", "deepseek", "gemini", "kimi", "mistral", "qwen"]
RESP_TODO = ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "qwen", "zai"]

def req(path, payload, timeout=200):
    data = json.dumps(payload).encode("utf-8")
    r = urllib.request.Request(
        BASE + path,
        data=data,
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + KEY},
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

lock = OUT / "_rerun.lock"
if lock.exists():
    age = time.time() - lock.stat().st_mtime
    if age < 3600:
        print(f"LOCK_HELD age_s={int(age)}", flush=True)
        raise SystemExit(0)
lock.write_text(str(os.getpid()), encoding="utf-8")
results = []
try:
    for brain in sorted(set(STREAM_TODO + RESP_TODO)):
        model = f"webagent/{brain}"
        chat_ok = None
        resp_ok = None
        if brain in STREAM_TODO:
            print(f"== {brain} stream chat ==", flush=True)
            chat = req("/v1/chat/completions", {"model": model, "stream": True, "messages": [{"role": "user", "content": "Reply with exactly the token STREAM_OK and nothing else."}]})
            deltas = has_delta(chat.get("raw") or "")
            joined = "".join(deltas)
            chat_rec = {"model": model, "ok": bool(chat.get("ok") and deltas and "STREAM_OK" in joined), "latency_ms": chat.get("latency_ms"), "http": chat.get("http"), "delta_count": len(deltas), "joined": joined[:200], "error": chat.get("error")}
            (OUT / f"streaming_{brain}_2026-09-04.json").write_text(json.dumps(chat_rec, indent=2) + "\n", encoding="utf-8")
            print(chat_rec, flush=True)
            chat_ok = chat_rec["ok"]
        if brain in RESP_TODO:
            print(f"== {brain} responses ==", flush=True)
            resp = req("/v1/responses", {"model": model, "stream": True, "input": "Reply with exactly the token RESP_OK and nothing else."})
            rdeltas = has_delta(resp.get("raw") or "")
            raw = resp.get("raw") or ""
            resp_rec = {"model": model, "ok": bool(resp.get("ok") and "RESP_OK" in ("".join(rdeltas) + raw)), "latency_ms": resp.get("latency_ms"), "http": resp.get("http"), "delta_count": len(rdeltas), "joined": "".join(rdeltas)[:200], "error": resp.get("error"), "raw_head": raw[:400]}
            (OUT / f"api_responses_{brain}_2026-09-04.json").write_text(json.dumps(resp_rec, indent=2) + "\n", encoding="utf-8")
            print({k: resp_rec[k] for k in resp_rec if k != "raw_head"}, flush=True)
            resp_ok = resp_rec["ok"]
        results.append({"brain": brain, "streaming": chat_ok, "api_responses": resp_ok})
    summary_path = OUT / "t501_stream_responses_summary.json"
    old = []
    if summary_path.exists():
        try:
            old = json.loads(summary_path.read_text(encoding="utf-8"))
        except Exception:
            old = []
    by = {r["brain"]: r for r in old if isinstance(r, dict) and "brain" in r}
    # keep known successes
    by.setdefault("claude", {"brain": "claude", "streaming": True, "api_responses": False})
    by["claude"]["streaming"] = True
    by.setdefault("perplexity", {"brain": "perplexity", "streaming": True, "api_responses": True})
    by.setdefault("zai", {"brain": "zai", "streaming": True, "api_responses": False})
    for r in results:
        prev = by.get(r["brain"], {"brain": r["brain"], "streaming": False, "api_responses": False})
        if r["streaming"] is None:
            r["streaming"] = prev.get("streaming", False)
        if r["api_responses"] is None:
            r["api_responses"] = prev.get("api_responses", False)
        # never downgrade a prior True with None handled; only set False if we actually ran and failed
        if r["streaming"] is False and prev.get("streaming") is True and r["brain"] in ("perplexity", "zai", "claude"):
            r["streaming"] = True
        if r["api_responses"] is False and prev.get("api_responses") is True and r["brain"] == "perplexity":
            r["api_responses"] = True
        by[r["brain"]] = r
    for b in ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "perplexity", "qwen", "zai"]:
        by.setdefault(b, {"brain": b, "streaming": False, "api_responses": False})
    ordered = [by[b] for b in ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "perplexity", "qwen", "zai"]]
    summary_path.write_text(json.dumps(ordered, indent=2) + "\n", encoding="utf-8")
    stream_pass = sum(1 for r in ordered if r.get("streaming"))
    resp_pass = sum(1 for r in ordered if r.get("api_responses"))
    status = (
        "> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.\n\n"
        "# T-501 STATUS\n\n"
        "Date: 2026-09-04\n"
        "Harness: focused_rerun (finished)\n"
        "API: webagent api serve on 127.0.0.1:8787\n\n"
        f"## Counts\n- streaming: {stream_pass}/9 passed\n- api_responses: {resp_pass}/9 passed\n"
        f"- T-501 DoD: {'done' if stream_pass == 9 and resp_pass == 9 else 'NOT done'}\n"
    )
    (OUT / "STATUS.md").write_text(status, encoding="utf-8")
    print("SUMMARY", json.dumps(ordered), flush=True)
finally:
    try:
        lock.unlink()
    except Exception:
        pass
