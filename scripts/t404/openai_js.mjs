import fs from "node:fs";
import path from "node:path";
import OpenAI, { APIError } from "openai";

const base = process.env.WEBAGENT_T404_BASE;
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

const client = new OpenAI({
  baseURL: base,
  apiKey: key,
  maxRetries: 0,
});

const models = await client.models.list();
const ids = models.data.map((item) => item.id);
dump("js_sdk_models.json", { ids, object: models.object });
if (!ids.includes("webagent/chatgpt")) {
  throw new Error(`js sdk models missing webagent/chatgpt: ${ids}`);
}

const chat = await client.chat.completions.create({
  model: "webagent/chatgpt",
  messages: [{ role: "user", content: "T404 ping" }],
});
dump("js_sdk_chat.json", chat);
if (chat.choices[0].message.content !== expect) {
  throw new Error(`js sdk chat got ${chat.choices[0].message.content}`);
}

const parts = [];
const stream = await client.chat.completions.create({
  model: "webagent/chatgpt",
  messages: [{ role: "user", content: "T404 ping" }],
  stream: true,
});
for await (const event of stream) {
  const delta = event.choices[0]?.delta?.content;
  if (delta) parts.push(delta);
}
const joined = parts.join("");
dump("js_sdk_chat_stream.json", { text: joined, parts });
if (joined !== expect) {
  throw new Error(`js sdk stream got ${joined}`);
}

const response = await client.responses.create({
  model: "webagent/chatgpt",
  input: "T404 ping",
});
dump("js_sdk_responses.json", response);
if (response.output_text !== expect) {
  throw new Error(`js sdk responses got ${response.output_text}`);
}

try {
  await client.chat.completions.create({
    model: "webagent/chatgpt",
    messages: [{ role: "user", content: "x" }],
    seed: 7,
  });
  throw new Error("js sdk seed should have been rejected");
} catch (error) {
  if (!(error instanceof APIError)) throw error;
  dump("js_sdk_seed_reject.json", {
    status: error.status,
    error: error.error,
  });
  const body = JSON.stringify(error.error ?? {});
  if (!body.includes("unsupported_parameter") || !body.includes("seed")) {
    throw new Error(`js sdk seed reject unexpected: ${body}`);
  }
}

console.log("js openai sdk ok");
