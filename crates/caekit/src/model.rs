use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::Error;
use crate::Output;
use crate::physics::{
    axial_bar::AxialBar,
    cooling_channel::CoolingChannel,
    elastic_solid::ElasticSolid,
    heat_conduction::{HeatConduction, MultimaterialHeatConduction, SolidThermal},
    laminar_channel::LaminarChannel,
    thermoelastic_solid::ThermoelasticSolid,
};
use caekit_mesh::{GeometryRevision, MeshMetrics};
use caekit_numerics::LinearSettings;
use caekit_types::error::nonempty;

/// Schema version of the model documents this crate reads, and the only value accepted for
/// [`Model::schema_version`].
pub const MODEL_SCHEMA_VERSION: u32 = 1;

/// Input to every execution surface. Hardware and precision selection are execution concerns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// Schema version of the document; must equal [`MODEL_SCHEMA_VERSION`], otherwise
    /// [`Error::UnsupportedModelVersion`] is returned.
    pub schema_version: u32,
    /// Human-readable run name; must contain a non-whitespace character. It is copied to the run
    /// report as `model_name`.
    pub name: String,
    /// Unit system of every numeric value in the model; `si` is the only choice.
    pub units: Units,
    /// Physical case to solve, with its geometry revision, mesh, materials and conditions. In JSON
    /// its `kind` key selects the case.
    pub physics: Physics,
    /// Stopping settings of the iterative linear solve of the case.
    ///
    /// Optional in JSON: when omitted the defaults of [`LinearSettings`] apply, and when present
    /// all of its fields are required. The relative tolerance must lie strictly between 0 and 1,
    /// the absolute tolerance must be nonnegative and finite and the iteration limit positive. The
    /// absolute tolerance is in the residual unit of the solved equation: N for the axial bar, the
    /// elastic solid and the mechanical stage of the thermoelastic solid, W for the two heat
    /// conduction cases and Pa for the momentum stage of the laminar and cooling channels.
    #[serde(default)]
    pub linear_solver: LinearSettings,
    /// Quantities to include in the run report, spelled in snake_case in JSON. The list must be
    /// nonempty and free of duplicates, and every entry must be available for the selected physics.
    pub outputs: Vec<Output>,
}

/// Unit system of a model's numeric values.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Units {
    /// SI units, spelled `si` in JSON. Physical input fields name their unit in a suffix such as
    /// `_m`, `_pa` or `_k`.
    Si,
}

/// Physical case of a model.
///
/// In JSON the variant is selected by the `kind` key in snake_case, placed next to the fields of
/// the case, for example `"kind": "axial_bar"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Physics {
    /// `axial_bar`: static axial deformation of a straight elastic bar with uniform cross-section.
    AxialBar(Box<AxialBar>),
    /// `laminar_channel`: fully developed incompressible laminar flow between two stationary
    /// parallel plates.
    LaminarChannel(Box<LaminarChannel>),
    /// `cooling_channel`: a laminar channel whose stream exchanges heat with a conductive wall.
    CoolingChannel(Box<CoolingChannel>),
    /// `elastic_solid`: static linear elasticity of an isotropic solid on a tetrahedral mesh.
    ElasticSolid(Box<ElasticSolid>),
    /// `heat_conduction`: steady isotropic heat conduction in a single-material solid on a
    /// tetrahedral mesh.
    HeatConduction(Box<HeatConduction>),
    /// `multimaterial_heat_conduction`: steady heat conduction with a material assigned to each
    /// named volume region of a tetrahedral mesh.
    MultimaterialHeatConduction(Box<MultimaterialHeatConduction>),
    /// `thermoelastic_solid`: steady heat conduction followed by linear elasticity with thermal
    /// strain on the same tetrahedral mesh.
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
            Self::MultimaterialHeatConduction(case) => vec![&case.geometry],
            Self::ThermoelasticSolid(case) => vec![&case.geometry],
        }
    }

    pub(crate) fn supports(&self, output: Output) -> bool {
        match self {
            Self::ElasticSolid(_) => ElasticSolid::OUTPUTS.contains(&output),
            Self::AxialBar(_) => AxialBar::OUTPUTS.contains(&output),
            Self::LaminarChannel(_) => LaminarChannel::OUTPUTS.contains(&output),
            Self::HeatConduction(_) => SolidThermal::OUTPUTS.contains(&output),
            Self::MultimaterialHeatConduction(_) => {
                MultimaterialHeatConduction::OUTPUTS.contains(&output)
            }
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

/// Outcome of a successful [`Model::validate`], written as JSON by the CLI `check` command.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelCheck {
    /// Schema version of the validated model.
    pub model_schema_version: u32,
    /// Metrics of each mesh of the physics case, named by its path in the model. Most cases have
    /// one mesh; the cooling channel lists its flow, wall axial, wall thickness and coolant axial
    /// meshes, in that order.
    pub meshes: Vec<MeshMetrics>,
}

impl Model {
    /// Reads a version 1 document and checks all geometry, mesh, conditions, and output requests.
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        #[derive(Deserialize)]
        struct Header {
            schema_version: u32,
        }

        let header: Header =
            serde_json::from_slice(bytes).map_err(|error| model_json_error(bytes, error))?;
        check_version(header.schema_version)?;
        let model: Self =
            serde_json::from_slice(bytes).map_err(|error| model_json_error(bytes, error))?;
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
            Physics::MultimaterialHeatConduction(case) => vec![case.validate()?],
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

fn model_json_error(bytes: &[u8], error: serde_json::Error) -> Error {
    // Some numbers fail while reading the JSON token, before material validation.
    // Recover the material field path on that error path only.
    if error.is_syntax() {
        let mut reader = serde_json::Deserializer::from_slice(bytes);
        if let Err(annotated) = serde_path_to_error::deserialize::<_, Model>(&mut reader) {
            let path = annotated.path().to_string();
            if path.starts_with("physics.thermal.materials[") {
                return Error::invalid(path, error.to_string());
            }
        }
    }
    Error::Json(error)
}

fn check_version(version: u32) -> Result<(), Error> {
    if version == MODEL_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(Error::UnsupportedModelVersion(version))
    }
}
