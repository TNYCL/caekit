//! Steady conjugate heat transfer between a conductive wall and a laminar stream.

use serde::{Deserialize, Serialize};

use crate::{Error, LineMesh, MeshMetrics};
use caekit_numerics::{Convergence, LinearSettings};
use caekit_types::error::positive;
use caekit_types::{Output, Quantity, SampleLocation, Solution};

use super::laminar_channel::LaminarChannel;
use caekit_thermal::{ChannelEnergy, solve_channel_energy};
pub use caekit_thermal::{
    ConductiveMaterial, ConductiveWall, Coolant, CouplingSettings, TemperatureBoundary,
};

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
pub struct ThermalConnection {
    pub solid_region: String,
    pub fluid_region: String,
    pub heat_transfer_coefficient_w_m2_k: f64,
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
        let [wall, thickness] = self.wall.check_meshes(length)?;
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
            .validate_properties(&self.flow.geometry.inlet_region)?;
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
        let (convergence, energy) = solve_channel_energy(
            &self.wall,
            &self.coolant,
            self.flow.geometry.width_m,
            flow.mass_flow_kg_s(),
            self.interface.heat_transfer_coefficient_w_m2_k,
            self.coupling_solver,
        )?;
        result.convergence.push(convergence);
        if let Some(energy) = energy {
            self.add_thermal_outputs(energy, outputs, &mut result)?;
            self.flow.add_outputs(flow, &mut result)?;
        }
        Ok(result)
    }

    fn add_thermal_outputs(
        &self,
        energy: ChannelEnergy,
        outputs: &[Output],
        result: &mut Solution<Convergence>,
    ) -> Result<(), Error> {
        let outlet_delta =
            energy.fluid_temperature_offset_k[energy.fluid_temperature_offset_k.len() - 1];
        result.balances.extend(energy.balances);
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
            (
                Output::HeatTransferRate,
                Quantity::scalar("W", energy.heat_to_coolant_w),
            ),
            (
                Output::FluidBulkTemperature,
                Quantity::field(
                    "K",
                    SampleLocation::Cell,
                    centers(&self.coolant.axial_mesh)
                        .map(|x| geometry.position(x, gap / 2.0))
                        .collect(),
                    energy
                        .fluid_temperature_offset_k
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
                    energy.wall_heat_flux_w_m2,
                ),
            ),
        ]);

        if outputs.contains(&Output::WallTemperature) {
            let thickness = self.wall.thickness_m;
            let hot_delta = self.wall.hot_boundary.temperature_k - inlet;
            let through_wall = &self.wall.thickness_mesh.vertices_m;
            let samples = through_wall
                .len()
                .checked_mul(energy.wall_temperature_offset_k.len())
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
            for (x, wall_theta) in
                centers(&self.wall.axial_mesh).zip(energy.wall_temperature_offset_k)
            {
                for &s in through_wall {
                    let across_wall = s / thickness;
                    let theta = (1.0 - across_wall) * hot_delta + across_wall * wall_theta;
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
