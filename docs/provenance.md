# Source and data provenance

- Project license terms are in [LICENSE-MIT](../LICENSE-MIT) and
  [LICENSE-APACHE](../LICENSE-APACHE). The latter is the
  [Apache Software Foundation text](https://www.apache.org/licenses/LICENSE-2.0.txt).
- The Rust dependencies are obtained unmodified from crates.io. The versions
  and checksums are in [Cargo.lock](../Cargo.lock). The direct dependencies are
  [Serde](https://github.com/serde-rs/serde), [Serde JSON](https://github.com/serde-rs/json)
  and [RustCrypto SHA-2](https://github.com/RustCrypto/hashes), each available
  under MIT or Apache-2.0. The locked graph includes the corresponding
  permissive dependencies, including the Unicode-3.0 terms applicable to
  `unicode-ident`. Inspect the exact resolved licenses and upstream
  repositories with `cargo metadata --locked --format-version 1`.
- The axial-bar properties and loads are synthetic inputs specified for the
  analytical fixture. They fall under the project license. The example uses
  a representative constant elastic modulus independent of any measured
  material curve.
- The parallel-plate channel example uses synthetic constant liquid properties
  and geometry under the project license. Its expected pressure loss and
  velocity profile follow from the analytical momentum equation.
- The cooling channel uses synthetic wall conductivity, fluid heat capacity
  and an assumed film coefficient under the project license. The analytic
  reference is a continuous thermal resistance and bulk energy balance.
