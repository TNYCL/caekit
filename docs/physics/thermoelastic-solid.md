# Three-dimensional linear thermoelasticity

The `thermoelastic_solid` case solves steady heat conduction and then
small-strain mechanical equilibrium on the same tetrahedral mesh. It uses
constant isotropic properties and one-way coupling from temperature to
mechanics. The heat stage supplies temperatures in the original node order
and both stages use the same checked topology and geometry revision.

```sh
cargo run --locked -- check examples/thermoelastic_solid.json
cargo run --locked -- run examples/thermoelastic_solid.json
cargo test --locked --test thermoelastic_solid -- --nocapture
```

The Rust API uses the same versioned input and report as the CLI.

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json = std::fs::read("examples/thermoelastic_solid.json")?;
    let model = caekit::Model::from_json(&json)?;
    let report = caekit::solve(&model)?;
    assert_eq!(report.stop_reason, caekit::StopReason::Converged);
    println!("{:?}", report.quantities[&caekit::Output::CauchyStress]);
    Ok(())
}
```

## Model and stages

- `physics.geometry` and `physics.mesh` define the shared solid.
- `physics.thermal` uses the independent conduction case's thermal material,
  optional volumetric source and exterior thermal conditions. [Heat conduction](heat-conduction.md)
- `physics.mechanical` takes the elastic material and the existing surface
  tractions, pressures, partial displacements and optional `body_force_n_m3`.
  The displacement constraints must remove all six rigid-body modes.
  [Elastic solid](elastic-solid.md)
- `physics.expansion` specifies the stress-free `reference_temperature_k`
  in K and a finite, signed `linear_expansion_coefficient_per_k` in 1/K.
  Use the mechanical material's source to cite the elastic and expansion
  properties.

The top-level `linear_solver` sets the mechanical tolerance in N and
`physics.thermal_solver` sets the thermal tolerance in W. Each accepts
`relative_residual_tolerance`, `absolute_residual_tolerance` and
`max_iterations` and defaults independently. The trace shows the stage's
stop reason, iteration count, residual threshold and recomputed residual
norm. An unconverged heat stage ends the combined solve before equilibrium.
An unconverged equilibrium ends the run with both traces. Iteration-limit
reports have exit code 2 and carry the executed stages without physical
fields or balances. [Model and report format](../model.md)

## Formulation and outputs

At temperature `T`, the isotropic free strain is
`ε_th = α(T − T_ref) I`. Stress is `σ = C : [ε(u) − ε_th]`.
The stiffness uses the same linear elastic tensor as the standalone solid.
The thermal load adds `∫ ε(v):C:ε_th dV` to the mechanical right-hand
side. A tetrahedron's linear temperature gives an exact centroid mean
for this integral and for its internal force. Reactions are the resulting
internal forces minus external mechanical loads on constrained components.
Positive reaction components act on the solid along the global coordinate
directions. Force and moment balances use external mechanical loads and
these support reactions, with moments taken about the first mesh vertex.

The stored elastic energy is `½ ∫ ε_el:C:ε_el dV`. For linear temperature
inside a cell, the squared strain can vary quadratically. CAEKit integrates
the mean elastic-strain contribution plus the exact contribution of the
temperature variation. The latter uses the thermal-strain variance
`Σ_i (α(T_i − T_ref) − mean)^2/20` times `9 K_bulk V/2`.

| Output | Sampling | Unit |
| --- | --- | --- |
| `solid_temperature` | Vertex, absolute temperature | K |
| `solid_heat_flux` | Cell centroid, global vector | W/m² |
| `boundary_heat_rate` | Exterior region, positive into the solid | W |
| `displacement_vector` | Vertex, global vector | m |
| `nodal_reaction` | Vertex, global vector with zero on free components | N |
| `cauchy_stress` | Cell centroid, global tensor with tension positive | Pa |
| `von_mises_stress` | Cell centroid | Pa |
| `elastic_energy` | Integral over the solid | J |

## Analytical checks

- **Free expansion.** A 1 m bar heated by 50 K with
  `α = 12 × 10⁻⁶/K` extends by 0.60 mm. On the three coordinate planes
  through the origin, the respective normal displacements are fixed and
  the tangential components can slide. The affine displacement
  `u = α ΔT [x,y,z]` is compatible with those supports. Stress, support
  reaction and elastic energy vanish.
- **Axially restrained expansion.** Adding axial restraint at the other
  end leaves transverse expansion free. With `E = 200 GPa`,
  `ν = 0.3` and a 100 mm² cross section, the example has
  `σ_xx = −E α ΔT = −120 MPa`. The left and right supports exert
  +12 kN and −12 kN in the x direction and the elastic energy is 3.6 J.
  The lateral strain is `(1 + ν) α ΔT`.
- **Temperature gradient.** A 1 m block with sliding side restraints has
  `T − T_ref = g x`, `g = 50 K/m` and zero axial displacement at
  `x = 0`. Equilibrium and the free far end give `σ_xx = 0` and
  `u_x = [(1 + ν)/(1 − ν)] α g x²/2`. The tests sample inside every
  tetrahedron and at the interior sections `x = 0.25, 0.5, 0.75 m`.

The two affine mechanical checks use 5e-12 m for displacement, 0.05 Pa for
stress and 5e-6 N for support forces. The absolute allowances cover linear
solver and roundoff error on the 0.60 mm, 120 MPa and 12 kN scales, including
components expected to vanish. The free and restrained energy tolerances
are 1e-10 J and 1e-8 J against the 3.6 J restrained scale.

For the temperature gradient, the measured interior RMS displacement errors
for 2, 4 and 8 divisions per axis on Linux were 24.00, 6.53 and 1.71 μm. The
test requires at least a factor of 2.7 per halving of spacing, with a
5 μm finest-grid tolerance at the internal sections and on the interior RMS
error. This allows discretization error while resolving the quadratic
profile against the analytical 557 μm tip displacement. The single-cell
energy check uses a fully restrained unit tetrahedron with the same
temperature gradient and verifies 4,500 J to 1e-8 J against the analytical
integral `∫ x² dV = 1/60`. Setting `α = 0` or `T = T_ref` recovers the
standalone mechanical quantities and balances.

The examples use synthetic geometry and materials under the project
license. Their analytical checks follow from heat conduction and isotropic
elastic equilibrium. [Provenance](../provenance.md)
