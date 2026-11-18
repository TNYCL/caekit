//! Versioned multiphysics models, checked reference solvers, and reproducible results.

mod error;
mod interface;
mod linear;
mod mesh;
mod model;
pub mod physics;
mod result;
mod tetra;

pub use error::Error;
pub use linear::{Convergence, LinearSettings, StopReason};
pub use mesh::{AxisFrame, GeometryRevision, LineMesh, MeshMetrics, MeshQuality};
pub use model::{MODEL_SCHEMA_VERSION, MaterialSource, Model, ModelCheck, Physics, Units};
pub use result::{
    Balance, Execution, Output, Quantity, RESULT_SCHEMA_VERSION, RunReport, SampleLocation,
};

pub use tetra::{FaceRegion, TetraMesh};

use sha2::{Digest, Sha256};

/// Validate and solve a model on the reference double-precision CPU path.
pub fn solve(model: &Model) -> Result<RunReport, Error> {
    let check = model.validate()?;
    let hash = Sha256::digest(serde_json::to_vec(model)?);
    let mut solution = match &model.physics {
        Physics::AxialBar(case) => case.solve(model.linear_solver)?,
        Physics::ElasticSolid(case) => case.solve(model.linear_solver, &model.outputs)?,
        Physics::LaminarChannel(case) => case.solve(model.linear_solver)?,
        Physics::CoolingChannel(case) => case.solve(model.linear_solver, &model.outputs)?,
    };

    for (key, quantity) in &solution.quantities {
        if !quantity.all_finite() {
            return Err(Error::NonFiniteResult {
                quantity: format!("{key:?}"),
            });
        }
    }
    solution
        .quantities
        .retain(|key, _| model.outputs.contains(key));
    let stop_reason = solution
        .convergence
        .iter()
        .find(|stage| stage.stop_reason != StopReason::Converged)
        .map_or(StopReason::Converged, |stage| stage.stop_reason);
    Ok(RunReport {
        schema_version: RESULT_SCHEMA_VERSION,
        model_schema_version: model.schema_version,
        model_name: model.name.clone(),
        model_sha256: format!("{hash:x}"),
        geometry: model.physics.geometry().into_iter().cloned().collect(),
        meshes: check.meshes,
        execution: Execution::reference_cpu(),
        method: solution.method.to_owned(),
        stop_reason,
        convergence: solution.convergence,
        balances: solution.balances,
        quantities: solution.quantities,
    })
}
