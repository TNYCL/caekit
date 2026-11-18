use serde::{Deserialize, Serialize};

use crate::{Error, error::nonempty};

/// Identifies the origin and terms of the property values used in a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialSource {
    pub citation: String,
    pub usage_terms: String,
}

impl MaterialSource {
    pub fn validate(&self, path: &str) -> Result<(), Error> {
        nonempty(&format!("{path}.citation"), &self.citation)?;
        nonempty(&format!("{path}.usage_terms"), &self.usage_terms)
    }
}
