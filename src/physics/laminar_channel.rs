//! Fully developed, incompressible flow between two stationary parallel plates.

use serde::{Deserialize, Serialize};

use crate::error::{nonempty, positive};
use crate::result::{Balance, Output, Quantity, SampleLocation, Solution};
use crate::{AxisFrame, Error, GeometryRevision, LineMesh, MaterialSource, MeshMetrics};
use caekit_numerics::{Convergence, LinearSettings, SparseAssembly, StopReason};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaminarChannel {
    pub geometry: ChannelGeometry,
    pub mesh: LineMesh,
    pub material: NewtonianFluid,
    pub walls: [NoSlipWall; 2],
    pub drive: FlowDrive,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelGeometry {
    pub identity: GeometryRevision,
    pub axial_frame: AxisFrame,
    pub transverse_direction: [f64; 3],
    pub length_m: f64,
    pub gap_m: f64,
    pub width_m: f64,
    pub inlet_region: String,
    pub outlet_region: String,
}

impl ChannelGeometry {
    fn validate(&self, path: &str) -> Result<(), Error> {
        self.identity.validate(&format!("{path}.identity"))?;
        self.axial_frame.validate(&format!("{path}.axial_frame"))?;
        AxisFrame {
            origin_m: [0.0; 3],
            direction: self.transverse_direction,
        }
        .validate(&format!("{path}.transverse_direction"))?;
        let projection: f64 = self
            .axial_frame
            .direction
            .iter()
            .zip(self.transverse_direction)
            .map(|(a, b)| a * b)
            .sum();
        if projection.abs() > 1.0e-12 {
            return Err(Error::invalid(path, "channel axes must be perpendicular"));
        }
        positive(&format!("{path}.length_m"), self.length_m)?;
        positive(&format!("{path}.gap_m"), self.gap_m)?;
        positive(&format!("{path}.width_m"), self.width_m)?;
        nonempty(&format!("{path}.inlet_region"), &self.inlet_region)?;
        nonempty(&format!("{path}.outlet_region"), &self.outlet_region)?;
        if self.inlet_region == self.outlet_region {
            return Err(Error::invalid(path, "inlet and outlet regions must differ"));
        }
        Ok(())
    }

    pub(crate) fn position(&self, axial_m: f64, transverse_m: f64) -> [f64; 3] {
        let origin = self.axial_frame.position(axial_m);
        std::array::from_fn(|i| origin[i] + transverse_m * self.transverse_direction[i])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewtonianFluid {
    pub id: String,
    pub density_kg_m3: f64,
    pub dynamic_viscosity_pa_s: f64,
    pub source: MaterialSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoSlipWall {
    pub region: String,
    pub condition: WallCondition,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallCondition {
    NoSlip,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlowDrive {
    Pressure {
        inlet: PressureAtPort,
        outlet: PressureAtPort,
    },
    VolumeFlow {
        inlet_region: String,
        rate_m3_s: f64,
        outlet: PressureAtPort,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PressureAtPort {
    pub region: String,
    pub pressure: Pressure,
}

/// Absolute pressure or a gauge reading with an explicitly specified absolute reference.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(tag = "reference", rename_all = "snake_case", deny_unknown_fields)]
pub enum Pressure {
    Absolute {
        value_pa: f64,
    },
    Gauge {
        value_pa: f64,
        reference_absolute_pa: f64,
    },
}

impl Pressure {
    fn parts(self) -> (f64, f64) {
        match self {
            Self::Absolute { value_pa } => (0.0, value_pa),
            Self::Gauge {
                value_pa,
                reference_absolute_pa,
            } => (reference_absolute_pa, value_pa),
        }
    }

    fn absolute_pa(self) -> f64 {
        let (reference, value) = self.parts();
        reference + value
    }

    pub(crate) fn difference_pa(self, other: Self) -> f64 {
        let (reference, value) = self.parts();
        let (other_reference, other_value) = other.parts();
        if reference == other_reference {
            value - other_value
        } else {
            self.absolute_pa() - other.absolute_pa()
        }
    }

    pub(crate) fn validate(self, path: &str) -> Result<(), Error> {
        if let Self::Gauge {
            reference_absolute_pa,
            ..
        } = self
        {
            positive(
                &format!("{path}.reference_absolute_pa"),
                reference_absolute_pa,
            )?;
        }
        positive(&format!("{path}.absolute_pressure_pa"), self.absolute_pa())
    }
}

impl PressureAtPort {
    fn validate(&self, path: &str, region: &str) -> Result<(), Error> {
        if self.region != region {
            return Err(Error::invalid(
                format!("{path}.region"),
                format!("expected {region:?}"),
            ));
        }
        self.pressure.validate(&format!("{path}.pressure"))
    }
}

pub(crate) struct FlowState {
    velocity_m_s: Vec<f64>,
    centers_m: Vec<f64>,
    volume_flow_m3_s: f64,
    pub(crate) mass_flow_kg_s: f64,
    pressure_loss_pa: f64,
    inlet_absolute_pa: f64,
    outlet_absolute_pa: f64,
    wall_drag_n: [f64; 2],
}

impl LaminarChannel {
    pub(crate) const OUTPUTS: &[Output] = &[
        Output::AxialVelocity,
        Output::VolumeFlowRate,
        Output::MassFlowRate,
        Output::PressureLoss,
        Output::AbsolutePressure,
        Output::ChannelReynolds,
        Output::WallDrag,
    ];
    const METHOD: &str = "steady_fully_developed_laminar_channel_cell_centered_fvm";

    pub(crate) fn validate(&self, path: &str) -> Result<MeshMetrics, Error> {
        self.geometry.validate(&format!("{path}.geometry"))?;
        let metrics = self
            .mesh
            .validate(&format!("{path}.mesh"), self.geometry.gap_m)?;
        nonempty(&format!("{path}.material.id"), &self.material.id)?;
        positive(
            &format!("{path}.material.density_kg_m3"),
            self.material.density_kg_m3,
        )?;
        positive(
            &format!("{path}.material.dynamic_viscosity_pa_s"),
            self.material.dynamic_viscosity_pa_s,
        )?;
        self.material
            .source
            .validate(&format!("{path}.material.source"))?;
        let [first, second] = &self.walls;
        if first.region == second.region
            || self.mesh.endpoint(&first.region).is_none()
            || self.mesh.endpoint(&second.region).is_none()
        {
            return Err(Error::invalid(
                format!("{path}.walls"),
                "assign a no-slip wall to each transverse mesh endpoint",
            ));
        }
        let inlet_region = &self.geometry.inlet_region;
        let outlet_region = &self.geometry.outlet_region;
        if [&self.mesh.start_region, &self.mesh.end_region]
            .iter()
            .any(|&region| region == inlet_region || region == outlet_region)
        {
            return Err(Error::invalid(
                path,
                "wall and port region names must differ",
            ));
        }
        match &self.drive {
            FlowDrive::Pressure { inlet, outlet } => {
                inlet.validate(&format!("{path}.drive.inlet"), inlet_region)?;
                outlet.validate(&format!("{path}.drive.outlet"), outlet_region)?;
                positive(
                    &format!("{path}.drive.pressure_drop_pa"),
                    inlet.pressure.difference_pa(outlet.pressure),
                )?;
            }
            FlowDrive::VolumeFlow {
                inlet_region: region,
                rate_m3_s,
                outlet,
            } => {
                if region != inlet_region {
                    return Err(Error::invalid(
                        format!("{path}.drive.inlet_region"),
                        format!("expected {inlet_region:?}"),
                    ));
                }
                positive(&format!("{path}.drive.rate_m3_s"), *rate_m3_s)?;
                outlet.validate(&format!("{path}.drive.outlet"), outlet_region)?;
            }
        }
        Ok(metrics)
    }

    pub(crate) fn solve(&self, settings: LinearSettings) -> Result<Solution, Error> {
        let (convergence, state) = self.compute(settings)?;
        let mut result = Solution::new(Self::METHOD, vec![convergence]);
        if let Some(state) = state {
            self.add_outputs(state, &mut result)?;
        }
        Ok(result)
    }

    pub(crate) fn compute(
        &self,
        settings: LinearSettings,
    ) -> Result<(Convergence, Option<FlowState>), Error> {
        let vertices = &self.mesh.vertices_m;
        let viscosity = self.material.dynamic_viscosity_pa_s;
        let length = self.geometry.length_m;
        let width = self.geometry.width_m;
        let centers: Vec<_> = vertices
            .windows(2)
            .map(|edge| edge[0] + 0.5 * (edge[1] - edge[0]))
            .collect();
        let n = centers.len();
        let (gradient, stage) = match &self.drive {
            FlowDrive::Pressure { inlet, outlet } => (
                inlet.pressure.difference_pa(outlet.pressure) / length,
                "axial_momentum",
            ),
            FlowDrive::VolumeFlow { .. } => (1.0, "unit_gradient_axial_momentum"),
        };

        let mut system = SparseAssembly::new(n);
        let lower_wall_conductance = viscosity / centers[0];
        let upper_wall_conductance = viscosity / (self.geometry.gap_m - centers[n - 1]);
        system.add(0, 0, lower_wall_conductance);
        system.add(n - 1, n - 1, upper_wall_conductance);
        for (i, pair) in centers.windows(2).enumerate() {
            let conductance = viscosity / (pair[1] - pair[0]);
            system.add(i, i, conductance);
            system.add(i, i + 1, -conductance);
            system.add(i + 1, i, -conductance);
            system.add(i + 1, i + 1, conductance);
        }
        let rhs: Vec<_> = vertices
            .windows(2)
            .map(|edge| gradient * (edge[1] - edge[0]))
            .collect();
        let (mut velocity, convergence) = system.solve(&rhs, settings, stage, "Pa");
        if convergence.stop_reason != StopReason::Converged {
            return Ok((convergence, None));
        }
        let integrated_flow = |velocity: &[f64]| -> f64 {
            width
                * vertices
                    .windows(2)
                    .zip(velocity)
                    .map(|(edge, u)| (edge[1] - edge[0]) * u)
                    .sum::<f64>()
        };

        let (pressure_loss, inlet_absolute, outlet_absolute) = match &self.drive {
            FlowDrive::Pressure { inlet, outlet } => (
                inlet.pressure.difference_pa(outlet.pressure),
                inlet.pressure.absolute_pa(),
                outlet.pressure.absolute_pa(),
            ),
            FlowDrive::VolumeFlow {
                rate_m3_s, outlet, ..
            } => {
                let response = integrated_flow(&velocity);
                if !response.is_finite() || response <= 0.0 {
                    return Err(Error::invalid(
                        "physics.drive",
                        "unit pressure gradient did not produce a positive flow response; tighten the linear solver tolerance or check the physical scales",
                    ));
                }
                let scale = rate_m3_s / response;
                velocity.iter_mut().for_each(|u| *u *= scale);
                let pressure_loss = scale * length;
                let outlet_absolute = outlet.pressure.absolute_pa();
                (
                    pressure_loss,
                    outlet_absolute + pressure_loss,
                    outlet_absolute,
                )
            }
        };
        let volume_flow = integrated_flow(&velocity);
        if !volume_flow.is_finite()
            || volume_flow <= 0.0
            || !pressure_loss.is_finite()
            || pressure_loss <= 0.0
            || !inlet_absolute.is_finite()
        {
            return Err(Error::invalid(
                "physics.drive",
                "computed flow and pressure loss must be positive and finite; check tolerances and physical scales",
            ));
        }
        let mass_flow = self.material.density_kg_m3 * volume_flow;
        if !mass_flow.is_finite() || mass_flow <= 0.0 {
            return Err(Error::NonFiniteResult {
                quantity: "mass_flow_rate".to_owned(),
            });
        }
        let wall_area = width * length;
        let state = FlowState {
            volume_flow_m3_s: volume_flow,
            mass_flow_kg_s: mass_flow,
            pressure_loss_pa: pressure_loss,
            inlet_absolute_pa: inlet_absolute,
            outlet_absolute_pa: outlet_absolute,
            wall_drag_n: [
                -lower_wall_conductance * velocity[0] * wall_area,
                -upper_wall_conductance * velocity[n - 1] * wall_area,
            ],
            velocity_m_s: velocity,
            centers_m: centers,
        };
        Ok((convergence, Some(state)))
    }

    pub(crate) fn add_outputs(&self, state: FlowState, result: &mut Solution) -> Result<(), Error> {
        let width = self.geometry.width_m;
        let gap = self.geometry.gap_m;
        let inv_kinematic_viscosity =
            self.material.density_kg_m3 / self.material.dynamic_viscosity_pa_s;
        // Hydraulic diameter = 2 * gap, mean velocity = volume flow / (width * gap).
        let reynolds = 2.0 * (state.volume_flow_m3_s / width) * inv_kinematic_viscosity;
        result.balances.extend([
            Balance::new(
                "channel_mass",
                "kg/s",
                [
                    ("inflow", state.mass_flow_kg_s),
                    ("outflow", -state.mass_flow_kg_s),
                ],
            )?,
            Balance::new(
                "channel_axial_momentum",
                "N",
                [
                    ("pressure_force", state.pressure_loss_pa * (width * gap)),
                    ("lower_wall_drag", state.wall_drag_n[0]),
                    ("upper_wall_drag", state.wall_drag_n[1]),
                ],
            )?,
        ]);
        result.quantities.extend([
            (
                Output::AxialVelocity,
                Quantity::field(
                    "m/s",
                    SampleLocation::Cell,
                    state
                        .centers_m
                        .iter()
                        .map(|&y| self.geometry.position(0.5 * self.geometry.length_m, y))
                        .collect(),
                    state.velocity_m_s,
                ),
            ),
            (
                Output::VolumeFlowRate,
                Quantity::scalar("m^3/s", state.volume_flow_m3_s),
            ),
            (
                Output::MassFlowRate,
                Quantity::scalar("kg/s", state.mass_flow_kg_s),
            ),
            (
                Output::PressureLoss,
                Quantity::scalar("Pa", state.pressure_loss_pa),
            ),
            (Output::ChannelReynolds, Quantity::scalar("1", reynolds)),
            (
                Output::AbsolutePressure,
                Quantity::by_region(
                    "Pa",
                    [
                        (self.geometry.inlet_region.clone(), state.inlet_absolute_pa),
                        (
                            self.geometry.outlet_region.clone(),
                            state.outlet_absolute_pa,
                        ),
                    ],
                ),
            ),
            (
                Output::WallDrag,
                Quantity::by_region(
                    "N",
                    [
                        (self.mesh.start_region.clone(), state.wall_drag_n[0]),
                        (self.mesh.end_region.clone(), state.wall_drag_n[1]),
                    ],
                ),
            ),
        ]);
        Ok(())
    }
}
