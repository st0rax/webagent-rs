#!/usr/bin/env python3
"""Official OpenAI Python SDK black-box against the local WebAgent bridge."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from openai import OpenAI
from openai import BadRequestError


def dump(path: Path, payload) -> None:
    path.write_text(json.dumps(payload, indent=2, default=str), encoding="utf-8")


def main() -> int:
    base = os.environ["WEBAGENT_T404_BASE"]
    key = os.environ["WEBAGENT_T404_KEY"]
    expect = os.environ["WEBAGENT_T404_EXPECT"]
    dump_dir = Path(os.environ["WEBAGENT_T404_DUMP"])
    dump_dir.mkdir(parents=True, exist_ok=True)

    client = OpenAI(base_url=base, api_key=key, max_retries=0)

    models = client.models.list()
    ids = [item.id for item in models.data]
    dump(dump_dir / "python_sdk_models.json", {"ids": ids, "object": models.object})
    if "webagent/chatgpt" not in ids:
        raise SystemExit(f"python sdk models missing webagent/chatgpt: {ids}")

    chat = client.chat.completions.create(
        model="webagent/chatgpt",
        messages=[{"role": "user", "content": "T404 ping"}],
    )
    dump(dump_dir / "python_sdk_chat.json", chat.model_dump())
    if chat.choices[0].message.content != expect:
        raise SystemExit(f"python sdk chat got {chat.choices[0].message.content!r}")

    parts: list[str] = []
    stream = client.chat.completions.create(
        model="webagent/chatgpt",
        messages=[{"role": "user", "content": "T404 ping"}],
        stream=True,
    )
    for event in stream:
        delta = event.choices[0].delta.content if event.choices else None
        if delta:
            parts.append(delta)
    joined = "".join(parts)
    dump(dump_dir / "python_sdk_chat_stream.json", {"text": joined, "parts": parts})
    if joined != expect:
        raise SystemExit(f"python sdk stream got {joined!r}")

    response = client.responses.create(model="webagent/chatgpt", input="T404 ping")
    dump(dump_dir / "python_sdk_responses.json", response.model_dump())
    text = getattr(response, "output_text", None) or ""
    if text != expect:
        raise SystemExit(f"python sdk responses got {text!r}")

    try:
        client.chat.completions.create(
            model="webagent/chatgpt",
            messages=[{"role": "user", "content": "x"}],
            extra_body={"seed": 7},
        )
        raise SystemExit("python sdk seed should have been rejected")
    except BadRequestError as error:
        dump(
            dump_dir / "python_sdk_seed_reject.json",
            {"status": error.status_code, "body": error.body},
        )
        body = json.dumps(error.body)
        if "unsupported_parameter" not in body or "seed" not in body:
            raise SystemExit(f"python sdk seed reject unexpected: {error.body!r}")

    print("python openai sdk ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
