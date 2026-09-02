#!/usr/bin/env python3
"""Stdlib urllib client (no OpenAI SDK) against the local WebAgent bridge."""

from __future__ import annotations

import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path


def dump(path: Path, payload) -> None:
    path.write_text(json.dumps(payload, indent=2, default=str), encoding="utf-8")


def request(url: str, key: str, body: dict | None = None) -> tuple[int, dict | str]:
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json",
        },
        method="GET" if body is None else "POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=10) as response:
            raw = response.read().decode("utf-8")
            try:
                return response.status, json.loads(raw)
            except json.JSONDecodeError:
                return response.status, raw
    except urllib.error.HTTPError as error:
        raw = error.read().decode("utf-8")
        try:
            return error.code, json.loads(raw)
        except json.JSONDecodeError:
            return error.code, raw


def main() -> int:
    base = os.environ["WEBAGENT_T404_BASE"].rstrip("/")
    key = os.environ["WEBAGENT_T404_KEY"]
    expect = os.environ["WEBAGENT_T404_EXPECT"]
    dump_dir = Path(os.environ["WEBAGENT_T404_DUMP"])
    dump_dir.mkdir(parents=True, exist_ok=True)

    status, models = request(f"{base}/models", key)
    dump(dump_dir / "urllib_models.json", {"status": status, "body": models})
    if status != 200:
        raise SystemExit(f"urllib models status {status}")
    ids = [item["id"] for item in models["data"]]
    if "webagent/chatgpt" not in ids:
        raise SystemExit(f"urllib models missing webagent/chatgpt: {ids}")

    status, chat = request(
        f"{base}/chat/completions",
        key,
        {"model": "webagent/chatgpt", "messages": [{"role": "user", "content": "ping"}]},
    )
    dump(dump_dir / "urllib_chat.json", {"status": status, "body": chat})
    if status != 200 or chat["choices"][0]["message"]["content"] != expect:
        raise SystemExit(f"urllib chat unexpected: {chat!r}")

    status, response = request(
        f"{base}/responses",
        key,
        {"model": "webagent/chatgpt", "input": "ping"},
    )
    dump(dump_dir / "urllib_responses.json", {"status": status, "body": response})
    if status != 200 or response.get("output_text") != expect:
        raise SystemExit(f"urllib responses unexpected: {response!r}")

    status, seed = request(
        f"{base}/chat/completions",
        key,
        {
            "model": "webagent/chatgpt",
            "seed": 7,
            "messages": [{"role": "user", "content": "x"}],
        },
    )
    dump(dump_dir / "urllib_seed_reject.json", {"status": status, "body": seed})
    if status != 400:
        raise SystemExit(f"urllib seed status {status}")
    err = json.dumps(seed)
    if "unsupported_parameter" not in err or "seed" not in err:
        raise SystemExit(f"urllib seed body {seed!r}")

    print("urllib client ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
