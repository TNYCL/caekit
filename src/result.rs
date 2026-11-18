use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::linear::{Convergence, StopReason};
use crate::{Error, GeometryRevision, MeshMetrics};

pub const RESULT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    AxialDisplacement,
    AxialStress,
    EndDisplacement,
    ReactionForce,
    AxialVelocity,
    VolumeFlowRate,
    MassFlowRate,
    PressureLoss,
    AbsolutePressure,
    ChannelReynolds,
    WallDrag,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleLocation {
    Node,
    Cell,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Quantity {
    Scalar {
        unit: String,
        value: f64,
    },
    Field {
        unit: String,
        location: SampleLocation,
        positions_m: Vec<[f64; 3]>,
        values: Vec<f64>,
    },
    ByRegion {
        unit: String,
        values: BTreeMap<String, f64>,
    },
}

impl Quantity {
    pub(crate) fn scalar(unit: &str, value: f64) -> Self {
        Self::Scalar {
            unit: unit.to_owned(),
            value,
        }
    }

    pub(crate) fn field(
        unit: &str,
        location: SampleLocation,
        positions_m: Vec<[f64; 3]>,
        values: Vec<f64>,
    ) -> Self {
        Self::Field {
            unit: unit.to_owned(),
            location,
            positions_m,
            values,
        }
    }

    pub(crate) fn by_region(unit: &str, values: impl IntoIterator<Item = (String, f64)>) -> Self {
        Self::ByRegion {
            unit: unit.to_owned(),
            values: values.into_iter().collect(),
        }
    }

    pub(crate) fn all_finite(&self) -> bool {
        match self {
            Self::Scalar { value, .. } => value.is_finite(),
            Self::Field {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().all(|x| x.is_finite())
            }
            Self::ByRegion { values, .. } => values.values().all(|x| x.is_finite()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Balance {
    pub name: String,
    pub unit: String,
    pub terms: BTreeMap<String, f64>,
    /// Signed sum of the terms under the solver's stated sign convention.
    pub imbalance: f64,
}

impl Balance {
    pub(crate) fn new(
        name: &str,
        unit: &str,
        terms: impl IntoIterator<Item = (&'static str, f64)>,
    ) -> Result<Self, Error> {
        let terms: BTreeMap<_, _> = terms
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect();
        let imbalance: f64 = terms.values().sum();
        if !imbalance.is_finite() {
            return Err(Error::NonFiniteResult {
                quantity: name.to_owned(),
            });
        }
        Ok(Self {
            name: name.to_owned(),
            unit: unit.to_owned(),
            terms,
            imbalance,
        })
    }
}

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

pub(crate) struct Solution {
    pub method: &'static str,
    pub convergence: Vec<Convergence>,
    pub balances: Vec<Balance>,
    pub quantities: BTreeMap<Output, Quantity>,
}

impl Solution {
    pub(crate) fn new(method: &'static str, convergence: Vec<Convergence>) -> Self {
        Self {
            method,
            convergence,
            balances: Vec::new(),
            quantities: BTreeMap::new(),
        }
    }
}
