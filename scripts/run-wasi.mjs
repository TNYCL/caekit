import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { WASI } from "node:wasi";

const [modulePath, ...args] = process.argv.slice(2);
const wasi = new WASI({
  version: "preview1",
  args: [modulePath, ...args],
  preopens: { examples: fileURLToPath(new URL("../examples/", import.meta.url)) },
  returnOnExit: true,
});
const module = await WebAssembly.compile(await readFile(modulePath));
const instance = await WebAssembly.instantiate(module, wasi.getImportObject());
process.exitCode = wasi.start(instance);
