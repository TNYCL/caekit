use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Balance, GeometryRevision, MeshMetrics, Output, Quantity};
use caekit_numerics::{Convergence, StopReason};

pub const RESULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct Execution {
    pub backend: String,
    pub precision: String,
    pub architecture: String,
    pub operating_system: String,
    pub threads: usize,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub schema_version: u32,
    pub model_schema_version: u32,
    pub model_name: String,
    pub model_sha256: String,
    pub geometry: Vec<GeometryRevision>,
    pub meshes: Vec<MeshMetrics>,
    pub execution: Execution,
    pub method: String,
    pub stop_reason: StopReason,
    pub convergence: Vec<Convergence>,
    pub balances: Vec<Balance>,
    pub quantities: BTreeMap<Output, Quantity>,
}
