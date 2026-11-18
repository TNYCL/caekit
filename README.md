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
cargo test --locked
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

See the [model and result format](docs/model.md) and
[source and data provenance](docs/provenance.md).

## Cargo workspace

The root `caekit` package exposes the versioned model, physics dispatch,
`solve`, and run reports. Its domain solvers depend on shared lower-level
crates. The command-line package uses the same public Rust API.

| Package | Responsibility |
| --- | --- |
| `caekit-cli` | The `caekit` executable, argument handling, and JSON I/O |
| `caekit-mechanics` | Axial bar and three-dimensional elastic solid solvers |
| `caekit-fluids` | Laminar momentum, mass flow, and pressure recovery |
| `caekit-thermal` | Wall conduction, bulk energy transport, and interface heat balance |
| `caekit-mesh` | Meshes, geometry revisions, regions, and mesh quality |
| `caekit-numerics` | Sparse systems, iterative solves, and convergence histories |
| `caekit-types` | Shared physical input and result values and diagnostics |

The root cooling case passes mass flow from the fluid solver into the
thermal solver and assembles the combined report. Build domain crates by
package name. The workspace selects the API and CLI packages by default at
the repository root. [Cargo workspace reference](https://doc.rust-lang.org/cargo/reference/workspaces.html)

```sh
cargo test --locked --workspace
cargo test --locked -p caekit-numerics
cargo install --locked --path crates/caekit-cli
```

## Verify WebAssembly results

The physics and numerical integration tests also run as WebAssembly programs
under Node.js with WASI Preview 1. These commands were checked with Node.js 24
and run from the repository root.

```sh
rustup target add wasm32-wasip1
export CARGO_TARGET_WASM32_WASIP1_RUNNER="node $PWD/scripts/run-wasi.mjs"
cargo test --workspace --exclude caekit-cli --target wasm32-wasip1 --locked --tests
cargo run -p caekit-cli --target wasm32-wasip1 --locked -- run examples/elastic_solid.json
```

## License

Choose either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE).
