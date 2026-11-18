use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::Error;
use crate::Output;
use crate::physics::{
    axial_bar::AxialBar,
    cooling_channel::CoolingChannel,
    elastic_solid::ElasticSolid,
    heat_conduction::{HeatConduction, SolidThermal},
    laminar_channel::LaminarChannel,
    thermoelastic_solid::ThermoelasticSolid,
};
use caekit_mesh::{GeometryRevision, MeshMetrics};
use caekit_numerics::LinearSettings;
use caekit_types::error::nonempty;

pub const MODEL_SCHEMA_VERSION: u32 = 1;

/// Input to every execution surface. Hardware and precision selection are execution concerns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    pub schema_version: u32,
    pub name: String,
    pub units: Units,
    pub physics: Physics,
    #[serde(default)]
    pub linear_solver: LinearSettings,
    pub outputs: Vec<Output>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    Si,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Physics {
    AxialBar(Box<AxialBar>),
    LaminarChannel(Box<LaminarChannel>),
    CoolingChannel(Box<CoolingChannel>),
    ElasticSolid(Box<ElasticSolid>),
    HeatConduction(Box<HeatConduction>),
    ThermoelasticSolid(Box<ThermoelasticSolid>),
}

impl Physics {
    pub(crate) fn geometry(&self) -> Vec<&GeometryRevision> {
        match self {
            Self::ElasticSolid(case) => vec![&case.geometry],
            Self::AxialBar(case) => vec![&case.geometry.identity],
            Self::LaminarChannel(case) => vec![&case.geometry.identity],
            Self::CoolingChannel(case) => vec![&case.flow.geometry.identity, &case.wall.identity],
            Self::HeatConduction(case) => vec![&case.geometry],
            Self::ThermoelasticSolid(case) => vec![&case.geometry],
        }
    }

    pub(crate) fn supports(&self, output: Output) -> bool {
        match self {
            Self::ElasticSolid(_) => ElasticSolid::OUTPUTS.contains(&output),
            Self::AxialBar(_) => AxialBar::OUTPUTS.contains(&output),
            Self::LaminarChannel(_) => LaminarChannel::OUTPUTS.contains(&output),
            Self::HeatConduction(_) => SolidThermal::OUTPUTS.contains(&output),
            Self::ThermoelasticSolid(_) => {
                SolidThermal::OUTPUTS.contains(&output) || ElasticSolid::OUTPUTS.contains(&output)
            }
            Self::CoolingChannel(_) => {
                LaminarChannel::OUTPUTS.contains(&output)
                    || CoolingChannel::THERMAL_OUTPUTS.contains(&output)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelCheck {
    pub model_schema_version: u32,
    pub meshes: Vec<MeshMetrics>,
}

impl Model {
    /// Reads a version 1 document and checks all geometry, mesh, conditions, and output requests.
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Header {
            schema_version: u32,
        }

        let header: Header = serde_json::from_slice(bytes)?;
        check_version(header.schema_version)?;
        let model: Self = serde_json::from_slice(bytes)?;
        model.validate()?;
        Ok(model)
    }

    /// Checks a deserialized or programmatically built model.
    pub fn validate(&self) -> Result<ModelCheck, Error> {
        self.validate_request()?;
        let meshes = match &self.physics {
            Physics::AxialBar(case) => vec![case.validate()?],
            Physics::ElasticSolid(case) => vec![case.validate()?],
            Physics::LaminarChannel(case) => vec![case.validate("physics")?],
            Physics::CoolingChannel(case) => case.validate()?,
            Physics::HeatConduction(case) => vec![case.validate()?],
            Physics::ThermoelasticSolid(case) => vec![case.validate()?],
        };
        Ok(ModelCheck {
            model_schema_version: self.schema_version,
            meshes,
        })
    }

    pub(crate) fn validate_request(&self) -> Result<(), Error> {
        check_version(self.schema_version)?;
        nonempty("name", &self.name)?;
        self.linear_solver
            .validate()
            .map_err(|e| Error::invalid(format!("linear_solver.{}", e.field), e.message))?;

        if self.outputs.is_empty() {
            return Err(Error::invalid("outputs", "request at least one quantity"));
        }
        let mut seen = BTreeSet::new();
        for (i, output) in self.outputs.iter().enumerate() {
            let path = format!("outputs[{i}]");
            if !self.physics.supports(*output) {
                return Err(Error::invalid(
                    path,
                    "quantity is unavailable for this physics",
                ));
            }
            if !seen.insert(output) {
                return Err(Error::invalid(path, "duplicate quantity"));
            }
        }

        Ok(())
    }
}

fn check_version(version: u32) -> Result<(), Error> {
    if version == MODEL_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(Error::UnsupportedModelVersion(version))
    }
}
