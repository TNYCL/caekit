use serde::{Deserialize, Serialize};

use crate::{Error, error::nonempty};

/// Identifies the origin and terms of the property values used in a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialSource {
    /// Reference that identifies where the property values come from. Must contain at least one
    /// non-whitespace character.
    pub citation: String,
    /// Licence or terms under which the cited values may be used. Must contain at least one
    /// non-whitespace character.
    pub usage_terms: String,
}

impl MaterialSource {
    /// Checks that `citation` and `usage_terms` are nonempty.
    ///
    /// `path` is the JSON path of this object, for example `physics.material.source`. A failure is
    /// an [`Error::InvalidModel`] at `{path}.citation` or `{path}.usage_terms`.
    pub fn validate(&self, path: &str) -> Result<(), Error> {
        nonempty(&format!("{path}.citation"), &self.citation)?;
        nonempty(&format!("{path}.usage_terms"), &self.usage_terms)
    }
}
