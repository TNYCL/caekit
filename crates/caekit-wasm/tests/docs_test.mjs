// Runs every `js` example of the repository README, of docs/ and of the package README
// under Node, against the built package. An example imports "caekit-wasm" as a user would;
// the test resolves that name to crates/caekit-wasm/npm. Examples run from the repository
// root. `console.assert` does not throw, so anything written to standard error fails the
// test. Browser examples are fenced as `html` and are not run.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";

import { npmDir, root } from "./helpers.mjs";

function markdown(dir) {
  const found = [];
  for (const entry of readdirSync(path.join(root, dir)).sort()) {
    const file = path.posix.join(dir, entry);
    if (statSync(path.join(root, file)).isDirectory()) found.push(...markdown(file));
    else if (entry.endsWith(".md")) found.push(file);
  }
  return found;
}

const pages = ["README.md", "crates/caekit-wasm/README.md", ...markdown("docs")];
const examples = pages.flatMap((page) =>
  [...readFileSync(path.join(root, page), "utf8").replaceAll("\r\n", "\n").matchAll(/^```js\n([\s\S]*?)^```/gm)].map(
    (match, index) => ({ name: `${page}#${index + 1}`, source: match[1] }),
  ),
);

// A project whose node_modules holds the built package under its published name.
const project = mkdtempSync(path.join(tmpdir(), "caekit-wasm-docs-"));
mkdirSync(path.join(project, "node_modules"));
symlinkSync(path.resolve(npmDir), path.join(project, "node_modules", "caekit-wasm"), "junction");
writeFileSync(path.join(project, "package.json"), JSON.stringify({ name: "docs-examples", private: true, type: "module" }));
test.after(() => rmSync(project, { recursive: true, force: true }));

test("the documentation has js examples", () => {
  const withExamples = new Set(examples.map((example) => example.name.split("#")[0]));
  for (const page of ["README.md", "docs/wasm.md", "crates/caekit-wasm/README.md"]) {
    assert.ok(withExamples.has(page), `${page} has a js example`);
  }
});

for (const [index, { name, source }] of examples.entries()) {
  test(`${name} runs`, () => {
    const file = path.join(project, `example-${index + 1}.mjs`);
    writeFileSync(file, source);
    const result = spawnSync(process.execPath, [file], { cwd: root, encoding: "utf8" });
    assert.equal(result.stderr, "", `${name} wrote to standard error`);
    assert.equal(result.status, 0, `${name} exited with ${result.status}`);
    assert.notEqual(result.stdout, "", `${name} prints its result`);
  });
}
