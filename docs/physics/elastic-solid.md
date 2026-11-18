# Three-dimensional linear elasticity

The `elastic_solid` case solves equilibrium of a single connected isotropic
elastic solid under small strain and displacement. Supply an elastic modulus,
a Poisson ratio strictly between −1 and 0.5 and a tetrahedral mesh in global
Cartesian coordinates. The solver uses a displacement formulation with
constant strain per tetrahedron. Use it for linear elastic response. Contact,
large deformation, material nonlinearity and dynamics require different
formulations. Linear tetrahedra can be too stiff in bending or near the
incompressible limit. Refine the mesh and check the engineering output.

The mesh uses zero-based vertex indexes and a positive determinant of the
three edges from the first node. Each exterior triangle must belong to one
named region. The order of triangle vertices is arbitrary. Outward normals
are oriented from the adjacent volume cell. Mesh checks report the smallest
and largest cell edges and the tetrahedron with the lowest mean-ratio
quality. Quality is one for a regular tetrahedron and tends to zero under
degeneration. Topology checks require a face-connected mesh and detect
inverted cells, repeated cells, nonmanifold faces, invalid regions and
vertices excluded from the volume mesh. They assume a conforming,
non-self-intersecting volume.

Specify displacement components in metres or leave selected components free
with JSON `null`. Overlapping assignments at shared vertices must be
consistent. Displacement constraints must restrain translation and rotation.
Traction vectors are in pascals. Pressure traction is
`−(p_applied − p_ambient) * outward_normal` with explicit absolute or gauge
pressure input as in the laminar channel. Tractions and pressures add.
Unloaded components use the natural zero-traction condition. A constant
body force can be specified in N/m³.

The weak form integrates `ε(v):σ(u)` over the domain. Stress is
`σ = λ tr(ε) I + 2 μ ε` and `ε = (grad(u) + grad(u)ᵀ)/2`.
Linear tetrahedral shape functions give constant gradients per cell.
Three-dimensional displacements are assembled into a sparse system with
symmetric elimination of prescribed components. Surface and body loads
are integrated with consistent basis weights. Constrained reactions are
recovered by subtracting applied forces from the internal forces. Reports
give displacement and reaction vectors at vertices, stress tensors and
von Mises scalars at cell centroids, strain energy and force and moment
balances. Moments use the first mesh vertex as the reference point.

The same constraints, loads and elastic material are available for
temperature-driven deformation. [Thermoelasticity](thermoelastic-solid.md)

The reference block is 2 m by 0.5 m by 0.4 m. Its 10 MPa applied traction
and 200 GPa modulus give 1e-4 m of extension independent of the solver.
Analytical transverse strains are −0.31 times the axial strain, support
reaction is −2 MN and strain energy is 100 J. The affine displacement and
constant stress form an exact patch test for linear elements. The test
also reverses the load through the pressure boundary, checks independent
force and moment integrals for a body load and rejects invalid
assignments and missing rigid support.

```sh
cargo run --locked -- check examples/elastic_solid.json
cargo run --locked -- run examples/elastic_solid.json
cargo test --locked --test elastic_solid
```
