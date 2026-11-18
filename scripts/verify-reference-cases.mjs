import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const [nativePath, wasiPath] = process.argv.slice(2);
if (!nativePath || !wasiPath || process.argv.length !== 4) {
  console.error("Usage: node scripts/verify-reference-cases.mjs <caekit> <caekit.wasm>");
  process.exit(2);
}

const root = fileURLToPath(new URL("../", import.meta.url));
const runner = fileURLToPath(new URL("./run-wasi.mjs", import.meta.url));

// Floors for differences near zero. Elsewhere allow 1e-10 relative to native.
const absoluteTolerance = new Map([
  ["m", 1e-13], ["Pa", 1e-7], ["N", 1e-8], ["N m", 1e-8],
  ["m/s", 1e-12], ["m^3/s", 1e-18], ["kg/s", 1e-15], ["1", 1e-11],
  ["K", 1e-9], ["W", 1e-8], ["W/m^2", 1e-7], ["J", 1e-8],
]);

function close(actual, expected, tolerance, label) {
  assert.ok(
    Number.isFinite(actual) && Number.isFinite(expected) &&
      Math.abs(actual - expected) <= tolerance,
    `${label}: expected ${expected} ± ${tolerance}, measured ${actual}`,
  );
}

function compareNumbers(actual, expected, floor, relative, label) {
  if (Array.isArray(expected)) {
    assert.ok(Array.isArray(actual), `${label}: expected an array, measured ${actual}`);
    assert.equal(actual.length, expected.length, `${label}: array length`);
    for (const [index, value] of expected.entries()) {
      compareNumbers(actual[index], value, floor, relative, `${label}[${index}]`);
    }
  } else {
    close(actual, expected, Math.max(floor, relative * Math.abs(expected)), label);
  }
}

function compareNamedNumbers(actual, expected, floor, label) {
  assert.deepEqual(Object.keys(actual).sort(), Object.keys(expected).sort(), `${label}: keys`);
  for (const [name, value] of Object.entries(expected)) {
    compareNumbers(actual[name], value, floor, 1e-10, `${label}.${name}`);
  }
}

function comparePhysics(wasi, native, model, label) {
  assert.equal(wasi.model_sha256, native.model_sha256, `${label}: model_sha256`);
  assert.equal(wasi.method, native.method, `${label}: solver method`);
  assert.deepEqual(wasi.geometry, native.geometry, `${label}: geometry revisions`);
  for (const key of model.outputs) {
    const actual = wasi.quantities[key];
    const expected = native.quantities[key];
    const quantity = `${label}: ${key}`;
    assert.equal(actual.kind, expected.kind, `${quantity}: kind`);
    assert.equal(actual.unit, expected.unit, `${quantity}: unit`);
    const floor = absoluteTolerance.get(expected.unit);
    assert.ok(floor, `${quantity}: missing tolerance for unit ${expected.unit}`);
    const metric = `${quantity} (${expected.unit})`;
    switch (expected.kind) {
      case "scalar":
        compareNumbers(actual.value, expected.value, floor, 1e-10, metric);
        break;
      case "by_region":
        compareNamedNumbers(actual.values, expected.values, floor, metric);
        break;
      case "field":
      case "vector_field":
      case "tensor_field":
        assert.equal(actual.location, expected.location, `${quantity}: sample location`);
        for (const field of [actual, expected]) {
          assert.ok(field.positions_m.length > 0, `${quantity}: no sample positions`);
          assert.equal(field.values.length, field.positions_m.length, `${quantity}: sample count`);
        }
        compareNumbers(actual.positions_m, expected.positions_m, 1e-12, 0, `${quantity}: positions (m)`);
        compareNumbers(actual.values, expected.values, floor, 1e-10, metric);
        break;
      default:
        assert.fail(`${quantity}: unsupported kind ${expected.kind}`);
    }
  }

  const balanceNames = (report) => report.balances.map(({ name }) => name).sort();
  assert.deepEqual(balanceNames(wasi), balanceNames(native), `${label}: balance names`);
  for (const expected of native.balances) {
    const actual = wasi.balances.find(({ name }) => name === expected.name);
    const balance = `${label}: ${expected.name}`;
    assert.equal(actual.unit, expected.unit, `${balance}: unit`);
    const floor = absoluteTolerance.get(expected.unit);
    assert.ok(floor, `${balance}: missing tolerance for unit ${expected.unit}`);
    const metric = `${balance} (${expected.unit})`;
    compareNamedNumbers(actual.terms, expected.terms, floor, metric);
    compareNumbers(actual.imbalance, expected.imbalance, floor, 1e-10, `${metric}: imbalance`);
  }
}

function run(executable, args, label) {
  const result = spawnSync(executable, args, { cwd: root, encoding: "utf8" });
  if (result.error) {
    throw new Error(`${label}: ${result.error.message}`, { cause: result.error });
  }
  assert.equal(
    result.status,
    0,
    `${label}: expected exit 0, measured ${result.signal ?? result.status}\n${result.stderr.trim()}`,
  );
  try {
    return JSON.parse(result.stdout);
  } catch (error) {
    throw new Error(`${label}: invalid JSON report: ${error.message}`, { cause: error });
  }
}

function verifyReport(report, model, backend, stageCount, label) {
  assert.equal(report.schema_version, 1, `${label}: report schema version`);
  assert.equal(report.model_schema_version, model.schema_version, `${label}: model schema version`);
  assert.equal(report.model_name, model.name, `${label}: model name`);
  assert.match(report.model_sha256, /^[0-9a-f]{64}$/, `${label}: model SHA-256`);
  assert.equal(report.execution.backend, backend, `${label}: execution backend`);
  assert.equal(report.execution.precision, "ieee754_binary64", `${label}: execution precision`);
  assert.equal(report.stop_reason, "converged", `${label}: run stop reason`);
  assert.equal(report.convergence.length, stageCount, `${label}: number of solver stages`);
  for (const stage of report.convergence) {
    const { final_true_residual_l2: residual, residual_threshold: threshold } = stage;
    const description = `${label}: ${stage.stage}`;
    assert.equal(stage.stop_reason, "converged", `${description}: stop reason`);
    assert.ok(
      Number.isFinite(threshold) && Number.isFinite(residual) && residual >= 0 && residual <= threshold,
      `${description}: expected final residual ≤ ${threshold} ${stage.residual_unit}, measured ${residual}`,
    );
  }
  assert.deepEqual(Object.keys(report.quantities).sort(), [...model.outputs].sort(), `${label}: requested outputs`);
}

function referenceQuantity(report, name, unit, label) {
  const quantity = report.quantities[name];
  assert.equal(quantity?.unit, unit, `${label}: ${name} unit`);
  return quantity;
}

function referenceField(report, name, unit, kind, location, label) {
  const field = referenceQuantity(report, name, unit, label);
  assert.equal(field.kind, kind, `${label}: ${name} kind`);
  assert.equal(field.location, location, `${label}: ${name} sample location`);
  return field;
}

function referenceBalance(report, name, unit, label) {
  const balance = report.balances.find((entry) => entry.name === name);
  assert.equal(balance?.unit, unit, `${label}: ${name} balance unit`);
  return balance;
}

// Use the analytical references and acceptance limits in crates/caekit/tests/<case>.rs.
const references = {
  axial_bar(report, label) {
    const tip = referenceQuantity(report, "end_displacement", "m", label).values?.tip;
    close(tip, 5e-5, 1e-13, `${label}: end_displacement.tip (m)`);
    return `tip extension ${tip} m`;
  },
  elastic_solid(report, label) {
    const energy = referenceQuantity(report, "elastic_energy", "J", label).value;
    const reaction = referenceBalance(report, "solid_force_x", "N", label).terms.reaction;
    close(energy, 100, 1e-6, `${label}: elastic_energy (J)`);
    close(reaction, -2e6, 1e-3, `${label}: solid_force_x.reaction (N)`);
    return `strain energy ${energy} J, x reaction ${reaction} N`;
  },
  laminar_channel(report, label) {
    const drop = referenceQuantity(report, "pressure_loss", "Pa", label).value;
    close(drop, 2.4, 0.005, `${label}: pressure_loss (Pa)`);
    return `pressure loss ${drop} Pa`;
  },
  cooling_channel(report, label) {
    const outlet = referenceQuantity(report, "outlet_temperature", "K", label).value;
    const heat = referenceQuantity(report, "heat_transfer_rate", "W", label).value;
    const resistance = 0.004 / 15 + 1 / 400;
    const ntu = (2 * 0.1 / resistance) / (0.01 * 4180);
    close(outlet, 360 - (360 - 298) * Math.exp(-ntu), 0.5, `${label}: outlet_temperature (K)`);
    close(heat, 0.01 * 4180 * (outlet - 298), 1e-6, `${label}: heat_transfer_rate (W)`);
    for (const name of ["wall_energy", "interface_energy", "coolant_energy", "system_energy"]) {
      const { imbalance } = referenceBalance(report, name, "W", label);
      close(imbalance, 0, 1e-6, `${label}: ${name}.imbalance (W)`);
    }
    return `outlet temperature ${outlet} K, heat transfer ${heat} W`;
  },
  heat_conduction(report, label) {
    assert.equal(report.convergence[0].stage, "solid_heat_conduction");
    assert.equal(report.convergence[0].residual_unit, "W");
    const temperature = referenceField(report, "solid_temperature", "K", "field", "node", label);
    for (const [index, [x]] of temperature.positions_m.entries()) {
      close(temperature.values[index], 400 - 1000 * x, 1e-8, `${label}: temperature[${index}] (K)`);
    }
    const midpoint = temperature.positions_m.findIndex((point) => point.every((x) => x === 0.05));
    const centerTemperature = temperature.values[midpoint];
    close(centerTemperature, 350, 1e-8, `${label}: center temperature (K)`);
    const flux = referenceField(report, "solid_heat_flux", "W/m^2", "vector_field", "cell", label);
    for (const [index, value] of flux.values.entries()) {
      compareNumbers(value, [20_000, 0, 0], 1e-6, 0, `${label}: flux[${index}] (W/m^2)`);
    }
    const heat = referenceQuantity(report, "boundary_heat_rate", "W", label).values;
    const expectedHeat = { x0: 200, x1: -200, y0: 0, y1: 0, z0: 0, z1: 0 };
    assert.deepEqual(Object.keys(heat).sort(), Object.keys(expectedHeat).sort());
    for (const [region, expected] of Object.entries(expectedHeat)) {
      close(heat[region], expected, 1e-8, `${label}: boundary heat rate ${region} (W)`);
    }
    const { imbalance } = referenceBalance(report, "solid_energy", "W", label);
    close(imbalance, 0, 1e-8, `${label}: solid energy balance (W)`);
    return `center ${centerTemperature} K, x flux ${flux.values[0][0]} W/m^2, boundary heat rates ${heat.x0} and ${heat.x1} W`;
  },
  thermoelastic_solid(report, label) {
    assert.deepEqual(
      report.convergence.map(({ stage, residual_unit }) => [stage, residual_unit]),
      [["solid_heat_conduction", "W"], ["solid_elastic_equilibrium", "N"]],
    );
    const temperature = referenceField(report, "solid_temperature", "K", "field", "node", label);
    for (const [index, value] of temperature.values.entries()) {
      close(value, 350, 1e-8, `${label}: temperature[${index}] (K)`);
    }
    const flux = referenceField(report, "solid_heat_flux", "W/m^2", "vector_field", "cell", label);
    for (const [index, value] of flux.values.entries()) {
      compareNumbers(value, [0, 0, 0], 1e-6, 0, `${label}: flux[${index}] (W/m^2)`);
    }
    const boundary = referenceQuantity(report, "boundary_heat_rate", "W", label);
    for (const [region, power] of Object.entries(boundary.values)) {
      close(power, 0, 1e-8, `${label}: boundary heat rate ${region} (W)`);
    }
    const displacement = referenceField(report, "displacement_vector", "m", "vector_field", "node", label);
    for (const [index, [, y, z]] of displacement.positions_m.entries()) {
      compareNumbers(displacement.values[index], [0, 0.00078 * y, 0.00078 * z], 5e-12, 0,
        `${label}: displacement[${index}] (m)`);
    }
    const stress = referenceField(report, "cauchy_stress", "Pa", "tensor_field", "cell", label);
    for (const [index, value] of stress.values.entries()) {
      compareNumbers(value, [[-120e6, 0, 0], [0, 0, 0], [0, 0, 0]], 0.05, 0,
        `${label}: stress[${index}] (Pa)`);
    }
    const vonMises = referenceField(report, "von_mises_stress", "Pa", "field", "cell", label);
    for (const [index, value] of vonMises.values.entries()) {
      close(value, 120e6, 0.05, `${label}: von Mises stress[${index}] (Pa)`);
    }
    const reaction = referenceField(report, "nodal_reaction", "N", "vector_field", "node", label);
    const endForce = [0, 0];
    for (const [index, [x]] of reaction.positions_m.entries()) {
      assert.ok(x === 0 || x === 1, `${label}: reaction plane ${x} m`);
      endForce[x] += reaction.values[index][0];
    }
    compareNumbers(endForce, [12_000, -12_000], 5e-6, 0, `${label}: end reactions (N)`);
    const energy = referenceQuantity(report, "elastic_energy", "J", label).value;
    close(energy, 3.6, 1e-8, `${label}: elastic energy (J)`);
    for (const [name, unit, tolerance] of [
      ["solid_energy", "W", 1e-8],
      ...["x", "y", "z"].map((axis) => [`solid_force_${axis}`, "N", 5e-6]),
      ...["x", "y", "z"].map((axis) => [`solid_moment_${axis}`, "N m", 5e-6]),
    ]) {
      const { imbalance } = referenceBalance(report, name, unit, label);
      close(imbalance, 0, tolerance, `${label}: ${name} balance (${unit})`);
    }
    return `axial stress ${stress.values[0][0][0]} Pa, end reactions ${endForce.join(" and ")} N, elastic energy ${energy} J`;
  },
};

for (const [name, reference] of Object.entries(references)) {
  const fixture = `examples/${name}.json`;
  test(fixture, (t) => {
    const model = JSON.parse(readFileSync(resolve(root, fixture), "utf8"));
    const stages = ["cooling_channel", "thermoelastic_solid"].includes(name) ? 2 : 1;
    const native = run(resolve(nativePath), ["run", fixture], `${fixture} (native)`);
    const wasi = run(process.execPath, [runner, resolve(wasiPath), "run", fixture], `${fixture} (WASI)`);
    for (const [report, backend, target] of [
      [native, "native_cpu", "native"],
      [wasi, "wasm_cpu", "WASI"],
    ]) {
      const label = `${fixture} (${target})`;
      verifyReport(report, model, backend, stages, label);
      t.diagnostic(`${target}: ${reference(report, label)}`);
    }
    comparePhysics(wasi, native, model, `${fixture} (WASI vs native)`);
  });
}
