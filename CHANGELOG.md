# Changelog

All notable changes to CAEKit are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Every crate and package of the
workspace carries the same version. Before 1.0 a minor version may change the Rust API;
version 1 of the model document keeps its meaning across all of them.

## [Unreleased]

### Added

- Versioned model document (schema version 1) with strict decoding: unknown versions,
  fields, physics kinds and condition kinds are errors that name the path of the field.
- Run report (schema version 1) with the SHA-256 identity of the typed model, mesh
  metrics, execution record, convergence evidence with a recomputed true residual, signed
  conservation balances and the requested quantities with units and sample positions.
- Physics kinds `axial_bar`, `laminar_channel`, `cooling_channel`, `elastic_solid`,
  `heat_conduction`, `multimaterial_heat_conduction` and `thermoelastic_solid`, each
  verified against an analytical reference.
- Rust crates `caekit`, `caekit-mechanics`, `caekit-fluids`, `caekit-thermal`,
  `caekit-mesh`, `caekit-numerics` and `caekit-types`, with documentation of every public
  item.
- `caekit-cli`: the `caekit` command with `check` and `run`, and exit codes 0, 1 and 2 for
  a converged run, an error and a run that stopped without convergence.
- Python package `caekit`: `solve` and `check` for mappings, JSON text, bytes and paths,
  with `InvalidModelError` and `CalculationError`.
- npm package `caekit-wasm` for Node.js and browsers: `solve`, `check` and `version`, with
  TypeScript declarations of the result document.
- Native and WASI builds that agree to a relative 1e-10 on every reference model.

[Unreleased]: https://github.com/caekit/caekit/commits/master
