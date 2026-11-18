# Steady three-dimensional solid heat conduction

The `heat_conduction` case solves `−∇·(k∇T) = Q` on one connected
tetrahedral solid with constant isotropic conductivity. Use global coordinates
in metres, absolute temperatures in kelvin and conductivity `k > 0` in
W/(m·K). The optional constant `volumetric_heat_source_w_m3` defaults to zero.
Positive `Q` generates heat and negative `Q` removes it. This analysis uses
steady conduction in a single homogeneous solid. The cooling-channel model
has its own wall and coolant formulation.
[Multimaterial heat conduction with ideal contact](multimaterial-heat-conduction.md)

```sh
cargo run --locked -- check examples/heat_conduction.json
cargo run --locked -- run examples/heat_conduction.json
cargo test --locked --test heat_conduction -- --nocapture
```

Set `physics.kind` to `heat_conduction` and supply `physics.geometry`,
`physics.mesh` and `physics.thermal`. The mesh uses the same positively
oriented tetrahedra and named exterior triangles as the elastic solid. The
`thermal.material` object holds `id`, `thermal_conductivity_w_m_k` and
`source` with a citation and usage terms. [Tetrahedral mesh format](elastic-solid.md)

## Boundary conditions

Assign at most one condition to each named region in `thermal.boundary_conditions`.
A region without an assignment is insulated.

| `kind` | Additional fields | Physical meaning |
| --- | --- | --- |
| `temperature` | `temperature_k` | Fix the surface temperature in K |
| `heat_flux` | `inward_heat_flux_w_m2` | Uniform flux in W/m², positive into the solid |
| `insulation` | None | Zero normal heat flux |
| `convection` | `ambient_temperature_k`, `film_coefficient_w_m2_k` | Inward heat flux `h(T_ambient − T_surface)`, with positive `h` in W/(m²·K) |

With outward unit normal `n` and physical flux vector `q = −k∇T`, the
inward flux is `−q·n`. Prescribed and ambient temperatures must be positive
and all inputs finite. Adjacent fixed-temperature surfaces must agree at each
shared vertex. Flux and convection are integrated over their own triangles
and may meet other conditions along edges. Validation requires a prescribed
temperature or a positive convection conductance on a finite-area surface
to fix the absolute temperature. A solved temperature at or below absolute
zero is a calculation error.

Linear temperature shape functions give a constant gradient in each
tetrahedron. The weak form adds `∫ h T N_i dA` on the left and
`∫ h T_ambient N_i dA` on the right for convection. CAEKit integrates the
linear triangle mass matrix with its off-diagonal terms and splits a
constant cell source among the four vertices. Conjugate gradient solves
the sparse system after symmetric elimination of prescribed temperatures.
The solved unknowns are offsets from a prescribed temperature. When convection
provides the only temperature reference, the solver instead uses the film-
conductance-weighted mean surface temperature implied by the global heat balance.
This keeps the small internal gradients resolvable under weak convection.
[MFEM boundary-condition formulation](https://mfem.org/fem_bc/)

## Read the report

| Output | Sampling | Unit |
| --- | --- | --- |
| `solid_temperature` | Mesh vertices in input order | K |
| `solid_heat_flux` | Tetrahedron centroids in cell order, three global components | W/m² |
| `boundary_heat_rate` | One integrated power per named exterior region, positive inward | W |

The `solid_energy` balance adds volume generation, prescribed inward flux,
convection and prescribed-temperature reaction power. At a fixed-temperature
vertex the reaction is the power needed to satisfy the nodal heat equation.
The global balance counts it once. If several prescribed-temperature regions
meet at that vertex, the reported regional contributions split the reaction
in proportion to their incident triangle areas. The reported imbalance is
the signed sum in W. The `solid_heat_conduction` trace reports the true
algebraic residual of the free equations and its tolerance in W.

## Analytical checks

- **Linear profile.** A 0.1 m block with 0.01 m² cross section and
  `k = 20 W/(m·K)` has end temperatures of 400 K and 300 K and insulated
  sides. The internal vertex is at 350 K, the flux vector is
  `[20000, 0, 0]` W/m² and the boundaries carry +200 W and −200 W.
- **Prescribed flux and convection.** An inward flux of 1,000 W/m² at one
  end of the same block gives 10 W into the solid. The other end exchanges
  heat with a 300 K ambient and `h = 100 W/(m²·K)`. The film temperature
  rise is 10 K and the conductive temperature drop is 5 K, giving 315 K
  on the heated face, 310 K on the cooled face and −10 W by convection.
- **Volumetric source.** At `Q = 200,000 W/m³` with both ends at 300 K,
  the analytical temperature is `300 + Q x (L − x)/(2k)` and the flux
  is `[Q(x − L/2), 0, 0]`. The block generates 200 W.

The first two solutions are affine, so the tests allow absolute errors of
1e-8 K, 1e-6 W/m² and 1e-8 W for binary64 arithmetic and the linear solve.
For the source case the comparison uses four interior samples per
equal-volume tetrahedron, with these measured RMS errors on Linux.

| Divisions per axis | Temperature error (K) | Flux vector error (W/m²) |
| --- | ---: | ---: |
| 2 | 2.173820 | 2886.751346 |
| 4 | 0.543455 | 1443.375673 |
| 8 | 0.135864 | 721.687836 |

The test requires error reduction factors above 2.7 for temperature and
1.65 for flux under each halving of mesh spacing, allowing margin around the
ideal factors of four and two. The finest-grid bounds are 0.3 K and
800 W/m², about 2.4% of the analytical peak temperature rise and 8% of the
peak flux. The 2e-6 W balance allowance is 1e-8 of the 200 W source.
The material values and block are synthetic analytical inputs under the
project license. [Provenance](../provenance.md)
