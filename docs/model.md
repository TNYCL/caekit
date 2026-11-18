# Model and result documents

A model is UTF-8 JSON. Dispatch uses the integer `schema_version` before the
remaining fields are decoded. CAEKit reads version 1. Unknown schema versions,
fields, physics kinds and condition kinds are errors. Future readers must
preserve the meaning of earlier versions or provide an explicit migration.

| Field | Meaning |
| --- | --- |
| `name` | Human-readable run name |
| `units` | `si`. Numeric input keys specify the SI quantity and unit |
| `physics` | Tagged physical case with geometry identity, mesh, material and conditions |
| `linear_solver` | Optional iterative stopping settings. If present, supply all fields |
| `outputs` | Requested quantities for the selected physics case |

Coordinates use a global Cartesian frame in metres. One-dimensional cases
place an oriented local axis in that frame. The origin and unit direction map
local mesh coordinates to global sample positions. Each mesh lists ordered
vertices including both endpoints. Conditions refer to named boundary regions
defined by that mesh. The tetrahedral mesh uses zero-based volume connectivity
and named exterior triangle sets. Assignments belong to the supplied geometry
revision.

The `heat_conduction` and `thermoelastic_solid` cases put thermal material and
boundary conditions under `physics.thermal`. The applied
`inward_heat_flux_w_m2` is positive into the solid, opposite to the outward
face normal. Heat rates by region use the same sign. Unassigned thermal
surfaces are insulated. The thermoelastic case adds `physics.mechanical` and
`physics.expansion` and uses one shared mesh and geometry revision.
[Heat conduction](physics/heat-conduction.md) · [Thermoelasticity](physics/thermoelastic-solid.md)

`multimaterial_heat_conduction` extends schema version 1 with a
`physics.thermal.materials` list and named `volume_regions` with per-cell
ownership. It adds the optional `interface_heat_rate` output in W with
explicit directions between volume regions. Its report includes a
`solid_energy:<region>` balance for each volume. Earlier models continue
to use their version 1 shape, report fields and typed-model hash.
[Multimaterial heat conduction](physics/multimaterial-heat-conduction.md)

Rust callers use the same `Model::from_json`, `Model::validate` and
`caekit::solve` entry points. Update exhaustive matches for
`Physics::MultimaterialHeatConduction`, `Output::InterfaceHeatRate` and
`Quantity::ByInterface`. The multimaterial configuration can be constructed
from `caekit::physics::heat_conduction` types.

The default linear settings are a relative residual tolerance of 1e-10, an
absolute residual tolerance of zero and an iteration limit of 20,000. For the
current reference algorithm the threshold is
`max(absolute_residual_tolerance, relative_residual_tolerance * ||b||₂)`
in the solved equation's residual units. A run reports this threshold and an
independently recomputed final residual `||b − Ax||₂`. History entries during
conjugate gradient are recursive residual estimates.

If repeated correction attempts stagnate above the requested tolerance within
the binary64 rounding scale of the residual evaluation, the stage stops with
`numerical_breakdown`. Its optional `diagnostic` explains the failure and
the trace retains the measured true residual and requested threshold.

In `heat_conduction` and `multimaterial_heat_conduction`, the top-level
`linear_solver` uses W for the absolute tolerance. In
`thermoelastic_solid`, `linear_solver` uses N and `physics.thermal_solver`
uses W. Each stage reports its own threshold,
iterations, stop reason and final true residual. The mechanical stage runs
after a converged thermal stage. An iteration-limit result includes the
traces of the stages that ran and withholds physical fields and balances.

The structural solvers remove a constant prescribed translation in each
direction before forming the reduced system. Stresses, reactions and
strain energy use these relative displacements. Reported nodal displacements
include the translation and preserve the original prescribed values.

The version 1 result records a SHA-256 digest of the compact serialization of
the typed model with defaults applied. Input indentation and object key order
do not affect the digest. Array order and explicit changes to physical or
numerical input affect the digest. Preserve the source model with the result.
The report includes geometry revisions, mesh metrics, the method and execution
metadata. Quantities carry units and their sampling locations. Signed balance
terms and the signed imbalance are reported independently of requested fields.
Material citations and usage terms are stored in the hashed source model.

The CLI `check` command writes validation and mesh metrics as JSON. The `run`
command writes the run report as JSON. Exit code 0 indicates a converged run.
Exit code 2 preserves the convergence report for a nonconverged run. Exit code
1 indicates an input, calculation-range or I/O error. A nonconverged solve
withholds physical fields. An algebraic convergence threshold measures
iterative error. Engineering error also depends on the physical assumptions,
geometry, mesh and properties.

## Outputs

A model requests quantities by name in `outputs`. Each physics kind supports the names
listed for it; any other name, and a repeated name, is an error. The report holds exactly
the requested quantities under `quantities`. Unit strings are written in ASCII: `m^3/s`,
`W/m^2`, `N m` and `1` for a dimensionless number.

| Output | Physics kinds | Quantity kind | Unit | Meaning |
| --- | --- | --- | --- | --- |
| `axial_displacement` | `axial_bar` | `field` at nodes | m | Displacement along the positive local axis, in vertex order |
| `axial_stress` | `axial_bar` | `field` at cells | Pa | Element stress `E (u_right − u_left)/h`, tension positive |
| `end_displacement` | `axial_bar` | `by_region` | m | Axial displacement of the two ends, keyed by the mesh's end region names |
| `reaction_force` | `axial_bar` | `by_region` | N | Force that the support exerts on the bar, positive along the local axis, for each end with a prescribed displacement |
| `axial_velocity` | `laminar_channel`, `cooling_channel` | `field` at cells | m/s | Axial velocity at the transverse cell centres, positioned at mid-length |
| `volume_flow_rate` | `laminar_channel`, `cooling_channel` | `scalar` | m^3/s | Volume flow from the inlet to the outlet |
| `mass_flow_rate` | `laminar_channel`, `cooling_channel` | `scalar` | kg/s | Density times the volume flow rate |
| `pressure_loss` | `laminar_channel`, `cooling_channel` | `scalar` | Pa | Inlet pressure minus outlet pressure |
| `absolute_pressure` | `laminar_channel`, `cooling_channel` | `by_region` | Pa | Absolute pressure at the inlet and outlet regions |
| `channel_reynolds` | `laminar_channel`, `cooling_channel` | `scalar` | 1 | Reynolds number from the mean velocity and the hydraulic diameter of twice the gap |
| `wall_drag` | `laminar_channel`, `cooling_channel` | `by_region` | N | Viscous axial force that each plate exerts on the fluid, negative when it opposes the flow |
| `fluid_bulk_temperature` | `cooling_channel` | `field` at cells | K | Coolant bulk temperature at the cell centres of the coolant mesh |
| `wall_temperature` | `cooling_channel` | `field` at samples | K | Wall temperature for each axial wall panel, one value per vertex of the through-thickness mesh, starting at the hot face |
| `outlet_temperature` | `cooling_channel` | `scalar` | K | Coolant bulk temperature in the last cell of the coolant mesh |
| `heat_transfer_rate` | `cooling_channel` | `scalar` | W | Total heat rate from the wall to the coolant, positive when the coolant is heated |
| `interface_heat_flux` | `cooling_channel` | `field` at interface faces | W/m^2 | Heat flux leaving the wall towards the coolant on each axial wall panel, positive when the coolant is heated |
| `displacement_vector` | `elastic_solid`, `thermoelastic_solid` | `vector_field` at nodes | m | Global displacement components, in vertex order |
| `cauchy_stress` | `elastic_solid`, `thermoelastic_solid` | `tensor_field` at cells | Pa | Cauchy stress at the tetrahedron centroids, tension positive |
| `von_mises_stress` | `elastic_solid`, `thermoelastic_solid` | `field` at cells | Pa | Von Mises equivalent stress at the tetrahedron centroids |
| `nodal_reaction` | `elastic_solid`, `thermoelastic_solid` | `vector_field` at nodes | N | Support reaction acting on the solid, zero on unconstrained components |
| `elastic_energy` | `elastic_solid`, `thermoelastic_solid` | `scalar` | J | Elastic strain energy integrated over the solid |
| `solid_temperature` | `heat_conduction`, `multimaterial_heat_conduction`, `thermoelastic_solid` | `field` at nodes | K | Absolute temperature, in vertex order |
| `solid_heat_flux` | `heat_conduction`, `multimaterial_heat_conduction`, `thermoelastic_solid` | `vector_field` at cells | W/m^2 | Heat flux `q = −k∇T` at the tetrahedron centroids |
| `boundary_heat_rate` | `heat_conduction`, `multimaterial_heat_conduction`, `thermoelastic_solid` | `by_region` | W | Integrated heat rate through each named exterior region, positive into the solid |
| `interface_heat_rate` | `multimaterial_heat_conduction` | `by_interface` | W | Integrated heat rate across the shared faces of each pair of volume regions, positive from `from_region` to `to_region` |

A `field`, `vector_field` or `tensor_field` lists `positions_m` in the global frame next
to `values`, in the same order. A `by_region` quantity maps region names to values.

## Numerical stages

Each entry of `convergence` is one numerical stage. The run's `stop_reason` is that of
the first stage that did not converge.

| Stage | Physics kinds | Residual unit | Algorithm | Settings |
| --- | --- | --- | --- | --- |
| `elastic_equilibrium` | `axial_bar` | N | `jacobi_preconditioned_conjugate_gradient` | `linear_solver` |
| `axial_momentum` | `laminar_channel`, `cooling_channel` with a pressure drive | Pa | `jacobi_preconditioned_conjugate_gradient` | `linear_solver` |
| `unit_gradient_axial_momentum` | `laminar_channel`, `cooling_channel` with a volume-flow drive | Pa | `jacobi_preconditioned_conjugate_gradient` | `linear_solver` |
| `coupled_wall_coolant_energy` | `cooling_channel` | W | `partitioned_gauss_seidel` | `physics.coupling_solver` |
| `solid_elastic_equilibrium` | `elastic_solid`, `thermoelastic_solid` | N | `jacobi_preconditioned_conjugate_gradient` | `linear_solver` |
| `solid_heat_conduction` | `heat_conduction`, `multimaterial_heat_conduction`, `thermoelastic_solid` | W | `jacobi_preconditioned_conjugate_gradient` | `linear_solver`; `physics.thermal_solver` in `thermoelastic_solid` |
| `solid_interface_equilibration` | `multimaterial_heat_conduction` | W | `jacobi_preconditioned_conjugate_gradient` | `linear_solver` |

The absolute residual tolerance of a stage is in its residual unit. The optional
`physics.coupling_solver` of the cooling channel takes `relative_residual_tolerance`,
`absolute_residual_tolerance_w` and `max_iterations` and defaults to 1e-10, 1e-10 W and
1,000 sweeps.
