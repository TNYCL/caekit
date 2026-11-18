use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Error;

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
    FluidBulkTemperature,
    WallTemperature,
    OutletTemperature,
    HeatTransferRate,
    InterfaceHeatFlux,
    DisplacementVector,
    CauchyStress,
    VonMisesStress,
    NodalReaction,
    ElasticEnergy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleLocation {
    Node,
    Cell,
    InterfaceFace,
    Sample,
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
    VectorField {
        unit: String,
        location: SampleLocation,
        positions_m: Vec<[f64; 3]>,
        values: Vec<[f64; 3]>,
    },
    TensorField {
        unit: String,
        location: SampleLocation,
        positions_m: Vec<[f64; 3]>,
        values: Vec<[[f64; 3]; 3]>,
    },
    ByRegion {
        unit: String,
        values: BTreeMap<String, f64>,
    },
}

impl Quantity {
    pub fn scalar(unit: &str, value: f64) -> Self {
        Self::Scalar {
            unit: unit.to_owned(),
            value,
        }
    }

    pub fn field(
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

    pub fn by_region(unit: &str, values: impl IntoIterator<Item = (String, f64)>) -> Self {
        Self::ByRegion {
            unit: unit.to_owned(),
            values: values.into_iter().collect(),
        }
    }

    pub fn all_finite(&self) -> bool {
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
            Self::VectorField {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().flatten().all(|x| x.is_finite())
            }
            Self::TensorField {
                positions_m,
                values,
                ..
            } => {
                positions_m.iter().flatten().all(|x| x.is_finite())
                    && values.iter().flatten().flatten().all(|x| x.is_finite())
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
    pub fn new(
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

pub struct Solution<Trace> {
    pub method: &'static str,
    pub convergence: Vec<Trace>,
    pub balances: Vec<Balance>,
    pub quantities: BTreeMap<Output, Quantity>,
}

impl<Trace> Solution<Trace> {
    pub fn new(method: &'static str, convergence: Vec<Trace>) -> Self {
        Self {
            method,
            convergence,
            balances: Vec::new(),
            quantities: BTreeMap::new(),
        }
    }
}
