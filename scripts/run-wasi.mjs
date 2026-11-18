import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { WASI } from "node:wasi";

const [modulePath, ...args] = process.argv.slice(2);
if (!modulePath) {
  console.error("Usage: node scripts/run-wasi.mjs <program.wasm> [args...]");
  process.exit(2);
}

const wasi = new WASI({
  version: "preview1",
  args: [modulePath, ...args],
  preopens: { examples: fileURLToPath(new URL("../examples/", import.meta.url)) },
  returnOnExit: true,
});
const { instance } = await WebAssembly.instantiate(
  await readFile(modulePath),
  wasi.getImportObject(),
);
process.exitCode = wasi.start(instance);
