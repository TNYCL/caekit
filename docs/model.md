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

The default linear settings are a relative residual tolerance of 1e-10, an
absolute residual tolerance of zero and an iteration limit of 20,000. For the
current reference algorithm the threshold is
`max(absolute_residual_tolerance, relative_residual_tolerance * ||b||₂)`
in the solved equation's residual units. A run reports this threshold and an
independently recomputed final residual `||b − Ax||₂`. History entries during
conjugate gradient are recursive residual estimates.

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

The CLI `check` command writes validation and mesh metrics as JSON. The `run`
command writes the run report as JSON. Exit code 0 indicates a converged run.
Exit code 2 preserves the convergence report for a nonconverged run. Exit code
1 indicates an input, calculation-range or I/O error. A nonconverged solve
withholds physical fields. An algebraic convergence threshold measures
iterative error. Engineering error also depends on the physical assumptions,
geometry, mesh and properties.
