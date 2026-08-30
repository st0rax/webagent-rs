import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

const BASE_URL = "http://127.0.0.1:8787/v1";
const FALLBACK_BRAINS = [
  "chatgpt",
  "claude",
  "deepseek",
  "gemini",
  "kimi",
  "mistral",
  "qwen",
  "zai",
];

type CatalogEntry = {
  id: string;
  brain?: string;
  context_window?: number;
  max_tokens?: number;
  modalities?: { input?: string[]; output?: string[] };
};

function modelConfig(entry: CatalogEntry) {
  const brain = entry.brain ?? entry.id.replace(/^webagent\//, "");
  return {
    id: entry.id,
    name: `WebAgent ${brain}`,
    reasoning: false,
    input: (entry.modalities?.input?.length ? entry.modalities.input : ["text", "image", "audio"]) as Array<"text" | "image" | "audio">,
    contextWindow: entry.context_window ?? 128000,
    maxTokens: entry.max_tokens ?? 16384,
    cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
    compat: {
      supportsStore: false,
      supportsDeveloperRole: true,
      supportsReasoningEffort: false,
      supportsUsageInStreaming: false,
      supportsFinishReason: true,
      supportsStrictMode: false,
      maxTokensField: "max_tokens" as const,
    },
  };
}

async function discover(signal?: AbortSignal): Promise<CatalogEntry[]> {
  const token = process.env.WEBAGENT_API_KEY;
  if (!token) throw new Error("WEBAGENT_API_KEY fehlt");
  const response = await fetch(`${BASE_URL}/models`, {
    headers: { Authorization: `Bearer ${token}` },
    signal,
  });
  if (!response.ok) throw new Error(`WebAgent-Modellkatalog: HTTP ${response.status}`);
  const payload = (await response.json()) as { data?: CatalogEntry[] };
  const entries = (payload.data ?? []).filter(
    (entry) => typeof entry.id === "string" && entry.id.startsWith("webagent/"),
  );
  if (entries.length === 0) throw new Error("WebAgent-Modellkatalog ist leer");
  return entries;
}

function fallbackCatalog(): CatalogEntry[] {
  return FALLBACK_BRAINS.map((brain) => ({ id: `webagent/${brain}`, brain }));
}

export default async function webagentModels(pi: ExtensionAPI) {
  let catalog: CatalogEntry[];
  try {
    catalog = await discover(AbortSignal.timeout(3000));
  } catch {
    // Pi soll auch starten koennen, wenn der lokale Dienst noch nicht laeuft.
    // /models aktualisiert den Katalog spaeter erneut vom echten Endpoint.
    catalog = fallbackCatalog();
  }

  const register = (entries: CatalogEntry[]) => {
    catalog = entries;
    pi.registerProvider("webagent", {
      name: "WebAgent Browser Brains",
      baseUrl: BASE_URL,
      apiKey: "$WEBAGENT_API_KEY",
      authHeader: true,
      api: "openai-completions",
      models: entries.map(modelConfig),
      refreshModels: async ({ signal }) => (await discover(signal)).map(modelConfig),
    });
  };

  register(catalog);

  pi.registerCommand("models", {
    description: "WebAgent-Brains aktualisieren, auflisten und wechseln",
    getArgumentCompletions: (prefix) => {
      const items = catalog
        .map((entry) => entry.id.replace(/^webagent\//, ""))
        .filter((brain) => brain.startsWith(prefix))
        .map((brain) => ({ value: brain, label: brain }));
      return items.length > 0 ? items : null;
    },
    handler: async (args, ctx) => {
      try {
        register(await discover(AbortSignal.timeout(3000)));
      } catch (error) {
        ctx.ui.notify(
          `Endpoint nicht erreichbar; verwende letzten Katalog: ${error instanceof Error ? error.message : String(error)}`,
          "warning",
        );
      }

      const models = ctx.modelRegistry
        .getAvailable()
        .filter((model) => model.provider === "webagent");
      if (models.length === 0) {
        ctx.ui.notify("Keine WebAgent-Brains verfuegbar.", "error");
        return;
      }

      const requested = args.trim().replace(/^webagent\//, "");
      let selected = requested
        ? models.find((model) => model.id.replace(/^webagent\//, "") === requested)
        : undefined;

      if (requested && !selected) {
        ctx.ui.notify(
          `Unbekanntes Brain '${requested}'. Verfuegbar: ${models.map((model) => model.id.replace(/^webagent\//, "")).join(", ")}`,
          "error",
        );
        return;
      }

      if (!selected) {
        if (!ctx.hasUI) {
          ctx.ui.notify(
            `WebAgent-Brains: ${models.map((model) => model.id.replace(/^webagent\//, "")).join(", ")}`,
            "info",
          );
          return;
        }
        const choice = await ctx.ui.select(
          "WebAgent-Brain waehlen",
          models.map((model) => model.id.replace(/^webagent\//, "")),
        );
        if (!choice) return;
        selected = models.find((model) => model.id === `webagent/${choice}`);
      }

      if (!selected || !(await pi.setModel(selected))) {
        ctx.ui.notify("Brain-Wechsel fehlgeschlagen.", "error");
        return;
      }
      ctx.ui.notify(`Aktives Brain: ${selected.id}`, "info");
    },
  });
}
