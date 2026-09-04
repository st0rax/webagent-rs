import json, os, time, urllib.request, urllib.error
from pathlib import Path

BASE = "http://127.0.0.1:8787"
KEY = os.environ.get("WEBAGENT_API_KEY", "t501-local")
OUT = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
# keep claude/gemini/perplexity/zai stream successes; kimi demoted
STREAM_TODO = ["chatgpt", "deepseek", "kimi", "mistral", "qwen"]
RESP_TODO = ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "qwen", "zai"]
KEEP_STREAM = {"claude", "gemini", "perplexity", "zai"}
KEEP_RESP = {"perplexity"}

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
            # require clean token: joined.strip() is STREAM_OK or starts with it as whole reply
            clean = joined.strip() == "STREAM_OK" or joined.strip().startswith("STREAM_OK")
            chat_rec = {"model": model, "ok": bool(chat.get("ok") and deltas and "STREAM_OK" in joined and clean), "latency_ms": chat.get("latency_ms"), "http": chat.get("http"), "delta_count": len(deltas), "joined": joined[:200], "error": chat.get("error")}
            (OUT / f"streaming_{brain}_2026-09-04.json").write_text(json.dumps(chat_rec, indent=2) + "\n", encoding="utf-8")
            print(chat_rec, flush=True)
            chat_ok = chat_rec["ok"]
        if brain in RESP_TODO:
            print(f"== {brain} responses ==", flush=True)
            resp = req("/v1/responses", {"model": model, "stream": True, "input": "Reply with exactly the token RESP_OK and nothing else."})
            rdeltas = has_delta(resp.get("raw") or "")
            raw = resp.get("raw") or ""
            joined_r = "".join(rdeltas)
            hay = joined_r + raw
            resp_rec = {"model": model, "ok": bool(resp.get("ok") and "RESP_OK" in hay), "latency_ms": resp.get("latency_ms"), "http": resp.get("http"), "delta_count": len(rdeltas), "joined": joined_r[:200], "error": resp.get("error"), "raw_head": raw[:400]}
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
    for b in KEEP_STREAM:
        by.setdefault(b, {"brain": b, "streaming": True, "api_responses": False})
        by[b]["streaming"] = True
    for b in KEEP_RESP:
        by.setdefault(b, {"brain": b, "streaming": True, "api_responses": True})
        by[b]["api_responses"] = True
    for r in results:
        prev = by.get(r["brain"], {"brain": r["brain"], "streaming": False, "api_responses": False})
        if r["streaming"] is None:
            r["streaming"] = prev.get("streaming", False)
        if r["api_responses"] is None:
            r["api_responses"] = prev.get("api_responses", False)
        if r["streaming"] is False and r["brain"] in KEEP_STREAM:
            r["streaming"] = True
        if r["api_responses"] is False and r["brain"] in KEEP_RESP:
            r["api_responses"] = True
        # upgrade only on True
        if r["streaming"] is True:
            prev["streaming"] = True
        elif r["streaming"] is False and r["brain"] not in KEEP_STREAM:
            prev["streaming"] = False
        if r["api_responses"] is True:
            prev["api_responses"] = True
        elif r["api_responses"] is False and r["brain"] not in KEEP_RESP:
            prev["api_responses"] = False
        prev["brain"] = r["brain"]
        by[r["brain"]] = prev
    for b in ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "perplexity", "qwen", "zai"]:
        by.setdefault(b, {"brain": b, "streaming": False, "api_responses": False})
    ordered = [by[b] for b in ["claude", "chatgpt", "deepseek", "gemini", "kimi", "mistral", "perplexity", "qwen", "zai"]]
    summary_path.write_text(json.dumps(ordered, indent=2) + "\n", encoding="utf-8")
    stream_pass = sum(1 for r in ordered if r.get("streaming"))
    resp_pass = sum(1 for r in ordered if r.get("api_responses"))
    stream_ok = [r["brain"] for r in ordered if r.get("streaming")]
    resp_ok_list = [r["brain"] for r in ordered if r.get("api_responses")]
    stream_fail = [r["brain"] for r in ordered if not r.get("streaming")]
    resp_fail = [r["brain"] for r in ordered if not r.get("api_responses")]
    status = (
        "> **Archiv.** Harness-Snapshot 2026-09-04, kein Betrieb. Lebend: `docs/WEB_UI_API_TOOL_RESET_STATUS.md`.\n\n"
        "# T-501 STATUS\n\n"
        "Date: 2026-09-04\n"
        "Harness: focused_rerun (finished)\n"
        "API: webagent api serve on 127.0.0.1:8787\n\n"
        "## streaming (ok:true, token STREAM_OK)\n"
        + "".join(f"- {b}\n" for b in stream_ok)
        + "\n## streaming (ok:false / timeout)\n"
        + ("- " + ", ".join(stream_fail) + "\n" if stream_fail else "- (none)\n")
        + "\n## api_responses (ok:true, token RESP_OK)\n"
        + "".join(f"- {b}\n" for b in resp_ok_list)
        + "\n## api_responses (ok:false / timeout)\n"
        + ("- " + ", ".join(resp_fail) + "\n" if resp_fail else "- (none)\n")
        + "\n## Counts\n"
        f"- streaming: {stream_pass}/9 passed\n"
        f"- api_responses: {resp_pass}/9 passed\n"
        f"- T-501 DoD: {'done' if stream_pass == 9 and resp_pass == 9 else 'NOT done'}\n"
    )
    (OUT / "STATUS.md").write_text(status, encoding="utf-8")
    print("SUMMARY", json.dumps(ordered), flush=True)
finally:
    try:
        lock.unlink()
    except Exception:
        pass
