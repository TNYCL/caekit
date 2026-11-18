// Shared by the tests of the built package in crates/caekit-wasm/npm (node build.mjs).
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const root = fileURLToPath(new URL("../../../", import.meta.url));
export const npmDir = fileURLToPath(new URL("../npm/", import.meta.url));

const examplesDir = path.join(root, "examples");

/** Every saved reference model: its name, JSON text and parsed document. */
export const examples = readdirSync(examplesDir)
  .filter((name) => name.endsWith(".json"))
  .sort()
  .map((name) => {
    const text = readFileSync(path.join(examplesDir, name), "utf8");
    return { name: path.basename(name, ".json"), text, model: JSON.parse(text) };
  });

export function example(name) {
  const found = examples.find((entry) => entry.name === name);
  if (!found) throw new Error(`examples/${name}.json is missing`);
  return found;
}
