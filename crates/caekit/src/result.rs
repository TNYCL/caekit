use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Balance, GeometryRevision, MeshMetrics, Output, Quantity};
use caekit_numerics::{Convergence, StopReason};

/// Schema version of the run reports this crate writes, stored in [`RunReport::schema_version`].
pub const RESULT_SCHEMA_VERSION: u32 = 1;

/// Execution environment that produced a run report.
#[derive(Debug, Serialize, Deserialize)]
pub struct Execution {
    /// Execution backend: `wasm_cpu` when built for the `wasm32` architecture, otherwise
    /// `native_cpu`.
    pub backend: String,
    /// Floating-point format of the calculation; the reference path records `ieee754_binary64`.
    pub precision: String,
    /// CPU architecture the solver was built for, as given by `std::env::consts::ARCH`.
    pub architecture: String,
    /// Operating system the solver was built for, as given by `std::env::consts::OS`.
    pub operating_system: String,
    /// Number of solver threads; the reference path records 1.
    pub threads: usize,
    /// Version of the `caekit` crate that produced the report.
    pub solver_version: String,
}

impl Execution {
    pub(crate) fn reference_cpu() -> Self {
        Self {
            backend: if cfg!(target_arch = "wasm32") {
                "wasm_cpu"
            } else {
                "native_cpu"
            }
            .to_owned(),
            precision: "ieee754_binary64".to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            operating_system: std::env::consts::OS.to_owned(),
            threads: 1,
            solver_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// Result document of [`crate::solve`], written as JSON by the CLI `run` command.
#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    /// Schema version of this report; [`crate::solve`] writes [`RESULT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Schema version of the solved model.
    pub model_schema_version: u32,
    /// Run name copied from the model.
    pub model_name: String,
    /// SHA-256 digest, in lowercase hexadecimal, of the compact JSON serialization of the typed
    /// model with defaults applied. Input whitespace and object key order do not affect it; array
    /// order and any explicit change to a value do.
    pub model_sha256: String,
    /// Geometry revisions the model is based on. Most cases have one; the cooling channel lists the
    /// flow geometry and then the wall.
    pub geometry: Vec<GeometryRevision>,
    /// Metrics of each mesh of the physics case, in the same order as in
    /// [`crate::ModelCheck::meshes`].
    pub meshes: Vec<MeshMetrics>,
    /// Backend, precision, platform and solver version of the run.
    pub execution: Execution,
    /// Identifier of the numerical method of the physics case, such as
    /// `static_small_strain_axial_bar_p1_fem`.
    pub method: String,
    /// Overall outcome: `converged` when every stage in `convergence` converged, otherwise the stop
    /// reason of the first stage that did not.
    pub stop_reason: StopReason,
    /// Convergence trace of each numerical stage that ran, in execution order.
    pub convergence: Vec<Convergence>,
    /// Global balances with signed terms and their signed sum, reported independently of the
    /// requested outputs. Empty when the run did not converge.
    pub balances: Vec<Balance>,
    /// Requested quantities keyed by output name, each with its unit and, for fields, its sample
    /// positions. Only entries listed in the model `outputs` appear, and none when the run did not
    /// converge.
    pub quantities: BTreeMap<Output, Quantity>,
}
