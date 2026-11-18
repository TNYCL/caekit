# CAEKit

A Rust project for open-source multiphysics engineering simulation.

## Run a reference case

Install Rust 1.85 or newer, then run the model through the CLI.

```sh
cargo run --locked -- check examples/axial_bar.json
cargo run --locked -- run examples/axial_bar.json
cargo run --locked -- run examples/laminar_channel.json
cargo run --locked -- run examples/cooling_channel.json
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
reports the outlet temperature and interface and system energy balances.

See the [model and result format](docs/model.md) and
[source and data provenance](docs/provenance.md).

## License

Choose either the [MIT license](LICENSE-MIT) or the
[Apache License, Version 2.0](LICENSE-APACHE).
