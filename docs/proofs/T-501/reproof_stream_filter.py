import json, os, time, urllib.request, urllib.error
from pathlib import Path
BASE="http://127.0.0.1:8788"
KEY=os.environ.get("WEBAGENT_API_KEY","t501-local")
OUT=Path(r"C:\Users\storax\projects\GitHub\webagent-rs\docs\proofs\T-501")
OUT.mkdir(parents=True, exist_ok=True)
BRAINS=["kimi","mistral","zai"]

def req(path, payload, timeout=200):
    data=json.dumps(payload).encode()
    r=urllib.request.Request(BASE+path, data=data, headers={"Content-Type":"application/json","Authorization":"Bearer "+KEY}, method="POST")
    t0=time.perf_counter()
    try:
        with urllib.request.urlopen(r, timeout=timeout) as resp:
            raw=resp.read().decode("utf-8","replace"); code=resp.status
    except urllib.error.HTTPError as e:
        raw=e.read().decode("utf-8","replace"); code=e.code
    except Exception as e:
        return {"ok":False,"error":str(e),"latency_ms":int((time.perf_counter()-t0)*1000)}
    return {"ok":200<=code<300,"http":code,"latency_ms":int((time.perf_counter()-t0)*1000),"raw":raw[:12000]}

def deltas(raw):
    texts=[]
    for line in (raw or "").splitlines():
        if not line.startswith("data:"): continue
        chunk=line[5:].strip()
        if chunk=="[DONE]": continue
        try: obj=json.loads(chunk)
        except Exception: continue
        for ch in obj.get("choices") or []:
            d=(ch.get("delta") or {}).get("content")
            if d: texts.append(d)
    return texts

results=[]
for brain in BRAINS:
    print("==", brain, flush=True)
    chat=req("/v1/chat/completions",{"model":f"webagent/{brain}","stream":True,"messages":[{"role":"user","content":"Reply with exactly the token STREAM_OK and nothing else."}]})
    ds=deltas(chat.get("raw") or "")
    joined="".join(ds)
    clean = joined.strip()=="STREAM_OK" or joined.strip().startswith("STREAM_OK") and len(joined.strip())<=12
    rec={"model":f"webagent/{brain}","ok":bool(chat.get("ok") and clean),"http":chat.get("http"),"latency_ms":chat.get("latency_ms"),"delta_count":len(ds),"joined":joined[:300],"error":chat.get("error"),"filter_build":"2ba2dce+060bcba","headless":True,"api_port":8788}
    (OUT/f"streaming_{brain}_2026-09-06b.json").write_text(json.dumps(rec,indent=2)+"\n",encoding="utf-8")
    print(rec, flush=True)
    results.append({"brain":brain,"ok":rec["ok"]})
(OUT/"t501_stream_reproof_filter_summary.json").write_text(json.dumps(results,indent=2)+"\n",encoding="utf-8")
print("SUMMARY", results, flush=True)
