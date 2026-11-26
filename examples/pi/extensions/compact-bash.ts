/**
 * compact-bash — provider-unabhaengige Pi-Erweiterung.
 *
 * Ersetzt den Built-in-bash-Tool durch eine Variante, die
 *  1. Kurzpfad-Variablen ($REPO, $PI, $PI_SESS, $WG, $WG_DATA, $TMP, $HOME)
 *     in die Shell injiziert und in der Ausgabe zuruecksetzt,
 *  2. Junction-Loop-Wiederholungen (…\Anwendungsdaten\Anwendungsdaten\…) kollabiert,
 *  3. doppelte Zeilen zu [×N] zusammenfasst und
 *  4. die Ausgabe kappen kann, mit Zeiger auf die vollstaendige Datei.
 *
 * Kein WebAgent-/Bridge-Bezug: funktioniert mit jedem Pi-Provider.
 * Not-Aus: COMPACT_BASH_OFF=1 gibt die Ausgabe unveraendert zurueck.
 */
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { createBashTool } from "@earendil-works/pi-coding-agent";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildAliases,
  compactOutput,
  makeSpawnEnv,
  shortenPaths,
} from "./compact-bash-lib.mjs";

export default function compactBash(pi: ExtensionAPI) {
  const cwd = process.cwd();
  const aliases = buildAliases(process.env, cwd);
  const base = createBashTool(cwd, {
    spawnHook: ({ command, cwd: execCwd, env }) => ({
      command,
      cwd: execCwd,
      env: makeSpawnEnv(env, aliases),
    }),
  });

  const guidelines = [
    ...((base as { promptGuidelines?: string[] }).promptGuidelines ?? []),
    "Use the exported short-path env vars instead of long absolute paths: $env:REPO / $env:PI / $env:PI_SESS / $env:WG / $env:WG_DATA / $env:TMP in PowerShell, $REPO / $PI / ... in bash. Bash output is shortened and capped; when a full-output pointer is shown, read only the needed line range from it.",
  ];

  pi.registerTool({
    ...base,
    name: "bash",
    promptGuidelines: guidelines,

    async execute(toolCallId, params, signal, onUpdate, ctx) {
      const result = await base.execute(toolCallId, params, signal, onUpdate, ctx);
      if (process.env.COMPACT_BASH_OFF === "1" || !Array.isArray(result.content)) {
        return result;
      }

      const content = [];
      for (const part of result.content) {
        const textPart = part as { type?: string; text?: string };
        if (textPart.type !== "text" || typeof textPart.text !== "string") {
          content.push(part);
          continue;
        }

        const compacted = compactOutput(textPart.text, aliases, process.env);
        let text = compacted.text;
        if (compacted.stats.capped) {
          const dir = await mkdtemp(join(tmpdir(), "compact-bash-"));
          const file = join(dir, "output.log");
          await writeFile(file, compacted.fullText, "utf8");
          const pointer = shortenPaths(file, aliases);
          text += `\n\n[compact-bash: ${compacted.stats.totalLines} Zeilen / ${compacted.stats.totalBytes} Bytes, ${compacted.stats.outputLines} Zeilen gezeigt. Vollstaendig: ${pointer}]`;
        }
        content.push({ ...textPart, text });
      }

      return { ...result, content };
    },
  });
}
