# CAEKit

A Rust project for open-source multiphysics engineering simulation.

## Run a reference case

Install Rust 1.85 or newer, then run the model through the CLI.

```sh
cargo run --locked -- check examples/axial_bar.json
cargo run --locked -- run examples/axial_bar.json
cargo run --locked -- run examples/laminar_channel.json
cargo run --locked -- run examples/cooling_channel.json
cargo run --locked -- run examples/elastic_solid.json
cargo run --locked -- check examples/heat_conduction.json
cargo run --locked -- run examples/heat_conduction.json
cargo run --locked -- check examples/thermoelastic_solid.json
cargo run --locked -- run examples/thermoelastic_solid.json
```

The [axial bar](docs/physics/axial-bar.md) example returns a 0.05 mm extension
for a 1 kN load. The JSON report includes displacements, stresses, reactions,
the force balance and linear convergence information. The Rust library exposes
the same model reader and solver through `Model::from_json` and `solve`.

The [laminar channel](docs/physics/laminar-channel.md) example solves a
fully developed volume-driven flow and reports pressure loss, the velocity
profile and balance residuals. The [cooling channel](docs/physics/cooling-channel.md)
exchanges heat between independently meshed wall and coolant domains and
reports the outlet temperature and interface and system energy balances. The
[elastic solid](docs/physics/elastic-solid.md) computes displacements, stress
tensors and reactions on a three-dimensional tetrahedral mesh.

The steady heat conduction example reports a center temperature of 350 K,
a heat flux of 20,000 W/m² along +x and boundary heat rates of +200 W and
−200 W. Positive boundary power enters the solid. [Heat conduction](docs/physics/heat-conduction.md)

The thermoelastic example transfers the computed 350 K field into a
small-strain mechanical solve on the same tetrahedra. With axial extension
restrained and lateral expansion free, the heated bar develops −120 MPa
axial stress and opposing 12 kN support reactions. [Thermoelasticity](docs/physics/thermoelastic-solid.md)

See the [model and result format](docs/model.md) and
[source and data provenance](docs/provenance.md).

## Cargo workspace

The root `caekit` package exposes the versioned model, physics dispatch,
`solve`, and run reports. Its domain solvers depend on shared lower-level
crates. The command-line package uses the same public Rust API.

| Package | Responsibility |
| --- | --- |
| `caekit-cli` | The `caekit` executable, argument handling, and JSON I/O |
| `caekit-mechanics` | Axial bar, three-dimensional elasticity and thermal expansion |
| `caekit-fluids` | Laminar momentum, mass flow, and pressure recovery |
| `caekit-thermal` | Tetrahedral solid conduction, wall conduction and coolant energy transport |
| `caekit-mesh` | Meshes, geometry revisions, regions, and mesh quality |
| `caekit-numerics` | Sparse systems, iterative solves, and convergence histories |
| `caekit-types` | Shared physical input and result values and diagnostics |

The root cooling case passes mass flow from the fluid solver to the thermal
solver. The root thermoelastic case passes nodal temperatures from the thermal
solver to mechanics. Build domain crates by package name. The workspace
selects the API and CLI packages by default at the repository root.
[Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html)

```sh
cargo test --locked -p caekit-numerics
cargo install --locked --path crates/caekit-cli
```

## Reproduce CI locally

**Verify CAEKit** runs on pull requests and pushes to `master`. Native tests
run the entire workspace, including doctests and the CLI, on Linux, macOS and
Windows with stable Rust. The Linux MSRV job reads the minimum Rust version
from `Cargo.toml`, currently **1.85.0**. For branch protection, select the
**Required CI** status check, which requires every verification job to succeed.

From the repository root, use `rustup` to run the native checks. The last two
commands reproduce the separate Rustfmt and Clippy jobs.

```sh
rustup toolchain install stable --profile minimal --component rustfmt,clippy
rustup toolchain install 1.85.0 --profile minimal
cargo +stable test --workspace --locked
cargo +1.85.0 test --workspace --locked
cargo +stable fmt --all -- --check
cargo +stable clippy --workspace --all-targets --locked -- -D warnings
```

### WebAssembly

CI compiles the public library for `wasm32-unknown-unknown` and runs the
physics and numerical tests for `wasm32-wasip1` under **Node.js 24.x** with
WASI Preview 1. The WASI job launches the compiled CLI on all six saved
models. The native workspace job covers the CLI tests that spawn subprocesses.

Use these commands from the repository root in a Linux shell with Node.js 24.x
on `PATH`.

```sh
rustup target add --toolchain stable wasm32-unknown-unknown wasm32-wasip1
cargo +stable build --lib -p caekit --target wasm32-unknown-unknown --release --locked
export CARGO_TARGET_WASM32_WASIP1_RUNNER="node $PWD/scripts/run-wasi.mjs"
cargo +stable test --workspace --exclude caekit-cli --target wasm32-wasip1 --locked --tests
cargo +stable build --locked -p caekit-cli
cargo +stable build --locked -p caekit-cli --target wasm32-wasip1
node scripts/verify-reference-cases.mjs target/debug/caekit target/wasm32-wasip1/debug/caekit.wasm
```

The reference check reports CLI exit errors and verifies convergence, the
analytical benchmarks, model identity, requested outputs, units and sample
positions. Numeric comparisons use a unit-specific absolute tolerance near
zero and a relative tolerance of `1e-10` at larger magnitudes. Failed
comparisons identify the model, quantity, expected value and measured value.

## License

Choose either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE).
