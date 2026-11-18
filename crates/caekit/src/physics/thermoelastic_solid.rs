//! One-way coupling from solid heat conduction to linear elasticity on a shared mesh.

use serde::{Deserialize, Serialize};

use caekit_mechanics::elastic_solid::{PreparedElasticSolid, SolidMechanics, ThermalExpansion};
use caekit_mesh::tetra::CheckedTetraMesh;
use caekit_mesh::{GeometryRevision, MeshMetrics, TetraMesh};
use caekit_numerics::{Convergence, LinearSettings, StopReason};
use caekit_thermal::heat_conduction::{PreparedConduction, SolidThermal};
use caekit_types::{Error, Output, Solution};

/// Steady heat conduction followed by linear elasticity with thermal strain on one tetrahedral mesh
/// (the `thermoelastic_solid` physics case).
///
/// The coupling is one-way: the computed vertex temperatures load the mechanical stage through the
/// isotropic thermal strain, and the deformation does not change the temperatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThermoelasticSolid {
    /// Identifier and revision of the geometry shared by both stages; both strings must be
    /// nonempty.
    pub geometry: GeometryRevision,
    /// Tetrahedral mesh shared by both stages, in global Cartesian coordinates in m. Thermal and
    /// mechanical conditions refer to the names of its exterior face regions.
    pub mesh: TetraMesh,
    /// Thermal material, exterior thermal conditions and optional volumetric heat source of the
    /// conduction stage.
    pub thermal: SolidThermal,
    /// Elastic material, displacement constraints, surface loads and optional body force of the
    /// mechanical stage.
    pub mechanical: SolidMechanics,
    /// Stress-free reference temperature and linear expansion coefficient that turn temperature
    /// into thermal strain.
    pub expansion: ThermalExpansion,
    /// Independent residual settings for heat conduction, in watts.
    #[serde(default)]
    pub thermal_solver: LinearSettings,
}

impl ThermoelasticSolid {
    const METHOD: &str = "one_way_steady_linear_thermoelastic_p1_tetrahedral_fem";

    /// Checks the geometry revision, the shared mesh, the thermal solver settings, the expansion
    /// data and the inputs of both stages, and returns the mesh metrics.
    ///
    /// A violation is an [`Error::InvalidModel`]; paths of model fields start with `physics.`.
    pub fn validate(&self) -> Result<MeshMetrics, Error> {
        let mesh = self.checked_mesh()?;
        self.prepare(&mesh)?;
        Ok(mesh.into_metrics())
    }

    /// Validates the case, solves heat conduction and then mechanical equilibrium, and returns the
    /// mesh metrics with the combined solution.
    ///
    /// The conduction stage uses `thermal_solver` with residuals in W and the mechanical stage uses
    /// `mechanical_settings` with residuals in N. `outputs` selects the optional fields of both
    /// stages. The mechanical stage runs only after the conduction stage has converged. With
    /// nonzero thermal strain it also stops with `numerical_breakdown` when, on an externally
    /// loaded axis, the recovered support reactions do not balance the applied forces within a
    /// tolerance derived from `mechanical_settings`. The solution lists the convergence traces of
    /// the stages that ran, in order. Its balances and quantities are those of both stages when
    /// every stage converged and are empty otherwise.
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
