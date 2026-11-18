# Conjugate cooling channel

The `cooling_channel` case couples a constant-property laminar channel and a
conductive solid wall. The flow solve supplies the mass flow to a separate
fluid energy mesh. The wall conducts through its thickness. The stream
transports heat by axial bulk enthalpy advection. The model assumes a uniform
hot-face temperature, constant wall conductivity, constant fluid heat
capacity, negligible axial conduction and a prescribed fluid-side film
coefficient. The film coefficient must come from an appropriate correlation
or measurement in engineering use. This model has no turbulence, phase
change, viscous dissipation, or temperature feedback into the momentum solve.

Each axial wall panel is a one-dimensional conduction column with linear
finite elements through the thickness. For constant conductivity and
source-free steady conduction, eliminating interior temperatures gives the
exact element-chain conductance `k A / thickness`. The normal mesh supplies
positions for the reconstructed wall temperature. The axial wall and coolant
meshes can have different subdivisions of the same physical interval. The
model ties the wetted wall face to one explicitly named hydraulic wall.

The solver intersects the two sets of interface intervals. Each overlap
uses the same area for the heat leaving the solid and entering the fluid.
For area `A_ij`, the contribution is `h A_ij (T_wall,j − T_fluid,i)`.
The fluid energy equation uses first-order upwind finite-volume fluxes.
Partitioned Gauss–Seidel alternates wall conduction with the downstream
fluid energy sweep. It checks the combined L2 norm of the wall-cell and
fluid-cell energy residuals in watts after each sweep. The convergence
threshold is the larger of the absolute tolerance in watts and the
relative tolerance times the initial norm. The result also reports separate
global balances for the solid, the interface, the fluid and the whole system.
Positive interface heat flux denotes heat supplied to the coolant.
Conservative transfer of extensive quantities across distinct meshes is
also described in the [preCICE mapping documentation](https://precice.org/docs/configuration/basics/mapping/).

An independent reference follows from the continuous thermal circuit.
The area resistance `R'' = thickness/k + 1/h` and energy capacity flow
`C = mass_flow * cp` give
`T_out = T_hot − (T_hot − T_in) exp(−width * length/(R'' C))`.
The flow and material properties in the example are synthetic. The
reference determines the temperature and the heat duty `C(T_out − T_in)`.
The 31-wall/48-fluid-panel example accepts 0.5 K temperature error and
requires each reported energy balance to close to within 1e-6 W.
A separate matching-grid test doubles the number of panels from 12 to 96
and verifies the first-order decrease in the outlet-temperature error.
These are numerical checks against the reduced equations.

```sh
cargo run --locked -- check examples/cooling_channel.json
cargo run --locked -- run examples/cooling_channel.json
cargo test --locked --test cooling_channel
```
