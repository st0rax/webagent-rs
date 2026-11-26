// Provider-unabhaengige Hilfsfunktionen fuer die compact-bash Pi-Erweiterung.
// Reines ESM-JS ohne Pi-Abhaengigkeit, damit node --test die Logik direkt pruefen kann.
import { join } from "node:path";

const DEFAULTS = Object.freeze({
  maxBytes: 16 * 1024,
  maxLines: 200,
  head: 25,
  tail: 25,
});

function positiveInt(value, fallback) {
  const n = Number(value);
  return Number.isInteger(n) && n > 0 ? n : fallback;
}

export function compactLimits(env = process.env) {
  return {
    maxBytes: positiveInt(env.COMPACT_BASH_MAX_BYTES, DEFAULTS.maxBytes),
    maxLines: positiveInt(env.COMPACT_BASH_MAX_LINES, DEFAULTS.maxLines),
    head: positiveInt(env.COMPACT_BASH_HEAD, DEFAULTS.head),
    tail: positiveInt(env.COMPACT_BASH_TAIL, DEFAULTS.tail),
  };
}

function toShellPath(path) {
  if (/^[A-Za-z]:[\\/]/.test(path)) {
    return path.replace(/^([A-Za-z]):/, (_, drive) => `/${drive.toLowerCase()}`).replace(/\\/g, "/");
  }
  return path.replace(/\\/g, "/");
}

/**
 * Baut die Kurzpfad-Registry. `name` wird als $NAME in Ausgaben und als
 * env-Variable im Shell-Prozess verwendet. Varianten decken Windows-,
 * Git-Bash- und die lokalisierten "Lokale Einstellungen"-Pfade ab.
 */
export function buildAliases(env = process.env, cwd = process.cwd()) {
  const home = env.USERPROFILE || env.HOME || "";
  const local = env.LOCALAPPDATA || (home ? join(home, "AppData", "Local") : "");
  const localizedLocal = home ? join(home, "Lokale Einstellungen", "Anwendungsdaten") : "";
  const agent = env.PI_CODING_AGENT_DIR || (home ? join(home, ".pi", "agent") : "");
  const tmp = env.TMP || env.TEMP || (local ? join(local, "Temp") : "");

  const entries = [];
  const add = (name, base, altBase, ...segments) => {
    if (!base) return;
    const primary = join(base, ...segments);
    const alternate = altBase ? join(altBase, ...segments) : "";
    const variants = [primary, primary.replace(/\\/g, "/")];
    if (alternate) variants.push(alternate, alternate.replace(/\\/g, "/"));
    for (const variant of [...variants]) {
      if (/^[A-Za-z]:[\\/]/.test(variant)) {
        variants.push(variant.replace(/^([A-Za-z]):/, (_, drive) => `/${drive.toLowerCase()}`));
      }
    }
    const unique = [...new Set(variants)].filter(Boolean).sort((a, b) => b.length - a.length);
    entries.push({ name, variants: unique, shellValue: toShellPath(primary) });
  };

  add("WG_DATA", local, localizedLocal, "webagent", "data");
  add("WG_BRIDGE", local, localizedLocal, "webagent", "bridge");
  add("WG", local, localizedLocal, "webagent");
  add("PI_SESS", agent, "", "sessions");
  add("PI", agent);
  add("REPO", cwd);
  add("LOCAL", local, localizedLocal);
  add("TMP", tmp);
  add("HOME", home);

  return entries.sort((a, b) => {
    const len = (entry) => Math.max(0, ...entry.variants.map((variant) => variant.length));
    return len(b) - len(a);
  });
}

export function shortenPaths(text, aliases) {
  let out = text;
  for (const alias of aliases) {
    for (const variant of alias.variants) {
      const escaped = variant.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      out = out.replace(new RegExp(escaped, "gi"), () => `$${alias.name}`);
    }
  }
  return out;
}

/** Kollabiert Pfadsegmente, die dreimal oder oefter hintereinander stehen (Junction-Loops). */
export function collapseRepeats(line) {
  return line.replace(/([\\/])([^\\/:]+)(?:\1\2){2,}/g, (full, separator, segment) => {
    const count = full.split(separator).filter(Boolean).length;
    return `${separator}${segment}×${count}`;
  });
}

export function dedupeLines(text) {
  const lines = text.split(/\r?\n/);
  const counts = new Map();
  for (const line of lines) counts.set(line, (counts.get(line) ?? 0) + 1);
  const seen = new Set();
  const unique = [];
  for (const line of lines) {
    if (seen.has(line)) continue;
    seen.add(line);
    unique.push(counts.get(line) > 1 ? `${line}  [×${counts.get(line)}]` : line);
  }
  const duplicates = [...counts.values()].reduce((sum, n) => sum + Math.max(0, n - 1), 0);
  return { text: unique.join("\n"), duplicates };
}

export function compactOutput(text, aliases, env = process.env) {
  const shortened = shortenPaths(text, aliases);
  const collapsed = shortened.split(/\r?\n/).map(collapseRepeats).join("\n");
  const deduped = dedupeLines(collapsed);
  const limits = compactLimits(env);
  const allLines = deduped.text.split(/\r?\n/);
  const totalLines = collapsed.split(/\r?\n/).length;
  const totalBytes = Buffer.byteLength(collapsed, "utf8");
  let outputText = deduped.text;

  if (allLines.length > limits.maxLines || Buffer.byteLength(outputText, "utf8") > limits.maxBytes) {
    const omitted = Math.max(0, allLines.length - limits.head - limits.tail);
    const marker = `… [compact-bash: ${omitted} Zeilen ausgelassen; Kopf ${limits.head}, Ende ${limits.tail}]`;
    outputText = [...allLines.slice(0, limits.head), marker, ...allLines.slice(-limits.tail)].join("\n");
  }

  const outputBytes = Buffer.byteLength(outputText, "utf8");
  return {
    text: outputText,
    fullText: deduped.text,
    stats: {
      changed: outputText !== text,
      capped: outputText !== deduped.text,
      duplicates: deduped.duplicates,
      totalLines,
      outputLines: outputText.split(/\r?\n/).length,
      totalBytes,
      outputBytes,
    },
  };
}

/** Exponiert die Registry im Shell-Prozess, damit $VAR in Befehlen direkt funktioniert. */
export function makeSpawnEnv(env, aliases) {
  const out = { ...env, COMPACT_BASH: "1" };
  for (const alias of aliases) out[alias.name] = alias.shellValue;
  return out;
}
