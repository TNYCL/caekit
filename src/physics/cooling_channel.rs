//! Steady conjugate heat transfer between a conductive wall and a laminar stream.

use serde::{Deserialize, Serialize};

use crate::interface::overlap;
use crate::{Error, GeometryRevision, LineMesh, MaterialSource, MeshMetrics};
use caekit_numerics::{Convergence, LinearSettings, StopReason, norm};
use caekit_types::error::{nonempty, positive};
use caekit_types::{Balance, Output, Quantity, SampleLocation, Solution};

use super::laminar_channel::{FlowState, LaminarChannel};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoolingChannel {
    pub flow: LaminarChannel,
    pub wall: ConductiveWall,
    pub coolant: Coolant,
    pub interface: ThermalConnection,
    #[serde(default)]
    pub coupling_solver: CouplingSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConductiveWall {
    pub identity: GeometryRevision,
    pub thickness_m: f64,
    pub material: ConductiveMaterial,
    pub axial_mesh: LineMesh,
    pub thickness_mesh: LineMesh,
    pub hot_boundary: TemperatureBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConductiveMaterial {
    pub id: String,
    pub thermal_conductivity_w_m_k: f64,
    pub source: MaterialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coolant {
    pub specific_heat_j_kg_k: f64,
    pub source: MaterialSource,
    pub axial_mesh: LineMesh,
    pub inlet: TemperatureBoundary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemperatureBoundary {
    pub region: String,
    pub temperature_k: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermalConnection {
    pub solid_region: String,
    pub fluid_region: String,
    pub heat_transfer_coefficient_w_m2_k: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CouplingSettings {
    pub relative_residual_tolerance: f64,
    pub absolute_residual_tolerance_w: f64,
    pub max_iterations: usize,
}

impl Default for CouplingSettings {
    fn default() -> Self {
        Self {
            relative_residual_tolerance: 1.0e-10,
            absolute_residual_tolerance_w: 1.0e-10,
            max_iterations: 1000,
        }
    }
}

impl CoolingChannel {
    pub(crate) const THERMAL_OUTPUTS: &[Output] = &[
        Output::FluidBulkTemperature,
        Output::WallTemperature,
        Output::OutletTemperature,
        Output::HeatTransferRate,
        Output::InterfaceHeatFlux,
    ];
    const METHOD: &str = "steady_laminar_fvm_conjugate_wall_fem_bulk_energy_fvm";

    pub(crate) fn validate(&self) -> Result<Vec<MeshMetrics>, Error> {
        let flow = self.flow.validate("physics.flow")?;
        let length = self.flow.geometry.length_m;
        self.wall.identity.validate("physics.wall.identity")?;
        if self.wall.identity.id == self.flow.geometry.identity.id {
            return Err(Error::invalid(
                "physics.wall.identity.id",
                "fluid and wall geometry IDs must differ",
            ));
        }
        positive("physics.wall.thickness_m", self.wall.thickness_m)?;
        nonempty("physics.wall.material.id", &self.wall.material.id)?;
        positive(
            "physics.wall.material.thermal_conductivity_w_m_k",
            self.wall.material.thermal_conductivity_w_m_k,
        )?;
        self.wall
            .material
            .source
            .validate("physics.wall.material.source")?;
        let wall = self
            .wall
            .axial_mesh
            .validate("physics.wall.axial_mesh", length)?;
        let thickness = self
            .wall
            .thickness_mesh
            .validate("physics.wall.thickness_mesh", self.wall.thickness_m)?;
        self.wall.hot_boundary.validate(
            "physics.wall.hot_boundary",
            &self.wall.thickness_mesh.start_region,
        )?;
        let coolant = self
            .coolant
            .axial_mesh
            .validate("physics.coolant.axial_mesh", length)?;
        if self.coolant.axial_mesh.start_region != self.flow.geometry.inlet_region
            || self.coolant.axial_mesh.end_region != self.flow.geometry.outlet_region
        {
            return Err(Error::invalid(
                "physics.coolant.axial_mesh",
                "axial thermal mesh must share the hydraulic inlet and outlet regions",
            ));
        }
        self.coolant
            .inlet
            .validate("physics.coolant.inlet", &self.flow.geometry.inlet_region)?;
        positive(
            "physics.coolant.specific_heat_j_kg_k",
            self.coolant.specific_heat_j_kg_k,
        )?;
        self.coolant.source.validate("physics.coolant.source")?;
        if self.interface.solid_region != self.wall.thickness_mesh.end_region {
            return Err(Error::invalid(
                "physics.interface.solid_region",
                "expected the wetted end of the wall thickness mesh",
            ));
        }
        if self
            .flow
            .mesh
            .endpoint(&self.interface.fluid_region)
            .is_none()
        {
            return Err(Error::invalid(
                "physics.interface.fluid_region",
                "expected a named no-slip fluid wall",
            ));
        }
        positive(
            "physics.interface.heat_transfer_coefficient_w_m2_k",
            self.interface.heat_transfer_coefficient_w_m2_k,
        )?;
        self.coupling_solver.validate()?;
        Ok(vec![flow, wall, thickness, coolant])
    }

    pub(crate) fn solve(
        &self,
        settings: LinearSettings,
        outputs: &[Output],
    ) -> Result<Solution<Convergence>, Error> {
        let (momentum, flow) = self.flow.compute(settings)?;
        let mut result = Solution::new(Self::METHOD, vec![momentum]);
        let Some(flow) = flow else {
            return Ok(result);
        };
        let thermal = EnergySystem::new(self, &flow)?;
        let (convergence, temperatures) = thermal.solve(self.coupling_solver);
        result.convergence.push(convergence);
        if let Some(temperatures) = temperatures {
            self.add_thermal_outputs(&thermal, temperatures, outputs, &mut result)?;
            self.flow.add_outputs(flow, &mut result)?;
        }
        Ok(result)
    }

    fn add_thermal_outputs(
        &self,
        energy: &EnergySystem,
        temperatures: Temperatures,
        outputs: &[Output],
        result: &mut Solution<Convergence>,
    ) -> Result<(), Error> {
        let flux = energy.fluxes(&temperatures);
        let hot_heat: f64 = energy
            .conductance
            .iter()
            .zip(&temperatures.wall)
            .map(|(conductance, wall)| conductance * (energy.hot_delta - wall))
            .sum();
        let outlet_delta = temperatures.fluid[temperatures.fluid.len() - 1];
        let carried_heat = energy.capacity * outlet_delta;
        let film_loss: f64 = flux.from_wall.iter().sum();
        let film_gain: f64 = flux.into_fluid.iter().sum();
        result.balances.extend([
            Balance::new(
                "wall_energy",
                "W",
                [
                    ("hot_face_input", hot_heat),
                    ("heat_to_interface", -film_loss),
                ],
            )?,
            Balance::new(
                "interface_energy",
                "W",
                [("from_wall", film_loss), ("to_fluid", -film_gain)],
            )?,
            Balance::new(
                "coolant_energy",
                "W",
                [
                    ("heat_from_interface", film_gain),
                    ("outlet_enthalpy_rise", -carried_heat),
                ],
            )?,
            Balance::new(
                "system_energy",
                "W",
                [
                    ("hot_face_input", hot_heat),
                    ("outlet_enthalpy_rise", -carried_heat),
                ],
            )?,
        ]);

        let inlet = self.coolant.inlet.temperature_k;
        let geometry = &self.flow.geometry;
        let gap = geometry.gap_m;
        let lower_wall = self.interface.fluid_region == self.flow.mesh.start_region;
        let wetted_y = if lower_wall { 0.0 } else { gap };
        let into_solid = if lower_wall { -1.0 } else { 1.0 };
        result.quantities.extend([
            (
                Output::OutletTemperature,
                Quantity::scalar("K", inlet + outlet_delta),
            ),
            (Output::HeatTransferRate, Quantity::scalar("W", film_gain)),
            (
                Output::FluidBulkTemperature,
                Quantity::field(
                    "K",
                    SampleLocation::Cell,
                    centers(&self.coolant.axial_mesh)
                        .map(|x| geometry.position(x, gap / 2.0))
                        .collect(),
                    temperatures
                        .fluid
                        .into_iter()
                        .map(|theta| inlet + theta)
                        .collect(),
                ),
            ),
            (
                Output::InterfaceHeatFlux,
                Quantity::field(
                    "W/m^2",
                    SampleLocation::InterfaceFace,
                    centers(&self.wall.axial_mesh)
                        .map(|x| geometry.position(x, wetted_y))
                        .collect(),
                    flux.from_wall
                        .into_iter()
                        .zip(&energy.wall_area)
                        .map(|(heat, area)| heat / area)
                        .collect(),
                ),
            ),
        ]);

        if outputs.contains(&Output::WallTemperature) {
            let thickness = self.wall.thickness_m;
            let through_wall = &self.wall.thickness_mesh.vertices_m;
            let samples = through_wall
                .len()
                .checked_mul(temperatures.wall.len())
                .ok_or_else(|| Error::InsufficientMemory {
                    resource: "wall temperature samples".to_owned(),
                })?;
            let mut positions = Vec::new();
            let mut values = Vec::new();
            positions
                .try_reserve_exact(samples)
                .and_then(|_| values.try_reserve_exact(samples))
                .map_err(|_| Error::InsufficientMemory {
                    resource: "wall temperature samples".to_owned(),
                })?;
            for (x, wall_theta) in centers(&self.wall.axial_mesh).zip(temperatures.wall) {
                for &s in through_wall {
                    let across_wall = s / thickness;
                    let theta = (1.0 - across_wall) * energy.hot_delta + across_wall * wall_theta;
                    positions.push(geometry.position(x, wetted_y + into_solid * (thickness - s)));
                    values.push(inlet + theta);
                }
            }
            result.quantities.insert(
                Output::WallTemperature,
                Quantity::field("K", SampleLocation::Sample, positions, values),
            );
        }
        Ok(())
    }
}

fn centers(mesh: &LineMesh) -> impl Iterator<Item = f64> + '_ {
    mesh.vertices_m
        .windows(2)
        .map(|edge| edge[0] + 0.5 * (edge[1] - edge[0]))
}

impl TemperatureBoundary {
    fn validate(&self, path: &str, region: &str) -> Result<(), Error> {
        if self.region != region {
            return Err(Error::invalid(
                format!("{path}.region"),
                format!("expected {region:?}"),
            ));
        }
        positive(&format!("{path}.temperature_k"), self.temperature_k)
    }
}

impl CouplingSettings {
    fn validate(self) -> Result<(), Error> {
        if !(0.0 < self.relative_residual_tolerance && self.relative_residual_tolerance < 1.0) {
            return Err(Error::invalid(
                "physics.coupling_solver.relative_residual_tolerance",
                "must be greater than zero and less than one",
            ));
        }
        if !(self.absolute_residual_tolerance_w.is_finite()
            && self.absolute_residual_tolerance_w >= 0.0)
        {
            return Err(Error::invalid(
                "physics.coupling_solver.absolute_residual_tolerance_w",
                "must be nonnegative and finite",
            ));
        }
        if self.max_iterations == 0 {
            return Err(Error::invalid(
                "physics.coupling_solver.max_iterations",
                "must be positive",
            ));
        }
        Ok(())
    }
}

struct ThermalFace {
    wall: usize,
    fluid: usize,
    coefficient_w_k: f64,
}

struct EnergySystem {
    faces: Vec<ThermalFace>,
    capacity: f64,
    hot_delta: f64,
    wall_area: Vec<f64>,
    conductance: Vec<f64>,
    wall_film: Vec<f64>,
    fluid_film: Vec<f64>,
}

struct Temperatures {
    wall: Vec<f64>,
    fluid: Vec<f64>,
}

struct InterfaceFlux {
    from_wall: Vec<f64>,
    into_fluid: Vec<f64>,
}

impl EnergySystem {
    fn new(case: &CoolingChannel, flow: &FlowState) -> Result<Self, Error> {
        let width = case.flow.geometry.width_m;
        let wall = &case.wall.axial_mesh.vertices_m;
        let fluid = &case.coolant.axial_mesh.vertices_m;
        let h = case.interface.heat_transfer_coefficient_w_m2_k;
        let capacity = flow.mass_flow_kg_s() * case.coolant.specific_heat_j_kg_k;
        positive("coolant heat capacity flow rate", capacity)?;

        let wall_area: Vec<_> = wall
            .windows(2)
            .map(|edge| width * (edge[1] - edge[0]))
            .collect();
        let coefficient = case.wall.material.thermal_conductivity_w_m_k / case.wall.thickness_m;
        // Condensed P1 conduction column. Uniform k and zero volumetric heat source
        // give a linear profile regardless of the normal element spacing.
        let conductance: Vec<_> = wall_area.iter().map(|area| coefficient * area).collect();
        let mut wall_film = vec![0.0; wall.len() - 1];
        let mut fluid_film = vec![0.0; fluid.len() - 1];
        let mut faces = Vec::new();
        for face in overlap(wall, fluid) {
            let coefficient = h * (width * face.length_m);
            positive(
                "interface face heat transfer coefficient times area",
                coefficient,
            )?;
            wall_film[face.left] += coefficient;
            fluid_film[face.right] += coefficient;
            faces.push(ThermalFace {
                wall: face.left,
                fluid: face.right,
                coefficient_w_k: coefficient,
            });
        }

        if !conductance
            .iter()
            .zip(&wall_film)
            .all(|(&a, &b)| a > 0.0 && b > 0.0 && (a + b).is_finite())
            || !fluid_film
                .iter()
                .all(|&b| b > 0.0 && (capacity + b).is_finite())
        {
            return Err(Error::NonFiniteResult {
                quantity: "thermal conductances".to_owned(),
            });
        }
        Ok(Self {
            faces,
            capacity,
            hot_delta: case.wall.hot_boundary.temperature_k - case.coolant.inlet.temperature_k,
            wall_area,
            conductance,
            wall_film,
            fluid_film,
        })
    }

    fn fluxes(&self, state: &Temperatures) -> InterfaceFlux {
        let mut from_wall = vec![0.0; state.wall.len()];
        let mut into_fluid = vec![0.0; state.fluid.len()];
        for face in &self.faces {
            let heat = face.coefficient_w_k * (state.wall[face.wall] - state.fluid[face.fluid]);
            from_wall[face.wall] += heat;
            into_fluid[face.fluid] += heat;
        }
        InterfaceFlux {
            from_wall,
            into_fluid,
        }
    }

    fn residual(&self, state: &Temperatures, imbalance: &mut Vec<f64>) -> f64 {
        let flux = self.fluxes(state);
        imbalance.clear();
        for (j, heat) in flux.from_wall.into_iter().enumerate() {
            imbalance.push(self.conductance[j] * (self.hot_delta - state.wall[j]) - heat);
        }
        let mut upstream = 0.0;
        for (temperature, heat) in state.fluid.iter().zip(flux.into_fluid) {
            imbalance.push(heat - self.capacity * (temperature - upstream));
            upstream = *temperature;
        }
        norm(imbalance)
    }

    fn solve(&self, settings: CouplingSettings) -> (Convergence, Option<Temperatures>) {
        let mut state = Temperatures {
            wall: vec![0.0; self.wall_film.len()],
            fluid: vec![0.0; self.fluid_film.len()],
        };
        let mut residual = Vec::new();
        let mut trace = Convergence {
            stage: "coupled_wall_coolant_energy".to_owned(),
            algorithm: "partitioned_gauss_seidel".to_owned(),
            stop_reason: StopReason::NumericalBreakdown,
            iterations: 0,
            residual_unit: "W".to_owned(),
            residual_threshold: None,
            residual_l2_history: Vec::new(),
            final_true_residual_l2: None,
        };
        let initial = self.residual(&state, &mut residual);
        if !initial.is_finite() {
            return (trace, None);
        }
        let threshold = settings
            .absolute_residual_tolerance_w
            .max(settings.relative_residual_tolerance * initial);
        trace.residual_threshold = Some(threshold);
        trace.residual_l2_history.push(initial);
        trace.final_true_residual_l2 = Some(initial);
        if initial == 0.0 {
            trace.stop_reason = StopReason::Converged;
            return (trace, Some(state));
        }

        let mut fluid_forcing = vec![0.0; self.fluid_film.len()];
        let mut wall_forcing = vec![0.0; self.wall_film.len()];
        for iteration in 1..=settings.max_iterations {
            wall_forcing.fill(0.0);
            for face in &self.faces {
                wall_forcing[face.wall] += face.coefficient_w_k * state.fluid[face.fluid];
            }
            for (j, &mapped_fluid) in wall_forcing.iter().enumerate() {
                state.wall[j] = (self.conductance[j] * self.hot_delta + mapped_fluid)
                    / (self.conductance[j] + self.wall_film[j]);
            }

            fluid_forcing.fill(0.0);
            for face in &self.faces {
                fluid_forcing[face.fluid] += face.coefficient_w_k * state.wall[face.wall];
            }
            let mut upstream = 0.0;
            for (i, &mapped_wall) in fluid_forcing.iter().enumerate() {
                let theta =
                    (self.capacity * upstream + mapped_wall) / (self.capacity + self.fluid_film[i]);
                state.fluid[i] = theta;
                upstream = theta;
            }

            trace.iterations = iteration;
            let actual = self.residual(&state, &mut residual);
            if !actual.is_finite() {
                trace.final_true_residual_l2 = None;
                return (trace, None);
            }
            trace.final_true_residual_l2 = Some(actual);
            trace.residual_l2_history.push(actual);
            if actual <= threshold {
                trace.stop_reason = StopReason::Converged;
                return (trace, Some(state));
            }
        }
        trace.stop_reason = StopReason::IterationLimit;
        (trace, None)
    }
}
