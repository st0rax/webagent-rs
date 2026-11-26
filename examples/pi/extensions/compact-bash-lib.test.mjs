import assert from "node:assert/strict";
import test from "node:test";
import {
  buildAliases,
  collapseRepeats,
  compactLimits,
  compactOutput,
  dedupeLines,
  makeSpawnEnv,
  shortenPaths,
} from "./compact-bash-lib.mjs";

const env = {
  USERPROFILE: "C:\\Users\\storax",
  LOCALAPPDATA: "C:\\Users\\storax\\AppData\\Local",
  PI_CODING_AGENT_DIR: "C:\\Users\\storax\\.pi\\agent",
  TMP: "C:\\Users\\storax\\AppData\\Local\\Temp",
};
const aliases = buildAliases(env, "C:\\Users\\storax\\Documents\\Standardprojekt\\repo");

test("Kurzpfad ersetzt Windows-, Git-Bash- und lokalisierte Pfade", () => {
  const input = [
    "C:\\Users\\storax\\AppData\\Local\\webagent\\data\\brain_score\\events.jsonl",
    "/c/Users/storax/AppData/Local/webagent/bridge/bridge.err.log",
    "C:/Users/storax/Lokale Einstellungen/Anwendungsdaten/webagent/data/x.json",
    "C:\\Users\\storax\\.pi\\agent\\sessions\\a.jsonl",
  ].join("\n");
  const out = shortenPaths(input, aliases);
  assert.match(out, /\$WG_DATA\\brain_score\\events\.jsonl/);
  assert.match(out, /\$WG_BRIDGE\/bridge\.err\.log/);
  assert.match(out, /\$WG_DATA\/x\.json/);
  assert.match(out, /\$PI_SESS\\a\.jsonl/);
  assert.doesNotMatch(out, /storax/);
});

test("laengste Variante gewinnt: WG_DATA nicht zu WG oder HOME gekuerzt", () => {
  const out = shortenPaths(
    "C:\\Users\\storax\\AppData\\Local\\webagent\\data\\a\nC:\\Users\\storax\\x",
    aliases,
  );
  assert.ok(out.startsWith("$WG_DATA\\a"));
  assert.ok(out.includes("$HOME\\x"));
});

test("Junction-Loop wird kollabiert", () => {
  const line =
    "C:/Users/storax/Lokale Einstellungen/Anwendungsdaten/Anwendungsdaten/Anwendungsdaten/Anwendungsdaten/x.py";
  const out = collapseRepeats(line);
  assert.match(out, /Anwendungsdaten×4/);
  assert.equal(out.includes("Anwendungsdaten/Anwendungsdaten/Anwendungsdaten/"), false);
});

test("identische Zeilen werden zu [×N] zusammengefasst", () => {
  const { text, duplicates } = dedupeLines("a\nb\na\na\n");
  assert.equal(duplicates, 2);
  assert.equal(text, "a  [×3]\nb\n");
});

test("compactOutput kapt und behaelt die volle Fassung", () => {
  const smallEnv = { ...env, COMPACT_BASH_MAX_LINES: "10", COMPACT_BASH_HEAD: "2", COMPACT_BASH_TAIL: "2" };
  const input = Array.from({ length: 40 }, (_, i) => `Zeile ${i}`).join("\n");
  const result = compactOutput(input, aliases, smallEnv);
  assert.equal(result.stats.capped, true);
  assert.equal(result.stats.totalLines, 40);
  assert.match(result.text, /compact-bash: 36 Zeilen ausgelassen/);
  assert.equal(result.fullText.split("\n").length, 40);
  assert.ok(result.text.includes("Zeile 39"));
});

test("ungueltige Grenzen fallen auf Defaults zurueck", () => {
  const limits = compactLimits({ ...env, COMPACT_BASH_MAX_BYTES: "0", COMPACT_BASH_MAX_LINES: "nope" });
  assert.equal(limits.maxBytes, 16 * 1024);
  assert.equal(limits.maxLines, 200);
});

test("Registry wird als Shell-Variablen exportiert", () => {
  const spawned = makeSpawnEnv({ PATH: "/bin" }, aliases);
  assert.equal(spawned.PATH, "/bin");
  assert.equal(spawned.WG_DATA, "/c/Users/storax/AppData/Local/webagent/data");
  assert.equal(spawned.REPO, "/c/Users/storax/Documents/Standardprojekt/repo");
  assert.equal(spawned.COMPACT_BASH, "1");
});
