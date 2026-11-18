// The hand-written TypeScript declarations (src/lib.rs) describe the documents that the
// module really returns: every key of a report is declared, and every declared required
// key is present.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import test from "node:test";

import { check, solve } from "../npm/node/caekit_wasm.mjs";
import { examples, npmDir } from "./helpers.mjs";

const declarations = readFileSync(path.join(npmDir, "web", "caekit_wasm.d.ts"), "utf8");

/** Keys of `export interface <name> { ... }`, split into required and optional. */
function declaredKeys(name) {
  const match = new RegExp(`export interface ${name}(?: extends [^{]+)? \\{([^}]*)\\}`).exec(declarations);
  assert.ok(match, `interface ${name} is declared`);
  const required = [];
  const optional = [];
  for (const [, key, mark] of match[1].matchAll(/^\s*([a-z_0-9]+)(\??):/gm)) {
    (mark ? optional : required).push(key);
  }
  return { required, optional };
}

function assertShape(value, name, label) {
  const { required, optional } = declaredKeys(name);
  const keys = Object.keys(value);
  for (const key of required) assert.ok(keys.includes(key), `${label}: ${name}.${key} is missing`);
  for (const key of keys) {
    assert.ok(required.includes(key) || optional.includes(key), `${label}: ${name}.${key} is not declared`);
  }
}

const quantityKinds = [...declarations.matchAll(/\{ kind: "([a-z_]+)";/g)].map((match) => match[1]);
const stopReasons = /export type StopReason = ([^;]+);/.exec(declarations)[1].match(/[a-z_]+/g);
const locations = /export type SampleLocation = ([^;]+);/.exec(declarations)[1].match(/[a-z_]+/g);

test("both builds ship the same declarations of the API", () => {
  const node = readFileSync(path.join(npmDir, "node", "caekit_wasm.d.ts"), "utf8");
  for (const signature of ["export function check(model: ModelInput): ModelCheck;", "export function solve(model: ModelInput): RunReport;", "export function version(): string;"]) {
    assert.ok(declarations.includes(signature), `web: ${signature}`);
    assert.ok(node.includes(signature), `node: ${signature}`);
  }
  assert.deepEqual(quantityKinds, ["scalar", "field", "vector_field", "tensor_field", "by_region", "by_interface"]);
});

for (const { name, text } of examples) {
  test(`declarations cover the report of examples/${name}.json`, () => {
    const checked = check(text);
    assertShape(checked, "ModelCheck", name);
    for (const mesh of checked.meshes) assertShape(mesh, "MeshMetrics", name);

    const report = solve(text);
    assertShape(report, "RunReport", name);
    assertShape(report.execution, "Execution", name);
    assert.ok(stopReasons.includes(report.stop_reason));
    for (const geometry of report.geometry) assertShape(geometry, "GeometryRevision", name);
    for (const mesh of report.meshes) assertShape(mesh, "MeshMetrics", name);
    for (const stage of report.convergence) {
      assertShape(stage, "Convergence", name);
      assert.ok(stopReasons.includes(stage.stop_reason));
    }
    for (const balance of report.balances) assertShape(balance, "Balance", name);
    for (const [output, quantity] of Object.entries(report.quantities)) {
      assert.ok(quantityKinds.includes(quantity.kind), `${name}: ${output} has kind ${quantity.kind}`);
      if ("location" in quantity) assert.ok(locations.includes(quantity.location), `${name}: ${output}`);
      if (quantity.kind === "by_interface") {
        for (const value of quantity.values) assertShape(value, "InterfaceValue", name);
      }
    }
  });
}
