# Fully developed laminar channel flow

The `laminar_channel` case describes incompressible, Newtonian flow of constant
density and viscosity between two parallel stationary plates. Both transverse
endpoints must be assigned a no-slip condition. The axial flow is steady and
fully developed. The width is a span multiplier for the two-dimensional
solution. Side walls, entrance development, compressibility, secondary motion
and turbulent stresses are outside this model. Apply the model where the flow
remains laminar and the width and developed length justify the approximation.

Specify upstream and downstream pressure, or the inlet volumetric flow and
outlet pressure. Absolute pressure uses pascals. A gauge reading also requires
its reference absolute pressure. The reported port pressures are absolute.
Pressure loss preserves small differences between readings even when the
absolute port pressures round to the same binary64 value. The axial
direction points from the inlet to the outlet. A solved
case has positive flow in this direction.

For gap `H` and length `L`, the transverse momentum equation is
`−μ u'' = (p_in − p_out)/L`. The cell-centred finite-volume method integrates
this equation over each interval. Interior viscous face fluxes are equal and
opposite in neighbouring cells. A wall flux spans the distance from the cell
centre to the no-slip face. The matrix is symmetric positive definite and the
sparse solver checks its force-per-area residual in pascals. The prescribed
volume case solves a reference gradient of 1 Pa/m and scales the velocity
field and gradient by the discrete integral of that field. Its convergence
history is for the reference gradient equation.

The reported wall drags come from the wall viscous fluxes. The momentum balance
compares these forces with the pressure force on the channel cross-section.
Constant volumetric flow gives the same mass flow at both ports. The reported
Reynolds number uses the mean velocity and parallel-plate hydraulic diameter
`2H`.

For fully developed laminar flow the closed-form velocity is
`u(y) = (Δp/(2 μ L)) y (H − y)`. Its integral gives
`Δp = 12 μ L Q/(width H³)`. The example inputs give a 2.4 Pa
analytical loss, 0.01 kg/s mass flow and a Reynolds number of 200.
The 32-cell pressure loss tolerance is 0.005 Pa. Doubling the number of
uniform cells should reduce the pressure loss error by about four. The
verification exercises both driving modes and checks the pressure reference,
analytical profile and axial force balance.

```sh
cargo run --locked -- run examples/laminar_channel.json
cargo test --locked --test laminar_channel
```
