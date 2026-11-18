//! Solid heat conduction and conservative bulk coolant energy transport.

pub mod heat_conduction;
mod interface;

use serde::{Deserialize, Serialize};

use caekit_mesh::{GeometryRevision, LineMesh, MeshMetrics};
use caekit_numerics::{Convergence, StopReason, norm};
use caekit_types::error::{nonempty, positive};
use caekit_types::{Balance, Error, MaterialSource};
use interface::overlap;

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

impl ConductiveWall {
    pub fn check_meshes(&self, length: f64) -> Result<[MeshMetrics; 2], Error> {
        positive("physics.wall.thickness_m", self.thickness_m)?;
        nonempty("physics.wall.material.id", &self.material.id)?;
        positive(
            "physics.wall.material.thermal_conductivity_w_m_k",
            self.material.thermal_conductivity_w_m_k,
        )?;
        self.material
            .source
            .validate("physics.wall.material.source")?;
        let wall = self
            .axial_mesh
            .validate("physics.wall.axial_mesh", length)?;
        let thickness = self
            .thickness_mesh
            .validate("physics.wall.thickness_mesh", self.thickness_m)?;
        self.hot_boundary.validate(
            "physics.wall.hot_boundary",
            &self.thickness_mesh.start_region,
        )?;
        Ok([wall, thickness])
    }
}

impl Coolant {
    pub fn validate_properties(&self, inlet_region: &str) -> Result<(), Error> {
        self.inlet.validate("physics.coolant.inlet", inlet_region)?;
        positive(
            "physics.coolant.specific_heat_j_kg_k",
            self.specific_heat_j_kg_k,
        )?;
        self.source.validate("physics.coolant.source")
    }
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
    pub fn validate(self) -> Result<(), Error> {
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

/// Thermal state in mesh order, with temperatures expressed relative to the coolant inlet.
pub struct ChannelEnergy {
    pub wall_temperature_offset_k: Vec<f64>,
    pub fluid_temperature_offset_k: Vec<f64>,
    pub wall_heat_flux_w_m2: Vec<f64>,
    pub heat_to_coolant_w: f64,
    pub balances: Vec<Balance>,
}

/// Solve a validated wall and coolant with a supplied mass flow and film coefficient.
/// Their axial meshes must cover the same interval.
pub fn solve_channel_energy(
    wall: &ConductiveWall,
    coolant: &Coolant,
    width_m: f64,
    mass_flow_kg_s: f64,
    film_coefficient_w_m2_k: f64,
    settings: CouplingSettings,
) -> Result<(Convergence, Option<ChannelEnergy>), Error> {
    let system = EnergySystem::new(
        wall,
        coolant,
        width_m,
        mass_flow_kg_s,
        film_coefficient_w_m2_k,
    )?;
    let (convergence, temperatures) = system.solve(settings);
    let energy = temperatures
        .map(|state| system.summary(state))
        .transpose()?;
    Ok((convergence, energy))
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
    fn new(
        solid: &ConductiveWall,
        coolant: &Coolant,
        width: f64,
        mass_flow_kg_s: f64,
        h: f64,
    ) -> Result<Self, Error> {
        let wall = &solid.axial_mesh.vertices_m;
        let fluid = &coolant.axial_mesh.vertices_m;
        let capacity = mass_flow_kg_s * coolant.specific_heat_j_kg_k;
        positive("coolant heat capacity flow rate", capacity)?;

        let wall_area: Vec<_> = wall
            .windows(2)
            .map(|edge| width * (edge[1] - edge[0]))
            .collect();
        let coefficient = solid.material.thermal_conductivity_w_m_k / solid.thickness_m;
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
            hot_delta: solid.hot_boundary.temperature_k - coolant.inlet.temperature_k,
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
            diagnostic: None,
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

    fn summary(&self, temperatures: Temperatures) -> Result<ChannelEnergy, Error> {
        let flux = self.fluxes(&temperatures);
        let hot_heat: f64 = self
            .conductance
            .iter()
            .zip(&temperatures.wall)
            .map(|(conductance, wall)| conductance * (self.hot_delta - wall))
            .sum();
        let outlet_delta = temperatures.fluid[temperatures.fluid.len() - 1];
        let carried_heat = self.capacity * outlet_delta;
        let film_loss: f64 = flux.from_wall.iter().sum();
        let film_gain: f64 = flux.into_fluid.iter().sum();
        let balances = vec![
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
        ];

        let wall_heat_flux_w_m2 = flux
            .from_wall
            .into_iter()
            .zip(&self.wall_area)
            .map(|(heat, area)| heat / area)
            .collect();
        Ok(ChannelEnergy {
            wall_temperature_offset_k: temperatures.wall,
            fluid_temperature_offset_k: temperatures.fluid,
            wall_heat_flux_w_m2,
            heat_to_coolant_w: film_gain,
            balances,
        })
    }
}
