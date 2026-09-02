import fs from "node:fs";
import path from "node:path";

const base = process.env.WEBAGENT_T404_BASE.replace(/\/$/, "");
const key = process.env.WEBAGENT_T404_KEY;
const expect = process.env.WEBAGENT_T404_EXPECT;
const dumpDir = process.env.WEBAGENT_T404_DUMP;

function dump(name, payload) {
  fs.writeFileSync(
    path.join(dumpDir, name),
    JSON.stringify(payload, null, 2),
    "utf8",
  );
}

async function call(pathname, body) {
  const response = await fetch(`${base}${pathname}`, {
    method: body ? "POST" : "GET",
    headers: {
      Authorization: `Bearer ${key}`,
      "Content-Type": "application/json",
    },
    body: body ? JSON.stringify(body) : undefined,
  });
  const json = await response.json();
  return { status: response.status, body: json };
}

const models = await call("/models");
dump("fetch_models.json", models);
const ids = models.body.data.map((item) => item.id);
if (models.status !== 200 || !ids.includes("webagent/chatgpt")) {
  throw new Error(`fetch models unexpected: ${JSON.stringify(models)}`);
}

const chat = await call("/chat/completions", {
  model: "webagent/chatgpt",
  messages: [{ role: "user", content: "ping" }],
});
dump("fetch_chat.json", chat);
if (chat.status !== 200 || chat.body.choices[0].message.content !== expect) {
  throw new Error(`fetch chat unexpected: ${JSON.stringify(chat)}`);
}

const response = await call("/responses", {
  model: "webagent/chatgpt",
  input: "ping",
});
dump("fetch_responses.json", response);
if (response.status !== 200 || response.body.output_text !== expect) {
  throw new Error(`fetch responses unexpected: ${JSON.stringify(response)}`);
}

const seed = await call("/chat/completions", {
  model: "webagent/chatgpt",
  seed: 7,
  messages: [{ role: "user", content: "x" }],
});
dump("fetch_seed_reject.json", seed);
const err = JSON.stringify(seed.body);
if (
  seed.status !== 400 ||
  !err.includes("unsupported_parameter") ||
  !err.includes("seed")
) {
  throw new Error(`fetch seed unexpected: ${err}`);
}

console.log("fetch client ok");
