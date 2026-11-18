# Steady heat conduction through multiple materials

Use `physics.kind = "multimaterial_heat_conduction"` to solve steady
isotropic conduction on one conforming tetrahedral mesh. The cells on
either side of an ideal thermal contact share vertices and triangular faces.
Assembling the cell stiffness with its own conductivity gives a single
continuous temperature field whose slope can change at each contact.

```sh
cargo run --locked -- check examples/multimaterial_heat_conduction.json
cargo run --locked -- run examples/multimaterial_heat_conduction.json
```

## Assign materials and sources

Use the same `physics.geometry` and `physics.mesh` format as for
homogeneous conduction. Set the following fields under `physics.thermal`.

| Field | Meaning |
| --- | --- |
| `materials` | List of materials with a unique `id`, positive finite `thermal_conductivity_w_m_k` in W/(m·K), and `source` with citation and usage terms |
| `volume_regions` | List of regions with a unique nonempty `name`, a `material_id` from the material list, and a nonempty list `cells` of zero-based mesh cell indices |
| `volume_regions[].volumetric_heat_source_w_m3` | Uniform generation within that region, defaulting to zero. Positive values generate heat and negative values remove it |
| `boundary_conditions` | Exterior surface temperature, inward flux, convection or insulation. Unassigned exterior surfaces are insulated |

Assign each tetrahedron exactly once. Several regions can use the same
material ID, and one region can contain several disjoint sets of cells.
Names and IDs are case-sensitive. Two different volume regions form an
interface wherever their cells share a face, including when they use the
same material. Boundary conditions resolve names in `mesh.boundary_regions`
and act on exterior faces. [Boundary conditions](heat-conduction.md#boundary-conditions) ·
[Tetrahedral mesh format](elastic-solid.md)

## Read temperatures, fluxes and power

| Output | Meaning |
| --- | --- |
| `solid_temperature` | Absolute temperature in K, in vertex order |
| `solid_heat_flux` | Global three-component vector in W/m² at each tetrahedron centroid, in cell order |
| `boundary_heat_rate` | Integrated W by named exterior surface, positive into the solid |
| `interface_heat_rate` | Integrated W from one volume region to another |

`interface_heat_rate` uses the `by_interface` quantity with a `values`
array. Each entry has `from_region`, `to_region` and `value`. The names
are ordered lexicographically and positive power goes from `from_region`
to `to_region`. The value includes all shared faces for the pair. A
negative value indicates flow in the reverse direction.

The global `solid_energy` balance reports volume sources, exterior flux,
convection and fixed-temperature reactions in W. Each `solid_energy:<name>`
balance gives `volumetric_generation`, `exterior_heat_input` and net
`interface_heat_input` in W. Positive heat input enters the named volume.
`imbalance` is the signed sum of these terms. Each exchange appears once
with each sign, so the regional imbalances sum to the global imbalance
within numerical precision.

```rust,no_run
use caekit::{Model, Output, Quantity, StopReason, solve};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::fs::read("examples/multimaterial_heat_conduction.json")?;
    let report = solve(&Model::from_json(&input)?)?;
    assert_eq!(report.stop_reason, StopReason::Converged);
    if let Quantity::ByInterface { unit, values } = &report.quantities[&Output::InterfaceHeatRate] {
        for transfer in values {
            println!(
                "{} → {} = {} {unit}",
                transfer.from_region, transfer.to_region, transfer.value
            );
        }
    }
    Ok(())
}
```

## Recover integrated power from the discrete solution

The cell output is `q = −k∇T` using the constant gradient of the linear
tetrahedron. The normal components of the two cell fluxes, measured in
a common direction, can differ across a shared face. For each ordered pair
of volume regions, the solver integrates the mean of these two normal
fluxes over the shared faces to obtain an initial power estimate.

Exterior flux and convection are integrated on their triangles and
attributed to the owner cell's volume region. On a fixed-temperature triangle,
the cell's inward normal flux times one third of the area gives a trial
power at each vertex. The solver computes the difference between the nodal
heat equation's reaction and the sum of these trials. It distributes that
difference to the incident fixed-temperature triangles by area. This uses
the conductivity on each side of a fixed vertex while recovering the
total nodal reaction.

The solver corrects the initial interface powers by minimizing
`Σ_e (P_e − P_e_initial)² / A_e`, where `A_e` is the total area of the
pair's faces. The constraint on the net outgoing power of each region
is its internal generation plus exterior input, less the mean imbalance
over all regions. This mean remains in the reported balances because
internal exchange cannot change the total. The Lagrange multiplier
system is a weighted graph Laplacian, with one reference multiplier fixed
and edge weights proportional to interface area. The projection acts
on pair-integrated powers. The reported cell heat flux still comes from
the cell gradient. [Balance-constrained flux postprocessing](https://arxiv.org/abs/1605.04076)

`solid_interface_equilibration` reports the projection's verified free
equation residual in W and uses the same `linear_solver` settings as
`solid_heat_conduction`. If either stage stops without convergence, the
report carries the stages that ran and withholds physical fields and
balances. The CLI exits with code 2. With a single volume region the
interface array is empty and the run has one stage.

## Two-layer reference

Each layer is 0.01 m thick with a cross section of 0.01 m². The hot and
cold ends are at 311 K and 300 K, the sides are insulated and generation
is zero. The conductivities are 10 W/(m·K) on the left and 1 W/(m·K) on
the right. The resistance per unit area is `0.01/10 + 0.01/1 = 0.011 m²·K/W`.
The 11 K difference gives 1,000 W/m² along +x in each layer. The first
layer drops 1 K, giving 310 K at the interface. Native and WASI CLI
checks measured 310 K, 1,000 W/m² and 10 W from left to right. The signed
exterior powers are +10 W at the left end and −10 W at the right. Both
region imbalances and the global imbalance are zero within 1e-8 W.

## Curved profile and refinement

The refinement case uses a cube 0.02 m on each side, with two layers of
the same thickness, conductivities and end temperatures. The left
and right heat sources are 200,000 and 50,000 W/m³. Let `L = 0.01 m` and
`z = x − L`. In each layer `i` the analytical solution is
`q_x = q_I + Q_i z` and `T = T_I − (q_I z + Q_i z²/2)/k_i`. The end
temperatures give
`q_I = [11 + Q_left L²/(2k_left) − Q_right L²/(2k_right)]/(L/k_left + L/k_right)`
and `T_I = 311 − q_I L/k_left + Q_left L²/(2k_left)`. This gives
863.636364 W/m² and 311.136364 K at the interface. The cross section
gives 0.345454545 W of interface transfer, with −0.454545455 W at the left
end and −0.545454545 W at the right. Generation is 0.8 W on the left and
0.2 W on the right. The independently calculated powers agree with
the report at every resolution to 2e-7 W.

Errors are RMS over four interior samples in every equal-volume tetrahedron.
The flux error compares the full vector with the analytical flux on its
own material side.

| Divisions per axis | Tetrahedra | Temperature RMS (K) | Left flux RMS (W/m²) | Right flux RMS (W/m²) | Heat + projection iterations | Solve time (ms) | Peak RSS (KiB) |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 4 | 384 | 0.08504175 | 280.672206 | 73.518557 | 24 + 1 | 0.381 | 5,072 |
| 8 | 3,072 | 0.02118178 | 143.024701 | 36.309127 | 49 + 1 | 2.888 | 6,936 |
| 16 | 24,576 | 0.00528861 | 71.966720 | 18.076959 | 93 + 1 | 26.069 | 22,068 |

Each halving of spacing reduced temperature error by 4.01× and 4.01×,
left flux error by 1.96× and 1.99× and right flux error by 2.02× and
2.01×. The test requires factors above 2.7 and 1.65 respectively, leaving
margin around second-order temperature and first-order gradient accuracy.
The finest-grid tolerances are 0.02 K and 100 and 35 W/m². These are
about 0.18% of the imposed temperature difference and 8.8% and 2.6% of
the peak flux magnitudes on the corresponding sides. The 2e-7 W power
tolerance is relative to 1 W of total generation.

The time and memory figures are the median solve time and largest peak
resident set in five separate release test processes per size, on a Ryzen
9 9900X with Ubuntu 26.04.1 LTS x86-64 and Rust 1.100.0. The time measures
`caekit::solve` including validation and report construction, on one solver
thread. RSS includes the process and case setup. To reproduce on Linux with
GNU time and jq, run the single case per process.

```sh
refinement_test=$(cargo test --locked --release --test multimaterial_heat_conduction \
  --no-run --message-format=json | jq -r 'select(.executable != null).executable')
for divisions in 4 8 16; do
  CAEKIT_REFINEMENT_DIVISIONS="$divisions" /usr/bin/time -f 'Peak RSS %M KiB' \
    "$refinement_test" --ignored --exact measure_sourced_layers --nocapture
done
```

The examples use synthetic materials and geometry under the project
license. [Provenance](../provenance.md)
