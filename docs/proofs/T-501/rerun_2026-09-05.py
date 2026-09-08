import base64, json, os, struct, time, urllib.request, urllib.error, zlib
from pathlib import Path

BASE = "http://127.0.0.1:8788"
KEY = os.environ.get("WEBAGENT_API_KEY", "t501-local")
OUT = Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
DATE = "2026-09-05"

# Offene Zellen aus CAPABILITY_MATRIX.json as_of 2026-09-04:
#   streaming: kimi (Reasoning-Echo), mistral (Timestamp/Capacity), zai (circuit_open)
#   api_responses: mistral, zai
#   attachment: kimi (502 ABSENDEKNOPF), qwen (502 0-von-1), not_run: mistral/zai/auto
#   managed_tools: brain-global 400 by design (Re-Run via auto reicht als Beweis)
STREAM_TODO = ["kimi", "mistral", "zai"]
RESP_TODO = ["mistral", "zai"]
ATTACH_TODO = ["kimi", "qwen", "mistral", "zai", "auto"]


def red_png_b64() -> str:
    # 1x1 rotes PNG, deterministisch erzeugt (kein externes Asset noetig).
    sig = b"\x89PNG\r\n\x1a\n"
    def chunk(typ: bytes, data: bytes) -> bytes:
        c = struct.pack(">I", len(data)) + typ + data
        return c + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)
    ihdr = struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0)
    idat = zlib.compress(b"\x00\xff\x00\x00")
    png = sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
    return base64.b64encode(png).decode("ascii")


def data_url() -> str:
    return "data:image/png;base64," + red_png_b64()


def req(path, payload, timeout=200):
    data = json.dumps(payload).encode("utf-8")
    r = urllib.request.Request(
        BASE + path, data=data,
        headers={"Content-Type": "application/json", "Authorization": "Bearer " + KEY},
        method="POST",
    )
    t0 = time.perf_counter()
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            body = resp.read()
            raw = body.decode("utf-8", "replace")
            code = resp.status
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8", "replace")
        code = e.code
    except Exception as e:
        return {"ok": False, "http": None, "latency_ms": int((time.perf_counter() - t0) * 1000), "error": str(e), "raw": ""}
    return {"ok": 200 <= code < 300, "http": code, "latency_ms": int((time.perf_counter() - t0) * 1000), "raw": raw}


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


def write(name, rec):
    p = OUT / f"{name}_{DATE}.json"
    p.write_text(json.dumps(rec, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"WROTE {p.name}")
    return p


results = {"streaming": {}, "api_responses": {}, "attachment": {}, "managed_tools": None}

for brain in STREAM_TODO:
    print(f"== [{DATE}] streaming {brain} ==", flush=True)
    model = f"webagent/{brain}"
    chat = req("/v1/chat/completions", {"model": model, "stream": True, "messages": [{"role": "user", "content": "Reply with exactly the token STREAM_OK and nothing else."}]})
    deltas = has_delta(chat.get("raw") or "")
    joined = "".join(deltas).strip()
    clean = joined == "STREAM_OK" or joined.startswith("STREAM_OK")
    rec = {
        "model": model, "ok": bool(chat.get("ok") and deltas and "STREAM_OK" in joined and clean),
        "http": chat.get("http"), "latency_ms": chat.get("latency_ms"),
        "delta_count": len(deltas), "joined": joined[:300],
        "error": chat.get("error"), "note": None,
    }
    if not rec["ok"]:
        rec["note"] = "noch ohne sauberes STREAM_OK (Provider-Quirk/Capacity) - ehrlich dokumentiert, keine gefaelschte Passage"
    write(f"streaming_{brain}", rec)
    results["streaming"][brain] = rec["ok"]

for brain in RESP_TODO:
    print(f"== [{DATE}] api_responses {brain} ==", flush=True)
    model = f"webagent/{brain}"
    resp = req("/v1/responses", {"model": model, "stream": True, "input": "Reply with exactly the token RESP_OK and nothing else."})
    rdeltas = has_delta(resp.get("raw") or "")
    raw = resp.get("raw") or ""
    joined_r = "".join(rdeltas)
    hay = joined_r + raw
    rec = {
        "model": model, "ok": bool(resp.get("ok") and "RESP_OK" in hay),
        "http": resp.get("http"), "latency_ms": resp.get("latency_ms"),
        "delta_count": len(rdeltas), "joined": joined_r[:300],
        "error": resp.get("error"), "note": None,
    }
    if not rec["ok"]:
        rec["note"] = "noch ohne sauberes RESP_OK (Provider-Capacity/Transport) - ehrlich dokumentiert"
    write(f"api_responses_{brain}", rec)
    results["api_responses"][brain] = rec["ok"]

for brain in ATTACH_TODO:
    print(f"== [{DATE}] attachment {brain} ==", flush=True)
    model = f"webagent/{brain}"
    payload = {
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "What color is the attached image? Answer with exactly: RED"},
                {"type": "image_url", "image_url": {"url": data_url()}},
            ],
        }],
    }
    a = req("/v1/chat/completions", payload, timeout=240)
    reply = ""
    try:
        body = json.loads(a.get("raw") or "{}")
        reply = (body.get("choices") or [{}])[0].get("message", {}).get("content") or ""
        if isinstance(reply, list):
            reply = "".join((p.get("text") or "") for p in reply if isinstance(p, dict))
    except Exception:
        pass
    rec = {
        "model": model, "ok": bool(a.get("ok") and "RED" in reply.upper()),
        "http": a.get("http"), "latency_ms": a.get("latency_ms"),
        "attempts": 1, "attachment": "1x1 red PNG (base64 image_url data:image/png)",
        "reply": reply[:300], "error": a.get("error"),
        "error_body": (a.get("raw") or "")[:400] if not a.get("ok") else None,
        "note": None,
    }
    if not rec["ok"]:
        rec["note"] = "fail-closed/kein RED - ehrlich dokumentiert, keine gefaelschte Passage"
    write(f"attachment_{brain}", rec)
    results["attachment"][brain] = rec["ok"]

# managed_tools: brain-global 400 by design (by-design Re-Run reicht als Beweis)
print(f"== [{DATE}] managed_tools (by design) ==", flush=True)
mt = req("/v1/chat/completions", {
    "model": "webagent/auto",
    "messages": [{"role": "user", "content": "Say hi"}],
    "tools": [{"type": "function", "function": {"name": "probe", "description": "probe", "parameters": {"type": "object", "properties": {}}}}],
})
mt_rec = {
    "name": "managed_tools_client_tools_rejected_by_design",
    "area": "managed_tools", "brain": "auto",
    "ok": mt.get("http") == 400,
    "http": mt.get("http"), "date": DATE, "latency_ms": mt.get("latency_ms"),
    "error_body": (mt.get("raw") or "")[:500],
    "note": "Bridge lehnt aktive Client-Function-Tools bewusst ab (clean browser text profile): erwarteter 400 invalid_request_error bestaetigt by design.",
}
write("managed_tools_rejected", mt_rec)
results["managed_tools"] = mt_rec["ok"]

summary = OUT / f"t501_rerun_{DATE}_summary.json"
summary.write_text(json.dumps(results, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print("SUMMARY_JSON:\n" + json.dumps(results, indent=2, ensure_ascii=False))