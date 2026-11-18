//! One-way coupling from solid heat conduction to linear elasticity on a shared mesh.

use serde::{Deserialize, Serialize};

use caekit_mechanics::elastic_solid::{PreparedElasticSolid, SolidMechanics, ThermalExpansion};
use caekit_mesh::tetra::CheckedTetraMesh;
use caekit_mesh::{GeometryRevision, MeshMetrics, TetraMesh};
use caekit_numerics::{Convergence, LinearSettings, StopReason};
use caekit_thermal::heat_conduction::{PreparedConduction, SolidThermal};
use caekit_types::{Error, Output, Solution};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermoelasticSolid {
    pub geometry: GeometryRevision,
    pub mesh: TetraMesh,
    pub thermal: SolidThermal,
    pub mechanical: SolidMechanics,
    pub expansion: ThermalExpansion,
    /// Independent residual settings for heat conduction, in watts.
    #[serde(default)]
    pub thermal_solver: LinearSettings,
}

impl ThermoelasticSolid {
    const METHOD: &str = "one_way_steady_linear_thermoelastic_p1_tetrahedral_fem";

    pub fn validate(&self) -> Result<MeshMetrics, Error> {
        let mesh = self.checked_mesh()?;
        self.prepare(&mesh)?;
        Ok(mesh.into_metrics())
    }

    pub fn solve(
        &self,
        mechanical_settings: LinearSettings,
        outputs: &[Output],
    ) -> Result<(MeshMetrics, Solution<Convergence>), Error> {
        let mesh = self.checked_mesh()?;
        let (thermal, mechanical) = self.prepare(&mesh)?;
        let (mut result, temperatures) = thermal.solve(self.thermal_solver, outputs)?;
        result.method = Self::METHOD;
        if let Some(temperatures) = temperatures {
            let mechanical = mechanical.solve_thermoelastic(
                mechanical_settings,
                outputs,
                self.expansion,
                temperatures.datum_k(),
                temperatures.offsets_k(),
            )?;
            if mechanical
                .convergence
                .iter()
                .all(|stage| stage.stop_reason == StopReason::Converged)
            {
                result.balances.extend(mechanical.balances);
                result.quantities.extend(mechanical.quantities);
            } else {
                result.balances.clear();
                result.quantities.clear();
            }
            result.convergence.extend(mechanical.convergence);
        }
        Ok((mesh.into_metrics(), result))
    }

    fn checked_mesh(&self) -> Result<CheckedTetraMesh<'_>, Error> {
        self.geometry.validate("physics.geometry")?;
        self.mesh.checked("physics.mesh")
    }

    fn prepare<'a>(
        &'a self,
        mesh: &'a CheckedTetraMesh<'a>,
    ) -> Result<(PreparedConduction<'a>, PreparedElasticSolid<'a>), Error> {
        self.thermal_solver.validate().map_err(|error| {
            Error::invalid(
                format!("physics.thermal_solver.{}", error.field),
                error.message,
            )
        })?;
        self.expansion.validate()?;
        Ok((self.thermal.prepare(mesh)?, self.mechanical.prepare(mesh)?))
    }
}
